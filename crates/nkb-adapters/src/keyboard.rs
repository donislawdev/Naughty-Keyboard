//! Delivering a value by pretending to be a keyboard.
//!
//! This is the whole of `DirectInjection` from architektura.md section 3, and it
//! is deliberately thin: it turns one port into one call and translates the
//! result. Everything that requires `unsafe` lives in `nkb-sys`, behind a safe
//! function, so nothing in this package needs it.

use nkb_app::ports::{Availability, Delivered, DeliveryError, TargetRef, ValueDelivery};

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
