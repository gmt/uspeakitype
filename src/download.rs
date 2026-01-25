//! Model download infrastructure for Barbara
//!
//! Handles automatic downloading of Silero VAD and Moonshine models from GitHub and HuggingFace.
//! Uses atomic write pattern (temp file → rename) to ensure partial downloads don't corrupt files.

use anyhow::{Context, Result};
use futures_util::StreamExt;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;

use crate::config::ModelVariant;

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
fn moonshine_onnx_url(variant: ModelVariant) -> String {
    let variant_name = match variant {
        ModelVariant::MoonshineBase => "base",
        ModelVariant::MoonshineTiny => "tiny",
    };
    format!(
        "https://huggingface.co/UsefulSensors/moonshine/resolve/main/onnx/merged/{}/float",
        variant_name
    )
}

/// Construct HuggingFace URL for variant-specific files (tokenizer, config)
///
/// Returns the base URL for downloading support files from the variant-specific repo.
/// URL pattern: `https://huggingface.co/UsefulSensors/moonshine-{variant}/resolve/main`
fn moonshine_variant_url(variant: ModelVariant) -> String {
    let variant_name = match variant {
        ModelVariant::MoonshineBase => "base",
        ModelVariant::MoonshineTiny => "tiny",
    };
    format!(
        "https://huggingface.co/UsefulSensors/moonshine-{}/resolve/main",
        variant_name
    )
}

/// Paths to all required model files
#[derive(Debug, Clone)]
pub struct ModelPaths {
    /// Path to Silero VAD model
    pub silero_vad: PathBuf,
    /// Directory containing Moonshine models
    pub moonshine_dir: PathBuf,
    /// Path to Moonshine encoder model
    pub moonshine_encoder: PathBuf,
    /// Path to Moonshine decoder model
    pub moonshine_decoder: PathBuf,
    /// Path to Moonshine tokenizer
    pub moonshine_tokenizer: PathBuf,
    /// Path to Moonshine preprocessor config
    pub moonshine_config: PathBuf,
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

/// Ensure all required models exist, downloading if necessary
///
/// Checks if all model files exist in the given directory. If any are missing,
/// downloads them from GitHub (Silero VAD) and HuggingFace (Moonshine).
///
/// Uses blocking runtime to handle async downloads from sync context.
pub fn ensure_models_exist(model_dir: &Path, variant: ModelVariant) -> Result<ModelPaths> {
    ensure_models_exist_with_progress(model_dir, variant, None, None)
}

/// Like `ensure_models_exist`, but accepts an optional progress callback (0.0..1.0)
/// for reporting download progress to the UI instead of printing to stdout.
pub fn ensure_models_exist_with_progress(
    model_dir: &Path,
    variant: ModelVariant,
    progress_callback: Option<Box<dyn Fn(f64) + Send + Sync>>,
    cancel_token: Option<&std::sync::atomic::AtomicBool>,
) -> Result<ModelPaths> {
    let silero_vad_path = model_dir.join("silero_vad.onnx");
    let moonshine_dir = model_dir.join(variant.dir_name());
    let moonshine_encoder = moonshine_dir.join("encoder_model.onnx");
    let moonshine_decoder = moonshine_dir.join("decoder_model_merged.onnx");
    let moonshine_tokenizer = moonshine_dir.join("tokenizer.json");
    let moonshine_config = moonshine_dir.join("preprocessor_config.json");

    let all_exist = silero_vad_path.exists()
        && moonshine_encoder.exists()
        && moonshine_decoder.exists()
        && moonshine_tokenizer.exists();

    if all_exist {
        if progress_callback.is_none() {
            println!("All models found at {:?}", model_dir);
        }
        return Ok(ModelPaths {
            silero_vad: silero_vad_path,
            moonshine_dir,
            moonshine_encoder,
            moonshine_decoder,
            moonshine_tokenizer,
            moonshine_config,
        });
    }

    if !moonshine_dir.exists() {
        fs::create_dir_all(&moonshine_dir).context("Failed to create moonshine model directory")?;
    }

    let rt = tokio::runtime::Runtime::new().context("Failed to create tokio runtime")?;
    let cb_ref = progress_callback.as_deref();

    if !silero_vad_path.exists() {
        rt.block_on(async { download_file(SILERO_VAD_URL, &silero_vad_path, cb_ref, cancel_token).await })
            .context("Failed to download Silero VAD model")?;
    }

    let onnx_url = moonshine_onnx_url(variant);
    for filename in MOONSHINE_ONNX_FILES.iter() {
        let file_path = moonshine_dir.join(filename);
        if !file_path.exists() {
            let url = format!("{}/{}", onnx_url, filename);
            rt.block_on(async { download_file(&url, &file_path, cb_ref, cancel_token).await })
                .context(format!("Failed to download Moonshine file: {}", filename))?;
        }
    }

    let variant_url = moonshine_variant_url(variant);
    for filename in MOONSHINE_VARIANT_FILES.iter() {
        let file_path = moonshine_dir.join(filename);
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
        moonshine_dir,
        moonshine_encoder,
        moonshine_decoder,
        moonshine_tokenizer,
        moonshine_config,
    })
}

/// Check if a specific model variant is fully downloaded (ONNX + tokenizer)
pub fn is_model_downloaded(model_dir: &Path, variant: ModelVariant) -> bool {
    let moonshine_dir = model_dir.join(variant.dir_name());
    let encoder = moonshine_dir.join("encoder_model.onnx");
    let decoder = moonshine_dir.join("decoder_model_merged.onnx");
    let tokenizer = moonshine_dir.join("tokenizer.json");
    encoder.exists() && decoder.exists() && tokenizer.exists()
}

/// List all available model variants in the model directory
pub fn available_models(model_dir: &Path) -> Vec<ModelVariant> {
    let mut available = Vec::new();
    
    if is_model_downloaded(model_dir, ModelVariant::MoonshineBase) {
        available.push(ModelVariant::MoonshineBase);
    }
    if is_model_downloaded(model_dir, ModelVariant::MoonshineTiny) {
        available.push(ModelVariant::MoonshineTiny);
    }
    
    available
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_paths_struct() {
        let model_dir = PathBuf::from("/tmp/models");
        let variant = ModelVariant::MoonshineBase;
        let paths = ModelPaths {
            silero_vad: model_dir.join("silero_vad.onnx"),
            moonshine_dir: model_dir.join(variant.dir_name()),
            moonshine_encoder: model_dir.join(variant.dir_name()).join("encoder_model.onnx"),
            moonshine_decoder: model_dir.join(variant.dir_name()).join("decoder_model_merged.onnx"),
            moonshine_tokenizer: model_dir.join(variant.dir_name()).join("tokenizer.json"),
            moonshine_config: model_dir.join(variant.dir_name()).join("preprocessor_config.json"),
        };

        // Verify all fields are populated
        assert!(!paths.silero_vad.as_os_str().is_empty());
        assert!(!paths.moonshine_dir.as_os_str().is_empty());
        assert!(!paths.moonshine_encoder.as_os_str().is_empty());
        assert!(!paths.moonshine_decoder.as_os_str().is_empty());
        assert!(!paths.moonshine_tokenizer.as_os_str().is_empty());
        assert!(!paths.moonshine_config.as_os_str().is_empty());
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
        let base_onnx = moonshine_onnx_url(ModelVariant::MoonshineBase);
        assert_eq!(
            base_onnx,
            "https://huggingface.co/UsefulSensors/moonshine/resolve/main/onnx/merged/base/float"
        );

        let tiny_onnx = moonshine_onnx_url(ModelVariant::MoonshineTiny);
        assert_eq!(
            tiny_onnx,
            "https://huggingface.co/UsefulSensors/moonshine/resolve/main/onnx/merged/tiny/float"
        );

        let base_variant = moonshine_variant_url(ModelVariant::MoonshineBase);
        assert_eq!(
            base_variant,
            "https://huggingface.co/UsefulSensors/moonshine-base/resolve/main"
        );

        let tiny_variant = moonshine_variant_url(ModelVariant::MoonshineTiny);
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
        assert!(!is_model_downloaded(model_dir, ModelVariant::MoonshineBase));
        assert!(!is_model_downloaded(model_dir, ModelVariant::MoonshineTiny));

        // Create Base model files
        let base_dir = model_dir.join("moonshine-base");
        fs::create_dir_all(&base_dir).expect("Failed to create base dir");
        fs::write(base_dir.join("encoder_model.onnx"), b"").expect("Failed to write encoder");
        fs::write(base_dir.join("decoder_model_merged.onnx"), b"").expect("Failed to write decoder");
        fs::write(base_dir.join("tokenizer.json"), b"{}").expect("Failed to write tokenizer");

        // Base should be detected
        assert!(is_model_downloaded(model_dir, ModelVariant::MoonshineBase));
        assert!(!is_model_downloaded(model_dir, ModelVariant::MoonshineTiny));

        // Create Tiny model files
        let tiny_dir = model_dir.join("moonshine-tiny");
        fs::create_dir_all(&tiny_dir).expect("Failed to create tiny dir");
        fs::write(tiny_dir.join("encoder_model.onnx"), b"").expect("Failed to write encoder");
        fs::write(tiny_dir.join("decoder_model_merged.onnx"), b"").expect("Failed to write decoder");
        fs::write(tiny_dir.join("tokenizer.json"), b"{}").expect("Failed to write tokenizer");

        // Both should be detected
        assert!(is_model_downloaded(model_dir, ModelVariant::MoonshineBase));
        assert!(is_model_downloaded(model_dir, ModelVariant::MoonshineTiny));
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
        fs::write(base_dir.join("decoder_model_merged.onnx"), b"").expect("Failed to write decoder");
        fs::write(base_dir.join("tokenizer.json"), b"{}").expect("Failed to write tokenizer");

        let available = available_models(model_dir);
        assert_eq!(available.len(), 1);
        assert_eq!(available[0], ModelVariant::MoonshineBase);

        // Create Tiny model
        let tiny_dir = model_dir.join("moonshine-tiny");
        fs::create_dir_all(&tiny_dir).expect("Failed to create tiny dir");
        fs::write(tiny_dir.join("encoder_model.onnx"), b"").expect("Failed to write encoder");
        fs::write(tiny_dir.join("decoder_model_merged.onnx"), b"").expect("Failed to write decoder");
        fs::write(tiny_dir.join("tokenizer.json"), b"{}").expect("Failed to write tokenizer");

        let available = available_models(model_dir);
        assert_eq!(available.len(), 2);
        assert_eq!(available[0], ModelVariant::MoonshineBase);
        assert_eq!(available[1], ModelVariant::MoonshineTiny);
    }
}
