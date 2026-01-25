//! Streaming loop coordinator - bridges VAD, audio buffering, and incremental transcription
//!
//! Key insight: Pre-roll buffer is always active (even during silence) to capture speech onset.
//! VAD transitions trigger commit events, not batching.

use std::collections::VecDeque;

use anyhow::Result;

use crate::audio::SileroVad;
use crate::backend::MoonshineStreamer;

/// Events emitted by streaming transcription
/// NOTE: This is the CANONICAL StreamEvent. The one in backend/moonshine.rs is unused and will be removed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamEvent {
    /// Partial transcription (may be revised, UI only)
    Partial(String),
    /// Committed transcription (final for this phrase, UI + future app output)
    Commit(String),
}

/// Configuration for streaming transcription
#[derive(Debug, Clone)]
pub struct StreamingConfig {
    pub pre_roll_samples: usize,
    pub update_interval_samples: usize,
}

impl Default for StreamingConfig {
    fn default() -> Self {
        Self {
            pre_roll_samples: 2560,
            update_interval_samples: 3200,
        }
    }
}

/// Trait for VAD processing (allows mocking in tests)
pub trait VadProcessor {
    fn process(&mut self, samples: &[f32]) -> Result<()>;
    fn is_speaking(&self) -> bool;
}

/// Trait for transcription (allows mocking in tests)
pub trait Transcriber {
    fn transcribe_incremental(&mut self, samples: &[f32]) -> Result<String>;
    fn reset(&mut self);
}

/// Streaming transcriber coordinator
pub struct StreamingTranscriber<V: VadProcessor, T: Transcriber> {
    vad: V,
    transcriber: T,
    config: StreamingConfig,
    pre_roll: VecDeque<f32>,
    transcription_buffer: Vec<f32>,
    samples_since_update: usize,
    was_speaking: bool,
}

impl<V: VadProcessor, T: Transcriber> StreamingTranscriber<V, T> {
    pub fn new(vad: V, transcriber: T, config: StreamingConfig) -> Self {
        let pre_roll_capacity = config.pre_roll_samples;
        Self {
            vad,
            transcriber,
            config,
            pre_roll: VecDeque::with_capacity(pre_roll_capacity),
            transcription_buffer: Vec::new(),
            samples_since_update: 0,
            was_speaking: false,
        }
    }

    /// Replace the transcriber with a new instance and reset all internal state.
    ///
    /// Used for model hot-swap: clears buffers, resets counters, and installs
    /// the new transcriber. The VAD is left untouched (same mic, same voice).
    pub fn swap_transcriber(&mut self, new_transcriber: T) {
        self.transcriber = new_transcriber;
        self.transcriber.reset();
        self.pre_roll.clear();
        self.transcription_buffer.clear();
        self.samples_since_update = 0;
        self.was_speaking = false;
    }

    pub fn is_speaking(&self) -> bool {
        self.vad.is_speaking()
    }

    pub fn process(&mut self, samples: &[f32]) -> Result<Vec<StreamEvent>> {
        let mut events = Vec::new();

        // 1. Maintain pre-roll buffer (always active, even during silence)
        self.pre_roll.extend(samples.iter().copied());
        while self.pre_roll.len() > self.config.pre_roll_samples {
            self.pre_roll.pop_front();
        }

        // 2. Run VAD to update internal state
        self.vad.process(samples)?;

        // 3. Detect state transitions
        let is_speaking = self.vad.is_speaking();
        let started_speaking = is_speaking && !self.was_speaking;
        let stopped_speaking = !is_speaking && self.was_speaking;

        // 4. Handle speech start (copy pre-roll to transcription buffer)
        if started_speaking {
            self.transcription_buffer.clear();
            self.transcription_buffer.extend(self.pre_roll.iter());
        }

        // 5. Handle active speech (accumulate + periodic updates)
        if is_speaking {
            self.transcription_buffer.extend_from_slice(samples);
            self.samples_since_update += samples.len();

            if self.samples_since_update >= self.config.update_interval_samples {
                let text = self
                    .transcriber
                    .transcribe_incremental(&self.transcription_buffer)?;
                events.push(StreamEvent::Partial(text));
                self.samples_since_update = 0;
            }
        }

        // 6. Handle speech stop (final transcription + commit)
        if stopped_speaking {
            let text = self
                .transcriber
                .transcribe_incremental(&self.transcription_buffer)?;
            events.push(StreamEvent::Commit(text));
            self.transcription_buffer.clear();
            self.transcriber.reset();
            self.samples_since_update = 0;
        }

        // 7. Update state for next call
        self.was_speaking = is_speaking;

        Ok(events)
    }
}

impl VadProcessor for SileroVad {
    fn process(&mut self, samples: &[f32]) -> Result<()> {
        self.process(samples)?;
        Ok(())
    }

    fn is_speaking(&self) -> bool {
        self.is_speaking()
    }
}

impl Transcriber for MoonshineStreamer {
    fn transcribe_incremental(&mut self, samples: &[f32]) -> Result<String> {
        self.transcribe_incremental(samples)
    }

    fn reset(&mut self) {
        self.reset()
    }
}

/// Production streaming transcriber with real VAD and ASR
pub type DefaultStreamingTranscriber = StreamingTranscriber<SileroVad, MoonshineStreamer>;

#[cfg(test)]
mod tests {
    use super::*;

    // Mock VAD for deterministic testing
    struct MockVad {
        speaking: bool,
    }

    impl VadProcessor for MockVad {
        fn process(&mut self, _samples: &[f32]) -> Result<()> {
            Ok(())
        }
        fn is_speaking(&self) -> bool {
            self.speaking
        }
    }

    // Mock Transcriber for deterministic testing
    struct MockTranscriber {
        call_count: usize,
    }

    impl Transcriber for MockTranscriber {
        fn transcribe_incremental(&mut self, _samples: &[f32]) -> Result<String> {
            self.call_count += 1;
            Ok(format!("transcription_{}", self.call_count))
        }
        fn reset(&mut self) {
            self.call_count = 0;
        }
    }

    #[test]
    fn test_pre_roll_buffer_capacity() {
        // Inject 5000 samples, verify pre_roll never exceeds 2560
        let vad = MockVad { speaking: false };
        let transcriber = MockTranscriber { call_count: 0 };
        let config = StreamingConfig::default();
        let mut streamer = StreamingTranscriber::new(vad, transcriber, config);

        let samples = vec![0.0f32; 5000];
        streamer.process(&samples).unwrap();

        assert_eq!(
            streamer.pre_roll.len(),
            2560,
            "Pre-roll should cap at 2560 samples"
        );
    }

    #[test]
    fn test_partial_event_after_update_interval() {
        // Mock speech for 3200+ samples, verify Partial event emitted
        let vad = MockVad { speaking: true };
        let transcriber = MockTranscriber { call_count: 0 };
        let config = StreamingConfig::default();
        let mut streamer = StreamingTranscriber::new(vad, transcriber, config);

        let samples = vec![0.0f32; 3200];
        let events = streamer.process(&samples).unwrap();

        assert_eq!(events.len(), 1, "Should emit one event");
        assert!(
            matches!(events[0], StreamEvent::Partial(_)),
            "Should be Partial event"
        );
    }

    #[test]
    fn test_commit_event_on_silence() {
        // Transition from speaking to silence, verify Commit event
        let vad = MockVad { speaking: false };
        let transcriber = MockTranscriber { call_count: 0 };
        let config = StreamingConfig::default();
        let mut streamer = StreamingTranscriber::new(vad, transcriber, config);

        // First call: speaking = true
        streamer.was_speaking = true;

        let samples = vec![0.0f32; 512];
        let events = streamer.process(&samples).unwrap();

        assert_eq!(events.len(), 1, "Should emit one event");
        assert!(
            matches!(events[0], StreamEvent::Commit(_)),
            "Should be Commit event"
        );
    }

    #[test]
    fn test_transcription_buffer_cleared_after_commit() {
        // After commit, transcription_buffer should be empty
        let vad = MockVad { speaking: false };
        let transcriber = MockTranscriber { call_count: 0 };
        let config = StreamingConfig::default();
        let mut streamer = StreamingTranscriber::new(vad, transcriber, config);

        // Build up buffer
        streamer.was_speaking = true;
        streamer.transcription_buffer.extend(vec![0.0f32; 1000]);

        // Transition to silence
        let samples = vec![0.0f32; 512];
        streamer.process(&samples).unwrap();

        assert_eq!(
            streamer.transcription_buffer.len(),
            0,
            "Buffer should be cleared after commit"
        );
    }

    #[test]
    fn test_swap_transcriber() {
        let vad = MockVad { speaking: false };
        let transcriber = MockTranscriber { call_count: 0 };
        let config = StreamingConfig::default();
        let mut streamer = StreamingTranscriber::new(vad, transcriber, config);

        // Build up state
        streamer.pre_roll.extend(vec![1.0, 2.0, 3.0]);
        streamer.transcription_buffer.extend(vec![4.0, 5.0]);
        streamer.samples_since_update = 100;
        streamer.was_speaking = true;

        // Swap transcriber
        let new_transcriber = MockTranscriber { call_count: 42 };
        streamer.swap_transcriber(new_transcriber);

        // All state must be cleared
        assert_eq!(streamer.pre_roll.len(), 0);
        assert_eq!(streamer.transcription_buffer.len(), 0);
        assert_eq!(streamer.samples_since_update, 0);
        assert!(!streamer.was_speaking);

        // New transcriber was reset (call_count was 42, reset sets to 0)
        assert_eq!(streamer.transcriber.call_count, 0);
    }
}
