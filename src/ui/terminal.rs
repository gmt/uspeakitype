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

use crate::spectrum::{
    quantize_intensity, ColorScheme, FlameScheme, SpectrumAnalyzer, SpectrumConfig,
    WaterfallHistory,
};

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
        }
    }

    pub fn set_color_scheme(&mut self, scheme: Box<dyn ColorScheme>) {
        self.color_scheme = scheme;
    }

    pub fn set_status_line(&mut self, status: String) {
        self.status_line = status;
    }

    pub fn push_samples(&mut self, samples: &[f32]) {
        self.analyzer.push_samples(samples);
    }

    pub fn process_and_render(&mut self) -> io::Result<()> {
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
        let status_row = self.box_top + self.config.height + 2;
        let status_len = self.status_line.chars().count();
        let center_offset = box_width.saturating_sub(status_len) / 2;

        self.cursor_to(status_row, self.box_left + center_offset);
        self.output_buffer.push_str("\x1b[2m");
        self.output_buffer.push_str(&self.status_line);
        self.output_buffer.push_str("\x1b[0m");
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

        self.draw_status_line();
        print!("{}", self.output_buffer);
        io::stdout().flush()
    }

    pub fn init_terminal() -> io::Result<()> {
        print!("\x1b[2J\x1b[?25l");
        io::stdout().flush()
    }

    pub fn cleanup_terminal() -> io::Result<()> {
        println!("\x1b[?25h\x1b[0m");
        io::stdout().flush()
    }
}
