//! Which model to use: a plain file in kfbox's data folder. It never holds
//! the API key, which is in the Keychain.

use std::path::Path;

use serde::{Deserialize, Serialize};

const FILE: &str = "assistant.json";

/// The models kfbox offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Model {
    /// Claude Sonnet 5.5: the default, for judgement over a track's structure.
    #[default]
    #[serde(rename = "claude-sonnet-5-5")]
    Sonnet,
    /// Claude Haiku 5.5: faster and much cheaper.
    #[serde(rename = "claude-haiku-5-5")]
    Haiku,
}

impl Model {
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Sonnet => "claude-sonnet-5-5",
            Self::Haiku => "claude-haiku-5-5",
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Sonnet => "Claude Sonnet 5.5",
            Self::Haiku => "Claude Haiku 5.5 (faster, cheaper)",
        }
    }

    pub const ALL: [Self; 2] = [Self::Sonnet, Self::Haiku];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Settings {
    pub model: Model,
}

impl Settings {
    /// The saved settings, or the defaults.
    #[must_use]
    pub fn load(dir: &Path) -> Self {
        std::fs::read(dir.join(FILE)).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default()
    }

    pub fn save(self, dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        std::fs::write(dir.join(FILE), serde_json::to_vec_pretty(&self).map_err(std::io::Error::other)?)
    }
}
