//! Control panel state management
//!
//! Manages the state for the 6 control panel controls:
//! - Device selector
//! - Gain slider
//! - AGC checkbox
//! - Pause button
//! - Viz mode toggle
//! - Color scheme picker

use crate::ui::spectrogram::SpectrogramMode;
use crate::ui::AudioSourceInfo;

/// The 6 control panel controls
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    DeviceSelector,
    GainSlider,
    AgcCheckbox,
    PauseButton,
    VizToggle,
    ColorPicker,
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
}
