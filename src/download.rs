//! Model download infrastructure for usit
//!
//! Handles automatic downloading of Silero VAD and Moonshine models from GitHub and HuggingFace.
//! Uses atomic write pattern (temp file → rename) to ensure partial downloads don't corrupt files.

use anyhow::{Context, Result};
use futures_util::StreamExt;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;

use crate::config::AsrModelId;

/// Silero VAD model URL (GitHub)
const SILERO_VAD_URL: &str =
    "https://github.com/snakers4/silero-vad/raw/master/src/silero_vad/data/silero_vad.onnx";

/// Moonshine ONNX model files to download (from main moonshine repo)
const MOONSHINE_ONNX_FILES: [&str; 2] = ["encoder_model.onnx", "decoder_model_merged.onnx"];

/// Moonshine support files to download (from variant-specific repo)
const MOONSHINE_VARIANT_FILES: [&str; 2] = ["tokenizer.json", "preprocessor_config.json"];

/// Parakeet (NeMo TDT) ONNX repo with exported artifacts.
const PARAKEET_ONNX_URL: &str =
    "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/main";

/// Optional Parakeet artifacts (downloaded if present).
const PARAKEET_OPTIONAL_FILES: [&str; 1] = ["config.json"];

/// Acceptable NeMo preprocessor graphs.
const PARAKEET_NEMO_FILES: [&str; 2] = ["nemo128.onnx", "nemo80.onnx"];

/// Acceptable Parakeet encoder/sidecar filename pairs.
const PARAKEET_ENCODER_FILE_PAIRS: [(&str, &str); 3] = [
    ("encoder-model.onnx", "encoder-model.onnx.data"),
    ("encoder.onnx", "encoder.onnx.data"),
    ("encoder_model.onnx", "encoder_model.onnx.data"),
];

fn any_exists(asr_dir: &Path, candidates: &[&str]) -> bool {
    candidates.iter().any(|f| asr_dir.join(f).exists())
}

fn any_matching_exists(asr_dir: &Path, candidates: &[(&str, &str)]) -> bool {
    candidates
        .iter()
        .any(|(primary, sidecar)| asr_dir.join(primary).exists() && asr_dir.join(sidecar).exists())
}

/// Construct HuggingFace URL for Moonshine ONNX files
///
/// Returns the base URL for downloading Moonshine ONNX files for the given variant.
/// URL pattern: `https://huggingface.co/UsefulSensors/moonshine/resolve/main/onnx/merged/{variant}/float`
fn moonshine_onnx_url(variant: AsrModelId) -> Result<String> {
    let variant_name = variant
        .moonshine_download_url_segment()
        .ok_or_else(|| anyhow::anyhow!("Moonshine download requested for non-Moonshine model"))?;
    Ok(format!(
        "https://huggingface.co/UsefulSensors/moonshine/resolve/main/onnx/merged/{}/float",
        variant_name
    ))
}

/// Construct HuggingFace URL for variant-specific files (tokenizer, config)
///
/// Returns the base URL for downloading support files from the variant-specific repo.
/// URL pattern: `https://huggingface.co/UsefulSensors/moonshine-{variant}/resolve/main`
fn moonshine_variant_url(variant: AsrModelId) -> Result<String> {
    let variant_name = variant
        .moonshine_download_url_segment()
        .ok_or_else(|| anyhow::anyhow!("Moonshine download requested for non-Moonshine model"))?;
    Ok(format!(
        "https://huggingface.co/UsefulSensors/moonshine-{}/resolve/main",
        variant_name
    ))
}

/// Paths to all required model files
#[derive(Debug, Clone)]
pub struct ModelPaths {
    /// Path to Silero VAD model
    pub silero_vad: PathBuf,
    /// Directory containing the selected ASR model.
    ///
    /// - Moonshine: contains `encoder_model.onnx`, `decoder_model_merged.onnx`, `tokenizer.json`, etc.\n    /// - NeMo transducer (Parakeet): contains `encoder-model.onnx`, `decoder_joint-model.onnx`, `vocab.txt`, `nemo128.onnx`, etc.
    pub asr_dir: PathBuf,
}

/// Download a file from URL to destination with atomic write pattern
///
/// Uses temp file (`.downloading`) during download, then atomically renames to final path.
/// This ensures partial downloads don't leave corrupted files.
///
/// If `progress_callback` is provided, it is called with progress values (0.0..1.0)
/// instead of printing to stdout.
///
/// If `cancel_token` is provided and set to `true`, the download is aborted and the
/// temp file is deleted.
async fn download_file(
    url: &str,
    dest: &Path,
    progress_callback: Option<&(dyn Fn(f64) + Send + Sync)>,
    cancel_token: Option<&std::sync::atomic::AtomicBool>,
) -> Result<()> {
    // Create parent directories if needed
    if let Some(parent) = dest.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).context("Failed to create parent directories")?;
        }
    }

    // Temp file for atomic write pattern
    let temp_path = dest.with_extension("downloading");

    // Get filename for progress display
    let filename = dest.file_name().and_then(|n| n.to_str()).unwrap_or("file");

    if progress_callback.is_none() {
        println!("Downloading {}...", filename);
    }

    let response = reqwest::get(url)
        .await
        .context(format!("Failed to download from {}", url))?;

    if !response.status().is_success() {
        return Err(anyhow::anyhow!(
            "Failed to download {}: HTTP {}",
            filename,
            response.status()
        ));
    }

    let total_size = response.content_length().unwrap_or(0);
    let mut file = tokio::fs::File::create(&temp_path)
        .await
        .context(format!("Failed to create temp file at {:?}", temp_path))?;

    let mut stream = response.bytes_stream();
    let mut downloaded: u64 = 0;

    loop {
        let Some(item) = stream.next().await else {
            break;
        };

        if cancel_token
            .map(|t| t.load(std::sync::atomic::Ordering::Relaxed))
            .unwrap_or(false)
        {
            drop(file);
            let _ = tokio::fs::remove_file(&temp_path).await;
            return Err(anyhow::anyhow!("Download cancelled"));
        }

        let chunk = item.context("Error while downloading file")?;
        file.write_all(&chunk)
            .await
            .context("Failed to write chunk to file")?;

        downloaded += chunk.len() as u64;
        if total_size > 0 {
            let progress = downloaded as f64 / total_size as f64;
            if let Some(cb) = &progress_callback {
                cb(progress);
            } else {
                print!(
                    "\rDownloading {}... {:.1}% ({}/{} bytes)",
                    filename,
                    progress * 100.0,
                    downloaded,
                    total_size
                );
                std::io::stdout().flush()?;
            }
        }
    }

    if progress_callback.is_none() {
        if total_size > 0 {
            println!(
                "\rDownload complete: {}/{} bytes (100%)    ",
                downloaded, total_size
            );
        } else {
            println!("\rDownload complete: {} bytes", downloaded);
        }
    }

    // Close the file before renaming
    drop(file);

    // Atomic rename: move temp file to final location
    fs::rename(&temp_path, dest).context(format!(
        "Failed to rename downloaded file from {:?} to {:?}",
        temp_path, dest
    ))?;

    Ok(())
}

/// Download a file if it exists (returns false on 404).
async fn download_file_optional(
    url: &str,
    dest: &Path,
    progress_callback: Option<&(dyn Fn(f64) + Send + Sync)>,
    cancel_token: Option<&std::sync::atomic::AtomicBool>,
) -> Result<bool> {
    if let Some(parent) = dest.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).context("Failed to create parent directories")?;
        }
    }

    let response = reqwest::get(url)
        .await
        .context(format!("Failed to download from {}", url))?;

    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(false);
    }

    if !response.status().is_success() {
        return Err(anyhow::anyhow!(
            "Failed to download {}: HTTP {}",
            dest.file_name().and_then(|n| n.to_str()).unwrap_or("file"),
            response.status()
        ));
    }

    let temp_path = dest.with_extension("downloading");
    let filename = dest.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    let total_size = response.content_length().unwrap_or(0);
    let mut file = tokio::fs::File::create(&temp_path)
        .await
        .context(format!("Failed to create temp file at {:?}", temp_path))?;

    if progress_callback.is_none() {
        println!("Downloading {}...", filename);
    }

    let mut stream = response.bytes_stream();
    let mut downloaded: u64 = 0;

    loop {
        let Some(item) = stream.next().await else {
            break;
        };

        if cancel_token
            .map(|t| t.load(std::sync::atomic::Ordering::Relaxed))
            .unwrap_or(false)
        {
            drop(file);
            let _ = tokio::fs::remove_file(&temp_path).await;
            return Err(anyhow::anyhow!("Download cancelled"));
        }

        let chunk = item.context("Error while downloading file")?;
        file.write_all(&chunk)
            .await
            .context("Failed to write chunk to file")?;

        downloaded += chunk.len() as u64;
        if total_size > 0 {
            let progress = downloaded as f64 / total_size as f64;
            if let Some(cb) = &progress_callback {
                cb(progress);
            } else {
                print!(
                    "\rDownloading {}... {:.1}% ({}/{} bytes)",
                    filename,
                    progress * 100.0,
                    downloaded,
                    total_size
                );
                std::io::stdout().flush()?;
            }
        }
    }

    if progress_callback.is_none() {
        if total_size > 0 {
            println!(
                "\rDownload complete: {}/{} bytes (100%)    ",
                downloaded, total_size
            );
        } else {
            println!("\rDownload complete: {} bytes", downloaded);
        }
    }

    drop(file);
    fs::rename(&temp_path, dest).context(format!(
        "Failed to rename downloaded file from {:?} to {:?}",
        temp_path, dest
    ))?;

    Ok(true)
}

/// Helper function to generate a helpful error message for missing Parakeet models
fn parakeet_not_found_error(asr_dir: &Path) -> anyhow::Error {
    anyhow::anyhow!(
        "Parakeet model download failed or incomplete.\n\n\
         Automatic download attempted from:\n  {}\n\n\
         Expected model files in:\n  {}\n\n\
         Required files (one of each group):\n  \
         - a matching encoder/model sidecar pair:\n    \
           encoder-model.onnx + encoder-model.onnx.data OR\n    \
           encoder.onnx + encoder.onnx.data OR\n    \
           encoder_model.onnx + encoder_model.onnx.data\n  \
         - decoder_joint-model.onnx OR decoder_joint.onnx OR decoder_joint_model.onnx\n  \
         - vocab.txt\n  \
         - nemo128.onnx OR nemo80.onnx\n\n\
         If automatic download fails, ensure network access or place the files manually.",
        PARAKEET_ONNX_URL,
        asr_dir.display()
    )
}

/// Ensure all required models exist, downloading if necessary
///
/// Checks if all model files exist in the given directory. If any are missing,
/// downloads them from GitHub (Silero VAD) and HuggingFace (Moonshine).
///
/// Uses blocking runtime to handle async downloads from sync context.
pub fn ensure_models_exist(model_dir: &Path, variant: AsrModelId) -> Result<ModelPaths> {
    ensure_models_exist_with_progress(model_dir, variant, None, None)
}

/// Like `ensure_models_exist`, but accepts an optional progress callback (0.0..1.0)
/// for reporting download progress to the UI instead of printing to stdout.
pub fn ensure_models_exist_with_progress(
    model_dir: &Path,
    variant: AsrModelId,
    progress_callback: Option<Box<dyn Fn(f64) + Send + Sync>>,
    cancel_token: Option<&std::sync::atomic::AtomicBool>,
) -> Result<ModelPaths> {
    let silero_vad_path = model_dir.join("silero_vad.onnx");
    let rt = tokio::runtime::Runtime::new().context("Failed to create tokio runtime")?;
    let cb_ref = progress_callback.as_deref();

    // Silero VAD is small and shared across ASR models, so we always ensure it exists.
    if !silero_vad_path.exists() {
        rt.block_on(async {
            download_file(SILERO_VAD_URL, &silero_vad_path, cb_ref, cancel_token).await
        })
        .context("Failed to download Silero VAD model")?;
    }

    if variant.is_moonshine() {
        let asr_dir = model_dir.join(variant.dir_name());
        let encoder = asr_dir.join("encoder_model.onnx");
        let decoder = asr_dir.join("decoder_model_merged.onnx");
        let tokenizer = asr_dir.join("tokenizer.json");

        let all_exist = encoder.exists() && decoder.exists() && tokenizer.exists();
        if all_exist {
            if progress_callback.is_none() {
                println!("All models found at {:?}", model_dir);
            }
            return Ok(ModelPaths {
                silero_vad: silero_vad_path,
                asr_dir,
            });
        }

        if !asr_dir.exists() {
            fs::create_dir_all(&asr_dir).context("Failed to create Moonshine model directory")?;
        }

        let onnx_url = moonshine_onnx_url(variant)?;
        for filename in MOONSHINE_ONNX_FILES.iter() {
            let file_path = asr_dir.join(filename);
            if !file_path.exists() {
                let url = format!("{}/{}", onnx_url, filename);
                rt.block_on(async { download_file(&url, &file_path, cb_ref, cancel_token).await })
                    .context(format!("Failed to download Moonshine file: {}", filename))?;
            }
        }

        let variant_url = moonshine_variant_url(variant)?;
        for filename in MOONSHINE_VARIANT_FILES.iter() {
            let file_path = asr_dir.join(filename);
            if !file_path.exists() {
                let url = format!("{}/{}", variant_url, filename);
                rt.block_on(async { download_file(&url, &file_path, cb_ref, cancel_token).await })
                    .context(format!("Failed to download Moonshine file: {}", filename))?;
            }
        }

        if progress_callback.is_none() {
            println!("All models ready at {:?}", model_dir);
        }

        Ok(ModelPaths {
            silero_vad: silero_vad_path,
            asr_dir,
        })
    } else {
        match variant {
            AsrModelId::ParakeetTdt06bV3 => {
                // Ensure Parakeet artifacts exist (download if missing).
                let asr_dir = model_dir.join(variant.dir_name());

                std::fs::create_dir_all(&asr_dir).context(format!(
                    "Failed to create Parakeet model directory at {}",
                    asr_dir.display()
                ))?;

                if !any_matching_exists(&asr_dir, &PARAKEET_ENCODER_FILE_PAIRS) {
                    let encoder_dest = asr_dir.join("encoder-model.onnx");
                    if !encoder_dest.exists() {
                        let url = format!("{}/{}", PARAKEET_ONNX_URL, "encoder-model.onnx");
                        rt.block_on(async {
                            download_file(&url, &encoder_dest, cb_ref, cancel_token).await
                        })
                        .context("Failed to download Parakeet encoder")?;
                    }

                    let sidecar_dest = asr_dir.join("encoder-model.onnx.data");
                    if !sidecar_dest.exists() {
                        let url = format!("{}/{}", PARAKEET_ONNX_URL, "encoder-model.onnx.data");
                        rt.block_on(async {
                            download_file(&url, &sidecar_dest, cb_ref, cancel_token).await
                        })
                        .context("Failed to download Parakeet encoder external data")?;
                    }
                }

                if !any_exists(
                    &asr_dir,
                    &[
                        "decoder_joint-model.onnx",
                        "decoder_joint.onnx",
                        "decoder_joint_model.onnx",
                    ],
                ) {
                    let dest = asr_dir.join("decoder_joint-model.onnx");
                    let url = format!("{}/{}", PARAKEET_ONNX_URL, "decoder_joint-model.onnx");
                    rt.block_on(async { download_file(&url, &dest, cb_ref, cancel_token).await })
                        .context("Failed to download Parakeet decoder/joint")?;
                }

                if !asr_dir.join("vocab.txt").exists() {
                    let dest = asr_dir.join("vocab.txt");
                    let url = format!("{}/{}", PARAKEET_ONNX_URL, "vocab.txt");
                    rt.block_on(async { download_file(&url, &dest, cb_ref, cancel_token).await })
                        .context("Failed to download Parakeet vocab")?;
                }

                for filename in PARAKEET_OPTIONAL_FILES.iter() {
                    let dest = asr_dir.join(filename);
                    if !dest.exists() {
                        let url = format!("{}/{}", PARAKEET_ONNX_URL, filename);
                        match rt.block_on(async {
                            download_file_optional(&url, &dest, cb_ref, cancel_token).await
                        }) {
                            Ok(_) => {}
                            Err(e) => {
                                log::warn!(
                                    "Failed to download Parakeet optional file {}: {}",
                                    filename,
                                    e
                                );
                            }
                        }
                    }
                }

                if !any_exists(&asr_dir, &PARAKEET_NEMO_FILES) {
                    for filename in PARAKEET_NEMO_FILES.iter() {
                        let dest = asr_dir.join(filename);
                        let url = format!("{}/{}", PARAKEET_ONNX_URL, filename);
                        match rt.block_on(async {
                            download_file_optional(&url, &dest, cb_ref, cancel_token).await
                        }) {
                            Ok(downloaded) => {
                                if downloaded {
                                    break;
                                }
                            }
                            Err(e) => {
                                log::warn!(
                                    "Failed to download Parakeet preprocessor {}: {}",
                                    filename,
                                    e
                                );
                            }
                        }
                    }
                }

                if !any_matching_exists(&asr_dir, &PARAKEET_ENCODER_FILE_PAIRS)
                    || !any_exists(
                        &asr_dir,
                        &[
                            "decoder_joint-model.onnx",
                            "decoder_joint.onnx",
                            "decoder_joint_model.onnx",
                        ],
                    )
                    || !asr_dir.join("vocab.txt").exists()
                    || !any_exists(&asr_dir, &PARAKEET_NEMO_FILES)
                {
                    return Err(parakeet_not_found_error(&asr_dir));
                }

                Ok(ModelPaths {
                    silero_vad: silero_vad_path,
                    asr_dir,
                })
            }
            _ => unreachable!("non-Moonshine model should be handled above"),
        }
    }
}

/// Check if a specific model variant is fully downloaded (ONNX + tokenizer)
pub fn is_model_downloaded(model_dir: &Path, variant: AsrModelId) -> bool {
    let asr_dir = model_dir.join(variant.dir_name());
    if variant.is_moonshine() {
        let encoder = asr_dir.join("encoder_model.onnx");
        let decoder = asr_dir.join("decoder_model_merged.onnx");
        let tokenizer = asr_dir.join("tokenizer.json");
        encoder.exists() && decoder.exists() && tokenizer.exists()
    } else {
        match variant {
            AsrModelId::ParakeetTdt06bV3 => {
                if !asr_dir.exists() {
                    return false;
                }
                any_matching_exists(&asr_dir, &PARAKEET_ENCODER_FILE_PAIRS)
                    && any_exists(
                        &asr_dir,
                        &[
                            "decoder_joint-model.onnx",
                            "decoder_joint.onnx",
                            "decoder_joint_model.onnx",
                        ],
                    )
                    && asr_dir.join("vocab.txt").exists()
                    && any_exists(&asr_dir, &["nemo128.onnx", "nemo80.onnx"])
            }
            _ => unreachable!("non-Moonshine model should be handled above"),
        }
    }
}

/// List all available model variants in the model directory
pub fn available_models(model_dir: &Path) -> Vec<AsrModelId> {
    AsrModelId::all()
        .iter()
        .copied()
        .filter(|variant| is_model_downloaded(model_dir, *variant))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_paths_struct() {
        let model_dir = PathBuf::from("/tmp/models");
        let variant = AsrModelId::MoonshineBase;
        let paths = ModelPaths {
            silero_vad: model_dir.join("silero_vad.onnx"),
            asr_dir: model_dir.join(variant.dir_name()),
        };

        // Verify all fields are populated
        assert!(!paths.silero_vad.as_os_str().is_empty());
        assert!(!paths.asr_dir.as_os_str().is_empty());
    }

    #[test]
    fn test_atomic_write_pattern() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let dest = temp_dir.path().join("test_file.txt");
        let temp_path = dest.with_extension("downloading");

        // Simulate atomic write pattern
        fs::write(&temp_path, b"test content").expect("Failed to write temp file");
        assert!(temp_path.exists());
        assert!(!dest.exists());

        // Atomic rename
        fs::rename(&temp_path, &dest).expect("Failed to rename");
        assert!(!temp_path.exists());
        assert!(dest.exists());

        // Verify content
        let content = fs::read_to_string(&dest).expect("Failed to read file");
        assert_eq!(content, "test content");
    }

    #[test]
    fn test_moonshine_url_construction() {
        let base_onnx = moonshine_onnx_url(AsrModelId::MoonshineBase).unwrap();
        assert_eq!(
            base_onnx,
            "https://huggingface.co/UsefulSensors/moonshine/resolve/main/onnx/merged/base/float"
        );

        let tiny_onnx = moonshine_onnx_url(AsrModelId::MoonshineTiny).unwrap();
        assert_eq!(
            tiny_onnx,
            "https://huggingface.co/UsefulSensors/moonshine/resolve/main/onnx/merged/tiny/float"
        );

        let base_variant = moonshine_variant_url(AsrModelId::MoonshineBase).unwrap();
        assert_eq!(
            base_variant,
            "https://huggingface.co/UsefulSensors/moonshine-base/resolve/main"
        );

        let tiny_variant = moonshine_variant_url(AsrModelId::MoonshineTiny).unwrap();
        assert_eq!(
            tiny_variant,
            "https://huggingface.co/UsefulSensors/moonshine-tiny/resolve/main"
        );

        let tiny_japanese_onnx = moonshine_onnx_url(AsrModelId::MoonshineTinyJapanese).unwrap();
        assert_eq!(
            tiny_japanese_onnx,
            "https://huggingface.co/UsefulSensors/moonshine/resolve/main/onnx/merged/tiny-ja/float"
        );

        let tiny_japanese_variant =
            moonshine_variant_url(AsrModelId::MoonshineTinyJapanese).unwrap();
        assert_eq!(
            tiny_japanese_variant,
            "https://huggingface.co/UsefulSensors/moonshine-tiny-ja/resolve/main"
        );
    }

    #[test]
    fn test_is_model_downloaded() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let model_dir = temp_dir.path();

        // No models downloaded initially
        assert!(!is_model_downloaded(model_dir, AsrModelId::MoonshineBase));
        assert!(!is_model_downloaded(model_dir, AsrModelId::MoonshineTiny));
        assert!(!is_model_downloaded(
            model_dir,
            AsrModelId::MoonshineTinyJapanese
        ));

        // Create Base model files
        let base_dir = model_dir.join("moonshine-base");
        fs::create_dir_all(&base_dir).expect("Failed to create base dir");
        fs::write(base_dir.join("encoder_model.onnx"), b"").expect("Failed to write encoder");
        fs::write(base_dir.join("decoder_model_merged.onnx"), b"")
            .expect("Failed to write decoder");
        fs::write(base_dir.join("tokenizer.json"), b"{}").expect("Failed to write tokenizer");

        // Base should be detected
        assert!(is_model_downloaded(model_dir, AsrModelId::MoonshineBase));
        assert!(!is_model_downloaded(model_dir, AsrModelId::MoonshineTiny));

        // Create Tiny model files
        let tiny_dir = model_dir.join("moonshine-tiny");
        fs::create_dir_all(&tiny_dir).expect("Failed to create tiny dir");
        fs::write(tiny_dir.join("encoder_model.onnx"), b"").expect("Failed to write encoder");
        fs::write(tiny_dir.join("decoder_model_merged.onnx"), b"")
            .expect("Failed to write decoder");
        fs::write(tiny_dir.join("tokenizer.json"), b"{}").expect("Failed to write tokenizer");

        // Both should be detected
        assert!(is_model_downloaded(model_dir, AsrModelId::MoonshineBase));
        assert!(is_model_downloaded(model_dir, AsrModelId::MoonshineTiny));

        let tiny_japanese_dir = model_dir.join("moonshine-tiny-ja");
        fs::create_dir_all(&tiny_japanese_dir).expect("Failed to create tiny Japanese dir");
        fs::write(tiny_japanese_dir.join("encoder_model.onnx"), b"")
            .expect("Failed to write encoder");
        fs::write(tiny_japanese_dir.join("decoder_model_merged.onnx"), b"")
            .expect("Failed to write decoder");
        fs::write(tiny_japanese_dir.join("tokenizer.json"), b"{}")
            .expect("Failed to write tokenizer");

        assert!(is_model_downloaded(
            model_dir,
            AsrModelId::MoonshineTinyJapanese
        ));
    }

    #[test]
    fn test_available_models() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let model_dir = temp_dir.path();

        // No models available initially
        let available = available_models(model_dir);
        assert!(available.is_empty());

        // Create Base model
        let base_dir = model_dir.join("moonshine-base");
        fs::create_dir_all(&base_dir).expect("Failed to create base dir");
        fs::write(base_dir.join("encoder_model.onnx"), b"").expect("Failed to write encoder");
        fs::write(base_dir.join("decoder_model_merged.onnx"), b"")
            .expect("Failed to write decoder");
        fs::write(base_dir.join("tokenizer.json"), b"{}").expect("Failed to write tokenizer");

        let available = available_models(model_dir);
        assert_eq!(available.len(), 1);
        assert_eq!(available[0], AsrModelId::MoonshineBase);

        // Create Tiny model
        let tiny_dir = model_dir.join("moonshine-tiny");
        fs::create_dir_all(&tiny_dir).expect("Failed to create tiny dir");
        fs::write(tiny_dir.join("encoder_model.onnx"), b"").expect("Failed to write encoder");
        fs::write(tiny_dir.join("decoder_model_merged.onnx"), b"")
            .expect("Failed to write decoder");
        fs::write(tiny_dir.join("tokenizer.json"), b"{}").expect("Failed to write tokenizer");

        let available = available_models(model_dir);
        assert_eq!(available.len(), 2);
        assert_eq!(available[0], AsrModelId::MoonshineBase);
        assert_eq!(available[1], AsrModelId::MoonshineTiny);

        let tiny_japanese_dir = model_dir.join("moonshine-tiny-ja");
        fs::create_dir_all(&tiny_japanese_dir).expect("Failed to create tiny Japanese dir");
        fs::write(tiny_japanese_dir.join("encoder_model.onnx"), b"")
            .expect("Failed to write encoder");
        fs::write(tiny_japanese_dir.join("decoder_model_merged.onnx"), b"")
            .expect("Failed to write decoder");
        fs::write(tiny_japanese_dir.join("tokenizer.json"), b"{}")
            .expect("Failed to write tokenizer");

        let available = available_models(model_dir);
        assert_eq!(available.len(), 3);
        assert_eq!(available[2], AsrModelId::MoonshineTinyJapanese);
    }

    #[test]
    fn test_is_model_downloaded_requires_parakeet_encoder_sidecar() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let model_dir = temp_dir.path();
        let asr_dir = model_dir.join(AsrModelId::ParakeetTdt06bV3.dir_name());

        fs::create_dir_all(&asr_dir).expect("Failed to create Parakeet dir");
        fs::write(asr_dir.join("encoder-model.onnx"), b"encoder").expect("write encoder");
        fs::write(asr_dir.join("decoder_joint-model.onnx"), b"decoder").expect("write decoder");
        fs::write(asr_dir.join("vocab.txt"), b"vocab").expect("write vocab");
        fs::write(asr_dir.join("nemo128.onnx"), b"nemo").expect("write nemo");

        assert!(!is_model_downloaded(
            model_dir,
            AsrModelId::ParakeetTdt06bV3
        ));

        fs::write(asr_dir.join("encoder-model.onnx.data"), b"sidecar").expect("write sidecar");

        assert!(is_model_downloaded(model_dir, AsrModelId::ParakeetTdt06bV3));
    }

    #[test]
    fn test_is_model_downloaded_rejects_mismatched_parakeet_encoder_sidecar() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let model_dir = temp_dir.path();
        let asr_dir = model_dir.join(AsrModelId::ParakeetTdt06bV3.dir_name());

        fs::create_dir_all(&asr_dir).expect("Failed to create Parakeet dir");
        fs::write(asr_dir.join("encoder.onnx"), b"encoder").expect("write encoder");
        fs::write(asr_dir.join("encoder-model.onnx.data"), b"sidecar").expect("write sidecar");
        fs::write(asr_dir.join("decoder_joint-model.onnx"), b"decoder").expect("write decoder");
        fs::write(asr_dir.join("vocab.txt"), b"vocab").expect("write vocab");
        fs::write(asr_dir.join("nemo128.onnx"), b"nemo").expect("write nemo");

        assert!(!is_model_downloaded(
            model_dir,
            AsrModelId::ParakeetTdt06bV3
        ));
    }
}
