//! WGPU rendering - window state and render loop

use std::sync::Arc;

use wgpu::util::DeviceExt;
use winit::dpi::PhysicalSize;
use winit::window::Window;

use glyphon::{Color, TextBounds};

use super::control_panel::{
    transcript_text_bounds, Control, PanelRect, PANEL_PADDING, TEXT_PANEL_HEIGHT,
};
use super::icon::IconRenderer;
use super::spectrogram::{Spectrogram, SpectrogramMode};
use super::text_renderer::TextRenderer;
use super::theme::{Theme, DEFAULT_THEME};
use super::SharedAudioState;

fn panel_text_bounds(rect: &PanelRect) -> TextBounds {
    TextBounds {
        left: (rect.x + PANEL_PADDING) as i32,
        top: (rect.y + PANEL_PADDING) as i32,
        right: (rect.x + rect.width - PANEL_PADDING) as i32,
        bottom: (rect.y + rect.height - PANEL_PADDING) as i32,
    }
}

const WINDOW_WIDTH: u32 = 400;
const SPECTROGRAM_HEIGHT: u32 = 120;

pub fn compute_layout_heights(window_height: u32) -> (u32, u32) {
    let text_panel_height_const = TEXT_PANEL_HEIGHT as u32;

    let actual_text_panel_height = if window_height > text_panel_height_const {
        text_panel_height_const
    } else {
        window_height.saturating_sub(1)
    };

    let spectrogram_height = window_height
        .saturating_sub(actual_text_panel_height)
        .max(1);

    (spectrogram_height, actual_text_panel_height)
}

pub struct Renderer {
    pub window: Arc<dyn Window>,
    surface: wgpu::Surface<'static>,
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    config: wgpu::SurfaceConfiguration,
    bg_pipeline: wgpu::RenderPipeline,
    bg_vertices: wgpu::Buffer,
    #[allow(dead_code)] // TODO: Will be used for control panel rendering
    bg_uniform_buffer: wgpu::Buffer,
    bg_bind_group: wgpu::BindGroup,
    #[allow(dead_code)] // TODO: Will be used for control panel theming
    theme: Theme,
    opacity: f32,
    text_renderer: TextRenderer,
    icon_renderer: IconRenderer,
    spectrogram: Spectrogram,
    audio_state: SharedAudioState,
    mode: SpectrogramMode,
    // Panel background rendering
    panel_bg_pipeline: wgpu::RenderPipeline,
    panel_bg_uniform_buffer: wgpu::Buffer,
    panel_bg_bind_group: wgpu::BindGroup,
}

impl Renderer {
    pub fn new(
        window: Box<dyn Window>,
        audio_state: SharedAudioState,
        mode: SpectrogramMode,
    ) -> Self {
        let window: Arc<dyn Window> = Arc::from(window);

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());

        let surface = instance
            .create_surface(window.clone())
            .expect("Failed to create surface");

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: Some(&surface),
            ..Default::default()
        }))
        .expect("Failed to find GPU adapter");

        let (device, queue) = pollster::block_on(adapter.request_device(&Default::default()))
            .expect("Failed to create device");

        let device = Arc::new(device);
        let queue = Arc::new(queue);

        let win_size = window.surface_size();
        let format = wgpu::TextureFormat::Rgba8UnormSrgb;

        let caps = surface.get_capabilities(&adapter);
        let alpha_mode = if caps
            .alpha_modes
            .contains(&wgpu::CompositeAlphaMode::PreMultiplied)
        {
            wgpu::CompositeAlphaMode::PreMultiplied
        } else if caps
            .alpha_modes
            .contains(&wgpu::CompositeAlphaMode::PostMultiplied)
        {
            wgpu::CompositeAlphaMode::PostMultiplied
        } else {
            caps.alpha_modes[0]
        };

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: win_size.width.max(1),
            height: win_size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Background Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/rounded_rect.wgsl").into()),
        });

        // Create uniform buffer for theme colors
        let theme_wgpu = DEFAULT_THEME.to_wgpu();
        let opacity = 0.85f32;
        let bg_uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Background Theme Uniform"),
            contents: bytemuck::cast_slice(&[
                theme_wgpu.background,
                theme_wgpu.shadow,
                [opacity, 0.0, 0.0, 0.0], // Pad to vec4 for alignment
            ]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // Create bind group layout for theme uniform
        let bg_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Background Bind Group Layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        // Create bind group
        let bg_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Background Bind Group"),
            layout: &bg_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: bg_uniform_buffer.as_entire_binding(),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Background Pipeline Layout"),
            bind_group_layouts: &[&bg_bind_group_layout],
            immediate_size: 0,
        });

        let bg_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Background Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: 8,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2],
                }],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        #[repr(C)]
        #[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
        struct Vertex {
            position: [f32; 2],
        }

        let vertices = [
            Vertex {
                position: [-1.0, -1.0],
            },
            Vertex {
                position: [1.0, -1.0],
            },
            Vertex {
                position: [-1.0, 1.0],
            },
            Vertex {
                position: [1.0, 1.0],
            },
        ];

        let bg_vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Background Vertices"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let text_renderer = TextRenderer::new(
            device.clone(),
            queue.clone(),
            PhysicalSize::new(win_size.width, win_size.height),
            format,
        );

        let spectrogram = Spectrogram::with_mode(
            device.clone(),
            queue.clone(),
            PhysicalSize::new(WINDOW_WIDTH, SPECTROGRAM_HEIGHT),
            win_size.height,
            format,
            mode,
        );

        let icon_renderer = IconRenderer::new(device.clone(), queue.clone(), format);

        let panel_bg_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Panel Background Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/panel_bg.wgsl").into()),
        });

        let panel_bg_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Panel Background Uniforms"),
            size: 32, // 2 * vec4<f32> = 32 bytes
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let panel_bg_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Panel Background Bind Group Layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let panel_bg_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Panel Background Bind Group"),
            layout: &panel_bg_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: panel_bg_uniform_buffer.as_entire_binding(),
            }],
        });

        let panel_bg_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Panel Background Pipeline Layout"),
                bind_group_layouts: &[&panel_bg_bind_group_layout],
                immediate_size: 0,
            });

        let panel_bg_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Panel Background Pipeline"),
            layout: Some(&panel_bg_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &panel_bg_shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &panel_bg_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        Self {
            window,
            surface,
            device,
            queue,
            config,
            bg_pipeline,
            bg_vertices,
            bg_uniform_buffer,
            bg_bind_group,
            theme: DEFAULT_THEME,
            opacity: 0.85,
            text_renderer,
            icon_renderer,
            spectrogram,
            audio_state,
            mode,
            panel_bg_pipeline,
            panel_bg_uniform_buffer,
            panel_bg_bind_group,
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        if self.config.width == width && self.config.height == height {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        self.text_renderer.resize(PhysicalSize::new(width, height));
        self.spectrogram
            .resize(PhysicalSize::new(width, SPECTROGRAM_HEIGHT), height);
    }

    pub fn toggle_mode(&mut self) {
        self.mode = match self.mode {
            SpectrogramMode::BarMeter => SpectrogramMode::Waterfall,
            SpectrogramMode::Waterfall => SpectrogramMode::BarMeter,
        };
        self.spectrogram.set_mode(self.mode);
    }

    pub fn set_mode(&mut self, mode: SpectrogramMode) {
        self.mode = mode;
        self.spectrogram.set_mode(mode);
    }

    pub fn set_color_scheme(&mut self, scheme_name: &str) {
        use crate::spectrum::get_color_scheme;
        self.spectrogram
            .set_color_scheme(get_color_scheme(scheme_name));
    }

    pub fn set_opacity(&mut self, value: f32) {
        self.opacity = value.clamp(0.0, 1.0);
        let theme_wgpu = self.theme.to_wgpu();
        self.queue.write_buffer(
            &self.bg_uniform_buffer,
            0,
            bytemuck::cast_slice(&[
                theme_wgpu.background,
                theme_wgpu.shadow,
                [self.opacity, 0.0, 0.0, 0.0],
            ]),
        );
        self.spectrogram.set_opacity(self.opacity);
    }

    pub fn draw(&mut self) {
        self.draw_with_panel(None);
    }

    pub fn draw_with_panel(
        &mut self,
        control_panel: Option<&super::control_panel::ControlPanelState>,
    ) {
        let output = match self.surface.get_current_texture() {
            Ok(t) => t,
            Err(_) => {
                self.surface.configure(&self.device, &self.config);
                return;
            }
        };

        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.0,
                            g: 0.0,
                            b: 0.0,
                            a: 0.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.bg_pipeline);
            pass.set_bind_group(0, &self.bg_bind_group, &[]);
            pass.set_vertex_buffer(0, self.bg_vertices.slice(..));
            pass.draw(0..4, 0..1);
        }

        let (committed, partial, samples) = {
            let state = self.audio_state.read();
            (
                state.committed.clone(),
                state.partial.clone(),
                state.samples.clone(),
            )
        };

        let (_spectrogram_height, actual_text_panel_height) =
            compute_layout_heights(self.config.height);
        let text_panel_y = (self.config.height - actual_text_panel_height) as f32;

        self.spectrogram.update(&samples);
        self.spectrogram
            .render(&mut encoder, &view, self.config.height);

        if actual_text_panel_height > 0 {
            let text_rect = PanelRect {
                x: 0.0,
                y: text_panel_y,
                width: self.config.width as f32,
                height: actual_text_panel_height as f32,
            };
            self.render_panel_background(&mut encoder, &view, &text_rect, [0.12, 0.12, 0.14, 1.0]);

            let transcript_bounds = transcript_text_bounds(&text_rect);

            self.text_renderer.render(
                &view,
                &mut encoder,
                &committed,
                &partial,
                0.0,
                text_panel_y,
                1.0,
                self.config.width,
                actual_text_panel_height,
                10.0,
                transcript_bounds,
            );
        }

        if let Some(panel) = control_panel {
            self.render_control_panel(&view, &mut encoder, panel);
        }

        self.queue.submit(Some(encoder.finish()));
        output.present();
        self.window.request_redraw();
    }

    fn render_control_panel(
        &mut self,
        view: &wgpu::TextureView,
        encoder: &mut wgpu::CommandEncoder,
        panel: &super::control_panel::ControlPanelState,
    ) {
        // Render gear icon using SVG renderer (avoids glyphon vertex buffer conflict)
        let gear_size = 32.0;
        let gear_x = self.config.width as f32 - gear_size - 10.0;
        let gear_y = 10.0;

        self.icon_renderer.render_gear(
            encoder,
            view,
            gear_x,
            gear_y,
            gear_size,
            self.config.width as f32,
            self.config.height as f32,
        );

        if !panel.is_open {
            return;
        }

        let rect = PanelRect::for_window(self.config.width as f32, self.config.height as f32);

        self.render_panel_background(encoder, view, &rect, [0.12, 0.12, 0.14, 1.0]);

        let mut lines: Vec<(String, f32, f32)> = Vec::with_capacity(12);

        lines.push((
            "Control Panel (click gear to close)".to_string(),
            rect.x + PANEL_PADDING,
            rect.title_y(),
        ));

        for (i, control) in Control::ALL.iter().enumerate() {
            let label = match control {
                Control::DeviceSelector => {
                    let name = panel
                        .selected_device
                        .map(|id| format!("#{}", id))
                        .unwrap_or_else(|| "Default".to_string());
                    format!("Device: {}", name)
                }
                Control::GainSlider => {
                    format!(
                        "Gain: {:.2}x{}",
                        panel.gain_value,
                        if panel.agc_enabled { " (AGC)" } else { "" }
                    )
                }
                Control::AgcCheckbox => {
                    format!("AGC: {}", if panel.agc_enabled { "ON" } else { "OFF" })
                }
                Control::PauseButton => {
                    format!(
                        "Capture: {}",
                        if panel.is_paused { "PAUSED" } else { "RUNNING" }
                    )
                }
                Control::VizToggle => {
                    format!("Viz: {:?}", panel.viz_mode)
                }
                Control::ColorPicker => {
                    format!("Colors: {}", panel.color_scheme_name)
                }
                Control::InjectionToggle => {
                    let enabled = self.audio_state.read().injection_enabled;
                    format!("Injection: {}", if enabled { "ON" } else { "OFF" })
                }
                Control::ModelSelector => {
                    format!("Model: {}", panel.model)
                }
                Control::AutoSaveToggle => {
                    format!("Auto-save: {}", if panel.auto_save { "ON" } else { "OFF" })
                }
                Control::OpacitySlider => {
                    format!("Opacity: {}%", (panel.opacity * 100.0) as u32)
                }
                Control::QuitButton => "Quit".to_string(),
            };

            lines.push((label, rect.x + PANEL_PADDING, rect.row_y(i)));
        }

        let bounds = panel_text_bounds(&rect);
        let color = Color::rgba(230, 230, 235, 255);
        let panel_width = rect.width - 2.0 * PANEL_PADDING;

        self.text_renderer
            .render_panel_text(encoder, view, lines, bounds, color, panel_width);
    }

    fn render_panel_background(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        rect: &super::PanelRect,
        color: [f32; 4],
    ) {
        let ndc = rect.to_ndc(self.config.width as f32, self.config.height as f32);

        self.queue.write_buffer(
            &self.panel_bg_uniform_buffer,
            0,
            bytemuck::cast_slice(&[ndc, color]),
        );

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Panel Background"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

        pass.set_pipeline(&self.panel_bg_pipeline);
        pass.set_bind_group(0, &self.panel_bg_bind_group, &[]);
        pass.draw(0..4, 0..1);
    }
}

#[cfg(test)]
mod layout_tests {
    use super::*;

    #[test]
    fn test_layout_sizing_normal() {
        let (spec_h, text_h) = compute_layout_heights(600);
        assert_eq!(spec_h, 540);
        assert_eq!(text_h, 60);
    }

    #[test]
    fn test_layout_sizing_small() {
        let (spec_h, text_h) = compute_layout_heights(60);
        assert_eq!(spec_h, 1);
        assert_eq!(text_h, 59);
    }

    #[test]
    fn test_layout_sizing_tiny() {
        let (spec_h, text_h) = compute_layout_heights(30);
        assert_eq!(spec_h, 1);
        assert_eq!(text_h, 29);
    }

    #[test]
    fn test_layout_sizing_degenerate() {
        let (spec_h, text_h) = compute_layout_heights(1);
        assert_eq!(spec_h, 1);
        assert_eq!(text_h, 0);
    }
}
