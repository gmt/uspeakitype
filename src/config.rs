use std::fmt;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum AsrModelId {
    #[default]
    MoonshineBase,
    MoonshineTiny,
    MoonshineTinyArabic,
    MoonshineTinyChinese,
    MoonshineTinyJapanese,
    MoonshineTinyKorean,
    MoonshineTinyUkrainian,
    MoonshineTinyVietnamese,
    ParakeetTdt06bV3,
}

impl AsrModelId {
    pub const ALL: [Self; 9] = [
        Self::MoonshineBase,
        Self::MoonshineTiny,
        Self::MoonshineTinyArabic,
        Self::MoonshineTinyChinese,
        Self::MoonshineTinyJapanese,
        Self::MoonshineTinyKorean,
        Self::MoonshineTinyUkrainian,
        Self::MoonshineTinyVietnamese,
        Self::ParakeetTdt06bV3,
    ];

    pub fn all() -> &'static [Self] {
        &Self::ALL
    }

    pub fn is_moonshine(self) -> bool {
        !matches!(self, Self::ParakeetTdt06bV3)
    }

    pub fn dir_name(self) -> &'static str {
        match self {
            Self::MoonshineBase => "moonshine-base",
            Self::MoonshineTiny => "moonshine-tiny",
            Self::MoonshineTinyArabic => "moonshine-tiny-ar",
            Self::MoonshineTinyChinese => "moonshine-tiny-zh",
            Self::MoonshineTinyJapanese => "moonshine-tiny-ja",
            Self::MoonshineTinyKorean => "moonshine-tiny-ko",
            Self::MoonshineTinyUkrainian => "moonshine-tiny-uk",
            Self::MoonshineTinyVietnamese => "moonshine-tiny-vi",
            Self::ParakeetTdt06bV3 => "parakeet-tdt-0.6b-v3",
        }
    }

    pub fn moonshine_download_url_segment(self) -> Option<&'static str> {
        match self {
            Self::MoonshineBase => Some("base"),
            Self::MoonshineTiny => Some("tiny"),
            Self::MoonshineTinyArabic => Some("tiny-ar"),
            Self::MoonshineTinyChinese => Some("tiny-zh"),
            Self::MoonshineTinyJapanese => Some("tiny-ja"),
            Self::MoonshineTinyKorean => Some("tiny-ko"),
            Self::MoonshineTinyUkrainian => Some("tiny-uk"),
            Self::MoonshineTinyVietnamese => Some("tiny-vi"),
            Self::ParakeetTdt06bV3 => None,
        }
    }
}

impl fmt::Display for AsrModelId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.dir_name())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub model: AsrModelId,
    #[serde(default)]
    pub auto_gain: bool,
    #[serde(default = "default_gain")]
    pub gain: f32,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub model_dir: Option<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            model: AsrModelId::default(),
            auto_gain: false,
            gain: default_gain(),
            source: None,
            model_dir: None,
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

    pub fn default_model_dir() -> PathBuf {
        dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("usit")
            .join("models")
    }
}

#[cfg(test)]
mod tests {
    use super::{AsrModelId, Config};

    #[test]
    fn partial_toml_uses_defaults() {
        let config: Config = toml::from_str("source = \"mic\"\n").unwrap();
        assert_eq!(config.source.as_deref(), Some("mic"));
        assert_eq!(config.model, AsrModelId::MoonshineBase);
        assert!(!config.auto_gain);
        assert_eq!(config.gain, 1.0);
        assert!(config.model_dir.is_none());
    }
}
