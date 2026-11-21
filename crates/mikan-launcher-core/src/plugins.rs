//! The `MikanXR` plugin a host app loads: whether the copy inside the host matches the
//! one the package ships, and putting the shipped one in place.
//!
//! A payload is a folder holding `plugin.json` and the files it names. State is a content
//! comparison, so it needs no version read out of a compiled plugin and leaves no marker
//! file inside the host.

use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::LoadError;

/// The manifest's file name at the root of a payload.
pub const MANIFEST_NAME: &str = "plugin.json";

/// A copy in flight beside its destination.
const INCOMING_SUFFIX: &str = ".mikan-new";

/// A file the host held open, moved aside so the new one could take its name.
const DISPLACED_SUFFIX: &str = ".old";

/// One file of a plugin.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PluginFile {
    /// Relative to the payload folder.
    pub src: PathBuf,
    /// Relative to the host app's install folder.
    pub dest: PathBuf,
}

/// A payload's `plugin.json`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PluginManifest {
    pub id: String,
    pub version: String,
    /// The host app's catalog id.
    pub host: String,
    pub files: Vec<PluginFile>,
}

/// How the host's copy compares with the payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginState {
    /// None of the plugin's files are in the host.
    NotInstalled,
    /// Every file is present and identical.
    UpToDate,
    /// Some file is missing or has other content: an older copy, a newer one, or a
    /// partial install. Content alone cannot tell which.
    Differs,
}

/// A plugin as the package ships it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginPayload {
    dir: PathBuf,
    manifest: PluginManifest,
}

impl PluginPayload {
    /// Reads the payload in a folder.
    ///
    /// # Errors
    ///
    /// When the manifest is absent, unreadable, or malformed, when it names no files, or
    /// when a path in it is not a plain relative path. A path that climbs out of its
    /// folder is refused here so an install can never write outside the host.
    pub fn load(dir: &Path) -> Result<Self, LoadError> {
        let path = dir.join(MANIFEST_NAME);
        let manifest: PluginManifest = crate::read_json(&path)?;
        let invalid = |reason: String| LoadError::Invalid {
            path: path.clone(),
            reason,
        };

        if manifest.files.is_empty() {
            return Err(invalid("it names no files".to_owned()));
        }

        for file in &manifest.files {
            for entry in [&file.src, &file.dest] {
                if !is_plain_relative(entry) {
                    return Err(invalid(format!(
                        "\"{}\" is not a plain relative path",
                        entry.display()
                    )));
                }
            }
        }

        Ok(Self {
            dir: dir.to_owned(),
            manifest,
        })
    }

    #[must_use]
    pub fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    /// Compares the host's copy with the payload, and clears files an earlier install
    /// moved aside once the host has let go of them.
    ///
    /// # Errors
    ///
    /// When a file on either side cannot be read.
    pub fn check(&self, host_root: &Path) -> io::Result<PluginState> {
        let mut missing = 0;
        let mut equal = 0;

        for file in &self.manifest.files {
            let dest = host_root.join(&file.dest);

            let _ = fs::remove_file(with_suffix(&dest, DISPLACED_SUFFIX));

            if !dest.is_file() {
                missing += 1;
            } else if same_content(&self.dir.join(&file.src), &dest)? {
                equal += 1;
            }
        }

        let total = self.manifest.files.len();

        Ok(if missing == total {
            PluginState::NotInstalled
        } else if equal == total {
            PluginState::UpToDate
        } else {
            PluginState::Differs
        })
    }

    /// Puts every file of the payload in place inside the host.
    ///
    /// Each file is copied beside its destination and renamed over it, so the host never
    /// sees half a file. A host that holds the old file loaded refuses the replacement
    /// but allows a rename, so the old file is moved aside and removed on a later check.
    ///
    /// # Errors
    ///
    /// When a folder cannot be created or a file cannot be copied or put in place. Files
    /// installed before the failure stay, and the state then reads as differing.
    pub fn install(&self, host_root: &Path) -> io::Result<()> {
        for file in &self.manifest.files {
            let dest = host_root.join(&file.dest);
            let incoming = with_suffix(&dest, INCOMING_SUFFIX);

            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent)?;
            }

            let placed = fs::copy(self.dir.join(&file.src), &incoming)
                .and_then(|_| replace(&incoming, &dest));

            if placed.is_err() {
                let _ = fs::remove_file(&incoming);
            }

            placed?;
        }

        Ok(())
    }
}

/// Whether `VNyan` allows third-party plugins, read from its `security.json`. `None` when
/// the file is absent or does not say.
#[must_use]
pub fn vnyan_mods_allowed(security_json: &Path) -> Option<bool> {
    let value: serde_json::Value = crate::read_json(security_json).ok()?;

    value.get("AllowMods")?.as_bool()
}

/// Moves `incoming` onto `dest`, moving a `dest` that cannot be replaced out of the way.
fn replace(incoming: &Path, dest: &Path) -> io::Result<()> {
    let Err(refused) = fs::rename(incoming, dest) else {
        return Ok(());
    };

    if !dest.exists() {
        return Err(refused);
    }

    let displaced = with_suffix(dest, DISPLACED_SUFFIX);

    let _ = fs::remove_file(&displaced);
    fs::rename(dest, &displaced)?;
    fs::rename(incoming, dest)
}

fn is_plain_relative(path: &Path) -> bool {
    let mut components = path.components().peekable();

    components.peek().is_some() && components.all(|part| matches!(part, Component::Normal(_)))
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();

    name.push(suffix);

    PathBuf::from(name)
}

fn same_content(left: &Path, right: &Path) -> io::Result<bool> {
    if fs::metadata(left)?.len() != fs::metadata(right)?.len() {
        return Ok(false);
    }

    Ok(digest(left)? == digest(right)?)
}

fn digest(path: &Path) -> io::Result<sha2::digest::Output<Sha256>> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];

    loop {
        let read = file.read(&mut buffer)?;

        if read == 0 {
            return Ok(hasher.finalize());
        }

        hasher.update(&buffer[..read]);
    }
}
