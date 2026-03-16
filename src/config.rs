use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub auto_gain: bool,
    #[serde(default = "default_gain")]
    pub gain: f32,
    #[serde(default)]
    pub source: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            auto_gain: false,
            gain: default_gain(),
            source: None,
        }
    }
}

fn default_gain() -> f32 {
    1.0
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(path)
            .with_context(|| format!("reading config file {}", path.display()))?;

        match toml::from_str::<Self>(&content) {
            Ok(config) => Ok(config),
            Err(error) => {
                log::warn!("invalid config file {}: {}", path.display(), error);
                log::warn!("using config defaults instead");
                Ok(Self::default())
            }
        }
    }

    pub fn config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("usit")
            .join("usit.toml")
    }
}

#[cfg(test)]
mod tests {
    use super::Config;

    #[test]
    fn partial_toml_uses_defaults() {
        let config: Config = toml::from_str("source = \"mic\"\n").unwrap();
        assert_eq!(config.source.as_deref(), Some("mic"));
        assert!(!config.auto_gain);
        assert_eq!(config.gain, 1.0);
    }
}
