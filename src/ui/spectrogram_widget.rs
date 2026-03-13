//! Ratatui widget for spectrogram visualization
//!
//! Renders a bar meter spectrogram using ratatui's Widget trait.
//! Displays frequency bands as vertical bars with color mapping via ColorScheme.

use ratatui::{buffer::Buffer, layout::Rect, style::Color, widgets::Widget};

use crate::spectrum::{quantize_intensity, ColorScheme};

/// Character set for bar meter visualization
#[allow(dead_code)]
const BLOCK_CHARS: [char; 9] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// Spectrogram widget for ratatui
///
/// Renders frequency bands as vertical bars with per-cell color mapping.
/// Uses lifetime `'a` to hold references to:
/// - `bands`: frequency intensity values [0.0, 1.0]
/// - `color_scheme`: trait object for intensity → color mapping
/// - `charset`: character set (BLOCK_CHARS or ASCII_CHARS)
pub struct SpectrogramWidget<'a> {
    /// Frequency band intensities [0.0, 1.0]
    pub bands: &'a [f32],
    /// Color scheme for intensity mapping
    pub color_scheme: &'a dyn ColorScheme,
    /// Character set for rendering (9 levels: space to full block)
    pub charset: &'a [char; 9],
}

fn boosted_intensity_for_tight_viewport(intensity: f32, area: Rect) -> f32 {
    let intensity = intensity.clamp(0.0, 1.0);

    // At very small terminal sizes, low-energy bins collapse into pure black
    // "holes" that read more like rendering corruption than useful detail.
    // Apply a small visibility floor so narrow ANSI layouts stay legible.
    if intensity > 0.0 && (area.width <= 36 || area.height <= 8) {
        intensity.max(0.18)
    } else {
        intensity
    }
}

impl<'a> SpectrogramWidget<'a> {
    /// Create a new spectrogram widget
    pub fn new(
        bands: &'a [f32],
        color_scheme: &'a dyn ColorScheme,
        charset: &'a [char; 9],
    ) -> Self {
        Self {
            bands,
            color_scheme,
            charset,
        }
    }
}

impl<'a> Widget for SpectrogramWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Early exit if area is too small
        if area.width == 0 || area.height == 0 || self.bands.is_empty() {
            return;
        }

        let width = (self.bands.len() as u16).min(area.width) as usize;
        let height = area.height as usize;
        let num_levels = self.charset.len();
        let x_offset = (area.width.saturating_sub(width as u16)) / 2;

        for col in 0..width {
            let raw_intensity = self.bands.get(col).copied().unwrap_or(0.0);
            let intensity = boosted_intensity_for_tight_viewport(raw_intensity, area);
            let x = area.left() + x_offset + col as u16;

            // Iterate rows bottom-to-top (reversed)
            for row in (0..height).rev() {
                let y = area.bottom().saturating_sub((height - row) as u16);

                // Calculate threshold for this row (bottom = low threshold, top = high threshold)
                let threshold = ((height - 1 - row) as f32 + 0.5) / height as f32;

                // Calculate cell fill: how much of this cell should be filled
                let cell_fill = ((intensity - threshold) * height as f32 + 0.5).clamp(0.0, 1.0);

                // Quantize to charset index
                let mut char_idx = quantize_intensity(cell_fill, num_levels);

                // In very tight views, preserve at least a faint visible mark for
                // nonzero bins so the display doesn't turn into black striped gaps.
                if raw_intensity > 0.0 && (area.width <= 36 || area.height <= 8) && char_idx == 0 {
                    char_idx = 1;
                }

                // Get color for this intensity
                let color = self.color_scheme.color_for_intensity(intensity);

                // Convert color from [0.0, 1.0] to [0, 255]
                let ratatui_color = Color::Rgb(
                    (color.r * 255.0) as u8,
                    (color.g * 255.0) as u8,
                    (color.b * 255.0) as u8,
                );

                // Set cell with character and color
                if let Some(cell) = buf.cell_mut((x, y)) {
                    let mut buf_char = [0; 4];
                    let symbol = self.charset[char_idx].encode_utf8(&mut buf_char);
                    cell.set_symbol(symbol).set_fg(ratatui_color);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spectrum::{FlameScheme, IceScheme, MonochromeScheme};

    #[test]
    fn spectrogram_widget_creation() {
        let bands = vec![0.0, 0.5, 1.0];
        let scheme = FlameScheme;
        let widget = SpectrogramWidget::new(&bands, &scheme, &BLOCK_CHARS);
        assert_eq!(widget.bands.len(), 3);
    }

    const ASCII_CHARS: [char; 9] = [' ', '.', ':', '-', '=', '+', '*', '#', '@'];

    #[test]
    fn spectrogram_widget_ascii_charset() {
        let bands = vec![0.0, 0.25, 0.5, 0.75, 1.0];
        let scheme = FlameScheme;
        let widget = SpectrogramWidget::new(&bands, &scheme, &ASCII_CHARS);

        assert_eq!(widget.charset.len(), 9);
        assert_eq!(widget.charset[0], ' ');
        assert_eq!(widget.charset[8], '@');

        let mut buf = ratatui::buffer::Buffer::empty(ratatui::layout::Rect::new(0, 0, 5, 3));
        widget.render(ratatui::layout::Rect::new(0, 0, 5, 3), &mut buf);

        let unicode_blocks = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
        for y in 0..3 {
            for x in 0..5 {
                let cell = buf.cell((x, y)).unwrap();
                let symbol = cell.symbol();
                for block in &unicode_blocks {
                    assert!(
                        !symbol.contains(*block),
                        "Buffer should not contain Unicode block at ({}, {})",
                        x,
                        y
                    );
                }
            }
        }
    }

    #[test]
    fn spectrogram_widget_ice_scheme() {
        let bands = vec![0.0, 0.5, 1.0];
        let scheme = IceScheme;
        let widget = SpectrogramWidget::new(&bands, &scheme, &BLOCK_CHARS);

        let mut buf = ratatui::buffer::Buffer::empty(ratatui::layout::Rect::new(0, 0, 3, 3));
        widget.render(ratatui::layout::Rect::new(0, 0, 3, 3), &mut buf);

        // Just verify it renders without panic
        assert!(buf.cell((0, 0)).is_some());
    }

    #[test]
    fn spectrogram_widget_mono_scheme() {
        let bands = vec![0.0, 0.5, 1.0];
        let scheme = MonochromeScheme;
        let widget = SpectrogramWidget::new(&bands, &scheme, &BLOCK_CHARS);

        let mut buf = ratatui::buffer::Buffer::empty(ratatui::layout::Rect::new(0, 0, 3, 3));
        widget.render(ratatui::layout::Rect::new(0, 0, 3, 3), &mut buf);

        assert!(buf.cell((0, 0)).is_some());
    }

    #[test]
    fn spectrogram_widget_tight_viewport_keeps_nonzero_bins_visible() {
        let bands = vec![0.01, 0.02, 0.03];
        let scheme = FlameScheme;
        let widget = SpectrogramWidget::new(&bands, &scheme, &BLOCK_CHARS);

        let area = ratatui::layout::Rect::new(0, 0, 3, 3);
        let mut buf = ratatui::buffer::Buffer::empty(area);
        widget.render(area, &mut buf);

        for x in 0..3 {
            let mut saw_non_space = false;
            for y in 0..3 {
                let cell = buf.cell((x, y)).unwrap();
                if cell.symbol() != " " {
                    saw_non_space = true;
                    break;
                }
            }
            assert!(saw_non_space, "column {} disappeared in a tight viewport", x);
        }
    }
}
