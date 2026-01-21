//! Terminal-based spectrogram visualization
//!
//! # Layout Schema
//!
//! The terminal UI mirrors the OpenGL overlay layout:
//!
//! ```text
//! ┌─────────────────────────────────────────────┐
//! │                                             │
//! │              (empty space)                  │
//! │                                             │
//! │         ┌───────────────────────┐           │
//! │         │                       │           │
//! │         │     spectrogram       │           │
//! │         │                       │           │
//! │         └───────────────────────┘           │
//! │              (bottom margin)                │
//! └─────────────────────────────────────────────┘
//! ```
//!
//! - **Horizontal**: Centered in terminal
//! - **Vertical**: Anchored toward bottom with small margin
//! - **Border**: Box-drawing characters (unicode) or `|`, `-`, `+` (ascii)
//!
//! This matches the Wayland layer shell overlay which uses:
//! - `Anchor::BOTTOM` - anchored to bottom edge
//! - Horizontal centering via layer shell
//! - 24px margins on all sides

use std::io::{self, Write};

use ratatui::{
    backend::CrosstermBackend,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem},
    Terminal as RatatuiTerminal,
};

use crate::spectrum::{
    quantize_intensity, ColorScheme, FlameScheme, SpectrumAnalyzer, SpectrumConfig,
    WaterfallHistory,
};

use super::control_panel::{Control, ControlPanelState};
use super::theme::{Theme, DEFAULT_THEME};

const BLOCK_CHARS: [char; 9] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
const ASCII_CHARS: [char; 9] = [' ', '.', ':', '-', '=', '+', '*', '#', '@'];

struct BorderChars {
    top_left: char,
    top_right: char,
    bottom_left: char,
    bottom_right: char,
    horizontal: char,
    vertical: char,
}

const UNICODE_BORDER: BorderChars = BorderChars {
    top_left: '┌',
    top_right: '┐',
    bottom_left: '└',
    bottom_right: '┘',
    horizontal: '─',
    vertical: '│',
};

const ASCII_BORDER: BorderChars = BorderChars {
    top_left: '+',
    top_right: '+',
    bottom_left: '+',
    bottom_right: '+',
    horizontal: '-',
    vertical: '|',
};

#[derive(Clone, Copy)]
pub enum TerminalMode {
    BarMeter,
    Waterfall,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutMode {
    /// Full layout: width >= 50, height >= 10
    /// Panel width 50, full labels, 25-char title
    Full,

    /// Compact layout: width >= 35, height >= 10
    /// Abbreviated labels (e.g., "Dev:" instead of "Device:")
    Compact,

    /// Minimal layout: width >= 25, height >= 8
    /// Single-char labels (e.g., "D:" instead of "Device:")
    Minimal,

    /// Degenerate layout: width < 25 OR height < 8
    /// Hide panel entirely, show single-char status indicator only
    Degenerate,
}

impl LayoutMode {
    /// Determine layout mode based on terminal dimensions
    pub fn from_size(width: usize, height: usize) -> Self {
        // Degenerate: width < 25 OR height < 8
        if width < 25 || height < 8 {
            return LayoutMode::Degenerate;
        }

        // Minimal: 25 <= width < 35, height >= 8
        if width < 35 {
            return LayoutMode::Minimal;
        }

        // Compact: 35 <= width < 50, height >= 10
        if width < 50 {
            return LayoutMode::Compact;
        }

        // Full: width >= 50, height >= 10
        LayoutMode::Full
    }
}

/// Format control label with appropriate abbreviation based on layout mode
fn format_control_label(control: Control, value: &str, mode: LayoutMode) -> String {
    let prefix = match (control, mode) {
        // DeviceSelector
        (Control::DeviceSelector, LayoutMode::Full) => "Device",
        (Control::DeviceSelector, LayoutMode::Compact) => "Dev",
        (Control::DeviceSelector, _) => "D",

        // GainSlider
        (Control::GainSlider, LayoutMode::Full) => "Gain",
        (Control::GainSlider, LayoutMode::Compact) => "Gn",
        (Control::GainSlider, _) => "G",

        // AgcCheckbox
        (Control::AgcCheckbox, LayoutMode::Full) => "AGC",
        (Control::AgcCheckbox, LayoutMode::Compact) => "AGC",
        (Control::AgcCheckbox, _) => "A",

        // PauseButton
        (Control::PauseButton, LayoutMode::Full) => "Pause",
        (Control::PauseButton, LayoutMode::Compact) => "Pse",
        (Control::PauseButton, _) => "P",

        // VizToggle
        (Control::VizToggle, LayoutMode::Full) => "Visualization",
        (Control::VizToggle, LayoutMode::Compact) => "Viz",
        (Control::VizToggle, _) => "V",

        // ColorPicker
        (Control::ColorPicker, LayoutMode::Full) => "Color",
        (Control::ColorPicker, LayoutMode::Compact) => "Col",
        (Control::ColorPicker, _) => "C",
    };

    format!("{}: {}", prefix, value)
}

/// Get panel title appropriate for layout mode
fn panel_title(mode: LayoutMode) -> &'static str {
    match mode {
        LayoutMode::Full => " Panel (Up/Dn/Enter/Esc) ", // 25 chars (Phase 1 canonical)
        LayoutMode::Compact => " Panel ",                // 8 chars
        LayoutMode::Minimal => " ... ",                  // 5 chars
        LayoutMode::Degenerate => "",                    // Not used (panel hidden)
    }
}

#[derive(Clone)]
pub struct TerminalConfig {
    pub width: usize,
    pub height: usize,
    pub mode: TerminalMode,
    pub use_color: bool,
    pub use_unicode: bool,
    pub term_width: usize,
    pub term_height: usize,
}

impl Default for TerminalConfig {
    fn default() -> Self {
        Self {
            width: 80,
            height: 6,
            mode: TerminalMode::BarMeter,
            use_color: true,
            use_unicode: true,
            term_width: 80,
            term_height: 24,
        }
    }
}

pub struct TerminalVisualizer {
    config: TerminalConfig,
    analyzer: SpectrumAnalyzer,
    history: WaterfallHistory,
    color_scheme: Box<dyn ColorScheme>,
    output_buffer: String,
    charset: &'static [char; 9],
    border: &'static BorderChars,
    box_left: usize,
    box_top: usize,
    status_line: String,
    theme: Theme,
    committed_text: String,
    partial_text: String,
    is_paused: bool,
    ratatui_terminal: Option<RatatuiTerminal<CrosstermBackend<std::io::Stdout>>>,
}

const BOTTOM_MARGIN: usize = 2;

impl TerminalVisualizer {
    pub fn new(config: TerminalConfig) -> Self {
        let num_bands = match config.mode {
            TerminalMode::BarMeter => config.width,
            TerminalMode::Waterfall => config.height,
        };

        let spectrum_config = SpectrumConfig {
            num_bands,
            ..Default::default()
        };

        let analyzer = SpectrumAnalyzer::new(spectrum_config);
        let history = WaterfallHistory::new(config.width, num_bands);
        let charset = if config.use_unicode {
            &BLOCK_CHARS
        } else {
            &ASCII_CHARS
        };
        let border = if config.use_unicode {
            &UNICODE_BORDER
        } else {
            &ASCII_BORDER
        };

        let box_width = config.width + 2;
        let box_height = config.height + 2;
        let box_left = config.term_width.saturating_sub(box_width) / 2;
        let box_top = config
            .term_height
            .saturating_sub(box_height + BOTTOM_MARGIN);

        let buffer_size = (config.width + 20) * (config.height + 4);

        Self {
            config,
            analyzer,
            history,
            color_scheme: Box::new(FlameScheme),
            output_buffer: String::with_capacity(buffer_size),
            charset,
            border,
            box_left,
            box_top,
            status_line: String::new(),
            theme: DEFAULT_THEME,
            committed_text: String::new(),
            partial_text: String::new(),
            is_paused: false,
            ratatui_terminal: None,
        }
    }

    pub fn set_color_scheme(&mut self, scheme: Box<dyn ColorScheme>) {
        self.color_scheme = scheme;
    }

    pub fn set_status_line(&mut self, status: String) {
        self.status_line = status;
    }

    pub fn set_transcript(&mut self, committed: String, partial: String) {
        self.committed_text = committed;
        self.partial_text = partial;
    }

    pub fn toggle_mode(&mut self) {
        self.config.mode = match self.config.mode {
            TerminalMode::BarMeter => TerminalMode::Waterfall,
            TerminalMode::Waterfall => TerminalMode::BarMeter,
        };

        let num_bands = match self.config.mode {
            TerminalMode::BarMeter => self.config.width,
            TerminalMode::Waterfall => self.config.height,
        };

        let spectrum_config = SpectrumConfig {
            num_bands,
            ..Default::default()
        };
        self.analyzer = SpectrumAnalyzer::new(spectrum_config);

        self.history = WaterfallHistory::new(self.config.width, num_bands);
    }

    pub fn push_samples(&mut self, samples: &[f32]) {
        self.analyzer.push_samples(samples);
    }

    /// Determine current layout mode based on terminal size
    pub fn layout_mode(&self) -> LayoutMode {
        LayoutMode::from_size(self.config.term_width, self.config.term_height)
    }

    /// Set pause state for degenerate mode status indicator
    pub fn set_paused(&mut self, paused: bool) {
        self.is_paused = paused;
    }

    pub fn process_and_render(&mut self) -> io::Result<()> {
        // DEGENERATE MODE CHECK - must be first
        if self.layout_mode() == LayoutMode::Degenerate {
            // Clear screen, show single ASCII status character at 0,0
            print!("\x1b[2J\x1b[1;1H"); // Clear and home
            let status_char = if self.is_paused { '-' } else { '*' };
            let color = if self.is_paused {
                "\x1b[33m"
            } else {
                "\x1b[32m"
            };
            print!("{}{}\x1b[0m", color, status_char);
            io::stdout().flush()?;
            return Ok(()); // Skip ALL other rendering (spectrogram, hints, status)
        }

        if !self.analyzer.process() {
            return Ok(());
        }

        self.history.push(&self.analyzer.data().bands);

        match self.config.mode {
            TerminalMode::BarMeter => self.render_bar_meter(),
            TerminalMode::Waterfall => self.render_waterfall(),
        }
    }

    fn cursor_to(&mut self, row: usize, col: usize) {
        // Bounds check: skip if out of terminal bounds
        if row >= self.config.term_height || col >= self.config.term_width {
            return; // Skip cursor movement if out of bounds
        }

        use std::fmt::Write;
        let _ = write!(self.output_buffer, "\x1b[{};{}H", row + 1, col + 1);
    }

    fn draw_border(&mut self) {
        let width = self.config.width;
        let height = self.config.height;
        let left = self.box_left;
        let top = self.box_top;

        self.cursor_to(top, left);
        self.output_buffer.push(self.border.top_left);
        for _ in 0..width {
            self.output_buffer.push(self.border.horizontal);
        }
        self.output_buffer.push(self.border.top_right);

        for row in 0..height {
            self.cursor_to(top + 1 + row, left);
            self.output_buffer.push(self.border.vertical);
            self.cursor_to(top + 1 + row, left + width + 1);
            self.output_buffer.push(self.border.vertical);
        }

        self.cursor_to(top + height + 1, left);
        self.output_buffer.push(self.border.bottom_left);
        for _ in 0..width {
            self.output_buffer.push(self.border.horizontal);
        }
        self.output_buffer.push(self.border.bottom_right);
    }

    fn draw_status_line(&mut self) {
        if self.status_line.is_empty() {
            return;
        }

        let box_width = self.config.width + 2;
        let status_row = self.box_top + self.config.height + 3;
        let status_len = self.status_line.chars().count();
        let center_offset = box_width.saturating_sub(status_len) / 2;

        self.cursor_to(status_row, self.box_left + center_offset);
        self.output_buffer.push_str("\x1b[2m");
        self.output_buffer.push_str(&self.status_line);
        self.output_buffer.push_str("\x1b[0m");
    }

    fn draw_keybind_hints(&mut self) {
        // No-op: keybinds are now displayed in the status line
    }

    fn draw_transcript(&mut self) {
        let transcript_row = self.box_top + self.config.height + 2;
        let max_width = self.config.term_width.saturating_sub(4);

        let mut full_text = String::new();
        let theme_ansi = self.theme.to_ansi();

        if !self.committed_text.is_empty() {
            full_text.push_str("\x1b[1m");
            full_text.push_str(&theme_ansi.text_committed);
            full_text.push_str(&self.committed_text);
            full_text.push_str("\x1b[0m");
        }

        if !self.committed_text.is_empty() && !self.partial_text.is_empty() {
            full_text.push(' ');
        }

        if !self.partial_text.is_empty() {
            full_text.push_str("\x1b[2m");
            full_text.push_str(&theme_ansi.text_partial);
            full_text.push_str(&self.partial_text);
            full_text.push_str("\x1b[0m");
        }

        let display_text = self.truncate_with_ansi(&full_text, max_width);

        self.cursor_to(transcript_row, 2);
        self.output_buffer.push_str(&display_text);
    }

    fn truncate_with_ansi(&self, text: &str, max_visible_chars: usize) -> String {
        if text.chars().count() <= max_visible_chars {
            return text.to_string();
        }

        let mut truncated = String::new();
        let mut visible_count = 0;
        let mut in_escape_sequence = false;

        for ch in text.chars() {
            if ch == '\x1b' {
                in_escape_sequence = true;
            }

            truncated.push(ch);

            if !in_escape_sequence {
                visible_count += 1;
                if visible_count >= max_visible_chars - 3 {
                    break;
                }
            }

            if in_escape_sequence && ch == 'm' {
                in_escape_sequence = false;
            }
        }

        truncated.push_str("...");
        truncated
    }

    fn render_bar_meter(&mut self) -> io::Result<()> {
        let bands = self.analyzer.data().bands.clone();
        let width = bands.len().min(self.config.width);
        let height = self.config.height;
        let num_levels = self.charset.len();
        let left = self.box_left + 1;
        let top = self.box_top + 1;

        self.output_buffer.clear();
        self.draw_border();

        for row in (0..height).rev() {
            let screen_row = top + (height - 1 - row);
            self.cursor_to(screen_row, left);

            let threshold = (row as f32 + 0.5) / height as f32;

            for col in 0..width {
                let intensity = bands.get(col).copied().unwrap_or(0.0);
                let cell_fill = ((intensity - threshold) * height as f32 + 0.5).clamp(0.0, 1.0);
                let char_idx = quantize_intensity(cell_fill, num_levels);

                if self.config.use_color && intensity > 0.01 {
                    let color = self.color_scheme.color_for_intensity(intensity);
                    self.output_buffer.push_str(&color.to_ansi_fg());
                }

                self.output_buffer.push(self.charset[char_idx]);

                if self.config.use_color && intensity > 0.01 {
                    self.output_buffer.push_str("\x1b[0m");
                }
            }
        }

        self.draw_transcript();
        self.draw_status_line();
        print!("{}", self.output_buffer);
        io::stdout().flush()
    }

    fn render_waterfall(&mut self) -> io::Result<()> {
        let width = self.config.width;
        let height = self.config.height;
        let num_levels = self.charset.len();
        let history_len = self.history.len();
        let left = self.box_left + 1;
        let top = self.box_top + 1;

        self.output_buffer.clear();
        self.draw_border();

        for row in (0..height).rev() {
            let screen_row = top + (height - 1 - row);
            self.cursor_to(screen_row, left);

            for col in 0..width {
                let hist_col = if history_len >= width {
                    col
                } else {
                    col.saturating_sub(width - history_len)
                };

                let intensity = if hist_col < history_len {
                    self.history.get_intensity(hist_col, row)
                } else {
                    0.0
                };

                let char_idx = quantize_intensity(intensity, num_levels);

                if self.config.use_color && intensity > 0.01 {
                    let color = self.color_scheme.color_for_intensity(intensity);
                    self.output_buffer.push_str(&color.to_ansi_fg());
                }

                self.output_buffer.push(self.charset[char_idx]);

                if self.config.use_color && intensity > 0.01 {
                    self.output_buffer.push_str("\x1b[0m");
                }
            }
        }

        self.draw_transcript();
        self.draw_status_line();
        print!("{}", self.output_buffer);
        io::stdout().flush()
    }

    pub fn init_terminal(&mut self) -> io::Result<()> {
        print!("\x1b[2J\x1b[?25l");
        print!("\x1b[1;1H\x1b[2K");
        io::stdout().flush()?;

        // Initialize ratatui terminal
        let backend = CrosstermBackend::new(io::stdout());
        self.ratatui_terminal = Some(RatatuiTerminal::new(backend)?);

        Ok(())
    }

    /// Restore terminal state on exit.
    ///
    /// IMPORTANT: This MUST position the cursor before emitting any newlines.
    /// The spectrogram rendering leaves the cursor at arbitrary positions.
    /// `println!` only emits LF (no CR), so without explicit positioning,
    /// the shell prompt appears mid-screen starting from wherever the cursor was.
    /// This has been a recurring bug - do NOT simplify to just `println!`.
    pub fn cleanup_terminal() -> io::Result<()> {
        print!("\x1b[999;1H\x1b[?25h\x1b[0m\n");
        io::stdout().flush()
    }

    fn safe_truncate(s: &str, max_chars: usize) -> String {
        if s.chars().count() <= max_chars {
            s.to_string()
        } else {
            let truncated: String = s.chars().take(max_chars.saturating_sub(2)).collect();
            format!("{}..", truncated)
        }
    }

    /// Returns (panel_left, panel_top, panel_width, panel_height) for current terminal size.
    /// SINGLE SOURCE OF TRUTH for panel geometry - used by render and clear.
    fn panel_geometry(&self) -> (usize, usize, usize, usize) {
        let desired_width = 50; // Fits 25-char ASCII title (from Task 1.0)
        let panel_height = 8; // 1 border + 6 controls + 1 border (from Task 1.1)
        let panel_width = desired_width.min(self.config.term_width);
        let panel_left = (self.config.term_width.saturating_sub(panel_width)) / 2;
        let panel_top = (self.config.term_height.saturating_sub(panel_height)) / 2;
        (panel_left, panel_top, panel_width, panel_height)
    }

    pub fn render_control_panel(&mut self, panel: &ControlPanelState) -> io::Result<()> {
        if !panel.is_open {
            return Ok(());
        }

        let mode = self.layout_mode();
        let (panel_left, panel_top, panel_width, panel_height) = self.panel_geometry();

        let device_value = panel
            .selected_device
            .map(|id| format!("#{}", id))
            .unwrap_or_else(|| "Default".to_string());
        let gain_value = format!(
            "{:.1}x {}",
            panel.gain_value,
            if panel.agc_enabled {
                "(AGC active)"
            } else {
                ""
            }
        );
        let agc_value = if panel.agc_enabled { "[X]" } else { "[ ]" };
        let pause_value = if panel.is_paused { "[X]" } else { "[ ]" };
        let viz_value = match panel.viz_mode {
            crate::ui::spectrogram::SpectrogramMode::BarMeter => "Bar Meter",
            crate::ui::spectrogram::SpectrogramMode::Waterfall => "Waterfall",
        };

        let controls = [
            (
                Control::DeviceSelector,
                format_control_label(Control::DeviceSelector, &device_value, mode),
            ),
            (
                Control::GainSlider,
                format_control_label(Control::GainSlider, &gain_value, mode),
            ),
            (
                Control::AgcCheckbox,
                format_control_label(Control::AgcCheckbox, agc_value, mode),
            ),
            (
                Control::PauseButton,
                format_control_label(Control::PauseButton, pause_value, mode),
            ),
            (
                Control::VizToggle,
                format_control_label(Control::VizToggle, viz_value, mode),
            ),
            (
                Control::ColorPicker,
                format_control_label(Control::ColorPicker, panel.color_scheme_name, mode),
            ),
        ];

        let title = panel_title(mode);
        let items: Vec<ListItem> = controls
            .iter()
            .map(|(control, label)| {
                let is_focused = panel.focused_control == Some(*control);
                let prefix = if is_focused { " > " } else { "   " };
                let style = if is_focused {
                    Style::default().add_modifier(Modifier::REVERSED)
                } else {
                    Style::default()
                };
                ListItem::new(format!("{}{}", prefix, label)).style(style)
            })
            .collect();

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::White));

        let list = List::new(items).block(block);

        let terminal = self.ratatui_terminal.as_mut().ok_or_else(|| {
            io::Error::new(io::ErrorKind::Other, "Ratatui terminal not initialized")
        })?;

        let area = Rect {
            x: panel_left as u16,
            y: panel_top as u16,
            width: panel_width as u16,
            height: panel_height as u16,
        };

        terminal.draw(|f| {
            f.render_widget(list, area);
        })?;

        Ok(())
    }

    pub fn clear_panel_area(&mut self) {
        let (panel_left, panel_top, panel_width, panel_height) = self.panel_geometry();
        self.output_buffer.clear();
        for row in 0..panel_height {
            self.cursor_to(panel_top + row, panel_left);
            for _ in 0..panel_width {
                self.output_buffer.push(' ');
            }
        }
        print!("{}", self.output_buffer);
        io::stdout().flush().ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safe_truncate_short() {
        assert_eq!(TerminalVisualizer::safe_truncate("hello", 3), "h..");
    }

    #[test]
    fn test_safe_truncate_exact() {
        assert_eq!(TerminalVisualizer::safe_truncate("hello", 5), "hello");
    }

    #[test]
    fn test_safe_truncate_zero() {
        assert_eq!(TerminalVisualizer::safe_truncate("hello", 0), "..");
    }

    #[test]
    fn test_safe_truncate_unicode() {
        assert_eq!(TerminalVisualizer::safe_truncate("日本語", 2), "..");
    }

    #[test]
    fn test_layout_mode_full() {
        assert_eq!(LayoutMode::from_size(80, 24), LayoutMode::Full);
        assert_eq!(LayoutMode::from_size(50, 10), LayoutMode::Full); // boundary
    }

    #[test]
    fn test_layout_mode_compact() {
        assert_eq!(LayoutMode::from_size(49, 10), LayoutMode::Compact); // just below Full
        assert_eq!(LayoutMode::from_size(40, 15), LayoutMode::Compact);
    }

    #[test]
    fn test_layout_mode_minimal() {
        assert_eq!(LayoutMode::from_size(30, 10), LayoutMode::Minimal);
        assert_eq!(LayoutMode::from_size(25, 8), LayoutMode::Minimal); // boundary
    }

    #[test]
    fn test_layout_mode_degenerate() {
        assert_eq!(LayoutMode::from_size(20, 6), LayoutMode::Degenerate);
        assert_eq!(LayoutMode::from_size(24, 10), LayoutMode::Degenerate); // width boundary
        assert_eq!(LayoutMode::from_size(50, 7), LayoutMode::Degenerate); // height boundary
    }

    #[test]
    fn test_format_control_label_full() {
        assert_eq!(
            format_control_label(Control::DeviceSelector, "Default", LayoutMode::Full),
            "Device: Default"
        );
        assert_eq!(
            format_control_label(Control::VizToggle, "Bar Meter", LayoutMode::Full),
            "Visualization: Bar Meter"
        );
    }

    #[test]
    fn test_format_control_label_compact() {
        assert_eq!(
            format_control_label(Control::DeviceSelector, "Default", LayoutMode::Compact),
            "Dev: Default"
        );
        assert_eq!(
            format_control_label(Control::GainSlider, "1.5x", LayoutMode::Compact),
            "Gn: 1.5x"
        );
    }

    #[test]
    fn test_format_control_label_minimal() {
        assert_eq!(
            format_control_label(Control::VizToggle, "Bar Meter", LayoutMode::Minimal),
            "V: Bar Meter"
        );
        assert_eq!(
            format_control_label(Control::ColorPicker, "flame", LayoutMode::Minimal),
            "C: flame"
        );
    }

    #[test]
    fn test_panel_title() {
        assert_eq!(panel_title(LayoutMode::Full), " Panel (Up/Dn/Enter/Esc) ");
        assert_eq!(panel_title(LayoutMode::Compact), " Panel ");
        assert_eq!(panel_title(LayoutMode::Minimal), " ... ");
    }

    #[test]
    fn test_cursor_to_bounds_checking() {
        let config = TerminalConfig {
            width: 80,
            height: 6,
            mode: TerminalMode::BarMeter,
            use_color: true,
            use_unicode: false,
            term_width: 80,
            term_height: 24,
        };
        let mut visualizer = TerminalVisualizer::new(config);

        // Clear output buffer
        visualizer.output_buffer.clear();

        // Try to move cursor out of bounds (row 100, height is 24)
        visualizer.cursor_to(100, 0);

        // Output buffer should remain empty (no cursor movement)
        assert_eq!(visualizer.output_buffer.len(), 0);

        // Try valid cursor position
        visualizer.cursor_to(10, 10);

        // Output buffer should now have cursor escape sequence
        assert!(visualizer.output_buffer.len() > 0);
    }
}
