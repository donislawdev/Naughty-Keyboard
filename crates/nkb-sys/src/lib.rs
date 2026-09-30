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
//! - ASCII, `U+00A0`, `U+200B`, `U+4E2D` and `U+1F600` all arrived intact.
//! - the astral character crossed as a SURROGATE PAIR - nine characters became
//!   ten UTF-16 units and twenty events - and recombined on the far side.
//! - the system accepted all twenty events it was given.
//! - with the probe not run, the same window reported an empty field, so the
//!   measurement distinguishes arrival from intent.
//!
//! That is why the unit of work below is a UTF-16 code unit rather than a
//! `char`: the API takes one `u16` per event, so anything above the basic plane
//! is two events and there is no way to make it one.

pub mod field;
pub mod held;
pub mod hotkey;
pub mod layout;
pub mod privilege;
pub mod startup;
pub mod window;

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
    /// A modifier key is physically held and did not come up within the wait.
    /// Nothing was sent. Under a held `Ctrl`, `Home` is the start of the
    /// DOCUMENT and `Shift+End` selects to its end, so the clearing recipe
    /// would delete far beyond the field - and a held modifier changes what
    /// the value's own characters mean to many applications too. The moment a
    /// global hotkey fires is exactly the moment its modifiers are still down,
    /// so this is the ordinary case, not a corner.
    ModifierHeld { key: &'static str },
    /// The system accepted fewer chords than it was handed. The field is in an
    /// unknown state between untouched and cleared.
    ChordsTruncated {
        chords_sent: usize,
        chords_expected: usize,
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
            Self::ModifierHeld { key } => write!(
                f,
                "{key} is still held on the keyboard, so nothing was sent - release it first"
            ),
            Self::ChordsTruncated {
                chords_sent,
                chords_expected,
            } => write!(
                f,
                "the system accepted {chords_sent} of {chords_expected} key presses"
            ),
        }
    }
}

/// A key the tool presses on its own account. Mirrors `nkb_core::keys::Key`
/// without depending on it - this crate depends on nothing of ours, and the
/// adapter that knows both does the one-line mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavKey {
    Home,
    End,
    Delete,
}

/// One press, optionally with `Shift` held for its duration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chord {
    pub key: NavKey,
    pub shift: bool,
}

/// How long a held modifier is given to come up before the send is refused.
///
/// Two seconds is longer than any key repeat and shorter than a person's
/// patience: a hotkey's modifiers come up within a few hundred milliseconds
/// of the press, and a modifier still down after two seconds is being held
/// on purpose - for something that is not us.
pub const MODIFIER_RELEASE_WAIT: core::time::Duration = core::time::Duration::from_secs(2);

/// An opaque handle to whatever window is in front.
///
/// Deliberately opaque, and deliberately NOT a title. architektura.md section 5
/// makes "does not read the contents of windows" a promise held up by the
/// absence of a port. A window title is the thin end of that, and it has its own
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
///
/// Windows only, like the one route that reads it: elsewhere it was dead code,
/// and `clippy -D warnings` refused the workspace on macOS and Linux - unseen
/// until 2026-09-23, when clippy first ran there.
#[cfg(windows)]
const CHUNK_UNITS: usize = 512;

#[cfg(windows)]
mod windows_impl {
    use super::{CHUNK_UNITS, SendError, SendOutcome, WindowRef};

    use super::{Chord, MODIFIER_RELEASE_WAIT, NavKey};
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY,
        KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, SendInput, VIRTUAL_KEY, VK_CONTROL, VK_DELETE, VK_END,
        VK_HOME, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

    /// The modifiers whose being held changes what every other key means.
    const MODIFIERS: [(VIRTUAL_KEY, &str); 5] = [
        (VK_CONTROL, "Ctrl"),
        (VK_MENU, "Alt"),
        (VK_SHIFT, "Shift"),
        (VK_LWIN, "Win"),
        (VK_RWIN, "Win"),
    ];

    fn held_modifier() -> Option<&'static str> {
        MODIFIERS.iter().find_map(|(key, name)| {
            // The high bit says "down right now". The low bit is history and
            // is deliberately ignored.
            let state = unsafe { GetAsyncKeyState(i32::from(*key)) };
            ((state as u16) & 0x8000 != 0).then_some(*name)
        })
    }

    /// Waits for every modifier to come up, for at most `MODIFIER_RELEASE_WAIT`.
    fn wait_for_modifiers_released() -> Result<(), SendError> {
        let deadline = std::time::Instant::now() + MODIFIER_RELEASE_WAIT;
        loop {
            let Some(key) = held_modifier() else {
                return Ok(());
            };
            if std::time::Instant::now() >= deadline {
                return Err(SendError::ModifierHeld { key });
            }
            std::thread::sleep(core::time::Duration::from_millis(10));
        }
    }

    fn virtual_key(key: NavKey) -> VIRTUAL_KEY {
        match key {
            NavKey::Home => VK_HOME,
            NavKey::End => VK_END,
            NavKey::Delete => VK_DELETE,
        }
    }

    /// One virtual-key event. `KEYEVENTF_EXTENDEDKEY` marks the navigation
    /// cluster, so an application that tells `Home` from the numeric keypad's
    /// `7` sees the former.
    fn key_event(key: VIRTUAL_KEY, flags: u32) -> INPUT {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: key,
                    wScan: 0,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    /// The events of one chord: `Shift` down if asked, the key down and up
    /// (extended), `Shift` up if it went down.
    fn events_for_chord(chord: Chord) -> Vec<INPUT> {
        let key = virtual_key(chord.key);
        let mut events = Vec::with_capacity(4);
        if chord.shift {
            events.push(key_event(VK_SHIFT, 0));
        }
        events.push(key_event(key, KEYEVENTF_EXTENDEDKEY));
        events.push(key_event(key, KEYEVENTF_EXTENDEDKEY | KEYEVENTF_KEYUP));
        if chord.shift {
            events.push(key_event(VK_SHIFT, KEYEVENTF_KEYUP));
        }
        events
    }

    pub fn send_chords(chords: &[Chord]) -> Result<usize, SendError> {
        if chords.is_empty() {
            return Ok(0);
        }
        wait_for_modifiers_released()?;
        let size = core::mem::size_of::<INPUT>() as i32;
        // One call per chord, so a short write is reported in chords - the unit
        // the caller reasons in - and so `Shift` never stays down across a
        // refused chord.
        for (index, chord) in chords.iter().enumerate() {
            let batch = events_for_chord(*chord);
            let count = u32::try_from(batch.len()).unwrap_or(u32::MAX);
            let accepted = unsafe { SendInput(count, batch.as_ptr(), size) } as usize;
            if accepted != batch.len() {
                return Err(SendError::ChordsTruncated {
                    chords_sent: index,
                    chords_expected: chords.len(),
                });
            }
        }
        Ok(chords.len())
    }

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
        if expected == 0 {
            // Nothing to send, so nothing to wait for: an empty value is a real
            // catalogue entry (`len-0`) and must not fail on a held key.
            return Ok(SendOutcome {
                units: 0,
                events: 0,
            });
        }
        wait_for_modifiers_released()?;
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

    pub fn send_chords(_chords: &[super::Chord]) -> Result<usize, SendError> {
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
/// The caller decides WHAT to send and WHERE the focus should be by then. This
/// function only puts the characters on the wire. It waits up to
/// [`MODIFIER_RELEASE_WAIT`] for a physically held modifier to come up and
/// refuses with [`SendError::ModifierHeld`] if it does not.
pub fn send_text(text: &str) -> Result<SendOutcome, SendError> {
    platform::send_text(text)
}

/// Press `chords` in order, each as a complete press-and-release, on whatever
/// holds the keyboard focus. Returns how many chords were pressed.
///
/// These are the keys that are NOT content. The caller (one place in the
/// whole program, guarded there) decides the recipe. This function waits for
/// held modifiers exactly as [`send_text`] does and puts the presses on the
/// wire one chord at a time.
///
/// # Errors
///
/// [`SendError::ModifierHeld`] when a modifier stays down past the wait,
/// [`SendError::ChordsTruncated`] when the system accepted fewer presses than
/// it was handed, [`SendError::Unsupported`] where there is no route.
pub fn send_chords(chords: &[Chord]) -> Result<usize, SendError> {
    platform::send_chords(chords)
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
    fn no_chords_press_nothing_and_say_so() {
        match send_chords(&[]) {
            Ok(pressed) => assert_eq!(pressed, 0),
            Err(SendError::Unsupported { .. }) => {}
            Err(other) => panic!("no chords must not fail this way: {other}"),
        }
    }

    #[test]
    fn a_held_modifier_is_named_in_the_refusal() {
        // The message is what a person acts on: "release it" needs a name.
        let error = SendError::ModifierHeld { key: "Ctrl" };
        let text = error.to_string();
        assert!(
            text.contains("Ctrl") && text.contains("release"),
            "got: {text}"
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
