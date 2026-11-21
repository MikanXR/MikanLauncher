//! Starting an app, seeing whether it runs, and stopping it.

use std::io;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

use crate::platform;

/// Starts an executable with its own folder as the working directory, which is where
/// each of these apps looks for its resources.
///
/// # Errors
///
/// When the process cannot be created.
pub fn launch(exe: &Path, args: &[String]) -> io::Result<Child> {
    let mut command = Command::new(exe);

    // The app's output is its own: nothing of it comes back through the launcher.
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    if let Some(folder) = exe.parent().filter(|folder| !folder.as_os_str().is_empty()) {
        command.current_dir(folder);
    }

    command.spawn()
}

/// The machine's running processes, by executable path. An app counts as running whoever
/// started it, so a copy Steam relaunched or the user opened by hand is seen too.
pub struct ProcessTable {
    system: System,
}

impl ProcessTable {
    /// Reads the process list for the first time.
    ///
    /// # Errors
    ///
    /// When the platform refuses the process list. The launcher does not run without it,
    /// since it could then start a second copy of an app that is already open.
    pub fn new() -> Result<Self, sysinfo::Error> {
        let mut table = Self {
            system: System::new()?,
        };

        table.refresh();

        Ok(table)
    }

    /// Reads the process list again. Only executable paths are read, and only for
    /// processes that appeared since the last refresh.
    pub fn refresh(&mut self) {
        self.system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet),
        );
    }

    /// How many processes run this executable, as of the last refresh.
    #[must_use]
    pub fn count(&self, exe: &Path) -> usize {
        self.processes_of(exe).count()
    }

    /// Whether any process runs this executable, as of the last refresh.
    #[must_use]
    pub fn is_running(&self, exe: &Path) -> bool {
        self.count(exe) > 0
    }

    /// Asks every process running this executable to close: through its windows where
    /// the platform has that, and by a termination signal elsewhere.
    pub fn request_close(&self, exe: &Path) {
        for (pid, process) in self.processes_of(exe) {
            if !platform::request_close(pid) {
                process.kill_with(sysinfo::Signal::Term);
            }
        }
    }

    /// Ends every process running this executable without asking.
    pub fn kill(&self, exe: &Path) {
        for (_, process) in self.processes_of(exe) {
            process.kill();
        }
    }

    fn processes_of(&self, exe: &Path) -> impl Iterator<Item = (u32, &sysinfo::Process)> {
        let wanted = comparable(&normalize(exe));

        self.system
            .processes()
            .iter()
            .filter(move |(_, process)| {
                process
                    .exe()
                    .is_some_and(|running| comparable(running) == wanted)
            })
            .map(|(pid, process)| (pid.as_u32(), process))
    }
}

/// The path with its links resolved, in the plain form the process list reports.
fn normalize(path: &Path) -> PathBuf {
    let resolved = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_owned());

    // Windows' resolved form carries a verbatim prefix the process list's paths do not.
    match resolved
        .to_str()
        .and_then(|text| text.strip_prefix(r"\\?\"))
    {
        Some(plain) if !plain.starts_with("UNC\\") => PathBuf::from(plain),
        _ => resolved,
    }
}

/// The form two paths are compared in: Windows file names ignore case.
fn comparable(path: &Path) -> String {
    let text = path.to_string_lossy();

    if cfg!(windows) {
        text.to_lowercase()
    } else {
        text.into_owned()
    }
}
