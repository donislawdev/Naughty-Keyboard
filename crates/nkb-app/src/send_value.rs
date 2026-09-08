//! Putting one value of one pack into whatever field has the focus.
//!
//! This is the use case behind `nkb send`, and the first one in the project that
//! reaches outside the process to something that is not a file.
//!
//! # What it does NOT do, and why the list matters here more than elsewhere
//!
//! - it does not clear the field first. Clearing sends keystrokes that are not
//!   the value (`ux-spec.md` 4 calls it the only such place), and those go
//!   through a different port, `KeystrokeSender`, which does not exist yet;
//! - it does not check what the target is. `product-spec.md` 10.1 requires the
//!   application name and window title before every insert, and that needs
//!   `TargetInspector` and the `WindowTitle` type - a separate piece, because
//!   the type is what keeps titles out of the log;
//! - it does not re-check the target mid-insert (race `W2`), does not handle
//!   `Escape` (`W3`), and writes nothing to a session file (`W4`).
//!
//! Every one of those belongs to step 3 of the plan of work. Step 2 answers one
//! question only - whether the core loop is possible at all - and saying which
//! parts are missing is untouchable rule 1 applied to a use case.

use crate::ports::{
    Availability, DeliveryError, PackFormat, PackSource, SourceError, ValueDelivery,
};
use nkb_core::lint::Severity;
use nkb_core::pack::{Pack, PackValue};
use nkb_core::value::ValueProblem;

/// What happened to one send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SendOutcome {
    /// The value reached the field.
    Sent {
        /// `pack-id/value-id`, the same shape `emit` prints.
        reference: String,
        name: String,
        code_points: usize,
        bytes: usize,
        /// What actually crossed the wire. Differs from `code_points` exactly
        /// when the value contains characters above the basic plane.
        utf16_units: usize,
        /// Warnings the pack carried. It was still sent - warnings never block,
        /// only errors do - but a silent send would hide them.
        warnings: usize,
    },
    /// No pack of that name.
    NotFound,
    /// Present but unreadable, or not UTF-8.
    Unreadable,
    /// The pack has errors, so it is not loaded at all - pack-format.md 11.
    Refused { errors: usize },
    /// The pack loaded, but has no value at that position.
    NoSuchIndex { asked: usize, available: usize },
    /// The recipe describes more text than the format allows.
    ValueTooLarge { id: String, code: &'static str },
    /// There is no delivery route on this system.
    RouteUnavailable { reason: String },
    /// There is a route, and it did not work.
    NotDelivered { error: DeliveryError },
}

/// Sends one value, addressed by position within its pack.
///
/// # Positions start at one
///
/// `ux-spec.md` 4 shows the palette counting `7/34` to a person, and untouchable
/// rule 12 asks that a feature mean the same thing from both surfaces. A CLI
/// that started at zero would make `--index 7` and the counter's `7` two
/// different values, which is the kind of difference nobody notices until it has
/// produced a wrong bug report.
///
/// # Order is the pack's order
///
/// `D47`: the order of `[[values]]` is the order of testing and is untouchable.
/// So position 3 means the third value as the pack file writes it, not the third
/// alphabetically and not the third of some filtered view.
#[must_use]
pub fn send_value(
    source: &dyn PackSource,
    format: &dyn PackFormat,
    delivery: &dyn ValueDelivery,
    pack_id: &str,
    position: usize,
) -> SendOutcome {
    // Asked FIRST, before reading anything. A machine with no route should say
    // so rather than spend the work and fail at the last step - and on macOS
    // that is not hypothetical: OBS-70 measured that the system refuses the
    // call outright without a permission no script can grant.
    if let Availability::Unavailable { reason } = delivery.availability() {
        return SendOutcome::RouteUnavailable { reason };
    }

    let text = match source.read(pack_id) {
        Ok(text) => text,
        Err(SourceError::NotFound) => return SendOutcome::NotFound,
        Err(SourceError::Unreadable | SourceError::NotUtf8) => return SendOutcome::Unreadable,
    };

    // check first, refuse on any error, parse after - pack-format.md 11 and the
    // binding order in architektura.md 3.
    let problems = format.check(&text, pack_id);
    let errors = problems
        .iter()
        .filter(|problem| problem.severity() == Severity::Error)
        .count();
    if errors > 0 {
        return SendOutcome::Refused { errors };
    }
    let warnings = problems.len() - errors;

    let Some(pack) = format.parse(&text) else {
        return SendOutcome::Refused { errors: 1 };
    };

    deliver_one(&pack, delivery, position, warnings)
}

fn deliver_one(
    pack: &Pack,
    delivery: &dyn ValueDelivery,
    position: usize,
    warnings: usize,
) -> SendOutcome {
    let available = pack.values.len();
    if position == 0 || position > available {
        return SendOutcome::NoSuchIndex {
            asked: position,
            available,
        };
    }
    // Checked immediately above, so this cannot wrap.
    let Some(value) = pack.values.get(position - 1) else {
        return SendOutcome::NoSuchIndex {
            asked: position,
            available,
        };
    };

    send_that_value(pack, value, delivery, warnings)
}

fn send_that_value(
    pack: &Pack,
    value: &PackValue,
    delivery: &dyn ValueDelivery,
    warnings: usize,
) -> SendOutcome {
    // Measured from the RECIPE, before anything is built - architektura.md 6.1.
    // A value declaring two billion characters is refused here without a single
    // byte being allocated for it.
    let Some(metrics) = value.body.metrics() else {
        return SendOutcome::ValueTooLarge {
            id: value.id.clone(),
            code: ValueProblem::RepeatProductUnmeasurable.code(),
        };
    };

    let literal = match value.body.materialise() {
        Ok(text) => text,
        Err(problem) => {
            return SendOutcome::ValueTooLarge {
                id: value.id.clone(),
                code: problem.code(),
            };
        }
    };

    let expected_units = literal.encode_utf16().count();

    match delivery.deliver(&literal) {
        Ok(delivered) => SendOutcome::Sent {
            reference: format!("{}/{}", pack.id, value.id),
            name: value.name.clone(),
            code_points: metrics.code_points,
            bytes: metrics.bytes,
            utf16_units: delivered.utf16_units,
            warnings,
        },
        Err(DeliveryError::Partial { units_sent, .. }) => SendOutcome::NotDelivered {
            // Rebuilt with the count this layer knows, so the two halves of the
            // bad news come from the same place and cannot disagree.
            error: DeliveryError::Partial {
                units_sent,
                units_expected: expected_units,
            },
        },
        Err(error) => SendOutcome::NotDelivered { error },
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
    use crate::ports::{Date, Delivered, TargetRef, TranslationCheck, TranslationTarget};
    use nkb_core::lint::{LintProblem, RuleCode};
    use nkb_core::pack::Risk;
    use nkb_core::text::LiteralText;
    use nkb_core::value::ValueBody;

    /// Records what it was handed, and can be told how to answer.
    struct Spy {
        available: Availability,
        fail_with: Option<DeliveryError>,
        seen: std::cell::RefCell<Vec<String>>,
    }

    impl Spy {
        fn ready() -> Self {
            Self {
                available: Availability::Ready,
                fail_with: None,
                seen: std::cell::RefCell::new(Vec::new()),
            }
        }
    }

    impl ValueDelivery for Spy {
        fn availability(&self) -> Availability {
            self.available.clone()
        }
        fn target(&self) -> Option<TargetRef> {
            Some(TargetRef(1))
        }
        fn deliver(&self, text: &str) -> Result<Delivered, DeliveryError> {
            self.seen.borrow_mut().push(text.to_owned());
            match &self.fail_with {
                None => Ok(Delivered {
                    utf16_units: text.encode_utf16().count(),
                }),
                Some(error) => Err(error.clone()),
            }
        }
    }

    struct Shelf;

    impl PackSource for Shelf {
        fn read(&self, _id: &str) -> Result<String, SourceError> {
            Ok("whatever, the format is scripted".to_owned())
        }
    }

    fn value(id: &str, text: &str) -> PackValue {
        PackValue {
            id: id.to_owned(),
            name: format!("Value {id}"),
            body: ValueBody::Literal(LiteralText::new(text.to_owned())),
            breaks: Some("It breaks something worth a whole sentence about it.".to_owned()),
            expect: None,
            risk: None,
            fields: Vec::new(),
            tags: Vec::new(),
            source: None,
            since: Some("1.0".to_owned()),
            shape: None,
            deprecated: false,
            replaced_by: None,
        }
    }

    fn a_pack(values: Vec<PackValue>) -> Pack {
        Pack {
            id: "sample".to_owned(),
            name: "Sample".to_owned(),
            description: "A pack for the tests in this module.".to_owned(),
            version: "1.0".to_owned(),
            updated: "2026-09-08".to_owned(),
            license: "CC-BY-4.0".to_owned(),
            authors: vec!["Naughty Keyboard".to_owned()],
            language: "en".to_owned(),
            risk: Risk::Normal,
            tags: Vec::new(),
            fields: vec!["any".to_owned()],
            source: None,
            values,
            pairs: Vec::new(),
        }
    }

    struct Scripted {
        errors: usize,
        pack: Option<Pack>,
    }

    impl Scripted {
        /// Two values: an ordinary one, and one carrying a character above the
        /// basic plane - the case the whole UTF-16 count exists for.
        fn two() -> Self {
            Self {
                errors: 0,
                pack: Some(a_pack(vec![
                    value("first", "ab"),
                    value("second", "a\u{1F600}"),
                ])),
            }
        }
    }

    impl PackFormat for Scripted {
        fn check(&self, _text: &str, _expected_id: &str) -> Vec<LintProblem> {
            (0..self.errors)
                .map(|_| LintProblem::new(RuleCode::PackWithoutValues))
                .collect()
        }
        fn parse(&self, _text: &str) -> Option<Pack> {
            self.pack.clone()
        }
        fn skeleton(&self, _id: &str, _today: Date) -> String {
            String::new()
        }
        fn canonical(&self, _text: &str) -> Option<String> {
            None
        }
        fn translated_pack(&self, _text: &str) -> TranslationTarget {
            TranslationTarget::NotATranslation
        }
        fn check_translation(&self, _t: &str, _o: &str) -> TranslationCheck {
            TranslationCheck::Compared(Vec::new())
        }
        fn same_insertions(&self, _before: &str, _after: &str) -> bool {
            true
        }
    }

    #[test]
    fn positions_start_at_one_because_the_palette_counts_from_one() {
        let spy = Spy::ready();
        let outcome = send_value(&Shelf, &Scripted::two(), &spy, "sample", 1);
        assert!(
            matches!(outcome, SendOutcome::Sent { .. }),
            "position 1 must be the first value, got {outcome:?}"
        );
        assert_eq!(spy.seen.borrow().as_slice(), ["ab"]);
    }

    #[test]
    fn position_zero_is_refused_rather_than_treated_as_the_first() {
        let spy = Spy::ready();
        let outcome = send_value(&Shelf, &Scripted::two(), &spy, "sample", 0);
        assert_eq!(
            outcome,
            SendOutcome::NoSuchIndex {
                asked: 0,
                available: 2
            }
        );
        assert!(
            spy.seen.borrow().is_empty(),
            "nothing may be delivered for a position that does not exist"
        );
    }

    #[test]
    fn a_position_past_the_end_says_how_many_there_are() {
        let spy = Spy::ready();
        let outcome = send_value(&Shelf, &Scripted::two(), &spy, "sample", 9);
        assert_eq!(
            outcome,
            SendOutcome::NoSuchIndex {
                asked: 9,
                available: 2
            }
        );
    }

    #[test]
    fn an_astral_value_reports_more_units_than_code_points() {
        // The whole reason the port counts UTF-16 units rather than characters.
        let spy = Spy::ready();
        let outcome = send_value(&Shelf, &Scripted::two(), &spy, "sample", 2);
        let SendOutcome::Sent {
            code_points,
            utf16_units,
            ..
        } = outcome
        else {
            panic!("expected a send, got {outcome:?}");
        };
        assert_eq!(code_points, 2, "'a' plus one emoji is two code points");
        assert_eq!(utf16_units, 3, "the emoji crosses as a surrogate pair");
    }

    #[test]
    fn a_machine_without_a_route_is_told_before_the_pack_is_even_read() {
        struct Exploding;
        impl PackSource for Exploding {
            fn read(&self, _id: &str) -> Result<String, SourceError> {
                panic!("the pack must not be read when there is no route to deliver it");
            }
        }
        let spy = Spy {
            available: Availability::Unavailable {
                reason: "macOS".to_owned(),
            },
            fail_with: None,
            seen: std::cell::RefCell::new(Vec::new()),
        };
        let outcome = send_value(&Exploding, &Scripted::two(), &spy, "sample", 1);
        assert_eq!(
            outcome,
            SendOutcome::RouteUnavailable {
                reason: "macOS".to_owned()
            }
        );
    }

    #[test]
    fn a_pack_with_errors_is_not_loaded_at_all_and_nothing_is_sent() {
        let spy = Spy::ready();
        let broken = Scripted {
            errors: 2,
            pack: Some(a_pack(vec![value("first", "ab")])),
        };
        let outcome = send_value(&Shelf, &broken, &spy, "sample", 1);
        assert_eq!(outcome, SendOutcome::Refused { errors: 2 });
        assert!(
            spy.seen.borrow().is_empty(),
            "pack-format.md 11 loads a pack whole or not at all - a refused pack sends nothing"
        );
    }

    #[test]
    fn a_partial_delivery_is_never_reported_as_a_send() {
        let spy = Spy {
            available: Availability::Ready,
            // The expected count is deliberately wrong here: this layer fills it
            // in, so both halves of the bad news come from one place.
            fail_with: Some(DeliveryError::Partial {
                units_sent: 1,
                units_expected: 0,
            }),
            seen: std::cell::RefCell::new(Vec::new()),
        };
        let outcome = send_value(&Shelf, &Scripted::two(), &spy, "sample", 2);
        let SendOutcome::NotDelivered { error } = outcome else {
            panic!("a partial delivery must not look like a send, got {outcome:?}");
        };
        assert_eq!(
            error,
            DeliveryError::Partial {
                units_sent: 1,
                units_expected: 3
            }
        );
    }
}
