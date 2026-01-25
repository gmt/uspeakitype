//! Control panel state management
//!
//! Manages the state for the 7 control panel controls:
//! - Device selector
//! - Gain slider
//! - AGC checkbox
//! - Pause button
//! - Viz mode toggle
//! - Color scheme picker
//! - Input injection toggle

use crate::audio::CaptureControl;
use crate::spectrum::{get_color_scheme, ColorScheme};
use crate::ui::spectrogram::{Spectrogram, SpectrogramMode};
use crate::ui::{AudioSourceInfo, AudioState};

/// The 7 control panel controls
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    DeviceSelector,
    GainSlider,
    AgcCheckbox,
    PauseButton,
    VizToggle,
    ColorPicker,
    InjectionToggle,
}

impl Control {
    /// All 7 controls in order (used for navigation and rendering)
    pub const ALL: &'static [Control] = &[
        Control::DeviceSelector,
        Control::GainSlider,
        Control::AgcCheckbox,
        Control::PauseButton,
        Control::VizToggle,
        Control::ColorPicker,
        Control::InjectionToggle,
    ];
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
        }
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

    /// Apply device selection to AudioState
    /// Note: Actual device switching requires audio system restart (out of scope)
    /// This only updates the selected_source_id field in AudioState
    pub fn apply_device(&self, audio_state: &mut AudioState) {
        audio_state.selected_source_id = self.selected_device;
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
        state.set_device(Some(42));

        let mut audio_state = AudioState::new();
        state.apply_device(&mut audio_state);

        assert_eq!(audio_state.selected_source_id, Some(42));
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
    fn control_all_has_seven_controls() {
        assert_eq!(Control::ALL.len(), 7);
    }
}
