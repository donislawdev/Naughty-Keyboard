//! Driving the sequence from the registered shortcuts: the loop the palette
//! runs on its worker thread.
//!
//! [`AdvanceSequence`] answers one action at a time. Something has to sit
//! between the shortcuts and it - wait for a press, hand it over, hand the
//! outcome to whoever draws, and decide what to do with a press that arrived
//! while the previous one was still being delivered. That something is here,
//! in `app`, because every one of those decisions is about the sequence's
//! state and `architektura.md` 6a keeps that state behind one gate.
//!
//! # What the caller supplies, and why it is shaped this way
//!
//! The GUI (`nkb-gui`, piece C5) runs this on a thread that is not the main
//! one, since a send takes seconds and the main thread belongs to Slint
//! (`architektura.md` 6.5). It supplies:
//!
//! - the live shortcuts it registered at startup, so it has already seen which
//!   were taken and said so;
//! - `present`, which carries each [`Outcome`] to the view - for Slint, an
//!   `invoke_from_event_loop`. Nothing here knows that;
//! - `keep_going`, consulted once per `tick`, which is how the loop is stopped
//!   from outside without a channel of its own: the caller flips a flag when
//!   the window closes and the loop notices within one tick.
//!
//! # `W1` with an atomic send: drain, name, do not replay
//!
//! `architektura.md` 6a `W1` says a shortcut pressed while a value is still
//! going in is ignored, but not in silence. The sequence machine encodes that
//! for the char-by-char delivery that is still to come. Today a send is one
//! blocking call, so the machine never sees a press land during `inserting` -
//! the press waits in the shortcut queue instead, and would be replayed as a
//! fresh "next value" the moment the send finished. That is exactly the
//! queueing `ux-spec.md` 3 rejects: hold the key for a second and a dozen
//! values go in, of which the tester sees the last.
//!
//! So after every action that reached the field, the loop asks the shortcuts
//! for whatever is already queued - a zero wait - and answers each such press
//! with [`Message::PressedWhileBusy`] rather than acting on it. The gate is
//! [`Outcome::attempted_send`]: a press queued behind an instant answer (an
//! end-of-pack warning, a missing target) was not pressed while the tool was
//! busy, and dropping it would lose the "press again to start over" that
//! `ux-spec.md` 4 promises.

use std::time::Duration;

use crate::advance_sequence::{AdvanceSequence, Message, Outcome, Ports};
use crate::ports::{LiveShortcuts, Wait};

/// Why the loop returned. It runs until told to stop or until it cannot go on,
/// and the two are told apart because the second is worth a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ended {
    /// `keep_going` said no. The presses still queued, if any, were left
    /// unread on purpose: the caller is shutting down.
    Stopped,
    /// The shortcuts reported [`Wait::Gone`] - the thread holding them has
    /// ended, so no press can ever arrive. The palette must say so rather than
    /// sit there looking alive.
    ShortcutsGone,
}

/// Runs the sequence from the shortcuts until stopped.
///
/// `tick` is how long one wait for a press lasts before `keep_going` is asked
/// again; it bounds how long a stop request can go unnoticed. A real caller
/// gives it a fraction of a second. A zero tick is not refused - a test with a
/// scripted source wants exactly that - but in a product it would spin.
///
/// Every outcome reaches `present` in the order it was produced, including one
/// per press dropped for arriving during a send.
pub fn drive_sequence(
    shortcuts: &dyn LiveShortcuts,
    sequence: &mut AdvanceSequence,
    ports: &Ports<'_>,
    tick: Duration,
    keep_going: &mut dyn FnMut() -> bool,
    present: &mut dyn FnMut(Outcome),
) -> Ended {
    loop {
        if !keep_going() {
            return Ended::Stopped;
        }
        let action = match shortcuts.next(tick) {
            Wait::Nothing => {
                // A quiet tick is where a window leaving the front is noticed,
                // so the clipboard bar for it goes (`D72`). It asks nothing
                // unless that bar is up.
                if let Some(outcome) = sequence.on_idle(ports) {
                    present(outcome);
                }
                continue;
            }
            Wait::Gone => return Ended::ShortcutsGone,
            Wait::Pressed(action) => action,
        };

        let outcome = sequence.on_action(action, ports);
        let reached_the_field = outcome.attempted_send;
        present(outcome);

        if reached_the_field {
            // Whatever queued up during the send was pressed while busy. It is
            // answered, one message per press, and never acted on - `W1`.
            while let Wait::Pressed(late) = shortcuts.next(Duration::ZERO) {
                present(Outcome {
                    sequence: sequence.sequence(),
                    sent: None,
                    messages: vec![Message::PressedWhileBusy { action: late }],
                    attempted_send: false,
                    clipboard_for_window: sequence.clipboard_for_window(),
                });
            }
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
    use crate::ports::{KeystrokeError, ShortcutRegistration, TargetRef};
    use crate::test_support::*;
    use nkb_core::hotkeys::HotkeyAction;
    use nkb_core::pack::Risk;
    use std::cell::RefCell;
    use std::collections::VecDeque;

    /// When a scripted press becomes visible to the loop.
    ///
    /// The real queue has a notion of time the fake must keep: a press that is
    /// `Queued` is already waiting and comes out even for a zero wait, while
    /// one that arrives `Later` needs a real wait. Without this distinction a
    /// test could not tell "drained as pressed-while-busy" from "processed
    /// normally on the next turn", which is the whole point of the loop.
    #[derive(Clone, Copy)]
    enum Arrives {
        Queued,
        Later,
    }

    struct Scripted {
        script: RefCell<VecDeque<(Arrives, Wait)>>,
        waits: RefCell<Vec<Duration>>,
    }

    impl Scripted {
        fn of(steps: &[(Arrives, Wait)]) -> Self {
            Self {
                script: RefCell::new(steps.iter().copied().collect()),
                waits: RefCell::new(Vec::new()),
            }
        }
        fn remaining(&self) -> usize {
            self.script.borrow().len()
        }
    }

    impl LiveShortcuts for Scripted {
        fn outcomes(&self) -> &[(HotkeyAction, ShortcutRegistration)] {
            &[]
        }
        fn next(&self, wait: Duration) -> Wait {
            self.waits.borrow_mut().push(wait);
            let mut script = self.script.borrow_mut();
            match script.front() {
                Some((Arrives::Queued, _)) => script.pop_front().map_or(Wait::Nothing, |(_, w)| w),
                Some((Arrives::Later, _)) if !wait.is_zero() => {
                    script.pop_front().map_or(Wait::Nothing, |(_, w)| w)
                }
                _ => Wait::Nothing,
            }
        }
    }

    const TICK: Duration = Duration::from_millis(50);
    const NEXT: Wait = Wait::Pressed(HotkeyAction::NextValue);

    /// A `keep_going` that answers yes `turns` times, then no.
    fn turns(mut turns: usize) -> impl FnMut() -> bool {
        move || {
            if turns == 0 {
                false
            } else {
                turns -= 1;
                true
            }
        }
    }

    fn run(
        shortcuts: &Scripted,
        sequence: &mut AdvanceSequence,
        mut going: impl FnMut() -> bool,
    ) -> (Ended, Vec<Outcome>) {
        let mut presented = Vec::new();
        let ended = drive_sequence(
            shortcuts,
            sequence,
            &Kit::ready().ports(),
            TICK,
            &mut going,
            &mut |outcome| presented.push(outcome),
        );
        (ended, presented)
    }

    #[test]
    fn a_quiet_tick_lets_the_clipboard_bar_go_when_another_window_comes_forward() {
        // `D72`: the window in front took no typing, its value went to the
        // clipboard and the bar went up. The tester switches windows, and the
        // next quiet tick takes the bar down - no press needed.
        let shortcuts = Scripted::of(&[(Arrives::Later, NEXT)]);
        let mut sequence = chosen(Risk::Normal);
        let kit = Kit::with_keys(FakeKeys::failing(KeystrokeError::HigherPrivileges));
        kit.direct.set_target(Some(TargetRef(7)));
        let mut presented: Vec<Outcome> = Vec::new();
        let mut going = turns(3);
        let _ = drive_sequence(
            &shortcuts,
            &mut sequence,
            &kit.ports(),
            TICK,
            &mut going,
            &mut |outcome| {
                if outcome.clipboard_for_window {
                    kit.direct.set_target(Some(TargetRef(8)));
                }
                presented.push(outcome);
            },
        );

        assert_eq!(
            presented.len(),
            2,
            "the press, then the tick that saw the window go"
        );
        assert!(presented[0].clipboard_for_window);
        assert!(!presented[1].clipboard_for_window);
        assert!(presented[1].sent.is_none(), "a tick sends nothing");
    }

    #[test]
    fn a_press_becomes_a_send_and_reaches_present() {
        let shortcuts = Scripted::of(&[(Arrives::Later, NEXT)]);
        let mut sequence = chosen(Risk::Normal);

        let (ended, presented) = run(&shortcuts, &mut sequence, turns(1));

        assert_eq!(ended, Ended::Stopped);
        assert_eq!(presented.len(), 1, "one press, one outcome");
        assert!(presented[0].sent.is_some(), "the press sent a value");
        assert_eq!(sequence.counter(), Some((1, 3)));
        assert_eq!(
            *shortcuts.waits.borrow(),
            vec![TICK, Duration::ZERO],
            "one real wait for the press, then one zero-wait drain after the send"
        );
    }

    #[test]
    fn presses_queued_during_a_send_are_named_not_replayed() {
        // Three presses: one that starts the send, two that pile up behind it
        // while the value is going in. W1: the two are answered, not sent.
        let shortcuts = Scripted::of(&[
            (Arrives::Later, NEXT),
            (Arrives::Queued, NEXT),
            (Arrives::Queued, NEXT),
        ]);
        let mut sequence = chosen(Risk::Normal);

        let (ended, presented) = run(&shortcuts, &mut sequence, turns(1));

        assert_eq!(ended, Ended::Stopped);
        assert_eq!(presented.len(), 3);
        assert!(presented[0].sent.is_some());
        for late in &presented[1..] {
            assert!(late.sent.is_none(), "a dropped press sends nothing");
            assert_eq!(
                late.messages,
                vec![Message::PressedWhileBusy {
                    action: HotkeyAction::NextValue
                }]
            );
        }
        assert_eq!(
            sequence.counter(),
            Some((1, 3)),
            "the counter moved once: the queued presses did not advance it"
        );
    }

    #[test]
    fn presses_behind_an_instant_answer_are_kept_for_the_next_turn() {
        // A one-value pack: the second press meets end-of-pack, an instant
        // answer, and the third must still count as "press again to start
        // over" rather than be dropped as pressed-while-busy.
        let shortcuts = Scripted::of(&[
            (Arrives::Later, NEXT),
            (Arrives::Later, NEXT),
            (Arrives::Later, NEXT),
        ]);
        let mut sequence = AdvanceSequence::new();
        sequence
            .choose_pack(
                &FakeSource,
                &FakeFormat::of(a_pack(vec![a_value("only", "solo", None)], Risk::Normal)),
                "sample",
            )
            .expect("a scripted pack loads");

        let (ended, presented) = run(&shortcuts, &mut sequence, turns(3));

        assert_eq!(ended, Ended::Stopped);
        assert_eq!(presented.len(), 3);
        assert!(
            presented[0].sent.is_some(),
            "first press sends the only value"
        );
        assert_eq!(
            presented[1].messages,
            vec![Message::EndOfPack { total: 1 }],
            "second press warns, instantly"
        );
        assert!(
            presented[2].sent.is_some(),
            "third press starts over and sends - it was not swallowed by a drain"
        );
        assert!(
            !presented
                .iter()
                .any(|o| matches!(o.messages.first(), Some(Message::PressedWhileBusy { .. }))),
            "nothing was reported as pressed while busy"
        );
    }

    #[test]
    fn a_press_before_any_pack_is_answered_without_a_drain() {
        // No pack: `on_action` answers NoPack without touching the field, so a
        // press queued right behind it is a real press for the next turn.
        let shortcuts = Scripted::of(&[(Arrives::Later, NEXT), (Arrives::Queued, NEXT)]);
        let mut sequence = AdvanceSequence::new();

        let (_, presented) = run(&shortcuts, &mut sequence, turns(2));

        assert_eq!(presented.len(), 2);
        assert!(
            presented
                .iter()
                .all(|o| o.messages == vec![Message::NoPack]),
            "both presses got the honest answer, neither was dropped"
        );
    }

    #[test]
    fn gone_ends_the_loop_and_says_which_way() {
        let shortcuts = Scripted::of(&[(Arrives::Later, Wait::Gone)]);
        let mut sequence = chosen(Risk::Normal);

        let (ended, presented) = run(&shortcuts, &mut sequence, turns(5));

        assert_eq!(ended, Ended::ShortcutsGone);
        assert!(presented.is_empty());
    }

    #[test]
    fn a_stop_is_honoured_before_any_press_is_read() {
        let shortcuts = Scripted::of(&[(Arrives::Queued, NEXT)]);
        let mut sequence = chosen(Risk::Normal);

        let (ended, presented) = run(&shortcuts, &mut sequence, turns(0));

        assert_eq!(ended, Ended::Stopped);
        assert!(presented.is_empty());
        assert_eq!(
            shortcuts.remaining(),
            1,
            "a queued press is left unread when shutting down, not consumed"
        );
        assert!(
            shortcuts.waits.borrow().is_empty(),
            "the shortcuts were never asked"
        );
    }

    #[test]
    fn nothing_pressed_within_a_tick_just_asks_again() {
        // An empty script answers Nothing to every wait; the loop must keep
        // asking, once per tick, until told to stop.
        let shortcuts = Scripted::of(&[]);
        let mut sequence = chosen(Risk::Normal);

        let (ended, presented) = run(&shortcuts, &mut sequence, turns(3));

        assert_eq!(ended, Ended::Stopped);
        assert!(presented.is_empty());
        assert_eq!(*shortcuts.waits.borrow(), vec![TICK, TICK, TICK]);
    }
}
