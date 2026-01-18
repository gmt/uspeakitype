//! Silero VAD - Voice Activity Detection
//!
//! TODO: Port from sonori/src/silero_audio_processor.rs
//! 
//! Key insight: In barbara, VAD is for COMMIT DETECTION, not batching.
//! - We transcribe continuously as audio arrives
//! - VAD tells us when speaker paused → commit the partial
//! - This is different from sonori where VAD gates when to transcribe

pub struct SileroVad {
    // TODO: ONNX session for silero model
}

pub enum VadEvent {
    /// Speech detected - keep buffering
    Speech,
    /// Silence detected - time to commit
    Silence,
}

impl SileroVad {
    pub fn new(model_path: &std::path::Path) -> anyhow::Result<Self> {
        todo!("Port from sonori")
    }

    pub fn process(&mut self, samples: &[f32]) -> anyhow::Result<VadEvent> {
        todo!()
    }
}
