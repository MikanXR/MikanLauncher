//! The launcher's window over the core's hub.
//!
//! Usage: `MikanLauncher [--root <dir>] [--settings <file>] [--status]`
//!   `--root <dir>`       the package folder, the launcher's own folder by default
//!   `--settings <file>`  the settings file, the user's own by default
//!   `--status`           print each slot's state as JSON and exit, opening no window

// A release build is a window with no console behind it.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use std::cell::RefCell;
use std::path::PathBuf;
use std::process::ExitCode;
use std::rc::Rc;
use std::time::Duration;

use mikan_launcher_core::catalog::Slot;
use mikan_launcher_core::hub::{Environment, Hub, PluginAction, SlotAction, SlotView};
use mikan_launcher_core::settings::Settings;
use slint::{ComponentHandle, Model, ModelRc, SharedString, Timer, TimerMode, VecModel};

slint::include_modules!();

/// How often the running state is read.
const TICK: Duration = Duration::from_secs(1);

struct Arguments {
    root: Option<PathBuf>,
    settings: Option<PathBuf>,
    status: bool,
}

impl Arguments {
    fn parse(mut given: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut arguments = Self {
            root: None,
            settings: None,
            status: false,
        };

        while let Some(flag) = given.next() {
            let mut value = || {
                given
                    .next()
                    .map(PathBuf::from)
                    .ok_or_else(|| format!("{flag} needs a path"))
            };

            match flag.as_str() {
                "--root" => arguments.root = Some(value()?),
                "--settings" => arguments.settings = Some(value()?),
                "--status" => arguments.status = true,
                other => return Err(format!("unknown argument: {other}")),
            }
        }

        Ok(arguments)
    }
}

fn main() -> ExitCode {
    let arguments = match Arguments::parse(std::env::args().skip(1)) {
        Ok(arguments) => arguments,
        Err(problem) => {
            eprintln!("{problem}");

            return ExitCode::from(2);
        }
    };

    let hub = match open_hub(&arguments) {
        Ok(hub) => hub,
        Err(problem) => {
            eprintln!("{problem}");

            if !arguments.status {
                rfd::MessageDialog::new()
                    .set_level(rfd::MessageLevel::Error)
                    .set_title("MikanXR Launcher")
                    .set_description(problem)
                    .show();
            }

            return ExitCode::FAILURE;
        }
    };

    if arguments.status {
        print_status(&hub);

        return ExitCode::SUCCESS;
    }

    match run_window(hub) {
        Ok(()) => ExitCode::SUCCESS,
        Err(problem) => {
            eprintln!("{problem}");

            ExitCode::FAILURE
        }
    }
}

fn open_hub(arguments: &Arguments) -> Result<Hub, String> {
    let root = match &arguments.root {
        Some(root) => root.clone(),
        None => std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(PathBuf::from))
            .ok_or("the launcher could not find its own folder")?,
    };

    // A machine that names no config folder keeps the settings beside the launcher.
    let settings = arguments
        .settings
        .clone()
        .or_else(Settings::default_path)
        .unwrap_or_else(|| root.join("settings.json"));

    Hub::open(&root, &settings, Environment::detect()).map_err(|problem| problem.to_string())
}

fn print_status(hub: &Hub) {
    let report: Vec<_> = Slot::ALL
        .into_iter()
        .map(|slot| {
            serde_json::json!({
                "slot": slot.key(),
                "app": hub.selected_app(slot).map(|app| &app.id),
                "view": hub.view(slot),
            })
        })
        .collect();

    println!("{}", serde_json::Value::Array(report));
}

fn run_window(hub: Hub) -> Result<(), slint::PlatformError> {
    let window = MainWindow::new()?;

    let rows: Vec<SlotRow> = Slot::ALL
        .into_iter()
        .map(|slot| SlotRow {
            title: slot.title().into(),
            options: ModelRc::new(VecModel::from(
                hub.option_names(slot)
                    .into_iter()
                    .map(SharedString::from)
                    .collect::<Vec<_>>(),
            )),
            selected: i32::try_from(hub.selected_index(slot)).unwrap_or(0),
        })
        .collect();

    let states = Rc::new(VecModel::from(
        Slot::ALL
            .into_iter()
            .map(|slot| slot_state(&hub.view(slot)))
            .collect::<Vec<_>>(),
    ));

    window.set_rows(ModelRc::new(VecModel::from(rows)));
    window.set_states(ModelRc::from(states.clone()));
    window.set_launcher_version(env!("CARGO_PKG_VERSION").into());
    window.set_package_text(hub.package_summary().into());

    let hub = Rc::new(RefCell::new(hub));

    // Runs one change against the hub, then brings the window up to date with it.
    let act = {
        let hub = hub.clone();
        let weak = window.as_weak();

        move |change: &dyn Fn(&mut Hub)| {
            let mut hub = hub.borrow_mut();

            change(&mut hub);

            if let Some(window) = weak.upgrade() {
                refresh(&window, &states, &hub);
            }
        }
    };

    act(&|_| {});

    window.on_select({
        let act = act.clone();

        move |slot, option| {
            if let (Some(slot), Ok(option)) = (slot_at(slot), usize::try_from(option)) {
                act(&|hub| hub.select(slot, option));
            }
        }
    });

    window.on_browse({
        let act = act.clone();

        move |slot| {
            let Some(slot) = slot_at(slot) else { return };
            let mut dialog = rfd::FileDialog::new().set_title("Locate the application");

            if cfg!(windows) {
                dialog = dialog.add_filter("Programs", &["exe"]);
            }

            if let Some(exe) = dialog.pick_file() {
                act(&|hub| hub.set_manual_path(slot, &exe));
            }
        }
    });

    window.on_activate({
        let act = act.clone();

        move |slot| {
            if let Some(slot) = slot_at(slot) {
                act(&|hub| hub.activate(slot));
            }
        }
    });

    window.on_plugin_action({
        let act = act.clone();

        move |slot| {
            if let Some(slot) = slot_at(slot) {
                act(&|hub| hub.run_plugin_action(slot));
            }
        }
    });

    window.on_launch_all({
        let act = act.clone();

        move || act(&Hub::launch_all)
    });

    window.on_stop_all({
        let act = act.clone();

        move || act(&Hub::stop_all)
    });

    let timer = Timer::default();

    timer.start(TimerMode::Repeated, TICK, move || act(&Hub::tick));

    window.run()
}

fn refresh(window: &MainWindow, states: &VecModel<SlotState>, hub: &Hub) {
    let mut any_launchable = false;
    let mut any_running = false;

    for slot in Slot::ALL {
        let view = hub.view(slot);

        any_launchable |= view.action == SlotAction::Launch && view.action_enabled;
        any_running |= view.running;

        // Only a changed row is written, so an open list or a pressed button under the
        // cursor is not rebuilt every second.
        let state = slot_state(&view);

        if states.row_data(slot.index()).as_ref() != Some(&state) {
            states.set_row_data(slot.index(), state);
        }
    }

    window.set_any_launchable(any_launchable);
    window.set_any_running(any_running);
}

fn slot_state(view: &SlotView) -> SlotState {
    SlotState {
        status: view.status.as_str().into(),
        can_browse: view.can_browse,
        action_label: match view.action {
            SlotAction::Launch => "Launch",
            SlotAction::Stop => "Stop",
            SlotAction::ForceStop => "Force stop",
        }
        .into(),
        action_enabled: view.action_enabled,
        plugin_text: view.plugin_text.as_str().into(),
        plugin_action: match view.plugin_action {
            Some(PluginAction::Install) => "Install",
            Some(PluginAction::Update) => "Update",
            None => "",
        }
        .into(),
        notice: view.notice.as_str().into(),
    }
}

fn slot_at(index: i32) -> Option<Slot> {
    Slot::ALL.get(usize::try_from(index).ok()?).copied()
}
