//! Delivering a value by pretending to be a keyboard.
//!
//! This is the whole of `DirectInjection` from architektura.md section 3, and it
//! is deliberately thin: it turns one port into one call and translates the
//! result. Everything that requires `unsafe` lives in `nkb-sys`, behind a safe
//! function, so nothing in this package needs it.

use nkb_app::ports::{
    Availability, Delivered, DeliveryError, KeystrokeError, KeystrokeSender, StopReason, TargetRef,
    ValueDelivery,
};
use nkb_core::keys::{Key, KeyChord};

/// Sends the value as synthetic keystrokes, straight to the focused field.
///
/// # Why this carries no state
///
/// architektura.md 6a puts every piece of mutable state in one place in `app`.
/// An adapter holding a handle, a cached target or a "currently sending" flag is
/// where the five named races come from - so this one holds nothing and can be
/// created wherever it is needed.
#[derive(Debug, Default, Clone, Copy)]
pub struct DirectInjection;

impl ValueDelivery for DirectInjection {
    fn availability(&self) -> Availability {
        if !nkb_sys::can_send() {
            // Asked from nkb-sys rather than decided here, so the answer to
            // "which systems can do this" lives in one place.
            return match nkb_sys::send_text("") {
                Err(nkb_sys::SendError::Unsupported { system }) => Availability::Unavailable {
                    reason: system.to_owned(),
                },
                _ => Availability::Unavailable {
                    reason: "this system".to_owned(),
                },
            };
        }
        // A route exists, but there may be nothing in front of it. That is a
        // different answer from "this system cannot", and the caller acts on it
        // differently - so it is reported by `deliver`, not folded in here.
        Availability::Ready
    }

    fn target(&self) -> Option<TargetRef> {
        nkb_sys::foreground_window().map(|window| TargetRef(window.0))
    }

    fn deliver(&self, text: &str) -> Result<Delivered, DeliveryError> {
        if nkb_sys::foreground_window().is_none() {
            return Err(DeliveryError::NoTarget);
        }
        if !text.is_empty() && blocked_by_privileges() {
            return Err(DeliveryError::HigherPrivileges);
        }
        if !text.is_empty() && focus_outside_a_text_field() {
            return Err(DeliveryError::NoTextField);
        }
        match nkb_sys::send_text(text) {
            Ok(outcome) => Ok(Delivered {
                utf16_units: outcome.units,
                paced: outcome.paced,
            }),
            Err(nkb_sys::SendError::Unsupported { system }) => Err(DeliveryError::Unsupported {
                system: system.to_owned(),
            }),
            Err(nkb_sys::SendError::Truncated {
                units_sent,
                units_expected,
                reason,
            }) => Err(value_stopped(units_sent, units_expected, reason)),
            Err(nkb_sys::SendError::ModifierHeld { key }) => Err(DeliveryError::ModifierHeld {
                which: key.to_owned(),
            }),
            // `send_text` never reports in chords. If it ever did, the honest
            // translation is "nothing is known to have arrived".
            Err(nkb_sys::SendError::ChordsTruncated { reason, .. }) => {
                Err(DeliveryError::NothingArrived {
                    reason: stop_reason(reason),
                })
            }
        }
    }
}

/// Whether the window in front runs with higher privileges than this process.
///
/// Asked right before anything is pressed, by both doors of this adapter - the
/// value and the clearing - because the system drops such input while reporting
/// success, so after the fact a blocked send cannot be told from a delivered one
/// (`OBS-128`, `D72`). Asked HERE rather than by the caller, so no caller can
/// forget it and the answer is as fresh as the send (`W2`). A level that could
/// not be read (`InputReach::Unknown`) presses as before - never a guess.
///
/// Nothing is pressed when there is nothing to press, so an empty value or an
/// empty recipe is not refused: refusing would report a blocked send that was
/// never going to happen.
fn blocked_by_privileges() -> bool {
    nkb_sys::foreground_window().is_some_and(|window| {
        nkb_sys::privilege::input_reach(window) == nkb_sys::privilege::InputReach::HigherPrivileges
    })
}

/// Whether the keyboard focus in the window in front is surely NOT a text field
/// - a button, a link, a list item, a page that is not editable (`D73`).
///
/// The clearing recipe keeps "never beyond the field" only inside a field: on a
/// list of files `Home`, `Shift+End`, `Delete` select them all and delete them
/// (`OBS-135`), and on a button a space from the value presses it. Asked by
/// the value's door, after the privilege check - a window that takes no typing
/// at all is the stronger answer - and asked HERE for the same reasons as that
/// one. `FocusedInput::Unknown` sends the value as before: only a type that
/// takes no text by definition stops a send, never a guess. The clearing door
/// asks the stricter [`clearing_verdict`].
fn focus_outside_a_text_field() -> bool {
    nkb_sys::foreground_window().is_some_and(|window| {
        nkb_sys::field::focused_input(window) == nkb_sys::field::FocusedInput::NotTextField
    })
}

/// Whether the clearing recipe may be pressed on this focus (`D76`).
///
/// Stricter than the value's door, and deliberately so. A value typed into a
/// focus nobody recognised lands where the tester put it. The recipe does not:
/// `Home`, `Shift+End`, `Delete` keep "never beyond the field" only inside a
/// field, and an application that does not say what holds its focus may be a
/// grid. Measured 2026-09-24 in LibreOffice Calc, which reports only its frame
/// window: the recipe on cell A1 emptied all five cells of the row. So only a
/// CONFIRMED text field is cleared, and the unknown is left as it is.
fn clearing_verdict(focus: nkb_sys::field::FocusedInput) -> Result<(), KeystrokeError> {
    match focus {
        nkb_sys::field::FocusedInput::TextField => Ok(()),
        nkb_sys::field::FocusedInput::NotTextField => Err(KeystrokeError::NoTextField),
        nkb_sys::field::FocusedInput::Unknown => Err(KeystrokeError::FieldUnconfirmed),
        // `OBS-141`: a terminal is a field for the value, and never for the
        // recipe - what the keys do there is the program's choice, not ours.
        nkb_sys::field::FocusedInput::Terminal => Err(KeystrokeError::InTerminal),
    }
}

/// Why the route stopped, in the words of `app`. Mirrors the enum of `nkb-sys`
/// one to one - the two crates do not depend on each other (`D95`).
fn stop_reason(reason: nkb_sys::StopReason) -> StopReason {
    match reason {
        nkb_sys::StopReason::Dropped => StopReason::Dropped,
        nkb_sys::StopReason::FocusMoved => StopReason::FocusMoved,
        nkb_sys::StopReason::NotTaking => StopReason::NotTaking,
        nkb_sys::StopReason::Escape => StopReason::Escape,
    }
}

/// A value that stopped part way. Zero units is NOT a fragment: the field holds
/// none of the value, and saying "partial" there is what `OBS-157` caught.
fn value_stopped(
    units_sent: usize,
    units_expected: usize,
    reason: nkb_sys::StopReason,
) -> DeliveryError {
    let reason = stop_reason(reason);
    if units_sent == 0 {
        DeliveryError::NothingArrived { reason }
    } else {
        DeliveryError::Partial {
            units_sent,
            units_expected,
            reason,
        }
    }
}

/// Clearing keys that stopped part way. Zero chords acted means the field is
/// exactly as it was - no key that could change it went down (`D95` point 5).
fn chords_stopped(
    chords_sent: usize,
    chords_expected: usize,
    reason: nkb_sys::StopReason,
) -> KeystrokeError {
    let reason = stop_reason(reason);
    if chords_sent == 0 {
        KeystrokeError::NothingArrived { reason }
    } else {
        KeystrokeError::Partial {
            chords_sent,
            chords_expected,
            reason,
        }
    }
}

/// The one-line mapping between the vocabulary `app` speaks and the one
/// `nkb-sys` speaks. Two enums rather than one shared type, because `nkb-sys`
/// depends on nothing of ours and `nkb-core` knows nothing about systems.
fn chord_for(chord: &KeyChord) -> nkb_sys::Chord {
    nkb_sys::Chord {
        key: match chord.key {
            Key::Home => nkb_sys::NavKey::Home,
            Key::End => nkb_sys::NavKey::End,
            Key::Delete => nkb_sys::NavKey::Delete,
        },
        shift: chord.shift,
    }
}

impl KeystrokeSender for DirectInjection {
    fn send_keystrokes(&self, chords: &[KeyChord]) -> Result<(), KeystrokeError> {
        let Some(window) = nkb_sys::foreground_window() else {
            return Err(KeystrokeError::NoTarget);
        };
        if !chords.is_empty() && blocked_by_privileges() {
            return Err(KeystrokeError::HigherPrivileges);
        }
        if !chords.is_empty() {
            clearing_verdict(nkb_sys::field::focused_input(window))?;
        }
        let mapped: Vec<nkb_sys::Chord> = chords.iter().map(chord_for).collect();
        match nkb_sys::send_chords(&mapped) {
            Ok(_) => Ok(()),
            Err(nkb_sys::SendError::Unsupported { system }) => Err(KeystrokeError::Unsupported {
                system: system.to_owned(),
            }),
            Err(nkb_sys::SendError::ModifierHeld { key }) => Err(KeystrokeError::ModifierHeld {
                which: key.to_owned(),
            }),
            Err(nkb_sys::SendError::ChordsTruncated {
                chords_sent,
                chords_expected,
                reason,
            }) => Err(chords_stopped(chords_sent, chords_expected, reason)),
            // `send_chords` never reports in UTF-16 units. If it ever did, the
            // honest translation is "nothing is known to have acted", not success.
            Err(nkb_sys::SendError::Truncated { reason, .. }) => {
                Err(KeystrokeError::NothingArrived {
                    reason: stop_reason(reason),
                })
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

    #[test]
    fn every_key_of_the_vocabulary_maps_to_a_navigation_key_and_shift_survives() {
        // The mapping is the only place the two vocabularies meet. A key that
        // fell through to a wrong neighbour would clear the wrong thing.
        assert_eq!(
            chord_for(&KeyChord::plain(Key::Home)),
            nkb_sys::Chord {
                key: nkb_sys::NavKey::Home,
                shift: false
            }
        );
        assert_eq!(
            chord_for(&KeyChord::shifted(Key::End)),
            nkb_sys::Chord {
                key: nkb_sys::NavKey::End,
                shift: true
            }
        );
        assert_eq!(
            chord_for(&KeyChord::plain(Key::Delete)),
            nkb_sys::Chord {
                key: nkb_sys::NavKey::Delete,
                shift: false
            }
        );
    }

    #[test]
    fn zero_is_nothing_arrived_and_anything_more_is_a_fragment_with_its_reason() {
        // `OBS-157`: under another program's `BlockInput` the field kept its own
        // content while the tool said it held a partial value.
        assert_eq!(
            value_stopped(0, 255, nkb_sys::StopReason::Dropped),
            DeliveryError::NothingArrived {
                reason: StopReason::Dropped
            }
        );
        assert_eq!(
            value_stopped(3, 255, nkb_sys::StopReason::NotTaking),
            DeliveryError::Partial {
                units_sent: 3,
                units_expected: 255,
                reason: StopReason::NotTaking
            }
        );
        assert_eq!(
            chords_stopped(0, 3, nkb_sys::StopReason::FocusMoved),
            KeystrokeError::NothingArrived {
                reason: StopReason::FocusMoved
            }
        );
        assert_eq!(
            chords_stopped(1, 3, nkb_sys::StopReason::Dropped),
            KeystrokeError::Partial {
                chords_sent: 1,
                chords_expected: 3,
                reason: StopReason::Dropped
            }
        );
    }

    #[test]
    fn every_reason_of_the_system_maps_to_its_own_reason() {
        let mapped = [
            stop_reason(nkb_sys::StopReason::Dropped),
            stop_reason(nkb_sys::StopReason::FocusMoved),
            stop_reason(nkb_sys::StopReason::NotTaking),
            stop_reason(nkb_sys::StopReason::Escape),
        ];
        assert_eq!(
            mapped,
            [
                StopReason::Dropped,
                StopReason::FocusMoved,
                StopReason::NotTaking,
                StopReason::Escape
            ]
        );
    }

    #[test]
    fn only_a_confirmed_text_field_is_cleared() {
        use nkb_sys::field::FocusedInput;
        assert_eq!(clearing_verdict(FocusedInput::TextField), Ok(()));
        assert_eq!(
            clearing_verdict(FocusedInput::NotTextField),
            Err(KeystrokeError::NoTextField)
        );
        // `D76`: the answer a spreadsheet gives. Clearing there emptied a row.
        assert_eq!(
            clearing_verdict(FocusedInput::Unknown),
            Err(KeystrokeError::FieldUnconfirmed)
        );
        // `OBS-141`: the same reading with a prompt and with a file manager in it.
        assert_eq!(
            clearing_verdict(FocusedInput::Terminal),
            Err(KeystrokeError::InTerminal)
        );
    }

    #[test]
    fn no_keystrokes_press_nothing_where_a_route_exists() {
        if !nkb_sys::can_send() {
            return;
        }
        match DirectInjection.send_keystrokes(&[]) {
            Ok(()) | Err(KeystrokeError::NoTarget) => {}
            Err(other) => panic!("an empty sequence must not fail this way: {other}"),
        }
    }

    #[test]
    fn availability_agrees_with_what_nkb_sys_says_about_this_build() {
        let availability = DirectInjection.availability();
        if nkb_sys::can_send() {
            assert_eq!(availability, Availability::Ready);
        } else {
            let Availability::Unavailable { reason } = availability else {
                panic!("a build that cannot send must not report itself ready");
            };
            assert!(
                !reason.is_empty(),
                "untouchable rule 1: the reason has to name something"
            );
        }
    }

    #[test]
    fn an_empty_value_is_not_an_error_where_a_route_exists() {
        // Sending nothing is a legitimate thing to ask for: `len-0` is a real
        // value in the shipped catalogue.
        if !nkb_sys::can_send() {
            return;
        }
        // No assertion on the target here - on a machine with no focused window
        // this is `NoTarget`, which is correct and not a failure of this code.
        match DirectInjection.deliver("") {
            Ok(delivered) => assert_eq!(delivered.utf16_units, 0),
            Err(DeliveryError::NoTarget) => {}
            Err(other) => panic!("an empty value must not fail this way: {other}"),
        }
    }
}
