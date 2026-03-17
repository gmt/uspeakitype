use std::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};
use std::sync::Arc;
use std::sync::OnceLock;

use parking_lot::Mutex;

use crate::capture::CaptureControl;

pub(crate) const CONTROL_COUNT: usize = 4;
pub(crate) const SOURCE_LABEL_MAX: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum VizMode {
    Bars = 0,
    Waterfall = 1,
}

impl VizMode {
    pub(crate) fn toggle(self) -> Self {
        match self {
            Self::Bars => Self::Waterfall,
            Self::Waterfall => Self::Bars,
        }
    }

    pub(crate) fn from_u8(value: u8) -> Self {
        match value {
            1 => Self::Waterfall,
            _ => Self::Bars,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ControlId {
    Pause,
    AutoGain,
    Gain,
    VizMode,
}

impl ControlId {
    pub(crate) fn all() -> [Self; CONTROL_COUNT] {
        [Self::Pause, Self::AutoGain, Self::Gain, Self::VizMode]
    }

    pub(crate) fn from_index(index: usize) -> Self {
        Self::all()[index.min(CONTROL_COUNT - 1)]
    }

    pub(crate) fn title(self) -> &'static str {
        match self {
            Self::Pause => "Capture pause",
            Self::AutoGain => "Auto gain",
            Self::Gain => "Manual gain",
            Self::VizMode => "Viz mode",
        }
    }

    pub(crate) fn help(self) -> &'static str {
        match self {
            Self::Pause => "Pause audio ingestion without tearing down the stream.",
            Self::AutoGain => "Let the capture path chase speech loudness automatically.",
            Self::Gain => "Set manual software gain when auto gain is disabled.",
            Self::VizMode => "Switch between the faster bar meter and the richer waterfall view.",
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct QtControlSnapshot {
    pub panel_open: u8,
    pub selected_index: u32,
    pub paused: u8,
    pub auto_gain_enabled: u8,
    pub viz_mode: u8,
    pub manual_gain: f32,
    pub current_gain: f32,
    pub source_label: [u8; SOURCE_LABEL_MAX],
}

impl Default for QtControlSnapshot {
    fn default() -> Self {
        Self {
            panel_open: 0,
            selected_index: 0,
            paused: 0,
            auto_gain_enabled: 0,
            viz_mode: VizMode::Bars as u8,
            manual_gain: 1.0,
            current_gain: 1.0,
            source_label: [0; SOURCE_LABEL_MAX],
        }
    }
}

pub(crate) struct RuntimeControls {
    capture: Arc<CaptureControl>,
    panel_open: AtomicBool,
    selected_index: AtomicUsize,
    viz_mode: AtomicU8,
    source_label: Mutex<String>,
}

impl RuntimeControls {
    pub(crate) fn new(source_label: String, auto_gain: bool, gain: f32) -> Arc<Self> {
        let capture = Arc::new(CaptureControl::new());
        capture.set_auto_gain(auto_gain);
        capture.set_manual_gain(gain);
        capture.set_current_gain(gain);
        Arc::new(Self {
            capture,
            panel_open: AtomicBool::new(false),
            selected_index: AtomicUsize::new(0),
            viz_mode: AtomicU8::new(VizMode::Bars as u8),
            source_label: Mutex::new(source_label),
        })
    }

    pub(crate) fn capture(&self) -> Arc<CaptureControl> {
        self.capture.clone()
    }

    pub(crate) fn is_open(&self) -> bool {
        self.panel_open.load(Ordering::Relaxed)
    }

    pub(crate) fn toggle_panel(&self) -> bool {
        let was_open = self.panel_open.fetch_xor(true, Ordering::Relaxed);
        !was_open
    }

    pub(crate) fn close_panel(&self) {
        self.panel_open.store(false, Ordering::Relaxed);
    }

    pub(crate) fn selected_index(&self) -> usize {
        self.selected_index
            .load(Ordering::Relaxed)
            .min(CONTROL_COUNT - 1)
    }

    pub(crate) fn selected_control(&self) -> ControlId {
        ControlId::from_index(self.selected_index())
    }

    pub(crate) fn focus_next(&self) {
        let next = (self.selected_index() + 1) % CONTROL_COUNT;
        self.selected_index.store(next, Ordering::Relaxed);
    }

    pub(crate) fn focus_previous(&self) {
        let current = self.selected_index();
        let previous = if current == 0 {
            CONTROL_COUNT - 1
        } else {
            current - 1
        };
        self.selected_index.store(previous, Ordering::Relaxed);
    }

    pub(crate) fn activate_selected(&self) {
        match self.selected_control() {
            ControlId::Pause => {
                self.capture.toggle_pause();
            }
            ControlId::AutoGain => {
                let enabled = !self.capture.is_auto_gain_enabled();
                self.capture.set_auto_gain(enabled);
                if enabled {
                    let gain = self.capture.get_current_gain();
                    self.capture.set_current_gain(gain);
                } else {
                    let gain = self.capture.get_manual_gain();
                    self.capture.set_current_gain(gain);
                }
            }
            ControlId::Gain => {}
            ControlId::VizMode => {
                self.toggle_viz_mode();
            }
        }
    }

    pub(crate) fn adjust_selected(&self, direction: i32) {
        if !matches!(self.selected_control(), ControlId::Gain) || direction == 0 {
            return;
        }

        let step = 0.1 * direction as f32;
        let next = (self.capture.get_manual_gain() + step).clamp(0.1, 10.0);
        self.capture.set_manual_gain(next);
        if !self.capture.is_auto_gain_enabled() {
            self.capture.set_current_gain(next);
        }
    }

    pub(crate) fn source_label(&self) -> String {
        self.source_label.lock().clone()
    }

    pub(crate) fn viz_mode(&self) -> VizMode {
        VizMode::from_u8(self.viz_mode.load(Ordering::Relaxed))
    }

    pub(crate) fn toggle_viz_mode(&self) {
        let next = self.viz_mode().toggle();
        self.viz_mode.store(next as u8, Ordering::Relaxed);
    }

    pub(crate) fn snapshot(&self) -> RuntimeControlSnapshot {
        RuntimeControlSnapshot {
            panel_open: self.is_open(),
            selected_control: self.selected_control(),
            paused: self.capture.is_paused(),
            auto_gain_enabled: self.capture.is_auto_gain_enabled(),
            viz_mode: self.viz_mode(),
            manual_gain: self.capture.get_manual_gain(),
            current_gain: self.capture.get_current_gain(),
            source_label: self.source_label(),
        }
    }

    pub(crate) fn qt_snapshot(&self) -> QtControlSnapshot {
        let snapshot = self.snapshot();
        let mut qt = QtControlSnapshot {
            panel_open: snapshot.panel_open as u8,
            selected_index: self.selected_index() as u32,
            paused: snapshot.paused as u8,
            auto_gain_enabled: snapshot.auto_gain_enabled as u8,
            viz_mode: snapshot.viz_mode as u8,
            manual_gain: snapshot.manual_gain,
            current_gain: snapshot.current_gain,
            source_label: [0; SOURCE_LABEL_MAX],
        };
        let bytes = snapshot.source_label.as_bytes();
        let copy_len = bytes.len().min(SOURCE_LABEL_MAX.saturating_sub(1));
        qt.source_label[..copy_len].copy_from_slice(&bytes[..copy_len]);
        qt
    }
}

pub(crate) struct RuntimeControlSnapshot {
    pub panel_open: bool,
    pub selected_control: ControlId,
    pub paused: bool,
    pub auto_gain_enabled: bool,
    pub viz_mode: VizMode,
    pub manual_gain: f32,
    pub current_gain: f32,
    pub source_label: String,
}

static QT_CONTROLS: OnceLock<Arc<RuntimeControls>> = OnceLock::new();

pub(crate) fn install_qt_controls(controls: Arc<RuntimeControls>) {
    let _ = QT_CONTROLS.set(controls);
}

fn with_qt_controls<F>(f: F)
where
    F: FnOnce(&RuntimeControls),
{
    if let Some(controls) = QT_CONTROLS.get() {
        f(controls);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn usit_qt_toggle_controls() {
    with_qt_controls(|controls| {
        controls.toggle_panel();
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn usit_qt_focus_next_control() {
    with_qt_controls(|controls| {
        controls.focus_next();
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn usit_qt_focus_previous_control() {
    with_qt_controls(|controls| {
        controls.focus_previous();
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn usit_qt_activate_control() {
    with_qt_controls(|controls| {
        controls.activate_selected();
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn usit_qt_adjust_control(direction: i32) {
    with_qt_controls(|controls| {
        controls.adjust_selected(direction);
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn usit_qt_get_control_snapshot(out: *mut QtControlSnapshot) {
    if out.is_null() {
        return;
    }

    if let Some(controls) = QT_CONTROLS.get() {
        unsafe {
            *out = controls.qt_snapshot();
        }
    } else {
        unsafe {
            *out = QtControlSnapshot::default();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{RuntimeControls, VizMode};

    #[test]
    fn gain_adjustment_updates_manual_gain() {
        let controls = RuntimeControls::new("requested source: default".to_string(), false, 1.0);
        controls.focus_next();
        controls.focus_next();
        controls.adjust_selected(2);
        assert!((controls.capture().get_manual_gain() - 1.2).abs() < 0.001);
        assert!((controls.capture().get_current_gain() - 1.2).abs() < 0.001);
    }

    #[test]
    fn activating_selected_toggle_changes_live_state() {
        let controls = RuntimeControls::new("requested source: default".to_string(), false, 1.0);
        controls.activate_selected();
        assert!(controls.capture().is_paused());
        controls.focus_next();
        controls.activate_selected();
        assert!(controls.capture().is_auto_gain_enabled());
    }

    #[test]
    fn activating_viz_mode_toggles_waterfall() {
        let controls = RuntimeControls::new("requested source: default".to_string(), false, 1.0);
        controls.focus_next();
        controls.focus_next();
        controls.focus_next();
        controls.activate_selected();
        assert_eq!(controls.viz_mode(), VizMode::Waterfall);
        controls.activate_selected();
        assert_eq!(controls.viz_mode(), VizMode::Bars);
    }
}
