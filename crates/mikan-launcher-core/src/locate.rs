//! Where an app's executable is on this machine.

use std::path::{Path, PathBuf};

use steamlocate::SteamDir;

use crate::catalog::{AppEntry, Source};

/// Looks up where Steam installed an app.
pub trait SteamApps {
    /// The app's install folder, when Steam has it installed.
    fn app_dir(&self, app_id: u32) -> Option<PathBuf>;
}

/// The machine's Steam installation, when there is one.
pub struct SteamInstall(Option<SteamDir>);

impl SteamInstall {
    /// Finds Steam the way the platform records it. A machine without Steam yields an
    /// install that knows no apps.
    #[must_use]
    pub fn locate() -> Self {
        Self(SteamDir::locate().ok())
    }

    /// Uses the Steam installation at this folder.
    #[must_use]
    pub fn from_dir(path: &Path) -> Self {
        Self(SteamDir::from_dir(path).ok())
    }
}

impl SteamApps for SteamInstall {
    fn app_dir(&self, app_id: u32) -> Option<PathBuf> {
        let (app, library) = self.0.as_ref()?.find_app(app_id).ok()??;

        Some(library.resolve_app_dir(&app))
    }
}

/// How an executable was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Manual,
    Bundled,
    Steam,
}

/// An app's executable on this machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub exe: PathBuf,
    pub origin: Origin,
}

impl Resolved {
    /// The folder the executable sits in: the app's working directory, and the root a
    /// plugin's files are installed under.
    #[must_use]
    pub fn install_dir(&self) -> &Path {
        self.exe.parent().unwrap_or(Path::new(""))
    }
}

/// Finds the app's executable. An executable the user browsed to wins while it exists,
/// since it is their explicit choice. Otherwise the catalog's sources are tried in order.
#[must_use]
pub fn resolve(
    app: &AppEntry,
    package_root: &Path,
    steam: &dyn SteamApps,
    manual: Option<&Path>,
) -> Option<Resolved> {
    if let Some(exe) = manual.filter(|exe| exe.is_file()) {
        return Some(Resolved {
            exe: exe.to_owned(),
            origin: Origin::Manual,
        });
    }

    app.sources.iter().find_map(|source| {
        let (exe, origin) = match source {
            Source::Bundled { exe } => (package_root.join(exe), Origin::Bundled),
            Source::Steam { app_id, exe } => (steam.app_dir(*app_id)?.join(exe), Origin::Steam),
        };

        // Rebuilt from its parts, so a catalog path written with forward slashes reads in the
        // platform's own form.
        let exe: PathBuf = exe.components().collect();

        exe.is_file().then_some(Resolved { exe, origin })
    })
}
