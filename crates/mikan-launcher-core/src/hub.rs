//! The launcher's state behind its window: what each slot holds, whether its app runs,
//! and what the window should show and offer for it.

use std::path::{Path, PathBuf};
use std::process::Child;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::LoadError;
use crate::catalog::{AppEntry, CATALOG_NAME, Catalog, Slot, Source};
use crate::locate::{self, Resolved, SteamApps, SteamInstall};
use crate::plugins::{self, PluginPayload, PluginState};
use crate::process::{self, ProcessTable};
use crate::settings::Settings;

/// How long an app gets to close before the launcher offers to end it.
const FORCE_STOP_AFTER: Duration = Duration::from_secs(5);

/// How many ticks pass between looks for an app that was not found, so one installed
/// while the launcher is open turns up without a restart.
const RESOLVE_EVERY_TICKS: u32 = 5;

/// The folder under the package root that holds the plugin payloads.
const PLUGINS_DIR: &str = "plugins";

/// The file the packaging step writes, naming each bundled component's version.
const PACKAGE_NAME: &str = "package.json";

/// What the launcher reads from the machine around it.
pub struct Environment {
    pub steam: Box<dyn SteamApps>,
    /// `VNyan`'s `security.json`, where the platform has one.
    pub vnyan_security: Option<PathBuf>,
}

impl Environment {
    /// The real machine.
    #[must_use]
    pub fn detect() -> Self {
        // VNyan is a Unity app, which keeps its files under LocalLow, a sibling of the
        // local data folder with no lookup of its own.
        let vnyan_security = cfg!(windows)
            .then(directories::BaseDirs::new)
            .flatten()
            .and_then(|base| {
                Some(
                    base.data_local_dir()
                        .parent()?
                        .join("LocalLow")
                        .join("Suvidriel")
                        .join("VNyan")
                        .join("security.json"),
                )
            });

        Self {
            steam: Box::new(SteamInstall::locate()),
            vnyan_security,
        }
    }
}

/// What a slot's main button does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SlotAction {
    Launch,
    Stop,
    ForceStop,
}

/// What the plugin button does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginAction {
    Install,
    Update,
}

/// Everything the window shows for one slot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SlotView {
    pub status: String,
    pub running: bool,
    pub can_browse: bool,
    pub action: SlotAction,
    pub action_enabled: bool,
    /// Empty when the selected app takes no plugin.
    pub plugin_text: String,
    pub plugin_action: Option<PluginAction>,
    /// Problems and advice. Empty when there is nothing to say.
    pub notice: String,
}

/// The plugin's state for a slot, as last checked.
#[derive(Debug, Clone, Default)]
struct PluginStatus {
    text: String,
    action: Option<PluginAction>,
    notice: String,
}

#[derive(Debug, Default)]
struct SlotRuntime {
    resolved: Option<Resolved>,
    /// When the user asked the app to close.
    stopping: Option<Instant>,
    plugin: Option<PluginStatus>,
    /// The last thing that went wrong in this slot.
    error: String,
}

/// The launcher's state.
pub struct Hub {
    catalog: Catalog,
    settings: Settings,
    settings_path: PathBuf,
    package_root: PathBuf,
    environment: Environment,
    processes: ProcessTable,
    slots: [SlotRuntime; 3],
    /// Apps this launcher started, kept so their exit is collected.
    children: Vec<Child>,
    ticks: u32,
}

impl Hub {
    /// Opens the package at `package_root` with the settings at `settings_path`.
    ///
    /// # Errors
    ///
    /// When the package's catalog cannot be loaded or the process list cannot be read.
    pub fn open(
        package_root: &Path,
        settings_path: &Path,
        environment: Environment,
    ) -> Result<Self, LoadError> {
        let mut hub = Self {
            catalog: Catalog::load(&package_root.join(CATALOG_NAME))?,
            settings: Settings::load(settings_path),
            settings_path: settings_path.to_owned(),
            package_root: package_root.to_owned(),
            environment,
            processes: ProcessTable::new()?,
            slots: Default::default(),
            children: Vec::new(),
            ticks: 0,
        };

        for slot in Slot::ALL {
            hub.resolve(slot);
            hub.check_plugin(slot);
        }

        Ok(hub)
    }

    /// The names a slot's list offers: "None", then the catalog's apps for the slot.
    #[must_use]
    pub fn option_names(&self, slot: Slot) -> Vec<String> {
        std::iter::once("None".to_owned())
            .chain(self.catalog.apps_for(slot).map(|app| app.name.clone()))
            .collect()
    }

    /// The position of the slot's selection in [`Hub::option_names`].
    #[must_use]
    pub fn selected_index(&self, slot: Slot) -> usize {
        self.selected_app(slot).map_or(0, |selected| {
            self.catalog
                .apps_for(slot)
                .position(|app| app.id == selected.id)
                .map_or(0, |position| position + 1)
        })
    }

    /// The app the slot holds. A slot the user never touched holds nothing, except the
    /// editor slot, which starts on the catalog's first editor.
    #[must_use]
    pub fn selected_app(&self, slot: Slot) -> Option<&AppEntry> {
        match self.settings.selected.get(slot.key()) {
            Some(id) => self.catalog.find(slot, id),
            None if slot == Slot::Editor => self.catalog.apps_for(slot).next(),
            None => None,
        }
    }

    /// Puts the app at a position of [`Hub::option_names`] in the slot.
    pub fn select(&mut self, slot: Slot, option: usize) {
        let id = option
            .checked_sub(1)
            .and_then(|position| self.catalog.apps_for(slot).nth(position))
            .map(|app| app.id.clone())
            .unwrap_or_default();

        self.settings.selected.insert(slot.key().to_owned(), id);
        self.slots[slot.index()] = SlotRuntime::default();
        self.save_settings(slot);
        self.resolve(slot);
        self.check_plugin(slot);
    }

    /// Records the executable the user browsed to for the slot's app.
    pub fn set_manual_path(&mut self, slot: Slot, exe: &Path) {
        let Some(id) = self.selected_app(slot).map(|app| app.id.clone()) else {
            return;
        };

        self.settings.manual_paths.insert(id, exe.to_owned());
        self.slots[slot.index()].error.clear();
        self.save_settings(slot);
        self.resolve(slot);
        self.check_plugin(slot);
    }

    /// Does what the slot's main button currently offers.
    pub fn activate(&mut self, slot: Slot) {
        self.processes.refresh();

        match self.view(slot).action {
            SlotAction::Launch => self.launch(slot),
            SlotAction::Stop => self.stop(slot),
            SlotAction::ForceStop => {
                if let Some(resolved) = &self.slots[slot.index()].resolved {
                    self.processes.kill(&resolved.exe);
                }
            }
        }

        self.processes.refresh();
    }

    /// Starts every slot's app that is found and not running, the editor first.
    pub fn launch_all(&mut self) {
        for slot in Slot::ALL {
            self.launch(slot);
        }
    }

    /// Asks every running slot's app to close.
    pub fn stop_all(&mut self) {
        self.processes.refresh();

        for slot in Slot::ALL {
            self.stop(slot);
        }
    }

    /// Installs or updates the slot's plugin from the package.
    pub fn run_plugin_action(&mut self, slot: Slot) {
        let Some((payload, host_root)) = self.plugin_target(slot) else {
            return;
        };

        match payload.install(&host_root) {
            Ok(()) => {
                let manifest = payload.manifest();

                self.settings
                    .installed_plugins
                    .insert(manifest.id.clone(), manifest.version.clone());
                self.slots[slot.index()].error.clear();
                self.save_settings(slot);
            }
            Err(error) => {
                self.slots[slot.index()].error =
                    format!("The plugin could not be installed: {error}");
            }
        }

        self.check_plugin(slot);
    }

    /// The once-a-second pass: reads the process list, collects apps that exited, and
    /// looks again for apps that were not found.
    pub fn tick(&mut self) {
        self.processes.refresh();
        self.children
            .retain_mut(|child| !matches!(child.try_wait(), Ok(Some(_))));
        self.ticks = self.ticks.wrapping_add(1);

        for slot in Slot::ALL {
            if !self.is_running(slot) {
                self.slots[slot.index()].stopping = None;
            }

            if self.ticks.is_multiple_of(RESOLVE_EVERY_TICKS)
                && self.slots[slot.index()].resolved.is_none()
                && self.selected_app(slot).is_some()
            {
                self.resolve(slot);

                if self.slots[slot.index()].resolved.is_some() {
                    self.check_plugin(slot);
                }
            }
        }
    }

    /// Whether the slot's app runs, as of the last look at the process list.
    #[must_use]
    pub fn is_running(&self, slot: Slot) -> bool {
        self.slots[slot.index()]
            .resolved
            .as_ref()
            .is_some_and(|resolved| self.processes.is_running(&resolved.exe))
    }

    /// What the window shows for the slot.
    #[must_use]
    pub fn view(&self, slot: Slot) -> SlotView {
        let runtime = &self.slots[slot.index()];
        let plugin = runtime.plugin.clone().unwrap_or_default();
        let running = self.is_running(slot);

        let Some(app) = self.selected_app(slot) else {
            return SlotView {
                status: "Nothing is launched for this slot.".to_owned(),
                running: false,
                can_browse: false,
                action: SlotAction::Launch,
                action_enabled: false,
                plugin_text: String::new(),
                plugin_action: None,
                notice: runtime.error.clone(),
            };
        };

        let (status, action, action_enabled) = match (&runtime.resolved, runtime.stopping) {
            (None, _) => (
                format!("{} was not found. Use Browse to locate it.", app.name),
                SlotAction::Launch,
                false,
            ),
            (Some(resolved), _) if !running => (
                format!("Ready: {}", resolved.exe.display()),
                SlotAction::Launch,
                true,
            ),
            (Some(_), None) => ("Running".to_owned(), SlotAction::Stop, true),
            (Some(_), Some(since)) if since.elapsed() >= FORCE_STOP_AFTER => (
                format!(
                    "{} has not closed. Force stop ends it without saving.",
                    app.name
                ),
                SlotAction::ForceStop,
                true,
            ),
            (Some(_), Some(_)) => ("Closing...".to_owned(), SlotAction::Stop, false),
        };

        // A bundled app is found by the package's own layout, so Browse is offered for it
        // only when that layout failed.
        let bundled = app
            .sources
            .iter()
            .any(|source| matches!(source, Source::Bundled { .. }));

        SlotView {
            status,
            running,
            can_browse: !running && (runtime.resolved.is_none() || !bundled),
            action,
            action_enabled,
            plugin_text: plugin.text,
            plugin_action: plugin.action.filter(|_| runtime.stopping.is_none()),
            notice: [runtime.error.as_str(), plugin.notice.as_str()]
                .into_iter()
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join(" "),
        }
    }

    /// One line naming the bundled components and their versions, from the file the
    /// packaging step writes. Empty for a tree that has none.
    #[must_use]
    pub fn package_summary(&self) -> String {
        #[derive(serde::Deserialize)]
        struct Package {
            components: std::collections::BTreeMap<String, String>,
        }

        crate::read_json::<Package>(&self.package_root.join(PACKAGE_NAME)).map_or_else(
            |_| String::new(),
            |package| {
                package
                    .components
                    .iter()
                    .map(|(name, version)| format!("{name} {version}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            },
        )
    }

    fn launch(&mut self, slot: Slot) {
        let Some(app) = self.selected_app(slot).cloned() else {
            return;
        };

        self.resolve(slot);
        self.check_plugin(slot);
        self.processes.refresh();

        let Some(resolved) = self.slots[slot.index()].resolved.clone() else {
            return;
        };

        // Never a second copy: the editor in particular does not survive one.
        if self.processes.is_running(&resolved.exe) {
            return;
        }

        match process::launch(&resolved.exe, &app.args) {
            Ok(child) => {
                self.children.push(child);
                self.slots[slot.index()].error.clear();
            }
            Err(error) => {
                self.slots[slot.index()].error =
                    format!("{} could not be started: {error}", app.name);
            }
        }

        self.processes.refresh();
    }

    fn stop(&mut self, slot: Slot) {
        let runtime = &mut self.slots[slot.index()];
        let Some(resolved) = &runtime.resolved else {
            return;
        };

        if runtime.stopping.is_none() && self.processes.is_running(&resolved.exe) {
            self.processes.request_close(&resolved.exe);
            runtime.stopping = Some(Instant::now());
        }
    }

    fn resolve(&mut self, slot: Slot) {
        let resolved = self.selected_app(slot).and_then(|app| {
            locate::resolve(
                app,
                &self.package_root,
                self.environment.steam.as_ref(),
                self.settings
                    .manual_paths
                    .get(&app.id)
                    .map(PathBuf::as_path),
            )
        });

        self.slots[slot.index()].resolved = resolved;
    }

    /// The payload for the slot's app and the host folder it installs into, when the
    /// app takes a plugin, was found, and the package carries the payload.
    fn plugin_target(&self, slot: Slot) -> Option<(PluginPayload, PathBuf)> {
        let folder = self.selected_app(slot)?.plugin.as_ref()?;
        let resolved = self.slots[slot.index()].resolved.as_ref()?;
        let payload =
            PluginPayload::load(&self.package_root.join(PLUGINS_DIR).join(folder)).ok()?;

        Some((payload, resolved.install_dir().to_owned()))
    }

    fn check_plugin(&mut self, slot: Slot) {
        self.slots[slot.index()].plugin = self.plugin_status(slot);
    }

    fn plugin_status(&self, slot: Slot) -> Option<PluginStatus> {
        let app = self.selected_app(slot)?;
        let folder = app.plugin.as_ref()?;
        let resolved = self.slots[slot.index()].resolved.as_ref()?;
        let name = &app.name;

        let payload = match PluginPayload::load(&self.package_root.join(PLUGINS_DIR).join(folder)) {
            Ok(payload) => payload,
            Err(error) if error.is_not_found() => {
                return Some(PluginStatus {
                    text: format!("This package does not include the MikanXR plugin for {name}."),
                    ..PluginStatus::default()
                });
            }
            Err(error) => {
                return Some(PluginStatus {
                    text: format!("The MikanXR plugin for {name} could not be read: {error}"),
                    ..PluginStatus::default()
                });
            }
        };

        let version = &payload.manifest().version;
        let state = match payload.check(resolved.install_dir()) {
            Ok(state) => state,
            Err(error) => {
                return Some(PluginStatus {
                    text: format!("The MikanXR plugin in {name} could not be checked: {error}"),
                    ..PluginStatus::default()
                });
            }
        };

        let (text, action) = match state {
            PluginState::NotInstalled => (
                format!("The MikanXR plugin {version} is not installed in {name}."),
                Some(PluginAction::Install),
            ),
            PluginState::Differs => (
                format!("The MikanXR plugin in {name} differs from the bundled version {version}."),
                Some(PluginAction::Update),
            ),
            PluginState::UpToDate => (
                format!("The MikanXR plugin {version} is installed in {name}."),
                None,
            ),
        };

        // VNyan loads no plugin while its own switch is off, whatever is installed.
        let blocked = payload.manifest().host == "vnyan"
            && state != PluginState::NotInstalled
            && self
                .environment
                .vnyan_security
                .as_deref()
                .and_then(plugins::vnyan_mods_allowed)
                == Some(false);

        let notice = if blocked {
            "VNyan has third-party plugins turned off. Turn on \"Allow 3rd party Mods/Plugins\" \
             in VNyan's Misc settings, then restart VNyan."
                .to_owned()
        } else {
            String::new()
        };

        Some(PluginStatus {
            text,
            action,
            notice,
        })
    }

    fn save_settings(&mut self, slot: Slot) {
        if let Err(error) = self.settings.save(&self.settings_path) {
            self.slots[slot.index()].error = format!("The settings could not be saved: {error}");
        }
    }
}
