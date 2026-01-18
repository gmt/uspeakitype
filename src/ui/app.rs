//! Winit event loop with Wayland layer shell support

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::monitor::VideoMode;
use winit::platform::wayland::{ActiveEventLoopExtWayland, WindowAttributesWayland};
use winit::platform::wayland::{Anchor, KeyboardInteractivity, Layer};
use winit::window::{WindowAttributes, WindowId};

use super::renderer::Renderer;
use super::SharedAudioState;

const MARGIN: i32 = 24;

pub fn run(audio_state: SharedAudioState, running: Arc<AtomicBool>) {
    let event_loop = EventLoop::new().expect("Failed to create event loop");

    let app = Box::new(OverlayApp {
        renderers: HashMap::new(),
        audio_state,
        running,
    });

    event_loop
        .run_app(Box::leak(app))
        .expect("Event loop failed");
}

struct OverlayApp {
    renderers: HashMap<WindowId, Renderer>,
    audio_state: SharedAudioState,
    running: Arc<AtomicBool>,
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

        let renderer = Renderer::new(window, self.audio_state.clone());
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

        if let Some(renderer) = self.renderers.get_mut(&window_id) {
            match event {
                WindowEvent::CloseRequested => {
                    self.running.store(false, Ordering::Relaxed);
                    event_loop.exit();
                }
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
            .with_keyboard_interactivity(KeyboardInteractivity::None);

        attrs = attrs.with_platform_attributes(Box::new(wayland_attrs));
    }

    attrs
}
