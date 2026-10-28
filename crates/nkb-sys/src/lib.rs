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
#[cfg(any(windows, target_os = "macos", target_os = "linux", test))]
mod cadence;
#[cfg(any(windows, target_os = "macos", test))]
mod confirm;
#[cfg(any(windows, target_os = "macos", target_os = "linux", test))]
mod escape;
pub mod field;
pub mod held;
pub mod hotkey;
pub mod layout;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(any(target_os = "linux", test))]
mod linux_atspi_rules;
#[cfg(any(target_os = "linux", test))]
mod linux_clearing_rules;
#[cfg(any(target_os = "linux", test))]
mod linux_hotkeys;
#[cfg(any(target_os = "linux", test))]
mod linux_ibus_rules;
#[cfg(any(target_os = "linux", test))]
mod linux_portal_rules;
#[cfg(any(target_os = "linux", test))]
mod linux_remap_rules;
#[cfg(any(target_os = "linux", test))]
mod linux_rules;
#[cfg(any(target_os = "linux", test))]
mod linux_window_rules;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(any(target_os = "macos", test))]
mod macos_hotkeys;
#[cfg(any(target_os = "macos", test))]
mod macos_rules;
pub mod portal;
pub mod privilege;
pub mod program;
pub mod screens;
pub mod spaces;
pub mod startup;
pub mod text_services;
pub mod window;

use core::sync::atomic::AtomicBool;

/// What a successful send actually did.
///
/// Both numbers are reported because they differ, and the difference is the
/// astral characters: a caller that logs only one of them cannot tell a value
/// full of emoji from a value twice as long.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SendOutcome {
    /// UTF-16 code units sent. Two per character above the basic plane.
    pub units: usize,
    /// Key events sent. Two per unit - one down, one up. On a route that hands
    /// the text over whole, the pieces it went in.
    pub events: usize,
    /// How far the send could be followed after it left, and so what the caller
    /// may say about where it went (`D95`, `OBS-158`, `D145`).
    pub following: Following,
}

/// How far a send could be followed after it left this program.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Following {
    /// Every event was taken by the application holding the keyboard focus, not
    /// only by the system (`D95`). Also what a send of nothing is: there was
    /// nothing to follow.
    Application,
    /// Only the system took the events: the queue of the application could not
    /// be followed, so characters may be missing if it was busy (`OBS-158`), and
    /// the caller has to say so.
    System,
    /// The text was handed whole to the desktop's input method, which gives it to
    /// whatever has the keyboard focus (Wayland, `D145`). No key was pressed and
    /// nothing after the hand-over can be followed: a focus that belongs to nobody
    /// stops the send before it (`D159`), but whether the field took the text
    /// cannot be told, so the caller says "handed", never "in the field".
    HandedOver,
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

/// What a send waits for besides the application it types into, said on the way
/// (`D145` point 76). Nothing is pressed while it waits, so the person is the only one
/// who can end it, and a send that stands without saying why looks like one that hangs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Waiting {
    /// The system is about to ask the person whether this program may press keys, because
    /// it holds no answer of theirs from before (the portal of a Wayland session, the first
    /// time). Said BEFORE the question is put, so that a window of ours that stands over
    /// everything can get out of the way of the window the system is about to show: one
    /// that leaves the top after that window has opened stays above it (`D145` point 77).
    Asking,
    /// The system is asking the person whether this program may press keys, and the send
    /// stands until they answer. Said once, when the question has stood for a moment: a
    /// desktop that remembers the answer asks nothing, and nothing is said.
    ForConsent,
    /// The question is answered, whichever way. Said once after every [`Waiting::Asking`]
    /// or [`Waiting::ForConsent`], so what was done for the question can be undone.
    Answered,
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
    /// A modifier went down while the send was going: `Alt`, which stops it at
    /// once, or another held longer than [`MODIFIER_RELEASE_WAIT`] (`D149`,
    /// `OBS-233`). `key` is its name on this system.
    KeyPressed { key: &'static str },
    /// The caller asked the send to stop through the flag it handed over: the
    /// palette is closing (R03). Answered like `Escape`, before the next event.
    Halted,
    /// The field the send typed into lost the keyboard focus, so the rest could
    /// have gone to another application (Wayland, `D159`). Said where the route
    /// can tell the field from the window, which on Wayland it cannot.
    FieldLeft,
    /// Nothing that takes typed text had the keyboard focus, so what the send
    /// would have handed over reaches nobody (Wayland, `D159`).
    NoField,
    /// The keyboard was switched to another layout group while the send was going,
    /// so a key of the layout would type a character of that one (X11, `D160`).
    LayoutChanged,
}

impl core::fmt::Display for StopReason {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Dropped => f.write_str("the system did not take the keys"),
            Self::FocusMoved => f.write_str("another window came to the front"),
            Self::NotTaking => f.write_str("the application stopped taking keys"),
            Self::Escape => f.write_str("Escape was pressed"),
            Self::KeyPressed { key } => write!(f, "{key} was pressed"),
            Self::Halted => f.write_str("the send was asked to stop"),
            Self::FieldLeft => f.write_str("the keyboard focus left the field"),
            Self::NoField => f.write_str("no field that takes typed text had the keyboard focus"),
            Self::LayoutChanged => f.write_str("the keyboard layout changed"),
        }
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
    /// The send stopped while keys it had sent were still on their way to the
    /// application (`D158`): `units_sent` arrived for certain, and the
    /// application holds keys it has not read yet, which it types whenever it
    /// reads them. So the count is a floor - possibly zero - and the field may
    /// get more of the value after the send is over. Apart from
    /// [`SendError::Truncated`], whose count is exact, because "nothing arrived"
    /// here would send the person to press again and get the value twice.
    InFlight {
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
    /// The system does not let this program type for the person: on macOS, the
    /// accessibility permission is not granted. Nothing was sent. Asked before
    /// every send, because the permission can be granted or taken back while the
    /// program runs - and because the system accepts the events of a program that
    /// lacks it and drops them without a word (`OBS-70`).
    NeedsPermission,
    /// Caps Lock is on, so nothing was sent. On X11 the toolkit turns the letters
    /// it is handed into capitals whatever key typed them (GTK, `OBS-202` point 8),
    /// so the field would hold another value than the one sent. A state somebody
    /// left on, not a key held: waiting for it would be waiting for them.
    CapsLockOn,
    /// The route cannot type this character at all, so nothing was sent. Refused
    /// by name before the first key rather than left out: `U+0000` is no keysym.
    CannotCarry { character: char },
    /// Another send of this program was typing on the same display for longer than
    /// one is waited for, so nothing was sent (`D157`). On X11 the keys of one send
    /// would be read by the other as keys of the layout. Passing: the same send may
    /// go once the other has ended.
    Busy,
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
            Self::InFlight {
                units_sent,
                units_expected,
                reason,
            } => write!(
                f,
                "{units_sent} of {units_expected} UTF-16 units are known to have arrived \
                 before the send stopped: {reason}, and more may still arrive"
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
            Self::NeedsPermission => {
                f.write_str("the system has not given this program permission to send keystrokes")
            }
            Self::CapsLockOn => {
                f.write_str("Caps Lock is on, so nothing was sent - turn it off first")
            }
            Self::CannotCarry { character } => write!(
                f,
                "U+{:04X} cannot be typed on this system, so nothing was sent",
                u32::from(*character)
            ),
            Self::Busy => {
                f.write_str("another send of this program is typing here, so nothing was sent")
            }
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

/// The handle that stands for "whatever has the keyboard focus", on a session that
/// cannot name the window in front: a Wayland compositor does not tell a client which
/// window has the focus (`D145`). Every question put to it answers "not known", and
/// two of them are equal whatever happened between, so it says nothing about the
/// focus having moved.
pub const UNNAMED_WINDOW: WindowRef = WindowRef(u64::MAX);

/// Whether `front`, the window in front right now, is still `target`, the window
/// the send was meant for (`D148`).
///
/// Asked once the held modifiers are up, which can take two seconds, and before
/// the first key. `false` presses nothing: the field was looked at in `target`,
/// and the clearing keys keep "never beyond the field" only in the field that was
/// confirmed (untouchable rule 17). Nothing in front is not `target` either. Two
/// unnamed windows are equal (a Wayland session, where the route types into
/// whatever has the focus and has no other answer to give).
pub(crate) fn still_in_front(target: WindowRef, front: Option<WindowRef>) -> bool {
    front == Some(target)
}

#[cfg(windows)]
mod windows_impl {
    use super::{Following, SendError, SendOutcome, SendProgress, StopReason, WindowRef};

    use super::cadence::{Cadence, PROGRESS_INTERVAL};
    use super::confirm::{
        Held, Pressed, Step, TAKEN_WAIT, TARGET_WAIT, Waits, Watch, chord_steps, chords_acted,
        press_confirmed, units_arrived,
    };
    use super::escape::{ReservedEscape, SystemEscape};
    use super::{Chord, MODIFIER_RELEASE_WAIT, NavKey};
    use core::sync::atomic::{AtomicBool, Ordering};
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
        MODIFIERS
            .iter()
            .find_map(|(key, name)| is_down(*key).then_some(*name))
    }

    /// Whether `key` is down right now, for the system. The high bit says "down
    /// right now". The low bit is history and is deliberately ignored.
    fn is_down(key: VIRTUAL_KEY) -> bool {
        let state = unsafe { GetAsyncKeyState(i32::from(key)) };
        (state as u16) & 0x8000 != 0
    }

    /// Why the wait before the first key ended with a modifier still down.
    enum NotReleased {
        /// Still held when the wait ran out.
        Held(&'static str),
        /// The caller asked the send to stop while it waited (R03).
        Halted,
    }

    /// Waits for every modifier to come up, for at most `MODIFIER_RELEASE_WAIT`,
    /// and no longer than the caller lets the send go on.
    fn wait_for_modifiers_released(halt: &AtomicBool) -> Result<(), NotReleased> {
        let deadline = std::time::Instant::now() + MODIFIER_RELEASE_WAIT;
        loop {
            let Some(key) = held_modifier() else {
                return Ok(());
            };
            if halt.load(Ordering::Relaxed) {
                return Err(NotReleased::Halted);
            }
            if std::time::Instant::now() >= deadline {
                return Err(NotReleased::Held(key));
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
    /// when the send began, with `Escape` held for as long as the send lasts and
    /// the caller's flag to stop it (R03).
    struct SystemWire<'a> {
        window: HWND,
        escape: ReservedEscape,
        halt: &'a AtomicBool,
    }

    impl super::confirm::Wire<INPUT> for SystemWire<'_> {
        fn send(&mut self, event: INPUT) -> bool {
            send_one(event)
        }
        fn system_down(&mut self, key: u16) -> bool {
            is_down(key)
        }
        fn target_down(&mut self, key: u16) -> bool {
            // Joined, our synchronous state IS the focused queue's (`D95`).
            let state = unsafe { GetKeyState(i32::from(key)) };
            (state as u16) & 0x8000 != 0
        }
        fn held(&mut self) -> Held {
            // The system's state, the one the next event meets: a key down when
            // `SendInput` takes our event is down for that event (`OBS-233`).
            Held {
                ctrl: is_down(VK_CONTROL),
                alt: is_down(VK_MENU),
                shift: is_down(VK_SHIFT),
                win: is_down(VK_LWIN) || is_down(VK_RWIN),
            }
        }
        fn watch(&mut self) -> Watch {
            // First: a person who pressed Escape asked for exactly this stop,
            // whatever else is true of the window by now.
            if self.escape.pressed() {
                Watch::Escape
            } else if self.halt.load(Ordering::Relaxed) {
                Watch::Halted
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
    struct Session<'a> {
        wire: SystemWire<'a>,
        joined: Option<Joined>,
    }

    impl<'a> Session<'a> {
        /// The session around `target`, or nothing when another window - or
        /// none - is in front by now (`D148`).
        fn begin(target: WindowRef, halt: &'a AtomicBool) -> Option<Self> {
            let window = unsafe { GetForegroundWindow() };
            if !super::still_in_front(target, window_ref(window)) {
                return None;
            }
            // Reserved on THIS thread, the one that asks it between events:
            // `WM_HOTKEY` goes to the thread that registered (`D96`).
            Some(Self {
                wire: SystemWire {
                    window,
                    escape: SystemEscape::reserve(),
                    halt,
                },
                joined: join_focus(window),
            })
        }

        fn waits(&self) -> Waits {
            Waits {
                system: TAKEN_WAIT,
                target: self.joined.as_ref().map(|_| TARGET_WAIT),
                release: MODIFIER_RELEASE_WAIT,
            }
        }

        fn press(&mut self, steps: &[Step<INPUT>]) -> Pressed {
            let waits = self.waits();
            press_confirmed(steps, &mut self.wire, waits)
        }
    }

    pub fn send_chords(
        chords: &[Chord],
        target: WindowRef,
        halt: &AtomicBool,
    ) -> Result<usize, SendError> {
        if chords.is_empty() {
            return Ok(0);
        }
        let nothing_pressed = |reason| SendError::ChordsTruncated {
            chords_sent: 0,
            chords_expected: chords.len(),
            reason,
        };
        wait_for_modifiers_released(halt).map_err(|why| match why {
            NotReleased::Held(key) => SendError::ModifierHeld { key },
            NotReleased::Halted => nothing_pressed(StopReason::Halted),
        })?;
        let Some(mut session) = Session::begin(target, halt) else {
            // Not the window the clearing was meant for, or none: nothing to
            // press into, and nothing pressed.
            return Err(nothing_pressed(StopReason::FocusMoved));
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

    /// Windows needs no permission to type: a window with higher privileges is
    /// refused per window (`privilege.rs`), not for the program as a whole.
    pub fn send_allowed() -> bool {
        true
    }

    /// The handle as the rest of the program compares it, or nothing for none.
    fn window_ref(handle: HWND) -> Option<WindowRef> {
        if handle.is_null() {
            None
        } else {
            Some(WindowRef(handle as usize as u64))
        }
    }

    pub fn foreground_window() -> Option<WindowRef> {
        window_ref(unsafe { GetForegroundWindow() })
    }

    pub fn send_text(
        text: &str,
        target: WindowRef,
        halt: &AtomicBool,
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
                following: Following::Application,
            });
        }
        let nothing_sent = |reason| SendError::Truncated {
            units_sent: 0,
            units_expected: expected,
            reason,
        };
        wait_for_modifiers_released(halt).map_err(|why| match why {
            NotReleased::Held(key) => SendError::ModifierHeld { key },
            NotReleased::Halted => nothing_sent(StopReason::Halted),
        })?;
        let Some(mut session) = Session::begin(target, halt) else {
            return Err(nothing_sent(StopReason::FocusMoved));
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
            // A character arrives as itself under `Shift` (`OBS-233`), so the
            // person's `Shift` holds no unit back.
            let steps = [
                Step {
                    event: down,
                    key: VK_PACKET,
                    down: true,
                    shift_counts: false,
                },
                Step {
                    event: up,
                    key: VK_PACKET,
                    down: false,
                    shift_counts: false,
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
            following: if session.joined.is_some() {
                Following::Application
            } else {
                Following::System
            },
        })
    }
}

#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
mod other_impl {
    use super::{SendError, SendOutcome, SendProgress, WindowRef};
    use core::sync::atomic::AtomicBool;

    /// Named rather than "this platform", so the message says something the
    /// reader can act on.
    const SYSTEM: &str = "this system";

    /// Nothing sends here, so nothing is allowed to.
    pub fn send_allowed() -> bool {
        false
    }

    pub fn foreground_window() -> Option<WindowRef> {
        None
    }

    pub fn send_text(
        _text: &str,
        _target: WindowRef,
        _halt: &AtomicBool,
        _progress: &mut dyn FnMut(SendProgress),
    ) -> Result<SendOutcome, SendError> {
        Err(SendError::Unsupported { system: SYSTEM })
    }

    pub fn send_chords(
        _chords: &[super::Chord],
        _target: WindowRef,
        _halt: &AtomicBool,
    ) -> Result<usize, SendError> {
        Err(SendError::Unsupported { system: SYSTEM })
    }
}

#[cfg(windows)]
use windows_impl as platform;

#[cfg(target_os = "macos")]
use macos as platform;

#[cfg(target_os = "linux")]
use linux as platform;

#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
use other_impl as platform;

/// The window in front right now, if there is one.
pub fn foreground_window() -> Option<WindowRef> {
    platform::foreground_window()
}

/// Send `text` to the field that holds the keyboard focus in `target`.
///
/// The caller decides WHAT to send and WHERE: `target` is the window the send was
/// meant for, read once when it began (`D148`). This function only puts the
/// characters on the wire. It waits up to [`MODIFIER_RELEASE_WAIT`] for a
/// physically held modifier to come up and refuses with
/// [`SendError::ModifierHeld`] if it does not. Another window in front once the
/// modifiers are up ends the send before its first key, with
/// [`StopReason::FocusMoved`]: the value does not follow the focus into a window
/// nobody looked at. Every event goes
/// alone and the next only once the system shows it taken (`D94`) and, where the
/// queue holding the keyboard focus could be joined, once that application took
/// it (`D95`) - [`SendOutcome::following`] says which. The window in front is looked
/// at before every event, and so is `Escape`, which the send holds for as long as
/// it lasts and gives back once the key is up (`D96`), and so are the modifiers on
/// Windows: `Alt` stops the send, `Ctrl` and `Win` hold the next key until they come
/// up (`D149`). A stop ends the send with [`SendError::Truncated`] and its
/// [`StopReason`].
///
/// `halt` is the caller's way to stop the send from another thread: set, it ends
/// the send where `Escape` would, with [`StopReason::Halted`] (R03, the palette
/// closing while a long value is still going). A Wayland session that waits for
/// the person in the window of its portal does not look at it.
///
/// `progress` hears how far the send got, on the sending thread, between two
/// units: at most once a tenth of a second and never during the first tenth, so
/// a short send says nothing on the way (`OBS-160`). It must return quickly -
/// the next key waits for it.
pub fn send_text(
    text: &str,
    target: WindowRef,
    halt: &AtomicBool,
    progress: &mut dyn FnMut(SendProgress),
) -> Result<SendOutcome, SendError> {
    platform::send_text(text, target, halt, progress)
}

/// Press `chords` in order, each as a complete press-and-release, in the field
/// that holds the keyboard focus in `target`. Returns how many chords were
/// pressed. Another window in front once held modifiers are up presses nothing,
/// as in [`send_text`] - for the clearing keys that is untouchable rule 17: the
/// field they may be pressed in was confirmed in `target` (`D148`).
///
/// These are the keys that are NOT content. The caller (one place in the
/// whole program, guarded there) decides the recipe. This function waits for
/// held modifiers exactly as [`send_text`] does and puts the presses on the
/// wire one chord at a time. On Windows the person's `Shift` also holds back the
/// first key of each chord, before the recipe's own `Shift` is down: `Shift+Home`
/// selects instead of going to the start of the line (`D149`).
///
/// # Errors
///
/// [`SendError::ModifierHeld`] when a modifier stays down past the wait,
/// [`SendError::ChordsTruncated`] when a press was not taken, another window came
/// to the front, the application stopped taking keys, `Escape` was pressed or a
/// modifier went down -
/// every event is confirmed before the next, as in [`send_text`] - and
/// [`SendError::Unsupported`] where there is no route.
///
/// `halt` stops the presses as in [`send_text`].
///
/// `waiting` hears what the send stands on besides the keys, on the sending thread: only a
/// Wayland session has anything to say (the window in which the person allows the keys,
/// `D145` point 76), and every other route never calls it. It must return quickly.
pub fn send_chords(
    chords: &[Chord],
    target: WindowRef,
    halt: &AtomicBool,
    waiting: &mut dyn FnMut(Waiting),
) -> Result<usize, SendError> {
    #[cfg(target_os = "linux")]
    {
        platform::send_chords(chords, target, halt, waiting)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = waiting;
        platform::send_chords(chords, target, halt)
    }
}

/// Whether the system lets this program send keystrokes RIGHT NOW.
///
/// Different from [`can_send`], which is a fact about the build: on macOS the
/// person has to grant the accessibility permission, and can take it back while
/// the program runs, so the answer is asked again before every send. Where there
/// is no route at all it is `false`, and [`can_send`] is the question that names
/// the system.
#[must_use]
pub fn send_allowed() -> bool {
    platform::send_allowed()
}

/// Whether this build, in this session, can press the keys that clear a field (`Home`,
/// `Shift+End`, `Delete`), which is a route of its own from typing text. Windows and macOS
/// always can. Linux can on a Wayland session with an input method (`D145` point 74): the
/// keys go through the remote-desktop portal, which the person allows once, and the engine of
/// the input method stands guard over each. It does not on an X11 session yet. On Linux this
/// makes one connection, to the input method daemon, and drops it.
#[must_use]
pub fn can_press_chords() -> bool {
    #[cfg(any(windows, target_os = "macos"))]
    {
        true
    }
    #[cfg(target_os = "linux")]
    {
        linux::can_press_chords()
    }
    #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
    {
        false
    }
}

/// Whether this build can deliver keystrokes at all.
///
/// Exists so a caller can say so BEFORE doing the work of building a value,
/// rather than after - and so the answer is one fact in one place instead of a
/// `cfg` repeated at every call site.
pub const fn can_send() -> bool {
    cfg!(any(windows, target_os = "macos", target_os = "linux"))
}

/// Why this build, on this machine and in this session, does not type - in the
/// words that follow `on` - when the system can say so before anything is sent.
///
/// A route can exist in principle and be shut for the session: on Linux there is
/// none to a display that is not on this machine, and none to a Wayland session
/// without an IBus daemon to hand the text to (`D145`). On Linux this makes one
/// connection, to the X server or to the daemon, and drops it. `None` means nothing
/// is known to stand in the way, not that a send will succeed.
#[must_use]
pub fn route_refusal() -> Option<&'static str> {
    #[cfg(target_os = "linux")]
    {
        linux::typing_refusal()
    }
    #[cfg(not(target_os = "linux"))]
    {
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

    #[test]
    fn an_empty_text_sends_nothing_and_says_so() {
        // Whatever the platform, sending nothing must not be reported as having
        // sent something.
        match send_text("", UNNAMED_WINDOW, &AtomicBool::new(false), &mut |_| {
            panic!("nothing to send, nothing to report")
        }) {
            Ok(outcome) => assert_eq!(
                outcome,
                SendOutcome {
                    units: 0,
                    events: 0,
                    following: Following::Application
                },
                "an empty text has no units and no events"
            ),
            Err(SendError::Unsupported { .. }) => {}
            Err(other) => panic!("an empty text must not be truncated: {other}"),
        }
    }

    #[test]
    fn a_route_shut_for_the_session_is_named_and_is_not_allowed() {
        // On Linux without an X server, as on a build server, the refusal says what
        // is in the way. Everywhere else nothing is known to be.
        if let Some(reason) = route_refusal() {
            assert!(!reason.is_empty(), "untouchable rule 1: the cause is named");
            assert!(!send_allowed(), "a shut route must not report itself open");
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
        let Err(error) = send_text("x", UNNAMED_WINDOW, &AtomicBool::new(false), &mut |_| {})
        else {
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
        match send_chords(&[], UNNAMED_WINDOW, &AtomicBool::new(false), &mut |_| {}) {
            Ok(pressed) => assert_eq!(pressed, 0),
            Err(SendError::Unsupported { .. }) => {}
            Err(other) => panic!("no chords must not fail this way: {other}"),
        }
    }

    #[test]
    fn only_the_window_the_send_was_meant_for_is_still_in_front() {
        // `D148`: the window a send was meant for, and no other, takes its keys.
        let meant = WindowRef(0x20);
        assert!(still_in_front(meant, Some(WindowRef(0x20))));
        assert!(
            !still_in_front(meant, Some(WindowRef(0x30))),
            "another window"
        );
        assert!(
            !still_in_front(meant, None),
            "nothing in front is not the window"
        );
        // A Wayland session names no window, and its route has no other answer.
        assert!(still_in_front(UNNAMED_WINDOW, Some(UNNAMED_WINDOW)));
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
            StopReason::KeyPressed { key: "Alt" },
            StopReason::Halted,
            StopReason::FieldLeft,
            StopReason::NoField,
            StopReason::LayoutChanged,
        ]
        .iter()
        .map(ToString::to_string)
        .collect();
        assert_eq!(words[4], "Alt was pressed", "the key is named");
        for (i, a) in words.iter().enumerate() {
            assert!(!a.is_empty());
            for b in &words[i + 1..] {
                assert_ne!(a, b, "two reasons must not read the same");
            }
        }
    }
}
