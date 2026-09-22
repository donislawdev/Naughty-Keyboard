//! Putting one value of one pack into whatever field has the focus.
//!
//! This is the use case behind `nkb send`, and the first one in the project that
//! reaches outside the process to something that is not a file.
//!
//! # Clearing the field first, and what that costs
//!
//! `ux-spec.md` 4: clearing is the ONLY place where the tool sends keystrokes
//! that are not the value, and it is done within the line - `Home`,
//! `Shift+End`, `Delete` - never with "select all", which in many applications
//! reaches past the field. The recipe is data in `nkb_core::keys`, the presses
//! go through `KeystrokeSender`, and this function is the one door through
//! which that port is called (architektura.md 5, guarded by
//! `tests/keystrokes_have_named_doors.rs`).
//!
//! The order matters and is fixed here: the value is BUILT before the field is
//! touched, so a value the format refuses (`E026`) refuses before a single key
//! goes out; then the clearing keys; then the value. If clearing does not go
//! through, the value is NOT sent - a half-cleared field with a whole value on
//! top of it is the worst of both outcomes, and the caller is told which half
//! happened.
//!
//! # What it still does NOT do
//!
//! - it does not ask before clearing a multi-line field. The recipe clears the
//!   current line only, which is the safe direction, and the question belongs
//!   to the palette (`ux-spec.md` 4, answer 2);
//! - it does not check what the target is. `product-spec.md` 10.1 requires the
//!   application name and window title before every insert, and that needs
//!   `TargetInspector` and the `WindowTitle` type - a separate piece, because
//!   the type is what keeps titles out of the log;
//! - it does not re-check the target mid-insert (race `W2`), does not handle
//!   `Escape` (`W3`), and writes nothing to a session file (`W4`).
//!
//! Every one of those belongs to a later piece of step 3, and saying which
//! parts are missing is untouchable rule 1 applied to a use case.

use crate::ports::{
    Availability, DeliveryError, KeystrokeError, KeystrokeSender, PackFormat, PackSource,
    SourceError, ValueDelivery,
};
use nkb_core::keys::line_clearing_recipe;
use nkb_core::pack::{Pack, PackValue};
use nkb_core::preview::{Preview, ShapeFact, preview, shape};
use nkb_core::value::ValueProblem;

/// Whether the field is cleared before the value goes in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Clearing {
    /// Send the value on top of whatever the field holds. No key other than
    /// the value's own characters is pressed.
    Keep,
    /// `Home`, `Shift+End`, `Delete`, then the value. Clears the current line
    /// of the field and never reaches beyond it.
    Line,
}

/// What to send: which value of which pack, and whether to clear first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendRequest<'a> {
    pub pack_id: &'a str,
    /// Counted from one, in the pack's own order - see [`send_value`].
    pub position: usize,
    pub clearing: Clearing,
}

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
        /// Whether the clearing keys went out before the value.
        cleared: bool,
        /// The value as a person can see it, invisible characters substituted.
        /// `ux-spec.md` 2 requires this line and `nkb_core::preview` builds it.
        preview: Preview,
        /// What the value is made of, as facts rather than as a sentence.
        shape: Vec<ShapeFact>,
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
    /// Clearing was asked for and did not go through. The value was NOT sent.
    NotCleared { error: KeystrokeError },
    /// There is a route, and the value did not arrive, or not all of it.
    NotDelivered {
        error: DeliveryError,
        /// Whether the field had already been cleared when this happened -
        /// the tester needs to know the field is empty-plus-fragment, not
        /// old-content-plus-fragment.
        cleared: bool,
    },
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
    keys: &dyn KeystrokeSender,
    request: &SendRequest<'_>,
) -> SendOutcome {
    // Asked FIRST, before reading anything. A machine with no route should say
    // so rather than spend the work and fail at the last step - and on macOS
    // that is not hypothetical: OBS-70 measured that the system refuses the
    // call outright without a permission no script can grant.
    if let Availability::Unavailable { reason } = delivery.availability() {
        return SendOutcome::RouteUnavailable { reason };
    }

    let text = match source.read(request.pack_id) {
        Ok(text) => text,
        Err(SourceError::NotFound) => return SendOutcome::NotFound,
        Err(SourceError::Unreadable | SourceError::NotUtf8) => return SendOutcome::Unreadable,
    };

    // check first, refuse on any error, parse after - pack-format.md 11 and the
    // binding order in architektura.md 3, both of which live in `load_pack`.
    match crate::load_pack::load(format, request.pack_id, &text) {
        Ok(loaded) => deliver_one(&loaded.pack, delivery, keys, request, loaded.warnings),
        Err(refused) => SendOutcome::Refused {
            errors: refused.errors,
        },
    }
}

fn deliver_one(
    pack: &Pack,
    delivery: &dyn ValueDelivery,
    keys: &dyn KeystrokeSender,
    request: &SendRequest<'_>,
    warnings: usize,
) -> SendOutcome {
    let available = pack.values.len();
    let position = request.position;
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

    deliver_value(pack, value, delivery, keys, request.clearing, warnings)
}

/// Delivers one value that is ALREADY in memory, and reports what happened.
///
/// # Why this is public and separate from [`send_value`]
///
/// [`send_value`] reads the pack from a source every time it runs, which is
/// right for `nkb send`, a one-shot command. The palette's core loop is the
/// opposite: it loads a pack once and sends many values from it, because `W5`
/// (architektura.md 6a) requires the value shown to the tester to be the very
/// value that was sent - a re-read between the two could differ if the file
/// changed underneath. So `AdvanceSequence` holds the pack and calls THIS, which
/// takes the value it already has rather than an index to look up.
///
/// The clearing keys go out before the value; if clearing does not go through,
/// the value is not sent, and the outcome says which half happened. The returned
/// [`SendOutcome`] carries both the delivery result and the numbers the palette
/// shows, so one call answers both questions.
pub fn deliver_value(
    pack: &Pack,
    value: &PackValue,
    delivery: &dyn ValueDelivery,
    keys: &dyn KeystrokeSender,
    clearing: Clearing,
    warnings: usize,
) -> SendOutcome {
    // Measured from the RECIPE, before anything is built - architektura.md 6.1.
    // A value declaring two billion characters is refused here without a single
    // byte being allocated for it - and, from today, without a single key
    // having been pressed in somebody else's field.
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

    // The one door. Clearing that does not go through stops everything: the
    // value must not land on a field in an unknown state.
    let cleared = match clearing {
        Clearing::Keep => false,
        Clearing::Line => match keys.send_keystrokes(&line_clearing_recipe()) {
            Ok(()) => true,
            Err(error) => return SendOutcome::NotCleared { error },
        },
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
            cleared,
            // Built from the LITERAL, which is what actually went out - not from
            // the written form, which is escaped. Showing the escaped form would
            // answer a different question, and `nkb emit` already answers it.
            preview: preview(&literal),
            shape: shape(&literal),
        },
        Err(DeliveryError::Partial { units_sent, .. }) => SendOutcome::NotDelivered {
            // Rebuilt with the count this layer knows, so the two halves of the
            // bad news come from the same place and cannot disagree.
            error: DeliveryError::Partial {
                units_sent,
                units_expected: expected_units,
            },
            cleared,
        },
        Err(error) => SendOutcome::NotDelivered { error, cleared },
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
    use nkb_core::keys::KeyChord;
    use nkb_core::lint::{LintProblem, RuleCode};
    use nkb_core::pack::Risk;
    use nkb_core::text::LiteralText;
    use nkb_core::value::ValueBody;
    use std::cell::RefCell;
    use std::rc::Rc;

    /// One log shared by both spies, so a test can assert the ORDER in which
    /// keys and text went out - which is the property that matters here.
    type Log = Rc<RefCell<Vec<String>>>;

    /// Records what it was handed, and can be told how to answer.
    struct Spy {
        available: Availability,
        fail_with: Option<DeliveryError>,
        log: Log,
    }

    impl Spy {
        fn ready(log: &Log) -> Self {
            Self {
                available: Availability::Ready,
                fail_with: None,
                log: Rc::clone(log),
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
            self.log.borrow_mut().push(format!("text:{text}"));
            match &self.fail_with {
                None => Ok(Delivered {
                    utf16_units: text.encode_utf16().count(),
                }),
                Some(error) => Err(error.clone()),
            }
        }
    }

    struct KeySpy {
        fail_with: Option<KeystrokeError>,
        log: Log,
    }

    impl KeySpy {
        fn working(log: &Log) -> Self {
            Self {
                fail_with: None,
                log: Rc::clone(log),
            }
        }
    }

    impl KeystrokeSender for KeySpy {
        fn send_keystrokes(&self, chords: &[KeyChord]) -> Result<(), KeystrokeError> {
            let described: Vec<String> = chords
                .iter()
                .map(|chord| format!("{}{:?}", if chord.shift { "Shift+" } else { "" }, chord.key))
                .collect();
            self.log
                .borrow_mut()
                .push(format!("keys:{}", described.join(",")));
            match &self.fail_with {
                None => Ok(()),
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

    fn request(position: usize, clearing: Clearing) -> SendRequest<'static> {
        SendRequest {
            pack_id: "sample",
            position,
            clearing,
        }
    }

    fn log() -> Log {
        Rc::new(RefCell::new(Vec::new()))
    }

    #[test]
    fn positions_start_at_one_because_the_palette_counts_from_one() {
        let log = log();
        let spy = Spy::ready(&log);
        let keys = KeySpy::working(&log);
        let outcome = send_value(
            &Shelf,
            &Scripted::two(),
            &spy,
            &keys,
            &request(1, Clearing::Keep),
        );
        assert!(
            matches!(outcome, SendOutcome::Sent { .. }),
            "position 1 must be the first value, got {outcome:?}"
        );
        assert_eq!(log.borrow().as_slice(), ["text:ab"]);
    }

    #[test]
    fn keeping_the_field_presses_no_key_at_all() {
        // The negative promise in its plainest form: without clearing, the
        // only thing that goes out is the value's own characters.
        let log = log();
        let spy = Spy::ready(&log);
        let keys = KeySpy::working(&log);
        let outcome = send_value(
            &Shelf,
            &Scripted::two(),
            &spy,
            &keys,
            &request(1, Clearing::Keep),
        );
        let SendOutcome::Sent { cleared, .. } = outcome else {
            panic!("expected a send, got {outcome:?}");
        };
        assert!(!cleared);
        assert!(
            log.borrow().iter().all(|entry| entry.starts_with("text:")),
            "no keys may be pressed when the field is kept, got {:?}",
            log.borrow()
        );
    }

    #[test]
    fn clearing_presses_the_line_recipe_before_the_value_and_nothing_else() {
        let log = log();
        let spy = Spy::ready(&log);
        let keys = KeySpy::working(&log);
        let outcome = send_value(
            &Shelf,
            &Scripted::two(),
            &spy,
            &keys,
            &request(1, Clearing::Line),
        );
        let SendOutcome::Sent { cleared, .. } = outcome else {
            panic!("expected a send, got {outcome:?}");
        };
        assert!(cleared);
        // Order is the property: keys first, exactly the recipe, then the text.
        assert_eq!(
            log.borrow().as_slice(),
            ["keys:Home,Shift+End,Delete", "text:ab"]
        );
    }

    #[test]
    fn a_held_modifier_refuses_the_clearing_and_the_value_stays_unsent() {
        // A half-cleared field with a whole value on top is the worst of both
        // outcomes, so a refused clearing stops everything and says which half.
        let log = log();
        let spy = Spy::ready(&log);
        let keys = KeySpy {
            fail_with: Some(KeystrokeError::ModifierHeld {
                which: "Ctrl".to_owned(),
            }),
            log: Rc::clone(&log),
        };
        let outcome = send_value(
            &Shelf,
            &Scripted::two(),
            &spy,
            &keys,
            &request(1, Clearing::Line),
        );
        assert_eq!(
            outcome,
            SendOutcome::NotCleared {
                error: KeystrokeError::ModifierHeld {
                    which: "Ctrl".to_owned()
                }
            }
        );
        assert!(
            !log.borrow().iter().any(|entry| entry.starts_with("text:")),
            "the value must not be sent after a refused clearing, got {:?}",
            log.borrow()
        );
    }

    #[test]
    fn a_value_the_format_refuses_touches_the_field_not_at_all() {
        // E026 refuses before allocation - and now before any key: a refused
        // value must leave the field exactly as it was, cleared or not.
        let log = log();
        let spy = Spy::ready(&log);
        let keys = KeySpy::working(&log);
        let bomb = Scripted {
            errors: 0,
            pack: Some(a_pack(vec![PackValue {
                body: ValueBody::Repeat {
                    unit: LiteralText::new("ab".to_owned()),
                    count: 2_000_000,
                },
                ..value("bomb", "")
            }])),
        };
        let outcome = send_value(&Shelf, &bomb, &spy, &keys, &request(1, Clearing::Line));
        assert!(
            matches!(outcome, SendOutcome::ValueTooLarge { .. }),
            "expected a refusal, got {outcome:?}"
        );
        assert!(
            log.borrow().is_empty(),
            "a refused value must press no key and send no text, got {:?}",
            log.borrow()
        );
    }

    #[test]
    fn position_zero_is_refused_rather_than_treated_as_the_first() {
        let log = log();
        let spy = Spy::ready(&log);
        let keys = KeySpy::working(&log);
        let outcome = send_value(
            &Shelf,
            &Scripted::two(),
            &spy,
            &keys,
            &request(0, Clearing::Line),
        );
        assert_eq!(
            outcome,
            SendOutcome::NoSuchIndex {
                asked: 0,
                available: 2
            }
        );
        assert!(
            log.borrow().is_empty(),
            "nothing may be pressed or delivered for a position that does not exist"
        );
    }

    #[test]
    fn a_position_past_the_end_says_how_many_there_are() {
        let log = log();
        let spy = Spy::ready(&log);
        let keys = KeySpy::working(&log);
        let outcome = send_value(
            &Shelf,
            &Scripted::two(),
            &spy,
            &keys,
            &request(9, Clearing::Keep),
        );
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
        let log = log();
        let spy = Spy::ready(&log);
        let keys = KeySpy::working(&log);
        let outcome = send_value(
            &Shelf,
            &Scripted::two(),
            &spy,
            &keys,
            &request(2, Clearing::Keep),
        );
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
        let log = log();
        let spy = Spy {
            available: Availability::Unavailable {
                reason: "macOS".to_owned(),
            },
            fail_with: None,
            log: Rc::clone(&log),
        };
        let keys = KeySpy::working(&log);
        let outcome = send_value(
            &Exploding,
            &Scripted::two(),
            &spy,
            &keys,
            &request(1, Clearing::Line),
        );
        assert_eq!(
            outcome,
            SendOutcome::RouteUnavailable {
                reason: "macOS".to_owned()
            }
        );
        assert!(log.borrow().is_empty(), "no route means no key and no text");
    }

    #[test]
    fn a_pack_with_errors_is_not_loaded_at_all_and_nothing_is_sent() {
        let log = log();
        let spy = Spy::ready(&log);
        let keys = KeySpy::working(&log);
        let broken = Scripted {
            errors: 2,
            pack: Some(a_pack(vec![value("first", "ab")])),
        };
        let outcome = send_value(&Shelf, &broken, &spy, &keys, &request(1, Clearing::Line));
        assert_eq!(outcome, SendOutcome::Refused { errors: 2 });
        assert!(
            log.borrow().is_empty(),
            "pack-format.md 11 loads a pack whole or not at all - a refused pack sends nothing"
        );
    }

    #[test]
    fn a_partial_delivery_is_never_reported_as_a_send_and_says_whether_the_field_was_cleared() {
        let log = log();
        let spy = Spy {
            available: Availability::Ready,
            // The expected count is deliberately wrong here: this layer fills it
            // in, so both halves of the bad news come from one place.
            fail_with: Some(DeliveryError::Partial {
                units_sent: 1,
                units_expected: 0,
            }),
            log: Rc::clone(&log),
        };
        let keys = KeySpy::working(&log);
        let outcome = send_value(
            &Shelf,
            &Scripted::two(),
            &spy,
            &keys,
            &request(2, Clearing::Line),
        );
        let SendOutcome::NotDelivered { error, cleared } = outcome else {
            panic!("a partial delivery must not look like a send, got {outcome:?}");
        };
        assert_eq!(
            error,
            DeliveryError::Partial {
                units_sent: 1,
                units_expected: 3
            }
        );
        assert!(
            cleared,
            "the tester must learn the field is empty-plus-fragment, not old-plus-fragment"
        );
    }
}
