//! Moonshine streaming backend
//!
//! Unlike sonori's multi-backend abstraction, we're Moonshine-only.
//! This simplifies everything.
//!
//! Key difference from sonori:
//! - sonori: transcribe(audio_segment) -> complete_text
//! - barbara: stream_transcribe(audio_buffer) -> partial_text (called repeatedly)

pub mod moonshine;

pub use moonshine::MoonshineStreamer;
