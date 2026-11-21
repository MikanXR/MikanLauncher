//! What the user chose: the app in each slot, the executables they pointed the launcher
//! at, and the plugin versions the launcher installed.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The per-user settings file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub struct Settings {
    /// Slot key to app id. An empty id is an explicit "None", and a missing key is a slot
    /// the user has not touched yet.
    pub selected: BTreeMap<String, String>,
    /// App id to the executable the user browsed to.
    pub manual_paths: BTreeMap<String, PathBuf>,
    /// Plugin id to the version the launcher last installed.
    pub installed_plugins: BTreeMap<String, String>,
}

impl Settings {
    /// Where the settings live for this user, when the platform names a config folder.
    #[must_use]
    pub fn default_path() -> Option<PathBuf> {
        let base = directories::BaseDirs::new()?;

        Some(
            base.config_dir()
                .join("MikanXR")
                .join("Launcher")
                .join("settings.json"),
        )
    }

    /// Reads the settings, falling back to defaults when the file is absent or unusable:
    /// a damaged settings file must not keep the launcher from opening.
    #[must_use]
    pub fn load(path: &Path) -> Self {
        crate::read_json(path).unwrap_or_default()
    }

    /// Writes the settings, creating the folder on the first save.
    ///
    /// # Errors
    ///
    /// When the folder cannot be created or the file cannot be written.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let text = serde_json::to_string_pretty(self).map_err(io::Error::other)?;

        std::fs::write(path, text)
    }
}
