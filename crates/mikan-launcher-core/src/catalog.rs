//! The app catalog: which apps the launcher offers in each slot and where each one is
//! looked for. It is data shipped beside the launcher, so adding an app needs no rebuild.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::LoadError;

/// The catalog's file name, beside the launcher.
pub const CATALOG_NAME: &str = "catalog.json";

/// One of the launcher's three rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Slot {
    Editor,
    Tracking,
    Renderer,
}

impl Slot {
    /// Every slot, in the order they are shown and launched.
    pub const ALL: [Self; 3] = [Self::Editor, Self::Tracking, Self::Renderer];

    /// The slot's position in [`Slot::ALL`].
    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::Editor => 0,
            Self::Tracking => 1,
            Self::Renderer => 2,
        }
    }

    /// The name the settings file stores a slot's selection under.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Editor => "editor",
            Self::Tracking => "tracking",
            Self::Renderer => "renderer",
        }
    }

    /// The row's heading.
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::Editor => "MikanXR",
            Self::Tracking => "Tracking App",
            Self::Renderer => "Character Renderer",
        }
    }
}

/// A place an app's executable may be found without asking the user.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Source {
    /// Shipped in the package: `exe` is relative to the launcher's folder.
    Bundled { exe: PathBuf },
    /// Installed through Steam: `exe` is relative to the app's install folder.
    Steam { app_id: u32, exe: PathBuf },
}

/// One app the launcher can start.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AppEntry {
    pub id: String,
    pub name: String,
    pub slot: Slot,
    /// Tried in order. An app with none is found only through Browse.
    #[serde(default)]
    pub sources: Vec<Source>,
    /// Arguments the app is started with.
    #[serde(default)]
    pub args: Vec<String>,
    /// The folder under the package's `plugins/` holding the `MikanXR` plugin for this app.
    #[serde(default)]
    pub plugin: Option<String>,
}

/// Every app the launcher offers.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Catalog {
    pub apps: Vec<AppEntry>,
}

impl Catalog {
    /// Reads the catalog file.
    ///
    /// # Errors
    ///
    /// When the file is absent, unreadable, malformed, or names the same app id twice.
    pub fn load(path: &Path) -> Result<Self, LoadError> {
        let catalog: Self = crate::read_json(path)?;

        for (index, app) in catalog.apps.iter().enumerate() {
            if catalog.apps[..index].iter().any(|other| other.id == app.id) {
                return Err(LoadError::Invalid {
                    path: path.to_owned(),
                    reason: format!("the app id \"{}\" appears twice", app.id),
                });
            }
        }

        Ok(catalog)
    }

    /// The apps offered in one slot, in catalog order.
    pub fn apps_for(&self, slot: Slot) -> impl Iterator<Item = &AppEntry> {
        self.apps.iter().filter(move |app| app.slot == slot)
    }

    /// The app with this id, when it belongs to the slot.
    #[must_use]
    pub fn find(&self, slot: Slot, id: &str) -> Option<&AppEntry> {
        self.apps_for(slot).find(|app| app.id == id)
    }
}
