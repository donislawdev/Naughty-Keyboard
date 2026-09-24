//! Whether the system will let our keystrokes into a window at all.
//!
//! # The measurement this stands on
//!
//! Measured 2026-09-23 (`OBS-128`), Windows 11, an administrator account with
//! UAC at its default: `SendInput` aimed at a window whose process runs with a
//! higher integrity level than ours returns FULL success, the clearing keys
//! included - and nothing reaches the field. The documentation says the same
//! thing about the return value: when input is blocked by UIPI, "neither
//! GetLastError nor the return value will indicate the failure". After the fact
//! a blocked send cannot be told from a delivered one.
//!
//! So the question is asked BEFORE: the integrity level of the window's process
//! against our own. A normal process read `0x3000` from an elevated one, so the
//! answer is there to be had.
//!
//! # Three answers, and why the third is not folded into either
//!
//! The same measurement surveyed every process on the machine: 125 of 397
//! refused the read (system services, processes of other accounts). "Could not
//! read" is therefore an ordinary answer, and it is NOT "higher": guessing a
//! cause the tool did not recognise is what `product-spec.md` 9.3 forbids, and a
//! wrong "higher" would send the tester to the clipboard for a window that takes
//! typing perfectly well. [`InputReach::Unknown`] keeps the send going exactly as
//! before this module existed.
//!
//! # What this reads, and what it does not
//!
//! One number: the mandatory-label RID of the process token behind the window,
//! and the same number for ourselves. The process is opened with
//! `PROCESS_QUERY_LIMITED_INFORMATION` and its token with `TOKEN_QUERY`, the
//! narrowest rights there are. Nothing about the window's title or contents is
//! touched - `architektura.md` 5 makes "does not read the contents of windows"
//! hold, and a label number is not content.

use crate::WindowRef;

/// Whether keystrokes sent now would reach `window`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputReach {
    /// The window's process runs at our level or below it. Typing works.
    Reachable,
    /// The window's process runs above our level. The system would drop every
    /// keystroke while reporting success (`OBS-128`).
    HigherPrivileges,
    /// One of the two levels could not be read. Not a guess in either
    /// direction: the send goes ahead as it always did.
    Unknown,
}

/// Whether keystrokes sent now would reach `window`.
///
/// Reads both levels fresh on every call. Nothing is cached: the adapter above
/// holds no state (`architektura.md` 6a), a process's level does not change
/// under it anyway, and two token reads cost microseconds next to a send.
#[must_use]
pub fn input_reach(window: WindowRef) -> InputReach {
    compare(platform::own_level(), platform::level_of_window(window))
}

/// The decision, apart from the reading, so it can be checked on every system.
///
/// A plain numeric comparison is the rule UIPI applies: a process may send input
/// to one at its own level or below. The standard levels are `0x1000` low,
/// `0x2000` medium, `0x3000` high and `0x4000` system, and the values between
/// them (medium-plus is `0x2100`) order the same way.
fn compare(own: Option<u32>, target: Option<u32>) -> InputReach {
    match (own, target) {
        (Some(own), Some(target)) if target > own => InputReach::HigherPrivileges,
        (Some(_), Some(_)) => InputReach::Reachable,
        _ => InputReach::Unknown,
    }
}

#[cfg(windows)]
mod platform {
    use crate::WindowRef;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, HWND};
    use windows_sys::Win32::Security::{
        GetSidSubAuthority, GetSidSubAuthorityCount, GetTokenInformation, TOKEN_MANDATORY_LABEL,
        TOKEN_QUERY, TokenIntegrityLevel,
    };
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;

    /// The most a mandatory label can take: the structure plus one SID with a
    /// single sub-authority is a few dozen bytes. A larger answer is not a label,
    /// and reading it would be trusting a length we did not expect.
    const LABEL_LIMIT: u32 = 1024;

    /// A handle this module opened, closed on every path out.
    struct Owned(HANDLE);

    impl Drop for Owned {
        fn drop(&mut self) {
            unsafe { CloseHandle(self.0) };
        }
    }

    /// The integrity RID of `process`, or `None` when any step refuses.
    fn level_of_process(process: HANDLE) -> Option<u32> {
        let mut token: HANDLE = core::ptr::null_mut();
        if unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) } == 0 {
            return None;
        }
        let token = Owned(token);

        // First call asks for the size. It "fails" by design and fills `needed`.
        let mut needed = 0u32;
        unsafe {
            GetTokenInformation(
                token.0,
                TokenIntegrityLevel,
                core::ptr::null_mut(),
                0,
                &mut needed,
            )
        };
        if needed == 0 || needed > LABEL_LIMIT {
            return None;
        }
        // `u64` elements, so the buffer is aligned for the pointer inside the
        // label - a byte vector would not promise that.
        let mut buffer = vec![0u64; (needed as usize).div_ceil(8)];
        if unsafe {
            GetTokenInformation(
                token.0,
                TokenIntegrityLevel,
                buffer.as_mut_ptr().cast(),
                needed,
                &mut needed,
            )
        } == 0
        {
            return None;
        }
        let label = unsafe { &*buffer.as_ptr().cast::<TOKEN_MANDATORY_LABEL>() };
        let sid = label.Label.Sid;
        if sid.is_null() {
            return None;
        }
        let count = unsafe { *GetSidSubAuthorityCount(sid) };
        if count == 0 {
            return None;
        }
        // The level is the LAST sub-authority of the label SID.
        Some(unsafe { *GetSidSubAuthority(sid, u32::from(count) - 1) })
    }

    pub fn own_level() -> Option<u32> {
        // A pseudo-handle: never closed, which is why it is not wrapped.
        level_of_process(unsafe { GetCurrentProcess() })
    }

    pub fn level_of_window(window: WindowRef) -> Option<u32> {
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(window.0 as usize as HWND, &mut pid) };
        if pid == 0 {
            return None;
        }
        let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if process.is_null() {
            return None;
        }
        let process = Owned(process);
        level_of_process(process.0)
    }
}

#[cfg(not(windows))]
mod platform {
    use crate::WindowRef;

    // No route to type into other windows exists here yet (`send_text` says so
    // by name), so there is nothing to compare against. `None` makes the answer
    // `Unknown`, never a claim about privileges this build cannot see.
    pub fn own_level() -> Option<u32> {
        None
    }

    pub fn level_of_window(_window: WindowRef) -> Option<u32> {
        None
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;

    const LOW: u32 = 0x1000;
    const MEDIUM: u32 = 0x2000;
    const MEDIUM_PLUS: u32 = 0x2100;
    const HIGH: u32 = 0x3000;
    const SYSTEM: u32 = 0x4000;

    #[test]
    fn only_a_strictly_higher_window_is_out_of_reach() {
        assert_eq!(
            compare(Some(MEDIUM), Some(HIGH)),
            InputReach::HigherPrivileges
        );
        assert_eq!(
            compare(Some(MEDIUM), Some(SYSTEM)),
            InputReach::HigherPrivileges
        );
        assert_eq!(
            compare(Some(MEDIUM), Some(MEDIUM_PLUS)),
            InputReach::HigherPrivileges
        );
        // Equal is reachable: an elevated tool types into an elevated window.
        assert_eq!(compare(Some(MEDIUM), Some(MEDIUM)), InputReach::Reachable);
        assert_eq!(compare(Some(HIGH), Some(HIGH)), InputReach::Reachable);
        // Lower is reachable: store apps run at low (measured: 18 processes).
        assert_eq!(compare(Some(MEDIUM), Some(LOW)), InputReach::Reachable);
        assert_eq!(compare(Some(HIGH), Some(MEDIUM)), InputReach::Reachable);
    }

    #[test]
    fn an_unread_level_is_unknown_and_never_higher() {
        // 125 of 397 processes refused the read on the measured machine. None of
        // those answers may be turned into a claim about privileges.
        assert_eq!(compare(Some(MEDIUM), None), InputReach::Unknown);
        assert_eq!(compare(None, Some(HIGH)), InputReach::Unknown);
        assert_eq!(compare(None, None), InputReach::Unknown);
    }

    #[test]
    fn our_own_level_is_readable_where_the_route_exists() {
        // Reads this test process's own token - no window, no other process.
        // Where there is a route, a level we cannot read about ourselves would
        // turn every answer into `Unknown` and switch the check off in silence.
        if !crate::can_send() {
            assert_eq!(platform::own_level(), None);
            return;
        }
        let Some(level) = platform::own_level() else {
            panic!("the test process could not read its own integrity level");
        };
        assert!((LOW..=SYSTEM).contains(&level), "got 0x{level:X}");
    }

    #[test]
    fn a_window_that_does_not_exist_is_unknown() {
        // A null handle has no process behind it: the read refuses, and the
        // answer must be `Unknown`, not `Reachable` and not `HigherPrivileges`.
        assert_eq!(input_reach(WindowRef(0)), InputReach::Unknown);
    }
}
