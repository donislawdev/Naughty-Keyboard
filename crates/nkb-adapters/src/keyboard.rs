//! Delivering a value by pretending to be a keyboard.
//!
//! This is the whole of `DirectInjection` from architektura.md section 3, and it
//! is deliberately thin: it turns one port into one call and translates the
//! result. Everything that requires `unsafe` lives in `nkb-sys`, behind a safe
//! function, so nothing in this package needs it.

use nkb_app::ports::{
    Availability, Delivered, DeliveryError, KeystrokeError, KeystrokeSender, TargetRef,
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
        match nkb_sys::send_text(text) {
            Ok(outcome) => Ok(Delivered {
                utf16_units: outcome.units,
            }),
            Err(nkb_sys::SendError::Unsupported { system }) => Err(DeliveryError::Unsupported {
                system: system.to_owned(),
            }),
            Err(nkb_sys::SendError::Truncated {
                units_sent,
                units_expected,
            }) => Err(DeliveryError::Partial {
                units_sent,
                units_expected,
            }),
            Err(nkb_sys::SendError::ModifierHeld { key }) => Err(DeliveryError::ModifierHeld {
                which: key.to_owned(),
            }),
            // `send_text` never reports in chords; if it ever did, the honest
            // translation is "nothing is known to have arrived".
            Err(nkb_sys::SendError::ChordsTruncated { .. }) => Err(DeliveryError::Partial {
                units_sent: 0,
                units_expected: text.encode_utf16().count(),
            }),
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
        if nkb_sys::foreground_window().is_none() {
            return Err(KeystrokeError::NoTarget);
        }
        if !chords.is_empty() && blocked_by_privileges() {
            return Err(KeystrokeError::HigherPrivileges);
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
            }) => Err(KeystrokeError::Partial {
                chords_sent,
                chords_expected,
            }),
            // `send_chords` never reports in UTF-16 units; if it ever did, the
            // honest translation is "some of it went out", not success.
            Err(nkb_sys::SendError::Truncated { .. }) => Err(KeystrokeError::Partial {
                chords_sent: 0,
                chords_expected: chords.len(),
            }),
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
        // The mapping is the only place the two vocabularies meet; a key that
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
