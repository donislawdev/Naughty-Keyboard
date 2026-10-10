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
    /// `done` values have been sent. The next one is number `done + 1`.
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
    /// Values go to the clipboard and the tester pastes them.
    ///
    /// Named after the glossary, never "degraded" or "fallback": `ux-spec.md` 8
    /// calls this a full route rather than a failure, and a name saying
    /// otherwise would say it in every place the mode is mentioned.
    ///
    /// `ux-spec.md` 2: the bar saying so is PERMANENT. Nothing in this module
    /// leaves this mode on its own, and entering it is always an event - the
    /// tester asking for it, or the tool finding no direct route (`D71`).
    /// Leaving it is an event too, and only ever the tester's: [`Event::UseDirect`]
    /// (`D99`).
    ClipboardMode,
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
    /// Values are to go through the clipboard from now on: the tester asked
    /// for it, or the tool learned before any send that this system has no
    /// direct route. Refused while a value is in flight, like a change of pack:
    /// the route must not change under a value that is still going out.
    UseClipboard,
    /// Values are to be typed into the field again - the tester turned clipboard
    /// mode off (`D99`). The mirror of [`Event::UseClipboard`]: refused while a
    /// value is in flight, and it moves nothing in the pack. Whether this system
    /// HAS a direct route is not the sequence's to know, so the caller asks
    /// before it sends this.
    UseDirect,
    /// A value finished arriving in the field.
    InsertionFinished,
    /// Direct delivery has no route at all, so the value did not arrive. This
    /// is the persistent inability (`product-spec.md` 9.3), NOT a passing
    /// hiccup.
    ///
    /// 🔴 In direct mode it switches to the clipboard and asks for the SAME
    /// value again, in the same press - "switching to clipboard mode, press
    /// paste now". The same value, by its index, and not "the next one": after
    /// `Previous` the next one is a different value. Once, by construction: in
    /// clipboard mode this event only returns to where the sequence stood
    /// (`D71`).
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
    /// The tester chose which value the next press sends - the value window
    /// (`UX-GUI-001`). Nothing is sent and nothing is said: the next band shows
    /// the choice. Refused while a value is in flight, like a change of pack. A
    /// number outside the pack changes nothing.
    SetNext { index: usize },
    /// The "skip value" shortcut or the arrow beside the next value (`D120`):
    /// the next value moves one forward and nothing is typed. From the last
    /// value it stops at the end of the pack, and from there it goes to the
    /// first value - the stops a press of "next" makes, each one on screen.
    Skip,
    /// The "back one value" shortcut or the arrow beside the next value
    /// (`D120`): the next value moves one back and nothing is typed. On the
    /// first value it moves nothing and says so.
    Back,
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
    /// Say the next value is the first one already, quoting `total` - a step
    /// back from there moves nothing (`D120`).
    AnnounceAtFirst { total: usize },
    /// Say the target moved while the counter stayed, quoting both numbers.
    AnnounceCounterKept { done: usize, total: usize },
    /// Say there is nowhere to send to.
    AnnounceNoTarget,
    /// Say no pack has been chosen yet.
    AnnounceNoPack,
    /// Say the insertion stopped part-way, and how far it got.
    AnnounceInterrupted { index: usize },
    /// Say values now go to the clipboard, replacing what the tester had
    /// copied. Once, on entering the mode - `ux-spec.md` 12 item 2 warns once
    /// and never restores.
    AnnounceClipboardMode,
    /// Say values are typed into the field again. Once, on leaving the mode.
    AnnounceDirect,
}

impl fmt::Display for Effect {
    /// The developer-facing marker, not the text a person reads. Same shape as
    /// `SourceError`: a stable name a log or a test can match on.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SendValue { .. } => f.write_str("send-value"),
            Self::AnnounceStillInserting => f.write_str("still-inserting"),
            Self::AnnounceEndOfPack { .. } => f.write_str("end-of-pack"),
            Self::AnnounceAtFirst { .. } => f.write_str("at-first"),
            Self::AnnounceCounterKept { .. } => f.write_str("counter-kept"),
            Self::AnnounceNoTarget => f.write_str("no-target"),
            Self::AnnounceNoPack => f.write_str("no-pack"),
            Self::AnnounceInterrupted { .. } => f.write_str("interrupted"),
            Self::AnnounceClipboardMode => f.write_str("clipboard-mode"),
            Self::AnnounceDirect => f.write_str("direct"),
        }
    }
}

/// What the next press of "next value" will do - asked BEFORE it is pressed.
///
/// The palette shows it so the tester does not type blind (`UX-GUI-001`). It is
/// a promise about [`Event::Next`], so it must never disagree with what
/// [`Sequence::apply`] does on that event. A test walks every kind of position
/// and holds the two together.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Upcoming {
    /// The press sends value number `index` (counting from 1) of `total`.
    Value { index: usize, total: usize },
    /// The press sends nothing: it says the pack is finished, and the press
    /// after it starts over - `ux-spec.md` 4 refuses to wrap silently.
    EndOfPack { total: usize },
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

    /// What the next press of "next value" will do, or `None` when it sends
    /// nothing worth announcing: no pack, a pack with no values, or a value
    /// still in flight - the press then only says it is busy (`W1`), and which
    /// value comes after depends on how the one in flight ends.
    #[must_use]
    pub const fn upcoming(&self) -> Option<Upcoming> {
        match self.position {
            Position::NoPack | Position::Inserting { .. } => None,
            Position::Ready { total } => Some(Upcoming::Value { index: 1, total }),
            Position::Running { done, total } => Some(Upcoming::Value {
                index: done + 1,
                total,
            }),
            Position::Exhausted { total: 0, .. } => None,
            Position::Exhausted {
                total,
                warned: false,
            } => Some(Upcoming::EndOfPack { total }),
            // Warned already, so this press starts over.
            Position::Exhausted {
                total,
                warned: true,
            } => Some(Upcoming::Value { index: 1, total }),
        }
    }

    /// Whether a step back ([`Event::Back`]) moves the next value: false on
    /// the first value, where it only says so, and wherever there is nothing
    /// to walk. The palette fades its arrow by this (`D120`), and a test holds
    /// it to what the event really does, so the arrow cannot invite a click
    /// that moves nothing.
    #[must_use]
    pub const fn steps_back(&self) -> bool {
        matches!(
            self.upcoming(),
            Some(Upcoming::EndOfPack { .. } | Upcoming::Value { index: 2.., .. })
        )
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
            Event::SetNext { index } => self.set_next(index),
            Event::Skip => self.skip(),
            Event::Back => self.back(),
            Event::TargetChanged => self.target_changed(),
            Event::TargetLost => self.nothing_but(Effect::AnnounceNoTarget),
            Event::UseClipboard => self.use_clipboard(),
            Event::UseDirect => self.use_direct(),
            // Nothing is in flight, so none of these describes anything.
            // `W3`: `Escape` belongs to `inserting` and is not captured outside
            // it, so arriving here at all means somebody wired it globally. A
            // failure outside a send switched the route until `D71`. Entering the
            // mode outside a send is `UseClipboard` now, with its own name.
            Event::InsertionFinished
            | Event::InsertionFailed
            | Event::InsertionRefused
            | Event::Cancelled => self.nothing(),
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
            // No direct route. The value did not land, so the counter does not
            // move - and the SAME value goes out again through the clipboard in
            // this very press (`D71`). The position stays `Inserting`, because
            // that value is still the one in flight.
            Event::InsertionFailed => match self.delivery {
                Delivery::Direct => Step {
                    sequence: Self {
                        delivery: Delivery::ClipboardMode,
                        ..self
                    },
                    effects: vec![
                        Effect::AnnounceClipboardMode,
                        Effect::SendValue { index: sending },
                    ],
                },
                // The clipboard route reports its own trouble as a refusal, so
                // this should not arrive here. If it does, asking again would be
                // the loop the rule above exists to rule out: go back instead.
                Delivery::ClipboardMode => self.back_before(sending, total),
            },
            // The send never started: no target, a held modifier, a clipboard
            // held by somebody else. Nothing reached the field and the route
            // stays as it was. No effect: the caller says WHY, because the
            // reason is a fact about delivery, not about the sequence.
            Event::InsertionRefused => self.back_before(sending, total),
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
            // Neither the pack, the route nor the place in the pack can be
            // swapped under a value that is still arriving.
            Event::PackChosen { .. }
            | Event::UseClipboard
            | Event::UseDirect
            | Event::SetNext { .. }
            | Event::Skip
            | Event::Back => Step {
                sequence: self,
                effects: vec![Effect::AnnounceStillInserting],
            },
        }
    }

    /// Back to exactly where the sequence stood before the press that started
    /// value `sending`: `Ready` when it was the first, `Running` on the value
    /// already done when it was a later one.
    fn back_before(self, sending: usize, total: usize) -> Step {
        Step {
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

    /// Puts the sequence where the next press sends value `index`.
    ///
    /// Through the same positions a walk would reach, never a new one: `Ready`
    /// for the first value, `Running` on the value before for any other. So
    /// [`Self::upcoming`] and [`Event::Next`] need no case of their own, and the
    /// test that holds them together covers this too.
    fn set_next(self, index: usize) -> Step {
        let total = match self.position {
            Position::NoPack => return self.nothing_but(Effect::AnnounceNoPack),
            Position::Ready { total }
            | Position::Running { total, .. }
            | Position::Exhausted { total, .. } => total,
            Position::Inserting { .. } => {
                return self.nothing_but(Effect::AnnounceStillInserting);
            }
        };
        if index == 0 || index > total {
            return self.nothing();
        }
        let position = if index == 1 {
            Position::Ready { total }
        } else {
            Position::Running {
                done: index - 1,
                total,
            }
        };
        Step {
            sequence: Self { position, ..self },
            effects: Vec::new(),
        }
    }

    /// One value forward without sending anything (`D120`).
    ///
    /// Through the positions a walk reaches and no new one: [`Self::set_next`]
    /// inside the pack, then the end of the pack - unwarned, so a press of
    /// "next" there still says it before it starts over - then the first value.
    /// So [`Self::upcoming`] needs no case of its own here either.
    fn skip(self) -> Step {
        match self.upcoming() {
            None => self.unwalkable(),
            Some(Upcoming::Value { index, total }) if index < total => self.set_next(index + 1),
            Some(Upcoming::Value { total, .. }) => Step {
                sequence: Self {
                    position: Position::Exhausted {
                        total,
                        warned: false,
                    },
                    ..self
                },
                effects: Vec::new(),
            },
            Some(Upcoming::EndOfPack { total }) => Step {
                sequence: Self {
                    position: Position::Ready { total },
                    ..self
                },
                effects: Vec::new(),
            },
        }
    }

    /// One value back without sending anything (`D120`). From the end of the
    /// pack to its last value, and on the first value nowhere - said rather
    /// than silent, because a key that does nothing leaves the tester pressing
    /// it and wondering.
    fn back(self) -> Step {
        match self.upcoming() {
            None => self.unwalkable(),
            Some(Upcoming::Value { index: 1, total }) => {
                self.nothing_but(Effect::AnnounceAtFirst { total })
            }
            Some(Upcoming::Value { index, .. }) => self.set_next(index - 1),
            Some(Upcoming::EndOfPack { total }) => self.set_next(total),
        }
    }

    /// What a step says where there is nothing to walk: no pack, or a pack
    /// with no values. A value in flight never gets here - `W1` answers first.
    fn unwalkable(self) -> Step {
        match self.position {
            Position::NoPack => self.nothing_but(Effect::AnnounceNoPack),
            _ => self.nothing_but(Effect::AnnounceEndOfPack { total: 0 }),
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

    /// Enters clipboard mode, and says so exactly once: a second request finds
    /// the mode already on and has nothing to announce.
    fn use_clipboard(self) -> Step {
        match self.delivery {
            Delivery::ClipboardMode => self.nothing(),
            Delivery::Direct => Step {
                sequence: Self {
                    delivery: Delivery::ClipboardMode,
                    ..self
                },
                effects: vec![Effect::AnnounceClipboardMode],
            },
        }
    }

    /// Leaves clipboard mode, and says so exactly once - the mirror of
    /// [`Self::use_clipboard`]. The place in the pack stays where it is: the
    /// route changes, not which value comes next.
    fn use_direct(self) -> Step {
        match self.delivery {
            Delivery::Direct => self.nothing(),
            Delivery::ClipboardMode => Step {
                sequence: Self {
                    delivery: Delivery::Direct,
                    ..self
                },
                effects: vec![Effect::AnnounceDirect],
            },
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

    /// Every position a sequence can reach, for packs of up to four values -
    /// `Running { done: 0 }` included, because an interrupted first value
    /// leaves exactly that.
    fn every_position() -> Vec<Position> {
        let mut positions = vec![Position::NoPack];
        for total in 0..=4 {
            positions.push(Position::Exhausted {
                total,
                warned: false,
            });
            positions.push(Position::Exhausted {
                total,
                warned: true,
            });
            if total == 0 {
                continue;
            }
            positions.push(Position::Ready { total });
            for done in 0..total {
                positions.push(Position::Running { done, total });
            }
            for sending in 1..=total {
                positions.push(Position::Inserting { sending, total });
            }
        }
        positions
    }

    #[test]
    fn what_next_will_do_is_exactly_what_next_does() {
        // The palette shows `upcoming` before the press (UX-GUI-001). A promise
        // that disagreed with the press would name one value and type another.
        for position in every_position() {
            for delivery in [Delivery::Direct, Delivery::ClipboardMode] {
                let sequence = Sequence { position, delivery };
                let effects = sequence.apply(Event::Next).effects;
                let sends: Vec<usize> = effects
                    .iter()
                    .filter_map(|effect| match effect {
                        Effect::SendValue { index } => Some(*index),
                        _ => None,
                    })
                    .collect();
                match sequence.upcoming() {
                    Some(Upcoming::Value { index, total }) => {
                        assert_eq!(effects, vec![Effect::SendValue { index }], "{position:?}");
                        assert_eq!(sequence.counter().map(|(_, all)| all), Some(total));
                    }
                    Some(Upcoming::EndOfPack { total }) => {
                        assert_eq!(
                            effects,
                            vec![Effect::AnnounceEndOfPack { total }],
                            "{position:?}"
                        );
                    }
                    None => assert!(sends.is_empty(), "{position:?} sends {sends:?} unannounced"),
                }
            }
        }
    }

    #[test]
    fn a_value_chosen_as_next_is_the_one_the_next_press_sends() {
        // The value window (UX-GUI-001) sets the next value. From every position,
        // for every number: in range it becomes the next value - promised and
        // sent - and nothing else moves. In flight, without a pack, or out of
        // range, nothing changes at all.
        for position in every_position() {
            for delivery in [Delivery::Direct, Delivery::ClipboardMode] {
                let sequence = Sequence { position, delivery };
                for index in 0..=5 {
                    let step = sequence.apply(Event::SetNext { index });
                    let total = sequence.counter().map(|(_, all)| all);
                    match position {
                        Position::Inserting { .. } => {
                            assert_eq!(step.sequence, sequence, "{position:?} moved in flight");
                            assert_eq!(step.effects, vec![Effect::AnnounceStillInserting]);
                        }
                        Position::NoPack => {
                            assert_eq!(step.sequence, sequence);
                            assert_eq!(step.effects, vec![Effect::AnnounceNoPack]);
                        }
                        _ if index == 0 || Some(index) > total => {
                            assert_eq!(step.sequence, sequence, "{position:?} took {index}");
                            assert!(step.effects.is_empty());
                        }
                        _ => {
                            assert!(step.effects.is_empty(), "choosing says nothing");
                            assert_eq!(step.sequence.delivery, delivery);
                            let all = total.expect("a pack is chosen");
                            assert_eq!(
                                step.sequence.upcoming(),
                                Some(Upcoming::Value { index, total: all }),
                                "{position:?} -> {index}"
                            );
                            assert_eq!(
                                step.sequence.apply(Event::Next).effects,
                                vec![Effect::SendValue { index }]
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn nothing_is_upcoming_without_a_pack_or_while_a_value_is_in_flight() {
        assert_eq!(Sequence::new().upcoming(), None);
        let flying = Sequence {
            position: Position::Inserting {
                sending: 2,
                total: 3,
            },
            delivery: Delivery::Direct,
        };
        assert_eq!(flying.upcoming(), None);
        let empty = Sequence::new()
            .apply(Event::PackChosen { total: 0 })
            .sequence;
        assert_eq!(empty.upcoming(), None);
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
    fn a_failed_direct_insertion_switches_to_the_clipboard_and_resends_that_value() {
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
        assert_eq!(step.sequence.delivery, Delivery::ClipboardMode);
        assert_eq!(
            step.effects,
            vec![
                Effect::AnnounceClipboardMode,
                Effect::SendValue { index: 2 }
            ],
            "D71: the same value goes out again through the clipboard, in this press"
        );
        assert!(
            step.sequence.is_inserting(),
            "value two is still the one in flight"
        );

        let landed = step.sequence.apply(Event::InsertionFinished).sequence;
        assert_eq!(
            landed.counter(),
            Some((2, 4)),
            "placed on the clipboard counts as sent - ux-spec.md 8"
        );
    }

    #[test]
    fn the_value_resent_after_previous_is_the_one_previous_asked_for() {
        // Resending by "next" would send value four here - a different value
        // from the one that never arrived.
        let running = run(
            Sequence::new(),
            &[
                Event::PackChosen { total: 5 },
                Event::Next,
                Event::InsertionFinished,
                Event::Next,
                Event::InsertionFinished,
                Event::Next,
                Event::InsertionFinished,
            ],
        )
        .sequence;
        let step = running
            .apply(Event::Previous)
            .sequence
            .apply(Event::InsertionFailed);
        assert_eq!(sent_values(&step), [2]);
    }

    #[test]
    fn a_failure_on_the_clipboard_route_goes_back_and_never_asks_again() {
        // The retry is once by construction: a second failure must end the
        // flight, or a route that keeps failing would be asked forever.
        let inserting = run(
            Sequence::new(),
            &[
                Event::PackChosen { total: 3 },
                Event::UseClipboard,
                Event::Next,
            ],
        )
        .sequence;
        let step = inserting.apply(Event::InsertionFailed);
        assert!(sent_values(&step).is_empty(), "no second retry");
        assert!(step.effects.is_empty());
        assert_eq!(step.sequence.position, Position::Ready { total: 3 });
        assert_eq!(step.sequence.delivery, Delivery::ClipboardMode);
    }

    #[test]
    fn delivery_is_a_second_axis_and_the_clipboard_does_not_disturb_the_counter() {
        // The whole reason clipboard mode is not a sixth position: a tester in
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
        let step = sequence.apply(Event::UseClipboard);
        assert_eq!(step.effects, vec![Effect::AnnounceClipboardMode]);
        let by_clipboard = step.sequence;

        assert_eq!(
            by_clipboard.counter(),
            Some((2, 9)),
            "the counter survives the change of route"
        );
        assert_eq!(by_clipboard.delivery, Delivery::ClipboardMode);

        let after = run(by_clipboard, &[Event::Next, Event::InsertionFinished]).sequence;
        assert_eq!(
            after.delivery,
            Delivery::ClipboardMode,
            "nothing here leaves clipboard mode on its own"
        );
        assert_eq!(after.counter(), Some((3, 9)));
    }

    #[test]
    fn entering_clipboard_mode_twice_announces_it_once() {
        // The announcement warns that the tester's clipboard is replaced, and
        // `ux-spec.md` 12 item 2 says to warn ONCE.
        let once = run(
            Sequence::new(),
            &[Event::PackChosen { total: 3 }, Event::UseClipboard],
        )
        .sequence;
        let twice = once.apply(Event::UseClipboard);
        assert!(twice.effects.is_empty());
        assert_eq!(twice.sequence, once);
    }

    #[test]
    fn the_route_cannot_change_under_a_value_in_flight() {
        let inserting = run(
            Sequence::new(),
            &[Event::PackChosen { total: 3 }, Event::Next],
        )
        .sequence;
        let step = inserting.apply(Event::UseClipboard);
        assert_eq!(step.effects, vec![Effect::AnnounceStillInserting]);
        assert_eq!(step.sequence, inserting);
    }

    #[test]
    fn turning_clipboard_mode_off_changes_the_route_and_nothing_else() {
        // D99, the tester's way out of the mode. From every position the pack
        // stays exactly where it was - the next press sends the value it would
        // have sent - and only the route changes. In flight it is refused aloud,
        // and outside the mode there is nothing to leave and nothing to say.
        for position in every_position() {
            let on = Sequence {
                position,
                delivery: Delivery::ClipboardMode,
            };
            let step = on.apply(Event::UseDirect);
            if let Position::Inserting { .. } = position {
                assert_eq!(step.sequence, on, "{position:?} changed route in flight");
                assert_eq!(step.effects, vec![Effect::AnnounceStillInserting]);
                continue;
            }
            assert_eq!(step.effects, vec![Effect::AnnounceDirect], "{position:?}");
            assert_eq!(
                step.sequence,
                Sequence {
                    position,
                    delivery: Delivery::Direct
                },
                "{position:?}"
            );
            assert_eq!(step.sequence.upcoming(), on.upcoming(), "{position:?}");

            let off = Sequence {
                position,
                delivery: Delivery::Direct,
            };
            let again = off.apply(Event::UseDirect);
            assert_eq!(again.sequence, off, "{position:?}");
            assert!(
                again.effects.is_empty(),
                "{position:?} announced leaving a mode it was not in"
            );
        }
    }

    #[test]
    fn the_mode_can_be_turned_on_and_off_again_and_each_turn_is_said() {
        // Each switch is the tester's own click, so each one is answered - the
        // warning that the clipboard is replaced included, because a tester who
        // turned the mode off and on again is about to lose what they copied in
        // between.
        let ready = run(Sequence::new(), &[Event::PackChosen { total: 3 }]).sequence;
        let on = ready.apply(Event::UseClipboard);
        let off = on.sequence.apply(Event::UseDirect);
        let again = off.sequence.apply(Event::UseClipboard);
        assert_eq!(on.effects, vec![Effect::AnnounceClipboardMode]);
        assert_eq!(off.effects, vec![Effect::AnnounceDirect]);
        assert_eq!(off.sequence, ready);
        assert_eq!(again.effects, vec![Effect::AnnounceClipboardMode]);
        assert_eq!(again.sequence, on.sequence);
    }

    #[test]
    fn a_failure_outside_an_insertion_does_nothing() {
        // Entering the mode outside a send is `UseClipboard`, with its own name.
        let ready = run(Sequence::new(), &[Event::PackChosen { total: 3 }]).sequence;
        let step = ready.apply(Event::InsertionFailed);
        assert_eq!(step.sequence, ready);
        assert!(step.effects.is_empty());
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
        assert_eq!(failed.delivery, Delivery::ClipboardMode);
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
            "product-spec.md 8.2 chose a global counter. Changing field does not reset it"
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
        // The validator refuses such a pack. This layer does not get to assume
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
            "restarting sets a position. It does not fire a value at the field"
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

    // ---- walking a pack without typing (`D120`) ---------------------------

    #[test]
    fn a_step_never_sends_and_never_changes_the_route() {
        // From every position and on both routes: a skip or a step back moves
        // at most which value comes next. In flight it is refused aloud, as a
        // change of pack is (`W1`).
        for position in every_position() {
            for delivery in [Delivery::Direct, Delivery::ClipboardMode] {
                let sequence = Sequence { position, delivery };
                for event in [Event::Skip, Event::Back] {
                    let step = sequence.apply(event);
                    assert!(sent_values(&step).is_empty(), "{position:?} {event:?} sent");
                    assert_eq!(step.sequence.delivery, delivery);
                    if let Position::Inserting { .. } = position {
                        assert_eq!(step.sequence, sequence, "{position:?} moved in flight");
                        assert_eq!(step.effects, vec![Effect::AnnounceStillInserting]);
                    }
                    if position == Position::NoPack {
                        assert_eq!(step.effects, vec![Effect::AnnounceNoPack]);
                    }
                }
            }
        }
    }

    #[test]
    fn skipping_walks_every_value_stops_at_the_end_then_starts_over() {
        let mut sequence = run(Sequence::new(), &[Event::PackChosen { total: 3 }]).sequence;
        let mut seen = Vec::new();
        for _ in 0..5 {
            seen.push(sequence.upcoming());
            let step = sequence.apply(Event::Skip);
            assert!(
                step.effects.is_empty(),
                "a skip says nothing: {:?}",
                step.effects
            );
            sequence = step.sequence;
        }
        assert_eq!(
            seen,
            vec![
                Some(Upcoming::Value { index: 1, total: 3 }),
                Some(Upcoming::Value { index: 2, total: 3 }),
                Some(Upcoming::Value { index: 3, total: 3 }),
                Some(Upcoming::EndOfPack { total: 3 }),
                Some(Upcoming::Value { index: 1, total: 3 }),
            ]
        );
    }

    #[test]
    fn a_press_after_a_skip_sends_the_value_the_band_showed() {
        // The band is a promise about the press (`UX-GUI-001`), and a skip
        // must keep it: after skipping to value 3, "next" sends value 3. At
        // the end of the pack it warns first, as it does after the last send.
        let at_three = run(
            Sequence::new(),
            &[Event::PackChosen { total: 3 }, Event::Skip, Event::Skip],
        );
        assert_eq!(sent_values(&at_three.sequence.apply(Event::Next)), vec![3]);
        let at_end = run(at_three.sequence, &[Event::Skip]);
        assert_eq!(
            at_end.sequence.apply(Event::Next).effects,
            vec![Effect::AnnounceEndOfPack { total: 3 }]
        );
    }

    #[test]
    fn a_step_back_goes_from_the_end_to_the_first_value_and_no_further() {
        let mut sequence = run(
            Sequence::new(),
            &[
                Event::PackChosen { total: 3 },
                Event::Skip,
                Event::Skip,
                Event::Skip,
            ],
        )
        .sequence;
        let mut seen = Vec::new();
        for _ in 0..3 {
            let step = sequence.apply(Event::Back);
            assert!(step.effects.is_empty(), "{:?}", step.effects);
            sequence = step.sequence;
            seen.push(sequence.upcoming());
        }
        assert_eq!(
            seen,
            vec![
                Some(Upcoming::Value { index: 3, total: 3 }),
                Some(Upcoming::Value { index: 2, total: 3 }),
                Some(Upcoming::Value { index: 1, total: 3 }),
            ]
        );
        let first = sequence.apply(Event::Back);
        assert_eq!(
            first.sequence, sequence,
            "the first value is as far back as it goes"
        );
        assert_eq!(first.effects, vec![Effect::AnnounceAtFirst { total: 3 }]);
    }

    #[test]
    fn steps_back_says_exactly_whether_a_step_back_moves() {
        for position in every_position() {
            for delivery in [Delivery::Direct, Delivery::ClipboardMode] {
                let sequence = Sequence { position, delivery };
                let moved = sequence.apply(Event::Back).sequence != sequence;
                assert_eq!(sequence.steps_back(), moved, "{position:?}");
            }
        }
    }

    #[test]
    fn a_pack_of_one_value_walks_between_it_and_the_end() {
        let one = run(Sequence::new(), &[Event::PackChosen { total: 1 }]).sequence;
        let end = one.apply(Event::Skip).sequence;
        assert_eq!(end.upcoming(), Some(Upcoming::EndOfPack { total: 1 }));
        assert_eq!(end.apply(Event::Back).sequence.upcoming(), one.upcoming());
        assert_eq!(end.apply(Event::Skip).sequence.upcoming(), one.upcoming());
    }

    #[test]
    fn a_pack_with_no_values_cannot_be_walked_and_says_so() {
        let empty = run(Sequence::new(), &[Event::PackChosen { total: 0 }]).sequence;
        for event in [Event::Skip, Event::Back] {
            let step = empty.apply(event);
            assert_eq!(step.sequence, empty);
            assert_eq!(step.effects, vec![Effect::AnnounceEndOfPack { total: 0 }]);
        }
    }

    #[test]
    fn every_effect_has_a_stable_marker_and_no_two_share_one() {
        // The markers are what a log and a test match on, so a duplicate would
        // make two different things indistinguishable after the fact.
        let all = [
            Effect::SendValue { index: 1 },
            Effect::AnnounceStillInserting,
            Effect::AnnounceEndOfPack { total: 1 },
            Effect::AnnounceAtFirst { total: 1 },
            Effect::AnnounceCounterKept { done: 1, total: 2 },
            Effect::AnnounceNoTarget,
            Effect::AnnounceNoPack,
            Effect::AnnounceInterrupted { index: 1 },
            Effect::AnnounceClipboardMode,
            Effect::AnnounceDirect,
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
