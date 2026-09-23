//! The palette's core loop: a shortcut moves the tester through a pack.
//!
//! This is the use case behind "next value", and it holds the one piece of
//! mutable state the whole product has - the [`Sequence`] - which
//! `architektura.md` 6a puts in `app`, behind one gate, and nowhere else. The
//! sequence machine in the core decides WHAT a keystroke means; this decides
//! what to DO about it: load the pack once, deliver a value from memory, and
//! feed the result back to the machine.
//!
//! # Loaded once, delivered from memory - `W5`
//!
//! The pack is read and parsed a single time, when it is chosen, and every send
//! comes from that in-memory copy through [`deliver_value`]. `send_value`, which
//! re-reads the file each time, is right for the one-shot `nkb send` and wrong
//! here: `W5` requires the value shown to the tester to be the very value that
//! was sent, and a re-read between the two could differ if the file changed
//! underneath. The sequence never sees the pack - it holds only a count - so a
//! file changing on disk cannot redefine what `7/34` means mid-walk.
//!
//! # Bridging four delivery outcomes onto the machine
//!
//! The sequence machine models four ends to a send, and this is where a real
//! [`SendOutcome`] is sorted onto them:
//!
//! | delivery outcome | event | why |
//! |---|---|---|
//! | the value landed | `InsertionFinished` | it is done |
//! | no route on this system | `InsertionFailed` | direct delivery cannot work; clipboard now |
//! | part of it landed | `Cancelled` | a fragment was left in the field |
//! | nothing was sent (no target, held modifier, clear failed) | `InsertionRefused` | try again, and do NOT switch to the clipboard for a passing hiccup |
//!
//! The last row is why `InsertionRefused` exists: folding it into
//! `InsertionFailed` would turn a held Ctrl+Alt into a permanent clipboard mode.
//! The machine returns the STATE for each; the MESSAGE that says which and with
//! what numbers is built here, because the numbers are facts about delivery.
//!
//! # What this deliberately does not do yet
//!
//! - only `Next`, `Previous` and `Restart` are wired. Repeat, the result marks,
//!   the report copy, the pack search and show/hide belong to later steps; an
//!   action that is not wired says so rather than doing nothing in silence;
//! - the send is atomic - one blocking [`deliver_value`] - so `Inserting` is
//!   passed straight through. The progress bar, `Escape` mid-send and the
//!   char-by-char mode (`W2`/`W3`) are the char-by-char delivery, still to come;
//! - it does not watch the target for a change mid-send (`TargetChanged`),
//!   because that needs `TargetInspector`, which does not exist yet. It checks
//!   the target ONCE before each send instead, which is what stops a value from
//!   going into the window the palette was launched from.

use nkb_core::hotkeys::HotkeyAction;
use nkb_core::pack::{Pack, Risk};
use nkb_core::preview::{ShapeFact, ValuePreview};
use nkb_core::sequence::{Effect, Event, Sequence};

use crate::load_pack;
use crate::ports::{
    DeliveryError, KeystrokeError, KeystrokeSender, PackFormat, PackSource, SourceError,
    ValueDelivery,
};
use crate::send_value::{Clearing, SendOutcome, deliver_value};

/// The core loop and its one piece of state.
#[derive(Debug, Default)]
pub struct AdvanceSequence {
    sequence: Sequence,
    loaded: Option<Loaded>,
}

/// A pack held in memory for the length of the sequence.
#[derive(Debug)]
struct Loaded {
    pack: Pack,
    /// Warnings the pack carried. It loaded anyway - warnings never block - but
    /// a send should not hide them, so they travel out with each `Sent`.
    warnings: usize,
}

/// Why a pack could not be chosen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChooseError {
    /// No pack of that name.
    NotFound,
    /// Present but unreadable, or not valid UTF-8.
    Unreadable,
    /// The pack has errors, so it loads not at all - `pack-format.md` 11.
    Refused { errors: usize },
}

/// What the palette shows after one action. Holds no toolkit and no sentence -
/// the sequence for the counter, the value that went out, and message KEYS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub sequence: Sequence,
    /// The value that just reached the field, when one did.
    pub sent: Option<Sent>,
    /// What the palette must say, in order. Keys and numbers, never words.
    pub messages: Vec<Message>,
    /// Whether this action went as far as the field - the machine asked for a
    /// send and the send was attempted, landed or not. A caller draining
    /// presses that queued in the meantime keys on this: presses queued behind
    /// an attempt were pressed while the tool was busy (`W1`); presses queued
    /// behind an instant answer, such as an end-of-pack warning, were not.
    pub attempted_send: bool,
}

/// What the palette shows about the value that just went out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sent {
    /// `pack-id/value-id`, the same shape `emit` prints.
    pub reference: String,
    pub name: String,
    /// Clusters, as a person sees them - the counter `ux-spec.md` 2 puts first.
    pub graphemes: usize,
    pub code_points: usize,
    pub bytes: usize,
    /// What actually crossed the wire. Differs from `code_points` exactly when
    /// the value holds characters above the basic plane.
    pub utf16_units: usize,
    /// Whether the value is offensive - the effective risk, the value's own if
    /// it declares one and otherwise the pack's. The palette marks it before a
    /// tester can wonder, `product-spec.md` 10.2.
    pub offensive: bool,
    pub warnings: usize,
    /// Whether the field was cleared before the value went in.
    pub cleared: bool,
    /// The value as a person can see it: invisible characters substituted by
    /// `nkb_core::preview::MARKER`, long values elided at both ends, generated
    /// values shown as their recipe.
    ///
    /// 🔴 The palette must say the total when a text preview's `elided_total`
    /// is `Some`. A preview that silently showed a fragment would be the tool
    /// claiming to answer "what did I send" while hiding most of it.
    pub preview: ValuePreview,
    /// What the value is made of. Facts, in a fixed order that puts what changes
    /// how the field READS above what changes where the text sits.
    pub shape: Vec<ShapeFact>,
}

/// A thing the palette must say. A key with its numbers - untouchable rule 9
/// keeps the words one layer further out, in the text module the GUI owns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    /// The pack is finished. The next press starts over.
    EndOfPack { total: usize },
    /// The target moved but the counter did not, quoting both numbers.
    CounterKept { done: usize, total: usize },
    /// No pack has been chosen yet.
    NoPack,
    /// Nothing holds the keyboard focus, so nothing was sent.
    NoTarget,
    /// A modifier is physically held, so nothing was sent - release it and try
    /// again. Names which one.
    ModifierHeld { key: String },
    /// The field could not be cleared, so the value was not sent. The field is
    /// in an unknown state between its old content and empty.
    ClearingFailed,
    /// The value was left half-written in the field: how far it got, of how much.
    Interrupted {
        units_sent: usize,
        units_expected: usize,
    },
    /// Direct delivery does not work here; the clipboard is the way now.
    Degraded,
    /// The recipe describes more text than the format allows. Should not reach
    /// here for a validated pack, and said plainly rather than swallowed.
    ValueTooLarge { id: String },
    /// The shortcut is not wired in this build. Said, never ignored.
    Unhandled { action: HotkeyAction },
    /// The shortcut arrived while the previous one was still being handled -
    /// a send in flight, or the wait for a held modifier - and was dropped, not
    /// queued. `W1`: replaying it afterwards would be exactly the queueing that
    /// `ux-spec.md` 3 rejects, and dropping it in silence would break rule 1.
    PressedWhileBusy { action: HotkeyAction },
}

impl AdvanceSequence {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The counter the palette shows, `n / total`, or `None` before a pack.
    #[must_use]
    pub fn counter(&self) -> Option<(usize, usize)> {
        self.sequence.counter()
    }

    /// The chosen pack's display name, if a pack is chosen.
    #[must_use]
    pub fn pack_name(&self) -> Option<&str> {
        self.loaded.as_ref().map(|loaded| loaded.pack.name.as_str())
    }

    /// The current sequence, for a caller that renders both of its axes.
    #[must_use]
    pub fn sequence(&self) -> Sequence {
        self.sequence
    }

    /// Reads, validates and chooses a pack, moving the sequence to `ready`.
    ///
    /// Called once at startup, since the pack search that would call it again is
    /// a later step. `check` runs first and the pack is refused on any error -
    /// `pack-format.md` 11 loads all of a pack or none of it - which is what
    /// `load_pack::load` does.
    ///
    /// # Errors
    ///
    /// [`ChooseError`] when the pack is missing, unreadable, or has errors.
    pub fn choose_pack(
        &mut self,
        source: &dyn PackSource,
        format: &dyn PackFormat,
        pack_id: &str,
    ) -> Result<(), ChooseError> {
        let text = match source.read(pack_id) {
            Ok(text) => text,
            Err(SourceError::NotFound) => return Err(ChooseError::NotFound),
            Err(SourceError::Unreadable | SourceError::NotUtf8) => {
                return Err(ChooseError::Unreadable);
            }
        };
        let loaded =
            load_pack::load(format, pack_id, &text).map_err(|refused| ChooseError::Refused {
                errors: refused.errors,
            })?;
        let total = loaded.pack.values.len();
        self.loaded = Some(Loaded {
            pack: loaded.pack,
            warnings: loaded.warnings,
        });
        self.sequence = self.sequence.apply(Event::PackChosen { total }).sequence;
        Ok(())
    }

    /// Applies one shortcut and, if it means sending, does the send.
    ///
    /// The target is checked ONCE before a send: nothing focused means the value
    /// would go into the window the palette was launched from, so the send is
    /// refused before a single key leaves.
    #[must_use]
    pub fn on_action(
        &mut self,
        action: HotkeyAction,
        delivery: &dyn ValueDelivery,
        keys: &dyn KeystrokeSender,
    ) -> Outcome {
        let event = match action {
            HotkeyAction::NextValue => Event::Next,
            HotkeyAction::PreviousValue => Event::Previous,
            HotkeyAction::RestartPack => Event::Restart,
            other => {
                return Outcome {
                    sequence: self.sequence,
                    sent: None,
                    messages: vec![Message::Unhandled { action: other }],
                    attempted_send: false,
                };
            }
        };

        let step = self.sequence.apply(event);
        let Some(index) = send_index(&step) else {
            // Nothing to send - a warning, an end-of-pack, a no-op. The state
            // moves and the machine's own effects become the messages.
            self.sequence = step.sequence;
            return Outcome {
                sequence: self.sequence,
                sent: None,
                messages: step.effects.iter().filter_map(announce).collect(),
                attempted_send: false,
            };
        };

        // The machine has put us in `Inserting` and asked for value `index`.
        self.sequence = step.sequence;
        self.send(index, delivery, keys)
    }

    /// Delivers value `index` from the held pack and settles the sequence.
    fn send(
        &mut self,
        index: usize,
        delivery: &dyn ValueDelivery,
        keys: &dyn KeystrokeSender,
    ) -> Outcome {
        // A SendValue effect only comes from a chosen pack, so this is present.
        // If it somehow is not, say so rather than reach into a `None`.
        let Some(loaded) = self.loaded.as_ref() else {
            self.sequence = self.sequence.apply(Event::InsertionRefused).sequence;
            return self.settled(None, vec![Message::NoPack], false);
        };

        // Refuse before touching the field if nothing is focused: the value
        // would otherwise land in the launching window. Measured as real in the
        // step-2 probe, not hypothetical.
        if delivery.target().is_none() {
            self.sequence = self.sequence.apply(Event::InsertionRefused).sequence;
            return self.settled(None, vec![Message::NoTarget], false);
        }

        let Some(value) = loaded.pack.values.get(index - 1) else {
            // The machine asked for an index the pack does not hold, which would
            // be a machine/pack disagreement rather than a delivery problem.
            self.sequence = self.sequence.apply(Event::InsertionRefused).sequence;
            return self.settled(None, Vec::new(), false);
        };
        let offensive = value.risk.unwrap_or(loaded.pack.risk) == Risk::Offensive;
        let outcome = deliver_value(
            &loaded.pack,
            value,
            delivery,
            keys,
            Clearing::Line,
            loaded.warnings,
        );

        let (event, sent, messages) = classify(outcome, offensive);
        self.sequence = self.sequence.apply(event).sequence;
        self.settled(sent, messages, true)
    }

    /// Wraps the current state with what the send produced.
    ///
    /// `attempted` is true only when [`deliver_value`] actually ran. The
    /// refusals before it (no pack, no target, an index the pack does not hold)
    /// answer instantly, so a press queued behind them was not pressed while
    /// the tool was busy and must not be reported as if it were.
    fn settled(&self, sent: Option<Sent>, messages: Vec<Message>, attempted: bool) -> Outcome {
        Outcome {
            sequence: self.sequence,
            sent,
            messages,
            attempted_send: attempted,
        }
    }
}

/// The `index` a step asks to send, if it asks at all.
fn send_index(step: &nkb_core::sequence::Step) -> Option<usize> {
    step.effects.iter().find_map(|effect| match effect {
        Effect::SendValue { index } => Some(*index),
        _ => None,
    })
}

/// The message a machine effect stands for, for the effects that reach a caller
/// outside a send. `SendValue` is handled by delivering, not by a message, and
/// the announce effects that only appear during a send are supplied from the
/// delivery outcome instead.
fn announce(effect: &Effect) -> Option<Message> {
    match effect {
        Effect::AnnounceEndOfPack { total } => Some(Message::EndOfPack { total: *total }),
        Effect::AnnounceCounterKept { done, total } => Some(Message::CounterKept {
            done: *done,
            total: *total,
        }),
        Effect::AnnounceNoPack => Some(Message::NoPack),
        Effect::AnnounceNoTarget => Some(Message::NoTarget),
        // These arise only while sending, and the send path builds a richer
        // message from the delivery outcome. Seeing one here would mean a send
        // effect leaked onto the no-send path, so it is dropped rather than
        // shown half-formed.
        Effect::SendValue { .. }
        | Effect::AnnounceStillInserting
        | Effect::AnnounceInterrupted { .. }
        | Effect::AnnounceDegraded => None,
    }
}

/// Sorts a delivery outcome onto a sequence event and the message that goes with
/// it. `offensive` rides along so a landed value can be marked.
fn classify(outcome: SendOutcome, offensive: bool) -> (Event, Option<Sent>, Vec<Message>) {
    match outcome {
        SendOutcome::Sent {
            reference,
            name,
            graphemes,
            code_points,
            bytes,
            utf16_units,
            warnings,
            cleared,
            preview,
            shape,
        } => (
            Event::InsertionFinished,
            Some(Sent {
                reference,
                name,
                graphemes,
                code_points,
                bytes,
                utf16_units,
                offensive,
                warnings,
                cleared,
                preview,
                shape,
            }),
            Vec::new(),
        ),
        // No route at all: the one outcome that degrades to the clipboard.
        SendOutcome::NotDelivered {
            error: DeliveryError::Unsupported { .. },
            ..
        } => (Event::InsertionFailed, None, vec![Message::Degraded]),
        // A fragment was left: interrupted, with how far it got.
        SendOutcome::NotDelivered {
            error:
                DeliveryError::Partial {
                    units_sent,
                    units_expected,
                },
            ..
        } => (
            Event::Cancelled,
            None,
            vec![Message::Interrupted {
                units_sent,
                units_expected,
            }],
        ),
        // Nothing was sent - the field is untouched. Refuse, do NOT degrade.
        SendOutcome::NotDelivered {
            error: DeliveryError::NoTarget,
            ..
        } => (Event::InsertionRefused, None, vec![Message::NoTarget]),
        SendOutcome::NotDelivered {
            error: DeliveryError::ModifierHeld { which },
            ..
        } => (
            Event::InsertionRefused,
            None,
            vec![Message::ModifierHeld { key: which }],
        ),
        // Clearing failed, so the value never went. The reason names the message.
        SendOutcome::NotCleared { error } => {
            (Event::InsertionRefused, None, vec![clearing_message(error)])
        }
        // A validated pack cannot describe a value this large, but if one did,
        // nothing was sent and it is said plainly.
        SendOutcome::ValueTooLarge { id, .. } => (
            Event::InsertionRefused,
            None,
            vec![Message::ValueTooLarge { id }],
        ),
        // deliver_value returns only the four families above. The load-path
        // outcomes cannot arise from an already-loaded value; if one somehow
        // did, nothing is known to have been sent, so refuse and stay quiet
        // rather than invent a message.
        SendOutcome::NotFound
        | SendOutcome::Unreadable
        | SendOutcome::Refused { .. }
        | SendOutcome::NoSuchIndex { .. }
        | SendOutcome::RouteUnavailable { .. } => (Event::InsertionRefused, None, Vec::new()),
    }
}

/// The message for a clearing that did not go through. Nothing was sent either
/// way; the reason tells the tester what to do.
fn clearing_message(error: KeystrokeError) -> Message {
    match error {
        KeystrokeError::NoTarget => Message::NoTarget,
        KeystrokeError::ModifierHeld { which } => Message::ModifierHeld { key: which },
        // No route to press keys, or a partial clear leaving the field in an
        // unknown state: both mean the field cannot be trusted to be clear.
        KeystrokeError::Unsupported { .. } | KeystrokeError::Partial { .. } => {
            Message::ClearingFailed
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
    use crate::test_support::*;
    use nkb_core::sequence::{Delivery, Position};

    #[test]
    fn choosing_a_pack_moves_to_ready_with_the_right_total() {
        let advance = chosen(Risk::Normal);
        assert_eq!(advance.counter(), Some((0, 3)));
        assert_eq!(advance.pack_name(), Some("Sample pack"));
    }

    #[test]
    fn the_first_next_sends_value_one_and_advances_the_counter() {
        let mut advance = chosen(Risk::Normal);
        let outcome = advance.on_action(
            HotkeyAction::NextValue,
            &FakeDelivery::ready(),
            &FakeKeys::working(),
        );
        let sent = outcome.sent.expect("value one must have been sent");
        assert_eq!(sent.reference, "sample/one");
        assert_eq!(sent.name, "Value one");
        assert!(sent.cleared, "the palette clears by default");
        assert_eq!(advance.counter(), Some((1, 3)));
        assert!(outcome.messages.is_empty());
    }

    #[test]
    fn walking_the_whole_pack_ends_exhausted() {
        let mut advance = chosen(Risk::Normal);
        for _ in 0..3 {
            let _ = advance.on_action(
                HotkeyAction::NextValue,
                &FakeDelivery::ready(),
                &FakeKeys::working(),
            );
        }
        assert_eq!(advance.counter(), Some((3, 3)));
        let outcome = advance.on_action(
            HotkeyAction::NextValue,
            &FakeDelivery::ready(),
            &FakeKeys::working(),
        );
        assert_eq!(outcome.messages, vec![Message::EndOfPack { total: 3 }]);
        assert!(outcome.sent.is_none());
    }

    #[test]
    fn no_target_refuses_the_send_without_degrading() {
        let mut advance = chosen(Risk::Normal);
        let outcome = advance.on_action(
            HotkeyAction::NextValue,
            &FakeDelivery::without_target(),
            &FakeKeys::working(),
        );
        assert_eq!(outcome.messages, vec![Message::NoTarget]);
        assert!(outcome.sent.is_none());
        assert_eq!(advance.sequence().position, Position::Ready { total: 3 });
        assert_eq!(advance.sequence().delivery, Delivery::Direct);
    }

    #[test]
    fn no_route_degrades_to_the_clipboard() {
        let mut advance = chosen(Risk::Normal);
        let outcome = advance.on_action(
            HotkeyAction::NextValue,
            &FakeDelivery::failing(DeliveryError::Unsupported {
                system: "macOS".to_owned(),
            }),
            &FakeKeys::working(),
        );
        assert_eq!(outcome.messages, vec![Message::Degraded]);
        assert_eq!(advance.sequence().delivery, Delivery::Degraded);
        assert_eq!(advance.counter(), Some((0, 3)));
    }

    #[test]
    fn a_partial_send_reports_a_fragment_and_does_not_degrade() {
        let mut advance = chosen(Risk::Normal);
        let outcome = advance.on_action(
            HotkeyAction::NextValue,
            &FakeDelivery::failing(DeliveryError::Partial {
                units_sent: 3,
                units_expected: 5,
            }),
            &FakeKeys::working(),
        );
        assert_eq!(
            outcome.messages,
            vec![Message::Interrupted {
                units_sent: 3,
                units_expected: 5
            }]
        );
        assert_eq!(advance.sequence().delivery, Delivery::Direct);
    }

    #[test]
    fn a_held_modifier_refuses_and_names_it_without_degrading() {
        let mut advance = chosen(Risk::Normal);
        let outcome = advance.on_action(
            HotkeyAction::NextValue,
            &FakeDelivery::failing(DeliveryError::ModifierHeld {
                which: "Ctrl".to_owned(),
            }),
            &FakeKeys::working(),
        );
        assert_eq!(
            outcome.messages,
            vec![Message::ModifierHeld {
                key: "Ctrl".to_owned()
            }]
        );
        assert_eq!(advance.sequence().delivery, Delivery::Direct);
        assert_eq!(advance.sequence().position, Position::Ready { total: 3 });
    }

    #[test]
    fn a_failure_to_clear_refuses_the_value() {
        let mut advance = chosen(Risk::Normal);
        let outcome = advance.on_action(
            HotkeyAction::NextValue,
            &FakeDelivery::ready(),
            &FakeKeys::failing(KeystrokeError::ModifierHeld {
                which: "Alt".to_owned(),
            }),
        );
        assert_eq!(
            outcome.messages,
            vec![Message::ModifierHeld {
                key: "Alt".to_owned()
            }]
        );
        assert!(outcome.sent.is_none());
        assert_eq!(advance.sequence().delivery, Delivery::Direct);
    }

    #[test]
    fn an_unwired_action_says_so_rather_than_doing_nothing() {
        let mut advance = chosen(Risk::Normal);
        let outcome = advance.on_action(
            HotkeyAction::CopyReport,
            &FakeDelivery::ready(),
            &FakeKeys::working(),
        );
        assert_eq!(
            outcome.messages,
            vec![Message::Unhandled {
                action: HotkeyAction::CopyReport
            }]
        );
        assert_eq!(advance.counter(), Some((0, 3)));
    }

    #[test]
    fn next_with_no_pack_chosen_says_no_pack() {
        let mut advance = AdvanceSequence::new();
        let outcome = advance.on_action(
            HotkeyAction::NextValue,
            &FakeDelivery::ready(),
            &FakeKeys::working(),
        );
        assert_eq!(outcome.messages, vec![Message::NoPack]);
        assert!(outcome.sent.is_none());
    }

    #[test]
    fn a_missing_pack_is_reported_not_panicked() {
        let mut advance = AdvanceSequence::new();
        let result = advance.choose_pack(&MissingSource, &FakeFormat::with_errors(0), "absent");
        assert_eq!(result, Err(ChooseError::NotFound));
    }

    #[test]
    fn a_pack_with_errors_is_refused_and_says_how_many() {
        let mut advance = AdvanceSequence::new();
        let result = advance.choose_pack(&FakeSource, &FakeFormat::with_errors(3), "sample");
        assert_eq!(result, Err(ChooseError::Refused { errors: 3 }));
    }

    #[test]
    fn an_offensive_value_is_marked_from_the_pack_default() {
        let mut advance = chosen(Risk::Offensive);
        let outcome = advance.on_action(
            HotkeyAction::NextValue,
            &FakeDelivery::ready(),
            &FakeKeys::working(),
        );
        let sent = outcome.sent.expect("sent");
        assert!(sent.offensive, "an offensive pack marks its values");
    }

    #[test]
    fn previous_at_the_start_keeps_the_counter_and_says_so() {
        let mut advance = chosen(Risk::Normal);
        let outcome = advance.on_action(
            HotkeyAction::PreviousValue,
            &FakeDelivery::ready(),
            &FakeKeys::working(),
        );
        assert_eq!(
            outcome.messages,
            vec![Message::CounterKept { done: 0, total: 3 }]
        );
        assert!(outcome.sent.is_none());
    }
}
