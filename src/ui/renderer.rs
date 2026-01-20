//! WGPU rendering - window state and render loop

use std::sync::Arc;

use wgpu::util::DeviceExt;
use winit::dpi::PhysicalSize;
use winit::window::Window;

use super::spectrogram::{Spectrogram, SpectrogramMode};
use super::text_renderer::TextRenderer;
use super::theme::{Theme, DEFAULT_THEME};
use super::SharedAudioState;

const WINDOW_WIDTH: u32 = 400;
const TEXT_HEIGHT: u32 = 80;
const SPECTROGRAM_HEIGHT: u32 = 70;
const GAP: u32 = 10;
const PADDING: f32 = 12.0;

pub struct Renderer {
    pub window: Arc<dyn Window>,
    surface: wgpu::Surface<'static>,
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    config: wgpu::SurfaceConfiguration,
    bg_pipeline: wgpu::RenderPipeline,
    bg_vertices: wgpu::Buffer,
    bg_uniform_buffer: wgpu::Buffer,
    bg_bind_group: wgpu::BindGroup,
    theme: Theme,
    text_renderer: TextRenderer,
    spectrogram: Spectrogram,
    audio_state: SharedAudioState,
    mode: SpectrogramMode,
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
        let bg_uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Background Theme Uniform"),
            contents: bytemuck::cast_slice(&[theme_wgpu.background, theme_wgpu.shadow]),
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
            push_constant_ranges: &[],
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
            multiview: None,
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
            PhysicalSize::new(WINDOW_WIDTH, TEXT_HEIGHT),
            format,
        );

        let spectrogram = Spectrogram::with_mode(
            device.clone(),
            queue.clone(),
            PhysicalSize::new(WINDOW_WIDTH, SPECTROGRAM_HEIGHT),
            format,
            mode,
        );

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
            text_renderer,
            spectrogram,
            audio_state,
            mode,
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
        self.text_renderer
            .resize(PhysicalSize::new(width, TEXT_HEIGHT));
        self.spectrogram
            .resize(PhysicalSize::new(width, SPECTROGRAM_HEIGHT));
    }

    pub fn toggle_mode(&mut self) {
        self.mode = match self.mode {
            SpectrogramMode::BarMeter => SpectrogramMode::Waterfall,
            SpectrogramMode::Waterfall => SpectrogramMode::BarMeter,
        };
        self.spectrogram.set_mode(self.mode);
    }

    pub fn draw(&mut self) {
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
                ..Default::default()
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

        self.spectrogram.update(&samples);
        self.spectrogram.render(&mut encoder, &view);

        self.text_renderer.render(
            &view,
            &mut encoder,
            &committed,
            &partial,
            0.0,
            (SPECTROGRAM_HEIGHT + GAP) as f32,
            1.0,
            self.config.width,
            TEXT_HEIGHT,
            PADDING,
        );

        self.queue.submit(Some(encoder.finish()));
        output.present();
        self.window.request_redraw();
    }
}
