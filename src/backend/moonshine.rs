//! Moonshine streaming transcription - ONNX-based ASR

use std::path::Path;
use std::sync::OnceLock;

use anyhow::{Context, Result};
use ndarray::{Array1, Array2, ArrayD, Axis, IxDyn};
use ort::session::{builder::GraphOptimizationLevel, Session, SessionInputValue, SessionInputs};
use ort::tensor::TensorElementType;
use ort::value::Tensor;
use ort::value::ValueType;
use parking_lot::Mutex;
use serde::Deserialize;
use tokenizers::Tokenizer;

static ORT_INITIALIZED: OnceLock<()> = OnceLock::new();

/// Initialize ORT runtime globally. Safe to call multiple times (no-op after first).
pub fn init_ort() {
    ORT_INITIALIZED.get_or_init(|| {
        ort::init().commit();
    });
}

pub struct MoonshineStreamer {
    encoder: Mutex<Session>,
    decoder: Mutex<Session>,
    tokenizer: MoonshineTokenizer,
    config: MoonshineConfig,

    past_cache: Option<Vec<ort::value::Tensor<f32>>>,
    cache_seq_len: usize,
    last_tokens: Vec<u32>,
}

#[derive(Debug, Clone)]
struct MoonshineConfig {
    do_normalize: bool,
    #[allow(dead_code)]
    sampling_rate: usize,
}

impl Default for MoonshineConfig {
    fn default() -> Self {
        Self {
            do_normalize: true,
            sampling_rate: 16000,
        }
    }
}

#[derive(Debug, Deserialize)]
struct PreprocessorConfig {
    do_normalize: Option<bool>,
    sampling_rate: Option<usize>,
}

struct MoonshineTokenizer {
    tokenizer: Tokenizer,
    bos_token_id: u32,
    eos_token_id: u32,
}

const BOS_CANDIDATES: &[&str] = &[
    "<s>",
    "<|startoftranscript|>",
    "<|startoftext|>",
    "<sos>",
    "<bos>",
    "[BOS]",
];
const EOS_CANDIDATES: &[&str] = &[
    "</s>",
    "<|endoftext|>",
    "<|endoftranscript|>",
    "<eos>",
    "[EOS]",
    "<|eot|>",
];

impl MoonshineTokenizer {
    fn from_dir(model_dir: &Path) -> Result<Self> {
        let tokenizer_path = model_dir.join("tokenizer.json");

        if !tokenizer_path.exists() {
            return Err(anyhow::anyhow!(
                "Tokenizer not found at {:?}. The new Moonshine repository structure does not include tokenizer.json. \
                This is a known limitation - see https://huggingface.co/UsefulSensors/moonshine for updates.",
                tokenizer_path
            ));
        }

        let tokenizer = Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| anyhow::anyhow!("loading tokenizer: {}", e))?;

        // Try to find BOS token, but use fallback if out of vocab range (32768)
        const MAX_VOCAB: u32 = 32768;
        let bos_token_id = BOS_CANDIDATES
            .iter()
            .filter_map(|t| tokenizer.token_to_id(t))
            .find(|&id| id < MAX_VOCAB)
            .unwrap_or(1); // Fallback to 1 if not found or out of range

        let eos_token_id = EOS_CANDIDATES
            .iter()
            .filter_map(|t| tokenizer.token_to_id(t))
            .find(|&id| id < MAX_VOCAB)
            .unwrap_or(2); // Fallback to 2 if not found or out of range

        Ok(Self {
            tokenizer,
            bos_token_id,
            eos_token_id,
        })
    }

    fn decode(&self, ids: &[u32]) -> Result<String> {
        self.tokenizer
            .decode(ids, true)
            .map_err(|e| anyhow::anyhow!("decode error: {}", e))
    }
}

impl MoonshineStreamer {
    pub fn new(model_dir: impl AsRef<Path>) -> Result<Self> {
        init_ort();

        let model_dir = model_dir.as_ref();

        // Prefer the official merged ONNX naming, but keep compatibility with older downloads.
        let encoder_path = if model_dir.join("encoder_model.onnx").exists() {
            model_dir.join("encoder_model.onnx")
        } else {
            model_dir.join("encode.onnx")
        };

        let decoder_path = if model_dir.join("decoder_model_merged.onnx").exists() {
            model_dir.join("decoder_model_merged.onnx")
        } else {
            model_dir.join("cached_decode.onnx")
        };
        let config_path = model_dir.join("preprocessor_config.json");

        if !encoder_path.exists() {
            return Err(anyhow::anyhow!(
                "Missing encoder model: {}",
                encoder_path.display()
            ));
        }
        if !decoder_path.exists() {
            return Err(anyhow::anyhow!(
                "Missing decoder model: {}",
                decoder_path.display()
            ));
        }

        let encoder = Session::builder()?
            .with_optimization_level(GraphOptimizationLevel::Level3)?
            .with_intra_threads(4)?
            .commit_from_file(&encoder_path)
            .context("loading encoder")?;

        let decoder = Session::builder()?
            .with_optimization_level(GraphOptimizationLevel::Level3)?
            .with_intra_threads(4)?
            .commit_from_file(&decoder_path)
            .context("loading decoder")?;

        let tokenizer = MoonshineTokenizer::from_dir(model_dir)?;

        let config = if config_path.exists() {
            let contents = std::fs::read_to_string(&config_path)?;
            let preprocessor: PreprocessorConfig = serde_json::from_str(&contents)?;
            MoonshineConfig {
                do_normalize: preprocessor.do_normalize.unwrap_or(true),
                sampling_rate: preprocessor.sampling_rate.unwrap_or(16000),
            }
        } else {
            MoonshineConfig::default()
        };

        Ok(Self {
            encoder: Mutex::new(encoder),
            decoder: Mutex::new(decoder),
            tokenizer,
            config,
            past_cache: None,
            cache_seq_len: 0,
            last_tokens: Vec::new(),
        })
    }

    /// Transcribe audio samples to text (one-shot, non-streaming).
    ///
    /// This resets internal state and uses the incremental decoder path,
    /// which handles varying ONNX input naming conventions across exporters.
    pub fn transcribe(&mut self, samples: &[f32]) -> Result<String> {
        self.reset();
        self.transcribe_incremental(samples)
    }

    pub fn transcribe_incremental(&mut self, samples: &[f32]) -> Result<String> {
        if samples.is_empty() {
            return Ok(String::new());
        }

        let normalized = self.preprocess(samples);
        let encoder_states = self.encode(&normalized)?;

        let token_ids = if self.past_cache.is_none() {
            self.last_tokens.clear();
            self.cache_seq_len = 0;
            self.greedy_decode_cached(&encoder_states)?
        } else {
            self.greedy_decode_cached(&encoder_states)?
        };

        self.last_tokens = token_ids.clone();
        self.tokenizer.decode(&token_ids)
    }

    pub fn reset(&mut self) {
        self.past_cache = None;
        self.cache_seq_len = 0;
        self.last_tokens.clear();
    }

    fn preprocess(&self, samples: &[f32]) -> Array2<f32> {
        let mut input = samples.to_vec();

        if self.config.do_normalize && !input.is_empty() {
            let mean = input.iter().sum::<f32>() / input.len() as f32;
            let var = input.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / input.len() as f32;
            let std = var.sqrt().max(1e-6);
            for v in &mut input {
                *v = (*v - mean) / std;
            }
        }

        Array2::from_shape_vec((1, input.len()), input).expect("shape error")
    }

    fn encode(&self, input: &Array2<f32>) -> Result<ArrayD<f32>> {
        let mut encoder = self.encoder.lock();

        let input_rank = encoder
            .inputs()
            .first()
            .and_then(|i| match i.dtype() {
                ValueType::Tensor { shape, .. } => Some(shape.len()),
                _ => None,
            })
            .unwrap_or(2);

        let input_tensor = match input_rank {
            2 => Tensor::from_array(input.clone())?,
            // Some Moonshine encoder exports include a channel dimension.
            3 => {
                let samples = input
                    .as_slice()
                    .ok_or_else(|| anyhow::anyhow!("encoder input is not contiguous"))?;
                let reshaped =
                    ArrayD::from_shape_vec(IxDyn(&[1, 1, input.shape()[1]]), samples.to_vec())?;
                Tensor::from_array(reshaped)?
            }
            other => {
                return Err(anyhow::anyhow!(
                    "unsupported encoder input rank: {} (expected 2 or 3)",
                    other
                ))
            }
        };

        let input_name = encoder
            .inputs()
            .first()
            .map(|i| i.name().to_string())
            .unwrap_or_else(|| "input_values".to_string());

        let output_name = find_output_name(
            encoder.outputs(),
            &["last_hidden_state", "encoder_hidden_states", "output"],
        )
        .or_else(|| encoder.outputs().first().map(|o| o.name().to_string()))
        .unwrap_or_else(|| "last_hidden_state".to_string());

        // Get sequence length for encoder (2nd input)
        let seq_len = input.shape()[1] as i32;

        let seq_len_input = encoder.inputs().get(1).and_then(|i| {
            let name = i.name().to_string();
            let (ty, rank) = match i.dtype() {
                ValueType::Tensor { ty, shape, .. } => (*ty, shape.len()),
                _ => return None,
            };

            // Some encoder exports include a scalar-ish sequence length input.
            // Avoid guessing for other second inputs (e.g. attention_mask).
            let name_lc = name.to_lowercase();
            let looks_like_len = name_lc.contains("len") || name_lc.contains("length");
            let is_int = ty == TensorElementType::Int64 || ty == TensorElementType::Int32;
            let is_rank1 = rank == 1;

            if looks_like_len && is_int && is_rank1 {
                Some((name, ty))
            } else {
                None
            }
        });

        let outputs = if let Some((ref seq_name, seq_ty)) = seq_len_input {
            let input_val: SessionInputValue = input_tensor.into();

            let seq_val: SessionInputValue = if seq_ty == TensorElementType::Int64 {
                let t = Tensor::from_array(Array1::from(vec![seq_len as i64]))?;
                t.into()
            } else {
                let t = Tensor::from_array(Array1::from(vec![seq_len]))?;
                t.into()
            };

            encoder.run(SessionInputs::from(vec![
                (input_name.clone(), input_val),
                (seq_name.clone(), seq_val),
            ]))
        } else {
            encoder.run(ort::inputs! { input_name.as_str() => input_tensor })
        }
        .context("encoder inference")?;

        outputs
            .get(output_name.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing encoder output"))?
            .try_extract_array::<f32>()
            .map(|a| a.to_owned())
            .map_err(|e| anyhow::anyhow!("encoder output error: {}", e))
    }

    fn greedy_decode_cached(&mut self, encoder_states: &ArrayD<f32>) -> Result<Vec<u32>> {
        let encoder_tensor = Tensor::from_array(encoder_states.clone())?;

        let mut decoder = self.decoder.lock();

        // HuggingFace Moonshine uses args_* names
        let input_ids_name = find_input_name(
            decoder.inputs(),
            &["input_ids", "decoder_input_ids", "args_0"],
        )
        .unwrap_or_else(|| "args_0".to_string());

        let encoder_name = find_input_name(
            decoder.inputs(),
            &["encoder_hidden_states", "encoder_outputs", "args_1"],
        )
        .unwrap_or_else(|| "args_1".to_string());

        let input_ids_is_i64 = decoder
            .inputs()
            .iter()
            .find(|i| i.name() == input_ids_name)
            .map(|i| {
                matches!(
                    i.dtype(),
                    ValueType::Tensor {
                        ty: TensorElementType::Int64,
                        ..
                    }
                )
            })
            .unwrap_or(false);

        let logits_name = find_output_name(
            decoder.outputs(),
            &["logits", "output", "reversible_embedding"],
        )
        .unwrap_or_else(|| "reversible_embedding".to_string());

        let use_cache_name = find_input_name(decoder.inputs(), &["use_cache_branch", "use_cache"]);

        let seq_len_name = find_input_name(
            decoder.inputs(),
            &["encoder_sequence_length", "seq_len", "args_2"],
        );

        let past_names: Vec<String> = decoder
            .inputs()
            .iter()
            .map(|i| i.name())
            .filter(|name| name.contains("past_key_values"))
            .map(|s| s.to_string())
            .collect();

        let present_names: Vec<String> = decoder
            .outputs()
            .iter()
            .map(|o| o.name())
            .filter(|name| name.contains("present_key_values") || name.contains("present"))
            .map(|s| s.to_string())
            .collect();

        if !past_names.is_empty() && past_names.len() != present_names.len() {
            return Err(anyhow::anyhow!(
                "decoder KV-cache past/present mismatch ({} vs {})",
                past_names.len(),
                present_names.len()
            ));
        }

        // Disable decoder KV-cache by default.
        //
        // Several Moonshine ONNX exports gate internal cache logic behind an `If` node and a
        // `use_cache*` boolean input. Running greedy decoding without the cache is slower but
        // avoids shape mismatches across model variants.
        let use_kv_cache = false;

        let max_tokens = estimate_max_tokens(encoder_states.shape());

        let mut tokens = self.last_tokens.clone();
        if tokens.is_empty() {
            tokens.push(self.tokenizer.bos_token_id);
        }

        let start_step = self.cache_seq_len.min(tokens.len());
        let mut cache = self.past_cache.clone();

        for _ in start_step..max_tokens {
            let input_ids_tensor: SessionInputValue = if use_kv_cache {
                let last_token = tokens
                    .last()
                    .copied()
                    .unwrap_or(self.tokenizer.bos_token_id);

                if input_ids_is_i64 {
                    let input_ids_array = Array2::from_shape_vec((1, 1), vec![last_token as i64])?;
                    Tensor::from_array(input_ids_array)?.into()
                } else {
                    let input_ids_array = Array2::from_shape_vec((1, 1), vec![last_token as i32])?;
                    Tensor::from_array(input_ids_array)?.into()
                }
            } else if input_ids_is_i64 {
                let ids: Vec<i64> = tokens.iter().map(|&t| t as i64).collect();
                let input_ids_array = Array2::from_shape_vec((1, ids.len()), ids)?;
                Tensor::from_array(input_ids_array)?.into()
            } else {
                let ids: Vec<i32> = tokens.iter().map(|&t| t as i32).collect();
                let input_ids_array = Array2::from_shape_vec((1, ids.len()), ids)?;
                Tensor::from_array(input_ids_array)?.into()
            };

            if use_kv_cache && cache.is_none() {
                cache = Some(init_cache_tensors(&decoder, &past_names)?);
            }

            let mut inputs: Vec<(String, SessionInputValue)> = vec![
                (input_ids_name.clone(), input_ids_tensor),
                (encoder_name.clone(), (&encoder_tensor).into()),
            ];

            if let Some(ref name) = seq_len_name {
                let seq_len = encoder_states.shape()[1] as i32;
                let seq_len_tensor = Tensor::from_array(Array1::from(vec![seq_len]))?;
                inputs.push((name.clone(), seq_len_tensor.into()));
            }

            if let Some(ref cache_name) = use_cache_name {
                let cache_value = Array1::from(vec![use_kv_cache]);
                let cache_tensor = Tensor::from_array(cache_value)?;
                inputs.push((cache_name.clone(), cache_tensor.into()));
            }

            if use_kv_cache {
                if let Some(ref cache_values) = cache {
                    for (name, value) in past_names.iter().zip(cache_values.iter()) {
                        inputs.push((name.clone(), value.view().into()));
                    }
                }
            }

            let outputs = decoder
                .run(SessionInputs::from(inputs))
                .context("decoder inference (cached)")?;

            let logits = outputs
                .get(logits_name.as_str())
                .ok_or_else(|| anyhow::anyhow!("missing decoder logits"))?
                .try_extract_array::<f32>()
                .map_err(|e| anyhow::anyhow!("logits error: {}", e))?;

            let next_token = select_next_token(logits.to_owned())?;

            if use_kv_cache && !present_names.is_empty() {
                let mut new_cache = Vec::with_capacity(present_names.len());
                for name in &present_names {
                    let dyn_value = outputs.get(name.as_str()).ok_or_else(|| {
                        anyhow::anyhow!("missing cached decoder output: {}", name)
                    })?;
                    let owned = dyn_value.view().try_upgrade().map_err(|_| {
                        anyhow::anyhow!("failed to upgrade decoder output: {}", name)
                    })?;
                    let tensor: ort::value::Tensor<f32> = owned.downcast()?;
                    new_cache.push(tensor);
                }
                cache = Some(new_cache);
            }

            tokens.push(next_token);
            self.cache_seq_len = tokens.len();

            if next_token == self.tokenizer.eos_token_id {
                break;
            }
        }

        self.past_cache = cache;
        Ok(tokens)
    }
}

fn init_cache_tensors(
    decoder: &Session,
    past_names: &[String],
) -> Result<Vec<ort::value::Tensor<f32>>> {
    let mut tensors = Vec::with_capacity(past_names.len());

    for name in past_names {
        let input = decoder
            .inputs()
            .iter()
            .find(|i| i.name() == name)
            .ok_or_else(|| anyhow::anyhow!("missing decoder cache input metadata: {}", name))?;

        let dtype = input.dtype();
        let shape = match dtype {
            ort::value::ValueType::Tensor { shape, .. } => shape,
            _ => {
                return Err(anyhow::anyhow!(
                    "decoder cache input is not a tensor: {}",
                    name
                ))
            }
        };

        let dims: Vec<usize> = shape
            .iter()
            .enumerate()
            .map(|(idx, d)| {
                if *d < 0 {
                    if idx == 0 {
                        1
                    } else {
                        0
                    }
                } else {
                    *d as usize
                }
            })
            .collect();

        let array = ArrayD::<f32>::zeros(IxDyn(&dims));
        let tensor = Tensor::from_array(array)?;
        tensors.push(tensor);
    }

    Ok(tensors)
}

fn find_input_name(inputs: &[ort::value::Outlet], candidates: &[&str]) -> Option<String> {
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

fn find_output_name(outputs: &[ort::value::Outlet], candidates: &[&str]) -> Option<String> {
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

fn estimate_max_tokens(encoder_shape: &[usize]) -> usize {
    let seq_len = encoder_shape.get(1).copied().unwrap_or(100);
    (seq_len / 10).clamp(32, 512)
}

fn select_next_token(logits: ArrayD<f32>) -> Result<u32> {
    let vector: Array1<f32> = match logits.ndim() {
        1 => logits.into_dimensionality()?,
        2 => {
            let last = logits.shape()[0].saturating_sub(1);
            logits
                .index_axis(Axis(0), last)
                .to_owned()
                .into_dimensionality()?
        }
        3 => {
            let batch = logits.index_axis(Axis(0), 0);
            let last = batch.shape()[0].saturating_sub(1);
            batch
                .index_axis(Axis(0), last)
                .to_owned()
                .into_dimensionality()?
        }
        _ => return Err(anyhow::anyhow!("unsupported logits shape")),
    };

    let (best_idx, _) = vector
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .unwrap_or((0, &0.0));

    Ok(best_idx as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ndarray::IxDyn;

    #[test]
    fn preprocess_normalizes_samples() {
        let samples = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let mut input = samples.clone();

        let mean = input.iter().sum::<f32>() / input.len() as f32;
        let var = input.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / input.len() as f32;
        let std = var.sqrt();

        for v in &mut input {
            *v = (*v - mean) / std;
        }

        let normalized_mean: f32 = input.iter().sum::<f32>() / input.len() as f32;
        assert!(normalized_mean.abs() < 0.001);
    }

    #[test]
    fn estimate_max_tokens_reasonable() {
        assert_eq!(estimate_max_tokens(&[1, 100, 256]), 32);
        assert_eq!(estimate_max_tokens(&[1, 1000, 256]), 100);
        assert_eq!(estimate_max_tokens(&[1, 10000, 256]), 512);
    }

    #[test]
    fn select_next_token_finds_max() {
        let logits = ArrayD::from_shape_vec(
            IxDyn(&[1, 3, 5]),
            vec![
                0.1, 0.2, 0.9, 0.3, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.8, 0.1,
            ],
        )
        .unwrap();

        let token = select_next_token(logits).unwrap();
        assert_eq!(token, 3);
    }

    fn build_toy_decoder() -> Session {
        use ort::editor::{Graph, Model, Node, Opset, ONNX_DOMAIN};
        use ort::tensor::{Shape, SymbolicDimensions, TensorElementType};
        use ort::value::{Outlet, ValueType};

        let mut graph = Graph::new().unwrap();

        graph
            .set_inputs([
                Outlet::new(
                    "input_ids",
                    ValueType::Tensor {
                        ty: TensorElementType::Int64,
                        shape: Shape::new([1, 1]),
                        dimension_symbols: SymbolicDimensions::empty(2),
                    },
                ),
                Outlet::new(
                    "encoder_hidden_states",
                    ValueType::Tensor {
                        ty: TensorElementType::Float32,
                        shape: Shape::new([1, 1, 1]),
                        dimension_symbols: SymbolicDimensions::empty(3),
                    },
                ),
                Outlet::new(
                    "use_cache_branch",
                    ValueType::Tensor {
                        ty: TensorElementType::Bool,
                        shape: Shape::new([1]),
                        dimension_symbols: SymbolicDimensions::empty(1),
                    },
                ),
            ])
            .unwrap();

        let allocator = ort::memory::Allocator::default();
        let mut logits = Tensor::<f32>::new(&allocator, [1usize, 1, 3]).unwrap();
        let (_, buf) = logits.extract_tensor_mut();
        buf.copy_from_slice(&[1.0, 0.0, -1.0]);

        graph
            .add_initializer("logits_const", logits, false)
            .unwrap();

        let node = Node::new(
            "Identity",
            ONNX_DOMAIN,
            "logits_out",
            ["logits_const"],
            ["logits"],
            [],
        )
        .unwrap();
        graph.add_node(node).unwrap();

        graph
            .set_outputs([Outlet::new(
                "logits",
                ValueType::Tensor {
                    ty: TensorElementType::Float32,
                    shape: Shape::new([1, 1, 3]),
                    dimension_symbols: SymbolicDimensions::empty(3),
                },
            )])
            .unwrap();

        let mut model = Model::new([Opset::new(ONNX_DOMAIN, 19).unwrap()]).unwrap();
        model.add_graph(graph).unwrap();
        model.into_session(Session::builder().unwrap()).unwrap()
    }

    #[test]
    fn reset_clears_incremental_state() {
        let decoder = build_toy_decoder();
        let mut streamer = MoonshineStreamer {
            encoder: Mutex::new(build_toy_decoder()),
            decoder: Mutex::new(decoder),
            tokenizer: MoonshineTokenizer {
                tokenizer: Tokenizer::new(tokenizers::models::bpe::BPE::default()),
                bos_token_id: 1,
                eos_token_id: 2,
            },
            config: MoonshineConfig::default(),
            past_cache: Some(vec![Tensor::from_array(ArrayD::<f32>::zeros(IxDyn(&[
                1, 0,
            ])))
            .unwrap()]),
            cache_seq_len: 42,
            last_tokens: vec![1, 2, 3],
        };

        streamer.reset();

        assert!(streamer.past_cache.is_none());
        assert_eq!(streamer.cache_seq_len, 0);
        assert!(streamer.last_tokens.is_empty());
    }

    #[test]
    fn greedy_decode_cached_updates_cache_seq_len() {
        let mut streamer = MoonshineStreamer {
            encoder: Mutex::new(build_toy_decoder()),
            decoder: Mutex::new(build_toy_decoder()),
            tokenizer: MoonshineTokenizer {
                tokenizer: Tokenizer::new(tokenizers::models::bpe::BPE::default()),
                bos_token_id: 1,
                eos_token_id: 0,
            },
            config: MoonshineConfig::default(),
            past_cache: None,
            cache_seq_len: 0,
            last_tokens: vec![1],
        };

        let encoder_states = ArrayD::<f32>::zeros(IxDyn(&[1, 1, 1]));
        let tokens = streamer.greedy_decode_cached(&encoder_states).unwrap();

        assert_eq!(streamer.cache_seq_len, tokens.len());
        assert!(streamer.cache_seq_len > 1);
    }
}
