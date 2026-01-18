//! Glyphon-based text rendering for transcription display

use std::sync::Arc;

use glyphon::{
    Attrs, Buffer, Cache, Color, Family, FontSystem, Metrics, Resolution, Shaping, SwashCache,
    TextArea, TextAtlas, TextBounds, TextRenderer as GlyphonTextRenderer, Viewport,
};
use wgpu::{Device, Queue, TextureView};
use winit::dpi::PhysicalSize;

pub struct TextRenderer {
    font_system: FontSystem,
    swash_cache: SwashCache,
    atlas: TextAtlas,
    renderer: GlyphonTextRenderer,
    buffer: Buffer,
    device: Arc<Device>,
    queue: Arc<Queue>,
    size: PhysicalSize<u32>,
    viewport: Viewport,
}

impl TextRenderer {
    pub fn new(
        device: Arc<Device>,
        queue: Arc<Queue>,
        size: PhysicalSize<u32>,
        surface_format: wgpu::TextureFormat,
    ) -> Self {
        let mut font_system = FontSystem::new();
        let swash_cache = SwashCache::new();

        font_system.db_mut().load_system_fonts();

        let cache = Cache::new(&device);
        let viewport = Viewport::new(&device, &cache);
        let mut atlas = TextAtlas::new(&device, &queue, &cache, surface_format);
        let renderer =
            GlyphonTextRenderer::new(&mut atlas, &device, wgpu::MultisampleState::default(), None);

        let mut buffer = Buffer::new(&mut font_system, Metrics::new(16.0, 20.0));
        buffer.set_size(
            &mut font_system,
            Some(size.width as f32),
            Some(size.height as f32),
        );

        Self {
            font_system,
            swash_cache,
            atlas,
            renderer,
            buffer,
            device,
            queue,
            size,
            viewport,
        }
    }

    pub fn resize(&mut self, size: PhysicalSize<u32>) {
        self.size = size;
        self.buffer.set_size(
            &mut self.font_system,
            Some(size.width as f32),
            Some(size.height as f32),
        );
        self.viewport.update(
            &self.queue,
            Resolution {
                width: size.width,
                height: size.height,
            },
        );
    }

    pub fn render(
        &mut self,
        view: &TextureView,
        encoder: &mut wgpu::CommandEncoder,
        committed: &str,
        partial: &str,
        x: f32,
        y: f32,
        scale: f32,
        area_width: u32,
        area_height: u32,
        padding: f32,
    ) {
        let full_text = if partial.is_empty() {
            committed.to_string()
        } else if committed.is_empty() {
            partial.to_string()
        } else {
            format!("{} {}", committed, partial)
        };

        if full_text.is_empty() {
            return;
        }

        self.buffer.lines.clear();

        let font_size = 14.0 * scale;
        let metrics = Metrics::new(font_size, font_size * 1.3);
        self.buffer.set_metrics(&mut self.font_system, metrics);

        let text_color = if committed.is_empty() {
            Color::rgba(180, 180, 180, 220)
        } else {
            Color::rgba(255, 255, 255, 255)
        };

        self.buffer.set_size(
            &mut self.font_system,
            Some(area_width as f32 - padding * 2.0),
            None,
        );

        self.buffer.set_text(
            &mut self.font_system,
            &full_text,
            &Attrs::new().family(Family::SansSerif).color(text_color),
            Shaping::Advanced,
        );

        self.buffer.shape_until_scroll(&mut self.font_system, true);

        self.viewport.update(
            &self.queue,
            Resolution {
                width: self.size.width,
                height: self.size.height,
            },
        );

        let text_area = TextArea {
            buffer: &self.buffer,
            left: x + padding,
            top: y + padding,
            scale: 1.0,
            bounds: TextBounds {
                left: 0,
                top: 0,
                right: area_width as i32,
                bottom: area_height as i32,
            },
            default_color: text_color,
            custom_glyphs: &[],
        };

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Text Render Pass"),
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

            render_pass.set_scissor_rect(0, 0, area_width, area_height);

            if self
                .renderer
                .prepare(
                    &self.device,
                    &self.queue,
                    &mut self.font_system,
                    &mut self.atlas,
                    &self.viewport,
                    [text_area],
                    &mut self.swash_cache,
                )
                .is_ok()
            {
                let _ = self
                    .renderer
                    .render(&self.atlas, &self.viewport, &mut render_pass);
            }
        }

        self.atlas.trim();
    }
}
