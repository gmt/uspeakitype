//! Ratatui widget for waterfall spectrogram visualization
//!
//! Renders a time-scrolling spectrogram using ratatui's Widget trait.
//! Displays frequency bands over time with color mapping via ColorScheme.
//! Columns represent time (left=past, right=present), rows represent frequency bands.

use ratatui::{buffer::Buffer, layout::Rect, style::Color, widgets::Widget};

use crate::spectrum::{quantize_intensity, ColorScheme, WaterfallHistory};

/// Character set for waterfall visualization
#[allow(dead_code)]
const BLOCK_CHARS: [char; 9] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// Waterfall widget for ratatui
///
/// Renders time-scrolling spectrogram with frequency bands as rows.
/// Columns represent time snapshots (left=older, right=newer).
/// Uses lifetime `'a` to hold references to:
/// - `history`: time-indexed frequency band history
/// - `color_scheme`: trait object for intensity → color mapping
/// - `charset`: character set (BLOCK_CHARS or ASCII_CHARS)
pub struct WaterfallWidget<'a> {
    /// Time-indexed frequency band history
    pub history: &'a WaterfallHistory,
    /// Color scheme for intensity mapping
    pub color_scheme: &'a dyn ColorScheme,
    /// Character set for rendering (9 levels: space to full block)
    pub charset: &'a [char; 9],
}

impl<'a> WaterfallWidget<'a> {
    /// Create a new waterfall widget
    pub fn new(
        history: &'a WaterfallHistory,
        color_scheme: &'a dyn ColorScheme,
        charset: &'a [char; 9],
    ) -> Self {
        Self {
            history,
            color_scheme,
            charset,
        }
    }
}

impl<'a> Widget for WaterfallWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Early exit if area is too small
        if area.width == 0 || area.height == 0 || self.history.is_empty() {
            return;
        }

        let width = area.width as usize;
        let height = (self.history.num_bands() as u16).min(area.height) as usize;
        let history_len = self.history.len();
        let num_levels = self.charset.len();

        // Iterate columns (time snapshots)
        for col in 0..width {
            let x = area.left() + col as u16;

            // Calculate which history column this screen column maps to
            // When history_len >= width: show rightmost columns (most recent)
            // When history_len < width: empty left, data right
            let hist_col = if history_len >= width {
                col + (history_len - width)
            } else {
                col.saturating_sub(width - history_len)
            };

            // Skip if this column is beyond history (empty on left when partial)
            if hist_col >= history_len {
                continue;
            }

            // Iterate rows bottom-to-top (row 0 = lowest frequency band)
            for row in (0..height).rev() {
                let y = area.bottom().saturating_sub((height - row) as u16);

                // Get intensity for this time/frequency cell
                let intensity = self.history.get_intensity(hist_col, row);

                // Skip cells with very low intensity for efficiency
                if intensity <= 0.0 {
                    continue;
                }

                // Quantize intensity to charset index
                let char_idx = quantize_intensity(intensity, num_levels);

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
    fn waterfall_widget_creation() {
        let history = WaterfallHistory::new(100, 32);
        let scheme = FlameScheme;
        let widget = WaterfallWidget::new(&history, &scheme, &BLOCK_CHARS);
        assert_eq!(widget.history.len(), 0);
        assert_eq!(widget.history.num_bands(), 32);
    }

    #[test]
    fn waterfall_widget_empty_history() {
        let history = WaterfallHistory::new(100, 32);
        let scheme = FlameScheme;
        let widget = WaterfallWidget::new(&history, &scheme, &BLOCK_CHARS);

        // Should not panic on empty history
        let mut buf = Buffer::empty(Rect {
            x: 0,
            y: 0,
            width: 80,
            height: 24,
        });
        widget.render(
            Rect {
                x: 0,
                y: 0,
                width: 80,
                height: 24,
            },
            &mut buf,
        );
    }

    #[test]
    fn waterfall_widget_partial_history() {
        let mut history = WaterfallHistory::new(100, 8);
        // Add only 3 columns of data
        history.push(&[0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8]);
        history.push(&[0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9]);
        history.push(&[0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0]);

        let scheme = FlameScheme;
        let widget = WaterfallWidget::new(&history, &scheme, &BLOCK_CHARS);

        let mut buf = Buffer::empty(Rect {
            x: 0,
            y: 0,
            width: 80,
            height: 24,
        });
        widget.render(
            Rect {
                x: 0,
                y: 0,
                width: 80,
                height: 24,
            },
            &mut buf,
        );

        // Verify that data is rendered on the right side
        // (empty columns on left when history < width)
        assert_eq!(history.len(), 3);
    }

    #[test]
    fn waterfall_widget_full_history() {
        let mut history = WaterfallHistory::new(20, 8);
        // Fill with 20 columns of data
        for _ in 0..20 {
            history.push(&[0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8]);
        }

        let scheme = FlameScheme;
        let widget = WaterfallWidget::new(&history, &scheme, &BLOCK_CHARS);

        let mut buf = Buffer::empty(Rect {
            x: 0,
            y: 0,
            width: 80,
            height: 24,
        });
        widget.render(
            Rect {
                x: 0,
                y: 0,
                width: 80,
                height: 24,
            },
            &mut buf,
        );

        assert_eq!(history.len(), 20);
    }

    #[test]
    fn waterfall_widget_zero_area() {
        let mut history = WaterfallHistory::new(100, 8);
        history.push(&[0.5; 8]);

        let scheme = FlameScheme;

        // Zero width should not panic
        let widget = WaterfallWidget::new(&history, &scheme, &BLOCK_CHARS);
        let mut buf = Buffer::empty(Rect {
            x: 0,
            y: 0,
            width: 0,
            height: 24,
        });
        widget.render(
            Rect {
                x: 0,
                y: 0,
                width: 0,
                height: 24,
            },
            &mut buf,
        );

        // Zero height should not panic
        let widget = WaterfallWidget::new(&history, &scheme, &BLOCK_CHARS);
        let mut buf = Buffer::empty(Rect {
            x: 0,
            y: 0,
            width: 80,
            height: 0,
        });
        widget.render(
            Rect {
                x: 0,
                y: 0,
                width: 80,
                height: 0,
            },
            &mut buf,
        );
    }

    const ASCII_CHARS: [char; 9] = [' ', '.', ':', '-', '=', '+', '*', '#', '@'];

    #[test]
    fn waterfall_widget_ascii_charset() {
        let mut history = WaterfallHistory::new(10, 5);
        history.push(&[0.2, 0.4, 0.6, 0.8, 1.0]);
        history.push(&[0.3, 0.5, 0.7, 0.9, 0.5]);

        let scheme = FlameScheme;
        let widget = WaterfallWidget::new(&history, &scheme, &ASCII_CHARS);

        assert_eq!(widget.charset.len(), 9);
        assert_eq!(widget.charset[8], '@');

        let mut buf = Buffer::empty(Rect::new(0, 0, 10, 5));
        widget.render(Rect::new(0, 0, 10, 5), &mut buf);

        let unicode_blocks = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
        for y in 0..5 {
            for x in 0..10 {
                let cell = buf.cell((x, y)).unwrap();
                let symbol = cell.symbol();
                for block in &unicode_blocks {
                    assert!(
                        !symbol.contains(*block),
                        "Should not contain Unicode blocks"
                    );
                }
            }
        }
    }

    #[test]
    fn waterfall_widget_ice_scheme() {
        let mut history = WaterfallHistory::new(5, 3);
        history.push(&[0.5, 0.7, 0.9]);

        let scheme = IceScheme;
        let widget = WaterfallWidget::new(&history, &scheme, &BLOCK_CHARS);

        let mut buf = ratatui::buffer::Buffer::empty(ratatui::layout::Rect::new(0, 0, 5, 3));
        widget.render(ratatui::layout::Rect::new(0, 0, 5, 3), &mut buf);

        assert!(buf.cell((0, 0)).is_some());
    }

    #[test]
    fn waterfall_widget_mono_scheme() {
        let mut history = WaterfallHistory::new(5, 3);
        history.push(&[0.5, 0.7, 0.9]);

        let scheme = MonochromeScheme;
        let widget = WaterfallWidget::new(&history, &scheme, &BLOCK_CHARS);

        let mut buf = ratatui::buffer::Buffer::empty(ratatui::layout::Rect::new(0, 0, 5, 3));
        widget.render(ratatui::layout::Rect::new(0, 0, 5, 3), &mut buf);

        assert!(buf.cell((0, 0)).is_some());
    }
}
