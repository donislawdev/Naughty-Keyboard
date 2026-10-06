//! Pressing one event at a time, and the next only once the last one is shown -
//! by the system (`OBS-156`, `D94`) and, when the send is paced, by the queue of
//! the application that holds the keyboard focus (`OBS-158`, `D95`).
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
//! What does follow the system taking an event is the asynchronous key state:
//! an accepted event changes it, a dropped one does not. Measured the same day,
//! 10 of 10 for `Home` and `Shift` in both directions and 5 of 5 for a character
//! (`VK_PACKET`), median 0.03 ms and at most 0.25 ms after the event - and under
//! another thread's `BlockInput`, no change at all, 5 of 5, with the window
//! receiving nothing. A keyboard hook that swallows the event and a window with
//! higher privileges give the same picture (measured 2026-10-06).
//!
//! # Why that is not enough either
//!
//! The system taking an event is not the application taking it. When an
//! application falls behind, the system COMBINES its pending key-down messages
//! into one and raises the repeat count (Microsoft Learn, "Keyboard Input
//! Overview", Repeat Count) - and every character typed this way is the same key,
//! `VK_PACKET`, so a combined message carries one character and the rest are gone.
//! Measured 2026-10-06: 100 000 characters to a slow window, 87 % arrived at exit
//! code 0, and 255 to a WinForms field, once 252. The application's own queue shows
//! what it took in its SYNCHRONOUS key state, which changes as its thread reads
//! keyboard messages (`GetKeyState`) and which a thread joined to that queue can
//! read. Waiting for it before the next event leaves nothing pending to combine:
//! 20 000 of 20 000 where the unpaced send got 6 832 (`tools/sondy/tempo-celu.ps1`).
//!
//! The logic is pure - the system is a [`Wire`] - so it is tested on every
//! system, while only Windows feeds it.

use core::time::Duration;

use crate::StopReason;

/// One event, and the key state the system shows once it has taken it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Step<E> {
    pub event: E,
    /// The virtual key whose state the event changes.
    pub key: u16,
    /// Whether that key is down once the event is taken.
    pub down: bool,
}

/// What the window in front looks like between two events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Watch {
    /// The same window, responding.
    Steady,
    /// Another window is in front: the next key would go there.
    Moved,
    /// The system considers the window not responding (`IsHungAppWindow`).
    Hung,
    /// The person pressed `Escape` to stop the send (race `W3`, `D96`).
    Escape,
}

/// The system, as the five questions a send asks it. A trait rather than four
/// closures so a test writes one fake system and every question sees its state.
pub(crate) trait Wire<E> {
    /// Hands one event to the system. True when it accepted it - which is not
    /// yet "it arrived".
    fn send(&mut self, event: E) -> bool;
    /// Whether the system shows `key` down right now (asynchronous state).
    fn system_down(&mut self, key: u16) -> bool;
    /// Whether the focused application's queue shows `key` down right now -
    /// its synchronous state, which moves as its thread takes key messages.
    /// Asked only when the send is paced.
    fn target_down(&mut self, key: u16) -> bool;
    /// What the window in front looks like, and whether `Escape` was pressed.
    fn watch(&mut self) -> Watch;
    /// Lets time pass between two looks. A test does nothing here.
    fn pause(&mut self, how_long: Duration);
}

/// How long each level is given to show one event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Waits {
    /// For the system to show the event taken.
    pub system: Duration,
    /// For the application to take it, or `None` when the send is not paced
    /// (`D95` point 2: no queue to follow, said by the caller, never silent).
    pub target: Option<Duration>,
}

/// How long the system is given to show one event before the send stops.
///
/// Not tuned to the measurement: the answer comes in a fraction of a
/// millisecond (at most 0.25 ms measured), and this is four orders of magnitude
/// above that and still short of a person's patience. An event not shown by then
/// was dropped - the system does not take an event late, it drops it.
pub(crate) const TAKEN_WAIT: Duration = Duration::from_secs(1);

/// How long the application is given to take one event before the send stops.
///
/// Not a measured number either: it is the system's own definition of a window
/// that is not responding - one whose thread has not taken a message for five
/// seconds (`IsHungAppWindow`). An application that takes no key for as long as
/// the system would call it hung has stopped taking keys, whatever it is doing.
/// The slowest answer measured was 418 ms (`tools/sondy/tempo-celu.ps1`, Edge
/// under a page that rewrote its title on every key).
pub(crate) const TARGET_WAIT: Duration = Duration::from_secs(5);

/// How long a wait spins before it starts sleeping between looks. An answer
/// from the system is expected within it, and most answers from an application
/// too. Past it, spinning a core for the rest of the wait would only heat the
/// machine.
pub(crate) const SPIN: Duration = Duration::from_millis(1);

/// How far a send got.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Pressed {
    /// Steps shown at every level asked, in order. Nothing after the first
    /// step that was not.
    pub taken: usize,
    /// Why it stopped before the end, or `None` when every step was taken.
    pub stop: Option<StopReason>,
}

/// Presses `steps` in order, each only after the previous one is shown.
///
/// Before each event the window in front is looked at: another window means the
/// next key would land there, so nothing more goes out (`FocusMoved`, race `W2`
/// at the level of the window), and a window the system calls hung would only
/// pile keys up (`NotTaking`). `Escape` pressed stops it the same way, the token
/// of race `W3` asked in the same look rather than in a second loop (`D96`) -
/// but only BEFORE an event, never while one is on its way: see [`target_takes`].
/// After each event the system must show it within
/// [`Waits::system`] (`Dropped` otherwise), and, when paced, the application
/// must take it within [`Waits::target`], the window being watched meanwhile.
pub(crate) fn press_confirmed<E: Copy>(
    steps: &[Step<E>],
    wire: &mut impl Wire<E>,
    waits: Waits,
) -> Pressed {
    for (index, step) in steps.iter().enumerate() {
        let stopped = |reason| Pressed {
            taken: index,
            stop: Some(reason),
        };
        match wire.watch() {
            Watch::Steady => {}
            Watch::Moved => return stopped(StopReason::FocusMoved),
            Watch::Hung => return stopped(StopReason::NotTaking),
            Watch::Escape => return stopped(StopReason::Escape),
        }
        if !wire.send(step.event) {
            return stopped(StopReason::Dropped);
        }
        if !system_shows(wire, step.key, step.down, waits.system) {
            return stopped(StopReason::Dropped);
        }
        if let Some(wait) = waits.target
            && let Err(reason) = target_takes(wire, step.key, step.down, wait)
        {
            return stopped(reason);
        }
    }
    Pressed {
        taken: steps.len(),
        stop: None,
    }
}

/// Whether the system shows the event within `wait`. Spins first, then sleeps.
fn system_shows<E>(wire: &mut impl Wire<E>, key: u16, down: bool, wait: Duration) -> bool {
    let start = std::time::Instant::now();
    loop {
        if wire.system_down(key) == down {
            return true;
        }
        let elapsed = start.elapsed();
        if elapsed >= wait {
            return false;
        }
        if elapsed < SPIN {
            std::thread::yield_now();
        } else {
            wire.pause(SPIN);
        }
    }
}

/// Whether the application takes the event within `wait`, watching the window
/// in front while it does not.
///
/// 🔴 `Escape` does NOT end this wait, and that is measured. The system has
/// already taken the event, so it is on its way into the application whatever
/// happens here - a slow application takes it a little later. Ended here, the
/// send reported one unit fewer than the field received: 2024 against 2025
/// (2026-10-06, `tools/sondy/escape-w3.ps1` scene P3). So the key in flight
/// lands, and the press stops the send before the NEXT event, where the count
/// is exact. An application that does not take it at all is what the bound and
/// a hung window answer for.
fn target_takes<E>(
    wire: &mut impl Wire<E>,
    key: u16,
    down: bool,
    wait: Duration,
) -> Result<(), StopReason> {
    let start = std::time::Instant::now();
    loop {
        if wire.target_down(key) == down {
            return Ok(());
        }
        let elapsed = start.elapsed();
        if elapsed >= wait {
            return Err(StopReason::NotTaking);
        }
        if elapsed < SPIN {
            std::thread::yield_now();
        } else {
            // Watched only once the answer is late: two system calls per look
            // would cost more than the answer itself takes to come.
            match wire.watch() {
                // Asked again before the next event, which is where it stops.
                Watch::Steady | Watch::Escape => {}
                Watch::Moved => return Err(StopReason::FocusMoved),
                Watch::Hung => return Err(StopReason::NotTaking),
            }
            wire.pause(SPIN);
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

/// How many chords ACTED, when the chord after the first `before` stopped with
/// `taken` of its steps shown. A chord acts on its main key going down - the
/// key, never `Shift` alone - so it counts once that step was shown, exactly as
/// a character counts on its key-down (`D95` point 5). Zero means nothing was
/// pressed that could change the field.
pub(crate) const fn chords_acted(before: usize, taken: usize, shifted: bool) -> usize {
    let main_down = if shifted { 1 } else { 0 };
    before + if taken > main_down { 1 } else { 0 }
}

/// How often a send says how far it got (`OBS-160`).
///
/// A tenth of a second is about the shortest change a person reads as movement
/// rather than flicker, and it is the delay before the FIRST report too: a send
/// that ends sooner says nothing on the way, so the many short values do not
/// make a progress band blink into view and out again.
pub(crate) const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

/// When the next report of a send is due. Pure, so it is tested with instants
/// of its own rather than with a clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Cadence {
    next: std::time::Instant,
    every: Duration,
}

impl Cadence {
    /// The first report is due `every` after `start`.
    pub(crate) fn starting(start: std::time::Instant, every: Duration) -> Self {
        Self {
            next: start + every,
            every,
        }
    }

    /// Whether a report is due at `now`. When it is, the next one is due a full
    /// interval after `now` - a send that stalled does not owe a burst of
    /// reports for the time it stood still.
    pub(crate) fn due(&mut self, now: std::time::Instant) -> bool {
        if now < self.next {
            return false;
        }
        self.next = now + self.every;
        true
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
    fn nothing_is_reported_before_the_first_interval() {
        // A short send says nothing on the way, so a band never blinks.
        let start = std::time::Instant::now();
        let mut cadence = Cadence::starting(start, PROGRESS_INTERVAL);
        assert!(!cadence.due(start));
        assert!(!cadence.due(start + Duration::from_millis(99)));
        assert!(cadence.due(start + Duration::from_millis(100)));
    }

    #[test]
    fn a_report_is_due_once_per_interval_and_a_stall_owes_no_burst() {
        let start = std::time::Instant::now();
        let mut cadence = Cadence::starting(start, PROGRESS_INTERVAL);
        let at = |ms| start + Duration::from_millis(ms);
        assert!(cadence.due(at(100)));
        assert!(!cadence.due(at(150)), "half an interval later is not due");
        assert!(cadence.due(at(200)));
        // The send stood still for a second: one report, then a full interval.
        assert!(cadence.due(at(1200)));
        assert!(
            !cadence.due(at(1250)),
            "no burst for the time it stood still"
        );
        assert!(cadence.due(at(1300)));
    }

    const SHIFT: u16 = 0x10;
    const HOME: u16 = 0x24;

    /// A fake system: the system's key state, the application's key state, and
    /// switches that make each level fail from a given event on.
    struct System {
        down: [bool; 256],
        taken_by_app: [bool; 256],
        sent: Vec<(u16, bool)>,
        refuse_from: Option<usize>,
        drop_from: Option<usize>,
        /// The application stops taking events from this one on.
        app_stops_from: Option<usize>,
        /// What `watch` answers from the given look on (counted from zero).
        watch_from: Option<(usize, Watch)>,
        looks: usize,
    }

    impl System {
        fn new() -> Self {
            Self {
                down: [false; 256],
                taken_by_app: [false; 256],
                sent: Vec::new(),
                refuse_from: None,
                drop_from: None,
                app_stops_from: None,
                watch_from: None,
                looks: 0,
            }
        }
    }

    impl Wire<(u16, bool)> for System {
        /// `(key, release)` as `chord_steps` builds it.
        fn send(&mut self, (key, release): (u16, bool)) -> bool {
            let index = self.sent.len();
            self.sent.push((key, release));
            if self.refuse_from.is_some_and(|from| index >= from) {
                return false;
            }
            if self.drop_from.is_none_or(|from| index < from) {
                self.down[usize::from(key)] = !release;
                if self.app_stops_from.is_none_or(|from| index < from) {
                    self.taken_by_app[usize::from(key)] = !release;
                }
            }
            true
        }
        fn system_down(&mut self, key: u16) -> bool {
            self.down[usize::from(key)]
        }
        fn target_down(&mut self, key: u16) -> bool {
            self.taken_by_app[usize::from(key)]
        }
        fn watch(&mut self) -> Watch {
            let look = self.looks;
            self.looks += 1;
            match self.watch_from {
                Some((from, answer)) if look >= from => answer,
                _ => Watch::Steady,
            }
        }
        fn pause(&mut self, _how_long: Duration) {}
    }

    const PACED: Waits = Waits {
        system: Duration::ZERO,
        target: Some(Duration::ZERO),
    };
    const UNPACED: Waits = Waits {
        system: Duration::ZERO,
        target: None,
    };

    fn shifted_home() -> Vec<Step<(u16, bool)>> {
        chord_steps(HOME, Some(SHIFT), |key, release| (key, release))
    }

    #[test]
    fn a_shifted_chord_is_four_steps_each_with_the_state_it_leaves() {
        // A wrong expected state is a send that stops on a key the system DID
        // take, or one that waits for the wrong direction - so the table is
        // checked whole.
        let shape: Vec<(u16, bool, bool)> = shifted_home()
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
    fn every_step_both_levels_show_goes_out_in_order() {
        for waits in [PACED, UNPACED] {
            let mut system = System::new();
            assert_eq!(
                press_confirmed(&shifted_home(), &mut system, waits),
                Pressed {
                    taken: 4,
                    stop: None
                }
            );
            assert_eq!(
                system.sent,
                vec![(SHIFT, false), (HOME, false), (HOME, true), (SHIFT, true)]
            );
            assert!(!system.down[usize::from(SHIFT)], "nothing is left down");
        }
    }

    #[test]
    fn nothing_goes_out_after_a_step_the_system_dropped() {
        // `OBS-153`: the system accepts the event and drops it. The count must
        // stop there AND no further event may follow - a `Delete` after a
        // dropped `Shift+End` deletes one character, not the selection.
        let mut system = System::new();
        system.drop_from = Some(1);
        assert_eq!(
            press_confirmed(&shifted_home(), &mut system, UNPACED),
            Pressed {
                taken: 1,
                stop: Some(StopReason::Dropped)
            },
            "Shift was shown, Home was not"
        );
        assert_eq!(system.sent.len(), 2, "nothing after the dropped Home");
    }

    #[test]
    fn a_step_the_system_refuses_stops_without_waiting_for_it() {
        let steps = chord_steps(HOME, None, |key, release| (key, release));
        let mut system = System::new();
        system.refuse_from = Some(0);
        assert_eq!(
            press_confirmed(&steps, &mut system, PACED),
            Pressed {
                taken: 0,
                stop: Some(StopReason::Dropped)
            }
        );
        assert_eq!(system.sent.len(), 1);
    }

    #[test]
    fn a_paced_send_waits_for_the_application_and_stops_where_it_stopped_taking() {
        // `OBS-158`: the system took the event, the application did not. Unpaced
        // the send goes on - exactly the loss being fixed - and paced it stops
        // there, with nothing after it.
        let mut system = System::new();
        system.app_stops_from = Some(2);
        assert_eq!(
            press_confirmed(&shifted_home(), &mut system, PACED),
            Pressed {
                taken: 2,
                stop: Some(StopReason::NotTaking)
            }
        );
        assert_eq!(system.sent.len(), 3, "nothing after the event not taken");

        let mut unpaced = System::new();
        unpaced.app_stops_from = Some(2);
        assert_eq!(
            press_confirmed(&shifted_home(), &mut unpaced, UNPACED).taken,
            4,
            "unpaced, only the system is asked"
        );
    }

    #[test]
    fn another_window_in_front_stops_the_send_before_the_next_key() {
        // Race `W2` at the level of the window: the key would land elsewhere.
        // Looked at before every event, so the event is NOT sent.
        let mut system = System::new();
        system.watch_from = Some((2, Watch::Moved));
        assert_eq!(
            press_confirmed(&shifted_home(), &mut system, UNPACED),
            Pressed {
                taken: 2,
                stop: Some(StopReason::FocusMoved)
            }
        );
        assert_eq!(system.sent.len(), 2, "the third event never went out");
    }

    #[test]
    fn a_window_that_moves_while_the_application_is_late_is_named_as_moved() {
        let mut system = System::new();
        system.app_stops_from = Some(0);
        // Look 0 is before the first event, look 1 the first while waiting.
        system.watch_from = Some((1, Watch::Moved));
        let waits = Waits {
            system: Duration::ZERO,
            target: Some(Duration::from_secs(5)),
        };
        assert_eq!(
            press_confirmed(&shifted_home(), &mut system, waits),
            Pressed {
                taken: 0,
                stop: Some(StopReason::FocusMoved)
            }
        );
    }

    #[test]
    fn a_hung_window_stops_the_send_before_anything_piles_up_in_it() {
        let mut system = System::new();
        system.watch_from = Some((0, Watch::Hung));
        assert_eq!(
            press_confirmed(&shifted_home(), &mut system, PACED),
            Pressed {
                taken: 0,
                stop: Some(StopReason::NotTaking)
            }
        );
        assert!(system.sent.is_empty(), "nothing sent to a hung window");
    }

    #[test]
    fn escape_stops_the_send_before_the_next_key() {
        // Race `W3` (`D96`): the press is asked in the same look as the window,
        // so the event after it is NOT sent.
        let mut system = System::new();
        system.watch_from = Some((3, Watch::Escape));
        assert_eq!(
            press_confirmed(&shifted_home(), &mut system, PACED),
            Pressed {
                taken: 3,
                stop: Some(StopReason::Escape)
            }
        );
        assert_eq!(system.sent.len(), 3, "the fourth event never went out");
    }

    #[test]
    fn escape_pressed_while_the_application_is_late_lets_the_key_in_flight_land() {
        // The system has taken the event, so it reaches the application anyway.
        // Ending the wait at the press reported one unit fewer than the field
        // held (P3 of the probe, 2025 against 2024). So the key in flight lands,
        // counted, and the send stops before the next event.
        struct Late {
            down: bool,
            pauses: usize,
            sent: usize,
        }
        impl Wire<bool> for Late {
            fn send(&mut self, down: bool) -> bool {
                self.down = down;
                self.pauses = 0;
                self.sent += 1;
                true
            }
            fn system_down(&mut self, _key: u16) -> bool {
                self.down
            }
            fn target_down(&mut self, _key: u16) -> bool {
                // Taken only after the wait has looked at the window twice,
                // so the press below is seen DURING the wait.
                if self.pauses >= 2 {
                    self.down
                } else {
                    !self.down
                }
            }
            fn watch(&mut self) -> Watch {
                if self.sent >= 1 {
                    Watch::Escape
                } else {
                    Watch::Steady
                }
            }
            fn pause(&mut self, _how_long: Duration) {
                self.pauses += 1;
            }
        }
        let steps = [
            Step {
                event: true,
                key: HOME,
                down: true,
            },
            Step {
                event: false,
                key: HOME,
                down: false,
            },
        ];
        let waits = Waits {
            system: Duration::from_secs(1),
            target: Some(Duration::from_secs(5)),
        };
        let mut late = Late {
            down: false,
            pauses: 0,
            sent: 0,
        };
        let start = std::time::Instant::now();
        assert_eq!(
            press_confirmed(&steps, &mut late, waits),
            Pressed {
                taken: 1,
                stop: Some(StopReason::Escape)
            },
            "the key in flight is counted, and nothing goes after it"
        );
        assert_eq!(late.sent, 1, "the next event never went out");
        assert!(
            start.elapsed() < Duration::from_secs(2),
            "stopped once the key landed, not at the bound"
        );
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
            press_confirmed(&steps, &mut system, UNPACED).taken,
            1,
            "the held down passes, the up does not"
        );
    }

    #[test]
    fn the_wait_ends_on_its_bound_rather_than_never() {
        struct Deaf;
        impl Wire<()> for Deaf {
            fn send(&mut self, (): ()) -> bool {
                true
            }
            fn system_down(&mut self, _key: u16) -> bool {
                false
            }
            fn target_down(&mut self, _key: u16) -> bool {
                false
            }
            fn watch(&mut self) -> Watch {
                Watch::Steady
            }
            fn pause(&mut self, how_long: Duration) {
                std::thread::sleep(how_long);
            }
        }
        let bound = Duration::from_millis(20);
        // Each wait on its own, so the elapsed time belongs to one of them.
        let start = std::time::Instant::now();
        assert!(!system_shows(&mut Deaf, HOME, true, bound));
        let elapsed = start.elapsed();
        assert!(elapsed >= bound, "the system wait ended early: {elapsed:?}");
        assert!(elapsed < Duration::from_secs(5), "waited {elapsed:?}");

        let start = std::time::Instant::now();
        assert_eq!(
            target_takes(&mut Deaf, HOME, true, bound),
            Err(StopReason::NotTaking)
        );
        let elapsed = start.elapsed();
        assert!(
            elapsed >= bound,
            "the application wait ended early: {elapsed:?}"
        );
        assert!(elapsed < Duration::from_secs(5), "waited {elapsed:?}");

        // And through the whole send, a deaf system is a dropped event.
        let steps = [Step {
            event: (),
            key: HOME,
            down: true,
        }];
        let waits = Waits {
            system: bound,
            target: None,
        };
        assert_eq!(
            press_confirmed(&steps, &mut Deaf, waits),
            Pressed {
                taken: 0,
                stop: Some(StopReason::Dropped)
            }
        );
    }

    #[test]
    fn an_answer_that_comes_late_but_within_the_bound_is_waited_for() {
        // The system answers within a fraction of a millisecond, and the
        // application within a few - but not on the first look. A wait that
        // gave up on the first look would call every such event dropped.
        struct Late {
            looks: usize,
            down: bool,
        }
        impl Wire<bool> for Late {
            fn send(&mut self, down: bool) -> bool {
                self.down = down;
                self.looks = 0;
                true
            }
            fn system_down(&mut self, _key: u16) -> bool {
                self.looks += 1;
                if self.looks > 3 {
                    self.down
                } else {
                    !self.down
                }
            }
            fn target_down(&mut self, _key: u16) -> bool {
                self.looks += 1;
                if self.looks > 6 {
                    self.down
                } else {
                    !self.down
                }
            }
            fn watch(&mut self) -> Watch {
                Watch::Steady
            }
            fn pause(&mut self, _how_long: Duration) {}
        }
        let steps = [
            Step {
                event: true,
                key: HOME,
                down: true,
            },
            Step {
                event: false,
                key: HOME,
                down: false,
            },
        ];
        let waits = Waits {
            system: Duration::from_secs(1),
            target: Some(Duration::from_secs(1)),
        };
        let mut late = Late {
            looks: 0,
            down: false,
        };
        assert_eq!(
            press_confirmed(&steps, &mut late, waits),
            Pressed {
                taken: 2,
                stop: None
            }
        );
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
    fn a_chord_acts_on_its_main_key_never_on_shift_alone() {
        assert_eq!(chords_acted(2, 0, false), 2, "Home not taken: nothing more");
        assert_eq!(chords_acted(2, 1, false), 3, "Home down taken: it acted");
        assert_eq!(chords_acted(0, 1, true), 0, "Shift alone acts on nothing");
        assert_eq!(chords_acted(0, 2, true), 1, "Shift+End: End went down");
        assert_eq!(chords_acted(0, 0, true), 0);
    }

    #[test]
    fn the_bounds_are_far_above_the_measured_answers_and_below_patience() {
        // 0.25 ms was the slowest system answer measured (2026-10-05), 418 ms the
        // slowest application answer (2026-10-06). The bounds are safety margins
        // and a system definition, not tuned numbers - this keeps them from
        // drifting into one in either direction.
        assert!(TAKEN_WAIT >= Duration::from_millis(250));
        assert!(TAKEN_WAIT <= Duration::from_secs(2));
        assert_eq!(
            TARGET_WAIT,
            Duration::from_secs(5),
            "the system's own 'not responding'"
        );
    }
}
