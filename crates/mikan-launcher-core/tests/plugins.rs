//! The plugin's state inside a host, and installing it.

mod common;

use std::fs;

use mikan_launcher_core::plugins::{self, PluginPayload, PluginState};

#[test]
fn state_follows_the_hosts_copy() {
    let folder = tempfile::tempdir().expect("a temporary folder");
    let host = folder.path().join("host");

    common::payload(&folder.path().join("payload"), "vnyan", "1.0.0");
    fs::create_dir_all(&host).expect("the host folder");

    let payload = PluginPayload::load(&folder.path().join("payload")).expect("the payload loads");

    assert_eq!(payload.manifest().version, "1.0.0");
    assert_eq!(
        payload.check(&host).expect("a check"),
        PluginState::NotInstalled
    );

    payload.install(&host).expect("an install");

    assert_eq!(
        payload.check(&host).expect("a check"),
        PluginState::UpToDate
    );
    assert_eq!(
        fs::read_to_string(host.join("Mods/MikanXR/shaders.bundle")).expect("the installed file"),
        "shaders 1.0.0"
    );

    // Same length, other content: only the digest tells them apart.
    common::write(&host.join("Mods/plugin.bin"), "plugin 9.9.9");

    assert_eq!(payload.check(&host).expect("a check"), PluginState::Differs);

    fs::remove_file(host.join("Mods/plugin.bin")).expect("the file is removed");

    assert_eq!(payload.check(&host).expect("a check"), PluginState::Differs);

    payload.install(&host).expect("a repair");

    assert_eq!(
        payload.check(&host).expect("a check"),
        PluginState::UpToDate
    );
}

#[test]
fn update_replaces_an_older_copy_and_leaves_nothing_behind() {
    let folder = tempfile::tempdir().expect("a temporary folder");
    let host = folder.path().join("host");

    common::payload(&folder.path().join("old"), "warudo", "1.0.0");
    common::payload(&folder.path().join("new"), "warudo", "1.1.0");

    PluginPayload::load(&folder.path().join("old"))
        .expect("the old payload")
        .install(&host)
        .expect("the old install");

    let payload = PluginPayload::load(&folder.path().join("new")).expect("the new payload");

    assert_eq!(payload.check(&host).expect("a check"), PluginState::Differs);

    payload.install(&host).expect("the update");

    assert_eq!(
        payload.check(&host).expect("a check"),
        PluginState::UpToDate
    );

    let mut names: Vec<_> = fs::read_dir(host.join("Mods"))
        .expect("the host's folder")
        .map(|entry| entry.expect("an entry").file_name())
        .collect();

    names.sort();

    assert_eq!(names, ["MikanXR", "plugin.bin"]);
}

#[test]
fn manifest_that_climbs_out_of_its_folder_is_refused() {
    let folder = tempfile::tempdir().expect("a temporary folder");

    for dest in ["../outside.bin", "/absolute.bin", ""] {
        common::write(
            &folder.path().join("plugin.json"),
            &format!(
                r#"{{ "id": "x", "version": "1", "host": "vnyan",
                     "files": [ {{ "src": "a.bin", "dest": "{dest}" }} ] }}"#
            ),
        );

        let error = PluginPayload::load(folder.path()).expect_err("the path is refused");

        assert!(error.to_string().contains("plain relative path"), "{error}");
    }

    common::write(
        &folder.path().join("plugin.json"),
        r#"{ "id": "x", "version": "1", "host": "vnyan", "files": [] }"#,
    );

    assert!(PluginPayload::load(folder.path()).is_err());
}

#[test]
fn missing_payload_reads_as_not_found() {
    let folder = tempfile::tempdir().expect("a temporary folder");
    let error = PluginPayload::load(&folder.path().join("absent")).expect_err("no payload");

    assert!(error.is_not_found(), "{error}");
}

#[test]
fn vnyan_switch_is_read_from_its_security_file() {
    let folder = tempfile::tempdir().expect("a temporary folder");
    let path = folder.path().join("security.json");

    assert_eq!(plugins::vnyan_mods_allowed(&path), None);

    common::write(&path, r#"{ "AllowMods": false }"#);

    assert_eq!(plugins::vnyan_mods_allowed(&path), Some(false));

    common::write(&path, r#"{ "AllowMods": true, "Other": 1 }"#);

    assert_eq!(plugins::vnyan_mods_allowed(&path), Some(true));
}

/// A host that has the plugin loaded holds it as a mapped image, which Windows will not
/// let another file replace but will let be renamed.
#[cfg(windows)]
#[test]
fn file_the_host_holds_loaded_is_moved_aside() {
    use windows::Win32::Foundation::FreeLibrary;
    use windows::Win32::System::LibraryLoader::{LOAD_LIBRARY_AS_IMAGE_RESOURCE, LoadLibraryExW};
    use windows::core::HSTRING;

    let folder = tempfile::tempdir().expect("a temporary folder");
    let host = folder.path().join("host");

    common::payload(&folder.path().join("old"), "vnyan", "1.0.0");
    common::payload(&folder.path().join("new"), "vnyan", "1.1.0");

    // The old plugin has to be a real image for Windows to map it as one, and this test's
    // own executable is the one that is always at hand.
    fs::copy(
        std::env::current_exe().expect("the test's own path"),
        folder.path().join("old").join("plugin.bin"),
    )
    .expect("an image stands in for the old plugin");

    PluginPayload::load(&folder.path().join("old"))
        .expect("the old payload")
        .install(&host)
        .expect("the old install");

    let held = host.join("Mods/plugin.bin");

    // SAFETY: the path is a live, terminated wide string, and mapping an image as a
    // resource runs none of its code.
    #[expect(
        unsafe_code,
        reason = "loading the file is how the host's hold is reproduced"
    )]
    let module = unsafe {
        LoadLibraryExW(
            &HSTRING::from(held.as_os_str()),
            None,
            LOAD_LIBRARY_AS_IMAGE_RESOURCE,
        )
    }
    .expect("the file loads as an image");

    // The hold is real: a plain replacement is refused.
    assert!(fs::rename(folder.path().join("new").join("shaders.bundle"), &held).is_err());
    common::payload(&folder.path().join("new"), "vnyan", "1.1.0");

    let payload = PluginPayload::load(&folder.path().join("new")).expect("the new payload");

    payload
        .install(&host)
        .expect("the update lands beside the held file");

    assert_eq!(
        fs::read_to_string(&held).expect("the new file"),
        "plugin 1.1.0"
    );
    assert!(host.join("Mods/plugin.bin.old").is_file());
    assert!(!host.join("Mods/plugin.bin.mikan-new").exists());

    // While the host holds the old file it cannot be cleared, and the state is still right.
    assert_eq!(
        payload.check(&host).expect("a check"),
        PluginState::UpToDate
    );
    assert!(host.join("Mods/plugin.bin.old").is_file());

    // SAFETY: `module` is the handle the load above returned, freed once.
    #[expect(unsafe_code, reason = "releasing the hold taken above")]
    unsafe { FreeLibrary(module) }.expect("the image unloads");

    assert_eq!(
        payload.check(&host).expect("a check"),
        PluginState::UpToDate
    );
    assert!(!host.join("Mods/plugin.bin.old").exists());
}
