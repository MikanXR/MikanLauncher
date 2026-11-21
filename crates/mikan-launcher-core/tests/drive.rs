//! Drives a real package the way the window's buttons do: starts every selected slot's
//! app, proves a second launch starts nothing, then stops them all.
//!
//! It opens real applications on the desktop, so it runs only when asked for:
//!
//! ```text
//! MIKAN_LAUNCHER_DRIVE_ROOT=<package folder> MIKAN_LAUNCHER_DRIVE_SETTINGS=<settings file> \
//!     cargo test -p mikan-launcher-core --test drive -- --ignored --nocapture
//! ```

use std::path::PathBuf;
use std::time::{Duration, Instant};

use mikan_launcher_core::catalog::Slot;
use mikan_launcher_core::hub::{Environment, Hub, SlotAction};
use mikan_launcher_core::locate::{self, SteamInstall};
use mikan_launcher_core::process::ProcessTable;
use mikan_launcher_core::settings::Settings;

const START_TIMEOUT: Duration = Duration::from_secs(90);
const STOP_TIMEOUT: Duration = Duration::from_secs(60);
const POLL: Duration = Duration::from_millis(500);

/// Ticks the hub until the condition holds, returning how long that took.
fn wait_until(hub: &mut Hub, limit: Duration, done: impl Fn(&Hub) -> bool) -> Option<Duration> {
    let started = Instant::now();

    while started.elapsed() < limit {
        hub.tick();

        if done(hub) {
            return Some(started.elapsed());
        }

        std::thread::sleep(POLL);
    }

    None
}

#[test]
#[ignore = "starts the real apps of the package MIKAN_LAUNCHER_DRIVE_ROOT names"]
fn package_launches_once_and_stops() {
    let root = PathBuf::from(std::env::var("MIKAN_LAUNCHER_DRIVE_ROOT").expect("the package"));
    let settings_path =
        PathBuf::from(std::env::var("MIKAN_LAUNCHER_DRIVE_SETTINGS").expect("the settings"));
    let mut hub = Hub::open(&root, &settings_path, Environment::detect()).expect("the hub opens");
    let slots: Vec<Slot> = Slot::ALL
        .into_iter()
        .filter(|slot| hub.selected_app(*slot).is_some())
        .collect();

    assert!(!slots.is_empty(), "the settings select no app");

    // The executables, found the same way the hub finds them, to count their processes.
    let settings = Settings::load(&settings_path);
    let steam = SteamInstall::locate();
    let executables: Vec<PathBuf> = slots
        .iter()
        .map(|slot| {
            let app = hub.selected_app(*slot).expect("the slot's app");
            let manual = settings.manual_paths.get(&app.id).map(PathBuf::as_path);

            locate::resolve(app, &root, &steam, manual)
                .expect("the app is found")
                .exe
        })
        .collect();

    for slot in &slots {
        let view = hub.view(*slot);

        assert_eq!(view.action, SlotAction::Launch, "{slot:?} already runs");
        assert!(view.action_enabled, "{slot:?}: {}", view.status);
    }

    hub.launch_all();

    let took = wait_until(&mut hub, START_TIMEOUT, |hub| {
        slots.iter().all(|slot| hub.is_running(*slot))
    })
    .expect("every app is seen running");

    println!("every app was seen running after {took:?}");

    // Give each app time to finish starting whatever processes it starts.
    std::thread::sleep(Duration::from_secs(10));

    let count = |executables: &[PathBuf]| -> Vec<usize> {
        let table = ProcessTable::new().expect("the process list");

        executables.iter().map(|exe| table.count(exe)).collect()
    };
    let before = count(&executables);

    println!("processes per app: {before:?}");

    hub.launch_all();

    // A running app's button is Stop, so the window offers no second launch either.
    for slot in &slots {
        assert_eq!(hub.view(*slot).action, SlotAction::Stop, "{slot:?}");
    }

    std::thread::sleep(Duration::from_secs(3));
    assert_eq!(
        count(&executables),
        before,
        "a second launch started a copy"
    );

    hub.stop_all();

    let started = Instant::now();
    let mut forced = Vec::new();

    while started.elapsed() < STOP_TIMEOUT && slots.iter().any(|slot| hub.is_running(*slot)) {
        hub.tick();

        for slot in &slots {
            if hub.view(*slot).action == SlotAction::ForceStop && !forced.contains(slot) {
                forced.push(*slot);
                hub.activate(*slot);
            }
        }

        std::thread::sleep(POLL);
    }

    hub.tick();

    println!(
        "everything stopped after {:?}, forced: {forced:?}",
        started.elapsed()
    );

    for slot in &slots {
        assert!(!hub.is_running(*slot), "{slot:?} still runs");
    }
}
