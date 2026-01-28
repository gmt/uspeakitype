//! ASR backends
//!
//! usit supports multiple ONNX-based ASR backends.
//!
//! Key difference from sonori:
//! - sonori: transcribe(audio_segment) -> complete_text
//! - usit: stream_transcribe(audio_buffer) -> partial_text (called repeatedly)

pub mod moonshine;
pub mod nemo_transducer;

pub use moonshine::{init_ort, MoonshineStreamer};
pub use nemo_transducer::NemoTransducerStreamer;
