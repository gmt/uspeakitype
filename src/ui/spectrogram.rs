use std::sync::Arc;
use std::time::Instant;

use wgpu::util::DeviceExt;
use winit::dpi::PhysicalSize;

use crate::spectrum::{
    intensity_to_height, ColorScheme, FlameScheme, SpectrumAnalyzer, SpectrumConfig,
    WaterfallHistory,
};

const ANIMATION_SPEED: f32 = 0.85;
const MIN_AMPLITUDE: f32 = 0.025;
const MIN_OPACITY: f32 = 0.15;

#[derive(Debug, Clone, Copy)]
pub enum SpectrogramMode {
    BarMeter,
    Waterfall,
}

pub struct Spectrogram {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    instance_buffer: wgpu::Buffer,
    instance_capacity: usize,
    instance_count: usize,
    size: PhysicalSize<u32>,
    window_height: u32,
    last_update: Instant,

    mode: SpectrogramMode,
    analyzer: SpectrumAnalyzer,
    history: WaterfallHistory,
    color_scheme: Box<dyn ColorScheme>,

    bar_data: Vec<f32>,
    target_bar_data: Vec<f32>,
    opacity: f32,
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 2],
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct BarInstance {
    position: [f32; 2],
    size: [f32; 2],
    color: [f32; 4],
}

impl Spectrogram {
    pub fn new(
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        size: PhysicalSize<u32>,
        window_height: u32,
        surface_format: wgpu::TextureFormat,
    ) -> Self {
        Self::with_mode(
            device,
            queue,
            size,
            window_height,
            surface_format,
            SpectrogramMode::BarMeter,
        )
    }

    pub fn with_mode(
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        size: PhysicalSize<u32>,
        window_height: u32,
        surface_format: wgpu::TextureFormat,
        mode: SpectrogramMode,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Spectrogram Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/spectrogram.wgsl").into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Spectrogram Pipeline Layout"),
            bind_group_layouts: &[],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Spectrogram Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[
                    wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &[wgpu::VertexAttribute {
                            offset: 0,
                            shader_location: 0,
                            format: wgpu::VertexFormat::Float32x2,
                        }],
                    },
                    wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<BarInstance>() as wgpu::BufferAddress,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &[
                            wgpu::VertexAttribute {
                                offset: 0,
                                shader_location: 1,
                                format: wgpu::VertexFormat::Float32x2,
                            },
                            wgpu::VertexAttribute {
                                offset: 8,
                                shader_location: 2,
                                format: wgpu::VertexFormat::Float32x2,
                            },
                            wgpu::VertexAttribute {
                                offset: 16,
                                shader_location: 3,
                                format: wgpu::VertexFormat::Float32x4,
                            },
                        ],
                    },
                ],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
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

        let vertices = [
            Vertex {
                position: [0.0, 0.0],
            },
            Vertex {
                position: [1.0, 0.0],
            },
            Vertex {
                position: [0.0, 1.0],
            },
            Vertex {
                position: [1.0, 1.0],
            },
        ];

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Spectrogram Vertices"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let num_bands = match mode {
            SpectrogramMode::BarMeter => 64,
            SpectrogramMode::Waterfall => (size.height as usize).max(16),
        };

        let spectrum_config = SpectrumConfig {
            num_bands,
            smoothing: 0.2,
            ..Default::default()
        };

        let analyzer = SpectrumAnalyzer::new(spectrum_config);
        let history = WaterfallHistory::new(size.width as usize, num_bands);
        let color_scheme: Box<dyn ColorScheme> = Box::new(FlameScheme);

        let bar_data = vec![MIN_AMPLITUDE; num_bands];
        let target_bar_data = vec![MIN_AMPLITUDE; num_bands];

        // Waterfall needs width * num_bands instances; bar meter needs only num_bands
        let max_instances = match mode {
            SpectrogramMode::BarMeter => num_bands,
            SpectrogramMode::Waterfall => (size.width as usize) * num_bands,
        };

        let instances = match mode {
            SpectrogramMode::BarMeter => Self::create_bar_instances(
                &bar_data,
                size,
                window_height,
                color_scheme.as_ref(),
                0.85,
            ),
            SpectrogramMode::Waterfall => Self::create_waterfall_instances(
                &history,
                size,
                window_height,
                color_scheme.as_ref(),
                0.85,
            ),
        };
        let buffer_size = (max_instances * std::mem::size_of::<BarInstance>()) as u64;
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Spectrogram Instances"),
            size: buffer_size,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&instance_buffer, 0, bytemuck::cast_slice(&instances));

        Self {
            device,
            queue,
            pipeline,
            vertex_buffer,
            instance_buffer,
            instance_capacity: max_instances,
            instance_count: instances.len(),
            size,
            window_height,
            last_update: Instant::now(),
            mode,
            analyzer,
            history,
            color_scheme,
            bar_data,
            target_bar_data,
            opacity: 0.85,
        }
    }

    pub fn set_color_scheme(&mut self, scheme: Box<dyn ColorScheme>) {
        self.color_scheme = scheme;
    }

    pub fn set_mode(&mut self, mode: SpectrogramMode) {
        let mode_changed = !matches!(
            (&self.mode, &mode),
            (SpectrogramMode::BarMeter, SpectrogramMode::BarMeter)
                | (SpectrogramMode::Waterfall, SpectrogramMode::Waterfall)
        );

        if !mode_changed {
            return;
        }

        self.mode = mode;

        let num_bands = match self.mode {
            SpectrogramMode::BarMeter => 64,
            SpectrogramMode::Waterfall => (self.size.height as usize).max(16),
        };

        let mut config = self.analyzer.config().clone();
        config.num_bands = num_bands;
        self.analyzer = SpectrumAnalyzer::new(config);

        self.history = WaterfallHistory::new(self.size.width as usize, num_bands);

        self.bar_data = vec![MIN_AMPLITUDE; num_bands];
        self.target_bar_data = vec![MIN_AMPLITUDE; num_bands];

        self.update_instance_buffer();
    }

    pub fn set_opacity(&mut self, opacity: f32) {
        self.opacity = opacity.clamp(0.0, 1.0);
        self.update_instance_buffer();
    }

    pub fn resize(&mut self, new_size: PhysicalSize<u32>, window_height: u32) {
        let size_changed = self.size.width != new_size.width || self.size.height != new_size.height;
        self.size = new_size;
        self.window_height = window_height;

        if size_changed {
            if matches!(self.mode, SpectrogramMode::Waterfall) {
                self.history = WaterfallHistory::new(
                    new_size.width as usize,
                    self.analyzer.config().num_bands,
                );
            }
            self.update_instance_buffer();
        }
    }

    pub fn update(&mut self, samples: &[f32]) {
        if samples.is_empty() || samples.iter().all(|&x| x == 0.0) {
            self.target_bar_data.fill(0.0);
            self.animate();
            return;
        }

        self.analyzer.push_samples(samples);
        if self.analyzer.process() {
            let bands = self.analyzer.data().bands.clone();
            self.history.push(&bands);

            for (i, &band) in bands.iter().enumerate() {
                if i < self.target_bar_data.len() {
                    self.target_bar_data[i] = band;
                }
            }
        }

        self.animate();
    }

    fn animate(&mut self) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_update).as_secs_f32().min(0.1);
        self.last_update = now;

        let is_active = self.analyzer.data().is_active;
        let (rise_speed, fall_speed) = if is_active {
            (ANIMATION_SPEED * 4.0, ANIMATION_SPEED * 2.0)
        } else {
            (ANIMATION_SPEED * 2.0, ANIMATION_SPEED * 3.0)
        };

        for (bar, target) in self.bar_data.iter_mut().zip(self.target_bar_data.iter()) {
            let diff = target - *bar;
            let speed = if diff > 0.0 { rise_speed } else { fall_speed };
            *bar += diff * speed * dt;
            *bar = bar.clamp(MIN_AMPLITUDE, 1.0);
        }

        self.update_instance_buffer();
    }

    fn update_instance_buffer(&mut self) {
        let instances = match self.mode {
            SpectrogramMode::BarMeter => Self::create_bar_instances(
                &self.bar_data,
                self.size,
                self.window_height,
                self.color_scheme.as_ref(),
                self.opacity,
            ),
            SpectrogramMode::Waterfall => Self::create_waterfall_instances(
                &self.history,
                self.size,
                self.window_height,
                self.color_scheme.as_ref(),
                self.opacity,
            ),
        };
        let instance_count = instances.len();
        self.instance_count = instance_count;

        if instance_count > self.instance_capacity {
            self.instance_capacity = instance_count;
            let buffer_size = (self.instance_capacity * std::mem::size_of::<BarInstance>()) as u64;
            self.instance_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Spectrogram Instances"),
                size: buffer_size,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        self.queue
            .write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(&instances));
    }

    fn create_bar_instances(
        bar_data: &[f32],
        size: PhysicalSize<u32>,
        window_height: u32,
        color_scheme: &dyn ColorScheme,
        opacity: f32,
    ) -> Vec<BarInstance> {
        let num_bars = bar_data.len();
        let bar_width = 2.0 / num_bars as f32;
        let spacing = bar_width * 0.1;
        let actual_width = bar_width - spacing;

        let spec_ratio = size.height as f32 / window_height.max(1) as f32;
        let base_y = 1.0 - 2.0 * spec_ratio;
        let max_height = 2.0 * spec_ratio;

        bar_data
            .iter()
            .enumerate()
            .map(|(i, &intensity)| {
                let x = -1.0 + i as f32 * bar_width;
                let height = intensity_to_height(intensity, max_height);

                let edge_factor = {
                    let pos = i as f32 / (num_bars - 1).max(1) as f32;
                    0.85 + 0.15 * (std::f32::consts::PI * (pos - 0.5)).cos()
                };

                let color = color_scheme.color_for_intensity(intensity);
                let alpha = (intensity * edge_factor).max(MIN_OPACITY) * opacity;

                BarInstance {
                    position: [x, base_y],
                    size: [actual_width, height * edge_factor],
                    color: [color.r, color.g, color.b, alpha],
                }
            })
            .collect()
    }

    fn create_waterfall_instances(
        history: &WaterfallHistory,
        size: PhysicalSize<u32>,
        window_height: u32,
        color_scheme: &dyn ColorScheme,
        opacity: f32,
    ) -> Vec<BarInstance> {
        let width = size.width as usize;
        let num_bands = history.num_bands();
        let history_len = history.len();

        let spec_ratio = size.height as f32 / window_height.max(1) as f32;
        let base_y = 1.0 - 2.0 * spec_ratio;
        let spec_height = 2.0 * spec_ratio;

        let cell_width = 2.0 / width as f32;
        let cell_height = spec_height / num_bands as f32;

        let mut instances = Vec::with_capacity(width * num_bands);

        for col in 0..width {
            let hist_idx = if history_len >= width {
                col
            } else if col >= width - history_len {
                col - (width - history_len)
            } else {
                continue;
            };

            for band in 0..num_bands {
                let intensity = history.get_intensity(hist_idx, band);
                if intensity < 0.01 {
                    continue;
                }

                let x = -1.0 + col as f32 * cell_width;
                let y = base_y + band as f32 * cell_height;
                let color = color_scheme.color_for_intensity(intensity);

                instances.push(BarInstance {
                    position: [x, y],
                    size: [cell_width, cell_height],
                    color: [color.r, color.g, color.b, intensity.max(0.3) * opacity],
                });
            }
        }

        instances
    }

    pub fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        _window_height: u32,
    ) {
        let instance_count = self.instance_count;

        if instance_count == 0 {
            return;
        }

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Spectrogram Pass"),
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
            multiview_mask: None,
            occlusion_query_set: None,
        });

        pass.set_pipeline(&self.pipeline);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
        pass.draw(0..4, 0..instance_count as u32);
    }

    pub fn is_speaking(&self) -> bool {
        self.analyzer.data().is_active
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bar_instance_size_is_32_bytes() {
        assert_eq!(
            std::mem::size_of::<BarInstance>(),
            32,
            "BarInstance must be 32 bytes for GPU buffer layout"
        );
    }

    #[test]
    fn set_mode_num_bands_calculation() {
        let _bar_mode = SpectrogramMode::BarMeter;
        let _waterfall_mode = SpectrogramMode::Waterfall;

        let size = PhysicalSize::new(800, 600);
        let bar_bands = match SpectrogramMode::BarMeter {
            SpectrogramMode::BarMeter => 64,
            SpectrogramMode::Waterfall => (size.height as usize).max(16),
        };
        assert_eq!(bar_bands, 64);

        let waterfall_bands = match SpectrogramMode::Waterfall {
            SpectrogramMode::BarMeter => 64,
            SpectrogramMode::Waterfall => (size.height as usize).max(16),
        };
        assert_eq!(waterfall_bands, 600);

        let small_size = PhysicalSize::new(100, 10);
        let min_bands = match SpectrogramMode::Waterfall {
            SpectrogramMode::BarMeter => 64,
            SpectrogramMode::Waterfall => (small_size.height as usize).max(16),
        };
        assert_eq!(min_bands, 16);
    }
}
