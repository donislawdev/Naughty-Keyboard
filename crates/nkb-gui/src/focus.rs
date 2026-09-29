//! Making the palette refuse the keyboard focus, once there is a window to ask.
//!
//! # Why this is a poll and not a line after `show()`
//!
//! Slint hands out a window handle only after the window manager has created the
//! window, which its own documentation places at "typically after at least one
//! iteration of the event loop following a call to `show()`". Measured
//! 2026-09-22 with `tools/sonda-ognisko` on Windows 11, that is **258 to 489 ms**
//! after the first ask, and the first timer tick does not even run before then.
//! A fixed delay would therefore be a guess dressed as a number, and a guess that
//! is too short fails silently: `refuse_for` would be handed nothing.
//!
//! So this asks repeatedly, acts the moment the handle exists, and gives up
//! loudly after [`HANDLE_WAIT`].
//!
//! # What runs where
//!
//! All of it on the MAIN thread. The window belongs to Slint and a `Weak` is the
//! only handle that may cross a thread boundary, so the worker in `live` cannot
//! do this and does not try. The two threads meet at [`Standing`], which is a
//! sentence rather than a state: the worker rebuilds the message band on every
//! view, so a sentence produced here has to be somewhere the worker will read it
//! again.
//!
//! # What this does NOT do
//!
//! It does not keep watch. The flag and the hand-back happen once, and measured
//! in the same probe they survive both the hand-back and later writes to the
//! window's properties. A palette that re-checked every second would be spending
//! a timer on an answer that does not change.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use nkb_adapters::KeptFocus;
use nkb_adapters::i18n::{self, Environment};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};

use crate::Palette;

/// How long the window manager is given to produce a window handle.
///
/// Six times the slowest measurement on this machine. Generous on purpose: the
/// cost of waiting is that the palette holds the focus a moment longer, and the
/// cost of giving up early is a sentence that blames the environment for our own
/// impatience.
pub const HANDLE_WAIT: Duration = Duration::from_secs(3);

/// How often the handle is asked for.
///
/// One screen frame at 60 Hz. Short enough that nothing perceptible is added to
/// the 258 ms the handle costs anyway, long enough that the poll is not a spin.
pub(crate) const POLL: Duration = Duration::from_millis(16);

/// A sentence that stays true for the rest of the run, shared with the worker.
///
/// `None` while nothing is wrong, which is the ordinary case and costs a lock
/// per view. Not a channel: a channel delivers once, and this has to be readable
/// again every time the worker rebuilds the message band.
pub type Standing = Arc<Mutex<Option<String>>>;

/// A fresh, empty standing message.
#[must_use]
pub fn standing() -> Standing {
    Arc::new(Mutex::new(None))
}

/// Reads the standing sentence, if there is one.
///
/// A poisoned lock answers `None` rather than panicking. The alternative is a
/// crash in front of a tester over a message ABOUT something having gone wrong,
/// which trades a small failure for the largest one.
#[must_use]
pub fn standing_line(standing: &Standing) -> Option<String> {
    standing.lock().ok().and_then(|held| held.clone())
}

/// Starts the poll. Returns immediately. The work happens on the event loop.
///
/// Call it before `run()`: work handed to the loop beforehand is delivered once
/// the loop starts, measured in `tools/sonda-petla` (`slint.md` 1.9).
pub fn refuse_focus(palette: &Palette, kept: KeptFocus, standing: &Standing) {
    let weak = palette.as_weak();
    let standing = Arc::clone(standing);
    let started = std::time::Instant::now();
    // `Rc`, not `Arc`: everything below runs on the main thread, and a `slint::Timer`
    // is neither `Send` nor `Sync`. An `Arc` here would claim a sharing that the
    // type cannot honour, which clippy says in as many words.
    let timer = std::rc::Rc::new(slint::Timer::default());
    let itself = std::rc::Rc::clone(&timer);
    timer.start(slint::TimerMode::Repeated, POLL, move || {
        let Some(palette) = weak.upgrade() else {
            itself.stop();
            return;
        };
        let handle = window_handle(palette.window());
        if handle.is_none() && started.elapsed() < HANDLE_WAIT {
            return;
        }
        itself.stop();
        if let Err(error) = kept.refuse_for(handle) {
            say(
                &palette,
                &standing,
                &i18n::environment(Environment::FocusNotRefused, &error.to_string()),
            );
        }
    });
    // Dropping a `slint::Timer` cancels it, so it is kept alive here for the
    // same reason and in the same way as the resting timer in `live`.
    KEEP.with_borrow_mut(|slot| *slot = Some(timer));
}

thread_local! {
    static KEEP: std::cell::RefCell<Option<std::rc::Rc<slint::Timer>>> =
        const { std::cell::RefCell::new(None) };
}

/// A window's own handle, if the window manager has made one - the palette's
/// here, the pack window's in `packs`.
///
/// A number rather than a type: it is what the adapter takes, and it is all this
/// layer knows. Anything that is not a Win32 handle answers `None`, which on
/// macOS and Linux is the honest answer today - there is no route there either
/// way, and the adapter says so in words.
pub(crate) fn window_handle(window: &slint::Window) -> Option<u64> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let window = window.window_handle();
    match window.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(handle) => Some(handle.hwnd.get() as u64),
        _ => None,
    }
}

/// Puts `line` in front of the tester now, and keeps it for later views.
///
/// Both halves are needed and neither is enough. The worker rebuilds the message
/// band from scratch on every view, so writing only to the window would lose the
/// sentence at the first shortcut. And the worker may be waiting on a press that
/// never comes, so writing only to [`Standing`] would leave the tester with a
/// palette that looks fine.
fn say(palette: &Palette, standing: &Standing, line: &str) {
    if let Ok(mut held) = standing.lock() {
        *held = Some(line.to_owned());
    }
    let mut lines = vec![SharedString::from(line)];
    lines.extend(palette.get_messages().iter());
    palette.set_messages(ModelRc::new(VecModel::from(lines)));
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
    fn a_standing_line_starts_empty_and_holds_what_it_was_given() {
        let standing = standing();
        assert_eq!(standing_line(&standing), None);
        *standing.lock().expect("a fresh lock is not poisoned") = Some(String::from("x"));
        assert_eq!(standing_line(&standing), Some(String::from("x")));
    }

    #[test]
    fn a_poisoned_lock_answers_nothing_rather_than_bringing_the_palette_down() {
        // The sentence this carries is ABOUT something having gone wrong, so a
        // panic here would replace a small failure with the largest one.
        let standing = standing();
        // 🔴 The value goes in BEFORE the poisoning, and that is what makes this
        // test able to fail. With an empty slot, an implementation that reached
        // past the poison with `into_inner` would answer `None` as well, and the
        // two would be indistinguishable.
        *standing.lock().expect("a fresh lock is not poisoned") = Some(String::from("kept"));
        let held = Arc::clone(&standing);
        let _ = std::thread::spawn(move || {
            let _guard = held.lock().expect("first lock");
            panic!("poisoning on purpose");
        })
        .join();
        assert!(standing.lock().is_err(), "the lock must really be poisoned");
        assert_eq!(standing_line(&standing), None);
    }

    /// The wait is longer than every measurement, and by a margin that is
    /// stated rather than felt.
    #[test]
    fn the_handle_wait_leaves_room_over_the_slowest_measurement() {
        // 489 ms is the slowest handle seen by `tools/sonda-ognisko` on this
        // machine. A wait that crept below it would start blaming the
        // environment for our own impatience.
        assert!(HANDLE_WAIT >= Duration::from_millis(489) * 6);
        assert!(POLL < Duration::from_millis(100), "a poll, not a nap");
    }
}
