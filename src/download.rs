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

    while let Some(item) = stream.next().await {
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

/// Helper function to generate a helpful error message for missing Parakeet models
fn parakeet_not_found_error(asr_dir: &Path) -> anyhow::Error {
    anyhow::anyhow!(
        "Parakeet model not found.\n\n\
         Please download model files to:\n  {}\n\n\
         Required files (one of each group):\n  \
         - encoder-model.onnx OR encoder.onnx OR encoder_model.onnx\n  \
         - decoder_joint-model.onnx OR decoder_joint.onnx OR decoder_joint_model.onnx\n  \
         - vocab.txt\n  \
         - nemo128.onnx OR nemo80.onnx\n\n\
         Download from: https://huggingface.co/nvidia/parakeet-tdt-0.6b\n\
         Export to ONNX using the NeMo toolkit.",
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

    match variant {
        AsrModelId::MoonshineBase | AsrModelId::MoonshineTiny => {
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
                fs::create_dir_all(&asr_dir)
                    .context("Failed to create Moonshine model directory")?;
            }

            let onnx_url = moonshine_onnx_url(variant)?;
            for filename in MOONSHINE_ONNX_FILES.iter() {
                let file_path = asr_dir.join(filename);
                if !file_path.exists() {
                    let url = format!("{}/{}", onnx_url, filename);
                    rt.block_on(async {
                        download_file(&url, &file_path, cb_ref, cancel_token).await
                    })
                    .context(format!("Failed to download Moonshine file: {}", filename))?;
                }
            }

            let variant_url = moonshine_variant_url(variant)?;
            for filename in MOONSHINE_VARIANT_FILES.iter() {
                let file_path = asr_dir.join(filename);
                if !file_path.exists() {
                    let url = format!("{}/{}", variant_url, filename);
                    rt.block_on(async {
                        download_file(&url, &file_path, cb_ref, cancel_token).await
                    })
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
        }
        AsrModelId::ParakeetTdt06bV3 => {
            // Phase 1 (local-dir-first): validate that the required ONNX artifacts exist.
            let asr_dir = model_dir.join(variant.dir_name());

            // Create directory if missing so user knows exact path
            std::fs::create_dir_all(&asr_dir).ok();

            if !asr_dir.exists() {
                return Err(parakeet_not_found_error(&asr_dir));
            }

            let required_any =
                |candidates: &[&str]| candidates.iter().any(|f| asr_dir.join(f).exists());
            if !required_any(&["encoder-model.onnx", "encoder.onnx", "encoder_model.onnx"]) {
                return Err(parakeet_not_found_error(&asr_dir));
            }
            if !required_any(&[
                "decoder_joint-model.onnx",
                "decoder_joint.onnx",
                "decoder_joint_model.onnx",
            ]) {
                return Err(parakeet_not_found_error(&asr_dir));
            }
            if !asr_dir.join("vocab.txt").exists() {
                return Err(parakeet_not_found_error(&asr_dir));
            }
            if !required_any(&["nemo128.onnx", "nemo80.onnx"]) {
                return Err(parakeet_not_found_error(&asr_dir));
            }

            Ok(ModelPaths {
                silero_vad: silero_vad_path,
                asr_dir,
            })
        }
    }
}

/// Check if a specific model variant is fully downloaded (ONNX + tokenizer)
pub fn is_model_downloaded(model_dir: &Path, variant: AsrModelId) -> bool {
    let asr_dir = model_dir.join(variant.dir_name());
    match variant {
        AsrModelId::MoonshineBase | AsrModelId::MoonshineTiny => {
            let encoder = asr_dir.join("encoder_model.onnx");
            let decoder = asr_dir.join("decoder_model_merged.onnx");
            let tokenizer = asr_dir.join("tokenizer.json");
            encoder.exists() && decoder.exists() && tokenizer.exists()
        }
        AsrModelId::ParakeetTdt06bV3 => {
            if !asr_dir.exists() {
                return false;
            }
            let required_any =
                |candidates: &[&str]| candidates.iter().any(|f| asr_dir.join(f).exists());
            required_any(&["encoder-model.onnx", "encoder.onnx", "encoder_model.onnx"])
                && required_any(&[
                    "decoder_joint-model.onnx",
                    "decoder_joint.onnx",
                    "decoder_joint_model.onnx",
                ])
                && asr_dir.join("vocab.txt").exists()
                && required_any(&["nemo128.onnx", "nemo80.onnx"])
        }
    }
}

/// List all available model variants in the model directory
pub fn available_models(model_dir: &Path) -> Vec<AsrModelId> {
    let mut available = Vec::new();

    if is_model_downloaded(model_dir, AsrModelId::MoonshineBase) {
        available.push(AsrModelId::MoonshineBase);
    }
    if is_model_downloaded(model_dir, AsrModelId::MoonshineTiny) {
        available.push(AsrModelId::MoonshineTiny);
    }
    if is_model_downloaded(model_dir, AsrModelId::ParakeetTdt06bV3) {
        available.push(AsrModelId::ParakeetTdt06bV3);
    }

    available
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
    }

    #[test]
    fn test_is_model_downloaded() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let model_dir = temp_dir.path();

        // No models downloaded initially
        assert!(!is_model_downloaded(model_dir, AsrModelId::MoonshineBase));
        assert!(!is_model_downloaded(model_dir, AsrModelId::MoonshineTiny));

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
    }
}
