use std::io::{self, Write};

use crate::spectrum::{
    quantize_intensity, ColorScheme, FlameScheme, SpectrumAnalyzer, SpectrumConfig,
    WaterfallHistory,
};

const BLOCK_CHARS: [char; 9] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
const ASCII_CHARS: [char; 9] = [' ', '.', ':', '-', '=', '+', '*', '#', '@'];

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
}

impl Default for TerminalConfig {
    fn default() -> Self {
        Self {
            width: 80,
            height: 6,
            mode: TerminalMode::BarMeter,
            use_color: true,
            use_unicode: true,
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
}

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
        let buffer_size = (config.width + 1) * config.height + 64;

        Self {
            config,
            analyzer,
            history,
            color_scheme: Box::new(FlameScheme),
            output_buffer: String::with_capacity(buffer_size),
            charset,
        }
    }

    pub fn set_color_scheme(&mut self, scheme: Box<dyn ColorScheme>) {
        self.color_scheme = scheme;
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

    fn render_bar_meter(&mut self) -> io::Result<()> {
        let bands = &self.analyzer.data().bands;
        let width = bands.len().min(self.config.width);
        let height = self.config.height;
        let num_levels = self.charset.len();

        self.output_buffer.clear();
        self.output_buffer.push_str("\x1b[H");

        for row in (0..height).rev() {
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
            self.output_buffer.push('\n');
        }

        print!("{}", self.output_buffer);
        io::stdout().flush()
    }

    fn render_waterfall(&mut self) -> io::Result<()> {
        let width = self.config.width;
        let height = self.config.height;
        let num_levels = self.charset.len();
        let history_len = self.history.len();

        self.output_buffer.clear();
        self.output_buffer.push_str("\x1b[H");

        for row in (0..height).rev() {
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
            self.output_buffer.push('\n');
        }

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
