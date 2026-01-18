//! Moonshine streaming transcription
//!
//! The core streaming loop:
//! ```text
//! while audio_arriving {
//!     buffer.extend(new_audio);
//!     partial = moonshine.transcribe(&buffer);  // fast! ~30ms for 1s audio
//!     yield Partial(partial);
//!     
//!     if vad.silence() {
//!         yield Commit(partial);
//!         buffer.clear();
//!     }
//! }
//! ```
//!
//! TODO: Port moonshine inference from sonori/src/backend/moonshine/
//! Key pieces:
//! - Model loading (encoder, decoder ONNX sessions)  
//! - Tokenizer
//! - greedy_decode_cached for efficient token-by-token generation

use std::path::Path;

pub struct MoonshineStreamer {
    // TODO: encoder, decoder, tokenizer
}

pub enum StreamEvent {
    /// Partial transcription (may be revised)
    Partial(String),
    /// Committed transcription (final for this phrase)
    Commit(String),
}

impl MoonshineStreamer {
    pub fn new(model_path: impl AsRef<Path>) -> anyhow::Result<Self> {
        todo!("Port from sonori moonshine backend")
    }

    /// Transcribe current buffer, returns partial result
    /// Called frequently as audio grows
    pub fn transcribe(&mut self, samples: &[f32]) -> anyhow::Result<String> {
        todo!()
    }

    /// Reset state for new phrase
    pub fn reset(&mut self) {
        todo!()
    }
}
