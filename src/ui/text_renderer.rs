//! Glyphon-based text rendering for transcription display

use std::sync::Arc;

use glyphon::cosmic_text::Align;
use glyphon::{
    Attrs, Buffer, Cache, Color, Family, FontSystem, Metrics, Resolution, Shaping, SwashCache,
    TextArea, TextAtlas, TextBounds, TextRenderer as GlyphonTextRenderer, Viewport,
};
use wgpu::{Device, Queue, TextureView};
use winit::dpi::PhysicalSize;

use super::theme::{Theme, DEFAULT_THEME};

pub struct TextRenderer {
    font_system: FontSystem,
    swash_cache: SwashCache,
    atlas: TextAtlas,
    renderer: GlyphonTextRenderer,
    buffer: Buffer,
    buffer_partial: Buffer,
    panel_buffers: Vec<Buffer>,
    device: Arc<Device>,
    queue: Arc<Queue>,
    size: PhysicalSize<u32>,
    viewport: Viewport,
    theme: Theme,
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

        let mut buffer_partial = Buffer::new(&mut font_system, Metrics::new(16.0, 20.0));
        buffer_partial.set_size(
            &mut font_system,
            Some(size.width as f32),
            Some(size.height as f32),
        );

        // Panel buffers for control panel text (12 lines: 1 title + 11 controls)
        let mut panel_buffers = Vec::with_capacity(12);
        for _ in 0..12 {
            let mut buf = Buffer::new(&mut font_system, Metrics::new(14.0, 18.0));
            buf.set_size(&mut font_system, Some(size.width as f32), None);
            panel_buffers.push(buf);
        }

        Self {
            font_system,
            swash_cache,
            atlas,
            renderer,
            buffer,
            buffer_partial,
            panel_buffers,
            device,
            queue,
            size,
            viewport,
            theme: DEFAULT_THEME,
        }
    }

    pub fn resize(&mut self, size: PhysicalSize<u32>) {
        self.size = size;
        self.buffer.set_size(
            &mut self.font_system,
            Some(size.width as f32),
            Some(size.height as f32),
        );
        self.buffer_partial.set_size(
            &mut self.font_system,
            Some(size.width as f32),
            Some(size.height as f32),
        );
        for buf in &mut self.panel_buffers {
            buf.set_size(&mut self.font_system, Some(size.width as f32), None);
        }
        self.viewport.update(
            &self.queue,
            Resolution {
                width: size.width,
                height: size.height,
            },
        );
    }

    #[allow(clippy::too_many_arguments)]
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
        _area_height: u32,
        padding: f32,
        bounds: TextBounds,
    ) {
        if committed.is_empty() && partial.is_empty() {
            return;
        }

        let theme_wgpu = self.theme.to_wgpu();

        let committed_color = Color::rgba(
            (theme_wgpu.text_committed[0] * 255.0) as u8,
            (theme_wgpu.text_committed[1] * 255.0) as u8,
            (theme_wgpu.text_committed[2] * 255.0) as u8,
            (theme_wgpu.text_committed[3] * 255.0) as u8,
        );

        let partial_color = Color::rgba(
            (theme_wgpu.text_partial[0] * 255.0) as u8,
            (theme_wgpu.text_partial[1] * 255.0) as u8,
            (theme_wgpu.text_partial[2] * 255.0) as u8,
            (theme_wgpu.text_partial[3] * 255.0) as u8,
        );

        let font_size = 14.0 * scale;
        let metrics = Metrics::new(font_size, font_size * 1.3);

        self.buffer.lines.clear();
        self.buffer_partial.lines.clear();

        self.buffer.set_metrics(&mut self.font_system, metrics);
        self.buffer_partial
            .set_metrics(&mut self.font_system, metrics);

        self.buffer
            .set_size(&mut self.font_system, Some(area_width as f32), None);
        self.buffer_partial
            .set_size(&mut self.font_system, Some(area_width as f32), None);

        let mut text_areas = Vec::new();

        if !committed.is_empty() {
            self.buffer.set_text(
                &mut self.font_system,
                committed,
                &Attrs::new()
                    .family(Family::SansSerif)
                    .color(committed_color),
                Shaping::Advanced,
                Some(Align::Left),
            );
            self.buffer.shape_until_scroll(&mut self.font_system, true);

            text_areas.push(TextArea {
                buffer: &self.buffer,
                left: x + padding,
                top: y + padding,
                scale: 1.0,
                bounds,
                default_color: committed_color,
                custom_glyphs: &[],
            });
        }

        if !partial.is_empty() {
            let committed_width = if committed.is_empty() {
                0.0
            } else {
                self.buffer
                    .layout_runs()
                    .flat_map(|run| run.glyphs.iter())
                    .map(|glyph| glyph.w)
                    .sum::<f32>()
            };

            let offset = if committed.is_empty() {
                0.0
            } else {
                committed_width + font_size * 0.3
            };

            self.buffer_partial.set_text(
                &mut self.font_system,
                partial,
                &Attrs::new().family(Family::SansSerif).color(partial_color),
                Shaping::Advanced,
                Some(Align::Left),
            );
            self.buffer_partial
                .shape_until_scroll(&mut self.font_system, true);

            text_areas.push(TextArea {
                buffer: &self.buffer_partial,
                left: x + padding + offset,
                top: y + padding,
                scale: 1.0,
                bounds,
                default_color: partial_color,
                custom_glyphs: &[],
            });
        }

        self.viewport.update(
            &self.queue,
            Resolution {
                width: self.size.width,
                height: self.size.height,
            },
        );

        let prepare_result = self.renderer.prepare(
            &self.device,
            &self.queue,
            &mut self.font_system,
            &mut self.atlas,
            &self.viewport,
            text_areas,
            &mut self.swash_cache,
        );

        if prepare_result.is_ok() {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Text Render Pass"),
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

            let _ = self
                .renderer
                .render(&self.atlas, &self.viewport, &mut render_pass);
        }

        self.atlas.trim();
    }

    /// Render batched panel text (control panel title + controls)
    pub fn render_panel_text(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        lines: Vec<(String, f32, f32)>,
        bounds: TextBounds,
        color: Color,
        panel_width: f32,
    ) {
        let line_count = lines.len().min(12);
        let line_positions: Vec<(f32, f32)> =
            lines.iter().take(12).map(|(_, x, y)| (*x, *y)).collect();

        for (i, (text, _, _)) in lines.into_iter().take(12).enumerate() {
            self.panel_buffers[i].set_metrics(&mut self.font_system, Metrics::new(14.0, 18.0));
            self.panel_buffers[i].set_size(&mut self.font_system, Some(panel_width), None);
            self.panel_buffers[i].set_text(
                &mut self.font_system,
                &text,
                &Attrs::new().family(Family::SansSerif),
                Shaping::Advanced,
                Some(Align::Left),
            );
            self.panel_buffers[i].shape_until_scroll(&mut self.font_system, false);
        }

        let text_areas: Vec<TextArea> = (0..line_count)
            .map(|i| {
                let (x, y) = line_positions[i];
                TextArea {
                    buffer: &self.panel_buffers[i],
                    left: x,
                    top: y,
                    scale: 1.0,
                    bounds,
                    default_color: color,
                    custom_glyphs: &[],
                }
            })
            .collect();

        self.viewport.update(
            &self.queue,
            Resolution {
                width: self.size.width,
                height: self.size.height,
            },
        );

        let prepare_result = self.renderer.prepare(
            &self.device,
            &self.queue,
            &mut self.font_system,
            &mut self.atlas,
            &self.viewport,
            text_areas,
            &mut self.swash_cache,
        );

        if prepare_result.is_ok() {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Panel Text Render Pass"),
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

            let _ = self
                .renderer
                .render(&self.atlas, &self.viewport, &mut render_pass);
        }

        self.atlas.trim();
    }
}
