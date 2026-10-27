//! What `nkb send` says when a value or its clearing did not go through: the
//! sentence of each refusal, and the words for why a send stopped. Out of
//! `main.rs` since the macOS permission (`D138`), under the ceiling of `D113`.
//! The sentences about the clearing that was SKIPPED, and the value went on,
//! are `clearing.rs`.

use std::io::Write;

use nkb_app::{DeliveryError, KeystrokeError, StopReason, ValueFacts};

/// What a value cut short leaves, as `nkb send` says it: the value by name,
/// how much of it arrived and why it stopped. `in_flight` when keys were still
/// on their way, and the count is a floor (`D158`).
///
/// The loudest message in this command on purpose: the field now holds a
/// fragment, and a person who does not know that will report the fragment as
/// the application's doing. It names the value, as a value that arrived whole
/// is named - until `OBS-126` a fragment was the one outcome that did not say
/// whose it was.
pub(crate) fn say_cut_short(
    err: &mut impl Write,
    facts: &ValueFacts,
    units_sent: usize,
    units_expected: usize,
    reason: StopReason,
    in_flight: bool,
) {
    let _ = writeln!(
        err,
        "nkb send: interrupted {} - {}",
        facts.reference, facts.name
    );
    let reason = stop_reason(reason);
    if in_flight {
        let _ = writeln!(
            err,
            "  at least {units_sent} of {units_expected} UTF-16 units arrived - {reason}, and more may still arrive."
        );
        let _ = writeln!(err, "Check the field and clear it before testing.");
    } else {
        let _ = writeln!(
            err,
            "  only {units_sent} of {units_expected} UTF-16 units arrived - {reason}."
        );
        let _ = writeln!(
            err,
            "The field holds a PARTIAL value. Clear it before testing."
        );
    }
}

/// Why the clearing asked for with `--clear` did not go through. Nothing was
/// sent after it, and every sentence says so.
pub(crate) fn say_not_cleared(err: &mut impl Write, error: &KeystrokeError) {
    let _ = match error {
        KeystrokeError::ModifierHeld { which } => writeln!(
            err,
            "nkb send: {which} is still held on the keyboard, so nothing was sent - release it and run again."
        ),
        KeystrokeError::NoTarget => {
            writeln!(
                err,
                "nkb send: nothing holds the keyboard focus, so there is nothing to clear."
            )
        }
        KeystrokeError::Unsupported { system } => {
            writeln!(err, "nkb send: clearing is not supported on {system} yet.")
        }
        KeystrokeError::Partial {
            chords_sent,
            chords_expected,
            reason,
        } => writeln!(
            err,
            "nkb send: only {chords_sent} of {chords_expected} clearing key presses arrived - {}. The field may be half-cleared. Nothing else was sent.",
            stop_reason(*reason)
        ),
        // `OBS-157`: zero presses acted, so the field is as it was -
        // "half-cleared" would send the tester looking for damage.
        KeystrokeError::NothingArrived { reason } => writeln!(
            err,
            "nkb send: none of the clearing key presses reached the field - {}. The field is as it was, and nothing was sent.",
            stop_reason(*reason)
        ),
        KeystrokeError::HigherPrivileges => writeln!(err, "{HIGHER_PRIVILEGES}"),
        KeystrokeError::NeedsPermission => writeln!(err, "{NEEDS_PERMISSION}"),
        KeystrokeError::NoTextField => writeln!(err, "{NO_TEXT_FIELD}"),
        // `send_value` treats this one as a skip and sends the value, so
        // it does not arrive here. If that ever stops, nothing was
        // pressed and nothing was sent, and that is what is said.
        KeystrokeError::FieldUnconfirmed => writeln!(
            err,
            "nkb send: the focus could not be confirmed as a text field, so it was not cleared and nothing was sent."
        ),
        // The same skip for a terminal, and the same fallback if it ever
        // arrives here: nothing pressed, nothing sent.
        KeystrokeError::InTerminal => writeln!(
            err,
            "nkb send: the focus is a terminal, so it was not cleared and nothing was sent."
        ),
        // And for a window closed to accessibility (`D125`).
        KeystrokeError::ClosedToAccessibility => writeln!(
            err,
            "nkb send: the window shows the system nothing inside it, so it was not cleared and nothing was sent."
        ),
        // And for a system whose clearing keys this build cannot press.
        KeystrokeError::NoClearingKeys => writeln!(
            err,
            "nkb send: this build cannot press the clearing keys on this system, so it was not cleared and nothing was sent."
        ),
    };
}

/// Why none of the value, or only part of it, reached the field - for a route
/// that reports it other than through `SendOutcome::Interrupted` or
/// `SendOutcome::InFlight`.
pub(crate) fn say_not_delivered(err: &mut impl Write, error: &DeliveryError) {
    match error {
        DeliveryError::NoTarget => {
            let _ = writeln!(err, "nkb send: nothing holds the keyboard focus.");
            let _ = writeln!(err, "Click into a field, then run this again.");
        }
        DeliveryError::Unsupported { system } => {
            let _ = writeln!(err, "nkb send: not supported on {system} yet.");
        }
        DeliveryError::ModifierHeld { which } => {
            let _ = writeln!(
                err,
                "nkb send: {which} is still held on the keyboard, so the value was not sent - release it and run again."
            );
        }
        // A state somebody left on, not a key held: the sentence says "off".
        DeliveryError::CapsLockOn => {
            let _ = writeln!(
                err,
                "nkb send: Caps Lock is on, so nothing was sent - many applications would turn the letters into capitals. Turn it off and run again."
            );
        }
        // `send_value` reports a fragment as `Interrupted`, above, with
        // the value it belongs to. Reached only if a route ever reports
        // one another way - still said, without the name it does not have.
        DeliveryError::Partial {
            units_sent,
            units_expected,
            reason,
        } => {
            let _ = writeln!(
                err,
                "nkb send: only {units_sent} of {units_expected} UTF-16 units arrived - {}.",
                stop_reason(*reason)
            );
            let _ = writeln!(
                err,
                "The field holds a PARTIAL value. Clear it before testing."
            );
        }
        // The same for keys still on their way (`D158`).
        DeliveryError::InFlight {
            units_sent,
            units_expected,
            reason,
        } => {
            let _ = writeln!(
                err,
                "nkb send: at least {units_sent} of {units_expected} UTF-16 units arrived - {}, and more may still arrive.",
                stop_reason(*reason)
            );
            let _ = writeln!(err, "Check the field and clear it before testing.");
        }
        // Typing on X11 waits for another send on the same display for a
        // while, and then gives up (D157). The clipboard route, the other
        // one that reports it, is not this command's (D71, OBS-131).
        DeliveryError::Busy => {
            let _ = writeln!(
                err,
                "nkb send: another send of Naughty Keyboard is typing on this display, so nothing was sent - run again once it has finished."
            );
        }
        // A refusal in the route's own words belongs to the clipboard
        // route of the palette, which this command does not have and will
        // not have (D71, OBS-131). A character the route cannot carry is
        // typing's too: U+0000 has no key on Linux (D145). Both are
        // answered in plain words rather than folded into a guess.
        DeliveryError::Refused { detail } => {
            let _ = writeln!(err, "nkb send: the route refused the value: {detail}.");
        }
        DeliveryError::CannotCarry { character } => {
            let _ = writeln!(
                err,
                "nkb send: this route cannot carry U+{:04X}, so nothing was sent.",
                u32::from(*character)
            );
        }
        DeliveryError::HigherPrivileges => {
            let _ = writeln!(err, "{HIGHER_PRIVILEGES}");
        }
        DeliveryError::NeedsPermission => {
            let _ = writeln!(err, "{NEEDS_PERMISSION}");
        }
        DeliveryError::NoTextField => {
            let _ = writeln!(err, "{NO_TEXT_FIELD}");
        }
        // `OBS-157`: the keys stopped before any reached the field. No
        // fragment - "PARTIAL" here sent testers to clear a field that
        // held only its own content. A clearing that went first was
        // said before this, by `report_send`.
        DeliveryError::NothingArrived { reason } => {
            let _ = writeln!(
                err,
                "nkb send: nothing of the value reached the field - {}.",
                stop_reason(*reason)
            );
        }
    }
}

/// Why a send stopped, in the words `nkb send` uses after a dash. The same
/// reasons the palette names (`D95`, `D96`, `D149`), said in the command's own
/// sentences.
fn stop_reason(reason: StopReason) -> String {
    match reason {
        StopReason::Dropped => {
            "the system did not deliver the keys, another program may be blocking input".to_owned()
        }
        StopReason::FocusMoved => "another window came to the front".to_owned(),
        StopReason::NotTaking => "the application stopped taking keys".to_owned(),
        StopReason::Escape => "Escape was pressed".to_owned(),
        StopReason::KeyPressed { key } => format!("{key} was pressed"),
        // `nkb send` hands over a flag nobody sets (`send_value`), so this is never
        // said today. Should a route ever report it, the words are true.
        StopReason::Halted => "the send was asked to stop".to_owned(),
    }
}

/// What `nkb send` says when macOS has not given the app permission to type.
///
/// The permission belongs to the app the system holds responsible, which for a
/// command typed in a terminal is the terminal. So that is what the sentence
/// tells the person to turn on, and not `nkb` itself (`D138`, `OBS-70`). Code 4,
/// the existing "the value could not be inserted" (`D46`).
pub(crate) const NEEDS_PERMISSION: &str = "nkb send: macOS has not allowed the app you run nkb from to type for you, so nothing was sent. Turn that app on in System Settings, Privacy and Security, Accessibility (for example Terminal), then run this again.";

/// What `nkb send` says when the window in front runs with higher privileges.
///
/// One sentence for both places it can come from - the clearing, which goes
/// first, and the send without `--clear` - because the fact is the same and no
/// key was pressed in either. Until `D72` this case printed "sent" and exited
/// with 0 while nothing reached the field (`OBS-128`). Code 4 is the existing
/// "the value could not be inserted", not a new code (`D46`).
pub(crate) const HIGHER_PRIVILEGES: &str = "nkb send: the window in front runs with higher privileges than nkb, so the system would drop the keystrokes without a word. Nothing was sent - run nkb with the same privileges to type into it.";

/// What `nkb send` says when the keyboard focus is surely not in a text field.
///
/// One sentence for both doors, like the one above: the clearing refuses first
/// when `--clear` is given, the send refuses otherwise, and no key was pressed
/// in either (`D73`). The cause is one the command READ - the control type of
/// the focused element - so it is named, and a focus it could not read sends.
///
/// "The system reports", because the command knows only what UI Automation
/// answered: a Firefox window asked for the first time reports its page for a
/// while, although the focus is in a field, until the focus moves (`OBS-140`).
/// Clicking into the field again is what ends that, so it is what is asked for.
pub(crate) const NO_TEXT_FIELD: &str = "nkb send: the system reports the keyboard focus outside a text field, so nothing was sent - keys there could press buttons or act on list items. Click into the field and run this again.";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_reason_a_send_stopped_reads_as_its_own_and_names_the_key() {
        // `D149`: the modifier is named, as the palette names it, so the person
        // knows which key of theirs stopped the value.
        let reasons = [
            StopReason::Dropped,
            StopReason::FocusMoved,
            StopReason::NotTaking,
            StopReason::Escape,
            StopReason::KeyPressed { key: "Alt" },
            StopReason::Halted,
        ]
        .map(stop_reason);
        for (i, reason) in reasons.iter().enumerate() {
            assert!(!reason.is_empty() && !reason.ends_with('.'), "{reason}");
            assert!(!reasons[i + 1..].contains(reason), "\"{reason}\" twice");
        }
        assert_eq!(reasons[4], "Alt was pressed");
    }
}
