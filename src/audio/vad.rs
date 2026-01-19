//! Silero VAD - Voice Activity Detection
//!
//! Key insight: In barbara, VAD is for COMMIT DETECTION, not batching.
//! We transcribe continuously as audio arrives; VAD only decides when to commit.

use std::collections::VecDeque;
use std::path::Path;

use anyhow::{anyhow, ensure, Context, Result};
use ndarray::{Array1, Array2, ArrayD, IxDyn};
use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use ort::value::Tensor;

const SAMPLE_RATE_HZ: i64 = 16000;
const FRAME_SIZE: usize = 512;

#[derive(Debug, Clone)]
pub struct VadConfig {
    pub speech_threshold: f32,
    pub silence_threshold: f32,
    pub min_speech_frames: usize,
    pub min_silence_frames: usize,
}

impl Default for VadConfig {
    fn default() -> Self {
        Self {
            speech_threshold: 0.3,
            silence_threshold: 0.2,
            min_speech_frames: 2,
            min_silence_frames: 20,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VadEvent {
    Speech,
    Silence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VadState {
    Silence,
    PossibleSpeech,
    Speech,
    PossibleSilence,
}

pub struct SileroVad {
    session: Session,
    state_tensor: Tensor<f32>,
    sample_rate_tensor: Tensor<i64>,
    config: VadConfig,
    current_state: VadState,
    frame_counter: usize,
    sample_buffer: VecDeque<f32>,
}

impl SileroVad {
    pub fn new(model_path: &Path, config: VadConfig) -> Result<Self> {
        let session = Session::builder()?
            .with_optimization_level(GraphOptimizationLevel::Level3)?
            .with_intra_threads(1)?
            .with_inter_threads(1)?
            .commit_from_file(model_path)
            .with_context(|| format!("loading silero vad model: {}", model_path.display()))?;

        let state_array = ArrayD::<f32>::zeros(IxDyn(&[2, 1, 128]));
        let state_tensor = Tensor::from_array(state_array).context("creating initial VAD state")?;

        let sample_rate_array = Array1::from_vec(vec![SAMPLE_RATE_HZ]);
        let sample_rate_tensor =
            Tensor::from_array(sample_rate_array).context("creating sample rate tensor")?;

        Ok(Self {
            session,
            state_tensor,
            sample_rate_tensor,
            config,
            current_state: VadState::Silence,
            frame_counter: 0,
            sample_buffer: VecDeque::new(),
        })
    }

    fn calc_speech_prob(&mut self, frame: &[f32]) -> Result<f32> {
        ensure!(
            frame.len() == FRAME_SIZE,
            "silero frame must be exactly {FRAME_SIZE} samples, got {}",
            frame.len()
        );

        let frame_array =
            Array2::from_shape_vec((1, FRAME_SIZE), frame.to_vec()).context("frame shape")?;
        let frame_tensor = Tensor::from_array(frame_array).context("creating frame tensor")?;

        let outputs = self
            .session
            .run(ort::inputs! {
                "input" => frame_tensor,
                "state" => &self.state_tensor,
                "sr" => &self.sample_rate_tensor,
            })
            .context("silero vad inference")?;

        let output = outputs
            .get("output")
            .ok_or_else(|| anyhow!("missing silero vad output"))?;
        let (_, prob_view) = output
            .try_extract_tensor::<f32>()
            .map_err(|e| anyhow!("extracting silero output tensor: {e}"))?;
        ensure!(!prob_view.is_empty(), "silero vad output tensor is empty");
        let prob: f32 = prob_view[0];

        let state_n = outputs
            .get("stateN")
            .ok_or_else(|| anyhow!("missing silero vad stateN"))?;
        let state_array = state_n
            .try_extract_array::<f32>()
            .map_err(|e| anyhow!("extracting silero stateN array: {e}"))?
            .to_owned();
        self.state_tensor = Tensor::from_array(state_array).context("creating next VAD state")?;

        Ok(prob)
    }

    pub fn process(&mut self, samples: &[f32]) -> Result<VadEvent> {
        self.sample_buffer.extend(samples.iter().copied());

        while self.sample_buffer.len() >= FRAME_SIZE {
            let mut frame: Vec<f32> = Vec::with_capacity(FRAME_SIZE);
            for _ in 0..FRAME_SIZE {
                frame.push(
                    self.sample_buffer
                        .pop_front()
                        .expect("len checked in loop condition"),
                );
            }

            let prob = self.calc_speech_prob(&frame)?;
            self.step_state(prob);
        }

        Ok(if self.is_speaking() {
            VadEvent::Speech
        } else {
            VadEvent::Silence
        })
    }

    pub fn is_speaking(&self) -> bool {
        is_speaking_state(self.current_state)
    }

    pub fn reset(&mut self) {
        self.current_state = VadState::Silence;
        self.frame_counter = 0;
        self.sample_buffer.clear();

        let state_array = ArrayD::<f32>::zeros(IxDyn(&[2, 1, 128]));
        if let Ok(tensor) = Tensor::from_array(state_array) {
            self.state_tensor = tensor;
        }
    }

    fn step_state(&mut self, prob: f32) {
        let (next_state, next_counter) =
            step_state(self.current_state, self.frame_counter, prob, &self.config);
        self.current_state = next_state;
        self.frame_counter = next_counter;
    }
}

fn is_speaking_state(state: VadState) -> bool {
    matches!(state, VadState::Speech | VadState::PossibleSilence)
}

fn step_state(
    state: VadState,
    frame_counter: usize,
    prob: f32,
    config: &VadConfig,
) -> (VadState, usize) {
    match state {
        VadState::Silence => {
            if prob > config.speech_threshold {
                (VadState::PossibleSpeech, 1)
            } else {
                (VadState::Silence, 0)
            }
        }
        VadState::PossibleSpeech => {
            if prob > config.speech_threshold {
                let next = frame_counter + 1;
                if next >= config.min_speech_frames {
                    (VadState::Speech, 0)
                } else {
                    (VadState::PossibleSpeech, next)
                }
            } else {
                (VadState::Silence, 0)
            }
        }
        VadState::Speech => {
            if prob < config.silence_threshold {
                (VadState::PossibleSilence, 1)
            } else {
                (VadState::Speech, 0)
            }
        }
        VadState::PossibleSilence => {
            if prob > config.silence_threshold {
                (VadState::Speech, 0)
            } else {
                let next = frame_counter + 1;
                if next >= config.min_silence_frames {
                    (VadState::Silence, 0)
                } else {
                    (VadState::PossibleSilence, next)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vad_state_transitions() {
        let config = VadConfig::default();

        let (s, c) = step_state(VadState::Silence, 0, 0.31, &config);
        assert_eq!(s, VadState::PossibleSpeech);
        assert_eq!(c, 1);

        let (s, c) = step_state(s, c, 0.35, &config);
        assert_eq!(s, VadState::Speech);
        assert_eq!(c, 0);

        let (s, c) = step_state(s, c, 0.19, &config);
        assert_eq!(s, VadState::PossibleSilence);
        assert_eq!(c, 1);

        let mut s2 = s;
        let mut c2 = c;
        for _ in 0..(config.min_silence_frames - 1) {
            let (ns, nc) = step_state(s2, c2, 0.0, &config);
            s2 = ns;
            c2 = nc;
        }
        assert_eq!(s2, VadState::Silence);
        assert_eq!(c2, 0);

        let (s, c) = step_state(VadState::Silence, 0, 0.31, &config);
        let (s, c) = step_state(s, c, 0.0, &config);
        assert_eq!(s, VadState::Silence);
        assert_eq!(c, 0);

        let (s, c) = step_state(VadState::Speech, 0, 0.0, &config);
        assert_eq!(s, VadState::PossibleSilence);
        let (s, c) = step_state(s, c, 0.21, &config);
        assert_eq!(s, VadState::Speech);
        assert_eq!(c, 0);
    }

    #[test]
    fn vad_is_speaking() {
        assert!(!is_speaking_state(VadState::Silence));
        assert!(!is_speaking_state(VadState::PossibleSpeech));
        assert!(is_speaking_state(VadState::Speech));
        assert!(is_speaking_state(VadState::PossibleSilence));
    }
}
