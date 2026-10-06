//! The `Escape` key during a send: ours for as long as one send lasts and not a
//! moment longer (`ux-spec.md` 3, race `W3`, `D96`).
//!
//! `Escape` is never a global shortcut - it is one of the most used keys in any
//! application, and a tool that held it would break the application under test
//! in a way nobody could diagnose. So it is reserved by the send itself: taken
//! when a send begins, given back when it ends, by one value that lives exactly
//! as long as the send. Two calls in two places would be the race `W3` names -
//! given back too late, the key is stolen from the application, taken too late,
//! it does not stop the first moments of a send.
//!
//! # What was measured before this was written
//!
//! A probe on 2026-10-06 (Windows 11, `tools/sondy/escape-w3.ps1`, a thread that
//! sends the way this crate does, joined to the queue of a WinForms field):
//!
//! - `RegisterHotKey` with no window on the SENDING thread delivers `WM_HOTKEY`
//!   to that thread while it is joined to the application's queue, 0.4-0.8 ms
//!   after the key went down.
//! - the application does not get the key-down. It does get the key-up - a
//!   release without a press, which applications act on rarely, and which no
//!   registration can take.
//! - repeats of a held key are taken too, for as long as the registration lasts.
//!   Given back while the key is still held, all 8 repeats of the probe reached
//!   the application - so the key is given back only once it is up.
//! - a press and release in one `SendInput` call fires `WM_HOTKEY`, while the
//!   asynchronous key state never showed the key down: watching the state alone
//!   would miss such a press.
//! - a second registration from another thread, while the first holds the key,
//!   is refused with `ERROR_HOTKEY_ALREADY_REGISTERED` (1409) - two sends at once,
//!   from two processes, cannot both hold it.
//! - no `WM_HOTKEY` was left in the queue after the key was given back, in any
//!   scene. The queue is emptied anyway, at both ends: a stale one would stop the
//!   next send before it began.
//!
//! The whole life of the reservation is pure - the system is an [`EscapeKey`],
//! five questions - so its order is tested on every system, while only Windows
//! answers them.

use core::time::Duration;

/// How long a held `Escape` is given to come up before it is given back anyway.
///
/// Its repeats belong to the press that stopped the send, and given back while
/// held they reach the application (measured, 8 of 8). Two seconds is longer than
/// any key-repeat delay and shorter than a person's patience: a key still down
/// after that is being held on purpose, for something that is not us - the same
/// reasoning as `MODIFIER_RELEASE_WAIT`.
pub(crate) const ESCAPE_RELEASE_WAIT: Duration = Duration::from_secs(2);

/// How long the wait for the release pauses between two looks.
const RELEASE_LOOK: Duration = Duration::from_millis(5);

/// The system, as the five things the reservation asks of it. A trait so a test
/// writes one fake key and sees the ORDER of what was asked.
pub(crate) trait EscapeKey {
    /// Takes the key from every application, for the calling thread. True when
    /// the system agreed.
    fn register(&mut self) -> bool;
    /// Gives the key back.
    fn unregister(&mut self);
    /// Whether the registered key fired since the last look, taking what fired
    /// out of the queue.
    fn fired(&mut self) -> bool;
    /// Whether the key is down right now (asynchronous state).
    fn down(&mut self) -> bool;
    /// Lets time pass between two looks.
    fn pause(&mut self, how_long: Duration);
}

/// `Escape`, held for as long as this value lives and given back on drop - on
/// every way out, a panic included - only once the key is up.
///
/// Registered, a press is the key firing. Not registered - another program holds
/// the key, refused with 1409 - a press is the key going down after it was seen
/// up: that program takes the key, the application under test does not get it
/// either, and the send still stops. A key already held when the send began is
/// not a press until it comes up and goes down again.
pub(crate) struct Reserved<K: EscapeKey> {
    key: K,
    registered: bool,
    was_down: bool,
    pressed: bool,
    release_wait: Duration,
}

impl<K: EscapeKey> Reserved<K> {
    /// Reserves the key, giving it back at most `release_wait` after the send
    /// if it is still held then.
    pub(crate) fn reserve(mut key: K, release_wait: Duration) -> Self {
        // A stale message from an earlier send would stop this one before it
        // began.
        let _ = key.fired();
        let registered = key.register();
        let was_down = key.down();
        Self {
            key,
            registered,
            was_down,
            pressed: false,
            release_wait,
        }
    }

    /// Whether `Escape` has been pressed since the send began. Once true, it
    /// stays true: a send stopped by the key does not resume.
    pub(crate) fn pressed(&mut self) -> bool {
        if !self.pressed {
            self.pressed = if self.registered {
                self.key.fired()
            } else {
                let down = self.key.down();
                let pressed = down && !self.was_down;
                self.was_down = down;
                pressed
            };
        }
        self.pressed
    }
}

impl<K: EscapeKey> Drop for Reserved<K> {
    fn drop(&mut self) {
        if !self.registered {
            return;
        }
        // Up first: the repeats of a held key belong to the press that stopped
        // the send, and given back while held they reach the application.
        let start = std::time::Instant::now();
        while self.key.down() && start.elapsed() < self.release_wait {
            self.key.pause(RELEASE_LOOK);
        }
        self.key.unregister();
        // What fired until then is ours and must not stop the next send.
        let _ = self.key.fired();
    }
}

#[cfg(windows)]
pub(crate) use platform::{ReservedEscape, SystemEscape};

#[cfg(windows)]
mod platform {
    use super::{ESCAPE_RELEASE_WAIT, EscapeKey, Reserved};
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, MOD_NOREPEAT, RegisterHotKey, UnregisterHotKey, VK_ESCAPE,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{MSG, PM_REMOVE, PeekMessageW, WM_HOTKEY};

    /// The `RegisterHotKey` id of the reserved key. Per thread, so it cannot meet
    /// the shortcuts of `hotkey`, which live on a thread of their own - distinct
    /// all the same, so a trace of the queue tells the two apart.
    const ESCAPE_ID: i32 = 0x4E4B;

    /// `Escape` on the calling thread. `WM_HOTKEY` goes to the thread that
    /// registered, so the one that reserves is the one that must ask.
    pub(crate) struct SystemEscape;

    /// `Escape` held by the sending thread for one send (`D96`).
    pub(crate) type ReservedEscape = Reserved<SystemEscape>;

    impl SystemEscape {
        pub(crate) fn reserve() -> ReservedEscape {
            Reserved::reserve(Self, ESCAPE_RELEASE_WAIT)
        }
    }

    impl EscapeKey for SystemEscape {
        fn register(&mut self) -> bool {
            // No window: posted to this thread's queue. No repeat: one message
            // per press - the repeats are taken all the same (measured).
            let accepted = unsafe {
                RegisterHotKey(
                    core::ptr::null_mut(),
                    ESCAPE_ID,
                    MOD_NOREPEAT,
                    u32::from(VK_ESCAPE),
                )
            };
            accepted != 0
        }
        fn unregister(&mut self) {
            unsafe { UnregisterHotKey(core::ptr::null_mut(), ESCAPE_ID) };
        }
        fn fired(&mut self) -> bool {
            // Only `WM_HOTKEY` is taken - the filter leaves anything else where
            // it is - and only ours counts.
            let mut fired = false;
            let mut message = MSG::default();
            while unsafe {
                PeekMessageW(
                    &mut message,
                    core::ptr::null_mut(),
                    WM_HOTKEY,
                    WM_HOTKEY,
                    PM_REMOVE,
                )
            } != 0
            {
                if message.wParam == ESCAPE_ID as usize {
                    fired = true;
                }
            }
            fired
        }
        fn down(&mut self) -> bool {
            // The high bit only: "down right now". The low bit is history.
            let state = unsafe { GetAsyncKeyState(i32::from(VK_ESCAPE)) };
            (state as u16) & 0x8000 != 0
        }
        fn pause(&mut self, how_long: core::time::Duration) {
            std::thread::sleep(how_long);
        }
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
    use std::cell::RefCell;
    use std::rc::Rc;

    /// Every question asked of the key, in order.
    type Log = Rc<RefCell<Vec<&'static str>>>;

    /// A scripted key, look by look, that writes down every question asked of it
    /// in a log the test keeps after the reservation is dropped.
    struct Key {
        registers: bool,
        /// What `fired` answers at each look, then false.
        fires: Vec<bool>,
        /// What `down` answers at each look, then the last answer.
        downs: Vec<bool>,
        look: usize,
        log: Log,
    }

    impl Key {
        fn new(registers: bool, fires: &[bool], downs: &[bool]) -> (Self, Log) {
            let log = Rc::new(RefCell::new(Vec::new()));
            let key = Self {
                registers,
                fires: fires.to_vec(),
                downs: downs.to_vec(),
                look: 0,
                log: Rc::clone(&log),
            };
            (key, log)
        }
    }

    impl EscapeKey for Key {
        fn register(&mut self) -> bool {
            self.log.borrow_mut().push("register");
            self.registers
        }
        fn unregister(&mut self) {
            self.log.borrow_mut().push("unregister");
        }
        fn fired(&mut self) -> bool {
            self.log.borrow_mut().push("fired");
            self.fires.get(self.look).copied().unwrap_or(false)
        }
        fn down(&mut self) -> bool {
            self.log.borrow_mut().push("down");
            self.downs
                .get(self.look)
                .or(self.downs.last())
                .copied()
                .unwrap_or(false)
        }
        fn pause(&mut self, how_long: Duration) {
            self.log.borrow_mut().push("pause");
            self.look += 1;
            std::thread::sleep(how_long);
        }
    }

    /// Asks `pressed` once per look, the look moving on after each answer.
    fn looks(reserved: &mut Reserved<Key>, times: usize) -> Vec<bool> {
        (0..times)
            .map(|_| {
                let pressed = reserved.pressed();
                reserved.key.look += 1;
                pressed
            })
            .collect()
    }

    /// Far above any test: a key the script leaves down would show as a slow
    /// test rather than pass unseen.
    const LONG_WAIT: Duration = Duration::from_secs(5);

    #[test]
    fn the_queue_is_emptied_before_the_key_is_taken() {
        // A stale press from the last send must not stop this one: it is taken
        // out BEFORE registering, so nothing of this send's can be lost in it.
        let (key, log) = Key::new(true, &[true], &[false]);
        let mut reserved = Reserved::reserve(key, LONG_WAIT);
        assert_eq!(*log.borrow(), vec!["fired", "register", "down"]);
        reserved.key.look = 1;
        assert!(!reserved.pressed(), "the stale press was not this send's");
    }

    #[test]
    fn registered_a_press_is_the_key_firing_and_it_stays_pressed() {
        let (key, _log) = Key::new(true, &[false, false, false, true, false], &[]);
        let mut reserved = Reserved::reserve(key, LONG_WAIT);
        reserved.key.look = 1;
        assert_eq!(
            looks(&mut reserved, 4),
            vec![false, false, true, true],
            "a send stopped by the key does not resume"
        );
    }

    #[test]
    fn registered_the_key_state_does_not_decide() {
        // A press and release in one call fires without the state ever showing
        // the key down (measured) - and a key shown down without firing is not
        // ours to call a press.
        let (key, _log) = Key::new(true, &[false, false], &[false, true, false]);
        let mut reserved = Reserved::reserve(key, LONG_WAIT);
        reserved.key.look = 1;
        assert!(!reserved.pressed());
    }

    #[test]
    fn not_registered_a_press_is_the_key_going_down_after_it_was_up() {
        let (key, _log) = Key::new(false, &[], &[false, false, false, true, true]);
        let mut reserved = Reserved::reserve(key, LONG_WAIT);
        reserved.key.look = 1;
        assert_eq!(looks(&mut reserved, 4), vec![false, false, true, true]);
    }

    #[test]
    fn not_registered_a_key_held_when_the_send_began_is_not_a_press() {
        // Held from before: only a release and a new press count.
        let (key, _log) = Key::new(false, &[], &[true, true, true, false, true]);
        let mut reserved = Reserved::reserve(key, LONG_WAIT);
        reserved.key.look = 1;
        assert_eq!(looks(&mut reserved, 4), vec![false, false, false, true]);
    }

    #[test]
    fn not_registered_what_fired_for_another_program_is_not_ours() {
        let (key, _log) = Key::new(false, &[false, true, true], &[false]);
        let mut reserved = Reserved::reserve(key, LONG_WAIT);
        reserved.key.look = 1;
        assert_eq!(looks(&mut reserved, 2), vec![false, false]);
    }

    #[test]
    fn the_key_is_given_back_only_once_it_is_up() {
        // Measured: given back while held, 8 of 8 repeats reached the
        // application. So the release is waited for BEFORE unregistering, and
        // what fired meanwhile is taken out after.
        let (key, log) = Key::new(true, &[], &[false, true, true, false]);
        let mut reserved = Reserved::reserve(key, LONG_WAIT);
        reserved.key.look = 1;
        drop(reserved);
        assert_eq!(
            *log.borrow(),
            vec![
                "fired",
                "register",
                "down",
                "down",
                "pause",
                "down",
                "pause",
                "down",
                "unregister",
                "fired"
            ]
        );
    }

    #[test]
    fn a_key_held_past_the_wait_is_given_back_anyway() {
        let bound = Duration::from_millis(20);
        let (key, log) = Key::new(true, &[], &[true]);
        let reserved = Reserved::reserve(key, bound);
        let start = std::time::Instant::now();
        drop(reserved);
        let elapsed = start.elapsed();
        assert!(elapsed >= bound, "gave up early: {elapsed:?}");
        assert!(elapsed < Duration::from_secs(5), "waited {elapsed:?}");
        assert_eq!(
            log.borrow()
                .iter()
                .rev()
                .take(2)
                .copied()
                .collect::<Vec<_>>(),
            vec!["fired", "unregister"],
            "given back at the bound, the queue emptied after"
        );
    }

    #[test]
    fn a_key_never_registered_is_never_given_back_or_waited_for() {
        // Another program holds it: unregistering would be asking to give back
        // what is not ours, and waiting for it would hold the send for nothing.
        let (key, log) = Key::new(false, &[], &[true]);
        drop(Reserved::reserve(key, LONG_WAIT));
        assert_eq!(*log.borrow(), vec!["fired", "register", "down"]);
    }

    #[test]
    fn the_release_wait_is_a_margin_not_a_tuned_number() {
        // Above any key-repeat delay (at most 1 s in the system settings), below
        // a person's patience - the bound `MODIFIER_RELEASE_WAIT` keeps too.
        assert!(ESCAPE_RELEASE_WAIT > Duration::from_secs(1));
        assert!(ESCAPE_RELEASE_WAIT <= crate::MODIFIER_RELEASE_WAIT);
    }
}
