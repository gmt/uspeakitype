//! Audio capture and VAD

pub mod capture;
pub mod vad;

pub use capture::{list_audio_sources, AudioCapture, CaptureConfig, CaptureControl};
pub use vad::{SileroVad, VadConfig, VadEvent};
