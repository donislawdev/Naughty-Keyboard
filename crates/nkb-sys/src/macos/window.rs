//! Keeping the keyboard where the tester put it, and lending it to a window.
//!
//! # What a window handle is here
//!
//! A `WindowRef` on macOS is the pid of a process (`front.rs`), and the handle
//! the toolkit gives the palette is the process the palette runs in. That is the
//! unit the system's own front application is told in, and it is enough for
//! everything below: nothing on macOS is done to a single window of ours.
//!
//! # What was measured before this was written
//!
//! Probed 2026-10-20 on macOS 27.0 (arm64) with `tools/sonda-paleta-macos`, a
//! Slint 1.18.1 window on winit 0.30.13 (`D138`, `macos_hotkeys.rs` has the rest):
//!
//! - a window that does not take the front at start needs THREE things at once -
//!   the accessory activation policy, no `activateIgnoringOtherApps` from winit and
//!   a window created not active. Left out, all three, each alone, and the two
//!   settings of winit without the policy took the front at start (5 runs of 5),
//!   and with all three the application under test kept it (17 runs of 17, and 2 of 2
//!   in a bundle with `LSUIElement`, which made no difference). The policy with
//!   only one of the two other settings was not tried. These are settings of the
//!   process and of the window before it exists, which the palette makes
//!   (`nkb-gui`), not calls on a handle.
//! - a click on such a window takes the front all the same: 15 of 15 runs, the
//!   application under test out of it within 21 ms. The Windows answer, a flag on
//!   the window, has no counterpart. What works is giving the front back: the
//!   Process Manager's `SetFrontProcess`, called once the front was ours, put the
//!   application back in front 40 to 65 ms after the press (6 of 6 runs), and the
//!   field it had held took the text typed after (3 of 3 runs with every event
//!   confirmed, the same text as a run with no click).
//! - the toolkit's own `focus_window` brings a second window of the process in
//!   front WITH the keyboard (3 letters of 3 reached it) and handing the front
//!   back to the application under test afterwards worked (3 of 3). `SetFrontProcess`
//!   on the process itself did not (1 run of 1): the Process Manager named the
//!   palette while `NSWorkspace` still named the application under test, no letter
//!   reached the second window, and the guard of the probe that reads `NSWorkspace`
//!   refused to type. So taking the front is the toolkit's, and this module only
//!   reads whether it happened.
//!
//! The front is read from the Process Manager (`front.rs`), and a hand-back is
//! believed only when the read shows it.

use core::time::Duration;

use super::{ffi, front};
use crate::WindowRef;
use crate::window::{FocusError, KeyMenuError};

/// How long the system is given to show a change of the front application.
///
/// The measured lag was 20 to 60 ms from the call to the read. This is six times
/// the worst of it, because a wait that is too short blames the system for our own
/// impatience and one that is too long is only spent when the answer is no.
const SETTLE: Duration = Duration::from_millis(360);

/// How often the front is looked at while waiting.
const LOOK: Duration = Duration::from_millis(5);

fn own(window: WindowRef) -> bool {
    window.0 == u64::from(std::process::id())
}

fn wait_until(mut done: impl FnMut() -> bool) -> bool {
    let start = std::time::Instant::now();
    loop {
        if done() {
            return true;
        }
        if start.elapsed() >= SETTLE {
            return false;
        }
        std::thread::sleep(LOOK);
    }
}

/// Whether `window` is a process of ours. Refusing the activation of a window is
/// not something done to a handle here, but a handle that is not our process is
/// no window of ours, and saying otherwise would be claiming a style that was
/// never set.
pub(crate) fn refuse_activation(window: WindowRef) -> Result<(), FocusError> {
    if own(window) {
        Ok(())
    } else {
        Err(FocusError::StyleRefused)
    }
}

/// Puts `previous` in front again, if `ours` holds the front.
pub(crate) fn hand_back_foreground(
    ours: WindowRef,
    previous: Option<WindowRef>,
) -> Result<(), FocusError> {
    if front::foreground_window() != Some(ours) {
        // We never took it, or have given it back already.
        return Ok(());
    }
    let Some(previous) = previous else {
        return Err(FocusError::StillForeground);
    };
    let mut psn = ffi::ProcessSerialNumber::default();
    let pid = i32::try_from(previous.0).unwrap_or(0);
    let called = pid > 0
        && i32::from(unsafe { ffi::GetProcessForPID(pid, &mut psn) }) == ffi::NO_ERR
        && i32::from(unsafe { ffi::SetFrontProcess(&psn) }) == ffi::NO_ERR;
    // The verdict from a READ, as everywhere in this crate: the call returning
    // success is not the application being in front.
    if called && wait_until(|| front::foreground_window() != Some(ours)) {
        Ok(())
    } else {
        Err(FocusError::StillForeground)
    }
}

/// Waits for `window` to be in front. The toolkit takes the front, before this
/// is asked (see the module text).
pub(crate) fn take_foreground(window: WindowRef) -> Result<(), FocusError> {
    // Never wait for a process that is not ours: it is not ours to bring up.
    if own(window) && wait_until(|| front::foreground_window() == Some(window)) {
        Ok(())
    } else {
        Err(FocusError::NotTaken)
    }
}

/// There is no window menu opened from the keyboard on macOS, so nothing takes
/// the next key after a shortcut with `Option` and the space bar.
pub(crate) fn no_keyboard_menu(window: WindowRef) -> Result<(), KeyMenuError> {
    if own(window) {
        Ok(())
    } else {
        Err(KeyMenuError::Refused)
    }
}
