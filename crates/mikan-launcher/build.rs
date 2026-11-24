use std::env;
use std::path::{Path, PathBuf};

use slint_build::CompilerConfiguration;

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("the crate sits two folders below the repository root");
    let branding = branding_dir(root);

    for file in ["icon.ico", "icon.png"] {
        let path = branding.join(file);
        assert!(
            path.is_file(),
            "branding file not found: {}",
            path.display()
        );
    }

    // The checkout of the official art can appear or go between builds, and the choice
    // follows it, so the whole branding folder is watched beside the set in use.
    println!("cargo::rerun-if-env-changed=MIKAN_BRANDING_DIR");
    println!(
        "cargo::rerun-if-changed={}",
        root.join("branding").display()
    );
    println!("cargo::rerun-if-changed={}", branding.display());
    // A build script has no quieter line that cargo shows on every build.
    println!("cargo::warning=Branding: {}", branding.display());

    if env::var("CARGO_CFG_TARGET_OS").is_ok_and(|os| os == "windows") {
        let icon = branding.join("icon.ico");
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon(icon.to_str().expect("the icon's path is UTF-8"));
        // Task Manager and the file's properties show these in place of the crate's own
        // name and description.
        resource.set("ProductName", "MikanXR Launcher");
        resource.set("FileDescription", "MikanXR Launcher");
        resource
            .compile()
            .expect("the executable's icon resource compiles");
    }

    // The markup names its images by file name alone and finds them in the chosen set.
    let config = CompilerConfiguration::new().with_include_paths(vec![branding]);
    slint_build::compile_with_config("ui/main.slint", config)
        .expect("the window's markup compiles");
}

/// The folder the icon and logo are taken from: the one `MIKAN_BRANDING_DIR` names, else
/// the official set when its checkout is present, else the generic set.
fn branding_dir(root: &Path) -> PathBuf {
    // The app's own folder inside a checkout of the official art, which is not part of the
    // repository, and the set the repository carries, which any checkout can build with.
    let official = root.join("branding").join("official").join("mikanlauncher");
    let generic = root.join("branding").join("generic");
    let dir = match env::var_os("MIKAN_BRANDING_DIR") {
        // A relative path is taken from the repository root, not the crate's folder.
        Some(dir) if !dir.is_empty() => root.join(dir),
        _ if official.join("icon.ico").is_file() => official,
        _ => generic,
    };
    assert!(dir.is_dir(), "branding folder not found: {}", dir.display());
    dir
}
