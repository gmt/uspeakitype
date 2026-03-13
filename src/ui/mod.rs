pub mod app;
pub mod control_panel;
pub mod icon;
pub mod renderer;
pub mod spectrogram;
pub mod spectrogram_widget;
pub mod status_widget;
pub mod terminal;
pub mod text_renderer;
pub mod theme;
pub mod transcript_widget;
pub mod waterfall_widget;

use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

use crate::config::AsrModelId;

pub use app::run;
pub use control_panel::{
    transcript_text_bounds, Control, ControlPanelState, PanelRect, PANEL_MARGIN, PANEL_MAX_WIDTH,
    PANEL_MIN_SIZE, PANEL_PADDING, ROW_HEIGHT, TEXT_PANEL_HEIGHT, TITLE_HEIGHT,
};
pub use status_widget::StatusWidget;
pub use waterfall_widget::WaterfallWidget;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessingState {
    Idle,
    Listening,
    Transcribing,
}

#[derive(Debug, Clone)]
pub struct AudioSourceInfo {
    pub id: u32,
    pub name: String,
    pub description: String,
}

#[derive(Debug)]
pub struct AudioState {
    pub samples: Vec<f32>,
    pub is_speaking: bool,
    pub committed: String,
    pub partial: String,
    pub processing_state: ProcessingState,
    pub is_paused: bool,
    pub auto_gain_enabled: bool,
    pub current_gain: f32,
    pub available_sources: Vec<AudioSourceInfo>,
    pub selected_source_id: Option<u32>,
    pub injection_enabled: bool,
    /// Download progress: None when not downloading, Some(0.0..1.0) during download
    pub download_progress: Option<f32>,
    /// Model/cache error message to display prominently
    pub model_error: Option<String>,
    /// Whether transcription is available (model loaded successfully)
    pub transcription_available: bool,
    /// The model the user last requested (may be downloading or not yet active)
    pub requested_model: Option<AsrModelId>,
    /// The designated driver model currently running (provides transcription while requested downloads)
    pub active_model: Option<AsrModelId>,
    /// Per-model download progress: key is model ID, value is 0.0..1.0 progress
    pub download_progress_by_model: HashMap<AsrModelId, f32>,
}

impl Default for AudioState {
    fn default() -> Self {
        Self {
            samples: Vec::with_capacity(4096),
            is_speaking: false,
            committed: String::new(),
            partial: String::new(),
            processing_state: ProcessingState::Idle,
            is_paused: false,
            auto_gain_enabled: false,
            current_gain: 1.0,
            available_sources: Vec::new(),
            selected_source_id: None,
            injection_enabled: true,
            download_progress: None,
            model_error: None,
            transcription_available: false,
            requested_model: None,
            active_model: None,
            download_progress_by_model: HashMap::new(),
        }
    }
}

impl AudioState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_partial(&mut self, text: String) {
        self.partial = text;
    }

    pub fn commit(&mut self) {
        if !self.partial.is_empty() {
            if !self.committed.is_empty() {
                self.committed.push(' ');
            }
            self.committed.push_str(&self.partial);
            self.partial.clear();
        }
    }

    pub fn update_samples(&mut self, new_samples: &[f32]) {
        self.samples.clear();
        self.samples.extend_from_slice(new_samples);
    }

    pub fn display(&self) -> String {
        if self.partial.is_empty() {
            self.committed.clone()
        } else if self.committed.is_empty() {
            self.partial.clone()
        } else {
            format!("{} {}", self.committed, self.partial)
        }
    }

    pub fn clear(&mut self) {
        self.samples.clear();
        self.committed.clear();
        self.partial.clear();
    }
}

pub type SharedAudioState = Arc<RwLock<AudioState>>;

pub fn new_shared_state() -> SharedAudioState {
    Arc::new(RwLock::new(AudioState::new()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_state_default_is_empty() {
        let state = AudioState::new();
        assert!(state.samples.is_empty());
        assert!(state.committed.is_empty());
        assert!(state.partial.is_empty());
        assert!(!state.is_speaking);
        assert_eq!(state.processing_state, ProcessingState::Idle);
    }

    #[test]
    fn set_partial_updates_partial_text() {
        let mut state = AudioState::new();
        state.set_partial("hello".to_string());
        assert_eq!(state.partial, "hello");
        assert!(state.committed.is_empty());
    }

    #[test]
    fn commit_moves_partial_to_committed() {
        let mut state = AudioState::new();
        state.set_partial("hello".to_string());
        state.commit();
        assert_eq!(state.committed, "hello");
        assert!(state.partial.is_empty());
    }

    #[test]
    fn commit_adds_space_between_phrases() {
        let mut state = AudioState::new();
        state.set_partial("hello".to_string());
        state.commit();
        state.set_partial("world".to_string());
        state.commit();
        assert_eq!(state.committed, "hello world");
    }

    #[test]
    fn commit_empty_partial_is_noop() {
        let mut state = AudioState::new();
        state.committed = "existing".to_string();
        state.commit();
        assert_eq!(state.committed, "existing");
    }

    #[test]
    fn display_shows_only_committed_when_no_partial() {
        let mut state = AudioState::new();
        state.committed = "hello world".to_string();
        assert_eq!(state.display(), "hello world");
    }

    #[test]
    fn display_shows_only_partial_when_no_committed() {
        let mut state = AudioState::new();
        state.partial = "typing".to_string();
        assert_eq!(state.display(), "typing");
    }

    #[test]
    fn display_combines_committed_and_partial() {
        let mut state = AudioState::new();
        state.committed = "hello".to_string();
        state.partial = "world".to_string();
        assert_eq!(state.display(), "hello world");
    }

    #[test]
    fn update_samples_replaces_buffer() {
        let mut state = AudioState::new();
        state.update_samples(&[1.0, 2.0, 3.0]);
        assert_eq!(state.samples, vec![1.0, 2.0, 3.0]);
        state.update_samples(&[4.0, 5.0]);
        assert_eq!(state.samples, vec![4.0, 5.0]);
    }

    #[test]
    fn clear_resets_all_state() {
        let mut state = AudioState::new();
        state.committed = "hello".to_string();
        state.partial = "world".to_string();
        state.samples = vec![1.0, 2.0];
        state.clear();
        assert!(state.committed.is_empty());
        assert!(state.partial.is_empty());
        assert!(state.samples.is_empty());
    }

    #[test]
    fn shared_state_is_thread_safe() {
        let state = new_shared_state();
        let state_clone = state.clone();

        std::thread::spawn(move || {
            state_clone.write().set_partial("from thread".to_string());
        })
        .join()
        .unwrap();

        assert_eq!(state.read().partial, "from thread");
    }

    // --- Async model selection/activation tests ---

    #[test]
    fn default_state_has_no_requested_or_active_model() {
        let state = AudioState::new();
        assert!(state.requested_model.is_none());
        assert!(state.active_model.is_none());
        assert!(state.download_progress_by_model.is_empty());
    }

    #[test]
    fn can_set_requested_model() {
        use crate::config::AsrModelId;

        let mut state = AudioState::new();
        state.requested_model = Some(AsrModelId::MoonshineBase);

        assert_eq!(state.requested_model, Some(AsrModelId::MoonshineBase));
        // Active model remains None until activation
        assert!(state.active_model.is_none());
    }

    #[test]
    fn can_track_active_model_as_designated_driver() {
        use crate::config::AsrModelId;

        let mut state = AudioState::new();
        // Simulate: DD (active) is MoonshineTiny while requested is MoonshineBase
        state.active_model = Some(AsrModelId::MoonshineTiny);
        state.requested_model = Some(AsrModelId::MoonshineBase);

        assert_eq!(state.active_model, Some(AsrModelId::MoonshineTiny));
        assert_eq!(state.requested_model, Some(AsrModelId::MoonshineBase));
        // They can differ while download is in progress
        assert_ne!(state.active_model, state.requested_model);
    }

    #[test]
    fn can_track_per_model_download_progress() {
        use crate::config::AsrModelId;

        let mut state = AudioState::new();
        // Simulate two parallel downloads
        state
            .download_progress_by_model
            .insert(AsrModelId::MoonshineBase, 0.5);
        state
            .download_progress_by_model
            .insert(AsrModelId::ParakeetTdt06bV3, 0.25);

        assert_eq!(
            state
                .download_progress_by_model
                .get(&AsrModelId::MoonshineBase),
            Some(&0.5)
        );
        assert_eq!(
            state
                .download_progress_by_model
                .get(&AsrModelId::ParakeetTdt06bV3),
            Some(&0.25)
        );
        assert_eq!(
            state
                .download_progress_by_model
                .get(&AsrModelId::MoonshineTiny),
            None
        );
    }

    #[test]
    fn requested_model_change_tracks_last_requested() {
        use crate::config::AsrModelId;

        let mut state = AudioState::new();

        // User requests Base
        state.requested_model = Some(AsrModelId::MoonshineBase);
        assert_eq!(state.requested_model, Some(AsrModelId::MoonshineBase));

        // User changes mind, requests Tiny
        state.requested_model = Some(AsrModelId::MoonshineTiny);
        assert_eq!(state.requested_model, Some(AsrModelId::MoonshineTiny));

        // Simulate Base download completes - should NOT auto-activate because it's no longer requested
        // (This logic is in the streaming worker, but state tracking should support it)
        assert_ne!(state.requested_model, Some(AsrModelId::MoonshineBase));
    }

    #[test]
    fn activation_updates_active_model_and_transcription_available() {
        use crate::config::AsrModelId;

        let mut state = AudioState::new();
        state.requested_model = Some(AsrModelId::MoonshineBase);

        // Initially no transcription
        assert!(!state.transcription_available);

        // Simulate model activation
        state.active_model = Some(AsrModelId::MoonshineBase);
        state.transcription_available = true;

        assert!(state.transcription_available);
        assert_eq!(state.active_model, Some(AsrModelId::MoonshineBase));
    }

    #[test]
    fn download_progress_cleared_on_completion() {
        use crate::config::AsrModelId;

        let mut state = AudioState::new();
        state.requested_model = Some(AsrModelId::MoonshineBase);
        state.download_progress = Some(0.75);
        state
            .download_progress_by_model
            .insert(AsrModelId::MoonshineBase, 0.75);

        // Simulate download completion
        state.download_progress = None;
        state
            .download_progress_by_model
            .remove(&AsrModelId::MoonshineBase);

        assert!(state.download_progress.is_none());
        assert!(state.download_progress_by_model.is_empty());
    }

    #[test]
    fn already_active_model_should_not_reactivate_on_download_complete() {
        use crate::config::AsrModelId;

        let mut state = AudioState::new();

        // Simulate: user selects MoonshineBase which is cached
        state.requested_model = Some(AsrModelId::MoonshineBase);
        // Model was activated immediately via model_swap_rx
        state.active_model = Some(AsrModelId::MoonshineBase);
        state.transcription_available = true;

        // When download manager sends Completed event, we should NOT reactivate
        // because active_model already matches completed_model
        let completed_model = AsrModelId::MoonshineBase;
        let should_activate = state.requested_model == Some(completed_model)
            && state.active_model != Some(completed_model);

        assert!(
            !should_activate,
            "Should not reactivate already-active model"
        );
    }

    #[test]
    fn different_requested_model_should_activate_on_download_complete() {
        use crate::config::AsrModelId;

        let mut state = AudioState::new();

        // Simulate: DD is Tiny, user requested Base (downloading)
        state.active_model = Some(AsrModelId::MoonshineTiny);
        state.requested_model = Some(AsrModelId::MoonshineBase);
        state.transcription_available = true;

        // When download manager sends Completed for Base, we SHOULD activate
        let completed_model = AsrModelId::MoonshineBase;
        let should_activate = state.requested_model == Some(completed_model)
            && state.active_model != Some(completed_model);

        assert!(
            should_activate,
            "Should activate newly-downloaded requested model"
        );
    }
}
