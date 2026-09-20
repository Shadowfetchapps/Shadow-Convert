use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::paths;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Quality {
    Small,
    Balanced,
    #[default]
    High,
    Maximum,
}

impl Quality {
    pub const ALL: [Quality; 4] = [
        Quality::Small,
        Quality::Balanced,
        Quality::High,
        Quality::Maximum,
    ];

    pub fn as_label(self) -> &'static str {
        match self {
            Self::Small => "Small",
            Self::Balanced => "Balanced",
            Self::High => "High",
            Self::Maximum => "Maximum",
        }
    }

    pub fn video_crf(self) -> u8 {
        match self {
            Self::Small => 32,
            Self::Balanced => 28,
            Self::High => 23,
            Self::Maximum => 18,
        }
    }

    pub fn nvenc_cq(self) -> u8 {
        self.video_crf()
    }

    pub fn audio_bitrate_k(self) -> u32 {
        match self {
            Self::Small => 96,
            Self::Balanced => 160,
            Self::High => 192,
            Self::Maximum => 320,
        }
    }

    pub fn image_quality(self) -> u8 {
        match self {
            Self::Small => 60,
            Self::Balanced => 75,
            Self::High => 88,
            Self::Maximum => 96,
        }
    }

    pub fn gif_fps(self) -> u32 {
        match self {
            Self::Small => 8,
            Self::Balanced => 10,
            Self::High => 12,
            Self::Maximum => 15,
        }
    }

    pub fn from_index(index: u32) -> Self {
        match index {
            0 => Self::Small,
            1 => Self::Balanced,
            3 => Self::Maximum,
            _ => Self::High,
        }
    }

    pub fn index(self) -> u32 {
        match self {
            Self::Small => 0,
            Self::Balanced => 1,
            Self::High => 2,
            Self::Maximum => 3,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

impl Theme {
    pub fn as_label(self) -> &'static str {
        match self {
            Self::System => "System",
            Self::Light => "Light",
            Self::Dark => "Dark",
        }
    }

    pub fn from_index(index: u32) -> Self {
        match index {
            1 => Self::Light,
            2 => Self::Dark,
            _ => Self::System,
        }
    }

    pub fn index(self) -> u32 {
        match self {
            Self::System => 0,
            Self::Light => 1,
            Self::Dark => 2,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub quality: Quality,
    pub prefer_hardware: bool,
    pub preserve_metadata: bool,
    pub output_dir: Option<PathBuf>,
    pub theme: Theme,
    pub last_video_container: String,
    pub last_audio_format: String,
    pub last_image_format: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            quality: Quality::High,
            prefer_hardware: true,
            preserve_metadata: true,
            output_dir: None,
            theme: Theme::System,
            last_video_container: "mp4".into(),
            last_audio_format: "mp3".into(),
            last_image_format: "png".into(),
        }
    }
}

impl Settings {
    pub fn load() -> Self {
        let Ok(path) = paths::settings_path() else {
            return Self::default();
        };
        let Ok(bytes) = std::fs::read(&path) else {
            return Self::default();
        };
        serde_json::from_slice(&bytes).unwrap_or_default()
    }

    pub fn save(&self) -> Result<()> {
        let path = paths::settings_path()?;
        if let Some(parent) = path.parent() {
            paths::ensure_dir(parent)?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|err| {
            crate::error::Error::detailed("Could not save settings.", err.to_string())
        })?;
        std::fs::write(&path, json)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quality_order_is_stable() {
        assert!(Quality::Small.video_crf() > Quality::High.video_crf());
        assert!(Quality::Maximum.audio_bitrate_k() > Quality::Small.audio_bitrate_k());
    }

    #[test]
    fn settings_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let mut s = Settings::default();
        s.quality = Quality::Small;
        s.prefer_hardware = false;
        std::fs::write(&path, serde_json::to_string(&s).unwrap()).unwrap();
        let loaded: Settings = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(loaded.quality, Quality::Small);
        assert!(!loaded.prefer_hardware);
    }
}
