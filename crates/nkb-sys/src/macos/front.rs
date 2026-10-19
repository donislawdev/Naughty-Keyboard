//! The program in front, and what it is called.
//!
//! The pid of the process in front, from the Process Manager (`GetFrontProcess`).
//! Measured 2026-10-19 against the three other ways to ask, in a process with no
//! run loop - which is how `nkb send` runs:
//!
//! | way | cost, median | follows a change |
//! |---|---|---|
//! | `NSWorkspace` | 3 us | NO: it kept answering the first application for the whole run |
//! | accessibility, focused application | 150-170 us | yes, with a 10 ms error in between |
//! | window list | 510-560 us | yes, up to half a second late |
//! | Process Manager | 22 us | yes, at once |
//!
//! `NSWorkspace` is the one a palette would reach for first and it is stale
//! without a run loop, so the answer comes from the Process Manager. It is
//! deprecated. If it ever stops answering, `None` says so and the send stops
//! rather than carrying on blind - the same as a window that cannot be read.
//!
//! The pid is the whole of what is known about the target here: a name for the
//! window and the program it belongs to are asked for separately, and the
//! program's name is cut down to its file (`program.rs`).

use super::ffi;
use crate::WindowRef;

/// The pid of the process in front, if the system says.
pub(crate) fn front_pid() -> Option<i32> {
    let mut psn = ffi::ProcessSerialNumber::default();
    if i32::from(unsafe { ffi::GetFrontProcess(&mut psn) }) != ffi::NO_ERR {
        return None;
    }
    let mut pid = 0i32;
    if i32::from(unsafe { ffi::GetProcessPID(&psn, &mut pid) }) != ffi::NO_ERR || pid <= 0 {
        return None;
    }
    Some(pid)
}

/// The application in front, as the handle the rest of the program compares.
pub(crate) fn foreground_window() -> Option<WindowRef> {
    front_pid()
        .and_then(|pid| u64::try_from(pid).ok())
        .map(WindowRef)
}

/// The path of the executable behind the application in front, as UTF-16 units
/// for `program.rs`, which cuts it down to a file name and drops the rest.
pub(crate) fn image_path(window: WindowRef) -> Option<Vec<u16>> {
    const MAX: usize = 4096;
    // Process zero is the kernel, which has a path of its own and no window.
    let pid = i32::try_from(window.0).ok().filter(|pid| *pid > 0)?;
    let mut buffer = vec![0u8; MAX];
    let length = unsafe { ffi::proc_pidpath(pid, buffer.as_mut_ptr().cast(), MAX as u32) };
    let length = usize::try_from(length).ok().filter(|length| *length > 0)?;
    let text = core::str::from_utf8(buffer.get(..length)?).ok()?;
    Some(text.encode_utf16().collect())
}
