//! Keeping the keyboard focus where the tester put it.
//!
//! # Why this thin file exists rather than a direct call
//!
//! The same reason as `startup.rs`, one layer over. `nkb-gui` depends on `core`,
//! `app` and this package, never on `nkb-sys`: `architektura.md` 2.3 makes the
//! graphical package an ADAPTER, and an adapter that calls the operating system
//! directly has stopped being one. Adding `nkb-sys` to it would also mean
//! widening the explicit allow-list in `tools/sprawdz-warstwy.ps1`, which buys
//! nothing and opens the door for every later call.
//!
//! The other half of the reason is specific to this seam. Measured 2026-09-22
//! with `tools/sonda-ognisko`, ten runs per variant: `WS_EX_NOACTIVATE` on its
//! own leaves the palette holding the foreground nine times out of ten, and
//! handing the foreground back on its own leaves a click on the palette stealing
//! the focus five times out of five. Each half looks like a fix and neither is
//! one. So they are not exported separately from here: [`KeptFocus`] is the only
//! way to reach them, and it does both or reports why not.
//!
//! # Why remembering is a separate step, and a type rather than an argument
//!
//! The window whose focus is being protected has to be read BEFORE the palette
//! opens, and the flag can only go on AFTER the window manager has created the
//! window - measured at 258 to 489 ms later. Those two moments are far apart in
//! the caller's code, and an argument passed between them is an argument that
//! can be forgotten or invented. A value that can only be made by
//! [`KeptFocus::remember`] cannot be either.

use nkb_sys::WindowRef;
use nkb_sys::window::{
    FocusError, KeyMenuError, can_refuse_focus, hand_back_foreground, refuse_activation,
    take_foreground,
};

/// Which window held the keyboard focus before the palette opened.
///
/// Make one with [`Self::remember`] before showing the window, then call
/// [`Self::refuse_for`] once the window handle exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeptFocus {
    /// `None` when nothing held the foreground - a fresh session, a desktop with
    /// no window up. There is then nobody to hand it back to, which is a fact
    /// about the desktop rather than a failure of ours.
    previous: Option<WindowRef>,
}

impl KeptFocus {
    /// Reads who holds the keyboard focus right now.
    ///
    /// Call this BEFORE the palette's window exists. Afterwards the answer is
    /// the palette itself, and handing the focus back to ourselves is a no-op
    /// that looks like a success.
    #[must_use]
    pub fn remember() -> Self {
        Self {
            previous: nkb_sys::foreground_window(),
        }
    }

    /// Whether this build can refuse the focus at all.
    ///
    /// Lets a caller say so once instead of repeating a `cfg`, and lets a test
    /// ask the question without knowing which system it is on.
    #[must_use]
    pub const fn possible() -> bool {
        can_refuse_focus()
    }

    /// Makes `window` refuse activation and hands the focus back.
    ///
    /// `window` is the raw handle of the palette's own window, as the graphical
    /// library reports it, and `None` when the library has none to give yet. A
    /// plain number rather than a type of ours on purpose: it is exactly what
    /// the caller knows, and widening it into something richer here would invent
    /// knowledge this layer does not have.
    ///
    /// The `Option` is in the signature rather than in the caller because the
    /// missing handle has a SENTENCE, and a caller left to invent one would put
    /// a literal where untouchable rule 9 forbids it.
    ///
    /// Both halves run, in this order, and the FIRST failure is returned. The
    /// order matters: the flag going on before the hand-back means that the
    /// moment the focus moves the window is already unable to take it again.
    ///
    /// # Errors
    ///
    /// Whatever `nkb-sys` reports: the style not taking, the palette still
    /// holding the foreground, or no route on this system.
    pub fn refuse_for(self, window: Option<u64>) -> Result<(), FocusError> {
        let window = WindowRef(window.ok_or(FocusError::HandleUnavailable)?);
        refuse_activation(window)?;
        hand_back_foreground(window, self.previous)
    }
}

/// The keyboard focus lent to the one window of ours that must take it - the
/// pack window - and the window it goes back to.
///
/// The palette's [`KeptFocus`] turned inside out: that window never takes the
/// focus, this one takes it on opening and gives it back on closing
/// (`ux-spec.md` 5.2). A separate type rather than a mode of the other, so a
/// caller cannot take the focus for the palette by picking the wrong flag.
///
/// Remembered BEFORE the window shows, for the reason [`KeptFocus::remember`]
/// gives: afterwards the answer is our own window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LentFocus {
    /// `None` when nothing held the foreground - then there is nobody to give
    /// it back to, and giving back says so.
    previous: Option<WindowRef>,
}

impl LentFocus {
    /// Reads who holds the keyboard focus right now.
    #[must_use]
    pub fn remember() -> Self {
        Self {
            previous: nkb_sys::foreground_window(),
        }
    }

    /// Brings `window` - the raw handle of our own window, `None` while the
    /// library has none to give - to the foreground.
    ///
    /// # Errors
    ///
    /// The handle missing, the system keeping another window in front, or no
    /// route on this system.
    pub fn take_for(&self, window: Option<u64>) -> Result<(), FocusError> {
        take_foreground(WindowRef(window.ok_or(FocusError::HandleUnavailable)?))
    }

    /// Gives the foreground back to the window remembered, if `window` holds
    /// it. A window that lost it already - the tester clicked elsewhere - has
    /// nothing to give back, and that is a success.
    ///
    /// # Errors
    ///
    /// The handle missing, our window still in front after the attempt, or no
    /// route on this system.
    pub fn give_back(self, window: Option<u64>) -> Result<(), FocusError> {
        let window = WindowRef(window.ok_or(FocusError::HandleUnavailable)?);
        hand_back_foreground(window, self.previous)
    }
}

/// Keeps the menu of `window` - the raw handle of our own window, `None`
/// while the library has none to give - from opening from the keyboard, so
/// `Alt+Space` recorded in the shortcuts window takes nothing after it
/// (K5.4, `slint.md` 2.37). Called on every opening, because a window shown
/// again is a new system window.
///
/// # Errors
///
/// The handle missing, the system refusing, or no route on this system.
pub fn no_keyboard_menu(window: Option<u64>) -> Result<(), KeyMenuError> {
    nkb_sys::window::no_keyboard_menu(WindowRef(window.ok_or(KeyMenuError::HandleUnavailable)?))
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
    fn a_lent_focus_with_no_handle_says_the_handle_is_missing_on_every_system() {
        let lent = LentFocus::remember();
        assert_eq!(lent.take_for(None), Err(FocusError::HandleUnavailable));
        assert_eq!(lent.give_back(None), Err(FocusError::HandleUnavailable));
    }

    #[test]
    fn a_keyboard_menu_with_no_handle_says_the_handle_is_missing_on_every_system() {
        assert_eq!(no_keyboard_menu(None), Err(KeyMenuError::HandleUnavailable));
    }

    #[test]
    fn a_lent_focus_never_reports_taking_a_window_that_is_not_there() {
        // The read-back is the verdict: a null handle is never in front.
        let outcome = LentFocus::remember().take_for(Some(0));
        if KeptFocus::possible() {
            assert_eq!(outcome, Err(FocusError::NotTaken));
        } else {
            assert!(matches!(outcome, Err(FocusError::Unsupported { .. })));
        }
    }

    #[test]
    fn no_handle_at_all_is_reported_as_the_missing_handle_on_every_system() {
        // Checked before the platform question, because this branch is ours
        // rather than the system's: it must answer the same way everywhere, or
        // a tester on Linux would be told the wrong thing about the same event.
        assert_eq!(
            KeptFocus::remember().refuse_for(None),
            Err(FocusError::HandleUnavailable)
        );
    }

    #[test]
    fn remembering_twice_in_a_row_answers_the_same_thing() {
        // Nothing here is supposed to CHANGE the foreground, and a session that
        // discovers otherwise has found a real problem rather than a flaky test.
        assert_eq!(KeptFocus::remember(), KeptFocus::remember());
    }

    #[test]
    fn a_handle_that_is_no_window_fails_rather_than_reporting_success() {
        // Zero is never a window. The point is not that it fails, it is that it
        // does not QUIETLY SUCCEED: the style write goes to a null handle, and a
        // version of this that trusted the write instead of reading it back
        // would report that the palette now refuses the focus when it does not.
        let kept = KeptFocus::remember();
        let outcome = kept.refuse_for(Some(0));
        if KeptFocus::possible() {
            assert_eq!(
                outcome,
                Err(FocusError::StyleRefused),
                "a style that did not take must be reported as not taken"
            );
        } else {
            assert!(matches!(outcome, Err(FocusError::Unsupported { .. })));
        }
    }

    #[test]
    fn the_answer_to_possible_matches_what_the_calls_actually_do() {
        // One fact in one place, checked against the other place. A build where
        // these disagree would have a caller printing "this system cannot" while
        // the call works, or the other way round.
        let says = KeptFocus::possible();
        let does = !matches!(
            KeptFocus::remember().refuse_for(Some(0)),
            Err(FocusError::Unsupported { .. })
        );
        assert_eq!(says, does);
    }
}
