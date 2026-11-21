//! The calls each platform spells its own way.

#[cfg(windows)]
mod windows;

/// Asks a process to close the way its user would, by its windows. Returns whether the
/// request reached any window. A platform with no such call returns false, and the caller
/// falls back to a signal.
#[must_use]
pub fn request_close(pid: u32) -> bool {
    #[cfg(windows)]
    {
        windows::request_close(pid)
    }

    #[cfg(not(windows))]
    {
        let _ = pid;

        false
    }
}
