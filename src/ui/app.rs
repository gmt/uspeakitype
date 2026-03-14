//! Winit event loop with Wayland layer shell support

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ButtonSource, ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::monitor::VideoMode;
use winit::platform::wayland::{ActiveEventLoopExtWayland, WindowAttributesWayland};
use winit::platform::wayland::{Anchor, KeyboardInteractivity, Layer};
use winit::window::{WindowAttributes, WindowId};

use crate::audio::CaptureControl;

use super::control_panel::{Control, ControlPanelState, PanelRect};
use super::renderer::Renderer;
use super::spectrogram::SpectrogramMode;
use super::SharedAudioState;

const MARGIN: i32 = 24;
const FRAME_INTERVAL: Duration = Duration::from_millis(16);

pub fn run(
    audio_state: SharedAudioState,
    running: Arc<AtomicBool>,
    capture_control: Option<Arc<CaptureControl>>,
    mode: SpectrogramMode,
    opacity: f32,
    tag: Option<String>,
) {
    let event_loop = EventLoop::new().expect("Failed to create event loop");

    let mut control_panel = ControlPanelState::new();
    control_panel.viz_mode = mode;
    control_panel.opacity = opacity;

    let app = Box::new(OverlayApp {
        renderers: HashMap::new(),
        audio_state,
        running,
        capture_control,
        mouse_position: None,
        mode,
        control_panel,
        tag,
        next_redraw_deadline: Instant::now(),
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
    tag: Option<String>,
    next_redraw_deadline: Instant,
}

impl OverlayApp {
    fn request_repaint_now(&mut self) {
        self.next_redraw_deadline = Instant::now();
        for renderer in self.renderers.values() {
            renderer.window.request_redraw();
        }
    }

    fn handle_pause_toggle(&self) {
        if let Some(ref control) = self.capture_control {
            let now_paused = control.toggle_pause();
            self.audio_state.write().is_paused = now_paused;
            log::debug!("Capture {}", if now_paused { "paused" } else { "resumed" });
        }
    }

    fn handle_auto_gain_toggle(&self) {
        if let Some(ref control) = self.capture_control {
            let new_state = !control.is_auto_gain_enabled();
            control.set_auto_gain(new_state);
            self.audio_state.write().auto_gain_enabled = new_state;
            log::debug!(
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
        let x = x as f32;
        let y = y as f32;

        // Gear icon hit-test (top-right corner, 32x32)
        let gear_icon_size = 32.0;
        let gear_x = width as f32 - gear_icon_size - 10.0;
        let gear_y = 10.0;

        if x >= gear_x && x < gear_x + gear_icon_size && y >= gear_y && y < gear_y + gear_icon_size
        {
            self.control_panel.is_open = !self.control_panel.is_open;
            self.request_repaint_now();
            return;
        }

        // If panel not open, nothing else to check
        if !self.control_panel.is_open {
            return;
        }

        // Get panel geometry
        let rect = PanelRect::for_window(width as f32, height as f32);

        // Click outside panel closes it
        if !rect.contains(x, y) {
            self.control_panel.is_open = false;
            self.request_repaint_now();
            return;
        }

        // Determine which control was clicked
        let Some(control_idx) = rect.control_at_y(y) else {
            return; // Click in title or padding area
        };

        // Dispatch to control action
        match Control::ALL.get(control_idx) {
            Some(Control::DeviceSelector) => {
                // No-op - device cycling is future work
            }
            Some(Control::GainSlider) => {
                let new_gain = if self.control_panel.gain_value >= 2.0 {
                    0.5
                } else {
                    (self.control_panel.gain_value + 0.25).min(2.0)
                };
                self.control_panel.set_gain(new_gain);
                let mut state = self.audio_state.write();
                self.control_panel.apply_gain(&mut state);
            }
            Some(Control::AgcCheckbox) => {
                self.control_panel.toggle_agc();
                let mut state = self.audio_state.write();
                self.control_panel.apply_agc(&mut state);
            }
            Some(Control::PauseButton) => {
                self.control_panel.toggle_pause();
                if let Some(ref ctrl) = self.capture_control {
                    self.control_panel.apply_pause(ctrl);
                }
            }
            Some(Control::VizToggle) => {
                self.control_panel.toggle_viz_mode();
                self.mode = self.control_panel.viz_mode;
                for renderer in self.renderers.values_mut() {
                    renderer.set_mode(self.control_panel.viz_mode);
                }
            }
            Some(Control::ColorPicker) => {
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
            Some(Control::InjectionToggle) => {
                let mut state = self.audio_state.write();
                self.control_panel.toggle_injection(&mut state);
            }
            Some(Control::ModelSelector) => {
                self.control_panel.toggle_model();
            }
            Some(Control::AutoSaveToggle) => {
                self.control_panel.toggle_auto_save();
            }
            Some(Control::OpacitySlider) => {
                self.control_panel.adjust_opacity();
                for renderer in self.renderers.values_mut() {
                    renderer.set_opacity(self.control_panel.opacity);
                }
            }
            Some(Control::QuitButton) => {
                self.running.store(false, Ordering::Relaxed);
                if let Some(ref control) = self.capture_control {
                    control.stop();
                }
            }
            None => {}
        }

        self.request_repaint_now();
    }
}

impl ApplicationHandler for OverlayApp {
    fn resumed(&mut self, event_loop: &dyn ActiveEventLoop) {
        if !self.running.load(Ordering::Relaxed) {
            event_loop.exit();
            return;
        }

        event_loop.set_control_flow(ControlFlow::wait_duration(FRAME_INTERVAL));
    }

    fn about_to_wait(&mut self, event_loop: &dyn ActiveEventLoop) {
        if !self.running.load(Ordering::Relaxed) {
            event_loop.exit();
            return;
        }

        self.sync_control_state();

        let now = Instant::now();
        if !self.renderers.is_empty() && now >= self.next_redraw_deadline {
            for renderer in self.renderers.values() {
                renderer.window.request_redraw();
            }
            self.next_redraw_deadline = now.checked_add(FRAME_INTERVAL).unwrap_or(now);
        }

        event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_redraw_deadline));
    }

    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        let Some((_, monitor)) = event_loop.available_monitors().enumerate().next() else {
            log::warn!("No monitors available");
            return;
        };

        let Some(mode) = monitor.current_video_mode() else {
            log::warn!("No video mode available");
            return;
        };

        let window_attrs =
            create_window_attributes(event_loop, &mode, &monitor, self.tag.as_deref());
        let window = event_loop
            .create_window(window_attrs)
            .expect("Failed to create window");

        let mut renderer = Renderer::new(window, self.audio_state.clone(), self.mode);
        renderer.set_opacity(self.control_panel.opacity);
        let window_id = renderer.window.id();
        self.renderers.insert(window_id, renderer);
        self.next_redraw_deadline = Instant::now();
        event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_redraw_deadline));

        if event_loop.is_wayland() {
            release_focus_to_previous_window();
        }
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
                    let mut needs_repaint = false;
                    match event.logical_key {
                        Key::Character(ref c) if c == "p" || c == "P" || c == " " => {
                            self.handle_pause_toggle();
                            needs_repaint = true;
                        }
                        Key::Character(ref c) if c == "g" || c == "G" => {
                            self.handle_auto_gain_toggle();
                            needs_repaint = true;
                        }
                        Key::Character(ref c) if c == "w" || c == "W" => {
                            if let Some(renderer) = self.renderers.get_mut(&window_id) {
                                renderer.toggle_mode();
                            }
                            needs_repaint = true;
                        }
                        Key::Character(ref c) if c == "c" || c == "C" => {
                            self.control_panel.toggle_open();
                            needs_repaint = true;
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

                    if needs_repaint {
                        self.request_repaint_now();
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

        let mut needs_repaint = false;
        let mut surface_error = None;

        if let Some(renderer) = self.renderers.get_mut(&window_id) {
            match event {
                WindowEvent::SurfaceResized(size) => {
                    renderer.resize(size.width, size.height);
                    needs_repaint = true;
                }
                WindowEvent::ScaleFactorChanged { .. } => {
                    needs_repaint = true;
                }
                WindowEvent::Occluded(false) => {
                    needs_repaint = true;
                }
                WindowEvent::RedrawRequested => {
                    surface_error = renderer.draw_with_panel(Some(&self.control_panel)).err();
                }
                _ => {}
            }
        }

        match surface_error {
            Some(error @ (wgpu::SurfaceError::Outdated | wgpu::SurfaceError::Lost)) => {
                log::debug!("Overlay surface reconfigured after redraw interruption: {error:?}");
                needs_repaint = true;
            }
            Some(error @ (wgpu::SurfaceError::Timeout | wgpu::SurfaceError::Other)) => {
                log::warn!("Overlay redraw skipped due to transient surface error: {error:?}");
                needs_repaint = true;
            }
            Some(wgpu::SurfaceError::OutOfMemory) => {
                log::error!("Overlay renderer ran out of memory");
                self.running.store(false, Ordering::Relaxed);
                if let Some(ref control) = self.capture_control {
                    control.stop();
                }
                event_loop.exit();
            }
            None => {}
        }

        if needs_repaint {
            self.request_repaint_now();
        }
    }
}

fn create_window_attributes(
    event_loop: &dyn ActiveEventLoop,
    mode: &VideoMode,
    monitor: &winit::monitor::MonitorHandle,
    tag: Option<&str>,
) -> WindowAttributes {
    let monitor_size = mode.size();
    let window_width = (monitor_size.width as f32 * 0.25) as u32;
    let window_height = 210u32;

    let size = PhysicalSize::new(window_width.max(300), window_height.max(80));

    let title = match tag {
        Some(t) => format!("usit [{}]", t),
        None => "usit".to_string(),
    };

    let mut attrs = WindowAttributes::default()
        .with_decorations(false)
        .with_transparent(true)
        .with_surface_size(size)
        .with_title(&title)
        .with_resizable(false);

    if event_loop.is_wayland() {
        let wayland_attrs = WindowAttributesWayland::default()
            .with_name("usit", title)
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

fn release_focus_to_previous_window() {
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_millis(100));

        if let Err(e) = release_focus_kwin() {
            log::debug!("Could not release focus via KWin: {}", e);
        }
    });
}

fn release_focus_kwin() -> Result<(), Box<dyn std::error::Error>> {
    let connection = zbus::blocking::Connection::session()?;
    let proxy = zbus::blocking::Proxy::new(
        &connection,
        "org.kde.kglobalaccel",
        "/component/kwin",
        "org.kde.kglobalaccel.Component",
    )?;
    proxy.call_method("invokeShortcut", &("Walk Through Windows (Reverse)",))?;
    Ok(())
}
