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

// Windows feeds it, the tests run it everywhere. Elsewhere it would be dead code,
// and `clippy -D warnings` refuses that on macOS and Linux (seen 2026-09-23).
#[cfg(any(windows, test))]
mod confirm;
#[cfg(any(windows, test))]
mod escape;
pub mod field;
pub mod held;
pub mod hotkey;
pub mod layout;
pub mod privilege;
pub mod program;
pub mod screens;
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
    /// Whether every event was also taken by the application holding the
    /// keyboard focus, not only by the system (`D95`). False when its queue
    /// could not be followed - then characters may be missing if it was busy
    /// (`OBS-158`), and the caller has to say so.
    pub paced: bool,
}

/// How far a send got, said on the way (`OBS-160`) - at most once per
/// `PROGRESS_INTERVAL`, and not at all for a send that ends sooner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SendProgress {
    /// UTF-16 units the application has taken so far - counted the way a
    /// stopped send counts them, so the last report and the end agree.
    pub units_arrived: usize,
    /// UTF-16 units in the whole text.
    pub units_total: usize,
}

/// Why a send stopped before its end (`D95`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    /// The system did not take an event - another program blocking input
    /// (`BlockInput`), a keyboard hook swallowing it, or a window with higher
    /// privileges (`D94`, all three measured to look the same).
    Dropped,
    /// Another window came to the front, so the next key would have landed
    /// there (race `W2`, at the level of the window).
    FocusMoved,
    /// The application holding the focus did not take an event: the system
    /// calls its window not responding, or it took nothing for as long.
    NotTaking,
    /// The person pressed `Escape`, which the send holds for as long as it
    /// lasts and no longer (race `W3`, `D96`).
    Escape,
}

impl core::fmt::Display for StopReason {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::Dropped => "the system did not take the keys",
            Self::FocusMoved => "another window came to the front",
            Self::NotTaking => "the application stopped taking keys",
            Self::Escape => "Escape was pressed",
        })
    }
}

/// Why a send did not happen, or did not finish.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SendError {
    /// This system has no implementation yet. Never silence - untouchable
    /// rule 1 - so the caller is told which system it is standing on.
    Unsupported { system: &'static str },
    /// Fewer units arrived than were handed over, and `reason` says why the
    /// send stopped (`D94`, `D95`). `units_sent` counts the units that arrived -
    /// possibly zero, which the caller must not call a partial value. Past zero
    /// the field holds a fragment, and saying so is the whole point: a tool that
    /// reports success here would leave a half-written value looking like a
    /// whole one.
    Truncated {
        units_sent: usize,
        units_expected: usize,
        reason: StopReason,
    },
    /// A modifier key is physically held and did not come up within the wait.
    /// Nothing was sent. Under a held `Ctrl`, `Home` is the start of the
    /// DOCUMENT and `Shift+End` selects to its end, so the clearing recipe
    /// would delete far beyond the field - and a held modifier changes what
    /// the value's own characters mean to many applications too. The moment a
    /// global hotkey fires is exactly the moment its modifiers are still down,
    /// so this is the ordinary case, not a corner.
    ModifierHeld { key: &'static str },
    /// Fewer chords acted than were handed over, as for [`SendError::Truncated`].
    /// `chords_sent` counts the chords whose main key went down - zero means
    /// nothing was pressed that could change the field (`D95` point 5). Nothing
    /// was pressed after the chord that stopped. Past zero the field is in an
    /// unknown state between untouched and cleared.
    ChordsTruncated {
        chords_sent: usize,
        chords_expected: usize,
        reason: StopReason,
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
                reason,
            } => write!(
                f,
                "{units_sent} of {units_expected} UTF-16 units arrived before the send \
                 stopped: {reason}"
            ),
            Self::ModifierHeld { key } => write!(
                f,
                "{key} is still held on the keyboard, so nothing was sent - release it first"
            ),
            Self::ChordsTruncated {
                chords_sent,
                chords_expected,
                reason,
            } => write!(
                f,
                "{chords_sent} of {chords_expected} key presses acted before the send \
                 stopped: {reason}"
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

#[cfg(windows)]
mod windows_impl {
    use super::{SendError, SendOutcome, SendProgress, StopReason, WindowRef};

    use super::confirm::{
        Cadence, PROGRESS_INTERVAL, Pressed, Step, TAKEN_WAIT, TARGET_WAIT, Waits, Watch,
        chord_steps, chords_acted, press_confirmed, units_arrived,
    };
    use super::escape::{ReservedEscape, SystemEscape};
    use super::{Chord, MODIFIER_RELEASE_WAIT, NavKey};
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, GetFocus, GetKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT,
        KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, SendInput, VIRTUAL_KEY,
        VK_CONTROL, VK_DELETE, VK_END, VK_HOME, VK_LWIN, VK_MENU, VK_PACKET, VK_RWIN, VK_SHIFT,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GUITHREADINFO, GetForegroundWindow, GetGUIThreadInfo, GetWindowThreadProcessId,
        IsHungAppWindow, MSG, PM_NOREMOVE, PeekMessageW,
    };

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

    /// The event for `key`, down or up. Navigation keys carry
    /// `KEYEVENTF_EXTENDEDKEY`, `Shift` does not.
    fn chord_event(key: VIRTUAL_KEY, release: bool) -> INPUT {
        let extended = if key == VK_SHIFT {
            0
        } else {
            KEYEVENTF_EXTENDEDKEY
        };
        key_event(key, extended | if release { KEYEVENTF_KEYUP } else { 0 })
    }

    /// One event to the system. True when it accepted it - which is not yet
    /// "it arrived" (`confirm`).
    fn send_one(event: INPUT) -> bool {
        let size = core::mem::size_of::<INPUT>() as i32;
        unsafe { SendInput(1, &event, size) == 1 }
    }

    /// Our thread joined to the input queue of the thread that holds the
    /// keyboard focus, for as long as one send lasts (`D95`). Leaves on drop -
    /// on every way out, a panic included - because a thread left joined would
    /// share that application's keyboard state for the rest of our life.
    struct Joined {
        ours: u32,
        theirs: u32,
    }

    impl Drop for Joined {
        fn drop(&mut self) {
            unsafe { AttachThreadInput(self.ours, self.theirs, 0) };
        }
    }

    /// Joins the queue that the keys will go to, or says why it cannot.
    ///
    /// The queue is the one of the window HOLDING THE FOCUS, which need not be
    /// the thread of the window in front. And joining is real only if, once
    /// joined, `GetFocus` names a window: that is the system saying the keyboard
    /// focus is in the queue we joined. Anything else - our own thread, a join
    /// the system refuses, a focus held in another queue (applications framed by
    /// another process) - is `None`, and the send goes on unpaced and says so.
    fn join_focus(window: HWND) -> Option<Joined> {
        let mut process = 0u32;
        let front = unsafe { GetWindowThreadProcessId(window, &mut process) };
        if front == 0 {
            return None;
        }
        let mut info = GUITHREADINFO {
            cbSize: core::mem::size_of::<GUITHREADINFO>() as u32,
            ..GUITHREADINFO::default()
        };
        let theirs =
            if unsafe { GetGUIThreadInfo(front, &mut info) } != 0 && !info.hwndFocus.is_null() {
                unsafe { GetWindowThreadProcessId(info.hwndFocus, core::ptr::null_mut()) }
            } else {
                front
            };
        let ours = unsafe { GetCurrentThreadId() };
        if theirs == 0 || theirs == ours {
            // Our own queue would never show the keys taken - we do not take
            // them - so following it would wait for nothing.
            return None;
        }
        // A thread without a message queue cannot be joined, and asking for a
        // message gives it one. `PM_NOREMOVE` takes nothing out, and the
        // threads that send have no windows that a sent message could reach.
        let mut message = MSG::default();
        unsafe { PeekMessageW(&mut message, core::ptr::null_mut(), 0, 0, PM_NOREMOVE) };
        if unsafe { AttachThreadInput(ours, theirs, 1) } == 0 {
            return None;
        }
        let joined = Joined { ours, theirs };
        if unsafe { GetFocus() }.is_null() {
            return None;
        }
        Some(joined)
    }

    /// The system as `confirm` asks it, around the window that was in front
    /// when the send began, with `Escape` held for as long as the send lasts.
    struct SystemWire {
        window: HWND,
        escape: ReservedEscape,
    }

    impl super::confirm::Wire<INPUT> for SystemWire {
        fn send(&mut self, event: INPUT) -> bool {
            send_one(event)
        }
        fn system_down(&mut self, key: u16) -> bool {
            // The high bit only, as in `held_modifier`.
            let state = unsafe { GetAsyncKeyState(i32::from(key)) };
            (state as u16) & 0x8000 != 0
        }
        fn target_down(&mut self, key: u16) -> bool {
            // Joined, our synchronous state IS the focused queue's (`D95`).
            let state = unsafe { GetKeyState(i32::from(key)) };
            (state as u16) & 0x8000 != 0
        }
        fn watch(&mut self) -> Watch {
            // First: a person who pressed Escape asked for exactly this stop,
            // whatever else is true of the window by now.
            if self.escape.pressed() {
                Watch::Escape
            } else if unsafe { GetForegroundWindow() } != self.window {
                Watch::Moved
            } else if unsafe { IsHungAppWindow(self.window) } != 0 {
                Watch::Hung
            } else {
                Watch::Steady
            }
        }
        fn pause(&mut self, how_long: core::time::Duration) {
            std::thread::sleep(how_long);
        }
    }

    /// The window in front, the queue joined to it if it could be, and the
    /// waits that follow from that. One per send: `Escape` is given back and the
    /// queue is left on drop - in that order, the fields' own.
    struct Session {
        wire: SystemWire,
        joined: Option<Joined>,
    }

    impl Session {
        fn begin() -> Option<Self> {
            let window = unsafe { GetForegroundWindow() };
            if window.is_null() {
                return None;
            }
            // Reserved on THIS thread, the one that asks it between events:
            // `WM_HOTKEY` goes to the thread that registered (`D96`).
            Some(Self {
                wire: SystemWire {
                    window,
                    escape: SystemEscape::reserve(),
                },
                joined: join_focus(window),
            })
        }

        fn waits(&self) -> Waits {
            Waits {
                system: TAKEN_WAIT,
                target: self.joined.as_ref().map(|_| TARGET_WAIT),
            }
        }

        fn press(&mut self, steps: &[Step<INPUT>]) -> Pressed {
            let waits = self.waits();
            press_confirmed(steps, &mut self.wire, waits)
        }
    }

    pub fn send_chords(chords: &[Chord]) -> Result<usize, SendError> {
        if chords.is_empty() {
            return Ok(0);
        }
        wait_for_modifiers_released()?;
        let Some(mut session) = Session::begin() else {
            // Nothing in front: nothing to press into, and nothing pressed.
            return Err(SendError::ChordsTruncated {
                chords_sent: 0,
                chords_expected: chords.len(),
                reason: StopReason::FocusMoved,
            });
        };
        // Each event alone, the next only once it is shown (`D94`) and, when the
        // focused queue could be joined, taken (`D95`). A chord that stopped is
        // reported in chords that ACTED, and nothing is pressed after it.
        for (index, chord) in chords.iter().enumerate() {
            let key = virtual_key(chord.key);
            let shift = chord.shift.then_some(VK_SHIFT);
            let steps = chord_steps(key, shift, chord_event);
            let pressed = session.press(&steps);
            if let Some(reason) = pressed.stop {
                // Unconfirmed on purpose: a key the system did not take is not
                // down, so its release is harmless - and one it took must not
                // stay down, least of all `Shift`.
                let _ = send_one(chord_event(key, true));
                if let Some(shift) = shift {
                    let _ = send_one(chord_event(shift, true));
                }
                return Err(SendError::ChordsTruncated {
                    chords_sent: chords_acted(index, pressed.taken, shift.is_some()),
                    chords_expected: chords.len(),
                    reason,
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

    pub fn send_text(
        text: &str,
        progress: &mut dyn FnMut(SendProgress),
    ) -> Result<SendOutcome, SendError> {
        let units: Vec<u16> = text.encode_utf16().collect();
        let expected = units.len();
        if expected == 0 {
            // Nothing to send, so nothing to wait for: an empty value is a real
            // catalogue entry (`len-0`) and must not fail on a held key. Nothing
            // needed following, so nothing was sent unpaced either.
            return Ok(SendOutcome {
                units: 0,
                events: 0,
                paced: true,
            });
        }
        wait_for_modifiers_released()?;
        let Some(mut session) = Session::begin() else {
            return Err(SendError::Truncated {
                units_sent: 0,
                units_expected: expected,
                reason: StopReason::FocusMoved,
            });
        };

        // One unit at a time, each event shown by the system (`D94`) and, when
        // the focused queue could be joined, taken by the application before
        // the next (`D95`) - so no two key-downs ever wait in its queue to be
        // combined into one (`OBS-158`). Not one buffer: a value may legitimately
        // be a million code points (`E026`). And not chunks: a chunk confirmed by
        // its last event hides a hole in its middle. Each unit is also where the
        // window in front is looked at (`W2`).
        let mut cadence = Cadence::starting(std::time::Instant::now(), PROGRESS_INTERVAL);
        for (index, unit) in units.iter().enumerate() {
            let [down, up] = events_for(*unit);
            let steps = [
                Step {
                    event: down,
                    key: VK_PACKET,
                    down: true,
                },
                Step {
                    event: up,
                    key: VK_PACKET,
                    down: false,
                },
            ];
            let pressed = session.press(&steps);
            if let Some(reason) = pressed.stop {
                // Always, unconfirmed: a down that was not taken makes the
                // release harmless, and a down that WAS taken - a stop between
                // the two halves of a unit, which `Escape` makes common - must
                // not leave the key held for the system and the next send
                // (`D96` ⊕). In another window it is a release without a press.
                let _ = send_one(up);
                // Stop at the first unit not taken. Carrying on would send the
                // rest of the value into a field that just lost part of it.
                return Err(SendError::Truncated {
                    units_sent: units_arrived(index, pressed.taken),
                    units_expected: expected,
                    reason,
                });
            }
            // Between units, after this one was taken: the count is what the
            // application holds, never what is merely on its way.
            if cadence.due(std::time::Instant::now()) {
                progress(SendProgress {
                    units_arrived: index + 1,
                    units_total: expected,
                });
            }
        }

        Ok(SendOutcome {
            units: expected,
            events: expected * 2,
            paced: session.joined.is_some(),
        })
    }
}

#[cfg(not(windows))]
mod other_impl {
    use super::{SendError, SendOutcome, SendProgress, WindowRef};

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

    pub fn send_text(
        _text: &str,
        _progress: &mut dyn FnMut(SendProgress),
    ) -> Result<SendOutcome, SendError> {
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
/// refuses with [`SendError::ModifierHeld`] if it does not. Every event goes
/// alone and the next only once the system shows it taken (`D94`) and, where the
/// queue holding the keyboard focus could be joined, once that application took
/// it (`D95`) - [`SendOutcome::paced`] says which. The window in front is looked
/// at before every event, and so is `Escape`, which the send holds for as long as
/// it lasts and gives back once the key is up (`D96`). A stop ends the send with
/// [`SendError::Truncated`] and its [`StopReason`].
///
/// `progress` hears how far the send got, on the sending thread, between two
/// units: at most once a tenth of a second and never during the first tenth, so
/// a short send says nothing on the way (`OBS-160`). It must return quickly -
/// the next key waits for it.
pub fn send_text(
    text: &str,
    progress: &mut dyn FnMut(SendProgress),
) -> Result<SendOutcome, SendError> {
    platform::send_text(text, progress)
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
/// [`SendError::ChordsTruncated`] when a press was not taken, another window came
/// to the front, the application stopped taking keys or `Escape` was pressed -
/// every event is confirmed before the next, as in [`send_text`] - and
/// [`SendError::Unsupported`] where there is no route.
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
        match send_text("", &mut |_| panic!("nothing to send, nothing to report")) {
            Ok(outcome) => assert_eq!(
                outcome,
                SendOutcome {
                    units: 0,
                    events: 0,
                    paced: true
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
        let Err(error) = send_text("x", &mut |_| {}) else {
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
            reason: StopReason::NotTaking,
        };
        let text = error.to_string();
        assert!(text.contains('7') && text.contains("100"), "got: {text}");
        // `D95`: and WHY it stopped. Whether the field holds a fragment is the
        // caller's sentence - at zero it holds none (`OBS-157`).
        assert!(
            text.contains("stopped taking"),
            "the message must name why the send stopped, got: {text}"
        );
    }

    #[test]
    fn every_reason_a_send_stops_has_its_own_words() {
        let words: Vec<String> = [
            StopReason::Dropped,
            StopReason::FocusMoved,
            StopReason::NotTaking,
            StopReason::Escape,
        ]
        .iter()
        .map(ToString::to_string)
        .collect();
        for (i, a) in words.iter().enumerate() {
            assert!(!a.is_empty());
            for b in &words[i + 1..] {
                assert_ne!(a, b, "two reasons must not read the same");
            }
        }
    }
}
