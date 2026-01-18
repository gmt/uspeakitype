//! Audio capture and VAD

pub mod capture;
pub mod vad;

pub use capture::{
    list_audio_sources, AgcConfig, AudioCapture, AudioSource, CaptureConfig, CaptureControl,
};
pub use vad::SileroVad;
