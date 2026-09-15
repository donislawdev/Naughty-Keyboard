//! The sequence: where the tester is inside a pack, and what a keystroke means
//! there.
//!
//! # Types here, the value itself one layer out
//!
//! architektura.md 6a puts every piece of mutable state in `app`, behind one
//! gate, and calls a core scattered with state both the source of races and the
//! source of ugly code. The same document also asks that the core be what stops
//! `inserting` from advancing (`W1`).
//!
//! Both hold at once, because what lives here is a FUNCTION: `state × event ->
//! (state, effects)`. Nothing in this module remembers anything between calls.
//! `app` holds the current [`Sequence`] and hands it back in.
//!
//! # Two axes, not one list of states
//!
//! `ux-spec.md` 4 tabulates six states, `degraded` among them. Written that way
//! it does not survive one question: what does "next" do while degraded? Leaving
//! the state loses the fact that delivery is degraded - and `ux-spec.md` 2 calls
//! that bar PERMANENT, because a message that vanished stops existing while the
//! limitation stays.
//!
//! So delivery is a SECOND axis. Where the counter stands and how values reach
//! the field are independent facts, and the palette shows both.
//!
//! # No messages, only keys
//!
//! This layer returns facts. `ux-spec.md` 6 fixes the wording of what a person
//! reads, and untouchable rule 9 puts that wording behind translation keys, so
//! an [`Effect`] names WHICH thing must be said and never says it.
//!
//! # What this deliberately does not cover
//!
//! **Paired values.** `ux-spec.md` 4 gives them a sequence of their own -
//! `pair-a`, `pair-b` - and step 3 of the plan of work does not include them.
//! Folding them in now would double the states before any of them has been used
//! once. When they arrive they extend [`Position`], not [`Sequence`].

use core::fmt;

/// How far through a pack the tester is.
///
/// # Why the count is carried rather than looked up
///
/// The sequence never holds a pack. It holds a NUMBER of values, fixed when the
/// pack was chosen, and `W5` is why: a catalogue reloaded mid-sequence would
/// otherwise silently redefine what `7/34` means. `ux-spec.md` 4 resolves that by
/// making a reload an explicit act that resets to `ready`, and a sequence that
/// cannot see the pack cannot be caught out by one changing underneath it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Position {
    /// Nothing chosen yet.
    NoPack,
    /// A pack is chosen and nothing has been sent from it.
    Ready { total: usize },
    /// `done` values have been sent; the next one is number `done + 1`.
    Running { done: usize, total: usize },
    /// A value is on its way into the field right now.
    ///
    /// Carries what it was doing, because an interruption has to be able to say
    /// how far the counter got - and because `W1` needs somewhere to refuse
    /// from that is not "somewhere in the middle".
    Inserting { sending: usize, total: usize },
    /// Every value has been sent.
    ///
    /// 🔴 `warned` is not bookkeeping. `ux-spec.md` 4 refuses to wrap silently:
    /// the first press says `End of pack (34/34). Press again to start over.`
    /// and only the second one starts over. Without this bit there are two
    /// options and both are forbidden - wrap quietly, or never wrap at all.
    Exhausted { total: usize, warned: bool },
}

/// How values are reaching the field. Independent of [`Position`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Delivery {
    /// Straight into the field. The normal case.
    #[default]
    Direct,
    /// Direct delivery failed, so values go via the clipboard.
    ///
    /// `ux-spec.md` 2: the bar saying so is PERMANENT. Nothing in this module
    /// leaves this mode on its own - the tester asks for it, or a diagnosis
    /// does, and both are events.
    Degraded,
}

/// Where the sequence stands: both axes, together.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sequence {
    pub position: Position,
    pub delivery: Delivery,
}

impl Default for Sequence {
    fn default() -> Self {
        Self {
            position: Position::NoPack,
            delivery: Delivery::Direct,
        }
    }
}

/// What happened, from outside the sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// A pack was chosen. `total` is how many values it holds.
    PackChosen { total: usize },
    /// The "next value" shortcut.
    Next,
    /// The "previous value" shortcut.
    Previous,
    /// The "start this pack from the beginning" shortcut.
    Restart,
    /// A value finished arriving in the field.
    InsertionFinished,
    /// Direct delivery is not working at all, so the value did not arrive and
    /// the clipboard is the way forward now. This is the persistent inability
    /// (`product-spec.md` 9.3), NOT a passing hiccup.
    InsertionFailed,
    /// The send never started, and the field was left untouched: nothing holds
    /// the focus, or a modifier is physically held. Distinct from
    /// [`Event::InsertionFailed`] because the answer is "try again", not "switch
    /// to the clipboard for good", and distinct from [`Event::Cancelled`]
    /// because there is no fragment in the field to report. The sequence returns
    /// to exactly where it stood before the `Next`, and the message that says
    /// WHY is the caller's, because the reason is a fact about delivery rather
    /// than about the sequence.
    InsertionRefused,
    /// `Escape` during an insertion, or a partial send: either way a fragment
    /// was left in the field.
    Cancelled,
    /// The focused window or field changed.
    TargetChanged,
    /// There is nothing focused at all any more.
    TargetLost,
}

/// What the layer outside must do or say. Never a sentence - a name for one.
///
/// # Why the counter travels inside the effect
///
/// `ux-spec.md` 6 fixes two messages that quote numbers back to the reader:
/// `End of pack (34/34)` and `counter is still at 7/34`. Handing the caller a
/// bare key would mean it has to reach into the state to fill them in, which is
/// exactly how a message and the thing it describes drift apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// Clear the field, then send value number `index` (counting from 1).
    ///
    /// One effect rather than two, because `ux-spec.md` 4 calls clearing part of
    /// sending: a caller free to do one without the other is a caller that will
    /// eventually append value twenty to the previous nineteen.
    SendValue { index: usize },
    /// Say that an insertion is already running, and how to stop it.
    AnnounceStillInserting,
    /// Say the pack is finished, quoting `total`. The NEXT press starts over.
    AnnounceEndOfPack { total: usize },
    /// Say the target moved while the counter stayed, quoting both numbers.
    AnnounceCounterKept { done: usize, total: usize },
    /// Say there is nowhere to send to.
    AnnounceNoTarget,
    /// Say no pack has been chosen yet.
    AnnounceNoPack,
    /// Say the insertion stopped part-way, and how far it got.
    AnnounceInterrupted { index: usize },
    /// Say direct delivery stopped working and the clipboard is being used.
    AnnounceDegraded,
}

impl fmt::Display for Effect {
    /// The developer-facing marker, not the text a person reads. Same shape as
    /// `SourceError`: a stable name a log or a test can match on.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SendValue { .. } => f.write_str("send-value"),
            Self::AnnounceStillInserting => f.write_str("still-inserting"),
            Self::AnnounceEndOfPack { .. } => f.write_str("end-of-pack"),
            Self::AnnounceCounterKept { .. } => f.write_str("counter-kept"),
            Self::AnnounceNoTarget => f.write_str("no-target"),
            Self::AnnounceNoPack => f.write_str("no-pack"),
            Self::AnnounceInterrupted { .. } => f.write_str("interrupted"),
            Self::AnnounceDegraded => f.write_str("degraded"),
        }
    }
}

/// What one event did: where the sequence now stands, and what must follow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub sequence: Sequence,
    /// In the order they must happen. Empty is a legitimate answer, and it means
    /// exactly one thing: the event was not applicable here and nothing is owed.
    pub effects: Vec<Effect>,
}

impl Sequence {
    /// A sequence with nothing chosen.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            position: Position::NoPack,
            delivery: Delivery::Direct,
        }
    }

    /// How many values have been sent, and out of how many.
    ///
    /// The palette's `7 / 34`. `NoPack` has no counter at all, which is a third
    /// answer and not `0 / 0`.
    #[must_use]
    pub const fn counter(&self) -> Option<(usize, usize)> {
        match self.position {
            Position::NoPack => None,
            Position::Ready { total } => Some((0, total)),
            Position::Running { done, total } => Some((done, total)),
            // The value in flight is not counted until it lands. An insertion
            // that is cancelled must not leave the counter claiming a value the
            // field never received.
            Position::Inserting { sending, total } => Some((sending.saturating_sub(1), total)),
            Position::Exhausted { total, .. } => Some((total, total)),
        }
    }

    /// Whether an insertion is in flight.
    ///
    /// Named rather than left to pattern matching, because `W3` makes `Escape`
    /// capture a PROPERTY OF THIS STATE - switched on entering and off leaving,
    /// in the same transition, never by two independent calls elsewhere.
    #[must_use]
    pub const fn is_inserting(&self) -> bool {
        matches!(self.position, Position::Inserting { .. })
    }

    /// Applies one event.
    #[must_use]
    pub fn apply(self, event: Event) -> Step {
        // 🔴 `W1` lives here and nowhere else: while a value is in flight, the
        // only events that mean anything are the ones that END the flight. A
        // second "next" is refused - and refused ALOUD, because a silent one
        // would interleave two values in somebody else's field, which is the
        // worst thing this product can do.
        if let Position::Inserting { sending, total } = self.position {
            return self.while_inserting(event, sending, total);
        }

        match event {
            Event::PackChosen { total } => self.pack_chosen(total),
            Event::Next => self.next(),
            Event::Previous => self.previous(),
            Event::Restart => self.restart(),
            Event::TargetChanged => self.target_changed(),
            Event::TargetLost => self.nothing_but(Effect::AnnounceNoTarget),
            Event::InsertionFailed => self.degrade(),
            // Nothing is in flight, so none of these describes anything.
            // `W3`: `Escape` belongs to `inserting` and is not captured outside
            // it, so arriving here at all means somebody wired it globally.
            Event::InsertionFinished | Event::InsertionRefused | Event::Cancelled => self.nothing(),
        }
    }

    fn while_inserting(self, event: Event, sending: usize, total: usize) -> Step {
        match event {
            Event::InsertionFinished => Step {
                sequence: Self {
                    position: if sending >= total {
                        Position::Exhausted {
                            total,
                            warned: false,
                        }
                    } else {
                        Position::Running {
                            done: sending,
                            total,
                        }
                    },
                    ..self
                },
                effects: Vec::new(),
            },
            // The value did not land, so the counter must not move: `done` stays
            // at what it was before this attempt.
            Event::InsertionFailed => Step {
                sequence: Self {
                    position: Position::Running {
                        done: sending.saturating_sub(1),
                        total,
                    },
                    delivery: Delivery::Degraded,
                },
                effects: vec![Effect::AnnounceDegraded],
            },
            // The send never started: no target, or a held modifier. Nothing
            // reached the field and nothing is degraded - the sequence returns
            // to exactly where it stood before this `Next`. From the first send
            // that is `Ready`; from any later one it is `Running` on the value
            // already done. No effect: the caller says WHY, because the reason
            // is a fact about delivery, not about the sequence.
            Event::InsertionRefused => Step {
                sequence: Self {
                    position: if sending <= 1 {
                        Position::Ready { total }
                    } else {
                        Position::Running {
                            done: sending - 1,
                            total,
                        }
                    },
                    ..self
                },
                effects: Vec::new(),
            },
            // Escape, or the target moving out from under a running insertion
            // (`W2`). Both leave a FRAGMENT in the field, and both say so.
            Event::Cancelled | Event::TargetChanged | Event::TargetLost => Step {
                sequence: Self {
                    position: Position::Running {
                        done: sending.saturating_sub(1),
                        total,
                    },
                    ..self
                },
                effects: vec![Effect::AnnounceInterrupted { index: sending }],
            },
            // The refusal `W1` exists for. Loud, and the state does not budge.
            Event::Next | Event::Previous | Event::Restart => Step {
                sequence: self,
                effects: vec![Effect::AnnounceStillInserting],
            },
            // A pack cannot be swapped under a value that is still arriving.
            Event::PackChosen { .. } => Step {
                sequence: self,
                effects: vec![Effect::AnnounceStillInserting],
            },
        }
    }

    fn pack_chosen(self, total: usize) -> Step {
        // A pack with no values cannot be started, and saying `0/0` and waiting
        // would be a sequence that can never move. The validator refuses such a
        // pack, but this layer does not get to assume the validator ran.
        let position = if total == 0 {
            Position::Exhausted {
                total: 0,
                warned: true,
            }
        } else {
            Position::Ready { total }
        };
        Step {
            sequence: Self { position, ..self },
            effects: Vec::new(),
        }
    }

    fn next(self) -> Step {
        match self.position {
            Position::NoPack => self.nothing_but(Effect::AnnounceNoPack),
            Position::Ready { total } => self.begin(1, total),
            Position::Running { done, total } => self.begin(done + 1, total),
            // First press warns, second press starts over - `ux-spec.md` 4.
            Position::Exhausted {
                total,
                warned: false,
            } => Step {
                sequence: Self {
                    position: Position::Exhausted {
                        total,
                        warned: true,
                    },
                    ..self
                },
                effects: vec![Effect::AnnounceEndOfPack { total }],
            },
            Position::Exhausted {
                total,
                warned: true,
            } => {
                if total == 0 {
                    // Nothing to start over with. Warn again rather than pretend.
                    self.nothing_but(Effect::AnnounceEndOfPack { total })
                } else {
                    self.begin(1, total)
                }
            }
            Position::Inserting { .. } => self.nothing_but(Effect::AnnounceStillInserting),
        }
    }

    fn previous(self) -> Step {
        match self.position {
            Position::NoPack => self.nothing_but(Effect::AnnounceNoPack),
            // Nothing has been sent, so there is nothing behind. Silence would
            // leave the tester pressing a key that does nothing and wondering.
            Position::Ready { total } => {
                self.nothing_but(Effect::AnnounceCounterKept { done: 0, total })
            }
            Position::Running { done, total } if done <= 1 => {
                self.nothing_but(Effect::AnnounceCounterKept { done, total })
            }
            Position::Running { done, total } => self.begin(done - 1, total),
            Position::Exhausted { total, .. } if total > 0 => self.begin(total, total),
            Position::Exhausted { total, .. } => {
                self.nothing_but(Effect::AnnounceEndOfPack { total })
            }
            Position::Inserting { .. } => self.nothing_but(Effect::AnnounceStillInserting),
        }
    }

    fn restart(self) -> Step {
        match self.position {
            Position::NoPack => self.nothing_but(Effect::AnnounceNoPack),
            Position::Ready { .. } => Step {
                sequence: self,
                effects: Vec::new(),
            },
            Position::Running { total, .. } | Position::Exhausted { total, .. } => Step {
                sequence: Self {
                    position: Position::Ready { total },
                    ..self
                },
                effects: Vec::new(),
            },
            Position::Inserting { .. } => self.nothing_but(Effect::AnnounceStillInserting),
        }
    }

    /// The target moved while nothing was in flight.
    ///
    /// 🔴 The state does not change and the counter does not reset - and that is
    /// exactly why this produces an effect. `product-spec.md` 8.2 chose a global
    /// counter knowing it means the second field starts at value eight, and
    /// `ux-spec.md` 4 compensates by SAYING so. A transition returning only a
    /// state would have nowhere to put that sentence.
    fn target_changed(self) -> Step {
        match self.position {
            Position::Running { done, total } if done > 0 => {
                self.nothing_but(Effect::AnnounceCounterKept { done, total })
            }
            _ => self.nothing(),
        }
    }

    fn degrade(self) -> Step {
        Step {
            sequence: Self {
                delivery: Delivery::Degraded,
                ..self
            },
            effects: vec![Effect::AnnounceDegraded],
        }
    }

    fn begin(self, index: usize, total: usize) -> Step {
        Step {
            sequence: Self {
                position: Position::Inserting {
                    sending: index,
                    total,
                },
                ..self
            },
            effects: vec![Effect::SendValue { index }],
        }
    }

    fn nothing(self) -> Step {
        Step {
            sequence: self,
            effects: Vec::new(),
        }
    }

    fn nothing_but(self, effect: Effect) -> Step {
        Step {
            sequence: self,
            effects: vec![effect],
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

    /// Applies events in order and hands back the last step.
    fn run(start: Sequence, events: &[Event]) -> Step {
        let mut sequence = start;
        let mut last = Step {
            sequence,
            effects: Vec::new(),
        };
        for event in events {
            last = sequence.apply(*event);
            sequence = last.sequence;
        }
        last
    }

    fn sent_values(step: &Step) -> Vec<usize> {
        step.effects
            .iter()
            .filter_map(|effect| match effect {
                Effect::SendValue { index } => Some(*index),
                _ => None,
            })
            .collect()
    }

    // ---- (N) the normal path ----------------------------------------------

    #[test]
    fn a_pack_is_walked_one_value_at_a_time_and_ends_exhausted() {
        let step = run(
            Sequence::new(),
            &[
                Event::PackChosen { total: 3 },
                Event::Next,
                Event::InsertionFinished,
                Event::Next,
                Event::InsertionFinished,
                Event::Next,
                Event::InsertionFinished,
            ],
        );
        assert_eq!(step.sequence.counter(), Some((3, 3)));
        assert_eq!(
            step.sequence.position,
            Position::Exhausted {
                total: 3,
                warned: false
            }
        );
    }

    #[test]
    fn each_press_sends_the_next_value_by_number() {
        let mut sequence = Sequence::new()
            .apply(Event::PackChosen { total: 3 })
            .sequence;
        let mut sent = Vec::new();
        for _ in 0..3 {
            let step = sequence.apply(Event::Next);
            sent.extend(sent_values(&step));
            sequence = step.sequence.apply(Event::InsertionFinished).sequence;
        }
        assert_eq!(
            sent,
            [1, 2, 3],
            "values go in the pack's own order, counting from 1"
        );
    }

    // ---- W1: two insertions must never overlap ----------------------------

    #[test]
    fn a_second_next_during_an_insertion_is_refused_and_says_so() {
        let sequence = run(
            Sequence::new(),
            &[Event::PackChosen { total: 5 }, Event::Next],
        )
        .sequence;
        assert!(sequence.is_inserting());

        let step = sequence.apply(Event::Next);
        assert_eq!(
            step.effects,
            vec![Effect::AnnounceStillInserting],
            "W1: refused, but never silently - two insertions would interleave in \
             somebody else's field"
        );
        assert_eq!(
            step.sequence, sequence,
            "the refusal must not move the sequence"
        );
        assert!(sent_values(&step).is_empty());
    }

    #[test]
    fn a_pack_cannot_be_swapped_under_a_value_that_is_still_arriving() {
        let sequence = run(
            Sequence::new(),
            &[Event::PackChosen { total: 5 }, Event::Next],
        )
        .sequence;
        let step = sequence.apply(Event::PackChosen { total: 9 });
        assert_eq!(step.sequence, sequence);
        assert_eq!(step.effects, vec![Effect::AnnounceStillInserting]);
    }

    // ---- the end of a pack does not wrap quietly --------------------------

    #[test]
    fn the_end_of_a_pack_warns_first_and_starts_over_only_on_the_second_press() {
        let sequence = run(
            Sequence::new(),
            &[
                Event::PackChosen { total: 1 },
                Event::Next,
                Event::InsertionFinished,
            ],
        )
        .sequence;
        assert_eq!(
            sequence.position,
            Position::Exhausted {
                total: 1,
                warned: false
            }
        );

        let first = sequence.apply(Event::Next);
        assert_eq!(
            first.effects,
            vec![Effect::AnnounceEndOfPack { total: 1 }],
            "the first press warns"
        );
        assert!(
            sent_values(&first).is_empty(),
            "a silent wrap makes the tester walk the pack again believing it is new"
        );

        let second = first.sequence.apply(Event::Next);
        assert_eq!(sent_values(&second), [1], "the second press starts over");
    }

    // ---- (A) failure ------------------------------------------------------

    #[test]
    fn a_failed_insertion_leaves_the_counter_where_it_was_and_degrades_delivery() {
        let before = run(
            Sequence::new(),
            &[
                Event::PackChosen { total: 4 },
                Event::Next,
                Event::InsertionFinished,
            ],
        )
        .sequence;
        assert_eq!(before.counter(), Some((1, 4)));

        let step = before
            .apply(Event::Next)
            .sequence
            .apply(Event::InsertionFailed);
        assert_eq!(
            step.sequence.counter(),
            Some((1, 4)),
            "value two never arrived, so it must not be counted as sent"
        );
        assert_eq!(step.sequence.delivery, Delivery::Degraded);
        assert_eq!(step.effects, vec![Effect::AnnounceDegraded]);
    }

    #[test]
    fn delivery_is_a_second_axis_and_degrading_does_not_disturb_the_counter() {
        // The whole reason `degraded` is not a sixth position: a tester in
        // clipboard mode is still somewhere in the pack, and ux-spec.md 2 calls
        // the bar saying so PERMANENT.
        let sequence = run(
            Sequence::new(),
            &[
                Event::PackChosen { total: 9 },
                Event::Next,
                Event::InsertionFinished,
                Event::Next,
                Event::InsertionFinished,
            ],
        )
        .sequence;
        let degraded = sequence.apply(Event::InsertionFailed).sequence;

        assert_eq!(
            degraded.counter(),
            Some((2, 9)),
            "the counter survives degradation"
        );
        assert_eq!(degraded.delivery, Delivery::Degraded);

        let after = run(degraded, &[Event::Next, Event::InsertionFinished]).sequence;
        assert_eq!(
            after.delivery,
            Delivery::Degraded,
            "nothing here leaves clipboard mode on its own"
        );
        assert_eq!(after.counter(), Some((3, 9)));
    }

    // ---- (T) teardown -----------------------------------------------------

    #[test]
    fn escape_during_an_insertion_reports_which_value_was_left_half_written() {
        let sequence = run(
            Sequence::new(),
            &[
                Event::PackChosen { total: 6 },
                Event::Next,
                Event::InsertionFinished,
                Event::Next,
            ],
        )
        .sequence;

        let step = sequence.apply(Event::Cancelled);
        assert_eq!(
            step.effects,
            vec![Effect::AnnounceInterrupted { index: 2 }],
            "the field holds part of value two, and silence about that is the failure mode"
        );
        assert_eq!(
            step.sequence.counter(),
            Some((1, 6)),
            "an interrupted value is not a sent value"
        );
        assert!(!step.sequence.is_inserting());
    }

    #[test]
    fn the_target_moving_mid_insertion_interrupts_just_like_escape() {
        // W2: the safeguard checks the window before starting, and the insertion
        // takes minutes. Without this the rest lands somewhere nobody approved.
        let sequence = run(
            Sequence::new(),
            &[Event::PackChosen { total: 2 }, Event::Next],
        )
        .sequence;
        let step = sequence.apply(Event::TargetChanged);
        assert_eq!(step.effects, vec![Effect::AnnounceInterrupted { index: 1 }]);
        assert!(!step.sequence.is_inserting());
    }

    #[test]
    fn a_refused_send_from_the_first_value_returns_to_ready_and_does_not_degrade() {
        // No target or a held modifier on the very first Next: nothing reached
        // the field, nothing is degraded, and the sequence is exactly back at
        // Ready. Crucially NOT Degraded - a passing hiccup must not switch the
        // tool to clipboard mode for good.
        let sequence = run(
            Sequence::new(),
            &[Event::PackChosen { total: 5 }, Event::Next],
        )
        .sequence;
        assert!(sequence.is_inserting());

        let step = sequence.apply(Event::InsertionRefused);
        assert_eq!(
            step.sequence.position,
            Position::Ready { total: 5 },
            "a refused first send goes back to Ready, not on to Running"
        );
        assert_eq!(
            step.sequence.delivery,
            Delivery::Direct,
            "a refusal is not a degradation"
        );
        assert!(
            step.effects.is_empty(),
            "the caller supplies the reason, not the machine"
        );
        assert_eq!(step.sequence.counter(), Some((0, 5)));
    }

    #[test]
    fn a_refused_send_from_a_later_value_returns_to_running_without_advancing() {
        let before = run(
            Sequence::new(),
            &[
                Event::PackChosen { total: 5 },
                Event::Next,
                Event::InsertionFinished,
                Event::Next,
                Event::InsertionFinished,
                Event::Next,
            ],
        )
        .sequence;
        assert_eq!(
            before.position,
            Position::Inserting {
                sending: 3,
                total: 5
            }
        );

        let step = before.apply(Event::InsertionRefused);
        assert_eq!(
            step.sequence.position,
            Position::Running { done: 2, total: 5 },
            "value three never started, so the counter stays at two done"
        );
        assert_eq!(step.sequence.delivery, Delivery::Direct);
        assert!(step.effects.is_empty());
    }

    #[test]
    fn a_refused_send_is_told_apart_from_a_failed_one() {
        // The whole reason the event exists: Failed degrades, Refused does not.
        let inserting = run(
            Sequence::new(),
            &[Event::PackChosen { total: 3 }, Event::Next],
        )
        .sequence;

        let refused = inserting.apply(Event::InsertionRefused).sequence;
        let failed = inserting.apply(Event::InsertionFailed).sequence;
        assert_eq!(refused.delivery, Delivery::Direct);
        assert_eq!(failed.delivery, Delivery::Degraded);
    }

    #[test]
    fn insertion_refused_outside_an_insertion_does_nothing() {
        // Nothing is in flight, so there is nothing to refuse.
        let ready = run(Sequence::new(), &[Event::PackChosen { total: 3 }]).sequence;
        let step = ready.apply(Event::InsertionRefused);
        assert_eq!(step.sequence, ready);
        assert!(step.effects.is_empty());
    }

    #[test]
    fn a_value_in_flight_is_not_counted_until_it_lands() {
        let sequence = run(
            Sequence::new(),
            &[Event::PackChosen { total: 4 }, Event::Next],
        )
        .sequence;
        assert_eq!(
            sequence.counter(),
            Some((0, 4)),
            "the palette must not show 1/4 for a value still on its way"
        );
    }

    // ---- the counter is kept across fields, and that is announced ---------

    #[test]
    fn changing_field_keeps_the_counter_and_the_sequence_says_so() {
        let sequence = run(
            Sequence::new(),
            &[
                Event::PackChosen { total: 34 },
                Event::Next,
                Event::InsertionFinished,
            ],
        )
        .sequence;

        let step = sequence.apply(Event::TargetChanged);
        assert_eq!(
            step.sequence, sequence,
            "product-spec.md 8.2 chose a global counter; changing field does not reset it"
        );
        assert_eq!(
            step.effects,
            vec![Effect::AnnounceCounterKept { done: 1, total: 34 }],
            "ux-spec.md 4 compensates that known weakness by SAYING it - a transition \
             returning only a state would have nowhere to put this"
        );
    }

    #[test]
    fn changing_field_before_anything_was_sent_says_nothing() {
        let sequence = Sequence::new()
            .apply(Event::PackChosen { total: 34 })
            .sequence;
        let step = sequence.apply(Event::TargetChanged);
        assert!(
            step.effects.is_empty(),
            "at 0/34 there is nothing to warn about, and a warning nobody needs teaches \
             people to ignore warnings"
        );
    }

    // ---- (D) degenerate but legal input -----------------------------------

    #[test]
    fn a_pack_with_no_values_cannot_be_started_and_never_pretends_otherwise() {
        // The validator refuses such a pack; this layer does not get to assume
        // the validator ran.
        let step = Sequence::new().apply(Event::PackChosen { total: 0 });
        assert_eq!(step.sequence.counter(), Some((0, 0)));

        let pressed = step.sequence.apply(Event::Next);
        assert_eq!(
            pressed.effects,
            vec![Effect::AnnounceEndOfPack { total: 0 }]
        );
        assert!(
            sent_values(&pressed).is_empty(),
            "there is no value one to send"
        );

        let again = pressed.sequence.apply(Event::Next);
        assert!(
            sent_values(&again).is_empty(),
            "pressing again must not start an endless warn-then-send loop"
        );
    }

    #[test]
    fn a_pack_of_one_value_is_exhausted_after_that_one_value() {
        let step = run(
            Sequence::new(),
            &[
                Event::PackChosen { total: 1 },
                Event::Next,
                Event::InsertionFinished,
            ],
        );
        assert_eq!(step.sequence.counter(), Some((1, 1)));
    }

    #[test]
    fn pressing_next_with_no_pack_chosen_says_so_rather_than_doing_nothing() {
        let step = Sequence::new().apply(Event::Next);
        assert_eq!(step.effects, vec![Effect::AnnounceNoPack]);
        assert_eq!(
            step.sequence.counter(),
            None,
            "no pack has no counter, not 0/0"
        );
    }

    #[test]
    fn previous_at_the_start_of_a_pack_does_not_walk_backwards_past_it() {
        let sequence = Sequence::new()
            .apply(Event::PackChosen { total: 5 })
            .sequence;
        let step = sequence.apply(Event::Previous);
        assert!(
            sent_values(&step).is_empty(),
            "there is nothing before value one"
        );
        assert_eq!(step.sequence, sequence);
    }

    #[test]
    fn previous_steps_back_one_value() {
        let sequence = run(
            Sequence::new(),
            &[
                Event::PackChosen { total: 5 },
                Event::Next,
                Event::InsertionFinished,
                Event::Next,
                Event::InsertionFinished,
            ],
        )
        .sequence;
        assert_eq!(sequence.counter(), Some((2, 5)));
        let step = sequence.apply(Event::Previous);
        assert_eq!(sent_values(&step), [1]);
    }

    #[test]
    fn restarting_puts_the_counter_back_to_zero_without_sending_anything() {
        let sequence = run(
            Sequence::new(),
            &[
                Event::PackChosen { total: 5 },
                Event::Next,
                Event::InsertionFinished,
                Event::Next,
                Event::InsertionFinished,
            ],
        )
        .sequence;
        let step = sequence.apply(Event::Restart);
        assert_eq!(step.sequence.counter(), Some((0, 5)));
        assert!(
            step.effects.is_empty(),
            "restarting sets a position; it does not fire a value at the field"
        );
    }

    #[test]
    fn choosing_a_pack_again_starts_it_from_the_beginning() {
        let sequence = run(
            Sequence::new(),
            &[
                Event::PackChosen { total: 5 },
                Event::Next,
                Event::InsertionFinished,
            ],
        )
        .sequence;
        let step = sequence.apply(Event::PackChosen { total: 5 });
        assert_eq!(
            step.sequence.counter(),
            Some((0, 5)),
            "ux-spec.md 4: choosing a pack sets the counter to 0 and the state to ready"
        );
    }

    #[test]
    fn losing_the_target_outside_an_insertion_says_so_and_changes_nothing() {
        let sequence = run(
            Sequence::new(),
            &[
                Event::PackChosen { total: 5 },
                Event::Next,
                Event::InsertionFinished,
            ],
        )
        .sequence;
        let step = sequence.apply(Event::TargetLost);
        assert_eq!(step.effects, vec![Effect::AnnounceNoTarget]);
        assert_eq!(step.sequence, sequence);
    }

    #[test]
    fn an_escape_arriving_when_nothing_is_in_flight_does_nothing_at_all() {
        // W3 makes Escape a property of `inserting`. Arriving here means
        // somebody wired it globally, and the answer is to do nothing rather
        // than invent a meaning for it.
        let sequence = Sequence::new()
            .apply(Event::PackChosen { total: 5 })
            .sequence;
        let step = sequence.apply(Event::Cancelled);
        assert_eq!(step.sequence, sequence);
        assert!(step.effects.is_empty());
    }

    #[test]
    fn every_effect_has_a_stable_marker_and_no_two_share_one() {
        // The markers are what a log and a test match on, so a duplicate would
        // make two different things indistinguishable after the fact.
        let all = [
            Effect::SendValue { index: 1 },
            Effect::AnnounceStillInserting,
            Effect::AnnounceEndOfPack { total: 1 },
            Effect::AnnounceCounterKept { done: 1, total: 2 },
            Effect::AnnounceNoTarget,
            Effect::AnnounceNoPack,
            Effect::AnnounceInterrupted { index: 1 },
            Effect::AnnounceDegraded,
        ];
        let mut markers: Vec<String> = all.iter().map(ToString::to_string).collect();
        let before = markers.len();
        markers.sort_unstable();
        markers.dedup();
        assert_eq!(
            before,
            markers.len(),
            "two effects share a marker: {markers:?}"
        );
        assert!(markers.iter().all(|marker| !marker.is_empty()));
    }
}
