//! Ratatui Widget for two-tone transcription text
//!
//! Renders committed (bold, bright) and partial (dim, gray) text with proper
//! truncation and spacing. Converts theme colors to ratatui Color format.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

use crate::ui::theme::Theme;

/// Widget for rendering two-tone transcription text
///
/// Displays committed text (bold, bright) followed by partial text (dim, gray).
/// Automatically truncates with "..." when text exceeds available width.
pub struct TranscriptWidget<'a> {
    committed: &'a str,
    partial: &'a str,
    theme: Theme,
    max_width: usize,
}

impl<'a> TranscriptWidget<'a> {
    /// Create a new TranscriptWidget
    pub fn new(committed: &'a str, partial: &'a str, theme: Theme, max_width: usize) -> Self {
        Self {
            committed,
            partial,
            theme,
            max_width,
        }
    }

    /// Convert theme color (0.0-1.0) to ratatui Color::Rgb (0-255)
    fn color_to_ratatui(&self, color: crate::spectrum::Color) -> Color {
        Color::Rgb(
            (color.r * 255.0) as u8,
            (color.g * 255.0) as u8,
            (color.b * 255.0) as u8,
        )
    }

    /// Build the styled Line with committed and partial text
    fn build_line(&self) -> Line<'a> {
        let mut spans = Vec::new();

        // Calculate total length to determine if truncation is needed
        let committed_len = self.committed.len();
        let partial_len = self.partial.len();
        let space_len = if !self.committed.is_empty() && !self.partial.is_empty() {
            1
        } else {
            0
        };
        let total_len = committed_len + space_len + partial_len;

        // If text fits, render normally
        if total_len <= self.max_width {
            self.add_committed_span(&mut spans);
            self.add_space_span(&mut spans);
            self.add_partial_span(&mut spans);
        } else {
            // Text exceeds width - truncate intelligently
            self.add_truncated_spans(&mut spans, total_len);
        }

        Line::from(spans)
    }

    /// Add committed text span (bold, bright color)
    fn add_committed_span(&self, spans: &mut Vec<Span<'a>>) {
        if !self.committed.is_empty() {
            let color = self.color_to_ratatui(self.theme.text_committed);
            spans.push(Span::styled(
                self.committed,
                Style::new().fg(color).add_modifier(Modifier::BOLD),
            ));
        }
    }

    /// Add space separator between committed and partial
    fn add_space_span(&self, spans: &mut Vec<Span<'a>>) {
        if !self.committed.is_empty() && !self.partial.is_empty() {
            spans.push(Span::raw(" "));
        }
    }

    /// Add partial text span (dim, gray color)
    fn add_partial_span(&self, spans: &mut Vec<Span<'a>>) {
        if !self.partial.is_empty() {
            let color = self.color_to_ratatui(self.theme.text_partial);
            spans.push(Span::styled(
                self.partial,
                Style::new().fg(color).add_modifier(Modifier::DIM),
            ));
        }
    }

    /// Add truncated spans when text exceeds max_width
    fn add_truncated_spans(&self, spans: &mut Vec<Span<'a>>, _total_len: usize) {
        let ellipsis = "...";
        let available = self.max_width.saturating_sub(ellipsis.len());

        // Truncate committed first, then partial
        if self.committed.len() <= available {
            // Committed fits entirely
            self.add_committed_span(spans);
            let remaining = available - self.committed.len();

            if remaining > 0 && !self.partial.is_empty() {
                // Add space if there's room
                if remaining > 1 {
                    spans.push(Span::raw(" "));
                    let partial_available = remaining - 1;
                    let truncated_partial =
                        &self.partial[..partial_available.min(self.partial.len())];
                    let color = self.color_to_ratatui(self.theme.text_partial);
                    spans.push(Span::styled(
                        truncated_partial,
                        Style::new().fg(color).add_modifier(Modifier::DIM),
                    ));
                }
            }
        } else {
            // Truncate committed
            let truncated_committed = &self.committed[..available];
            let color = self.color_to_ratatui(self.theme.text_committed);
            spans.push(Span::styled(
                truncated_committed,
                Style::new().fg(color).add_modifier(Modifier::BOLD),
            ));
        }

        // Always add ellipsis when truncating
        spans.push(Span::raw(ellipsis));
    }
}

impl<'a> Widget for TranscriptWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let line = self.build_line();
        Paragraph::new(line).render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::theme::DEFAULT_THEME;

    #[test]
    fn only_committed_text() {
        let widget = TranscriptWidget::new("hello", "", DEFAULT_THEME, 100);
        let line = widget.build_line();
        assert_eq!(line.spans.len(), 1);
        assert_eq!(line.spans[0].content, "hello");
    }

    #[test]
    fn only_partial_text() {
        let widget = TranscriptWidget::new("", "world", DEFAULT_THEME, 100);
        let line = widget.build_line();
        assert_eq!(line.spans.len(), 1);
        assert_eq!(line.spans[0].content, "world");
    }

    #[test]
    fn both_committed_and_partial() {
        let widget = TranscriptWidget::new("hello", "world", DEFAULT_THEME, 100);
        let line = widget.build_line();
        // Should have: committed, space, partial
        assert_eq!(line.spans.len(), 3);
        assert_eq!(line.spans[0].content, "hello");
        assert_eq!(line.spans[1].content, " ");
        assert_eq!(line.spans[2].content, "world");
    }

    #[test]
    fn empty_text() {
        let widget = TranscriptWidget::new("", "", DEFAULT_THEME, 100);
        let line = widget.build_line();
        assert_eq!(line.spans.len(), 0);
    }

    #[test]
    fn truncation_with_ellipsis() {
        let widget = TranscriptWidget::new("hello world", "test", DEFAULT_THEME, 10);
        let line = widget.build_line();
        // Should have ellipsis
        let has_ellipsis = line.spans.iter().any(|s| s.content.contains("..."));
        assert!(has_ellipsis);
    }

    #[test]
    fn committed_color_is_bright() {
        let widget = TranscriptWidget::new("hello", "", DEFAULT_THEME, 100);
        let line = widget.build_line();
        let style = line.spans[0].style;
        // Committed should have BOLD modifier
        assert!(style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn partial_color_is_dim() {
        let widget = TranscriptWidget::new("", "world", DEFAULT_THEME, 100);
        let line = widget.build_line();
        let style = line.spans[0].style;
        // Partial should have DIM modifier
        assert!(style.add_modifier.contains(Modifier::DIM));
    }

    #[test]
    fn space_only_when_both_present() {
        let widget1 = TranscriptWidget::new("hello", "", DEFAULT_THEME, 100);
        let line1 = widget1.build_line();
        let has_space1 = line1.spans.iter().any(|s| s.content == " ");
        assert!(!has_space1);

        let widget2 = TranscriptWidget::new("hello", "world", DEFAULT_THEME, 100);
        let line2 = widget2.build_line();
        let has_space2 = line2.spans.iter().any(|s| s.content == " ");
        assert!(has_space2);
    }

    #[test]
    fn truncation_respects_max_width() {
        let committed = "a".repeat(50);
        let partial = "b".repeat(50);
        let widget = TranscriptWidget::new(&committed, &partial, DEFAULT_THEME, 20);
        let line = widget.build_line();
        // Calculate visible length (excluding styling)
        let visible_len: usize = line.spans.iter().map(|s| s.content.len()).sum();
        assert!(visible_len <= 20);
    }
}
