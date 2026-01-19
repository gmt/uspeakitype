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

/// Silero VAD model URL (GitHub)
const SILERO_VAD_URL: &str =
    "https://github.com/snakers4/silero-vad/raw/master/src/silero_vad/data/silero_vad.onnx";

/// Base URL for Moonshine models on HuggingFace
const MOONSHINE_BASE_URL: &str =
    "https://huggingface.co/UsefulSensors/moonshine/resolve/main/onnx/merged/moonshine-base-onnx/float";

/// Moonshine model files to download
const MOONSHINE_FILES: [&str; 4] = [
    "encoder_model.onnx",
    "decoder_model_merged.onnx",
    "tokenizer.json",
    "preprocessor_config.json",
];

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
async fn download_file(url: &str, dest: &Path) -> Result<()> {
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

    println!("Downloading {}...", filename);

    // Perform the download
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
        let chunk = item.context("Error while downloading file")?;
        file.write_all(&chunk)
            .await
            .context("Failed to write chunk to file")?;

        downloaded += chunk.len() as u64;
        if total_size > 0 {
            let progress = (downloaded as f64 / total_size as f64) * 100.0;
            print!(
                "\rDownloading {}... {:.1}% ({}/{} bytes)",
                filename, progress, downloaded, total_size
            );
            std::io::stdout().flush()?;
        }
    }

    if total_size > 0 {
        println!(
            "\rDownload complete: {}/{} bytes (100%)    ",
            downloaded, total_size
        );
    } else {
        println!("\rDownload complete: {} bytes", downloaded);
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
pub fn ensure_models_exist(model_dir: &Path) -> Result<ModelPaths> {
    // Build all model paths
    let silero_vad_path = model_dir.join("silero_vad.onnx");
    let moonshine_dir = model_dir.join("moonshine-base");
    let moonshine_encoder = moonshine_dir.join("encoder_model.onnx");
    let moonshine_decoder = moonshine_dir.join("decoder_model_merged.onnx");
    let moonshine_tokenizer = moonshine_dir.join("tokenizer.json");
    let moonshine_config = moonshine_dir.join("preprocessor_config.json");

    // Check if all files exist
    let all_exist = silero_vad_path.exists()
        && moonshine_encoder.exists()
        && moonshine_decoder.exists()
        && moonshine_tokenizer.exists()
        && moonshine_config.exists();

    if all_exist {
        println!("All models found at {:?}", model_dir);
        return Ok(ModelPaths {
            silero_vad: silero_vad_path,
            moonshine_dir,
            moonshine_encoder,
            moonshine_decoder,
            moonshine_tokenizer,
            moonshine_config,
        });
    }

    // Create moonshine directory if needed
    if !moonshine_dir.exists() {
        fs::create_dir_all(&moonshine_dir).context("Failed to create moonshine model directory")?;
    }

    // Download missing files using blocking runtime
    let rt = tokio::runtime::Runtime::new().context("Failed to create tokio runtime")?;

    // Download Silero VAD if missing
    if !silero_vad_path.exists() {
        rt.block_on(async { download_file(SILERO_VAD_URL, &silero_vad_path).await })
            .context("Failed to download Silero VAD model")?;
    }

    // Download Moonshine files if missing
    for filename in MOONSHINE_FILES.iter() {
        let file_path = moonshine_dir.join(filename);
        if !file_path.exists() {
            let url = format!("{}/{}", MOONSHINE_BASE_URL, filename);
            rt.block_on(async { download_file(&url, &file_path).await })
                .context(format!("Failed to download Moonshine file: {}", filename))?;
        }
    }

    println!("All models ready at {:?}", model_dir);

    Ok(ModelPaths {
        silero_vad: silero_vad_path,
        moonshine_dir,
        moonshine_encoder,
        moonshine_decoder,
        moonshine_tokenizer,
        moonshine_config,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_paths_struct() {
        let model_dir = PathBuf::from("/tmp/models");
        let paths = ModelPaths {
            silero_vad: model_dir.join("silero_vad.onnx"),
            moonshine_dir: model_dir.join("moonshine-base"),
            moonshine_encoder: model_dir.join("moonshine-base/encoder_model.onnx"),
            moonshine_decoder: model_dir.join("moonshine-base/decoder_model_merged.onnx"),
            moonshine_tokenizer: model_dir.join("moonshine-base/tokenizer.json"),
            moonshine_config: model_dir.join("moonshine-base/preprocessor_config.json"),
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
}
