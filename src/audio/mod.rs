//! Audio capture and VAD
//! 
//! Responsibilities:
//! - Capture audio from microphone (portaudio)
//! - Run Silero VAD to detect speech/silence
//! - Signal when to commit (silence detected)
//!
//! Note: VAD here is for COMMIT DETECTION, not batching.
//! We stream continuously, VAD just tells us when a phrase is "done".

pub mod capture;
pub mod vad;

pub use capture::AudioCapture;
pub use vad::SileroVad;
