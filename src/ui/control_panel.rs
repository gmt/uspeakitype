//! Control panel state management
//!
//! Manages the state for the 11 control panel controls:
//! - Device selector
//! - Gain slider
//! - AGC checkbox
//! - Pause button
//! - Viz mode toggle
//! - Color scheme picker
//! - Input injection toggle
//! - Model selector
//! - Auto-save toggle
//! - Opacity slider (WGPU only)
//! - Quit button

use crate::audio::CaptureControl;
use crate::config::AsrModelId;
use crate::spectrum::{get_color_scheme, ColorScheme};
use crate::ui::spectrogram::{Spectrogram, SpectrogramMode};
use crate::ui::{helper_capability_label, AudioSourceInfo, AudioState};

// Panel geometry constants
pub const PANEL_MAX_WIDTH: f32 = 460.0;
pub const PANEL_MIN_SIZE: f32 = 140.0; // Minimum dimension to prevent collapse
pub const PANEL_MARGIN: f32 = 20.0; // Margin from window edges
pub const ROW_HEIGHT: f32 = 30.0; // Height per control row
pub const SECTION_HEIGHT: f32 = 22.0; // Height per section heading
pub const SECTION_GAP: f32 = 8.0; // Gap between section blocks
pub const TITLE_HEIGHT: f32 = 32.0; // Height for title row
pub const PANEL_PADDING: f32 = 12.0; // Internal padding (top/bottom/left/right)
pub const HELP_PANEL_HEIGHT: f32 = 84.0; // Height of the contextual help card
pub const HELP_PANEL_GAP: f32 = 12.0; // Gap between controls and help card
pub const TEXT_PANEL_HEIGHT: f32 = 60.0; // Height for text panel at bottom

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlSection {
    Capture,
    Recognition,
    Desktop,
    Session,
}

impl ControlSection {
    pub fn title(self) -> &'static str {
        match self {
            ControlSection::Capture => "Capture",
            ControlSection::Recognition => "Recognition",
            ControlSection::Desktop => "Desktop",
            ControlSection::Session => "Session",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelEntry {
    Section(ControlSection),
    Control(Control),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PanelEntryLayout {
    pub entry: PanelEntry,
    pub y: f32,
    pub height: f32,
}

/// Rectangle representing panel position and size in pixel coordinates.
/// Origin is top-left of window, Y increases downward.
#[derive(Debug, Clone, Copy)]
pub struct PanelRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl PanelRect {
    /// Calculate panel rect for given window dimensions.
    /// Panel is centered with margin, clamped to minimum size.
    pub fn for_window(window_width: f32, window_height: f32) -> Self {
        let raw_height = TITLE_HEIGHT
            + panel_entries(true)
                .iter()
                .enumerate()
                .map(|(index, entry)| match entry {
                    PanelEntry::Section(_) if index == 0 => SECTION_HEIGHT,
                    PanelEntry::Section(_) => SECTION_HEIGHT + SECTION_GAP,
                    PanelEntry::Control(_) => ROW_HEIGHT,
                })
                .sum::<f32>()
            + HELP_PANEL_GAP
            + HELP_PANEL_HEIGHT
            + PANEL_PADDING * 2.0;
        let raw_width = PANEL_MAX_WIDTH;

        // Clamp to window bounds with margin
        let available_width = (window_width - PANEL_MARGIN * 2.0).max(PANEL_MIN_SIZE);
        let available_height = (window_height - PANEL_MARGIN * 2.0).max(PANEL_MIN_SIZE);

        let width = raw_width.min(available_width);
        let height = raw_height.min(available_height);

        // Center in window
        let x = (window_width - width) / 2.0;
        let y = (window_height - height) / 2.0;

        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Y coordinate for title text
    pub fn title_y(&self) -> f32 {
        self.y + PANEL_PADDING
    }

    pub fn content_top(&self) -> f32 {
        self.y + PANEL_PADDING + TITLE_HEIGHT
    }

    pub fn help_rect(&self) -> PanelRect {
        PanelRect {
            x: self.x + PANEL_PADDING,
            y: self.y + self.height - PANEL_PADDING - HELP_PANEL_HEIGHT,
            width: self.width - PANEL_PADDING * 2.0,
            height: HELP_PANEL_HEIGHT,
        }
    }

    pub fn entry_layouts(&self, include_wgpu_only: bool) -> Vec<PanelEntryLayout> {
        let mut y = self.content_top();
        let mut layouts = Vec::new();

        for (index, entry) in panel_entries(include_wgpu_only).into_iter().enumerate() {
            let height = match entry {
                PanelEntry::Section(_) => {
                    if index > 0 {
                        y += SECTION_GAP;
                    }
                    SECTION_HEIGHT
                }
                PanelEntry::Control(_) => ROW_HEIGHT,
            };

            layouts.push(PanelEntryLayout { entry, y, height });
            y += height;
        }

        layouts
    }

    /// Convert Y coordinate to a clicked control, or None if outside a control row.
    pub fn control_at_y(&self, click_y: f32) -> Option<Control> {
        self.entry_layouts(true)
            .into_iter()
            .find_map(|layout| match layout.entry {
                PanelEntry::Control(control)
                    if click_y >= layout.y && click_y < layout.y + layout.height =>
                {
                    Some(control)
                }
                _ => None,
            })
    }

    /// Check if point is inside panel bounds
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.x + self.width && y >= self.y && y < self.y + self.height
    }

    /// Convert pixel rect to NDC for shader uniform
    pub fn to_ndc(&self, window_width: f32, window_height: f32) -> [f32; 4] {
        let ndc_left = (self.x / window_width) * 2.0 - 1.0;
        let ndc_top = 1.0 - (self.y / window_height) * 2.0;
        let ndc_width = (self.width / window_width) * 2.0;
        let ndc_height = (self.height / window_height) * 2.0;
        [ndc_left, ndc_top, ndc_width, ndc_height]
    }
}

/// The 11 control panel controls
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    DeviceSelector,
    GainSlider,
    AgcCheckbox,
    PauseButton,
    VizToggle,
    ColorPicker,
    InjectionToggle,
    ModelSelector,
    AutoSaveToggle,
    OpacitySlider,
    QuitButton,
}

impl Control {
    /// All 11 controls in order (used for navigation and rendering)
    pub const ALL: &'static [Control] = &[
        Control::DeviceSelector,
        Control::GainSlider,
        Control::AgcCheckbox,
        Control::PauseButton,
        Control::VizToggle,
        Control::ColorPicker,
        Control::InjectionToggle,
        Control::ModelSelector,
        Control::AutoSaveToggle,
        Control::OpacitySlider,
        Control::QuitButton,
    ];

    /// Returns true if this control is WGPU-only (not available in TUI)
    pub fn is_wgpu_only(&self) -> bool {
        matches!(self, Control::OpacitySlider)
    }

    pub fn section(self) -> ControlSection {
        match self {
            Control::DeviceSelector
            | Control::GainSlider
            | Control::AgcCheckbox
            | Control::PauseButton => ControlSection::Capture,
            Control::VizToggle | Control::ColorPicker | Control::ModelSelector => {
                ControlSection::Recognition
            }
            Control::InjectionToggle | Control::OpacitySlider => ControlSection::Desktop,
            Control::AutoSaveToggle | Control::QuitButton => ControlSection::Session,
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Control::DeviceSelector => "Input source",
            Control::GainSlider => "Software gain",
            Control::AgcCheckbox => "Auto gain",
            Control::PauseButton => "Capture",
            Control::VizToggle => "Spectrogram mode",
            Control::ColorPicker => "Palette",
            Control::InjectionToggle => "Text injection",
            Control::ModelSelector => "Model",
            Control::AutoSaveToggle => "Auto-save",
            Control::OpacitySlider => "Overlay opacity",
            Control::QuitButton => "Quit",
        }
    }

    pub fn help(self) -> (&'static str, &'static str) {
        match self {
            Control::DeviceSelector => (
                "Capture source",
                "Choose which microphone or PipeWire source usit should prefer on the next launch or capture restart. The current helper keeps this honest instead of pretending it can hot-switch a live PipeWire stream.",
            ),
            Control::GainSlider => (
                "Software gain",
                "Trim the incoming signal before recognition. When auto gain is active this becomes a status readout, because the helper should not fight the user and the algorithm at the same time.",
            ),
            Control::AgcCheckbox => (
                "Automatic gain control",
                "Let usit ride software gain for you when the room or mic discipline is inconsistent. Turn it off when you want exact, manual behavior.",
            ),
            Control::PauseButton => (
                "Capture gate",
                "Pause listening without tearing the rest of the session down. This is the closest thing the current UI has to a lightweight helper standby mode.",
            ),
            Control::VizToggle => (
                "Spectrogram mode",
                "Switch between the faster bar view and the richer waterfall view. This should stay mirrored across surfaces so the app still feels like one instrument.",
            ),
            Control::ColorPicker => (
                "Palette",
                "Choose how the spectrogram heat maps onto the surface. It is cosmetic, but keeping it near the model controls helps the overlay feel more like a coherent helper than a debug HUD.",
            ),
            Control::InjectionToggle => (
                "Text injection",
                "Enable or disable typing into the focused application. This is a trust boundary, so it belongs in its own desktop section rather than hiding among audio controls.",
            ),
            Control::ModelSelector => (
                "Recognition model",
                "Pick the designated driver for recognition and downloads. Longer term this is where user intent, provenance, and learning discipline need to stay explicit.",
            ),
            Control::AutoSaveToggle => (
                "Auto-save",
                "Persist explicit control changes to the config file as they happen. This keeps the GUI, the terminal, and the ML knobs aligned around one source of truth.",
            ),
            Control::OpacitySlider => (
                "Overlay opacity",
                "Adjust how present the graphical helper feels over the desktop. This is WGPU-only for now, so other surfaces treat it as a desktop-local affordance.",
            ),
            Control::QuitButton => (
                "Quit session",
                "Shut the helper down cleanly. Keeping a real exit control in the panel matters because a trusted desktop tool should always advertise how to stop it.",
            ),
        }
    }
}

pub fn panel_entries(include_wgpu_only: bool) -> Vec<PanelEntry> {
    let mut entries = Vec::new();
    let mut active_section = None;

    for control in navigable_controls(include_wgpu_only) {
        let section = control.section();
        if active_section != Some(section) {
            entries.push(PanelEntry::Section(section));
            active_section = Some(section);
        }
        entries.push(PanelEntry::Control(control));
    }

    entries
}

pub fn navigable_controls(include_wgpu_only: bool) -> Vec<Control> {
    Control::ALL
        .iter()
        .copied()
        .filter(|control| include_wgpu_only || !control.is_wgpu_only())
        .collect()
}

pub fn default_help() -> (&'static str, &'static str) {
    (
        "Input helper panel",
        "Keep capture, recognition, desktop trust, and session controls legible. The same structural model should feel natural in both ANSI and Wayland even when the chrome differs.",
    )
}

/// State for the control panel UI
#[derive(Debug, Clone)]
pub struct ControlPanelState {
    pub is_open: bool,
    pub focused_control: Option<Control>,
    pub device_list: Vec<AudioSourceInfo>,
    pub selected_device: Option<u32>,
    pub gain_value: f32, // 0.0 to 2.0
    pub agc_enabled: bool,
    pub is_paused: bool,
    pub viz_mode: SpectrogramMode,
    pub color_scheme_name: &'static str, // "flame", "ice", "mono"
    pub model: AsrModelId,
    pub auto_save: bool,
    pub opacity: f32, // 0.5 to 1.0 (WGPU overlay only)
}

impl Default for ControlPanelState {
    fn default() -> Self {
        Self {
            is_open: false,
            focused_control: None,
            device_list: Vec::new(),
            selected_device: None,
            gain_value: 1.0,
            agc_enabled: false,
            is_paused: false,
            viz_mode: SpectrogramMode::BarMeter,
            color_scheme_name: "flame",
            model: AsrModelId::MoonshineBase,
            auto_save: true,
            opacity: 0.85,
        }
    }
}

/// Generate text bounds for transcript text panel
pub fn transcript_text_bounds(text_rect: &PanelRect) -> glyphon::TextBounds {
    glyphon::TextBounds {
        left: (text_rect.x + PANEL_PADDING) as i32,
        top: (text_rect.y + PANEL_PADDING) as i32,
        right: (text_rect.x + text_rect.width - PANEL_PADDING) as i32,
        bottom: (text_rect.y + text_rect.height - PANEL_PADDING) as i32,
    }
}

impl ControlPanelState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn toggle_open(&mut self) {
        self.is_open = !self.is_open;
    }

    pub fn set_focused(&mut self, control: Option<Control>) {
        self.focused_control = control;
    }

    pub fn open_for_surface(&mut self, include_wgpu_only: bool) {
        self.is_open = true;
        if self.focused_control.is_none() {
            self.focused_control = navigable_controls(include_wgpu_only).into_iter().next();
        }
    }

    pub fn close(&mut self) {
        self.is_open = false;
    }

    pub fn toggle_open_for_surface(&mut self, include_wgpu_only: bool) {
        if self.is_open {
            self.close();
        } else {
            self.open_for_surface(include_wgpu_only);
        }
    }

    pub fn focus_next(&mut self, include_wgpu_only: bool) {
        let controls = navigable_controls(include_wgpu_only);
        if controls.is_empty() {
            self.focused_control = None;
            return;
        }

        let current = self
            .focused_control
            .and_then(|focused| controls.iter().position(|control| *control == focused))
            .unwrap_or(0);
        self.focused_control = Some(controls[(current + 1) % controls.len()]);
    }

    pub fn focus_previous(&mut self, include_wgpu_only: bool) {
        let controls = navigable_controls(include_wgpu_only);
        if controls.is_empty() {
            self.focused_control = None;
            return;
        }

        let current = self
            .focused_control
            .and_then(|focused| controls.iter().position(|control| *control == focused))
            .unwrap_or(0);
        let next = if current == 0 {
            controls.len() - 1
        } else {
            current - 1
        };
        self.focused_control = Some(controls[next]);
    }

    pub fn help_copy(&self) -> (&'static str, &'static str) {
        self.focused_control
            .map(Control::help)
            .unwrap_or_else(default_help)
    }

    pub fn set_gain(&mut self, gain: f32) {
        self.gain_value = gain.clamp(0.0, 2.0);
    }

    pub fn toggle_agc(&mut self) {
        self.agc_enabled = !self.agc_enabled;
    }

    pub fn toggle_pause(&mut self) {
        self.is_paused = !self.is_paused;
    }

    pub fn toggle_viz_mode(&mut self) {
        self.viz_mode = match self.viz_mode {
            SpectrogramMode::BarMeter => SpectrogramMode::Waterfall,
            SpectrogramMode::Waterfall => SpectrogramMode::BarMeter,
        };
    }

    pub fn set_color_scheme(&mut self, name: &'static str) {
        self.color_scheme_name = name;
    }

    pub fn set_device(&mut self, device_id: Option<u32>) {
        self.selected_device = device_id;
    }

    pub fn update_device_list(&mut self, devices: Vec<AudioSourceInfo>) {
        self.device_list = devices;
    }

    pub fn toggle_model(&mut self) {
        self.model = self.model.next();
    }

    pub fn toggle_auto_save(&mut self) {
        self.auto_save = !self.auto_save;
    }

    pub fn adjust_opacity(&mut self) {
        self.opacity = match self.opacity {
            t if t < 0.6 => 0.7,
            t if t < 0.8 => 0.85,
            t if t < 0.95 => 1.0,
            _ => 0.5,
        };
    }

    pub fn control_value(&self, control: Control, audio_state: &AudioState) -> String {
        match control {
            Control::DeviceSelector => self.device_value(audio_state),
            Control::GainSlider => {
                if self.agc_enabled {
                    format!("{:.2}x (auto)", self.gain_value)
                } else {
                    format!("{:.2}x", self.gain_value)
                }
            }
            Control::AgcCheckbox => {
                if self.agc_enabled {
                    "Enabled".to_string()
                } else {
                    "Manual".to_string()
                }
            }
            Control::PauseButton => {
                if self.is_paused {
                    "Standby".to_string()
                } else {
                    "Listening".to_string()
                }
            }
            Control::VizToggle => match self.viz_mode {
                SpectrogramMode::BarMeter => "Bar meter".to_string(),
                SpectrogramMode::Waterfall => "Waterfall".to_string(),
            },
            Control::ColorPicker => self.color_scheme_name.to_string(),
            Control::InjectionToggle => match helper_capability_label(audio_state) {
                "Trusted input" => "Trusted".to_string(),
                _ => "Display-only".to_string(),
            },
            Control::ModelSelector => self.model_value(audio_state),
            Control::AutoSaveToggle => {
                if self.auto_save {
                    "Immediate".to_string()
                } else {
                    "Manual".to_string()
                }
            }
            Control::OpacitySlider => format!("{}%", (self.opacity * 100.0) as u32),
            Control::QuitButton => "Stop helper".to_string(),
        }
    }

    fn device_value(&self, audio_state: &AudioState) -> String {
        let mut label = self
            .selected_device
            .and_then(|id| self.device_list.iter().find(|device| device.id == id))
            .map(|device| {
                if device.description.is_empty() || device.description == device.name {
                    device.name.clone()
                } else {
                    device.description.clone()
                }
            })
            .or_else(|| audio_state.selected_source_name.clone())
            .unwrap_or_else(|| "Default".to_string());

        if audio_state.source_change_pending_restart {
            label.push_str(" (next launch)");
        }

        label
    }

    fn model_value(&self, audio_state: &AudioState) -> String {
        let requested = audio_state.requested_model.unwrap_or(self.model);
        let active = audio_state.active_model;

        if audio_state.download_progress.is_some() && active != Some(requested) {
            match active {
                Some(active_model) => format!("{} -> {} (dl)", active_model, requested),
                None => format!("{} (dl)", requested),
            }
        } else if let Some(active_model) = active {
            if active_model != requested {
                format!("{} -> {}", active_model, requested)
            } else {
                active_model.to_string()
            }
        } else {
            requested.to_string()
        }
    }

    // ===== Apply methods to push state to app components =====

    /// Apply gain change to AudioState
    pub fn apply_gain(&self, audio_state: &mut AudioState) {
        audio_state.current_gain = self.gain_value;
    }

    /// Apply AGC toggle to AudioState
    pub fn apply_agc(&self, audio_state: &mut AudioState) {
        audio_state.auto_gain_enabled = self.agc_enabled;
    }

    /// Apply pause toggle to CaptureControl
    pub fn apply_pause(&self, capture_control: &CaptureControl) {
        if self.is_paused {
            capture_control.pause();
        } else {
            capture_control.resume();
        }
    }

    /// Apply viz mode change to Spectrogram
    pub fn apply_viz_mode(&self, spectrogram: &mut Spectrogram) {
        spectrogram.set_mode(self.viz_mode);
    }

    /// Get the color scheme for the current selection
    /// Returns a boxed ColorScheme that can be set on Spectrogram
    pub fn get_color_scheme(&self) -> Box<dyn ColorScheme> {
        get_color_scheme(self.color_scheme_name)
    }

    /// Apply deferred device intent to shared state.
    ///
    /// This does not hot-swap a live PipeWire stream. It records which source should be preferred
    /// on the next launch or capture restart, then lets config persistence make that intent real.
    pub fn apply_device(&self, audio_state: &mut AudioState) {
        audio_state.selected_source_id = self.selected_device;
        audio_state.selected_source_name = self.selected_device.and_then(|id| {
            self.device_list
                .iter()
                .find(|device| device.id == id)
                .map(|device| device.name.clone())
        });
        audio_state.source_change_pending_restart =
            audio_state.selected_source_name != audio_state.session_source_name;
    }

    pub fn cycle_device(&mut self, audio_state: &mut AudioState) {
        let next_device = match self.selected_device {
            None => self.device_list.first().map(|device| device.id),
            Some(current_id) => self
                .device_list
                .iter()
                .position(|device| device.id == current_id)
                .and_then(|index| self.device_list.get(index + 1))
                .map(|device| device.id),
        };

        self.selected_device = next_device;
        self.apply_device(audio_state);
    }

    /// Toggle injection in AudioState
    pub fn toggle_injection(&self, audio_state: &mut AudioState) {
        audio_state.injection_enabled = !audio_state.injection_enabled;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_panel_default_state() {
        let state = ControlPanelState::new();
        assert!(!state.is_open);
        assert_eq!(state.gain_value, 1.0);
        assert!(!state.agc_enabled);
        assert!(!state.is_paused);
    }

    #[test]
    fn toggle_operations() {
        let mut state = ControlPanelState::new();
        state.toggle_open();
        assert!(state.is_open);
        state.toggle_agc();
        assert!(state.agc_enabled);
        state.toggle_pause();
        assert!(state.is_paused);
    }

    #[test]
    fn gain_clamping() {
        let mut state = ControlPanelState::new();
        state.set_gain(3.0);
        assert_eq!(state.gain_value, 2.0);
        state.set_gain(-1.0);
        assert_eq!(state.gain_value, 0.0);
    }

    #[test]
    fn viz_mode_toggle() {
        let mut state = ControlPanelState::new();
        assert!(matches!(state.viz_mode, SpectrogramMode::BarMeter));
        state.toggle_viz_mode();
        assert!(matches!(state.viz_mode, SpectrogramMode::Waterfall));
        state.toggle_viz_mode();
        assert!(matches!(state.viz_mode, SpectrogramMode::BarMeter));
    }

    #[test]
    fn apply_gain_updates_audio_state() {
        let mut state = ControlPanelState::new();
        state.set_gain(1.5);

        let mut audio_state = AudioState::new();
        state.apply_gain(&mut audio_state);

        assert_eq!(audio_state.current_gain, 1.5);
    }

    #[test]
    fn apply_agc_updates_audio_state() {
        let mut state = ControlPanelState::new();
        state.toggle_agc();

        let mut audio_state = AudioState::new();
        state.apply_agc(&mut audio_state);

        assert!(audio_state.auto_gain_enabled);
    }

    #[test]
    fn apply_pause_calls_capture_control() {
        let state = ControlPanelState::new();
        let control = CaptureControl::new();

        assert!(!control.is_paused());

        let mut paused_state = state.clone();
        paused_state.toggle_pause();
        paused_state.apply_pause(&control);

        assert!(control.is_paused());

        state.apply_pause(&control);
        assert!(!control.is_paused());
    }

    #[test]
    fn get_color_scheme_returns_correct_scheme() {
        let mut state = ControlPanelState::new();

        state.set_color_scheme("flame");
        assert_eq!(state.get_color_scheme().name(), "flame");

        state.set_color_scheme("ice");
        assert_eq!(state.get_color_scheme().name(), "ice");

        state.set_color_scheme("mono");
        assert_eq!(state.get_color_scheme().name(), "mono");
    }

    #[test]
    fn apply_device_updates_audio_state() {
        let mut state = ControlPanelState::new();
        state.update_device_list(vec![AudioSourceInfo {
            id: 42,
            name: "desk-mic".to_string(),
            description: "Desk Mic".to_string(),
        }]);
        state.set_device(Some(42));

        let mut audio_state = AudioState::new();
        audio_state.session_source_name = Some("headset".to_string());
        state.apply_device(&mut audio_state);

        assert_eq!(audio_state.selected_source_id, Some(42));
        assert_eq!(
            audio_state.selected_source_name.as_deref(),
            Some("desk-mic")
        );
        assert!(audio_state.source_change_pending_restart);
    }

    #[test]
    fn toggle_injection_updates_audio_state() {
        let state = ControlPanelState::new();
        let mut audio_state = AudioState::new();

        assert!(audio_state.injection_enabled);
        state.toggle_injection(&mut audio_state);
        assert!(!audio_state.injection_enabled);
        state.toggle_injection(&mut audio_state);
        assert!(audio_state.injection_enabled);
    }

    #[test]
    fn control_all_has_eleven_controls() {
        assert_eq!(Control::ALL.len(), 11);
    }

    #[test]
    fn toggle_model_cycles_correctly() {
        let mut state = ControlPanelState::new();
        assert_eq!(state.model, AsrModelId::MoonshineBase);
        state.toggle_model();
        assert_eq!(state.model, AsrModelId::MoonshineTiny);
        state.toggle_model();
        assert_eq!(state.model, AsrModelId::MoonshineTinyArabic);

        for _ in 0..(AsrModelId::all().len() - 2) {
            state.toggle_model();
        }

        assert_eq!(state.model, AsrModelId::MoonshineBase);
    }

    #[test]
    fn toggle_auto_save_toggles_correctly() {
        let mut state = ControlPanelState::new();
        assert!(state.auto_save);
        state.toggle_auto_save();
        assert!(!state.auto_save);
        state.toggle_auto_save();
        assert!(state.auto_save);
    }

    #[test]
    fn default_values_correct() {
        let state = ControlPanelState::new();
        assert_eq!(state.model, AsrModelId::MoonshineBase);
        assert!(state.auto_save);
    }

    #[test]
    fn focus_navigation_skips_wgpu_only_controls_in_terminal_mode() {
        let mut state = ControlPanelState::new();
        state.open_for_surface(false);
        assert_eq!(state.focused_control, Some(Control::DeviceSelector));

        for _ in 0..9 {
            state.focus_next(false);
        }
        assert_eq!(state.focused_control, Some(Control::QuitButton));
        state.focus_next(false);
        assert_eq!(state.focused_control, Some(Control::DeviceSelector));
    }

    #[test]
    fn panel_entries_insert_section_headers() {
        let entries = panel_entries(false);
        assert_eq!(
            entries.first(),
            Some(&PanelEntry::Section(ControlSection::Capture))
        );
        assert!(entries.contains(&PanelEntry::Section(ControlSection::Recognition)));
        assert!(entries.contains(&PanelEntry::Section(ControlSection::Desktop)));
        assert!(entries.contains(&PanelEntry::Section(ControlSection::Session)));
        assert!(!entries.contains(&PanelEntry::Control(Control::OpacitySlider)));
    }

    #[test]
    fn model_value_surfaces_requested_vs_active_status() {
        let mut panel = ControlPanelState::new();
        panel.model = AsrModelId::MoonshineTiny;

        let mut audio_state = AudioState::new();
        audio_state.active_model = Some(AsrModelId::MoonshineBase);
        audio_state.requested_model = Some(AsrModelId::MoonshineTiny);
        audio_state.download_progress = Some(0.42);

        assert_eq!(
            panel.control_value(Control::ModelSelector, &audio_state),
            "Moonshine Base -> Moonshine Tiny (dl)"
        );
    }

    #[test]
    fn device_selector_help_mentions_next_launch() {
        let (_, body) = Control::DeviceSelector.help();
        assert!(body.contains("next launch"));
    }

    #[test]
    fn device_value_uses_name_and_marks_pending_restart() {
        let mut panel = ControlPanelState::new();
        panel.update_device_list(vec![AudioSourceInfo {
            id: 9,
            name: "usb-mic".to_string(),
            description: "USB Mic".to_string(),
        }]);
        panel.set_device(Some(9));

        let mut audio_state = AudioState::new();
        audio_state.session_source_name = Some("desk-mic".to_string());
        audio_state.source_change_pending_restart = true;

        assert_eq!(
            panel.control_value(Control::DeviceSelector, &audio_state),
            "USB Mic (next launch)"
        );
    }

    #[test]
    fn device_value_clears_next_launch_suffix_when_selection_matches_session() {
        let mut panel = ControlPanelState::new();
        panel.update_device_list(vec![AudioSourceInfo {
            id: 9,
            name: "usb-mic".to_string(),
            description: "USB Mic".to_string(),
        }]);
        panel.set_device(Some(9));

        let mut audio_state = AudioState::new();
        audio_state.session_source_name = Some("usb-mic".to_string());
        audio_state.selected_source_name = Some("usb-mic".to_string());
        panel.apply_device(&mut audio_state);

        assert_eq!(
            panel.control_value(Control::DeviceSelector, &audio_state),
            "USB Mic"
        );
        assert!(!audio_state.source_change_pending_restart);
    }

    #[test]
    fn cycle_device_rotates_to_default_after_last_source() {
        let mut panel = ControlPanelState::new();
        panel.update_device_list(vec![
            AudioSourceInfo {
                id: 1,
                name: "desk-mic".to_string(),
                description: "Desk Mic".to_string(),
            },
            AudioSourceInfo {
                id: 2,
                name: "headset".to_string(),
                description: "USB Headset".to_string(),
            },
        ]);

        let mut audio_state = AudioState::new();
        panel.cycle_device(&mut audio_state);
        assert_eq!(panel.selected_device, Some(1));
        assert_eq!(
            audio_state.selected_source_name.as_deref(),
            Some("desk-mic")
        );

        panel.cycle_device(&mut audio_state);
        assert_eq!(panel.selected_device, Some(2));
        assert_eq!(audio_state.selected_source_name.as_deref(), Some("headset"));

        panel.cycle_device(&mut audio_state);
        assert_eq!(panel.selected_device, None);
        assert!(audio_state.selected_source_name.is_none());
    }
}

#[cfg(test)]
mod panel_rect_tests {
    use super::*;

    #[test]
    fn test_panel_rect_unclamped() {
        let rect = PanelRect::for_window(800.0, 600.0);
        assert_eq!(rect.width, 460.0);
        assert_eq!(rect.height, 560.0); // clamped from the full helper layout
    }

    #[test]
    fn test_panel_rect_width_clamped() {
        let rect = PanelRect::for_window(400.0, 600.0);
        assert_eq!(rect.width, 360.0); // 400 - 2*20
        assert_eq!(rect.height, 560.0);
    }

    #[test]
    fn test_panel_rect_minimum() {
        let rect = PanelRect::for_window(50.0, 50.0);
        assert_eq!(rect.width, 140.0);
        assert_eq!(rect.height, 140.0);
    }

    #[test]
    fn test_control_at_y() {
        let rect = PanelRect::for_window(800.0, 600.0);
        let layouts = rect.entry_layouts(true);
        let first_control = layouts
            .iter()
            .find(|layout| layout.entry == PanelEntry::Control(Control::DeviceSelector))
            .unwrap();
        let injection = layouts
            .iter()
            .find(|layout| layout.entry == PanelEntry::Control(Control::InjectionToggle))
            .unwrap();

        assert_eq!(rect.control_at_y(rect.title_y()), None);
        assert_eq!(
            rect.control_at_y(first_control.y + 1.0),
            Some(Control::DeviceSelector)
        );
        assert_eq!(
            rect.control_at_y(injection.y + 5.0),
            Some(Control::InjectionToggle)
        );
    }
}
