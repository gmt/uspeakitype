//! NeMo transducer (RNN-T / TDT) backend (ONNX Runtime)
//!
//! Supports NeMo Conformer-family transducer exports, such as NVIDIA Parakeet TDT 0.6B v3
//! converted to ONNX (e.g. `encoder-model.onnx` + `decoder_joint-model.onnx` + `vocab.txt`).
//!
//! Note: This module focuses on model loading and decoding correctness. Chunked incremental
//! streaming is layered on top by keeping internal decode state across calls.

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use ndarray::{Array1, Array2, Array3, Axis, Ix3};
use ort::session::{builder::GraphOptimizationLevel, Session, SessionInputValue, SessionInputs};
use ort::tensor::TensorElementType;
use ort::value::{Outlet, Tensor, ValueType};
use serde::Deserialize;

use crate::backend::init_ort;
use crate::streaming::Transcriber;

const SAMPLE_RATE_HZ: usize = 16_000;
const HOP_LENGTH_SAMPLES: usize = 160; // 10ms at 16kHz (matches NeMo log-mel preprocessor)

// Defaults tuned for interactive use (low latency) while still providing context.
const DEFAULT_CHUNK_SECS: f32 = 0.8;
const DEFAULT_LEFT_CONTEXT_SECS: f32 = 4.0;
const DEFAULT_RIGHT_CONTEXT_SECS: f32 = 0.2;

/// Result of a single decoder step: (logits, duration, new_state1, new_state2)
type DecodeStepResult = (Vec<f32>, usize, Array3<f32>, Array3<f32>);

#[derive(Debug, Clone)]
struct NemoTransducerConfig {
    features_size: usize,
    subsampling_factor: usize,
    max_tokens_per_step: usize,
}

#[derive(Debug, Deserialize)]
struct NemoConfigFile {
    model_type: Option<String>,
    features_size: Option<usize>,
    subsampling_factor: Option<usize>,
    max_tokens_per_step: Option<usize>,
}

#[derive(Debug, Clone)]
struct PreprocessorIo {
    in_waveforms: String,
    in_waveforms_lens: String,
    out_features: String,
    out_features_lens: String,
}

#[derive(Debug, Clone)]
struct EncoderIo {
    in_audio_signal: String,
    in_length: String,
    out_outputs: String,
    out_encoded_lengths: String,
}

#[derive(Debug, Clone)]
struct DecoderJointIo {
    in_encoder_outputs: String,
    in_targets: String,
    in_targets_ty: TensorElementType,
    in_target_length: String,
    in_target_length_ty: TensorElementType,
    in_state1: String,
    in_state2: String,
    out_outputs: String,
    out_state1: String,
    out_state2: String,
}

/// NeMo transducer backend (TDT) with greedy decoding.
pub struct NemoTransducerStreamer {
    preprocessor: Session,
    encoder: Session,
    decoder_joint: Session,

    pre_io: PreprocessorIo,
    enc_io: EncoderIo,
    dec_io: DecoderJointIo,

    config: NemoTransducerConfig,

    vocab: Vec<String>,
    blank_idx: u32,

    // Decoder recurrent state (carried across incremental decoding).
    state1: Array3<f32>,
    state2: Array3<f32>,
    tokens: Vec<u32>,

    // Chunked streaming bookkeeping (sample indices into the utterance buffer provided by
    // `StreamingTranscriber`).
    chunk_samples: usize,
    left_context_samples: usize,
    right_context_samples: usize,
    processed_samples: usize,
    last_seen_samples: usize,
}

impl NemoTransducerStreamer {
    pub fn new(model_dir: impl AsRef<Path>) -> Result<Self> {
        init_ort();

        let model_dir = model_dir.as_ref().to_path_buf();
        let config_path = model_dir.join("config.json");
        let vocab_path = model_dir.join("vocab.txt");

        let config_file = if config_path.exists() {
            let contents = std::fs::read_to_string(&config_path)
                .with_context(|| format!("reading {}", config_path.display()))?;
            serde_json::from_str::<NemoConfigFile>(&contents)
                .with_context(|| format!("parsing {}", config_path.display()))?
        } else {
            NemoConfigFile {
                model_type: None,
                features_size: None,
                subsampling_factor: None,
                max_tokens_per_step: None,
            }
        };

        if let Some(model_type) = &config_file.model_type {
            if !model_type.to_lowercase().contains("nemo-conformer") {
                log::warn!(
                    "Unknown NeMo model_type '{}'; attempting to load anyway",
                    model_type
                );
            }
        }

        let config = NemoTransducerConfig {
            features_size: config_file.features_size.unwrap_or(128),
            subsampling_factor: config_file.subsampling_factor.unwrap_or(8),
            max_tokens_per_step: config_file.max_tokens_per_step.unwrap_or(10),
        };

        let preprocessor_path = match config.features_size {
            80 => model_dir.join("nemo80.onnx"),
            128 => model_dir.join("nemo128.onnx"),
            other => {
                anyhow::bail!(
                    "Unsupported NeMo features_size {} (expected 80 or 128)",
                    other
                )
            }
        };

        let encoder_path = first_encoder_with_matching_sidecar(&model_dir)
            .or_else(|_| {
                first_existing(
                    &model_dir,
                    &["encoder-model.onnx", "encoder.onnx", "encoder_model.onnx"],
                )
            })
            .context("finding NeMo encoder ONNX")?;
        let decoder_joint_path = first_existing(
            &model_dir,
            &[
                "decoder_joint-model.onnx",
                "decoder_joint.onnx",
                "decoder_joint_model.onnx",
            ],
        )
        .context("finding NeMo decoder_joint ONNX")?;

        if !preprocessor_path.exists() {
            anyhow::bail!(
                "Missing NeMo preprocessor model: {}",
                preprocessor_path.display()
            );
        }
        if !vocab_path.exists() {
            anyhow::bail!("Missing NeMo vocab: {}", vocab_path.display());
        }

        let preprocessor = Session::builder()?
            .with_optimization_level(GraphOptimizationLevel::Level3)?
            .with_intra_threads(4)?
            .commit_from_file(&preprocessor_path)
            .context("loading NeMo preprocessor")?;

        let encoder = Session::builder()?
            .with_optimization_level(GraphOptimizationLevel::Level3)?
            .with_intra_threads(4)?
            .commit_from_file(&encoder_path)
            .context("loading NeMo encoder")?;

        let decoder_joint = Session::builder()?
            .with_optimization_level(GraphOptimizationLevel::Level3)?
            .with_intra_threads(4)?
            .commit_from_file(&decoder_joint_path)
            .context("loading NeMo decoder_joint")?;

        let pre_io = PreprocessorIo {
            in_waveforms: find_input_name(preprocessor.inputs(), &["waveforms"])
                .unwrap_or_else(|| "waveforms".to_string()),
            in_waveforms_lens: find_input_name(
                preprocessor.inputs(),
                &["waveforms_lens", "lens", "length"],
            )
            .unwrap_or_else(|| "waveforms_lens".to_string()),
            out_features: find_output_name(preprocessor.outputs(), &["features"])
                .unwrap_or_else(|| "features".to_string()),
            out_features_lens: find_output_name(
                preprocessor.outputs(),
                &["features_lens", "lens", "length"],
            )
            .unwrap_or_else(|| "features_lens".to_string()),
        };

        let enc_io = EncoderIo {
            in_audio_signal: find_input_name(encoder.inputs(), &["audio_signal"])
                .unwrap_or_else(|| "audio_signal".to_string()),
            in_length: find_input_name(encoder.inputs(), &["length", "audio_signal_lens", "lens"])
                .unwrap_or_else(|| "length".to_string()),
            out_outputs: find_output_name(encoder.outputs(), &["outputs", "output"])
                .unwrap_or_else(|| "outputs".to_string()),
            out_encoded_lengths: find_output_name(
                encoder.outputs(),
                &["encoded_lengths", "lengths", "output_lengths"],
            )
            .unwrap_or_else(|| "encoded_lengths".to_string()),
        };

        let in_encoder_outputs = find_input_name(decoder_joint.inputs(), &["encoder_outputs"])
            .unwrap_or_else(|| "encoder_outputs".to_string());
        let in_targets = find_input_name(decoder_joint.inputs(), &["targets"])
            .unwrap_or_else(|| "targets".to_string());
        let in_target_length = find_input_name(decoder_joint.inputs(), &["target_length"])
            .unwrap_or_else(|| "target_length".to_string());
        let in_state1 = find_input_name(decoder_joint.inputs(), &["input_states_1"])
            .unwrap_or_else(|| "input_states_1".to_string());
        let in_state2 = find_input_name(decoder_joint.inputs(), &["input_states_2"])
            .unwrap_or_else(|| "input_states_2".to_string());
        let out_outputs = find_output_name(decoder_joint.outputs(), &["outputs", "output"])
            .unwrap_or_else(|| "outputs".to_string());
        let out_state1 = find_output_name(decoder_joint.outputs(), &["output_states_1"])
            .unwrap_or_else(|| "output_states_1".to_string());
        let out_state2 = find_output_name(decoder_joint.outputs(), &["output_states_2"])
            .unwrap_or_else(|| "output_states_2".to_string());

        let dec_io = DecoderJointIo {
            in_encoder_outputs,
            in_targets_ty: integer_input_type(decoder_joint.inputs(), &in_targets)?,
            in_targets,
            in_target_length_ty: integer_input_type(decoder_joint.inputs(), &in_target_length)?,
            in_target_length,
            in_state1,
            in_state2,
            out_outputs,
            out_state1,
            out_state2,
        };
        log::info!(
            "NeMo decoder_joint input types: targets={}, target_length={}",
            dec_io.in_targets_ty,
            dec_io.in_target_length_ty
        );

        let (vocab, blank_idx) = load_vocab(&vocab_path)?;
        let (state1, state2) = init_decoder_states(&decoder_joint, &dec_io)?;

        let chunk_samples = secs_to_samples(DEFAULT_CHUNK_SECS).max(1);
        let left_context_samples = secs_to_samples(DEFAULT_LEFT_CONTEXT_SECS);
        let right_context_samples = secs_to_samples(DEFAULT_RIGHT_CONTEXT_SECS);

        Ok(Self {
            preprocessor,
            encoder,
            decoder_joint,
            pre_io,
            enc_io,
            dec_io,
            config,
            vocab,
            blank_idx,
            state1,
            state2,
            tokens: Vec::new(),
            chunk_samples,
            left_context_samples,
            right_context_samples,
            processed_samples: 0,
            last_seen_samples: 0,
        })
    }

    fn reset_internal(&mut self) {
        self.tokens.clear();
        self.state1.fill(0.0);
        self.state2.fill(0.0);
        self.processed_samples = 0;
        self.last_seen_samples = 0;
    }

    fn preprocess(&mut self, samples: &[f32]) -> Result<(Array3<f32>, Array1<i64>)> {
        let waveforms = Array2::from_shape_vec((1, samples.len()), samples.to_vec())
            .context("building waveforms tensor")?;
        let waveforms_lens = Array1::from(vec![samples.len() as i64]);

        let waveforms_tensor = Tensor::from_array(waveforms)?;
        let waveforms_lens_tensor = Tensor::from_array(waveforms_lens.clone())?;

        let waveforms_val: SessionInputValue = waveforms_tensor.into();
        let waveforms_lens_val: SessionInputValue = waveforms_lens_tensor.into();

        let outputs = self
            .preprocessor
            .run(SessionInputs::from(vec![
                (self.pre_io.in_waveforms.clone(), waveforms_val),
                (self.pre_io.in_waveforms_lens.clone(), waveforms_lens_val),
            ]))
            .context("preprocessor inference")?;

        let features = outputs
            .get(self.pre_io.out_features.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing preprocessor output: features"))?
            .try_extract_array::<f32>()
            .map_err(|e| anyhow::anyhow!("preprocessor features extract error: {}", e))?
            .to_owned();
        let features_lens = outputs
            .get(self.pre_io.out_features_lens.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing preprocessor output: features_lens"))?
            .try_extract_array::<i64>()
            .map_err(|e| anyhow::anyhow!("preprocessor lens extract error: {}", e))?
            .to_owned();

        let features: Array3<f32> = features
            .into_dimensionality::<Ix3>()
            .context("preprocessor features shape")?;
        let features_lens: Array1<i64> = features_lens
            .into_dimensionality()
            .context("preprocessor lens shape")?;

        Ok((features, features_lens))
    }

    fn encode(
        &mut self,
        features: &Array3<f32>,
        features_lens: &Array1<i64>,
    ) -> Result<(Array2<f32>, usize)> {
        let audio_signal_tensor = Tensor::from_array(features.clone())?;
        let length_tensor = Tensor::from_array(features_lens.clone())?;

        let audio_signal_val: SessionInputValue = audio_signal_tensor.into();
        let length_val: SessionInputValue = length_tensor.into();

        let outputs = self
            .encoder
            .run(SessionInputs::from(vec![
                (self.enc_io.in_audio_signal.clone(), audio_signal_val),
                (self.enc_io.in_length.clone(), length_val),
            ]))
            .context("encoder inference")?;

        let encoder_out = outputs
            .get(self.enc_io.out_outputs.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing encoder output: outputs"))?
            .try_extract_array::<f32>()
            .map_err(|e| anyhow::anyhow!("encoder outputs extract error: {}", e))?
            .to_owned();
        let encoded_lengths = outputs
            .get(self.enc_io.out_encoded_lengths.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing encoder output: encoded_lengths"))?
            .try_extract_array::<i64>()
            .map_err(|e| anyhow::anyhow!("encoder lengths extract error: {}", e))?
            .to_owned();

        let encoded_lengths: Array1<i64> = encoded_lengths
            .into_dimensionality()
            .context("encoder lengths shape")?;
        let encoded_len = encoded_lengths.get(0).copied().unwrap_or(0).max(0) as usize;

        // Encoder output is typically [B, D, T] or [B, T, D]; normalize to [T, D] for batch=1.
        let out3: Array3<f32> = encoder_out
            .into_dimensionality::<Ix3>()
            .context("encoder outputs shape")?;
        let b0 = out3.index_axis(Axis(0), 0);
        let (d1, d2) = (b0.shape()[0], b0.shape()[1]);
        let time_major: Array2<f32> = if d1 > d2 {
            // [D, T] -> [T, D]
            b0.t().to_owned()
        } else {
            // [T, D]
            b0.to_owned()
        };

        let time_len = time_major.shape()[0];
        Ok((time_major, encoded_len.min(time_len)))
    }

    fn encoder_step_samples(&self) -> Result<usize> {
        let step_samples = HOP_LENGTH_SAMPLES * self.config.subsampling_factor;
        if step_samples == 0 {
            anyhow::bail!(
                "invalid subsampling_factor: {}",
                self.config.subsampling_factor
            );
        }
        Ok(step_samples)
    }

    fn prepare_window(&mut self, samples: &[f32]) -> Result<(Array2<f32>, usize)> {
        let (features, features_lens) = self.preprocess(samples)?;
        self.encode(&features, &features_lens)
    }

    fn decode_range_commit(
        &mut self,
        encodings: &Array2<f32>,
        start_t: usize,
        end_t: usize,
    ) -> Result<()> {
        let mut t: usize = start_t;
        let mut emitted_tokens: usize = 0;
        let mut state1 = self.state1.clone();
        let mut state2 = self.state2.clone();

        while t < end_t {
            let last_token = self.tokens.last().copied().unwrap_or(self.blank_idx);

            let (logits, step, new_state1, new_state2) = self.decode_step(
                encodings.index_axis(Axis(0), t),
                last_token,
                &state1,
                &state2,
            )?;

            let token = argmax(&logits) as u32;
            if token != self.blank_idx {
                state1 = new_state1;
                state2 = new_state2;
                self.tokens.push(token);
                emitted_tokens += 1;
            }

            let (next_t, next_emitted_tokens) = advance_tdt_cursor(
                t,
                token,
                self.blank_idx,
                step,
                emitted_tokens,
                self.config.max_tokens_per_step,
            );
            t = next_t;
            emitted_tokens = next_emitted_tokens;
        }

        self.state1 = state1;
        self.state2 = state2;
        Ok(())
    }

    fn decode_range_preview(
        &mut self,
        encodings: &Array2<f32>,
        start_t: usize,
        end_t: usize,
        state1: &mut Array3<f32>,
        state2: &mut Array3<f32>,
        tokens: &mut Vec<u32>,
    ) -> Result<()> {
        let mut t: usize = start_t;
        let mut emitted_tokens: usize = 0;

        while t < end_t {
            let last_token = tokens.last().copied().unwrap_or(self.blank_idx);

            let (logits, step, new_state1, new_state2) =
                self.decode_step(encodings.index_axis(Axis(0), t), last_token, state1, state2)?;

            let token = argmax(&logits) as u32;
            if token != self.blank_idx {
                *state1 = new_state1;
                *state2 = new_state2;
                tokens.push(token);
                emitted_tokens += 1;
            }

            let (next_t, next_emitted_tokens) = advance_tdt_cursor(
                t,
                token,
                self.blank_idx,
                step,
                emitted_tokens,
                self.config.max_tokens_per_step,
            );
            t = next_t;
            emitted_tokens = next_emitted_tokens;
        }

        Ok(())
    }

    fn decode_step(
        &mut self,
        enc_vec: ndarray::ArrayView1<'_, f32>,
        last_token: u32,
        state1: &Array3<f32>,
        state2: &Array3<f32>,
    ) -> Result<DecodeStepResult> {
        let dim = enc_vec.len();
        let encoder_outputs = Array3::from_shape_vec((1, dim, 1), enc_vec.to_vec())
            .context("building encoder_outputs")?;

        let encoder_outputs_tensor: SessionInputValue = Tensor::from_array(encoder_outputs)?.into();
        let targets_tensor = singleton_token_input(
            last_token,
            self.dec_io.in_targets_ty,
            &self.dec_io.in_targets,
        )?;
        let target_length_tensor = singleton_length_input(
            1,
            self.dec_io.in_target_length_ty,
            &self.dec_io.in_target_length,
        )?;
        let state1_tensor: SessionInputValue = Tensor::from_array(state1.clone())?.into();
        let state2_tensor: SessionInputValue = Tensor::from_array(state2.clone())?.into();

        let outputs = self
            .decoder_joint
            .run(SessionInputs::from(vec![
                (
                    self.dec_io.in_encoder_outputs.clone(),
                    encoder_outputs_tensor,
                ),
                (self.dec_io.in_targets.clone(), targets_tensor),
                (self.dec_io.in_target_length.clone(), target_length_tensor),
                (self.dec_io.in_state1.clone(), state1_tensor),
                (self.dec_io.in_state2.clone(), state2_tensor),
            ]))
            .with_context(|| {
                format!(
                    "decoder_joint inference (encoder_dim={}, last_token={}, state1_shape={:?}, state2_shape={:?})",
                    dim,
                    last_token,
                    state1.raw_dim(),
                    state2.raw_dim()
                )
            })?;

        let out = outputs
            .get(self.dec_io.out_outputs.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing decoder_joint output: outputs"))?
            .try_extract_array::<f32>()
            .map_err(|e| anyhow::anyhow!("decoder_joint outputs extract error: {}", e))?
            .to_owned();

        let out_state1 = outputs
            .get(self.dec_io.out_state1.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing decoder_joint output: output_states_1"))?
            .try_extract_array::<f32>()
            .map_err(|e| anyhow::anyhow!("decoder_joint state1 extract error: {}", e))?
            .to_owned()
            .into_dimensionality::<Ix3>()
            .context("decoder_joint state1 shape")?;
        let out_state2 = outputs
            .get(self.dec_io.out_state2.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing decoder_joint output: output_states_2"))?
            .try_extract_array::<f32>()
            .map_err(|e| anyhow::anyhow!("decoder_joint state2 extract error: {}", e))?
            .to_owned()
            .into_dimensionality::<Ix3>()
            .context("decoder_joint state2 shape")?;

        // Outputs often come as [1, N] or [1, 1, N]. Flatten to 1-D.
        let flat: Vec<f32> = out.iter().copied().collect();
        if flat.len() < self.vocab.len() {
            anyhow::bail!(
                "decoder_joint output too small ({}), vocab size {}",
                flat.len(),
                self.vocab.len()
            );
        }

        let vocab_size = self.vocab.len();
        let logits = flat[..vocab_size].to_vec();
        let step = argmax(&flat[vocab_size..]);

        Ok((logits, step, out_state1, out_state2))
    }

    fn tokens_to_text(&self, token_ids: &[u32]) -> String {
        tokens_to_text_from_vocab(&self.vocab, token_ids)
    }
}

impl Transcriber for NemoTransducerStreamer {
    fn transcribe_incremental(&mut self, samples: &[f32]) -> Result<String> {
        if samples.is_empty() {
            return Ok(String::new());
        }

        // The streaming coordinator passes the entire utterance buffer each call.
        // We keep an internal cursor (`processed_samples`) and decode incrementally in chunks.
        if samples.len() < self.last_seen_samples {
            // New utterance (buffer reset).
            self.reset_internal();
        }
        self.last_seen_samples = samples.len();

        // Only finalize chunks when we have enough right-context audio after them.
        let stable_limit = samples.len().saturating_sub(self.right_context_samples);
        let step_samples = self.encoder_step_samples()?;

        while self.processed_samples + self.chunk_samples <= stable_limit {
            let chunk_start = self.processed_samples;
            let chunk_end = chunk_start + self.chunk_samples;

            let window_start = chunk_start.saturating_sub(self.left_context_samples);
            let window_end = (chunk_end + self.right_context_samples).min(samples.len());

            let window = &samples[window_start..window_end];
            let (encodings, enc_len) = self.prepare_window(window)?;

            let rel_start = chunk_start - window_start;
            let rel_end = chunk_end - window_start;
            let start_frame = (rel_start / step_samples).min(enc_len);
            let end_frame = ceil_div(rel_end, step_samples).min(enc_len);
            if start_frame < end_frame {
                self.decode_range_commit(&encodings, start_frame, end_frame)?;
            }

            self.processed_samples = chunk_end;
        }

        // Speculative tail decode (not committed to internal state).
        let mut preview_tokens: Vec<u32> = Vec::new();
        if self.processed_samples < samples.len() {
            let window_start = self
                .processed_samples
                .saturating_sub(self.left_context_samples);
            let window_end = samples.len();

            let stable_len = self.tokens.len();
            let mut tmp_state1 = self.state1.clone();
            let mut tmp_state2 = self.state2.clone();
            let mut tmp_tokens = self.tokens.clone();

            let window = &samples[window_start..window_end];
            let (encodings, enc_len) = self.prepare_window(window)?;

            let rel_start = self.processed_samples - window_start;
            let rel_end = samples.len() - window_start;
            let start_frame = (rel_start / step_samples).min(enc_len);
            let end_frame = ceil_div(rel_end, step_samples).min(enc_len);

            if start_frame < end_frame {
                self.decode_range_preview(
                    &encodings,
                    start_frame,
                    end_frame,
                    &mut tmp_state1,
                    &mut tmp_state2,
                    &mut tmp_tokens,
                )?;
            }

            if tmp_tokens.len() > stable_len {
                preview_tokens.extend_from_slice(&tmp_tokens[stable_len..]);
            }
        }

        let mut all_tokens = self.tokens.clone();
        all_tokens.extend_from_slice(&preview_tokens);
        Ok(self.tokens_to_text(&all_tokens))
    }

    fn reset(&mut self) {
        self.reset_internal();
    }
}

fn first_existing(model_dir: &Path, candidates: &[&str]) -> Result<PathBuf> {
    for name in candidates {
        let p = model_dir.join(name);
        if p.exists() {
            return Ok(p);
        }
    }
    Err(anyhow::anyhow!(
        "none of the candidate files exist: {:?}",
        candidates
    ))
}

fn first_encoder_with_matching_sidecar(model_dir: &Path) -> Result<PathBuf> {
    for (encoder_name, sidecar_name) in [
        ("encoder-model.onnx", "encoder-model.onnx.data"),
        ("encoder.onnx", "encoder.onnx.data"),
        ("encoder_model.onnx", "encoder_model.onnx.data"),
    ] {
        let encoder_path = model_dir.join(encoder_name);
        if encoder_path.exists() && model_dir.join(sidecar_name).exists() {
            return Ok(encoder_path);
        }
    }

    Err(anyhow::anyhow!(
        "none of the encoder variants had a matching ONNX external-data sidecar"
    ))
}

fn load_vocab(path: &Path) -> Result<(Vec<String>, u32)> {
    let file = File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let reader = BufReader::new(file);

    let mut vocab: Vec<(u32, String)> = Vec::new();
    let mut blank_idx: Option<u32> = None;

    for (line_no, line) in reader.lines().enumerate() {
        let line = line.with_context(|| format!("reading {}:{}", path.display(), line_no + 1))?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let (token, id_str) = line.rsplit_once(' ').ok_or_else(|| {
            anyhow::anyhow!(
                "invalid vocab line {}:{}: {:?}",
                path.display(),
                line_no + 1,
                line
            )
        })?;
        let id: u32 = id_str.parse().with_context(|| {
            format!(
                "parsing vocab id {}:{}: {:?}",
                path.display(),
                line_no + 1,
                id_str
            )
        })?;

        let token = token.replace('\u{2581}', " ");
        if token == "<blk>" {
            blank_idx = Some(id);
        }
        vocab.push((id, token));
    }

    if vocab.is_empty() {
        anyhow::bail!("empty vocab: {}", path.display());
    }

    let max_id = vocab.iter().map(|(id, _)| *id).max().unwrap_or(0) as usize;
    let mut table = vec![String::new(); max_id + 1];
    for (id, token) in vocab {
        let idx = id as usize;
        if idx >= table.len() {
            continue;
        }
        table[idx] = token;
    }

    let blank_idx = blank_idx.ok_or_else(|| anyhow::anyhow!("vocab missing <blk> token"))?;
    Ok((table, blank_idx))
}

fn init_decoder_states(
    decoder_joint: &Session,
    io: &DecoderJointIo,
) -> Result<(Array3<f32>, Array3<f32>)> {
    let (s1_batch, s1_hidden) = state_shape(decoder_joint, &io.in_state1)?;
    let (s2_batch, s2_hidden) = state_shape(decoder_joint, &io.in_state2)?;
    Ok((
        Array3::<f32>::zeros((s1_batch, 1, s1_hidden)),
        Array3::<f32>::zeros((s2_batch, 1, s2_hidden)),
    ))
}

fn state_shape(decoder_joint: &Session, name: &str) -> Result<(usize, usize)> {
    let outlet = decoder_joint
        .inputs()
        .iter()
        .find(|i| i.name() == name)
        .ok_or_else(|| anyhow::anyhow!("missing decoder_joint input: {}", name))?;

    let shape = match outlet.dtype() {
        ValueType::Tensor { shape, .. } => shape,
        _ => anyhow::bail!("decoder_joint input {} is not a tensor", name),
    };

    if shape.len() != 3 {
        anyhow::bail!(
            "decoder_joint input {} has unexpected rank {} (expected 3)",
            name,
            shape.len()
        );
    }

    let batch = if shape[0] > 0 { shape[0] as usize } else { 1 };
    let hidden = if shape[2] > 0 {
        shape[2] as usize
    } else {
        anyhow::bail!("decoder_joint input {} has unknown hidden dim", name);
    };

    Ok((batch, hidden))
}

fn integer_input_type(inputs: &[Outlet], name: &str) -> Result<TensorElementType> {
    let outlet = inputs
        .iter()
        .find(|input| input.name() == name)
        .ok_or_else(|| anyhow::anyhow!("missing decoder_joint input: {}", name))?;

    match outlet.dtype().tensor_type() {
        Some(TensorElementType::Int32) => Ok(TensorElementType::Int32),
        Some(TensorElementType::Int64) => Ok(TensorElementType::Int64),
        Some(other) => anyhow::bail!(
            "decoder_joint input {} has unsupported integer tensor type {}",
            name,
            other
        ),
        None => anyhow::bail!("decoder_joint input {} is not a tensor", name),
    }
}

fn singleton_token_input(
    value: u32,
    tensor_type: TensorElementType,
    input_name: &str,
) -> Result<SessionInputValue<'static>> {
    match tensor_type {
        TensorElementType::Int32 => {
            let value = i32::try_from(value)
                .with_context(|| format!("converting {} value {} to i32", input_name, value))?;
            let tensor = Array2::from_shape_vec((1, 1), vec![value]).context("building targets")?;
            Ok(Tensor::from_array(tensor)?.into())
        }
        TensorElementType::Int64 => {
            let tensor = Array2::from_shape_vec((1, 1), vec![i64::from(value)])
                .context("building targets")?;
            Ok(Tensor::from_array(tensor)?.into())
        }
        other => anyhow::bail!(
            "unsupported tensor type {} for decoder_joint input {}",
            other,
            input_name
        ),
    }
}

fn singleton_length_input(
    value: usize,
    tensor_type: TensorElementType,
    input_name: &str,
) -> Result<SessionInputValue<'static>> {
    match tensor_type {
        TensorElementType::Int32 => {
            let value = i32::try_from(value)
                .with_context(|| format!("converting {} value {} to i32", input_name, value))?;
            Ok(Tensor::from_array(Array1::from(vec![value]))?.into())
        }
        TensorElementType::Int64 => {
            let value = i64::try_from(value)
                .with_context(|| format!("converting {} value {} to i64", input_name, value))?;
            Ok(Tensor::from_array(Array1::from(vec![value]))?.into())
        }
        other => anyhow::bail!(
            "unsupported tensor type {} for decoder_joint input {}",
            other,
            input_name
        ),
    }
}

fn find_input_name(inputs: &[Outlet], candidates: &[&str]) -> Option<String> {
    for candidate in candidates {
        for input in inputs {
            let name = input.name();
            if name.eq_ignore_ascii_case(candidate)
                || name.to_lowercase().contains(&candidate.to_lowercase())
            {
                return Some(name.to_string());
            }
        }
    }
    None
}

fn find_output_name(outputs: &[Outlet], candidates: &[&str]) -> Option<String> {
    for candidate in candidates {
        for output in outputs {
            let name = output.name();
            if name.eq_ignore_ascii_case(candidate)
                || name.to_lowercase().contains(&candidate.to_lowercase())
            {
                return Some(name.to_string());
            }
        }
    }
    None
}

fn tokens_to_text_from_vocab(vocab: &[String], token_ids: &[u32]) -> String {
    let mut s = String::new();
    for &id in token_ids {
        let idx = id as usize;
        if idx >= vocab.len() {
            continue;
        }
        let tok = &vocab[idx];

        // Skip special/control tokens to keep UI output clean.
        if tok.starts_with("<|") && tok.ends_with("|>") {
            continue;
        }
        if tok == "<blk>" {
            continue;
        }
        s.push_str(tok);
    }

    // Trim leading whitespace and collapse runs of whitespace.
    let s = s.trim_start();
    let mut out = String::with_capacity(s.len());
    let mut prev_space = false;
    for ch in s.chars() {
        if ch.is_whitespace() {
            if !prev_space {
                out.push(' ');
                prev_space = true;
            }
        } else {
            out.push(ch);
            prev_space = false;
        }
    }
    out
}

fn advance_tdt_cursor(
    t: usize,
    token: u32,
    blank_idx: u32,
    step: usize,
    emitted_tokens: usize,
    max_tokens_per_step: usize,
) -> (usize, usize) {
    if step > 0 {
        return (t.saturating_add(step), 0);
    }
    if token == blank_idx || emitted_tokens >= max_tokens_per_step {
        return (t + 1, 0);
    }
    (t, emitted_tokens)
}

fn argmax(values: &[f32]) -> usize {
    values
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(i, _)| i)
        .unwrap_or(0)
}

fn secs_to_samples(secs: f32) -> usize {
    if secs <= 0.0 {
        return 0;
    }
    (secs * SAMPLE_RATE_HZ as f32).round() as usize
}

fn ceil_div(n: usize, d: usize) -> usize {
    if d == 0 {
        return 0;
    }
    n.div_ceil(d)
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs;

    use tempfile::TempDir;

    use ort::tensor::{Shape, SymbolicDimensions};

    #[test]
    fn load_vocab_parses_and_finds_blank() {
        let dir = TempDir::new().unwrap();
        let vocab_path = dir.path().join("vocab.txt");
        fs::write(&vocab_path, "▁hello 0\nworld 1\n<blk> 2\n").unwrap();

        let (vocab, blank) = load_vocab(&vocab_path).unwrap();
        assert_eq!(blank, 2);
        assert_eq!(vocab[0], " hello"); // ▁ → space
        assert_eq!(vocab[1], "world");
        assert_eq!(vocab[2], "<blk>");
    }

    #[test]
    fn load_vocab_errors_without_blank() {
        let dir = TempDir::new().unwrap();
        let vocab_path = dir.path().join("vocab.txt");
        fs::write(&vocab_path, "a 0\nb 1\n").unwrap();

        let err = load_vocab(&vocab_path).unwrap_err().to_string();
        assert!(err.contains("<blk>"));
    }

    #[test]
    fn tokens_to_text_skips_control_and_collapses_whitespace() {
        let vocab = vec![
            " hello".to_string(),
            "<|en|>".to_string(),
            "  world".to_string(),
            "<blk>".to_string(),
            "\nthere".to_string(),
        ];

        let out = tokens_to_text_from_vocab(&vocab, &[0, 1, 2, 3, 4]);
        assert_eq!(out, "hello world there");
    }

    #[test]
    fn advance_tdt_cursor_step_has_priority() {
        let blank = 999;
        assert_eq!(advance_tdt_cursor(5, 123, blank, 3, 1, 10), (8, 0));
        assert_eq!(advance_tdt_cursor(5, blank, blank, 3, 0, 10), (8, 0));
    }

    #[test]
    fn advance_tdt_cursor_blank_advances_one() {
        let blank = 999;
        assert_eq!(advance_tdt_cursor(5, blank, blank, 0, 0, 10), (6, 0));
    }

    #[test]
    fn advance_tdt_cursor_emits_multiple_tokens_per_frame_until_max() {
        let blank = 999;
        // After emitting a token (token != blank) with step=0, we stay on the same frame.
        assert_eq!(advance_tdt_cursor(5, 123, blank, 0, 1, 10), (5, 1));
        // When we reach max_tokens_per_step, advance to the next frame.
        assert_eq!(advance_tdt_cursor(5, 123, blank, 0, 10, 10), (6, 0));
    }

    #[test]
    fn integer_input_type_accepts_int32_and_int64() {
        let inputs = vec![
            Outlet::new(
                "targets",
                ValueType::Tensor {
                    ty: TensorElementType::Int32,
                    shape: Shape::from(vec![1usize, 1]),
                    dimension_symbols: SymbolicDimensions::empty(2),
                },
            ),
            Outlet::new(
                "target_length",
                ValueType::Tensor {
                    ty: TensorElementType::Int64,
                    shape: Shape::from(vec![1usize]),
                    dimension_symbols: SymbolicDimensions::empty(1),
                },
            ),
        ];

        assert_eq!(
            integer_input_type(&inputs, "targets").unwrap(),
            TensorElementType::Int32
        );
        assert_eq!(
            integer_input_type(&inputs, "target_length").unwrap(),
            TensorElementType::Int64
        );
    }

    #[test]
    fn integer_input_type_rejects_non_integer_tensors() {
        let inputs = vec![Outlet::new(
            "targets",
            ValueType::Tensor {
                ty: TensorElementType::Float32,
                shape: Shape::from(vec![1usize, 1]),
                dimension_symbols: SymbolicDimensions::empty(2),
            },
        )];

        let err = integer_input_type(&inputs, "targets")
            .unwrap_err()
            .to_string();
        assert!(err.contains("unsupported integer tensor type"));
    }

    #[test]
    fn first_encoder_with_matching_sidecar_skips_broken_higher_priority_encoder() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("encoder-model.onnx"), b"broken official").unwrap();
        fs::write(dir.path().join("encoder.onnx"), b"legacy").unwrap();
        fs::write(dir.path().join("encoder.onnx.data"), b"legacy sidecar").unwrap();

        let encoder = first_encoder_with_matching_sidecar(dir.path()).unwrap();

        assert_eq!(
            encoder.file_name().unwrap().to_str().unwrap(),
            "encoder.onnx"
        );
    }
}
