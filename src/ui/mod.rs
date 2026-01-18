//! WGPU overlay UI with Wayland layer shell
//!
//! TODO: Port from sonori/src/ui/
//! 
//! Key pieces to bring over:
//! - app.rs: winit event loop with layer shell
//! - window.rs: WGPU setup, render pipeline
//! - text_renderer.rs: glyphon text rendering
//!
//! Key changes from sonori:
//! - Simpler state: just partial_text and committed_text
//! - Partial text rendered differently (dimmed/italic)
//! - No spectrogram, buttons, etc (for now)

pub mod app;
pub mod renderer;

/// Text state for the overlay
pub struct TranscriptState {
    /// Committed text (finalized, won't change)
    pub committed: String,
    /// Partial text (may be revised with next inference)
    pub partial: String,
}

impl TranscriptState {
    pub fn new() -> Self {
        Self {
            committed: String::new(),
            partial: String::new(),
        }
    }

    /// Update partial (replaces previous partial)
    pub fn set_partial(&mut self, text: String) {
        self.partial = text;
    }

    /// Commit current partial
    pub fn commit(&mut self) {
        if !self.partial.is_empty() {
            if !self.committed.is_empty() {
                self.committed.push(' ');
            }
            self.committed.push_str(&self.partial);
            self.partial.clear();
        }
    }

    /// Full display text
    pub fn display(&self) -> String {
        if self.partial.is_empty() {
            self.committed.clone()
        } else if self.committed.is_empty() {
            self.partial.clone()
        } else {
            format!("{} {}", self.committed, self.partial)
        }
    }

    /// Clear everything
    pub fn clear(&mut self) {
        self.committed.clear();
        self.partial.clear();
    }
}
