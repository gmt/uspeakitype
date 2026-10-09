//! Streaming loop coordinator - bridges VAD, audio buffering, and incremental transcription
//!
//! Key insight: Pre-roll buffer is always active (even during silence) to capture speech onset.
//! VAD transitions trigger commit events, not batching.

use std::collections::VecDeque;

use anyhow::Result;

use crate::audio::SileroVad;
use crate::backend::MoonshineStreamer;
use crate::config::AsrModelId;

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
    /// A held segment this long without a pause is split at its quietest point.
    pub max_segment_samples: usize,
    /// Audio kept from before the VAD hears a held segment begin.
    pub hold_pre_roll_samples: usize,
}

impl Default for StreamingConfig {
    fn default() -> Self {
        Self {
            pre_roll_samples: 2560,
            update_interval_samples: 3200,
            max_segment_samples: 16_000 * 20,
            hold_pre_roll_samples: 8_000,
        }
    }
}

/// VAD decisions inside a hold are made per Silero frame.
const VAD_SLICE_SAMPLES: usize = 512;
/// Forced splits look this far back for a quiet place to cut.
const SPLIT_SEARCH_SAMPLES: usize = 16_000 * 3;
const SPLIT_WINDOW_SAMPLES: usize = 1_600;

/// Middle of the quietest window in the last `search` samples, where a long
/// segment can be cut with the least chance of splitting a word.
fn quietest_split(samples: &[f32], search: usize, window: usize) -> usize {
    let window = window.max(1);
    let start = samples.len().saturating_sub(search);
    let mut best = (f32::INFINITY, samples.len());
    let mut at = start;
    while at + window <= samples.len() {
        let energy: f32 = samples[at..at + window].iter().map(|s| s * s).sum();
        if energy < best.0 {
            best = (energy, at + window / 2);
        }
        at += (window / 2).max(1);
    }
    best.1
}

impl StreamingConfig {
    /// Parakeet re-encodes a context window for every live update, so it refreshes
    /// the preview less often to stay well inside real time on consumer CPUs.
    pub fn for_model(model: AsrModelId) -> Self {
        if model.is_moonshine() {
            Self::default()
        } else {
            Self {
                update_interval_samples: 6400,
                ..Self::default()
            }
        }
    }
}

/// Trait for VAD processing (allows mocking in tests)
pub trait VadProcessor {
    fn process(&mut self, samples: &[f32]) -> Result<()>;
    fn is_speaking(&self) -> bool;
    fn reset(&mut self) {}
}

/// Trait for transcription (allows mocking in tests)
pub trait Transcriber {
    fn transcribe_incremental(&mut self, samples: &[f32]) -> Result<String>;
    /// Text for a finished phrase. Backends whose streaming windows trade
    /// accuracy for latency may decode the whole phrase again here.
    fn transcribe_final(&mut self, samples: &[f32]) -> Result<String> {
        self.transcribe_incremental(samples)
    }
    fn reset(&mut self);
}

/// Boxed transcriber used by production for model hot-swap across families.
pub type BoxedTranscriber = Box<dyn Transcriber + Send>;

impl<T: Transcriber + ?Sized> Transcriber for Box<T> {
    fn transcribe_incremental(&mut self, samples: &[f32]) -> Result<String> {
        (**self).transcribe_incremental(samples)
    }

    fn transcribe_final(&mut self, samples: &[f32]) -> Result<String> {
        (**self).transcribe_final(samples)
    }

    fn reset(&mut self) {
        (**self).reset();
    }
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
    ptt_active: bool,
    /// The hold has already committed a segment at a pause or split.
    segmented: bool,
    /// The VAD heard speech in the current segment.
    segment_speech: bool,
    /// Audio waits in pre-roll until the VAD hears a segment begin.
    awaiting_speech: bool,
    /// Everything held before the first onset, decoded at release if the VAD
    /// never hears speech, so soft speech is not lost to a cautious VAD.
    unheard: Vec<f32>,
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
            ptt_active: false,
            segmented: false,
            segment_speech: false,
            awaiting_speech: false,
            unheard: Vec::new(),
        }
    }

    /// Replace the transcriber with a new instance and reset all internal state.
    ///
    /// Model hot-swap discards the previous utterance and its VAD context.
    pub fn swap_transcriber(&mut self, new_transcriber: T) {
        self.transcriber = new_transcriber;
        self.reset();
    }

    /// Discard all state, including VAD context and pre-roll, across capture sessions.
    pub fn reset(&mut self) {
        self.vad.reset();
        self.transcriber.reset();
        self.pre_roll.clear();
        self.transcription_buffer.clear();
        self.samples_since_update = 0;
        self.was_speaking = false;
        self.ptt_active = false;
        self.segmented = false;
        self.segment_speech = false;
        self.awaiting_speech = false;
        self.unheard.clear();
    }

    pub fn is_speaking(&self) -> bool {
        self.ptt_active && !self.transcription_buffer.is_empty() || self.vad.is_speaking()
    }

    /// A held key defines one utterance independently of silence detection.
    pub fn begin_push_to_talk(&mut self) {
        self.reset();
        self.ptt_active = true;
        self.awaiting_speech = true;
    }

    /// Long holds are committed in segments: at each pause the VAD hears, and at
    /// the quietest point of a segment that reaches `max_segment_samples`. Short
    /// segments keep every decode fast and inside what the models handle well.
    pub fn process_push_to_talk(&mut self, samples: &[f32]) -> Result<Vec<StreamEvent>> {
        if !self.ptt_active {
            return Ok(Vec::new());
        }
        let mut events = Vec::new();
        for slice in samples.chunks(VAD_SLICE_SAMPLES) {
            self.vad.process(slice)?;
            let speaking = self.vad.is_speaking();
            let paused = self.was_speaking && !speaking;
            self.was_speaking = speaking;
            if self.awaiting_speech {
                if !speaking {
                    self.pre_roll.extend(slice.iter().copied());
                    while self.pre_roll.len() > self.config.hold_pre_roll_samples {
                        self.pre_roll.pop_front();
                    }
                    if !self.segmented && self.unheard.len() < self.config.max_segment_samples {
                        self.unheard.extend_from_slice(slice);
                        self.samples_since_update += slice.len();
                    }
                    continue;
                }
                self.awaiting_speech = false;
                self.unheard.clear();
                self.transcription_buffer.extend(self.pre_roll.drain(..));
                self.samples_since_update = self.transcription_buffer.len();
            }
            self.transcription_buffer.extend_from_slice(slice);
            self.samples_since_update += slice.len();
            self.segment_speech |= speaking;
            if paused && self.segment_speech {
                events.extend(self.commit_segment(self.transcription_buffer.len())?);
                self.awaiting_speech = true;
            } else if self.transcription_buffer.len() >= self.config.max_segment_samples {
                let cut = quietest_split(
                    &self.transcription_buffer,
                    SPLIT_SEARCH_SAMPLES.min(self.config.max_segment_samples / 2),
                    SPLIT_WINDOW_SAMPLES.min(self.config.max_segment_samples / 8),
                );
                events.extend(self.commit_segment(cut)?);
                self.segment_speech = speaking;
            }
        }
        let previewed = if self.transcription_buffer.is_empty() {
            &self.unheard
        } else {
            &self.transcription_buffer
        };
        if self.samples_since_update >= self.config.update_interval_samples && !previewed.is_empty()
        {
            let text = self.transcriber.transcribe_incremental(previewed)?;
            self.samples_since_update = 0;
            events.push(StreamEvent::Partial(text));
        }
        Ok(events)
    }

    /// Commit the first `cut` samples as a finished segment and keep the rest.
    fn commit_segment(&mut self, cut: usize) -> Result<Vec<StreamEvent>> {
        let rest = self.transcription_buffer.split_off(cut);
        let text = self
            .transcriber
            .transcribe_final(&self.transcription_buffer)?;
        self.transcriber.reset();
        self.transcription_buffer = rest;
        self.samples_since_update = self.transcription_buffer.len();
        self.segment_speech = false;
        self.segmented = true;
        self.pre_roll.clear();
        Ok(if text.trim().is_empty() {
            Vec::new()
        } else {
            vec![StreamEvent::Commit(text)]
        })
    }

    /// Decode every admitted sample, including a tail shorter than the update interval.
    /// Repeated release events are harmless and never commit a second time. After a
    /// segment commit, a tail the VAD never heard speech in is discarded.
    pub fn finish_push_to_talk(&mut self) -> Result<Vec<StreamEvent>> {
        let tail = if !self.transcription_buffer.is_empty() && self.segment_speech {
            &self.transcription_buffer
        } else {
            // Only a hold the VAD never heard falls back to everything it held.
            &self.unheard
        };
        let result = if self.ptt_active && !tail.is_empty() {
            self.transcriber
                .transcribe_final(tail)
                .map(|text| vec![StreamEvent::Commit(text)])
        } else {
            Ok(Vec::new())
        };
        self.reset();
        result
    }

    pub fn process(&mut self, samples: &[f32]) -> Result<Vec<StreamEvent>> {
        let mut events = Vec::new();

        // 1. Run VAD to update internal state
        self.vad.process(samples)?;

        // 2. Detect state transitions
        let is_speaking = self.vad.is_speaking();
        let started_speaking = is_speaking && !self.was_speaking;
        let stopped_speaking = !is_speaking && self.was_speaking;

        // 3. Copy only preceding audio on speech start. The current samples are
        // appended below, once, even when this frame exceeds the pre-roll capacity.
        if started_speaking {
            self.transcription_buffer.clear();
            self.transcription_buffer.extend(self.pre_roll.iter());
        }

        // 4. Maintain pre-roll for the next call (including during silence).
        self.pre_roll.extend(samples.iter().copied());
        while self.pre_roll.len() > self.config.pre_roll_samples {
            self.pre_roll.pop_front();
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
                .transcribe_final(&self.transcription_buffer)?;
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
    fn reset(&mut self) {
        SileroVad::reset(self);
    }
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
pub type DefaultStreamingTranscriber = StreamingTranscriber<SileroVad, BoxedTranscriber>;

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

    #[derive(Default)]
    struct RecordingTranscriber {
        inputs: Vec<Vec<f32>>,
    }

    impl Transcriber for RecordingTranscriber {
        fn transcribe_incremental(&mut self, samples: &[f32]) -> Result<String> {
            self.inputs.push(samples.to_vec());
            Ok(format!("{} samples", samples.len()))
        }

        fn reset(&mut self) {}
    }

    #[test]
    fn ptt_release_flushes_short_tail_once_and_next_hold_starts_clean() {
        let mut streamer = StreamingTranscriber::new(
            MockVad { speaking: false },
            RecordingTranscriber::default(),
            StreamingConfig {
                pre_roll_samples: 3,
                update_interval_samples: 3,
                ..StreamingConfig::default()
            },
        );
        // Idle audio cannot leak into an explicitly held utterance.
        streamer.process(&[99.0, 98.0]).unwrap();
        streamer.begin_push_to_talk();
        assert_eq!(
            streamer.process_push_to_talk(&[1.0, 2.0, 3.0]).unwrap(),
            vec![StreamEvent::Partial("3 samples".into())]
        );
        assert!(streamer.process_push_to_talk(&[4.0]).unwrap().is_empty());
        assert_eq!(
            streamer.finish_push_to_talk().unwrap(),
            vec![StreamEvent::Commit("4 samples".into())]
        );
        assert_eq!(
            streamer.transcriber.inputs.last().unwrap(),
            &[1.0, 2.0, 3.0, 4.0]
        );
        assert!(streamer.finish_push_to_talk().unwrap().is_empty());
        assert!(streamer.process_push_to_talk(&[97.0]).unwrap().is_empty());
        streamer.begin_push_to_talk();
        streamer.process_push_to_talk(&[5.0]).unwrap();
        assert_eq!(
            streamer.finish_push_to_talk().unwrap(),
            vec![StreamEvent::Commit("1 samples".into())]
        );
        assert_eq!(streamer.transcriber.inputs.last().unwrap(), &[5.0]);
        streamer.begin_push_to_talk();
        assert!(streamer.finish_push_to_talk().unwrap().is_empty());
    }

    /// Treats samples above 0.5 as speech, so tests script the VAD with audio.
    #[derive(Default)]
    struct LoudnessVad {
        speaking: bool,
    }

    impl VadProcessor for LoudnessVad {
        fn process(&mut self, samples: &[f32]) -> Result<()> {
            self.speaking = samples.iter().any(|sample| *sample > 0.5);
            Ok(())
        }
        fn is_speaking(&self) -> bool {
            self.speaking
        }
        fn reset(&mut self) {
            self.speaking = false;
        }
    }

    fn held(
        vad: LoudnessVad,
        config: StreamingConfig,
    ) -> StreamingTranscriber<LoudnessVad, RecordingTranscriber> {
        let mut streamer = StreamingTranscriber::new(vad, RecordingTranscriber::default(), config);
        streamer.begin_push_to_talk();
        streamer
    }

    fn commits(events: Vec<StreamEvent>) -> Vec<String> {
        events
            .into_iter()
            .filter_map(|event| match event {
                StreamEvent::Commit(text) => Some(text),
                StreamEvent::Partial(_) => None,
            })
            .collect()
    }

    #[test]
    fn pauses_commit_segments_and_resume_with_pre_roll_on_speech() {
        let config = StreamingConfig {
            hold_pre_roll_samples: 256,
            update_interval_samples: usize::MAX,
            ..StreamingConfig::default()
        };
        let mut streamer = held(LoudnessVad::default(), config);
        let speech = vec![1.0; VAD_SLICE_SAMPLES];
        let quiet = vec![0.1; VAD_SLICE_SAMPLES];
        let mut audio = [speech.clone(), speech.clone(), quiet.clone()].concat();
        assert_eq!(
            commits(streamer.process_push_to_talk(&audio).unwrap()),
            vec![format!("{} samples", 3 * VAD_SLICE_SAMPLES)]
        );
        // Silence after the pause waits in pre-roll and never becomes a segment.
        audio = [quiet.clone(), quiet.clone(), speech.clone()].concat();
        assert!(commits(streamer.process_push_to_talk(&audio).unwrap()).is_empty());
        assert_eq!(
            commits(streamer.finish_push_to_talk().unwrap()),
            vec![format!("{} samples", 256 + VAD_SLICE_SAMPLES)]
        );
        let resumed = streamer.transcriber.inputs.last().unwrap();
        assert!(resumed[..256].iter().all(|sample| *sample == 0.1));
        assert!(resumed[256..].iter().all(|sample| *sample == 1.0));
    }

    #[test]
    fn a_hold_starts_at_speech_onset_with_pre_roll_not_at_key_down() {
        let config = StreamingConfig {
            hold_pre_roll_samples: 100,
            update_interval_samples: usize::MAX,
            ..StreamingConfig::default()
        };
        let mut streamer = held(LoudnessVad::default(), config);
        let audio = [
            vec![0.1; 4 * VAD_SLICE_SAMPLES],
            vec![1.0; VAD_SLICE_SAMPLES],
        ]
        .concat();
        streamer.process_push_to_talk(&audio).unwrap();
        assert_eq!(
            commits(streamer.finish_push_to_talk().unwrap()),
            vec![format!("{} samples", 100 + VAD_SLICE_SAMPLES)]
        );
    }

    #[test]
    fn release_after_a_committed_pause_types_nothing_more() {
        let config = StreamingConfig {
            update_interval_samples: usize::MAX,
            ..StreamingConfig::default()
        };
        let mut streamer = held(LoudnessVad::default(), config);
        let audio = [
            vec![1.0; VAD_SLICE_SAMPLES],
            vec![0.0; 3 * VAD_SLICE_SAMPLES],
        ]
        .concat();
        assert_eq!(
            commits(streamer.process_push_to_talk(&audio).unwrap()).len(),
            1
        );
        assert!(streamer.finish_push_to_talk().unwrap().is_empty());
        // A hold the VAD never heard still commits, as before segmentation.
        let mut quiet = held(LoudnessVad::default(), StreamingConfig::default());
        quiet.process_push_to_talk(&[0.2; 100]).unwrap();
        assert_eq!(
            commits(quiet.finish_push_to_talk().unwrap()),
            vec!["100 samples"]
        );
    }

    #[test]
    fn unbroken_speech_splits_at_its_quietest_point_and_keeps_the_rest() {
        let config = StreamingConfig {
            update_interval_samples: usize::MAX,
            max_segment_samples: 8 * VAD_SLICE_SAMPLES,
            ..StreamingConfig::default()
        };
        let mut streamer = held(LoudnessVad::default(), config);
        let mut audio = vec![1.0; 8 * VAD_SLICE_SAMPLES];
        // A dip the VAD still calls speech: one loud sample keeps each slice "speaking".
        for sample in &mut audio[6 * VAD_SLICE_SAMPLES..7 * VAD_SLICE_SAMPLES] {
            *sample = 0.6;
        }
        let split = commits(streamer.process_push_to_talk(&audio).unwrap());
        assert_eq!(split.len(), 1);
        let cut: usize = split[0].split(' ').next().unwrap().parse().unwrap();
        assert!(
            (6 * VAD_SLICE_SAMPLES..7 * VAD_SLICE_SAMPLES).contains(&cut),
            "{cut}"
        );
        assert_eq!(
            commits(streamer.finish_push_to_talk().unwrap()),
            vec![format!("{} samples", 8 * VAD_SLICE_SAMPLES - cut)]
        );
    }

    #[test]
    fn quietest_split_finds_the_softest_window_in_the_search_span() {
        let mut samples = vec![1.0; 1000];
        samples[700..800].fill(0.0);
        samples[100..200].fill(0.0); // outside the search span
        let cut = quietest_split(&samples, 500, 100);
        assert!((700..800).contains(&cut), "{cut}");
        assert_eq!(quietest_split(&[], 500, 100), 0);
    }

    #[test]
    fn phrase_ends_use_the_final_decode_and_partials_do_not() {
        struct TwoPass;
        impl Transcriber for TwoPass {
            fn transcribe_incremental(&mut self, _: &[f32]) -> Result<String> {
                Ok("streamed".into())
            }
            fn transcribe_final(&mut self, _: &[f32]) -> Result<String> {
                Ok("final".into())
            }
            fn reset(&mut self) {}
        }
        let config = StreamingConfig {
            pre_roll_samples: 1,
            update_interval_samples: 2,
            ..StreamingConfig::default()
        };
        let mut held =
            StreamingTranscriber::new(MockVad { speaking: false }, TwoPass, config.clone());
        held.begin_push_to_talk();
        assert_eq!(
            held.process_push_to_talk(&[1.0, 2.0]).unwrap(),
            vec![StreamEvent::Partial("streamed".into())]
        );
        assert_eq!(
            held.finish_push_to_talk().unwrap(),
            vec![StreamEvent::Commit("final".into())]
        );

        let mut continuous = StreamingTranscriber::new(MockVad { speaking: true }, TwoPass, config);
        assert_eq!(
            continuous.process(&[1.0, 2.0]).unwrap(),
            vec![StreamEvent::Partial("streamed".into())]
        );
        continuous.vad.speaking = false;
        assert_eq!(
            continuous.process(&[0.0]).unwrap(),
            vec![StreamEvent::Commit("final".into())]
        );
    }

    #[test]
    fn parakeet_refreshes_partials_less_often_than_moonshine() {
        let moonshine = StreamingConfig::for_model(AsrModelId::MoonshineBase);
        let parakeet = StreamingConfig::for_model(AsrModelId::ParakeetTdt06bV3);
        assert_eq!(moonshine.update_interval_samples, 3200);
        assert_eq!(parakeet.update_interval_samples, 6400);
        assert_eq!(parakeet.pre_roll_samples, moonshine.pre_roll_samples);
    }

    #[test]
    fn ptt_reset_discards_pending_audio_and_resets_vad_context() {
        struct StatefulVad {
            dirty: bool,
        }
        impl VadProcessor for StatefulVad {
            fn process(&mut self, _: &[f32]) -> Result<()> {
                self.dirty = true;
                Ok(())
            }
            fn is_speaking(&self) -> bool {
                self.dirty
            }
            fn reset(&mut self) {
                self.dirty = false;
            }
        }
        let mut streamer = StreamingTranscriber::new(
            StatefulVad { dirty: true },
            RecordingTranscriber::default(),
            StreamingConfig::default(),
        );
        streamer.begin_push_to_talk();
        assert!(!streamer.vad.dirty);
        streamer.process_push_to_talk(&[1.0, 2.0]).unwrap();
        streamer.reset();
        assert!(streamer.finish_push_to_talk().unwrap().is_empty());
        assert!(streamer.transcriber.inputs.is_empty());
        assert!(streamer.pre_roll.is_empty());
    }

    #[test]
    fn speech_onset_keeps_preceding_audio_and_each_current_sample_once() {
        // Include frames shorter and longer than pre-roll: onset must not be
        // duplicated or truncated to the pre-roll capacity.
        for onset in [vec![4.0, 5.0], vec![4.0, 5.0, 6.0, 7.0, 8.0]] {
            let mut streamer = StreamingTranscriber::new(
                MockVad { speaking: false },
                RecordingTranscriber::default(),
                StreamingConfig {
                    pre_roll_samples: 3,
                    update_interval_samples: 1,
                    ..StreamingConfig::default()
                },
            );
            assert!(streamer.process(&[0.0, 1.0, 2.0, 3.0]).unwrap().is_empty());
            streamer.vad.speaking = true;
            streamer.process(&onset).unwrap();

            let mut expected = vec![1.0, 2.0, 3.0];
            expected.extend_from_slice(&onset);
            assert_eq!(streamer.transcriber.inputs, vec![expected.clone()]);

            // The tail before VAD declares silence belongs to the utterance,
            // including a final frame too short to trigger a partial update.
            streamer.config.update_interval_samples = 100;
            assert!(streamer.process(&[9.0, 10.0]).unwrap().is_empty());
            expected.extend_from_slice(&[9.0, 10.0]);
            streamer.vad.speaking = false;
            let events = streamer.process(&[11.0, 12.0]).unwrap();
            assert_eq!(
                events,
                vec![StreamEvent::Commit(format!("{} samples", expected.len()))]
            );
            assert_eq!(streamer.transcriber.inputs.last(), Some(&expected));
            assert!(streamer.transcription_buffer.is_empty());

            // The next phrase sees the latest preceding audio, not the old
            // utterance buffer, and its onset is also present exactly once.
            streamer.vad.speaking = true;
            streamer.config.update_interval_samples = 1;
            streamer.process(&[13.0, 14.0]).unwrap();
            assert_eq!(
                streamer.transcriber.inputs.last().unwrap(),
                &[10.0, 11.0, 12.0, 13.0, 14.0]
            );
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
