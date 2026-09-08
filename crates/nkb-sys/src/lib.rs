//! Talking to the operating system, and nothing else.
//!
//! This package knows what a keystroke is. It does not know what a pack is, what
//! a value is, or that a catalogue exists - and it depends on no other crate of
//! ours, which is what keeps that true.
//!
//! # Why this crate exists at all
//!
//! The workspace sets `unsafe_code = "forbid"`, and `forbid` cannot be lifted
//! from inside the code: measured 2026-09-08, `#![allow(unsafe_code)]` under it
//! is `error[E0453]`. Rather than weaken the rule for four packages that have no
//! business calling the system, `unsafe` was given one small room of its own.
//! `tests/unsafe_lives_here_only.rs` is what keeps everyone else out of it.
//!
//! # What was measured before this was written
//!
//! A probe on 2026-09-08 (Windows 11, own test window, oracle computed before
//! the run) sent nine characters through `SendInput` with `KEYEVENTF_UNICODE`
//! and read back what the field actually held:
//!
//! - ASCII, `U+00A0`, `U+200B`, `U+4E2D` and `U+1F600` all arrived intact;
//! - the astral character crossed as a SURROGATE PAIR - nine characters became
//!   ten UTF-16 units and twenty events - and recombined on the far side;
//! - the system accepted all twenty events it was given;
//! - with the probe not run, the same window reported an empty field, so the
//!   measurement distinguishes arrival from intent.
//!
//! That is why the unit of work below is a UTF-16 code unit rather than a
//! `char`: the API takes one `u16` per event, so anything above the basic plane
//! is two events and there is no way to make it one.

/// What a successful send actually did.
///
/// Both numbers are reported because they differ, and the difference is the
/// astral characters: a caller that logs only one of them cannot tell a value
/// full of emoji from a value twice as long.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SendOutcome {
    /// UTF-16 code units sent. Two per character above the basic plane.
    pub units: usize,
    /// Key events sent. Two per unit - one down, one up.
    pub events: usize,
}

/// Why a send did not happen, or did not finish.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SendError {
    /// This system has no implementation yet. Never silence - untouchable
    /// rule 1 - so the caller is told which system it is standing on.
    Unsupported { system: &'static str },
    /// The system accepted fewer events than it was handed. The field now holds
    /// a partial value, and saying so is the whole point: a tool that reports
    /// success here would leave a half-written value looking like a whole one.
    Truncated {
        units_sent: usize,
        units_expected: usize,
    },
}

impl core::fmt::Display for SendError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Unsupported { system } => {
                write!(f, "sending keystrokes is not implemented on {system} yet")
            }
            Self::Truncated {
                units_sent,
                units_expected,
            } => write!(
                f,
                "the system accepted {units_sent} of {units_expected} UTF-16 units, \
                 so the field holds a partial value"
            ),
        }
    }
}

/// An opaque handle to whatever window is in front.
///
/// Deliberately opaque, and deliberately NOT a title. architektura.md section 5
/// makes "does not read the contents of windows" a promise held up by the
/// absence of a port; a window title is the thin end of that, and it has its own
/// type (`WindowTitle`) and its own port (`TargetInspector`) waiting for the day
/// it is genuinely needed. Comparing two of these answers "is the target still
/// the same one" without learning anything about either.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowRef(pub u64);

/// How many UTF-16 units go to the system in one call.
///
/// Not one big buffer: a value may legitimately be a million code points
/// (`E026`), which would be four million `INPUT` structures held at once. Not
/// one unit per call either, which would multiply the syscall count by the
/// length of the value. This is also the seam where step 3 will check that the
/// target has not changed mid-insert - architektura.md 6a, race `W2`.
const CHUNK_UNITS: usize = 512;

#[cfg(windows)]
mod windows_impl {
    use super::{CHUNK_UNITS, SendError, SendOutcome, WindowRef};

    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, SendInput,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

    /// One UTF-16 unit as the pair of events the system expects.
    fn events_for(unit: u16) -> [INPUT; 2] {
        let event = |flags: u32| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    // Zero: the unit below is a character, not a key. That is
                    // what KEYEVENTF_UNICODE means, and it is why layout does
                    // not matter - the probe sent CJK on a Polish keyboard.
                    wVk: 0,
                    wScan: unit,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        [
            event(KEYEVENTF_UNICODE),
            event(KEYEVENTF_UNICODE | KEYEVENTF_KEYUP),
        ]
    }

    pub fn foreground_window() -> Option<WindowRef> {
        let handle = unsafe { GetForegroundWindow() };
        if handle.is_null() {
            None
        } else {
            Some(WindowRef(handle as usize as u64))
        }
    }

    pub fn send_text(text: &str) -> Result<SendOutcome, SendError> {
        let units: Vec<u16> = text.encode_utf16().collect();
        let expected = units.len();
        let size = core::mem::size_of::<INPUT>() as i32;

        let mut sent_units = 0usize;
        let mut sent_events = 0usize;

        for chunk in units.chunks(CHUNK_UNITS) {
            let mut batch: Vec<INPUT> = Vec::with_capacity(chunk.len() * 2);
            for unit in chunk {
                batch.extend_from_slice(&events_for(*unit));
            }
            // Cast is checked rather than assumed: the chunk is bounded above by
            // CHUNK_UNITS, so this cannot narrow.
            let count = u32::try_from(batch.len()).unwrap_or(u32::MAX);
            let accepted = unsafe { SendInput(count, batch.as_ptr(), size) } as usize;

            sent_events += accepted;
            sent_units += accepted / 2;

            if accepted != batch.len() {
                // Stop at the first short write. Carrying on would send the rest
                // of the value into a field that just refused half of it.
                return Err(SendError::Truncated {
                    units_sent: sent_units,
                    units_expected: expected,
                });
            }
        }

        Ok(SendOutcome {
            units: sent_units,
            events: sent_events,
        })
    }
}

#[cfg(not(windows))]
mod other_impl {
    use super::{SendError, SendOutcome, WindowRef};

    /// Named rather than "this platform", so the message says something the
    /// reader can act on.
    const SYSTEM: &str = if cfg!(target_os = "macos") {
        "macOS"
    } else if cfg!(target_os = "linux") {
        "Linux"
    } else {
        "this system"
    };

    pub fn foreground_window() -> Option<WindowRef> {
        None
    }

    pub fn send_text(_text: &str) -> Result<SendOutcome, SendError> {
        // macOS is not merely unwritten: OBS-70 measured that `CGEventPost` is
        // refused outright without the accessibility permission, which cannot be
        // granted by a script. Linux depends on the session protocol - X11 has a
        // measured route, Wayland has a named candidate and no measurement.
        Err(SendError::Unsupported { system: SYSTEM })
    }
}

#[cfg(windows)]
use windows_impl as platform;

#[cfg(not(windows))]
use other_impl as platform;

/// The window in front right now, if there is one.
pub fn foreground_window() -> Option<WindowRef> {
    platform::foreground_window()
}

/// Send `text` to whatever holds the keyboard focus.
///
/// The caller decides WHAT to send and WHERE the focus should be by then; this
/// function only puts the characters on the wire.
pub fn send_text(text: &str) -> Result<SendOutcome, SendError> {
    platform::send_text(text)
}

/// Whether this build can deliver keystrokes at all.
///
/// Exists so a caller can say so BEFORE doing the work of building a value,
/// rather than after - and so the answer is one fact in one place instead of a
/// `cfg` repeated at every call site.
pub const fn can_send() -> bool {
    cfg!(windows)
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_text_sends_nothing_and_says_so() {
        // Whatever the platform, sending nothing must not be reported as having
        // sent something.
        match send_text("") {
            Ok(outcome) => assert_eq!(
                outcome,
                SendOutcome {
                    units: 0,
                    events: 0
                },
                "an empty text has no units and no events"
            ),
            Err(SendError::Unsupported { .. }) => {}
            Err(other) => panic!("an empty text must not be truncated: {other}"),
        }
    }

    #[test]
    fn the_unsupported_message_names_the_system_rather_than_shrugging() {
        // Untouchable rule 1: a run that did less than it promised has to say
        // so, and "not supported" without a name is not something a reader can
        // act on.
        if can_send() {
            return;
        }
        let Err(error) = send_text("x") else {
            panic!("a build that cannot send must not report success");
        };
        let text = error.to_string();
        assert!(
            text.contains("macOS") || text.contains("Linux") || text.contains("this system"),
            "the message must name the system, got: {text}"
        );
    }

    #[test]
    fn a_truncated_send_reports_both_halves_of_the_bad_news() {
        // The type is what carries the promise, so it is checked here even on
        // platforms that cannot produce it: a caller must be able to tell HOW
        // MUCH arrived, not merely that something went wrong.
        let error = SendError::Truncated {
            units_sent: 7,
            units_expected: 100,
        };
        let text = error.to_string();
        assert!(text.contains('7') && text.contains("100"), "got: {text}");
        assert!(
            text.contains("partial"),
            "the message must say the field is left partial, got: {text}"
        );
    }
}
