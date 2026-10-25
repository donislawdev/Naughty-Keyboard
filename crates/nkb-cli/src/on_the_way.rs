//! What `nkb send` says while it is under way: one line when a send runs long (`OBS-160`) and
//! one when a clearing waits for the person to allow the keys (`D145` point 76). Out of
//! `main.rs` under the ceiling of `D113`.

use core::time::Duration;

use nkb_app::{OnTheWay, Progress, Waiting};

/// How long a send runs before `nkb send` says, once, that it is still typing.
///
/// A second, and not the palette's tenth: the palette redraws a band in place,
/// while a line printed here stays in the terminal and in a CI log for good, so
/// it is kept for the sends a person would wonder about.
const STILL_TYPING_AFTER: Duration = Duration::from_secs(1);

/// What `nkb send --clear` says while it waits for the person. On a Wayland desktop the first
/// clearing of a field is allowed in a window of the system, nothing is pressed or typed until
/// it is answered, and a command that stands for minutes in silence reads as one that hangs.
const WAITING_FOR_CONSENT: &str =
    "nkb send: waiting for the desktop to allow pressing keys - answer in the window it shows.";

/// The line for one thing a send says on its way, or nothing. `elapsed` is how long the send
/// has run, and `said` whether the line of a long send has been said: it is said once. The
/// line of a wait for the person is not that line, and does not use it up.
pub(crate) fn line_for(way: OnTheWay<'_>, elapsed: Duration, said: &mut bool) -> Option<String> {
    match way {
        OnTheWay::Typing(sending) => still_typing(elapsed, said, sending.progress),
        OnTheWay::Waiting(waiting) => waiting_line(waiting).map(str::to_owned),
    }
}

/// The one line a long send says on its way, or nothing - before the first
/// second, and after the line was said once.
fn still_typing(elapsed: Duration, said: &mut bool, progress: Progress) -> Option<String> {
    if *said || elapsed < STILL_TYPING_AFTER {
        return None;
    }
    *said = true;
    Some(format!(
        "nkb send: still typing - {} of {} UTF-16 units so far. Press Escape to stop.",
        progress.units_arrived, progress.units_total
    ))
}

/// The line a wait for the person says, or nothing. The wait is said when the question has stood
/// for a second (the route decides that), so a desktop that remembers the answer never causes
/// it. That the question is about to be put is for a window that has to get out of its way and
/// the command has none, and the end of the wait is not said: what follows says what came of it.
const fn waiting_line(waiting: Waiting) -> Option<&'static str> {
    match waiting {
        Waiting::ForConsent => Some(WAITING_FOR_CONSENT),
        Waiting::Asking | Waiting::Answered => None,
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
    fn a_long_send_says_once_that_it_is_still_typing_and_how_to_stop_it() {
        // `OBS-160`: nothing before a second, one line after it, never two - a
        // line printed here stays in a CI log for good.
        let step = Progress {
            units_arrived: 1784,
            units_total: 65535,
        };
        let mut said = false;
        assert_eq!(
            still_typing(Duration::from_millis(900), &mut said, step),
            None
        );
        let Some(line) = still_typing(STILL_TYPING_AFTER, &mut said, step) else {
            panic!("a send past a second says so");
        };
        assert!(
            line.contains("1784 of 65535") && line.contains("Escape"),
            "the line says how far and how to stop: {line}"
        );
        assert_eq!(
            still_typing(Duration::from_secs(9), &mut said, step),
            None,
            "said once, not once a report"
        );
    }

    #[test]
    fn the_wait_for_the_person_is_said_once_and_its_end_is_not_said() {
        assert_eq!(waiting_line(Waiting::ForConsent), Some(WAITING_FOR_CONSENT));
        assert_eq!(waiting_line(Waiting::Asking), None);
        assert_eq!(waiting_line(Waiting::Answered), None);
        assert!(
            WAITING_FOR_CONSENT.starts_with("nkb send: "),
            "every line of the command says whose it is"
        );
    }

    #[test]
    fn the_line_of_a_wait_does_not_use_up_the_line_of_a_long_send() {
        let mut said = false;
        let line = line_for(
            OnTheWay::Waiting(Waiting::ForConsent),
            Duration::from_secs(60),
            &mut said,
        );
        assert_eq!(line.as_deref(), Some(WAITING_FOR_CONSENT));
        assert!(!said, "the long-send line is still to be said");
        assert_eq!(
            line_for(
                OnTheWay::Waiting(Waiting::Answered),
                Duration::from_secs(60),
                &mut said
            ),
            None
        );
    }
}
