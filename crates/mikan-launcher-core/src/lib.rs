//! The launcher's logic, with no window: the app catalog, where each app is installed,
//! starting and stopping it, and the `MikanXR` plugin's state inside a host app.

pub mod catalog;
pub mod hub;
pub mod locate;
pub mod platform;
pub mod plugins;
pub mod process;
pub mod settings;

use std::path::PathBuf;

/// A data file the launcher could not use.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("could not read {}: {source}", path.display())]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("could not parse {}: {source}", path.display())]
    Json {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error("{} is not valid: {reason}", path.display())]
    Invalid { path: PathBuf, reason: String },
    #[error("the list of running programs could not be read: {0}")]
    Processes(#[from] sysinfo::Error),
}

impl LoadError {
    /// Whether the file is simply absent, as opposed to unreadable or malformed.
    #[must_use]
    pub fn is_not_found(&self) -> bool {
        matches!(self, Self::Io { source, .. } if source.kind() == std::io::ErrorKind::NotFound)
    }
}

/// Reads and parses one JSON file.
pub(crate) fn read_json<T: serde::de::DeserializeOwned>(
    path: &std::path::Path,
) -> Result<T, LoadError> {
    let text = std::fs::read_to_string(path).map_err(|source| LoadError::Io {
        path: path.to_owned(),
        source,
    })?;

    serde_json::from_str(&text).map_err(|source| LoadError::Json {
        path: path.to_owned(),
        source,
    })
}
