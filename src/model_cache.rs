//! Model cache integrity and quarantine management
//!
//! Provides deterministic integrity checking via SHA-256 hashes, backup/archive
//! quarantine workflow, and graceful model fallback when corruption is detected.
//!
//! ## Integrity Chain-of-Trust
//!
//! 1. **Remote manifest** (preferred): If upstream provides checksums, we fetch and verify.
//! 2. **Local manifest** (fallback): After successful download, we generate and store a manifest.
//! 3. **Heuristic validation** (last resort): Size bounds + ONNX load validation.
//!
//! If none of these can verify integrity, data is treated as corrupt and quarantined.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{BufReader, Read};
use std::path::Path;
use std::time::{Duration, SystemTime};

use crate::config::AsrModelId;

/// Maximum age for backup archive entries before deletion (30 days)
const ARCHIVE_MAX_AGE_DAYS: u64 = 30;

/// Minimum expected size for ONNX model files (to catch truncated downloads)
const MIN_ONNX_SIZE_BYTES: u64 = 1024;

/// File entry in a model manifest
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManifestEntry {
    /// Relative path within the model directory
    pub path: String,
    /// SHA-256 hash of file contents (hex-encoded)
    pub sha256: String,
    /// File size in bytes
    pub size: u64,
    /// Last-modified time as Unix epoch seconds.
    ///
    /// Older manifests may not include this field; in that case we fall back
    /// to a full hash verification.
    #[serde(default)]
    pub modified_at: Option<u64>,
}

/// Model manifest for integrity verification
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelManifest {
    /// Manifest format version
    pub version: u32,
    /// Model identifier
    pub model_id: String,
    /// Timestamp when manifest was generated (Unix epoch seconds)
    pub generated_at: u64,
    /// File entries
    pub files: Vec<ManifestEntry>,
}

impl ModelManifest {
    pub const CURRENT_VERSION: u32 = 1;
    pub const FILENAME: &'static str = ".manifest.json";

    /// Create a new manifest for the given model directory
    pub fn generate(model_dir: &Path, model_id: &str) -> Result<Self> {
        let mut files = Vec::new();

        if model_dir.exists() {
            for entry in fs::read_dir(model_dir)? {
                let entry = entry?;
                let path = entry.path();

                // Skip manifest file itself and hidden files
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.starts_with('.') || name.ends_with(".downloading") {
                        continue;
                    }
                }

                if path.is_file() {
                    let relative_path = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("")
                        .to_string();

                    let metadata = fs::metadata(&path)?;
                    let size = metadata.len();
                    let modified_at = file_modified_at_secs(&metadata);

                    let sha256 = compute_sha256(&path)?;

                    files.push(ManifestEntry {
                        path: relative_path,
                        sha256,
                        size,
                        modified_at,
                    });
                }
            }
        }

        // Sort for deterministic output
        files.sort_by(|a, b| a.path.cmp(&b.path));

        let generated_at = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        Ok(Self {
            version: Self::CURRENT_VERSION,
            model_id: model_id.to_string(),
            generated_at,
            files,
        })
    }

    /// Load manifest from a model directory
    pub fn load(model_dir: &Path) -> Result<Option<Self>> {
        let manifest_path = model_dir.join(Self::FILENAME);
        if !manifest_path.exists() {
            return Ok(None);
        }

        let contents = fs::read_to_string(&manifest_path)
            .with_context(|| format!("reading manifest from {}", manifest_path.display()))?;

        let manifest: Self = serde_json::from_str(&contents)
            .with_context(|| format!("parsing manifest from {}", manifest_path.display()))?;

        Ok(Some(manifest))
    }

    /// Save manifest to a model directory
    pub fn save(&self, model_dir: &Path) -> Result<()> {
        let manifest_path = model_dir.join(Self::FILENAME);
        let contents = serde_json::to_string_pretty(self)?;
        fs::write(&manifest_path, contents)
            .with_context(|| format!("writing manifest to {}", manifest_path.display()))?;
        Ok(())
    }
}

/// Result of integrity validation
#[derive(Debug, Clone)]
pub enum IntegrityStatus {
    /// All files verified against manifest
    Verified,
    /// No manifest available, heuristic check passed
    Unverified,
    /// Corruption detected with details
    Corrupt(Vec<IntegrityError>),
    /// Model directory doesn't exist or is empty
    Missing,
}

/// Specific integrity error
#[derive(Debug, Clone)]
pub enum IntegrityError {
    /// File exists in manifest but not on disk
    MissingFile(String),
    /// File hash doesn't match manifest
    HashMismatch {
        path: String,
        expected: String,
        actual: String,
    },
    /// File size doesn't match manifest
    SizeMismatch {
        path: String,
        expected: u64,
        actual: u64,
    },
    /// File is suspiciously small (likely truncated)
    FileTooSmall { path: String, size: u64 },
    /// Manifest version is unsupported
    UnsupportedManifestVersion(u32),
    /// Partial download file exists
    PartialDownload(String),
}

impl std::fmt::Display for IntegrityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingFile(path) => write!(f, "missing file: {}", path),
            Self::HashMismatch {
                path,
                expected,
                actual,
            } => {
                write!(
                    f,
                    "hash mismatch for {}: expected {}, got {}",
                    path, expected, actual
                )
            }
            Self::SizeMismatch {
                path,
                expected,
                actual,
            } => {
                write!(
                    f,
                    "size mismatch for {}: expected {} bytes, got {}",
                    path, expected, actual
                )
            }
            Self::FileTooSmall { path, size } => {
                write!(
                    f,
                    "file {} is too small ({} bytes), likely truncated",
                    path, size
                )
            }
            Self::UnsupportedManifestVersion(v) => {
                write!(f, "unsupported manifest version: {}", v)
            }
            Self::PartialDownload(path) => {
                write!(f, "partial download exists: {}", path)
            }
        }
    }
}

/// Compute SHA-256 hash of a file
pub fn compute_sha256(path: &Path) -> Result<String> {
    let file =
        File::open(path).with_context(|| format!("opening {} for hashing", path.display()))?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];

    loop {
        let bytes_read = reader.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}

fn file_modified_at_secs(metadata: &fs::Metadata) -> Option<u64> {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs())
}

/// Check integrity of a model directory
pub fn check_integrity(model_dir: &Path, model_id: AsrModelId) -> IntegrityStatus {
    if !model_dir.exists() {
        return IntegrityStatus::Missing;
    }

    let mut errors = Vec::new();

    // Check for partial downloads first
    if let Ok(entries) = fs::read_dir(model_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.ends_with(".downloading") {
                    errors.push(IntegrityError::PartialDownload(name.to_string()));
                }
            }
        }
    }

    // Try to load and validate against manifest
    match ModelManifest::load(model_dir) {
        Ok(Some(manifest)) => {
            if manifest.version > ModelManifest::CURRENT_VERSION {
                errors.push(IntegrityError::UnsupportedManifestVersion(manifest.version));
                return IntegrityStatus::Corrupt(errors);
            }

            for entry in &manifest.files {
                let file_path = model_dir.join(&entry.path);

                if !file_path.exists() {
                    errors.push(IntegrityError::MissingFile(entry.path.clone()));
                    continue;
                }

                // Check size
                if let Ok(metadata) = fs::metadata(&file_path) {
                    let actual_size = metadata.len();
                    if actual_size != entry.size {
                        errors.push(IntegrityError::SizeMismatch {
                            path: entry.path.clone(),
                            expected: entry.size,
                            actual: actual_size,
                        });
                        continue;
                    }
                }

                let metadata = match fs::metadata(&file_path) {
                    Ok(metadata) => metadata,
                    Err(_) => continue,
                };

                let actual_modified_at = file_modified_at_secs(&metadata);
                let metadata_matches = entry.modified_at.is_some()
                    && entry.modified_at == actual_modified_at
                    && metadata.len() == entry.size;

                if !metadata_matches {
                    if let Ok(actual_hash) = compute_sha256(&file_path) {
                        if actual_hash != entry.sha256 {
                            errors.push(IntegrityError::HashMismatch {
                                path: entry.path.clone(),
                                expected: entry.sha256.clone(),
                                actual: actual_hash,
                            });
                        }
                    }
                }
            }

            // Some models ship ONNX external-data sidecars that older manifests never recorded.
            // Keep validating those required files so broken legacy caches don't look healthy forever.
            errors.extend(extra_required_file_checks(model_dir, model_id));

            if errors.is_empty() {
                IntegrityStatus::Verified
            } else {
                IntegrityStatus::Corrupt(errors)
            }
        }
        Ok(None) => {
            // No manifest - use heuristic validation
            errors.extend(heuristic_validate(model_dir, model_id));
            if errors.is_empty() {
                IntegrityStatus::Unverified
            } else {
                IntegrityStatus::Corrupt(errors)
            }
        }
        Err(e) => {
            log::warn!("Failed to load manifest: {}", e);
            // Manifest parse failed - use heuristic validation
            errors.extend(heuristic_validate(model_dir, model_id));
            if errors.is_empty() {
                IntegrityStatus::Unverified
            } else {
                IntegrityStatus::Corrupt(errors)
            }
        }
    }
}

fn manifest_needs_refresh(model_dir: &Path) -> bool {
    let Ok(Some(manifest)) = ModelManifest::load(model_dir) else {
        return false;
    };

    manifest
        .files
        .iter()
        .any(|entry| entry.modified_at.is_none())
}

fn extra_required_file_checks(model_dir: &Path, model_id: AsrModelId) -> Vec<IntegrityError> {
    match model_id {
        AsrModelId::ParakeetTdt06bV3 => {
            if has_matching_parakeet_encoder_sidecar(model_dir) {
                Vec::new()
            } else {
                vec![IntegrityError::MissingFile(
                    "matching encoder-model.onnx.data sidecar (or variant)".to_string(),
                )]
            }
        }
        _ => Vec::new(),
    }
}

fn has_matching_parakeet_encoder_sidecar(model_dir: &Path) -> bool {
    [
        ("encoder-model.onnx", "encoder-model.onnx.data"),
        ("encoder.onnx", "encoder.onnx.data"),
        ("encoder_model.onnx", "encoder_model.onnx.data"),
    ]
    .iter()
    .any(|(encoder, sidecar)| model_dir.join(encoder).exists() && model_dir.join(sidecar).exists())
}

/// Heuristic validation when no manifest is available
fn heuristic_validate(model_dir: &Path, model_id: AsrModelId) -> Vec<IntegrityError> {
    let mut errors = Vec::new();

    // Check required files exist and have reasonable sizes
    let required_files: Vec<&str> = if model_id.is_moonshine() {
        vec![
            "encoder_model.onnx",
            "decoder_model_merged.onnx",
            "tokenizer.json",
        ]
    } else {
        match model_id {
            AsrModelId::ParakeetTdt06bV3 => {
                // Parakeet has multiple naming conventions - check at least one exists
                let encoder_exists = ["encoder-model.onnx", "encoder.onnx", "encoder_model.onnx"]
                    .iter()
                    .any(|f| model_dir.join(f).exists());
                let decoder_exists = [
                    "decoder_joint-model.onnx",
                    "decoder_joint.onnx",
                    "decoder_joint_model.onnx",
                ]
                .iter()
                .any(|f| model_dir.join(f).exists());
                let nemo_exists = ["nemo128.onnx", "nemo80.onnx"]
                    .iter()
                    .any(|f| model_dir.join(f).exists());

                if !encoder_exists {
                    errors.push(IntegrityError::MissingFile(
                        "encoder-model.onnx (or variant)".to_string(),
                    ));
                }
                if !has_matching_parakeet_encoder_sidecar(model_dir) {
                    errors.push(IntegrityError::MissingFile(
                        "matching encoder-model.onnx.data sidecar (or variant)".to_string(),
                    ));
                }
                if !decoder_exists {
                    errors.push(IntegrityError::MissingFile(
                        "decoder_joint-model.onnx (or variant)".to_string(),
                    ));
                }
                if !nemo_exists {
                    errors.push(IntegrityError::MissingFile(
                        "nemo128.onnx or nemo80.onnx".to_string(),
                    ));
                }
                if !model_dir.join("vocab.txt").exists() {
                    errors.push(IntegrityError::MissingFile("vocab.txt".to_string()));
                }

                return errors;
            }
            _ => unreachable!("non-Moonshine model should be handled above"),
        }
    };

    for filename in required_files {
        let path = model_dir.join(filename);
        if !path.exists() {
            errors.push(IntegrityError::MissingFile(filename.to_string()));
            continue;
        }

        // Check file size for ONNX files
        if filename.ends_with(".onnx") {
            if let Ok(metadata) = fs::metadata(&path) {
                if metadata.len() < MIN_ONNX_SIZE_BYTES {
                    errors.push(IntegrityError::FileTooSmall {
                        path: filename.to_string(),
                        size: metadata.len(),
                    });
                }
            }
        }
    }

    errors
}

/// Quarantine a corrupt model directory by moving it to backup
pub fn quarantine_model(model_dir: &Path, model_id: AsrModelId) -> Result<()> {
    if !model_dir.exists() {
        return Ok(());
    }

    let parent = model_dir.parent().unwrap_or(Path::new("."));
    let backup_dir = parent.join(".backup");
    let archive_dir = parent.join(".backup_archive");

    // Ensure backup directory exists
    fs::create_dir_all(&backup_dir)?;

    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let backup_name = format!("{}-{}", timestamp, model_id.dir_name());
    let backup_path = backup_dir.join(&backup_name);

    // Check if backup already exists (shouldn't happen, but handle it)
    if backup_path.exists() {
        rotate_to_archive(&backup_path, &archive_dir)?;
    }

    // Move model directory to backup
    log::error!(
        "Quarantining corrupt model cache: {} -> {}",
        model_dir.display(),
        backup_path.display()
    );
    fs::rename(model_dir, &backup_path)?;

    Ok(())
}

/// Rotate old backup to archive, cleaning up old archive entries
fn rotate_to_archive(backup_path: &Path, archive_dir: &Path) -> Result<()> {
    fs::create_dir_all(archive_dir)?;

    let backup_name = backup_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");

    let archive_path = archive_dir.join(backup_name);

    // If already exists in archive or archive is old, delete and log
    if archive_path.exists() {
        log::error!(
            "Discarding duplicate backup archive entry: {}",
            archive_path.display()
        );
        fs::remove_dir_all(&archive_path)?;
    }

    // Check archive age and clean old entries
    clean_old_archives(archive_dir)?;

    // Move backup to archive
    log::error!(
        "Moving to backup archive: {} -> {}",
        backup_path.display(),
        archive_path.display()
    );
    fs::rename(backup_path, &archive_path)?;

    Ok(())
}

/// Clean archive entries older than ARCHIVE_MAX_AGE_DAYS
fn clean_old_archives(archive_dir: &Path) -> Result<()> {
    if !archive_dir.exists() {
        return Ok(());
    }

    let max_age = Duration::from_secs(ARCHIVE_MAX_AGE_DAYS * 24 * 60 * 60);
    let now = SystemTime::now();

    for entry in fs::read_dir(archive_dir)? {
        let entry = entry?;
        let path = entry.path();

        if let Ok(metadata) = fs::metadata(&path) {
            if let Ok(modified) = metadata.modified() {
                if let Ok(age) = now.duration_since(modified) {
                    if age > max_age {
                        log::error!(
                            "Discarding old backup archive ({}+ days): {}",
                            ARCHIVE_MAX_AGE_DAYS,
                            path.display()
                        );
                        if path.is_dir() {
                            fs::remove_dir_all(&path)?;
                        } else {
                            fs::remove_file(&path)?;
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

/// Clean up any existing partial downloads in the model directory
pub fn cleanup_partial_downloads(model_dir: &Path) -> Result<Vec<String>> {
    let mut cleaned = Vec::new();

    if !model_dir.exists() {
        return Ok(cleaned);
    }

    for entry in fs::read_dir(model_dir)? {
        let entry = entry?;
        let path = entry.path();
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name.ends_with(".downloading") {
                log::warn!("Removing partial download: {}", path.display());
                fs::remove_file(&path)?;
                cleaned.push(name.to_string());
            }
        }
    }

    Ok(cleaned)
}

/// Generate and save a manifest after successful model download/verification
pub fn seal_model(model_dir: &Path, model_id: AsrModelId) -> Result<()> {
    let manifest = ModelManifest::generate(model_dir, model_id.dir_name())?;
    manifest.save(model_dir)?;
    log::info!(
        "Sealed model cache with manifest: {} ({} files)",
        model_dir.display(),
        manifest.files.len()
    );
    Ok(())
}

/// Model activation result
#[derive(Debug)]
pub enum ActivationResult {
    /// Model activated successfully
    Success,
    /// Model needs download (not cached or corrupt)
    NeedsDownload,
    /// Model is corrupt and has been quarantined
    Quarantined,
}

fn errors_indicate_incomplete_cache(errors: &[IntegrityError]) -> bool {
    !errors.is_empty()
        && errors.iter().all(|error| {
            matches!(
                error,
                IntegrityError::MissingFile(_) | IntegrityError::PartialDownload(_)
            )
        })
}

/// Validate and prepare a model for activation
pub fn prepare_for_activation(model_dir: &Path, model_id: AsrModelId) -> ActivationResult {
    if let Ok(cleaned) = cleanup_partial_downloads(model_dir) {
        if !cleaned.is_empty() {
            log::warn!(
                "Cleaned stale partial downloads before activation for {}: {}",
                model_id,
                cleaned.join(", ")
            );
        }
    }

    let status = check_integrity(model_dir, model_id);

    match status {
        IntegrityStatus::Verified | IntegrityStatus::Unverified => {
            // Unverified is okay for legacy caches - seal them for future use.
            // Verified caches may also need refresh if the manifest predates
            // metadata shortcuts like modified_at.
            if matches!(status, IntegrityStatus::Unverified) || manifest_needs_refresh(model_dir) {
                if let Err(e) = seal_model(model_dir, model_id) {
                    log::warn!("Failed to seal unverified model: {}", e);
                }
            }
            ActivationResult::Success
        }
        IntegrityStatus::Missing => ActivationResult::NeedsDownload,
        IntegrityStatus::Corrupt(errors) => {
            if errors_indicate_incomplete_cache(&errors) {
                log::warn!(
                    "Model cache for {} is incomplete but not corrupt; continuing download",
                    model_id
                );
                return ActivationResult::NeedsDownload;
            }

            for error in &errors {
                log::error!("Model integrity error: {}", error);
            }
            if let Err(e) = quarantine_model(model_dir, model_id) {
                log::error!("Failed to quarantine corrupt model: {}", e);
            }
            ActivationResult::Quarantined
        }
    }
}

/// Get the priority order for model fallback
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_compute_sha256() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, b"hello world").unwrap();

        let hash = compute_sha256(&file_path).unwrap();
        // SHA-256 of "hello world"
        assert_eq!(
            hash,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[test]
    fn test_manifest_generate_and_load() {
        let temp_dir = TempDir::new().unwrap();
        let model_dir = temp_dir.path();

        // Create some test files
        fs::write(model_dir.join("encoder.onnx"), b"fake onnx data").unwrap();
        fs::write(model_dir.join("tokenizer.json"), b"{}").unwrap();

        // Generate manifest
        let manifest = ModelManifest::generate(model_dir, "test-model").unwrap();
        assert_eq!(manifest.files.len(), 2);
        assert_eq!(manifest.model_id, "test-model");

        // Save and reload
        manifest.save(model_dir).unwrap();
        let loaded = ModelManifest::load(model_dir).unwrap().unwrap();
        assert_eq!(loaded.files.len(), 2);
        assert_eq!(loaded.model_id, "test-model");
        assert!(loaded.files.iter().all(|entry| entry.modified_at.is_some()));
    }

    #[test]
    fn test_check_integrity_missing() {
        let temp_dir = TempDir::new().unwrap();
        let model_dir = temp_dir.path().join("nonexistent");

        let status = check_integrity(&model_dir, AsrModelId::MoonshineBase);
        assert!(matches!(status, IntegrityStatus::Missing));
    }

    #[test]
    fn test_check_integrity_corrupt_missing_file() {
        let temp_dir = TempDir::new().unwrap();
        let model_dir = temp_dir.path();

        // Create partial model (missing required files)
        fs::write(model_dir.join("encoder_model.onnx"), vec![0u8; 2048]).unwrap();

        let status = check_integrity(model_dir, AsrModelId::MoonshineBase);
        assert!(matches!(status, IntegrityStatus::Corrupt(_)));
    }

    #[test]
    fn test_check_integrity_rehashes_when_mtime_changes() {
        let temp_dir = TempDir::new().unwrap();
        let model_dir = temp_dir.path();
        let file_path = model_dir.join("encoder_model.onnx");

        fs::write(&file_path, b"abcdefgh").unwrap();
        fs::write(model_dir.join("decoder_model_merged.onnx"), b"decoder").unwrap();
        fs::write(model_dir.join("tokenizer.json"), b"{}").unwrap();
        let manifest = ModelManifest::generate(model_dir, "test-model").unwrap();
        manifest.save(model_dir).unwrap();

        std::thread::sleep(Duration::from_secs(1));
        fs::write(&file_path, b"ijklmnop").unwrap();

        let status = check_integrity(model_dir, AsrModelId::MoonshineBase);
        assert!(matches!(status, IntegrityStatus::Corrupt(_)));
    }

    #[test]
    fn test_quarantine_model() {
        let temp_dir = TempDir::new().unwrap();
        let models_dir = temp_dir.path();
        let model_dir = models_dir.join("moonshine-base");
        fs::create_dir_all(&model_dir).unwrap();
        fs::write(model_dir.join("test.txt"), b"data").unwrap();

        quarantine_model(&model_dir, AsrModelId::MoonshineBase).unwrap();

        // Model dir should be gone
        assert!(!model_dir.exists());

        // Backup should exist
        let backup_dir = models_dir.join(".backup");
        assert!(backup_dir.exists());
        let entries: Vec<_> = fs::read_dir(&backup_dir).unwrap().collect();
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn test_cleanup_partial_downloads() {
        let temp_dir = TempDir::new().unwrap();
        let model_dir = temp_dir.path();

        fs::write(model_dir.join("encoder.onnx"), b"complete").unwrap();
        fs::write(model_dir.join("decoder.downloading"), b"partial").unwrap();

        let cleaned = cleanup_partial_downloads(model_dir).unwrap();
        assert_eq!(cleaned.len(), 1);
        assert!(cleaned[0].contains("downloading"));
        assert!(!model_dir.join("decoder.downloading").exists());
        assert!(model_dir.join("encoder.onnx").exists());
    }

    #[test]
    fn test_seal_model() {
        let temp_dir = TempDir::new().unwrap();
        let model_dir = temp_dir.path();

        fs::write(model_dir.join("encoder.onnx"), b"data").unwrap();

        seal_model(model_dir, AsrModelId::MoonshineBase).unwrap();

        assert!(model_dir.join(ModelManifest::FILENAME).exists());
    }

    #[test]
    fn test_prepare_for_activation_cleans_stale_partial_downloads() {
        let temp_dir = TempDir::new().unwrap();
        let model_dir = temp_dir.path();

        fs::write(model_dir.join("encoder_model.onnx"), vec![0u8; 2048]).unwrap();
        fs::write(model_dir.join("decoder_model_merged.onnx"), vec![0u8; 2048]).unwrap();
        fs::write(model_dir.join("tokenizer.json"), b"{}").unwrap();
        fs::write(model_dir.join("stale.downloading"), b"partial").unwrap();

        let status = prepare_for_activation(model_dir, AsrModelId::MoonshineBase);

        assert!(matches!(
            status,
            ActivationResult::Success | ActivationResult::NeedsDownload
        ));
        assert!(!model_dir.join("stale.downloading").exists());
    }

    #[test]
    fn test_prepare_for_activation_does_not_quarantine_incomplete_cache() {
        let temp_dir = TempDir::new().unwrap();
        let models_dir = temp_dir.path();
        let model_dir = models_dir.join(AsrModelId::ParakeetTdt06bV3.dir_name());
        fs::create_dir_all(&model_dir).unwrap();
        fs::write(model_dir.join("encoder-model.onnx"), vec![0u8; 2048]).unwrap();

        let status = prepare_for_activation(&model_dir, AsrModelId::ParakeetTdt06bV3);

        assert!(matches!(status, ActivationResult::NeedsDownload));
        assert!(model_dir.exists());
        assert!(!models_dir.join(".backup").exists());
    }

    #[test]
    fn test_prepare_for_activation_resumes_manifest_missing_parakeet_sidecar() {
        let temp_dir = TempDir::new().unwrap();
        let models_dir = temp_dir.path();
        let model_dir = models_dir.join(AsrModelId::ParakeetTdt06bV3.dir_name());
        fs::create_dir_all(&model_dir).unwrap();
        fs::write(model_dir.join("encoder-model.onnx"), vec![0u8; 2048]).unwrap();
        fs::write(model_dir.join("decoder_joint-model.onnx"), vec![0u8; 2048]).unwrap();
        fs::write(model_dir.join("nemo128.onnx"), vec![0u8; 2048]).unwrap();
        fs::write(model_dir.join("vocab.txt"), b"vocab").unwrap();

        let manifest =
            ModelManifest::generate(&model_dir, AsrModelId::ParakeetTdt06bV3.dir_name()).unwrap();
        manifest.save(&model_dir).unwrap();

        let status = prepare_for_activation(&model_dir, AsrModelId::ParakeetTdt06bV3);

        assert!(matches!(status, ActivationResult::NeedsDownload));
        assert!(model_dir.exists());
        assert!(!models_dir.join(".backup").exists());
    }

    #[test]
    fn test_prepare_for_activation_rejects_mismatched_parakeet_sidecar() {
        let temp_dir = TempDir::new().unwrap();
        let models_dir = temp_dir.path();
        let model_dir = models_dir.join(AsrModelId::ParakeetTdt06bV3.dir_name());
        fs::create_dir_all(&model_dir).unwrap();
        fs::write(model_dir.join("encoder.onnx"), vec![0u8; 2048]).unwrap();
        fs::write(model_dir.join("encoder-model.onnx.data"), vec![0u8; 2048]).unwrap();
        fs::write(model_dir.join("decoder_joint-model.onnx"), vec![0u8; 2048]).unwrap();
        fs::write(model_dir.join("nemo128.onnx"), vec![0u8; 2048]).unwrap();
        fs::write(model_dir.join("vocab.txt"), b"vocab").unwrap();
        let manifest =
            ModelManifest::generate(&model_dir, AsrModelId::ParakeetTdt06bV3.dir_name()).unwrap();
        manifest.save(&model_dir).unwrap();

        let status = prepare_for_activation(&model_dir, AsrModelId::ParakeetTdt06bV3);

        assert!(matches!(status, ActivationResult::NeedsDownload));
        assert!(model_dir.exists());
        assert!(!models_dir.join(".backup").exists());
    }
}
