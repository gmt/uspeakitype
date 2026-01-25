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
use std::sync::Arc;

pub use app::run;
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
}
