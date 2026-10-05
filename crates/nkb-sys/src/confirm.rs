//! Pressing one event at a time, and the next only once the system shows the
//! last one (`OBS-156`, `D94`).
//!
//! # Why the count `SendInput` returns is not enough
//!
//! `SendInput` says how many events it ACCEPTED, not how many reached anything.
//! While another thread holds `BlockInput`, the system accepts the events of
//! every other thread and then drops them - and `SendKeys` in .NET holds it for
//! as long as it sends (`dotnet/winforms`, `SendKeys.cs`). Measured 2026-10-05
//! with an own window and a trace of what its thread took from the queue: the
//! palette's `Home`, `Shift+End`, `Delete` were accepted, seen by a low-level
//! keyboard hook, and never reached the window, while the palette said
//! "cleared first" (`OBS-153`).
//!
//! What does follow arrival is the asynchronous key state: an accepted event
//! changes it, a dropped one does not. Measured the same day, 10 of 10 for
//! `Home` and `Shift` in both directions and 5 of 5 for a character
//! (`VK_PACKET`), median 0.03 ms and at most 0.25 ms after the event - and
//! under another thread's `BlockInput`, no change at all, 5 of 5, with the
//! window receiving nothing. So each event goes alone, and the next goes only
//! once its key shows the state the event leaves behind.
//!
//! The logic is pure - the system is two closures - so it is tested on every
//! system, while only Windows feeds it.

use core::time::Duration;

/// One event, and the key state the system shows once it has taken it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Step<E> {
    pub event: E,
    /// The virtual key whose state the event changes.
    pub key: u16,
    /// Whether that key is down once the event is taken.
    pub down: bool,
}

/// How long the system is given to show one event before the send stops.
///
/// Not tuned to the measurement: the answer comes in a fraction of a
/// millisecond (at most 0.25 ms measured), and this is four orders of magnitude
/// above that and still short of a person's patience. An event not shown by then
/// was dropped - the system does not take an event late, it drops it.
pub(crate) const TAKEN_WAIT: Duration = Duration::from_secs(1);

/// How long the wait spins before it starts sleeping between checks. An answer
/// is expected within it. Past it the input is most likely blocked, and spinning
/// a core for the rest of [`TAKEN_WAIT`] would only heat the machine.
const SPIN: Duration = Duration::from_millis(1);

/// Presses `steps` in order, each only after the previous one is shown.
///
/// `send` hands one event to the system and says whether it accepted it.
/// `is_down` reads whether a key is down right now. Returns how many steps were
/// both accepted and shown, and stops at the first that was not: nothing goes
/// out after an event the system did not take.
pub(crate) fn press_confirmed<E: Copy>(
    steps: &[Step<E>],
    mut send: impl FnMut(E) -> bool,
    mut is_down: impl FnMut(u16) -> bool,
    wait: Duration,
) -> usize {
    for (index, step) in steps.iter().enumerate() {
        if !send(step.event) || !shown(step.key, step.down, &mut is_down, wait) {
            return index;
        }
    }
    steps.len()
}

fn shown(key: u16, down: bool, is_down: &mut impl FnMut(u16) -> bool, wait: Duration) -> bool {
    let start = std::time::Instant::now();
    loop {
        if is_down(key) == down {
            return true;
        }
        let elapsed = start.elapsed();
        if elapsed >= wait {
            return false;
        }
        if elapsed < SPIN {
            std::thread::yield_now();
        } else {
            std::thread::sleep(SPIN);
        }
    }
}

/// The steps of one chord: `Shift` down if asked, the key down and up, `Shift`
/// up if it went down - each with the state it leaves. `event` builds the
/// system's event from a key and whether it is a release.
pub(crate) fn chord_steps<E>(
    key: u16,
    shift: Option<u16>,
    event: impl Fn(u16, bool) -> E,
) -> Vec<Step<E>> {
    let mut steps = Vec::with_capacity(4);
    let mut step = |key: u16, down: bool| {
        steps.push(Step {
            event: event(key, !down),
            key,
            down,
        });
    };
    if let Some(shift) = shift {
        step(shift, true);
    }
    step(key, true);
    step(key, false);
    if let Some(shift) = shift {
        step(shift, false);
    }
    steps
}

/// How many UTF-16 units of a text arrived, when the unit after the first
/// `before` stopped with `taken` of its two events shown. A character goes in
/// on its key-down, so a unit whose down was shown arrived even if its up was
/// not - and saying fewer would hide a character that is in the field.
pub(crate) const fn units_arrived(before: usize, taken: usize) -> usize {
    before + if taken >= 1 { 1 } else { 0 }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;

    const SHIFT: u16 = 0x10;
    const HOME: u16 = 0x24;

    /// A fake system: a key state per key, events that change it unless the
    /// system is told to drop them from a given event on.
    struct System {
        down: [bool; 256],
        sent: Vec<(u16, bool)>,
        drop_from: Option<usize>,
        refuse_from: Option<usize>,
    }

    impl System {
        fn new() -> Self {
            Self {
                down: [false; 256],
                sent: Vec::new(),
                drop_from: None,
                refuse_from: None,
            }
        }

        /// `(key, release)` as `chord_steps` builds it.
        fn send(&mut self, (key, release): (u16, bool)) -> bool {
            let index = self.sent.len();
            self.sent.push((key, release));
            if self.refuse_from.is_some_and(|from| index >= from) {
                return false;
            }
            if self.drop_from.is_none_or(|from| index < from) {
                self.down[usize::from(key)] = !release;
            }
            true
        }
    }

    fn press(system: &mut System, steps: &[Step<(u16, bool)>]) -> usize {
        let system = core::cell::RefCell::new(system);
        press_confirmed(
            steps,
            |event| system.borrow_mut().send(event),
            |key| system.borrow().down[usize::from(key)],
            Duration::ZERO,
        )
    }

    #[test]
    fn a_shifted_chord_is_four_steps_each_with_the_state_it_leaves() {
        // A wrong expected state is a send that stops on a key the system DID
        // take, or one that waits for the wrong direction - so the table is
        // checked whole.
        let steps = chord_steps(HOME, Some(SHIFT), |key, release| (key, release));
        let shape: Vec<(u16, bool, bool)> = steps
            .iter()
            .map(|step| (step.key, step.down, step.event.1))
            .collect();
        assert_eq!(
            shape,
            vec![
                (SHIFT, true, false),
                (HOME, true, false),
                (HOME, false, true),
                (SHIFT, false, true),
            ]
        );
        let plain = chord_steps(HOME, None, |key, release| (key, release));
        assert_eq!(plain.len(), 2, "no Shift, no Shift steps");
    }

    #[test]
    fn every_step_the_system_shows_goes_out_in_order() {
        let steps = chord_steps(HOME, Some(SHIFT), |key, release| (key, release));
        let mut system = System::new();
        assert_eq!(press(&mut system, &steps), 4);
        assert_eq!(
            system.sent,
            vec![(SHIFT, false), (HOME, false), (HOME, true), (SHIFT, true)]
        );
        assert!(!system.down[usize::from(SHIFT)], "nothing is left down");
    }

    #[test]
    fn nothing_goes_out_after_a_step_the_system_dropped() {
        // `OBS-153`: the system accepts the event and drops it. The count must
        // stop there AND no further event may follow - a `Delete` after a
        // dropped `Shift+End` deletes one character, not the selection.
        let steps = chord_steps(HOME, Some(SHIFT), |key, release| (key, release));
        let mut system = System::new();
        system.drop_from = Some(1);
        assert_eq!(
            press(&mut system, &steps),
            1,
            "Shift was shown, Home was not"
        );
        assert_eq!(system.sent.len(), 2, "nothing after the dropped Home");
    }

    #[test]
    fn a_step_the_system_refuses_stops_without_waiting_for_it() {
        let steps = chord_steps(HOME, None, |key, release| (key, release));
        let mut system = System::new();
        system.refuse_from = Some(0);
        assert_eq!(press(&mut system, &steps), 0);
        assert_eq!(system.sent.len(), 1);
    }

    #[test]
    fn a_key_already_in_the_expected_state_counts_as_shown() {
        // Named rather than hidden: a key held physically shows "down" whether
        // or not our event arrived. A dropped event slips through only in that
        // case, and only for the key the person is holding.
        let steps = chord_steps(HOME, None, |key, release| (key, release));
        let mut system = System::new();
        system.down[usize::from(HOME)] = true;
        system.drop_from = Some(0);
        assert_eq!(
            press(&mut system, &steps),
            1,
            "the held down passes, the up does not"
        );
    }

    #[test]
    fn the_wait_ends_on_its_bound_rather_than_never() {
        let start = std::time::Instant::now();
        let steps = [Step {
            event: (),
            key: HOME,
            down: true,
        }];
        let taken = press_confirmed(&steps, |()| true, |_| false, Duration::from_millis(20));
        assert_eq!(taken, 0);
        let elapsed = start.elapsed();
        assert!(elapsed >= Duration::from_millis(20), "waited {elapsed:?}");
        assert!(elapsed < Duration::from_secs(5), "waited {elapsed:?}");
    }

    #[test]
    fn a_character_whose_key_down_was_shown_arrived() {
        assert_eq!(
            units_arrived(7, 0),
            7,
            "down not shown: the character is not in"
        );
        assert_eq!(
            units_arrived(7, 1),
            8,
            "down shown, up not: the character is in"
        );
        assert_eq!(units_arrived(7, 2), 8);
    }

    #[test]
    fn the_bound_is_far_above_the_measured_answer_and_below_patience() {
        // 0.25 ms was the slowest answer measured (2026-10-05). The bound is a
        // safety margin, not a tuned number - this keeps it from drifting into
        // one in either direction.
        assert!(TAKEN_WAIT >= Duration::from_millis(250));
        assert!(TAKEN_WAIT <= Duration::from_secs(2));
    }
}
