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

        // Iterate columns (bands)
        for col in 0..width {
            let intensity = self.bands.get(col).copied().unwrap_or(0.0);
            let x = area.left() + col as u16;

            // Iterate rows bottom-to-top (reversed)
            for row in (0..height).rev() {
                let y = area.bottom().saturating_sub((height - row) as u16);

                // Calculate threshold for this row
                let threshold = (row as f32 + 0.5) / height as f32;

                // Calculate cell fill: how much of this cell should be filled
                let cell_fill = ((intensity - threshold) * height as f32 + 0.5).clamp(0.0, 1.0);

                // Quantize to charset index
                let char_idx = quantize_intensity(cell_fill, num_levels);

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
    use crate::spectrum::FlameScheme;

    #[test]
    fn spectrogram_widget_creation() {
        let bands = vec![0.0, 0.5, 1.0];
        let scheme = FlameScheme;
        let widget = SpectrogramWidget::new(&bands, &scheme, &BLOCK_CHARS);
        assert_eq!(widget.bands.len(), 3);
    }
}
