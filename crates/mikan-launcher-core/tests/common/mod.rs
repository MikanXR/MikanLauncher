//! Fixture trees the tests build under a temporary folder.

#![allow(dead_code, reason = "each test file uses its own part of the fixtures")]

use std::fs;
use std::path::{Path, PathBuf};

use mikan_launcher_core::hub::Environment;
use mikan_launcher_core::locate::SteamApps;

/// The catalog the repository ships.
pub fn shipped_catalog() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../catalog.json")
}

/// Writes a file, creating its folders.
pub fn write(path: &Path, content: &str) {
    fs::create_dir_all(path.parent().expect("the fixture path has a folder"))
        .expect("the fixture folder is created");
    fs::write(path, content).expect("the fixture file is written");
}

/// A package folder holding the shipped catalog and nothing else.
pub fn package(root: &Path) {
    fs::create_dir_all(root).expect("the package folder is created");
    fs::copy(shipped_catalog(), root.join("catalog.json")).expect("the catalog is copied");
}

/// A plugin payload of two files, one of them in a subfolder of the host.
pub fn payload(dir: &Path, host: &str, version: &str) {
    write(
        &dir.join("plugin.json"),
        &format!(
            r#"{{ "id": "mikanxr-{host}", "version": "{version}", "host": "{host}",
                 "files": [
                   {{ "src": "plugin.bin", "dest": "Mods/plugin.bin" }},
                   {{ "src": "shaders.bundle", "dest": "Mods/MikanXR/shaders.bundle" }} ] }}"#
        ),
    );
    write(&dir.join("plugin.bin"), &format!("plugin {version}"));
    write(&dir.join("shaders.bundle"), &format!("shaders {version}"));
}

/// A Steam installation whose one library holds one app, laid out the way Steam writes it.
pub fn steam(root: &Path, app_id: u32, install_dir: &str, exe: &str) -> PathBuf {
    let library = root.to_string_lossy().replace('\\', "\\\\");

    write(
        &root.join("steamapps").join("libraryfolders.vdf"),
        &format!(
            "\"libraryfolders\"\n{{\n\t\"0\"\n\t{{\n\t\t\"path\"\t\t\"{library}\"\n\t\t\"apps\"\n\t\t{{\n\t\t\t\"{app_id}\"\t\t\"0\"\n\t\t}}\n\t}}\n}}\n"
        ),
    );
    write(
        &root
            .join("steamapps")
            .join(format!("appmanifest_{app_id}.acf")),
        &format!(
            "\"AppState\"\n{{\n\t\"appid\"\t\t\"{app_id}\"\n\t\"name\"\t\t\"{install_dir}\"\n\t\"installdir\"\t\t\"{install_dir}\"\n}}\n"
        ),
    );

    let exe = root
        .join("steamapps")
        .join("common")
        .join(install_dir)
        .join(exe);

    write(&exe, "");

    exe
}

/// A machine with no Steam.
pub struct NoSteam;

impl SteamApps for NoSteam {
    fn app_dir(&self, _app_id: u32) -> Option<PathBuf> {
        None
    }
}

/// A machine with no Steam and, optionally, a `VNyan` security file.
pub fn environment(vnyan_security: Option<PathBuf>) -> Environment {
    Environment {
        steam: Box::new(NoSteam),
        vnyan_security,
    }
}
