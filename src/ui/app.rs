//! Winit event loop with Wayland layer shell support

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ButtonSource, ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::monitor::VideoMode;
use winit::platform::wayland::{ActiveEventLoopExtWayland, WindowAttributesWayland};
use winit::platform::wayland::{Anchor, KeyboardInteractivity, Layer};
use winit::window::{WindowAttributes, WindowId};

use crate::audio::CaptureControl;

use super::control_panel::ControlPanelState;
use super::renderer::Renderer;
use super::spectrogram::SpectrogramMode;
use super::SharedAudioState;

const MARGIN: i32 = 24;

pub fn run(
    audio_state: SharedAudioState,
    running: Arc<AtomicBool>,
    capture_control: Option<Arc<CaptureControl>>,
    mode: SpectrogramMode,
    transparency: f32,
) {
    let event_loop = EventLoop::new().expect("Failed to create event loop");

    let mut control_panel = ControlPanelState::new();
    control_panel.viz_mode = mode;
    control_panel.transparency = transparency;

    let app = Box::new(OverlayApp {
        renderers: HashMap::new(),
        audio_state,
        running,
        capture_control,
        mouse_position: None,
        mode,
        control_panel,
    });

    event_loop
        .run_app(Box::leak(app))
        .expect("Event loop failed");
}

struct OverlayApp {
    renderers: HashMap<WindowId, Renderer>,
    audio_state: SharedAudioState,
    running: Arc<AtomicBool>,
    capture_control: Option<Arc<CaptureControl>>,
    mouse_position: Option<(f64, f64)>,
    mode: SpectrogramMode,
    control_panel: ControlPanelState,
}

impl OverlayApp {
    fn handle_pause_toggle(&self) {
        if let Some(ref control) = self.capture_control {
            let now_paused = control.toggle_pause();
            self.audio_state.write().is_paused = now_paused;
            eprintln!("Capture {}", if now_paused { "paused" } else { "resumed" });
        }
    }

    fn handle_auto_gain_toggle(&self) {
        if let Some(ref control) = self.capture_control {
            let new_state = !control.is_auto_gain_enabled();
            control.set_auto_gain(new_state);
            self.audio_state.write().auto_gain_enabled = new_state;
            eprintln!(
                "Auto-gain {}",
                if new_state { "enabled" } else { "disabled" }
            );
        }
    }

    fn sync_control_state(&self) {
        if let Some(ref control) = self.capture_control {
            let mut state = self.audio_state.write();
            state.is_paused = control.is_paused();
            state.auto_gain_enabled = control.is_auto_gain_enabled();
            state.current_gain = control.get_current_gain();
        }
    }

    fn handle_control_panel_click(&mut self, x: f64, y: f64, width: u32, height: u32) {
        let gear_icon_size = 40.0;
        let gear_x = width as f64 - gear_icon_size - 10.0;
        let gear_y = 10.0;

        if x >= gear_x
            && x <= gear_x + gear_icon_size
            && y >= gear_y
            && y <= gear_y + gear_icon_size
        {
            self.control_panel.toggle_open();
            return;
        }

        if !self.control_panel.is_open {
            return;
        }

        let panel_width = 400.0;
        let panel_height = 300.0;
        let panel_x = (width as f64 - panel_width) / 2.0;
        let panel_y = (height as f64 - panel_height) / 2.0;

        if x < panel_x || x > panel_x + panel_width || y < panel_y || y > panel_y + panel_height {
            self.control_panel.toggle_open();
            return;
        }

        let control_height = 40.0;
        let start_y = panel_y + 50.0;
        let relative_y = y - start_y;

        if relative_y < 0.0 || relative_y > control_height * Control::ALL.len() as f64 {
            return;
        }

        let control_idx = (relative_y / control_height) as usize;

        use super::control_panel::Control;
        let controls = Control::ALL;

        if control_idx < controls.len() {
            match controls[control_idx] {
                Control::AgcCheckbox => {
                    self.control_panel.toggle_agc();
                    let mut state = self.audio_state.write();
                    self.control_panel.apply_agc(&mut state);
                }
                Control::InjectionToggle => {
                    let mut state = self.audio_state.write();
                    self.control_panel.toggle_injection(&mut state);
                }
                Control::PauseButton => {
                    self.control_panel.toggle_pause();
                    if let Some(ref ctrl) = self.capture_control {
                        self.control_panel.apply_pause(ctrl);
                    }
                }
                Control::VizToggle => {
                    self.control_panel.toggle_viz_mode();
                    self.mode = self.control_panel.viz_mode;
                    for renderer in self.renderers.values_mut() {
                        renderer.set_mode(self.control_panel.viz_mode);
                    }
                }
                Control::ColorPicker => {
                    let next_scheme = match self.control_panel.color_scheme_name {
                        "flame" => "ice",
                        "ice" => "mono",
                        _ => "flame",
                    };
                    self.control_panel.set_color_scheme(next_scheme);
                    for renderer in self.renderers.values_mut() {
                        renderer.set_color_scheme(next_scheme);
                    }
                }
                Control::GainSlider => {
                    let new_gain = if self.control_panel.gain_value >= 2.0 {
                        0.5
                    } else {
                        (self.control_panel.gain_value + 0.5).min(2.0)
                    };
                    self.control_panel.set_gain(new_gain);
                    let mut state = self.audio_state.write();
                    self.control_panel.apply_gain(&mut state);
                }
                Control::ModelSelector => {
                    self.control_panel.toggle_model();
                }
                Control::AutoSaveToggle => {
                    self.control_panel.toggle_auto_save();
                }
                Control::TransparencySlider => {
                    self.control_panel.adjust_transparency();
                    for renderer in self.renderers.values_mut() {
                        renderer.set_transparency(self.control_panel.transparency);
                    }
                }
                _ => {}
            }
        }
    }
}

impl ApplicationHandler for OverlayApp {
    fn resumed(&mut self, event_loop: &dyn ActiveEventLoop) {
        if !self.running.load(Ordering::Relaxed) {
            event_loop.exit();
        }
    }

    fn about_to_wait(&mut self, event_loop: &dyn ActiveEventLoop) {
        if !self.running.load(Ordering::Relaxed) {
            event_loop.exit();
            return;
        }

        self.sync_control_state();

        for renderer in self.renderers.values() {
            renderer.window.request_redraw();
        }
    }

    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        let Some((_, monitor)) = event_loop.available_monitors().enumerate().next() else {
            eprintln!("No monitors available");
            return;
        };

        let Some(mode) = monitor.current_video_mode() else {
            eprintln!("No video mode available");
            return;
        };

        let window_attrs = create_window_attributes(event_loop, &mode, &monitor);
        let window = event_loop
            .create_window(window_attrs)
            .expect("Failed to create window");

        let mut renderer = Renderer::new(window, self.audio_state.clone(), self.mode);
        renderer.set_transparency(self.control_panel.transparency);
        let window_id = renderer.window.id();
        self.renderers.insert(window_id, renderer);
    }

    fn window_event(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        if !self.running.load(Ordering::Relaxed) {
            event_loop.exit();
            return;
        }

        match event {
            WindowEvent::CloseRequested => {
                self.running.store(false, Ordering::Relaxed);
                if let Some(ref control) = self.capture_control {
                    control.stop();
                }
                event_loop.exit();
            }
            WindowEvent::KeyboardInput { ref event, .. } => {
                if event.state == ElementState::Pressed {
                    match event.logical_key {
                        Key::Character(ref c) if c == "p" || c == "P" || c == " " => {
                            self.handle_pause_toggle();
                        }
                        Key::Character(ref c) if c == "g" || c == "G" => {
                            self.handle_auto_gain_toggle();
                        }
                        Key::Character(ref c) if c == "w" || c == "W" => {
                            if let Some(renderer) = self.renderers.get_mut(&window_id) {
                                renderer.toggle_mode();
                            }
                        }
                        Key::Character(ref c) if c == "c" || c == "C" => {
                            self.control_panel.toggle_open();
                        }
                        Key::Character(ref c) if c == "q" || c == "Q" => {
                            self.running.store(false, Ordering::Relaxed);
                            if let Some(ref control) = self.capture_control {
                                control.stop();
                            }
                            event_loop.exit();
                        }
                        Key::Named(NamedKey::Escape) => {
                            // TODO: tray-icon - WGPU mode needs system tray icon for exit
                            // Keybindings are not viable for WGPU overlay mode (designed for
                            // no-keyboard environments). See AGENTS.md "Dual UX Requirement"
                            // for architectural rationale. Tray icon should provide:
                            // - Quit option
                            // - Show/hide overlay toggle
                            // - Settings access (when control panel implemented)
                            self.running.store(false, Ordering::Relaxed);
                            if let Some(ref control) = self.capture_control {
                                control.stop();
                            }
                            event_loop.exit();
                        }
                        _ => {}
                    }
                }
            }
            WindowEvent::PointerMoved { position, .. } => {
                self.mouse_position = Some((position.x, position.y));
            }
            WindowEvent::PointerButton {
                state: ElementState::Pressed,
                button: ButtonSource::Mouse(MouseButton::Left),
                ..
            } => {
                if let Some((x, y)) = self.mouse_position {
                    if let Some(renderer) = self.renderers.get(&window_id) {
                        let size = renderer.window.surface_size();
                        self.handle_control_panel_click(x, y, size.width, size.height);
                    }
                }
            }
            _ => {}
        }

        if let Some(renderer) = self.renderers.get_mut(&window_id) {
            match event {
                WindowEvent::SurfaceResized(size) => {
                    renderer.resize(size.width, size.height);
                }
                WindowEvent::RedrawRequested => {
                    renderer.draw_with_panel(Some(&self.control_panel));
                }
                _ => {}
            }
        }
    }
}

fn create_window_attributes(
    event_loop: &dyn ActiveEventLoop,
    mode: &VideoMode,
    monitor: &winit::monitor::MonitorHandle,
) -> WindowAttributes {
    let monitor_size = mode.size();
    let window_width = (monitor_size.width as f32 * 0.25) as u32;
    let window_height = 210u32;

    let size = PhysicalSize::new(window_width.max(300), window_height.max(80));

    let mut attrs = WindowAttributes::default()
        .with_decorations(false)
        .with_transparent(true)
        .with_surface_size(size)
        .with_title("Barbara")
        .with_resizable(false);

    if event_loop.is_wayland() {
        let wayland_attrs = WindowAttributesWayland::default()
            .with_layer_shell()
            .with_anchor(Anchor::BOTTOM)
            .with_layer(Layer::Overlay)
            .with_margin(MARGIN, MARGIN, MARGIN, MARGIN)
            .with_output(monitor.native_id())
            .with_keyboard_interactivity(KeyboardInteractivity::OnDemand);

        attrs = attrs.with_platform_attributes(Box::new(wayland_attrs));
    }

    attrs
}
