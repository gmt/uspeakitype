//! Configuration management for usit
//!
//! Handles TOML-based persistent configuration with atomic writes.
//! Supports partial TOML files with sensible defaults for missing fields.

use anyhow::{Context, Result};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::{Path, PathBuf};

/// Model variant selection
///
/// Moonshine has two drop-in compatible ONNX models: Base (~120MB) and Tiny (~100MB).
/// Both use the same encoder/decoder interface; only the model size differs.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum ModelVariant {
    /// Moonshine Base model (~120MB) - better accuracy
    #[default]
    #[value(name = "moonshine-base")]
    MoonshineBase,
    /// Moonshine Tiny model (~100MB) - faster, lower memory
    #[value(name = "moonshine-tiny")]
    MoonshineTiny,
}

impl fmt::Display for ModelVariant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ModelVariant::MoonshineBase => write!(f, "Moonshine Base"),
            ModelVariant::MoonshineTiny => write!(f, "Moonshine Tiny"),
        }
    }
}

impl ModelVariant {
    /// Directory name for model storage (e.g., "moonshine-base")
    pub fn dir_name(&self) -> &str {
        match self {
            ModelVariant::MoonshineBase => "moonshine-base",
            ModelVariant::MoonshineTiny => "moonshine-tiny",
        }
    }

    /// URL segment for HuggingFace download (e.g., "base" or "tiny")
    pub fn download_url_segment(&self) -> &str {
        match self {
            ModelVariant::MoonshineBase => "base",
            ModelVariant::MoonshineTiny => "tiny",
        }
    }
}

/// usit configuration
///
/// All fields support `#[serde(default)]` for partial TOML loading.
/// Missing fields use their Default implementations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    /// ASR model variant to use
    #[serde(default)]
    pub model: ModelVariant,

    /// Enable automatic gain control
    #[serde(default)]
    pub auto_gain: bool,

    /// Software gain multiplier (1.0 = no change)
    #[serde(default)]
    pub gain: f32,

    /// Spectrogram style ("bars" or "waterfall")
    #[serde(default)]
    pub style: String,

    /// Color scheme ("flame", "cool", etc.)
    #[serde(default)]
    pub color: String,

    /// Audio source device name (None = default)
    #[serde(default)]
    pub source: Option<String>,

    /// Enable text injection into focused window
    #[serde(default)]
    pub injection_enabled: bool,

    /// Auto-save config changes to file
    #[serde(default)]
    pub auto_save: bool,

    /// Directory containing model files
    #[serde(default)]
    pub model_dir: Option<PathBuf>,

    /// Window opacity (0.0-1.0, default: 0.85)
    #[serde(default = "default_opacity")]
    pub opacity: f32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            model: ModelVariant::MoonshineBase,
            auto_gain: false,
            gain: 1.0,
            style: "bars".to_string(),
            color: "flame".to_string(),
            source: None,
            injection_enabled: true,
            auto_save: true,
            model_dir: None,
            opacity: 0.85,
        }
    }
}

fn default_opacity() -> f32 {
    0.85
}

impl Config {
    /// Load configuration from TOML file
    ///
    /// Missing fields use defaults. Invalid fields are warned but don't crash.
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(path)
            .context(format!("Failed to read config file: {:?}", path))?;

        match toml::from_str::<Self>(&content) {
            Ok(config) => Ok(config),
            Err(e) => {
                log::warn!("Invalid config file {:?}: {}", path, e);
                log::warn!("Using defaults");
                Ok(Self::default())
            }
        }
    }

    /// Save configuration to TOML file with atomic write pattern
    ///
    /// Uses temp file (`.saving`) during write, then atomically renames to final path.
    pub fn save(&self, path: &Path) -> Result<()> {
        // Create parent directories if needed
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent).context("Failed to create config directories")?;
            }
        }

        // Temp file for atomic write pattern
        let temp_path = path.with_extension("saving");

        // Serialize to TOML
        let content = toml::to_string_pretty(self).context("Failed to serialize config to TOML")?;

        // Write to temp file
        std::fs::write(&temp_path, content)
            .context(format!("Failed to write temp config file: {:?}", temp_path))?;

        // Atomic rename: move temp file to final location
        std::fs::rename(&temp_path, path).context(format!(
            "Failed to rename config file from {:?} to {:?}",
            temp_path, path
        ))?;

        Ok(())
    }

    /// Get the default config file path
    ///
    /// Returns `~/.config/usit/usit.toml`
    pub fn config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("usit")
            .join("usit.toml")
    }

    /// Load config from default path, or return defaults if file doesn't exist
    pub fn load_or_default() -> Self {
        Self::load(&Self::config_path()).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.model, ModelVariant::MoonshineBase);
        assert!(!config.auto_gain);
        assert_eq!(config.gain, 1.0);
        assert_eq!(config.style, "bars");
        assert_eq!(config.color, "flame");
        assert!(config.injection_enabled);
        assert!(config.auto_save);
        assert_eq!(config.source, None);
        assert_eq!(config.model_dir, None);
    }

    #[test]
    fn test_config_round_trip() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("test.toml");

        let original = Config {
            model: ModelVariant::MoonshineTiny,
            auto_gain: true,
            gain: 2.5,
            style: "waterfall".to_string(),
            color: "cool".to_string(),
            source: Some("microphone".to_string()),
            injection_enabled: false,
            auto_save: false,
            model_dir: Some(PathBuf::from("/tmp/models")),
            opacity: 0.75,
        };

        // Save
        original.save(&config_path).unwrap();

        // Load
        let loaded = Config::load(&config_path).unwrap();

        // Verify all fields match
        assert_eq!(loaded.model, original.model);
        assert_eq!(loaded.auto_gain, original.auto_gain);
        assert_eq!(loaded.gain, original.gain);
        assert_eq!(loaded.style, original.style);
        assert_eq!(loaded.color, original.color);
        assert_eq!(loaded.source, original.source);
        assert_eq!(loaded.injection_enabled, original.injection_enabled);
        assert_eq!(loaded.auto_save, original.auto_save);
        assert_eq!(loaded.model_dir, original.model_dir);
    }

    #[test]
    fn test_partial_toml_loads() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("partial.toml");

        // Write partial TOML (only some fields)
        let partial_toml = r#"
model = "moonshine-tiny"
gain = 1.5
style = "waterfall"
color = "flame"
injection_enabled = true
auto_save = true
"#;
        std::fs::write(&config_path, partial_toml).unwrap();

        // Load should succeed with defaults for missing fields
        let config = Config::load(&config_path).unwrap();

        assert_eq!(config.model, ModelVariant::MoonshineTiny);
        assert_eq!(config.gain, 1.5);
        assert_eq!(config.style, "waterfall");
        // Explicitly set fields
        assert_eq!(config.color, "flame");
        assert!(config.injection_enabled);
        assert!(config.auto_save);
        // Missing fields use serde defaults (false, 0, empty string)
        assert!(!config.auto_gain);
    }

    #[test]
    fn test_empty_file_loads_defaults() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("empty.toml");

        // Write empty file
        std::fs::write(&config_path, "").unwrap();

        // Load should succeed - empty TOML deserializes with serde defaults
        // (which are 0, false, empty string for primitive types)
        let config = Config::load(&config_path).unwrap();
        // Just verify it loaded without error
        assert_eq!(config.model, ModelVariant::MoonshineBase);
    }

    #[test]
    fn test_missing_file_returns_defaults() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("nonexistent.toml");

        // Load non-existent file should return defaults
        let config = Config::load(&config_path).unwrap();
        assert_eq!(config, Config::default());
    }

    #[test]
    fn test_invalid_toml_warns_and_uses_defaults() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("invalid.toml");

        // Write invalid TOML
        std::fs::write(&config_path, "this is not valid toml [[[").unwrap();

        // Load should warn but not crash, returning defaults
        let config = Config::load(&config_path).unwrap();
        assert_eq!(config, Config::default());
    }

    #[test]
    fn test_invalid_field_value_warns_and_uses_defaults() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("invalid_field.toml");

        // Write TOML with invalid enum value
        let invalid_toml = r#"
model = "invalid_model"
gain = 1.5
"#;
        std::fs::write(&config_path, invalid_toml).unwrap();

        // Load should warn but not crash, returning defaults
        let config = Config::load(&config_path).unwrap();
        assert_eq!(config, Config::default());
    }

    #[test]
    fn test_config_path_ends_with_usit_toml() {
        let path = Config::config_path();
        assert!(path.ends_with("usit/usit.toml"));
    }

    #[test]
    fn test_atomic_write_creates_no_temp_file() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("atomic.toml");

        let config = Config::default();
        config.save(&config_path).unwrap();

        // Verify final file exists
        assert!(config_path.exists());

        // Verify no temp file left behind
        let temp_path = config_path.with_extension("saving");
        assert!(!temp_path.exists());
    }

    #[test]
    fn test_load_or_default_nonexistent() {
        // This test just verifies the method doesn't panic
        // It will use defaults since we can't control the actual config path
        let _config = Config::load_or_default();
        // If we got here without panic, test passes
    }

    #[test]
    fn test_model_variant_default() {
        assert_eq!(ModelVariant::default(), ModelVariant::MoonshineBase);
    }

    #[test]
    fn test_model_variant_display() {
        assert_eq!(ModelVariant::MoonshineBase.to_string(), "Moonshine Base");
        assert_eq!(ModelVariant::MoonshineTiny.to_string(), "Moonshine Tiny");
    }

    #[test]
    fn test_model_variant_dir_name() {
        assert_eq!(ModelVariant::MoonshineBase.dir_name(), "moonshine-base");
        assert_eq!(ModelVariant::MoonshineTiny.dir_name(), "moonshine-tiny");
    }

    #[test]
    fn test_model_variant_download_url_segment() {
        assert_eq!(ModelVariant::MoonshineBase.download_url_segment(), "base");
        assert_eq!(ModelVariant::MoonshineTiny.download_url_segment(), "tiny");
    }

    #[test]
    fn test_model_variant_serialization() {
        let base = ModelVariant::MoonshineBase;
        let tiny = ModelVariant::MoonshineTiny;

        let base_str = serde_json::to_string(&base).unwrap();
        let tiny_str = serde_json::to_string(&tiny).unwrap();

        assert!(base_str.contains("moonshine-base"));
        assert!(tiny_str.contains("moonshine-tiny"));
    }

    #[test]
    fn test_model_variant_deserialization() {
        let base: ModelVariant = serde_json::from_str("\"moonshine-base\"").unwrap();
        let tiny: ModelVariant = serde_json::from_str("\"moonshine-tiny\"").unwrap();

        assert_eq!(base, ModelVariant::MoonshineBase);
        assert_eq!(tiny, ModelVariant::MoonshineTiny);
    }

    #[test]
    fn test_model_variant_serde_roundtrip() {
        for variant in [ModelVariant::MoonshineBase, ModelVariant::MoonshineTiny] {
            let json = serde_json::to_string(&variant).unwrap();
            let deserialized: ModelVariant = serde_json::from_str(&json).unwrap();
            assert_eq!(variant, deserialized);
        }
    }
}
