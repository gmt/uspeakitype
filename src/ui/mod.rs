//! WGPU overlay UI with Wayland layer shell

pub mod app;
pub mod renderer;
pub mod spectrogram;
pub mod text_renderer;

use parking_lot::RwLock;
use std::sync::Arc;

pub use app::run;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessingState {
    Idle,
    Listening,
    Transcribing,
}

#[derive(Debug)]
pub struct AudioState {
    pub samples: Vec<f32>,
    pub is_speaking: bool,
    pub committed: String,
    pub partial: String,
    pub processing_state: ProcessingState,
}

impl Default for AudioState {
    fn default() -> Self {
        Self {
            samples: Vec::with_capacity(4096),
            is_speaking: false,
            committed: String::new(),
            partial: String::new(),
            processing_state: ProcessingState::Idle,
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
