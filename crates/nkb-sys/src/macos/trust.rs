//! Whether macOS lets this program type for the person.
//!
//! Two questions with one grant behind them, and both are asked: the system will
//! not post an event for a program that lacks the accessibility permission
//! (`OBS-70`), and the same permission is what the accessibility queries of
//! `pulse.rs` and `ax.rs` stand on. Measured 2026-10-19 on a bundle with and
//! without it, the two answers agreed every time (1/1 granted, 0/0 not).
//!
//! The permission belongs to the program the system holds responsible, which for
//! a command run in a terminal is the terminal. The sentence that says it is not
//! this module's: `nkb-adapters` has the words.
//!
//! Nothing here asks the system to show its prompt. A tool that pops a system
//! dialog on its own is changing the machine. The person is told what to grant
//! and where, and does it.

use super::ffi;

/// Whether the program may post keyboard events right now.
pub(crate) fn may_send() -> bool {
    unsafe { ffi::CGPreflightPostEventAccess() && ffi::AXIsProcessTrusted() != 0 }
}
