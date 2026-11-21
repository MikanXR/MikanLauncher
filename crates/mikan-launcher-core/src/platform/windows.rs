//! Win32: closing another process's windows.

#![expect(
    unsafe_code,
    reason = "Win32's window enumeration and message posting are C interfaces"
)]

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowThreadProcessId, IsWindowVisible, PostMessageW, WM_CLOSE,
};
use windows::core::BOOL;

/// What the enumeration looks for and what it found.
struct Search {
    pid: u32,
    posted: bool,
}

/// Posts `WM_CLOSE` to every visible top-level window the process owns.
pub fn request_close(pid: u32) -> bool {
    let mut search = Search { pid, posted: false };

    // SAFETY: the callback receives the pointer to `search` only during this call, which
    // returns before `search` goes out of scope.
    let _ = unsafe { EnumWindows(Some(visit), LPARAM(&raw mut search as isize)) };

    search.posted
}

unsafe extern "system" fn visit(window: HWND, parameter: LPARAM) -> BOOL {
    // SAFETY: `parameter` is the pointer `request_close` passed, live for the enumeration.
    let search = unsafe { &mut *(parameter.0 as *mut Search) };
    let mut owner = 0u32;

    // SAFETY: `window` is a handle the enumeration just produced, and `owner` outlives the call.
    unsafe { GetWindowThreadProcessId(window, Some(&raw mut owner)) };

    // SAFETY: both calls take the same enumerated handle and no pointers.
    if owner == search.pid
        && unsafe { IsWindowVisible(window) }.as_bool()
        && unsafe { PostMessageW(Some(window), WM_CLOSE, WPARAM(0), LPARAM(0)) }.is_ok()
    {
        search.posted = true;
    }

    true.into()
}
