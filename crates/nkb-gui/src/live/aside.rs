//! The palette steps aside from the window the desktop shows to ask whether keys may be
//! pressed, and steps back when it is answered (`D145` point 77, `OBS-232`).
//!
//! # Why the palette leaves the top, and why before the question
//!
//! On a Wayland desktop the first clearing of a field puts a window of the system in front of
//! the person, and the palette, which stands above every window, covered the half of it with
//! the switch the person has to turn on. Measured on GNOME 50 (`nkb-sys`,
//! `linux_window_rules.rs`): a palette that leaves the top AFTER that window has opened is put
//! on top of the layer it goes to, which is above the window all the same, and one that has
//! left BEFORE it opens is below it. So the question to the person is preceded by a word, and
//! this thread waits for the main thread to have done what it asks, for a moment at most.
//!
//! The window is the main thread's, so the stepping aside and back is done there. What the
//! window stood as is kept there too, from the first until the second, so a late closure can
//! never lose what the other one needs: the loop runs them in the order they were handed over.

use std::cell::{Cell, RefCell};
use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

use nkb_adapters::i18n::{self, Environment};
use nkb_adapters::{Aside, StackingError, step_aside, step_back};
use nkb_app::ports::Waiting;
use slint::{ComponentHandle, Weak};

use super::{apply_waiting, waiting_messages};
use crate::Palette;
use crate::focus::{Standing, say, window_handle};

/// How long this thread waits for the palette to be off the top before the question is put.
///
/// More than the window manager is given to show the change (`nkb-sys` waits up to 400 ms for
/// it), so that a manager that does not follow is the one that answers late and not this
/// wait that gives up first. Where the main thread is busy the question goes on after it:
/// the person is asked, and the palette may stand over the window, which the sentence says.
const ASIDE_WAIT: Duration = Duration::from_millis(800);

thread_local! {
    /// What the palette stood as, from the moment it stepped aside until it steps back.
    static ASIDE: RefCell<Option<Aside>> = const { RefCell::new(None) };
}

/// What the palette does for the question of the desktop: steps aside before it, says what it
/// waits for while it stands, and steps back when it is answered.
pub(super) struct Question<'a> {
    palette: &'a Weak<Palette>,
    standing: &'a Standing,
    /// Whether the palette could not step aside for the question that stands now, so that it
    /// may cover the window of the desktop and the sentence says where it goes.
    covers: Cell<bool>,
}

impl<'a> Question<'a> {
    pub(super) fn new(palette: &'a Weak<Palette>, standing: &'a Standing) -> Self {
        Self {
            palette,
            standing,
            covers: Cell::new(false),
        }
    }

    /// Does what one word of the route asks for. Called on the worker thread, between two
    /// things a send does, so it only hands finished work to the main thread.
    pub(super) fn on(&self, waiting: Waiting) {
        match waiting {
            Waiting::Asking => self.covers.set(self.step_aside()),
            Waiting::ForConsent => {
                self.hand_over(
                    waiting_messages(waiting, self.covers.get(), self.standing),
                    false,
                );
            }
            Waiting::Answered => {
                self.covers.set(false);
                self.hand_over(waiting_messages(waiting, false, self.standing), true);
            }
        }
    }

    /// Asks the main thread to take the palette off the top, and waits for it to say how it
    /// went. `true` when the palette may still stand over the window the desktop will show.
    fn step_aside(&self) -> bool {
        let (done, answered) = mpsc::channel();
        let handed = self.palette.upgrade_in_event_loop(move |palette| {
            let _ = done.send(leave(&palette));
        });
        // A window that is gone, or a loop that is not running, covers nothing.
        handed.is_ok() && answered.recv_timeout(ASIDE_WAIT).unwrap_or(true)
    }

    /// Puts the message band as `messages` say, and then, when `back` is set, the palette on
    /// top again. In that order: the sentence of the question comes down first.
    fn hand_over(&self, messages: Vec<String>, back: bool) {
        let standing = Arc::clone(self.standing);
        let _ = self.palette.upgrade_in_event_loop(move |palette| {
            apply_waiting(&palette, messages);
            if back {
                come_back(&palette, &standing);
            }
        });
    }
}

/// Whether a palette that was asked to step aside may still cover the window of the desktop:
/// the window manager kept it on top. A palette with no handle is no window of the system's
/// to stand over anything, so it covers nothing.
fn covers_after(outcome: Option<&StackingError>) -> bool {
    matches!(outcome, Some(StackingError::NotAside))
}

/// The sentence for a palette that could not be put back on top, or nothing when it could.
fn said_after(outcome: Result<(), StackingError>) -> Option<String> {
    outcome
        .err()
        .map(|error| i18n::environment(Environment::NotBackOnTop, &error.to_string()))
}

/// Runs on the MAIN thread: takes the palette off the top and keeps what it stood as.
fn leave(palette: &Palette) -> bool {
    let outcome = step_aside(window_handle(palette.window()));
    let covers = covers_after(outcome.as_ref().err());
    if let Ok(aside) = outcome {
        ASIDE.with_borrow_mut(|kept| *kept = Some(aside));
    }
    covers
}

/// Runs on the MAIN thread: puts the palette back on top, if it stepped aside, and says when
/// it could not be put back, because then it is behind the application under test.
fn come_back(palette: &Palette, standing: &Standing) {
    let Some(aside) = ASIDE.with_borrow_mut(Option::take) else {
        return;
    };
    if let Some(line) = said_after(step_back(window_handle(palette.window()), &aside)) {
        say(palette, standing, &line);
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
    use crate::focus::standing_line;
    use crate::live::tests::{a_palette, quiet};
    use slint::Model;

    #[test]
    fn a_palette_the_manager_kept_on_top_may_cover_the_window_of_the_desktop() {
        assert!(covers_after(Some(&StackingError::NotAside)));
    }

    #[test]
    fn a_palette_that_stepped_aside_or_has_no_window_of_the_systems_covers_nothing() {
        assert!(!covers_after(None));
        assert!(!covers_after(Some(&StackingError::HandleUnavailable)));
        assert!(!covers_after(Some(&StackingError::NotBack)));
    }

    #[test]
    fn a_palette_that_came_back_says_nothing() {
        assert_eq!(said_after(Ok(())), None);
    }

    #[test]
    fn a_palette_that_could_not_come_back_says_why_and_how_to_bring_it_back() {
        for error in [StackingError::NotBack, StackingError::HandleUnavailable] {
            let line = said_after(Err(error.clone())).expect("a sentence");
            assert!(line.contains(&error.to_string()), "{line}");
            assert!(line.contains("Restart"), "{line}");
        }
    }

    #[test]
    fn coming_back_when_the_palette_never_stepped_aside_changes_nothing() {
        // A question that asked nothing of the palette, or a second word of its end, finds
        // nothing to put back and says nothing.
        let palette = a_palette();
        let standing = quiet();
        ASIDE.with_borrow_mut(|kept| *kept = None);
        come_back(&palette, &standing);
        assert_eq!(standing_line(&standing), None);
        assert_eq!(palette.get_messages().iter().count(), 0);
    }
}
