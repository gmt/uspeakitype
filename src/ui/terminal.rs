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
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
    Terminal as RatatuiTerminal,
};

use super::spectrogram_widget::SpectrogramWidget;
use super::status_widget::{StatusInfo as WidgetStatusInfo, StatusWidget};
use super::transcript_widget::TranscriptWidget;
use super::waterfall_widget::WaterfallWidget;

use crate::spectrum::{
    ColorScheme, FlameScheme, SpectrumAnalyzer, SpectrumConfig, WaterfallHistory,
};

use super::control_panel::{Control, ControlPanelState};
use super::theme::{Theme, DEFAULT_THEME};

const BLOCK_CHARS: [char; 9] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
const ASCII_CHARS: [char; 9] = [' ', '.', ':', '-', '=', '+', '*', '#', '@'];

#[derive(Clone, Copy, Debug)]
pub enum StatusInfo {
    Demo,
    Live { sample_rate: u32, channels: u16 },
}

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
        // Also used as fallback when height is 8-9 but width >= 35
        if width < 35 || height < 10 {
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

        // InjectionToggle
        (Control::InjectionToggle, LayoutMode::Full) => "Injection",
        (Control::InjectionToggle, LayoutMode::Compact) => "Inj",
        (Control::InjectionToggle, _) => "I",

        // ModelSelector
        (Control::ModelSelector, LayoutMode::Full) => "Model",
        (Control::ModelSelector, LayoutMode::Compact) => "Mdl",
        (Control::ModelSelector, _) => "M",

        // AutoSaveToggle
        (Control::AutoSaveToggle, LayoutMode::Full) => "Auto-Save",
        (Control::AutoSaveToggle, LayoutMode::Compact) => "AS",
        (Control::AutoSaveToggle, _) => "S",
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

/// Calculate a centered rectangle for popup overlays
fn centered_rect(percent_x: u16, fixed_height: u16, r: Rect) -> Rect {
    let popup_layout = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(fixed_height),
        Constraint::Fill(1),
    ])
    .split(r);

    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(popup_layout[1])[1]
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
    charset: &'static [char; 9],
    box_left: usize,
    box_top: usize,
    status_info: StatusInfo,
    theme: Theme,
    committed_text: String,
    partial_text: String,
    is_paused: bool,
    is_speaking: bool,
    injection_enabled: bool,
    panel_open: bool,
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

        let box_width = config.width + 2;
        let box_height = config.height + 2;
        let box_left = config.term_width.saturating_sub(box_width) / 2;
        let box_top = config
            .term_height
            .saturating_sub(box_height + BOTTOM_MARGIN);

        Self {
            config,
            analyzer,
            history,
            color_scheme: Box::new(FlameScheme),
            charset,
            box_left,
            box_top,
            status_info: StatusInfo::Demo,
            theme: DEFAULT_THEME,
            committed_text: String::new(),
            partial_text: String::new(),
            is_paused: false,
            is_speaking: false,
            injection_enabled: true,
            panel_open: false,
            ratatui_terminal: None,
        }
    }

    pub fn set_color_scheme(&mut self, scheme: Box<dyn ColorScheme>) {
        self.color_scheme = scheme;
    }

    pub fn set_status_info(&mut self, info: StatusInfo) {
        self.status_info = info;
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

    /// Resize the terminal visualizer to new dimensions.
    ///
    /// Recalculates layout and reinitializes the spectrogram analyzer if width changes.
    /// Clears the screen to prevent visual artifacts.
    pub fn resize(&mut self, new_width: u16, new_height: u16) {
        let new_width = new_width as usize;
        let new_height = new_height as usize;

        if self.config.term_width == new_width && self.config.term_height == new_height {
            return;
        }

        self.config.term_width = new_width;
        self.config.term_height = new_height;

        let new_spec_width = (new_width as f32 * 0.6).round() as usize;

        if self.config.width != new_spec_width {
            self.config.width = new_spec_width;

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

        let box_width = self.config.width + 2;
        let box_height = self.config.height + 2;
        self.box_left = self.config.term_width.saturating_sub(box_width) / 2;
        self.box_top = self
            .config
            .term_height
            .saturating_sub(box_height + BOTTOM_MARGIN);

        print!("\x1b[2J");
        let _ = io::stdout().flush();
    }

    pub fn push_samples(&mut self, samples: &[f32]) {
        self.analyzer.push_samples(samples);
    }

    /// Determine current layout mode based on terminal size
    pub fn layout_mode(&self) -> LayoutMode {
        LayoutMode::from_size(self.config.term_width, self.config.term_height)
    }

    pub fn set_paused(&mut self, paused: bool) {
        self.is_paused = paused;
    }

    pub fn set_speaking(&mut self, speaking: bool) {
        self.is_speaking = speaking;
    }

    pub fn set_injection_enabled(&mut self, enabled: bool) {
        self.injection_enabled = enabled;
    }

    pub fn set_panel_open(&mut self, open: bool) {
        self.panel_open = open;
    }

    pub fn process_and_render(&mut self) -> io::Result<()> {
        if self.layout_mode() == LayoutMode::Degenerate {
            print!("\x1b[2J\x1b[1;1H");
            let (icon, color) = if self.is_paused {
                let i = if self.config.use_unicode { '‖' } else { '=' };
                (i, "\x1b[33m")
            } else if self.is_speaking {
                let i = if self.config.use_unicode { '●' } else { '*' };
                (i, "\x1b[31m")
            } else {
                let i = if self.config.use_unicode { '▶' } else { '>' };
                (i, "\x1b[32m")
            };
            if self.config.use_color {
                print!("{}{}\x1b[0m", color, icon);
            } else {
                print!("{}", icon);
            }
            io::stdout().flush()?;
            return Ok(());
        }

        if !self.analyzer.process() {
            return Ok(());
        }

        self.history.push(&self.analyzer.data().bands);

        // Legacy ANSI rendering path - now handled by process_and_render_ratatui()
        Ok(())
    }

    fn convert_status_info(&self) -> WidgetStatusInfo {
        match self.status_info {
            StatusInfo::Demo => WidgetStatusInfo::Demo,
            StatusInfo::Live {
                sample_rate,
                channels,
            } => WidgetStatusInfo::Live {
                sample_rate,
                channels: channels as u32,
            },
        }
    }

    fn build_panel_data(
        &self,
        panel: &ControlPanelState,
    ) -> (Vec<String>, Option<usize>, &'static str) {
        let mode = self.layout_mode();

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
        let injection_value = if self.injection_enabled { "[X]" } else { "[ ]" };

        let controls: Vec<(Control, String)> = Control::ALL
            .iter()
            .map(|&control| {
                let value = match control {
                    Control::DeviceSelector => device_value.clone(),
                    Control::GainSlider => gain_value.clone(),
                    Control::AgcCheckbox => agc_value.to_string(),
                    Control::PauseButton => pause_value.to_string(),
                    Control::VizToggle => viz_value.to_string(),
                    Control::ColorPicker => panel.color_scheme_name.to_string(),
                    Control::InjectionToggle => injection_value.to_string(),
                    Control::ModelSelector => panel.model.to_string(),
                    Control::AutoSaveToggle => {
                        if panel.auto_save { "[X]" } else { "[ ]" }.to_string()
                    }
                };
                (control, format_control_label(control, &value, mode))
            })
            .collect();

        let labels: Vec<String> = controls.iter().map(|(_, label)| label.clone()).collect();

        let selected_index = panel
            .focused_control
            .and_then(|focused| controls.iter().position(|(c, _)| c == &focused));

        let title = panel_title(mode);

        (labels, selected_index, title)
    }

    pub fn process_and_render_ratatui(&mut self, panel: &ControlPanelState) -> io::Result<()> {
        if self.ratatui_terminal.is_none() {
            return self.process_and_render();
        }

        if self.layout_mode() != LayoutMode::Degenerate {
            if !self.analyzer.process() {
                return Ok(());
            }
            self.history.push(&self.analyzer.data().bands);
        }

        let layout_mode = self.layout_mode();
        let is_paused = self.is_paused;
        let is_speaking = self.is_speaking;
        let use_unicode = self.config.use_unicode;
        let bands = self.analyzer.data().bands.clone();
        let terminal_mode = self.config.mode;
        let status_info = self.convert_status_info();
        let committed_text = self.committed_text.clone();
        let partial_text = self.partial_text.clone();
        let theme = self.theme;
        let panel_is_open = panel.is_open;

        let panel_data = if panel_is_open {
            Some(self.build_panel_data(panel))
        } else {
            None
        };

        let color_scheme = &*self.color_scheme;
        let charset = self.charset;
        let history = &self.history;

        self.ratatui_terminal.as_mut().unwrap().draw(|frame| {
            if layout_mode == LayoutMode::Degenerate {
                let icon = if is_paused {
                    if use_unicode {
                        "‖"
                    } else {
                        "="
                    }
                } else if is_speaking {
                    if use_unicode {
                        "●"
                    } else {
                        "*"
                    }
                } else if use_unicode {
                    "▶"
                } else {
                    ">"
                };
                let paragraph = Paragraph::new(icon).alignment(Alignment::Center);
                frame.render_widget(paragraph, frame.area());
                return;
            }

            let layout = Layout::vertical([
                Constraint::Length(1),
                Constraint::Fill(1),
                Constraint::Length(1),
            ]);
            let areas = layout.split(frame.area());
            let status_area = areas[0];
            let main_area = areas[1];
            let transcript_area = areas[2];

            // Center visualization at 60% width (matching config.width calculation)
            let viz_width = (main_area.width as f32 * 0.6).round() as u16;
            let viz_area = {
                let h_layout = Layout::horizontal([
                    Constraint::Fill(1),
                    Constraint::Length(viz_width),
                    Constraint::Fill(1),
                ]);
                h_layout.split(main_area)[1]
            };

            match terminal_mode {
                TerminalMode::BarMeter => {
                    let widget = SpectrogramWidget::new(&bands, color_scheme, charset);
                    frame.render_widget(widget, viz_area);
                }
                TerminalMode::Waterfall => {
                    let widget = WaterfallWidget::new(history, color_scheme, charset);
                    frame.render_widget(widget, viz_area);
                }
            }

            let status_widget = StatusWidget::new(status_info)
                .paused(is_paused)
                .speaking(is_speaking);
            frame.render_widget(status_widget, status_area);

            let transcript_widget = TranscriptWidget::new(
                &committed_text,
                &partial_text,
                theme,
                transcript_area.width as usize,
            );
            frame.render_widget(transcript_widget, transcript_area);

            if let Some((labels, selected_index, title)) = panel_data {
                let items: Vec<ListItem> = labels
                    .iter()
                    .map(|label| ListItem::new(Line::raw(label.as_str())))
                    .collect();

                let mut list_state = ListState::default();
                list_state.select(selected_index);

                let list = List::new(items)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(Line::raw(title).alignment(Alignment::Center)),
                    )
                    .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
                    .highlight_symbol("> ");

                let popup_area = centered_rect(60, Control::ALL.len() as u16 + 2, frame.area());
                frame.render_widget(Clear, popup_area);
                frame.render_stateful_widget(list, popup_area, &mut list_state);
            }
        })?;

        Ok(())
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
        print!("\x1b[999;1H\x1b[?25h\x1b[0m");
        println!();
        io::stdout().flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_resize_updates_dimensions() {
        let config = TerminalConfig {
            width: 48,
            height: 6,
            mode: TerminalMode::BarMeter,
            use_color: true,
            use_unicode: false,
            term_width: 80,
            term_height: 24,
        };
        let mut visualizer = TerminalVisualizer::new(config);

        // Resize from 80x24 to 120x40
        visualizer.resize(120, 40);

        // Verify config was updated
        assert_eq!(visualizer.config.term_width, 120);
        assert_eq!(visualizer.config.term_height, 40);
    }

    #[test]
    fn test_resize_noop_when_same_size() {
        let config = TerminalConfig {
            width: 48,
            height: 6,
            mode: TerminalMode::BarMeter,
            use_color: true,
            use_unicode: false,
            term_width: 80,
            term_height: 24,
        };
        let mut visualizer = TerminalVisualizer::new(config);

        // Save original box_left value
        let original_box_left = visualizer.box_left;

        // Resize to same dimensions (should be no-op)
        visualizer.resize(80, 24);

        // Verify box_left unchanged (early return optimization)
        assert_eq!(visualizer.box_left, original_box_left);
    }

    #[test]
    fn test_resize_to_degenerate() {
        let config = TerminalConfig {
            width: 48,
            height: 6,
            mode: TerminalMode::BarMeter,
            use_color: true,
            use_unicode: false,
            term_width: 80,
            term_height: 24,
        };
        let mut visualizer = TerminalVisualizer::new(config);

        // Resize to degenerate dimensions (20x6 is below threshold: 25 cols or 8 rows)
        visualizer.resize(20, 6);

        // Verify layout_mode() returns Degenerate
        assert_eq!(visualizer.layout_mode(), LayoutMode::Degenerate);
    }
}
