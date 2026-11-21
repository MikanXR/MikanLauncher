//! The catalog the repository ships, and finding an app's executable.

mod common;

use mikan_launcher_core::catalog::{Catalog, Slot};
use mikan_launcher_core::locate::{self, Origin, SteamInstall};

#[test]
fn shipped_catalog_loads_with_an_app_in_every_slot() {
    let catalog = Catalog::load(&common::shipped_catalog()).expect("the shipped catalog loads");

    for slot in Slot::ALL {
        assert!(catalog.apps_for(slot).next().is_some(), "{slot:?} is empty");
    }

    assert!(catalog.find(Slot::Renderer, "warudo").is_some());
    assert!(catalog.find(Slot::Tracking, "warudo").is_none());
}

#[test]
fn catalog_with_a_repeated_id_is_refused() {
    let folder = tempfile::tempdir().expect("a temporary folder");
    let path = folder.path().join("catalog.json");

    common::write(
        &path,
        r#"{ "apps": [ { "id": "a", "name": "A", "slot": "editor" },
                       { "id": "a", "name": "B", "slot": "tracking" } ] }"#,
    );

    let error = Catalog::load(&path).expect_err("a repeated id fails");

    assert!(error.to_string().contains("appears twice"), "{error}");
}

#[test]
fn bundled_app_resolves_under_the_package_root() {
    let folder = tempfile::tempdir().expect("a temporary folder");
    let catalog = Catalog::load(&common::shipped_catalog()).expect("the shipped catalog loads");
    let editor = catalog
        .find(Slot::Editor, "mikanxr")
        .expect("the editor row");

    assert_eq!(
        locate::resolve(editor, folder.path(), &common::NoSteam, None),
        None
    );

    let exe = folder.path().join("MikanXR").join("Mikan.exe");

    common::write(&exe, "");

    let resolved =
        locate::resolve(editor, folder.path(), &common::NoSteam, None).expect("the editor");

    assert_eq!(resolved.exe, exe);
    assert_eq!(resolved.origin, Origin::Bundled);
    assert_eq!(resolved.install_dir(), folder.path().join("MikanXR"));
}

#[test]
fn steam_app_resolves_through_the_library() {
    let folder = tempfile::tempdir().expect("a temporary folder");
    let exe = common::steam(folder.path(), 2_079_120, "Warudo", "Warudo.exe");
    let steam = SteamInstall::from_dir(folder.path());
    let catalog = Catalog::load(&common::shipped_catalog()).expect("the shipped catalog loads");
    let warudo = catalog.find(Slot::Renderer, "warudo").expect("the row");

    let resolved =
        locate::resolve(warudo, folder.path(), &steam, None).expect("Warudo in the library");

    assert_eq!(resolved.exe, exe);
    assert_eq!(resolved.origin, Origin::Steam);
}

#[test]
fn browsed_executable_wins_while_it_exists() {
    let folder = tempfile::tempdir().expect("a temporary folder");
    let steam_exe = common::steam(folder.path(), 2_079_120, "Warudo", "Warudo.exe");
    let steam = SteamInstall::from_dir(folder.path());
    let catalog = Catalog::load(&common::shipped_catalog()).expect("the shipped catalog loads");
    let warudo = catalog.find(Slot::Renderer, "warudo").expect("the row");
    let browsed = folder.path().join("elsewhere").join("Warudo.exe");

    // A browsed path that no longer exists falls back to the catalog's sources.
    let resolved = locate::resolve(warudo, folder.path(), &steam, Some(&browsed)).expect("Warudo");

    assert_eq!(resolved.exe, steam_exe);

    common::write(&browsed, "");

    let resolved = locate::resolve(warudo, folder.path(), &steam, Some(&browsed)).expect("Warudo");

    assert_eq!(resolved.exe, browsed);
    assert_eq!(resolved.origin, Origin::Manual);
}
