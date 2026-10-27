//! What a delivery outcome means to the sequence and to the tester: the event the
//! machine is given and the sentences the palette says. Apart from the loop in
//! `advance_sequence.rs`, which stood at the ceiling of the size of a file (`D113`).

use nkb_core::report::Arrival;
use nkb_core::sequence::Event;
use nkb_core::substitutions::Substitutions;

use super::{Message, Sent, ValueKey, arrival_of};
use crate::ports::{DeliveryError, Following, KeystrokeError};
use crate::send_value::{ClearingOutcome, SendOutcome, SkipReason};

/// Sorts a delivery outcome onto a sequence event and the message that goes with
/// it. `offensive` rides along so a landed value can be marked, `on_clipboard`
/// so it can say which route it took, and `key` so a refusal can name it and
/// a value that went out carries the identifiers a Copy button needs (`D98`).
pub(super) fn classify(
    outcome: SendOutcome,
    offensive: bool,
    on_clipboard: bool,
    key: &ValueKey,
) -> (Event, Option<Sent>, Vec<Message>) {
    // The one answer to "how did it get there", shared with the report block.
    // `Some` for both outcomes that carry a value, so the fallbacks below name
    // the case it already gave and are never what the palette shows.
    let arrival = arrival_of(&outcome, on_clipboard);
    match outcome {
        SendOutcome::Sent {
            facts,
            utf16_units,
            clearing,
            following,
            may_alter,
        } => (
            Event::InsertionFinished,
            Some(Sent {
                key: key.clone(),
                facts,
                utf16_units,
                offensive,
                cleared: clearing == ClearingOutcome::Done,
                arrival: arrival.unwrap_or(Arrival::Whole),
                // Filled by `attempt`, which read it at the press - and the
                // block, which it builds once the value is the last one.
                target: None,
                report: None,
            }),
            notes_after_landing(following, may_alter, clearing),
        ),
        // A fragment was left: interrupted, with how far it got - and WHICH
        // value, so the palette shows the value the report block describes
        // rather than the one before it (`OBS-126`). The interruption is said
        // first, because it is the worse news.
        SendOutcome::Interrupted {
            facts,
            units_sent,
            units_expected,
            clearing,
            reason,
        } => (
            Event::Cancelled,
            Some(Sent {
                key: key.clone(),
                facts,
                utf16_units: units_expected,
                offensive,
                cleared: clearing == ClearingOutcome::Done,
                arrival: arrival.unwrap_or(Arrival::Interrupted {
                    units_sent,
                    units_expected,
                }),
                target: None,
                report: None,
            }),
            std::iter::once(Message::Interrupted {
                units_sent,
                units_expected,
                reason,
            })
            .chain(skipped_clearing(clearing))
            .collect(),
        ),
        // No route at all: the one outcome that moves to the clipboard, and the
        // machine asks for the same value there in the same press.
        SendOutcome::NotDelivered {
            error: DeliveryError::Unsupported { system },
            ..
        } => (
            Event::InsertionFailed,
            None,
            vec![Message::NoDirectRoute { system }],
        ),
        // The same move for a route the system holds shut until the person allows
        // it: clipboard mode, and the value goes there in the same press.
        SendOutcome::NotDelivered {
            error: DeliveryError::NeedsPermission,
            ..
        } => (Event::InsertionFailed, None, vec![Message::NeedsPermission]),
        // `deliver_value` reports a fragment as `Interrupted`, with its value.
        // Reached only if a route ever reports one another way: still said as
        // an interruption, with how far it got.
        SendOutcome::NotDelivered {
            error:
                DeliveryError::Partial {
                    units_sent,
                    units_expected,
                    reason,
                },
            ..
        } => (
            Event::Cancelled,
            None,
            vec![Message::Interrupted {
                units_sent,
                units_expected,
                reason,
            }],
        ),
        // `OBS-157`: the keys stopped before any reached the field. Nothing of
        // the value is in it, so this is a refusal - the counter stays and the
        // same press tries the same value - and NOT a fragment. A field cleared
        // first is empty now, and the tester has to know that.
        SendOutcome::NotDelivered {
            error: DeliveryError::NothingArrived { reason },
            clearing,
        } => (
            Event::InsertionRefused,
            None,
            vec![if clearing == ClearingOutcome::Done {
                Message::ClearedThenNothingArrived { reason }
            } else {
                Message::NothingArrived { reason }
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
        // A state left on, not a key held. Nothing was sent: refuse, and say it.
        SendOutcome::NotDelivered {
            error: DeliveryError::CapsLockOn,
            ..
        } => (Event::InsertionRefused, None, vec![Message::CapsLockOn]),
        // `attempt` sends this value to the clipboard before classifying, so
        // this arm is reached only if a route other than the direct one ever
        // reported it. Nothing was pressed: refuse, and say why.
        SendOutcome::NotDelivered {
            error: DeliveryError::HigherPrivileges,
            ..
        } => (
            Event::InsertionRefused,
            None,
            vec![Message::HigherPrivileges],
        ),
        // `D73`: the focus is on a button, a link, a list item - nothing was
        // pressed. Not a window problem, so no clipboard: the tester clicks into
        // a field and the same value goes on the next press.
        SendOutcome::NotDelivered {
            error: DeliveryError::NoTextField,
            ..
        } => (Event::InsertionRefused, None, vec![Message::NoTextField]),
        // A route held by somebody else. Nothing went out, so the counter stays.
        SendOutcome::NotDelivered {
            error: DeliveryError::Busy,
            ..
        } => (Event::InsertionRefused, None, vec![busy(on_clipboard)]),
        // The two below are reported by the clipboard route alone - the direct
        // one presses keys and has no clipboard to refuse. A direct route that
        // began to report them would need sentences of its own. The words here
        // name the clipboard, because only it gets here. Nothing reached the
        // clipboard in either, so the counter stays.
        SendOutcome::NotDelivered {
            error: DeliveryError::Refused { detail },
            ..
        } => (
            Event::InsertionRefused,
            None,
            vec![Message::ClipboardFailed { detail }],
        ),
        SendOutcome::NotDelivered {
            error: DeliveryError::CannotCarry { character },
            ..
        } => (
            Event::InsertionRefused,
            None,
            vec![Message::NotForClipboard {
                id: key.value.clone(),
                character,
            }],
        ),
        // Clearing failed, so the value never went. The reason names both the
        // event and the message.
        SendOutcome::NotCleared { error } => {
            let (event, message) = after_clearing(error);
            (event, None, vec![message])
        }
        // A validated pack cannot describe a value this large, but if one did,
        // nothing was sent and it is said plainly.
        SendOutcome::ValueTooLarge { id, .. } => (
            Event::InsertionRefused,
            None,
            vec![Message::ValueTooLarge { id }],
        ),
        // deliver_value returns only the four families above. The load-path
        // outcomes cannot arise from an already-loaded value. If one somehow
        // did, nothing is known to have been sent, so refuse and stay quiet
        // rather than invent a message.
        SendOutcome::NotFound
        | SendOutcome::Unreadable
        | SendOutcome::Refused { .. }
        | SendOutcome::NoSuchIndex { .. }
        | SendOutcome::RouteUnavailable { .. } => (Event::InsertionRefused, None, Vec::new()),
    }
}

/// What a route held by somebody else tells the tester: the clipboard by another
/// application, or the keys of a display by another send of this program (`D157`).
fn busy(on_clipboard: bool) -> Message {
    if on_clipboard {
        Message::ClipboardBusy
    } else {
        Message::TypingBusy
    }
}

/// What a value that went in tells the tester, worse news first (`D95`, `D139`,
/// `D76`, `OBS-141`, `D145`): that it may be missing characters because the
/// application could not be followed, or that it was only handed to the input
/// method, that a text service of the system may have changed it, and that it
/// landed uncleared, with why.
fn notes_after_landing(
    following: Following,
    may_alter: Substitutions,
    clearing: ClearingOutcome,
) -> Vec<Message> {
    let followed = match following {
        Following::Application => None,
        Following::System => Some(Message::NotPaced),
        Following::HandedOver => Some(Message::HandedOver),
    };
    followed
        .into_iter()
        .chain((!may_alter.is_empty()).then_some(Message::MayBeAltered(may_alter)))
        .chain(skipped_clearing(clearing))
        .collect()
}

/// The sentence a value that went in UNCLEARED carries, if it did (`D76`,
/// `OBS-141`, `D125`) - whole or cut short, the field held something before it.
fn skipped_clearing(clearing: ClearingOutcome) -> Option<Message> {
    match clearing {
        ClearingOutcome::Skipped(SkipReason::Unconfirmed) => Some(Message::ClearingSkipped),
        ClearingOutcome::Skipped(SkipReason::Terminal) => Some(Message::ClearingSkippedInTerminal),
        ClearingOutcome::Skipped(SkipReason::ClosedToAccessibility) => {
            Some(Message::ClearingSkippedClosedToAccessibility)
        }
        ClearingOutcome::Skipped(SkipReason::NoClearingKeys) => {
            Some(Message::ClearingSkippedNoKeys)
        }
        ClearingOutcome::Done | ClearingOutcome::NotAsked => None,
    }
}

/// What a clearing that did not go through means. Nothing was sent either way.
/// The reason tells the machine where to go and the tester what to do.
fn after_clearing(error: KeystrokeError) -> (Event, Message) {
    match error {
        // 🔴 No route to press keys is no route at all - the SAME fact a send
        // would have reported one step later, and the same answer: clipboard
        // mode, with the value going there now. Until `D71` it was sorted as a
        // failed clear, "the field may hold part of its old content", about a
        // field nothing had touched - and it never reached clipboard mode,
        // because the clear goes first (`OBS-129`).
        KeystrokeError::Unsupported { system } => {
            (Event::InsertionFailed, Message::NoDirectRoute { system })
        }
        // The same for a route the system holds shut until the person allows it.
        KeystrokeError::NeedsPermission => (Event::InsertionFailed, Message::NeedsPermission),
        KeystrokeError::NoTarget => (Event::InsertionRefused, Message::NoTarget),
        KeystrokeError::ModifierHeld { which } => (
            Event::InsertionRefused,
            Message::ModifierHeld { key: which },
        ),
        // A partial clear leaves the field in an unknown state: it cannot be
        // trusted to be clear, and nothing goes on top of it.
        KeystrokeError::Partial { .. } => (Event::InsertionRefused, Message::ClearingFailed),
        // `OBS-157`: no clearing key acted, so the field is exactly as it was.
        // "It may hold part of its old content" would be the wrong worry.
        KeystrokeError::NothingArrived { reason } => {
            (Event::InsertionRefused, Message::NothingArrived { reason })
        }
        // Rerouted to the clipboard in `attempt` before it gets here. Reached
        // only if that ever stops. No key was pressed and the field is intact.
        KeystrokeError::HigherPrivileges => (Event::InsertionRefused, Message::HigherPrivileges),
        // `D73`, `OBS-135`: the clearing goes first, so this is where a focus on
        // a list of files is caught - before `Home`, `Shift+End`, `Delete`.
        KeystrokeError::NoTextField => (Event::InsertionRefused, Message::NoTextField),
        // `deliver_value` treats this one as a skip and sends the value, so it
        // does not arrive here. If that ever stops, nothing was pressed and
        // nothing was sent - the careful sentence, which asks to check the field.
        KeystrokeError::FieldUnconfirmed
        | KeystrokeError::InTerminal
        | KeystrokeError::ClosedToAccessibility
        | KeystrokeError::NoClearingKeys => (Event::InsertionRefused, Message::ClearingFailed),
    }
}
