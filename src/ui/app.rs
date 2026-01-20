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

use super::renderer::Renderer;
use super::spectrogram::SpectrogramMode;
use super::SharedAudioState;

const MARGIN: i32 = 24;

pub fn run(
    audio_state: SharedAudioState,
    running: Arc<AtomicBool>,
    capture_control: Option<Arc<CaptureControl>>,
    mode: SpectrogramMode,
) {
    let event_loop = EventLoop::new().expect("Failed to create event loop");

    let app = Box::new(OverlayApp {
        renderers: HashMap::new(),
        audio_state,
        running,
        capture_control,
        mouse_position: None,
        mode,
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

    fn get_window_width(&self, window_id: &WindowId) -> Option<u32> {
        self.renderers
            .get(window_id)
            .map(|r| r.window.surface_size().width)
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
        let Some((_, monitor)) = event_loop
            .available_monitors()
            .into_iter()
            .enumerate()
            .next()
        else {
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

        let renderer = Renderer::new(window, self.audio_state.clone(), self.mode);
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
                        Key::Character(ref c) if c == "q" || c == "Q" => {
                            self.running.store(false, Ordering::Relaxed);
                            if let Some(ref control) = self.capture_control {
                                control.stop();
                            }
                            event_loop.exit();
                        }
                        Key::Named(NamedKey::Escape) => {
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
                if let Some((x, _y)) = self.mouse_position {
                    if let Some(width) = self.get_window_width(&window_id) {
                        let relative_x = x / width as f64;

                        if relative_x < 0.15 {
                            self.handle_pause_toggle();
                        } else if relative_x > 0.85 {
                            self.handle_auto_gain_toggle();
                        }
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
                    renderer.draw();
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
    let window_height = 160u32;

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
