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
use std::time::Instant;

use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Gauge, List, ListItem, ListState, Paragraph},
    Terminal as RatatuiTerminal,
};

use super::spectrogram_widget::SpectrogramWidget;
use super::status_widget::{StatusInfo as WidgetStatusInfo, StatusWidget};
use super::transcript_widget::TranscriptWidget;
use super::waterfall_widget::WaterfallWidget;

use crate::spectrum::{
    ColorScheme, FlameScheme, SpectrumAnalyzer, SpectrumConfig, WaterfallHistory, WaterfallPacer,
    DEFAULT_WATERFALL_SECONDS_PER_SCREEN,
};

use super::control_panel::{panel_entries, Control, ControlPanelState, PanelEntry};
use super::theme::{Theme, DEFAULT_THEME};
use crate::config::AsrModelId;
use crate::ui::helper_status_short_summary;

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

        // OpacitySlider (WGPU-only, filtered in Task 5)
        (Control::OpacitySlider, LayoutMode::Full) => "Opacity",
        (Control::OpacitySlider, LayoutMode::Compact) => "Opac",
        (Control::OpacitySlider, _) => "O",

        // QuitButton
        (Control::QuitButton, LayoutMode::Full) => "Quit",
        (Control::QuitButton, LayoutMode::Compact) => "Quit",
        (Control::QuitButton, _) => "Q",
    };

    format!("{}: {}", prefix, value)
}

/// Get panel title appropriate for layout mode
fn panel_title(mode: LayoutMode) -> &'static str {
    match mode {
        LayoutMode::Full => " Input Helper (Up/Dn/Enter/Esc) ",
        LayoutMode::Compact => " Input Helper ",
        LayoutMode::Minimal => " ... ",
        LayoutMode::Degenerate => "", // Not used (panel hidden)
    }
}

struct PanelPopupItem {
    label: String,
    is_section: bool,
    control: Option<Control>,
}

struct PanelPopupData {
    items: Vec<PanelPopupItem>,
    selected_index: Option<usize>,
    title: &'static str,
    help_title: &'static str,
    help_body: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalMouseAction {
    None,
    ClosePanel,
    Activate(Control),
    FocusPrevious,
    FocusNext,
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
    pub requested_width: Option<usize>,
    pub requested_height: Option<usize>,
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
            requested_width: None,
            requested_height: None,
            mode: TerminalMode::BarMeter,
            use_color: true,
            use_unicode: true,
            term_width: 80,
            term_height: 24,
        }
    }
}

pub fn clamp_terminal_surface_dimensions(
    requested_width: Option<usize>,
    requested_height: Option<usize>,
    term_width: usize,
    term_height: usize,
) -> (usize, usize) {
    let live_term_width = term_width.max(1);
    let live_term_height = term_height.max(1);
    let max_surface_height = live_term_height.saturating_sub(4).max(1);

    let width = requested_width
        .unwrap_or(live_term_width)
        .max(1)
        .min(live_term_width);
    let height = requested_height.unwrap_or(6).max(1).min(max_surface_height);

    (width, height)
}

pub struct TerminalVisualizer {
    config: TerminalConfig,
    analyzer: SpectrumAnalyzer,
    history: WaterfallHistory,
    waterfall_pacer: WaterfallPacer,
    last_waterfall_column_at: Instant,
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
    transcription_available: bool,
    selected_source_name: Option<String>,
    source_change_pending_restart: bool,
    requested_model: Option<AsrModelId>,
    active_model: Option<AsrModelId>,
    download_progress: Option<f32>,
    /// Model/cache error to display prominently in red
    model_error: Option<String>,
    panel_open: bool,
    tag: Option<String>,
    ratatui_terminal: Option<RatatuiTerminal<CrosstermBackend<std::io::Stdout>>>,
}

const BOTTOM_MARGIN: usize = 2;

impl TerminalVisualizer {
    pub fn new(config: TerminalConfig, tag: Option<String>) -> Self {
        let waterfall_bands = config.term_height.saturating_sub(2).max(1);
        let num_bands = match config.mode {
            TerminalMode::BarMeter => config.width,
            TerminalMode::Waterfall => waterfall_bands,
        };

        let spectrum_config = SpectrumConfig {
            num_bands,
            ..Default::default()
        };

        let analyzer = SpectrumAnalyzer::new(spectrum_config);
        let history = WaterfallHistory::new(config.width, num_bands);
        let waterfall_pacer = WaterfallPacer::new(DEFAULT_WATERFALL_SECONDS_PER_SCREEN);
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
            waterfall_pacer,
            last_waterfall_column_at: Instant::now(),
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
            transcription_available: false,
            selected_source_name: None,
            source_change_pending_restart: false,
            requested_model: None,
            active_model: None,
            download_progress: None,
            model_error: None,
            panel_open: false,
            tag,
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

    fn waterfall_surface_height(&self) -> usize {
        self.config.term_height.saturating_sub(2).max(1)
    }

    fn analyzer_num_bands_for_mode(&self, mode: TerminalMode) -> usize {
        match mode {
            TerminalMode::BarMeter => self.config.width,
            TerminalMode::Waterfall => self.waterfall_surface_height(),
        }
    }

    fn reset_spectrum_state(&mut self) {
        let num_bands = self.analyzer_num_bands_for_mode(self.config.mode);
        let spectrum_config = SpectrumConfig {
            num_bands,
            ..Default::default()
        };

        self.analyzer = SpectrumAnalyzer::new(spectrum_config);
        self.history = WaterfallHistory::new(self.config.width, num_bands);
        self.waterfall_pacer.reset();
        self.last_waterfall_column_at = Instant::now();
    }

    pub fn toggle_mode(&mut self) {
        self.config.mode = match self.config.mode {
            TerminalMode::BarMeter => TerminalMode::Waterfall,
            TerminalMode::Waterfall => TerminalMode::BarMeter,
        };
        self.reset_spectrum_state();
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

        let (new_spec_width, new_spec_height) = clamp_terminal_surface_dimensions(
            self.config.requested_width,
            self.config.requested_height,
            new_width,
            new_height,
        );
        let width_changed = self.config.width != new_spec_width;
        let height_changed = self.config.height != new_spec_height;
        self.config.width = new_spec_width;
        self.config.height = new_spec_height;

        let target_num_bands = self.analyzer_num_bands_for_mode(self.config.mode);
        let bands_changed = self.analyzer.config().num_bands != target_num_bands;

        if width_changed || height_changed || bands_changed {
            self.reset_spectrum_state();
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

    pub fn set_transcription_available(&mut self, available: bool) {
        self.transcription_available = available;
    }

    pub fn set_source_status(
        &mut self,
        selected_source_name: Option<String>,
        source_change_pending_restart: bool,
    ) {
        self.selected_source_name = selected_source_name;
        self.source_change_pending_restart = source_change_pending_restart;
    }

    pub fn set_model_status(
        &mut self,
        requested_model: Option<AsrModelId>,
        active_model: Option<AsrModelId>,
    ) {
        self.requested_model = requested_model;
        self.active_model = active_model;
    }

    pub fn set_download_progress(&mut self, progress: Option<f32>) {
        self.download_progress = progress;
    }

    pub fn set_model_error(&mut self, error: Option<String>) {
        self.model_error = error;
    }

    pub fn set_panel_open(&mut self, open: bool) {
        self.panel_open = open;
    }

    fn push_waterfall_column(&mut self, bands: &[f32]) {
        if !matches!(self.config.mode, TerminalMode::Waterfall) {
            return;
        }

        let now = Instant::now();
        let elapsed = now.duration_since(self.last_waterfall_column_at);
        self.last_waterfall_column_at = now;
        self.waterfall_pacer
            .push_for_elapsed(&mut self.history, bands, elapsed);
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

        let bands = self.analyzer.data().bands.clone();
        self.push_waterfall_column(&bands);

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

    fn build_panel_data(&self, panel: &ControlPanelState) -> PanelPopupData {
        let mode = self.layout_mode();
        let (help_title, help_body) = panel.help_copy();
        let audio_state = crate::ui::AudioState {
            injection_enabled: self.injection_enabled,
            transcription_available: self.transcription_available,
            selected_source_name: self.selected_source_name.clone(),
            source_change_pending_restart: self.source_change_pending_restart,
            requested_model: self.requested_model,
            active_model: self.active_model,
            download_progress: self.download_progress,
            model_error: self.model_error.clone(),
            ..crate::ui::AudioState::default()
        };

        let mut items = Vec::new();
        let mut selected_index = None;

        for entry in panel_entries(false) {
            match entry {
                PanelEntry::Section(section) => items.push(PanelPopupItem {
                    label: section.title().to_uppercase(),
                    is_section: true,
                    control: None,
                }),
                PanelEntry::Control(control) => {
                    let value = panel.control_value(control, &audio_state);
                    if panel.focused_control == Some(control) {
                        selected_index = Some(items.len());
                    }
                    items.push(PanelPopupItem {
                        label: format_control_label(control, &value, mode),
                        is_section: false,
                        control: Some(control),
                    });
                }
            }
        }

        PanelPopupData {
            items,
            selected_index,
            title: panel_title(mode),
            help_title,
            help_body,
        }
    }

    fn panel_popup_geometry(&self, panel: &ControlPanelState) -> Option<(Rect, Rect, Rect)> {
        if !panel.is_open || self.layout_mode() == LayoutMode::Degenerate {
            return None;
        }

        let panel_popup = self.build_panel_data(panel);
        let frame_area = Rect::new(
            0,
            0,
            self.config.term_width as u16,
            self.config.term_height as u16,
        );
        let popup_height = panel_popup.items.len() as u16 + 8;
        let popup_area = centered_rect(70, popup_height, frame_area);
        let popup_layout = Layout::vertical([
            Constraint::Length(panel_popup.items.len() as u16 + 2),
            Constraint::Length(5),
        ])
        .split(popup_area);

        Some((popup_area, popup_layout[0], popup_layout[1]))
    }

    pub fn mouse_action_for_panel(
        &self,
        panel: &ControlPanelState,
        column: u16,
        row: u16,
        scroll_up: bool,
        scroll_down: bool,
    ) -> TerminalMouseAction {
        if !panel.is_open {
            return TerminalMouseAction::None;
        }

        if scroll_up {
            return TerminalMouseAction::FocusPrevious;
        }
        if scroll_down {
            return TerminalMouseAction::FocusNext;
        }

        let Some((popup_area, list_area, _help_area)) = self.panel_popup_geometry(panel) else {
            return TerminalMouseAction::None;
        };

        let inside_popup = column >= popup_area.x
            && column < popup_area.x + popup_area.width
            && row >= popup_area.y
            && row < popup_area.y + popup_area.height;

        if !inside_popup {
            return TerminalMouseAction::ClosePanel;
        }

        let inside_list_inner = column > list_area.x
            && column < list_area.x + list_area.width.saturating_sub(1)
            && row > list_area.y
            && row < list_area.y + list_area.height.saturating_sub(1);

        if !inside_list_inner {
            return TerminalMouseAction::None;
        }

        let item_index = (row - list_area.y - 1) as usize;
        let panel_popup = self.build_panel_data(panel);
        panel_popup
            .items
            .get(item_index)
            .and_then(|item| item.control)
            .map(TerminalMouseAction::Activate)
            .unwrap_or(TerminalMouseAction::None)
    }

    pub fn process_and_render_ratatui(&mut self, panel: &ControlPanelState) -> io::Result<()> {
        if self.ratatui_terminal.is_none() {
            return self.process_and_render();
        }

        if self.layout_mode() != LayoutMode::Degenerate {
            if !self.analyzer.process() {
                return Ok(());
            }
            let bands = self.analyzer.data().bands.clone();
            self.push_waterfall_column(&bands);
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
        let download_progress = self.download_progress;
        let model_error = self.model_error.clone();

        let panel_data = if panel_is_open {
            Some(self.build_panel_data(panel))
        } else {
            None
        };

        let helper_state = crate::ui::AudioState {
            injection_enabled: self.injection_enabled,
            transcription_available: self.transcription_available,
            requested_model: self.requested_model,
            active_model: self.active_model,
            download_progress: self.download_progress,
            model_error: self.model_error.clone(),
            ..crate::ui::AudioState::default()
        };
        let helper_summary = helper_status_short_summary(&helper_state);

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

            let viz_area = main_area;

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

            if let Some(ref error) = model_error {
                // Show model error prominently in red
                let error_text = if error.len() > status_area.width as usize - 4 {
                    format!("ERR: {}...", &error[..status_area.width as usize - 8])
                } else {
                    format!("ERR: {}", error)
                };
                let error_paragraph = Paragraph::new(error_text)
                    .style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD));
                frame.render_widget(error_paragraph, status_area);
            } else if let Some(progress) = download_progress {
                let ratio = progress.clamp(0.0, 1.0) as f64;
                let pct = (ratio * 100.0) as u16;
                let gauge = Gauge::default()
                    .block(Block::default())
                    .gauge_style(Style::default().fg(Color::Cyan).bg(Color::DarkGray))
                    .ratio(ratio)
                    .label(format!("Downloading model... {}%", pct));
                frame.render_widget(gauge, status_area);
            } else {
                let status_widget = StatusWidget::new(status_info, self.tag.clone())
                    .paused(is_paused)
                    .speaking(is_speaking)
                    .capability(self.transcription_available, self.injection_enabled)
                    .helper_summary(Some(helper_summary.clone()));
                frame.render_widget(status_widget, status_area);
            }

            let transcript_widget = TranscriptWidget::new(
                &committed_text,
                &partial_text,
                theme,
                transcript_area.width as usize,
            );
            frame.render_widget(transcript_widget, transcript_area);

            if let Some(panel_popup) = panel_data {
                let items: Vec<ListItem> = panel_popup
                    .items
                    .iter()
                    .map(|item| {
                        if item.is_section {
                            ListItem::new(Line::from(vec![Span::styled(
                                item.label.as_str(),
                                Style::default()
                                    .fg(Color::Yellow)
                                    .add_modifier(Modifier::BOLD),
                            )]))
                        } else {
                            ListItem::new(Line::raw(item.label.as_str()))
                        }
                    })
                    .collect();

                let mut list_state = ListState::default();
                list_state.select(panel_popup.selected_index);

                let list = List::new(items)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(Line::raw(panel_popup.title).alignment(Alignment::Center)),
                    )
                    .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
                    .highlight_symbol("> ");

                let help_text = format!("{}\n{}", panel_popup.help_title, panel_popup.help_body);
                let help = Paragraph::new(help_text)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(" Focused control "),
                    )
                    .wrap(ratatui::widgets::Wrap { trim: true })
                    .style(Style::default().fg(Color::Gray));

                let popup_height = panel_popup.items.len() as u16 + 8;
                let popup_area = centered_rect(70, popup_height, frame.area());
                let popup_layout = Layout::vertical([
                    Constraint::Length(panel_popup.items.len() as u16 + 2),
                    Constraint::Length(5),
                ])
                .split(popup_area);
                frame.render_widget(Clear, popup_area);
                frame.render_stateful_widget(list, popup_layout[0], &mut list_state);
                frame.render_widget(help, popup_layout[1]);
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
        assert_eq!(
            panel_title(LayoutMode::Full),
            " Input Helper (Up/Dn/Enter/Esc) "
        );
        assert_eq!(panel_title(LayoutMode::Compact), " Input Helper ");
        assert_eq!(panel_title(LayoutMode::Minimal), " ... ");
    }

    #[test]
    fn test_mouse_action_activates_clicked_control() {
        let config = TerminalConfig {
            width: 48,
            height: 6,
            requested_width: None,
            requested_height: None,
            mode: TerminalMode::BarMeter,
            use_color: true,
            use_unicode: false,
            term_width: 80,
            term_height: 24,
        };
        let visualizer = TerminalVisualizer::new(config, None);
        let mut panel = ControlPanelState::new();
        panel.open_for_surface(false);

        let (_, list_area, _) = visualizer.panel_popup_geometry(&panel).unwrap();
        let click_column = list_area.x + 3;
        let click_row = list_area.y + 2;

        assert_eq!(
            visualizer.mouse_action_for_panel(&panel, click_column, click_row, false, false),
            TerminalMouseAction::Activate(Control::DeviceSelector)
        );
    }

    #[test]
    fn test_mouse_action_closes_when_clicking_outside_popup() {
        let config = TerminalConfig {
            width: 48,
            height: 6,
            requested_width: None,
            requested_height: None,
            mode: TerminalMode::BarMeter,
            use_color: true,
            use_unicode: false,
            term_width: 80,
            term_height: 24,
        };
        let visualizer = TerminalVisualizer::new(config, None);
        let mut panel = ControlPanelState::new();
        panel.open_for_surface(false);

        assert_eq!(
            visualizer.mouse_action_for_panel(&panel, 0, 0, false, false),
            TerminalMouseAction::ClosePanel
        );
    }

    #[test]
    fn test_mouse_action_scroll_moves_focus() {
        let config = TerminalConfig {
            width: 48,
            height: 6,
            requested_width: None,
            requested_height: None,
            mode: TerminalMode::BarMeter,
            use_color: true,
            use_unicode: false,
            term_width: 80,
            term_height: 24,
        };
        let visualizer = TerminalVisualizer::new(config, None);
        let mut panel = ControlPanelState::new();
        panel.open_for_surface(false);

        assert_eq!(
            visualizer.mouse_action_for_panel(&panel, 10, 10, true, false),
            TerminalMouseAction::FocusPrevious
        );
        assert_eq!(
            visualizer.mouse_action_for_panel(&panel, 10, 10, false, true),
            TerminalMouseAction::FocusNext
        );
    }

    #[test]
    fn test_resize_updates_dimensions() {
        let config = TerminalConfig {
            width: 48,
            height: 6,
            requested_width: None,
            requested_height: None,
            mode: TerminalMode::BarMeter,
            use_color: true,
            use_unicode: false,
            term_width: 80,
            term_height: 24,
        };
        let mut visualizer = TerminalVisualizer::new(config, None);

        // Resize from 80x24 to 120x40
        visualizer.resize(120, 40);

        // Verify config was updated
        assert_eq!(visualizer.config.term_width, 120);
        assert_eq!(visualizer.config.term_height, 40);
        assert_eq!(visualizer.config.width, 120);
        assert_eq!(visualizer.config.height, 6);
    }

    #[test]
    fn test_resize_noop_when_same_size() {
        let config = TerminalConfig {
            width: 48,
            height: 6,
            requested_width: None,
            requested_height: None,
            mode: TerminalMode::BarMeter,
            use_color: true,
            use_unicode: false,
            term_width: 80,
            term_height: 24,
        };
        let mut visualizer = TerminalVisualizer::new(config, None);

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
            requested_width: None,
            requested_height: None,
            mode: TerminalMode::BarMeter,
            use_color: true,
            use_unicode: false,
            term_width: 80,
            term_height: 24,
        };
        let mut visualizer = TerminalVisualizer::new(config, None);

        // Resize to degenerate dimensions (20x6 is below threshold: 25 cols or 8 rows)
        visualizer.resize(20, 6);

        // Verify layout_mode() returns Degenerate
        assert_eq!(visualizer.layout_mode(), LayoutMode::Degenerate);
    }

    #[test]
    fn test_waterfall_mode_uses_surface_height_for_band_count() {
        let config = TerminalConfig {
            width: 48,
            height: 6,
            requested_width: None,
            requested_height: None,
            mode: TerminalMode::Waterfall,
            use_color: true,
            use_unicode: false,
            term_width: 80,
            term_height: 24,
        };
        let visualizer = TerminalVisualizer::new(config, None);

        assert_eq!(visualizer.analyzer.config().num_bands, 22);
    }

    #[test]
    fn test_waterfall_resize_updates_band_count_with_surface_height() {
        let config = TerminalConfig {
            width: 48,
            height: 6,
            requested_width: None,
            requested_height: None,
            mode: TerminalMode::Waterfall,
            use_color: true,
            use_unicode: false,
            term_width: 80,
            term_height: 24,
        };
        let mut visualizer = TerminalVisualizer::new(config, None);

        visualizer.resize(120, 40);

        assert_eq!(visualizer.analyzer.config().num_bands, 38);
    }

    #[test]
    fn test_resize_reclamps_explicit_requested_dimensions() {
        let config = TerminalConfig {
            width: 48,
            height: 20,
            requested_width: Some(48),
            requested_height: Some(20),
            mode: TerminalMode::BarMeter,
            use_color: true,
            use_unicode: false,
            term_width: 80,
            term_height: 24,
        };
        let mut visualizer = TerminalVisualizer::new(config, None);

        visualizer.resize(30, 12);

        assert_eq!(visualizer.config.width, 30);
        assert_eq!(visualizer.config.height, 8);

        visualizer.resize(90, 40);

        assert_eq!(visualizer.config.width, 48);
        assert_eq!(visualizer.config.height, 20);
    }

    #[test]
    fn test_clamp_terminal_surface_dimensions_tracks_terminal_defaults() {
        assert_eq!(
            clamp_terminal_surface_dimensions(None, None, 120, 40),
            (120, 6)
        );
        assert_eq!(
            clamp_terminal_surface_dimensions(None, None, 20, 4),
            (20, 1)
        );
    }
}
