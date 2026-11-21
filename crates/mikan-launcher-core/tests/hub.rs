//! The hub: what each slot holds and what the window is told to show for it.

mod common;

use mikan_launcher_core::catalog::Slot;
use mikan_launcher_core::hub::{Hub, PluginAction, SlotAction};

/// The position of an app in a slot's list, by name.
fn option(hub: &Hub, slot: Slot, name: &str) -> usize {
    hub.option_names(slot)
        .iter()
        .position(|offered| offered == name)
        .expect("the slot offers the app")
}

#[test]
fn untouched_slots_hold_the_editor_and_nothing_else() {
    let folder = tempfile::tempdir().expect("a temporary folder");
    let root = folder.path().join("package");

    common::package(&root);

    let hub = Hub::open(
        &root,
        &folder.path().join("settings.json"),
        common::environment(None),
    )
    .expect("the hub opens");

    assert_eq!(
        hub.selected_app(Slot::Editor).map(|app| app.id.as_str()),
        Some("mikanxr")
    );
    assert_eq!(hub.selected_index(Slot::Editor), 1);
    assert!(hub.selected_app(Slot::Tracking).is_none());
    assert!(hub.selected_app(Slot::Renderer).is_none());
    assert_eq!(hub.option_names(Slot::Editor), ["None", "MikanXR"]);

    // The package holds no editor yet, so the row asks for one and offers no launch.
    let view = hub.view(Slot::Editor);

    assert!(view.status.contains("was not found"), "{}", view.status);
    assert!(view.can_browse);
    assert!(!view.action_enabled);

    let view = hub.view(Slot::Tracking);

    assert!(!view.can_browse);
    assert!(!view.action_enabled);
}

#[test]
fn bundled_editor_is_ready_and_offers_no_browse() {
    let folder = tempfile::tempdir().expect("a temporary folder");
    let root = folder.path().join("package");

    common::package(&root);
    common::write(&root.join("MikanXR").join("Mikan.exe"), "");

    let hub = Hub::open(
        &root,
        &folder.path().join("settings.json"),
        common::environment(None),
    )
    .expect("the hub opens");
    let view = hub.view(Slot::Editor);

    assert!(view.status.starts_with("Ready:"), "{}", view.status);
    assert_eq!(view.action, SlotAction::Launch);
    assert!(view.action_enabled);
    assert!(!view.can_browse);
    assert!(!view.running);
}

#[test]
fn selection_and_browsed_path_survive_a_restart() {
    let folder = tempfile::tempdir().expect("a temporary folder");
    let root = folder.path().join("package");
    let settings = folder.path().join("user").join("settings.json");
    let exe = folder.path().join("vnyan").join("VNyan.exe");

    common::package(&root);
    common::write(&exe, "");

    let mut hub = Hub::open(&root, &settings, common::environment(None)).expect("the hub opens");

    hub.select(Slot::Editor, 0);
    hub.select(Slot::Renderer, option(&hub, Slot::Renderer, "VNyan"));
    hub.set_manual_path(Slot::Renderer, &exe);

    let hub = Hub::open(&root, &settings, common::environment(None)).expect("the hub reopens");

    assert!(hub.selected_app(Slot::Editor).is_none());
    assert_eq!(hub.selected_index(Slot::Editor), 0);
    assert_eq!(
        hub.selected_app(Slot::Renderer).map(|app| app.id.as_str()),
        Some("vnyan")
    );
    assert!(hub.view(Slot::Renderer).status.starts_with("Ready:"));
    assert!(hub.view(Slot::Renderer).can_browse);
}

#[test]
fn plugin_is_offered_installed_and_then_reported_current() {
    let folder = tempfile::tempdir().expect("a temporary folder");
    let root = folder.path().join("package");
    let settings = folder.path().join("settings.json");
    let security = folder.path().join("security.json");
    let exe = folder.path().join("vnyan").join("VNyan.exe");

    common::package(&root);
    common::write(&exe, "");
    common::write(&security, r#"{ "AllowMods": false }"#);

    let mut hub = Hub::open(
        &root,
        &settings,
        common::environment(Some(security.clone())),
    )
    .expect("the hub opens");

    hub.select(Slot::Renderer, option(&hub, Slot::Renderer, "VNyan"));
    hub.set_manual_path(Slot::Renderer, &exe);

    // A package without the payload says so and offers nothing.
    let view = hub.view(Slot::Renderer);

    assert!(
        view.plugin_text.contains("does not include"),
        "{}",
        view.plugin_text
    );
    assert_eq!(view.plugin_action, None);

    common::payload(&root.join("plugins").join("vnyan"), "vnyan", "0.2.0");
    hub.select(Slot::Renderer, option(&hub, Slot::Renderer, "VNyan"));

    let view = hub.view(Slot::Renderer);

    assert!(
        view.plugin_text.contains("0.2.0 is not installed"),
        "{}",
        view.plugin_text
    );
    assert_eq!(view.plugin_action, Some(PluginAction::Install));
    assert_eq!(view.notice, "");

    hub.run_plugin_action(Slot::Renderer);

    let view = hub.view(Slot::Renderer);

    assert!(
        view.plugin_text.contains("0.2.0 is installed"),
        "{}",
        view.plugin_text
    );
    assert_eq!(view.plugin_action, None);
    assert!(view.notice.contains("Allow 3rd party"), "{}", view.notice);
    assert!(exe.with_file_name("Mods").join("plugin.bin").is_file());

    // The switch turned on, and a newer payload in the package.
    common::write(&security, r#"{ "AllowMods": true }"#);
    common::payload(&root.join("plugins").join("vnyan"), "vnyan", "0.3.0");
    hub.select(Slot::Renderer, option(&hub, Slot::Renderer, "VNyan"));

    let view = hub.view(Slot::Renderer);

    assert!(view.plugin_text.contains("differs"), "{}", view.plugin_text);
    assert_eq!(view.plugin_action, Some(PluginAction::Update));
    assert_eq!(view.notice, "");
}

#[test]
fn package_summary_names_the_components() {
    let folder = tempfile::tempdir().expect("a temporary folder");
    let root = folder.path().join("package");

    common::package(&root);

    let settings = folder.path().join("settings.json");
    let hub = Hub::open(&root, &settings, common::environment(None)).expect("the hub opens");

    assert_eq!(hub.package_summary(), "");

    common::write(
        &root.join("package.json"),
        r#"{ "components": { "MikanXR": "2026.09.14", "MikanTrack": "0.1.0" } }"#,
    );

    assert_eq!(
        hub.package_summary(),
        "MikanTrack 0.1.0, MikanXR 2026.09.14"
    );
}
