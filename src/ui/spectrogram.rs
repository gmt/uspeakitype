//! Audio spectrogram visualization using instanced WGPU rendering

use std::sync::Arc;
use std::time::Instant;

use wgpu::util::DeviceExt;
use winit::dpi::PhysicalSize;

const ANIMATION_SPEED: f32 = 0.85;
const MIN_AMPLITUDE: f32 = 0.025;
const MAX_BAR_HEIGHT: f32 = 0.9;
const MIN_OPACITY: f32 = 0.15;
const SPEAKING_THRESHOLD: f32 = 0.2;

pub struct Spectrogram {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    instance_buffer: wgpu::Buffer,
    bar_data: Vec<f32>,
    target_bar_data: Vec<f32>,
    size: PhysicalSize<u32>,
    last_update: Instant,
    is_speaking: bool,
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
        surface_format: wgpu::TextureFormat,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Spectrogram Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/spectrogram.wgsl").into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Spectrogram Pipeline Layout"),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
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
            multiview: None,
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

        let num_bars = size.width as usize;
        let bar_data = vec![MIN_AMPLITUDE; num_bars];
        let target_bar_data = vec![MIN_AMPLITUDE; num_bars];

        let instances = create_instances(&bar_data, size);
        let instance_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Spectrogram Instances"),
            contents: bytemuck::cast_slice(&instances),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });

        Self {
            device,
            queue,
            pipeline,
            vertex_buffer,
            instance_buffer,
            bar_data,
            target_bar_data,
            size,
            last_update: Instant::now(),
            is_speaking: false,
        }
    }

    pub fn resize(&mut self, new_size: PhysicalSize<u32>) {
        let width_changed = self.size.width != new_size.width;
        self.size = new_size;

        if width_changed {
            let new_bars = new_size.width as usize;
            self.bar_data.resize(new_bars, MIN_AMPLITUDE);
            self.target_bar_data.resize(new_bars, MIN_AMPLITUDE);

            let instances = create_instances(&self.bar_data, new_size);
            self.instance_buffer =
                self.device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Spectrogram Instances"),
                        contents: bytemuck::cast_slice(&instances),
                        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    });
        } else {
            self.update_instance_buffer();
        }
    }

    pub fn update(&mut self, samples: &[f32]) {
        let num_bars = self.bar_data.len();

        if samples.is_empty() || samples.iter().all(|&x| x == 0.0) {
            self.is_speaking = false;
            self.target_bar_data.fill(0.0);
            self.animate();
            return;
        }

        let energy: f32 = samples.iter().take(100).map(|x| x.abs()).sum::<f32>() / 100.0;
        self.is_speaking = energy > SPEAKING_THRESHOLD;

        let samples_per_bar = samples.len().max(1) / num_bars.max(1);

        for i in 0..num_bars {
            let start = i * samples_per_bar;
            let end = ((i + 1) * samples_per_bar).min(samples.len());

            if start < samples.len() {
                let sum: f32 = samples[start..end].iter().map(|x| x.abs()).sum();
                let avg = sum / (end - start).max(1) as f32;
                self.target_bar_data[i] = (avg.sqrt() * 1.5).min(MAX_BAR_HEIGHT);
            }
        }

        self.animate();
    }

    fn animate(&mut self) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_update).as_secs_f32().min(0.1);
        self.last_update = now;

        let (rise_speed, fall_speed) = if self.is_speaking {
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

    fn update_instance_buffer(&self) {
        let instances = create_instances(&self.bar_data, self.size);
        self.queue
            .write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(&instances));
    }

    pub fn render(&self, encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Spectrogram Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        pass.set_pipeline(&self.pipeline);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
        pass.draw(0..4, 0..self.bar_data.len() as u32);
    }

    pub fn is_speaking(&self) -> bool {
        self.is_speaking
    }
}

fn create_instances(bar_data: &[f32], _size: PhysicalSize<u32>) -> Vec<BarInstance> {
    let num_bars = bar_data.len();
    let bar_width = 2.0 / num_bars as f32;
    let spacing = bar_width * 0.1;
    let actual_width = bar_width - spacing;

    bar_data
        .iter()
        .enumerate()
        .map(|(i, &amplitude)| {
            let x = -1.0 + i as f32 * bar_width;
            let height = amplitude * 2.0;
            let y = -1.0;

            let edge_factor = {
                let pos = i as f32 / (num_bars - 1).max(1) as f32;
                0.75 + 0.25 * (std::f32::consts::PI * (pos - 0.5)).cos()
            };

            let adjusted = amplitude * edge_factor;

            BarInstance {
                position: [x, y],
                size: [actual_width, height * edge_factor],
                color: [1.0, 1.0, 1.0, adjusted.max(MIN_OPACITY)],
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_instances_matches_bar_count() {
        let bar_data = vec![0.5; 100];
        let size = PhysicalSize::new(100, 80);
        let instances = create_instances(&bar_data, size);
        assert_eq!(instances.len(), bar_data.len());
    }

    #[test]
    fn create_instances_different_sizes() {
        for width in [50, 100, 200, 400, 640] {
            let bar_data = vec![MIN_AMPLITUDE; width];
            let size = PhysicalSize::new(width as u32, 80);
            let instances = create_instances(&bar_data, size);
            assert_eq!(
                instances.len(),
                width,
                "instance count must match bar count for width {}",
                width
            );
        }
    }

    #[test]
    fn bar_instance_size_is_32_bytes() {
        assert_eq!(
            std::mem::size_of::<BarInstance>(),
            32,
            "BarInstance must be 32 bytes for GPU buffer layout"
        );
    }
}
