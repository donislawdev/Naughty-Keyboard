//! Taking the catalogue out of the tool: the use case behind `nkb emit`.
//!
//! # Why this command exists at all
//!
//! `product-spec.md` 8.5: the catalogue lives inside the tool's repository, so
//! it does not become a citable resource by virtue of where it sits. One command
//! that prints it in a usable shape does that job instead, without a second
//! repository to keep in step. Everything here serves somebody who is not going
//! to run the palette.
//!
//! # This is the one place a recipe becomes text
//!
//! `architektura.md` 6.1 keeps a generated value as a recipe everywhere else,
//! because the size has to be known before the text exists. `emit` is the
//! separate, explicit operation that rule allows for, and it is explicit here:
//! the size of everything is counted from the recipes FIRST, into
//! [`Emission::code_points`], and only then is anything built.
//!
//! That ordering is what lets the command line print an account of what is
//! coming before it comes. A warning that arrives after the memory is gone is
//! not a warning.
//!
//! # What this does NOT do
//!
//! It does not render. JSON, CSV and one-value-per-line are three public
//! contracts with three different answers about escaping, and choosing between
//! them is a decision about what a person or a script reads - which belongs one
//! layer out, beside every other such decision. This hands over the values and
//! the facts about them.

use nkb_core::pack::{Pack, Risk};
use nkb_core::value::{ValueBody, ValueProblem};

use crate::ports::{PackFormat, PackSource, SourceError};

/// One value, materialised, with everything the JSON contract names.
///
/// Both forms of the text are carried rather than one, because the three output
/// formats disagree about which they want and recomputing the escaped form in
/// two places is how the two would drift apart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmittedValue {
    /// The pack this came from.
    pub pack: String,
    /// `pack-id/value-id`, the form that identifies a value from outside.
    ///
    /// Deliberately not a value identifier on its own: those are unique only
    /// within a pack, and something printed for other people's scripts has to
    /// stay unique in the only scope they have, which is all of the catalogue.
    /// `D28` already settled that this spelling names nothing inside the format,
    /// so it cannot be confused with a reference the validator resolves.
    pub reference: String,
    pub id: String,
    pub name: String,
    /// The text as it would arrive in a field.
    pub literal: String,
    /// The same text in the form a pack file stores.
    pub escaped: String,
    pub breaks: Option<String>,
    pub expect: Option<String>,
    pub fields: Vec<String>,
    pub tags: Vec<String>,
    /// The risk that applies, the pack's when the value declares none.
    pub risk: Risk,
    pub since: Option<String>,
    /// The attribution that applies, the pack's when the value declares none.
    pub source: Option<String>,
    /// True when the text was built from a recipe rather than written out.
    ///
    /// Carried because it changes what a warning about size means: a hundred
    /// thousand characters from a one-line recipe is the case the length-bomb
    /// warning is about, and the same count of literal text is not.
    pub generated: bool,
}

/// Everything one run of `emit` produced, and what it cost.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Emission {
    pub values: Vec<EmittedValue>,
    /// Total size, counted from the recipes BEFORE anything was built.
    ///
    /// 🔴 The number the account on the error stream is made of. Counted first
    /// on purpose - taking it from the built strings afterwards would report the
    /// size of a thing that had already been held in memory, which answers the
    /// question too late to be of use.
    pub code_points: usize,
    pub bytes: usize,
    /// Warnings the pack carries. It still loads - `pack-format.md` 11 makes
    /// them the one exception to all-or-nothing - and staying silent about them
    /// would let a warned pack look identical to a clean one.
    pub warnings: usize,
}

/// What came of asking for a pack's values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmitOutcome {
    Emitted(Box<Emission>),
    /// Present and refused. A pack is whole or absent, so nothing is printed -
    /// half a catalogue on standard output is worse than none, because a script
    /// cannot tell it from a complete one.
    Refused {
        errors: usize,
    },
    NotFound,
    Unreadable,
    /// A value passed validation and could not be built anyway.
    ///
    /// 🔴 Should be unreachable: `check` runs first and the size rules refuse
    /// anything above the ceiling. It is a variant rather than an `unwrap`
    /// because the two halves of the format disagreeing is exactly the fault
    /// this project keeps finding, and the honest answer to it is a refusal that
    /// names the value - never a panic in front of somebody's pipeline, and
    /// never a quietly shorter list.
    ValueTooLarge {
        id: String,
        code: &'static str,
    },
}

/// Every value of one pack, materialised.
///
/// Takes a [`PackSource`] rather than a catalogue: this reads one pack by name
/// and never asks what else exists.
#[must_use]
pub fn emit_values(source: &dyn PackSource, format: &dyn PackFormat, id: &str) -> EmitOutcome {
    let text = match source.read(id) {
        Ok(text) => text,
        Err(SourceError::NotFound) => return EmitOutcome::NotFound,
        Err(SourceError::Unreadable | SourceError::NotUtf8) => return EmitOutcome::Unreadable,
    };

    // check first, refuse on any error, parse after - pack-format.md 11.
    // The order lives in `load_pack` rather than here, so that it is one rule
    // rather than one rule per use case.
    match crate::load_pack::load(format, id, &text) {
        Ok(loaded) => build(&loaded.pack, loaded.warnings),
        // Reported as a refusal rather than as an empty catalogue, for the same
        // reason the listing does it: a pack must never vanish quietly.
        Err(refused) => EmitOutcome::Refused {
            errors: refused.errors,
        },
    }
}

/// Counts everything from the recipes, then builds. Never the other way round.
fn build(pack: &Pack, warnings: usize) -> EmitOutcome {
    let mut code_points = 0usize;
    let mut bytes = 0usize;
    for value in &pack.values {
        let Some(metrics) = value.body.metrics() else {
            return EmitOutcome::ValueTooLarge {
                id: value.id.clone(),
                code: ValueProblem::RepeatProductUnmeasurable.code(),
            };
        };
        code_points = code_points.saturating_add(metrics.code_points);
        bytes = bytes.saturating_add(metrics.bytes);
    }

    let mut values = Vec::with_capacity(pack.values.len());
    for value in &pack.values {
        let literal = match value.body.materialise() {
            Ok(text) => text,
            Err(problem) => {
                return EmitOutcome::ValueTooLarge {
                    id: value.id.clone(),
                    code: problem.code(),
                };
            }
        };
        // Escaped from the materialised text rather than copied out of the file,
        // so it describes the value instead of its author's spelling - the same
        // rule `nkb show` follows.
        let escaped = nkb_core::text::LiteralText::new(literal.clone())
            .escape()
            .as_str()
            .to_owned();

        values.push(EmittedValue {
            pack: pack.id.clone(),
            reference: format!("{}/{}", pack.id, value.id),
            id: value.id.clone(),
            name: value.name.clone(),
            literal,
            escaped,
            breaks: value.breaks.clone(),
            expect: value.expect.clone(),
            fields: pack.fields_of(value).to_vec(),
            tags: value.tags.clone(),
            risk: pack.risk_of(value),
            since: value.since.clone(),
            source: pack.source_of(value).map(ToOwned::to_owned),
            generated: matches!(value.body, ValueBody::Repeat { .. }),
        });
    }

    EmitOutcome::Emitted(Box::new(Emission {
        values,
        code_points,
        bytes,
        warnings,
    }))
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;
    use crate::ports::{Date, TranslationCheck, TranslationTarget};
    use nkb_core::lint::{LintProblem, RuleCode};
    use nkb_core::pack::{PackValue, Risk};
    use nkb_core::text::LiteralText;
    use std::collections::BTreeMap;

    struct Shelf(BTreeMap<String, Result<String, SourceError>>);

    impl Shelf {
        fn of(id: &str) -> Self {
            Self(BTreeMap::from([(
                id.to_owned(),
                Ok("whatever, the format is scripted".to_owned()),
            )]))
        }
    }

    impl PackSource for Shelf {
        fn read(&self, id: &str) -> Result<String, SourceError> {
            self.0
                .get(id)
                .cloned()
                .unwrap_or(Err(SourceError::NotFound))
        }
    }

    fn value(id: &str, body: ValueBody) -> PackValue {
        PackValue {
            id: id.to_owned(),
            name: format!("Value {id}"),
            body,
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
            source: Some("https://example.invalid/the-pack".to_owned()),
            values,
            pairs: Vec::new(),
        }
    }

    struct Scripted {
        errors: usize,
        warnings: usize,
        pack: Option<Pack>,
    }

    impl PackFormat for Scripted {
        fn check(&self, _text: &str, _expected_id: &str) -> Vec<LintProblem> {
            let mut problems = Vec::new();
            for _ in 0..self.errors {
                problems.push(LintProblem::new(RuleCode::PackWithoutValues));
            }
            for _ in 0..self.warnings {
                problems.push(LintProblem::new(RuleCode::PackTooLarge));
            }
            problems
        }
        fn translated_pack(&self, _text: &str) -> TranslationTarget {
            TranslationTarget::NotATranslation
        }
        fn check_translation(&self, _text: &str, _translated: &str) -> TranslationCheck {
            TranslationCheck::Compared(Vec::new())
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
        fn same_insertions(&self, _before: &str, _after: &str) -> bool {
            true
        }
    }

    fn scripted(pack: Pack) -> Scripted {
        Scripted {
            errors: 0,
            warnings: 0,
            pack: Some(pack),
        }
    }

    fn emitted(outcome: EmitOutcome) -> Emission {
        match outcome {
            EmitOutcome::Emitted(emission) => *emission,
            other => panic!("expected values, got {other:?}"),
        }
    }

    #[test]
    fn a_literal_value_comes_out_in_both_forms() {
        let pack = a_pack(vec![value(
            "zero-width",
            ValueBody::Literal(LiteralText::new("a\u{200B}b")),
        )]);
        let emission = emitted(emit_values(&Shelf::of("sample"), &scripted(pack), "sample"));

        let first = emission.values.first().expect("one value");
        assert_eq!(first.literal, "a\u{200B}b");
        assert_eq!(
            first.escaped, "a\\u200Bb",
            "the escaped form is what makes an invisible value readable at all"
        );
        assert!(!first.generated);
    }

    #[test]
    fn a_generated_value_is_materialised_here_and_marked_as_generated() {
        // The one place a recipe becomes text. Everywhere else it stays a recipe.
        let pack = a_pack(vec![value(
            "many-spaces",
            ValueBody::Repeat {
                unit: LiteralText::new("\u{0020}"),
                count: 1000,
            },
        )]);
        let emission = emitted(emit_values(&Shelf::of("sample"), &scripted(pack), "sample"));

        let first = emission.values.first().expect("one value");
        assert_eq!(first.literal.chars().count(), 1000);
        assert!(first.generated, "a recipe must stay distinguishable");
    }

    #[test]
    fn the_size_is_counted_from_the_recipes_and_matches_what_was_built() {
        // 🔴 The property the account on the error stream rests on. The number
        // is taken before anything is built, so it has to agree with what comes
        // out - otherwise the warning describes a different run than the one
        // that happens.
        let pack = a_pack(vec![
            value("literal", ValueBody::Literal(LiteralText::new("abc"))),
            value(
                "recipe",
                ValueBody::Repeat {
                    unit: LiteralText::new("\u{0105}"),
                    count: 10,
                },
            ),
        ]);
        let emission = emitted(emit_values(&Shelf::of("sample"), &scripted(pack), "sample"));

        let built: usize = emission
            .values
            .iter()
            .map(|v| v.literal.chars().count())
            .sum();
        let built_bytes: usize = emission.values.iter().map(|v| v.literal.len()).sum();
        assert_eq!(emission.code_points, built, "3 + 10");
        assert_eq!(
            emission.bytes, built_bytes,
            "3 + 20, because ą is two bytes"
        );
    }

    #[test]
    fn a_reference_names_the_pack_and_the_value() {
        let pack = a_pack(vec![value(
            "trailing-space",
            ValueBody::Literal(LiteralText::new("x")),
        )]);
        let emission = emitted(emit_values(&Shelf::of("sample"), &scripted(pack), "sample"));
        assert_eq!(
            emission.values.first().expect("one value").reference,
            "sample/trailing-space",
            "a value identifier alone is unique only inside its pack"
        );
    }

    #[test]
    fn inherited_risk_source_and_fields_arrive_resolved() {
        // A consumer of the JSON has no pack header to look at, so what applies
        // has to be worked out here rather than left to them.
        let mut declaring = value("own", ValueBody::Literal(LiteralText::new("a")));
        declaring.risk = Some(Risk::Offensive);
        declaring.source = Some("https://example.invalid/this-value".to_owned());
        let inheriting = value("inherited", ValueBody::Literal(LiteralText::new("b")));
        let pack = a_pack(vec![declaring, inheriting]);

        let emission = emitted(emit_values(&Shelf::of("sample"), &scripted(pack), "sample"));
        let own = emission.values.first().expect("two values");
        let inherited = emission.values.get(1).expect("two values");

        assert_eq!(own.risk, Risk::Offensive);
        assert_eq!(
            own.source.as_deref(),
            Some("https://example.invalid/this-value")
        );
        assert_eq!(inherited.risk, Risk::Normal);
        assert_eq!(
            inherited.source.as_deref(),
            Some("https://example.invalid/the-pack"),
            "a value that declares no source takes the pack's attribution"
        );
        assert_eq!(inherited.fields, ["any"]);
    }

    #[test]
    fn one_error_prints_nothing_at_all() {
        // Half a catalogue on standard output is worse than none: a script
        // cannot tell it from a complete one.
        let format = Scripted {
            errors: 2,
            warnings: 0,
            pack: Some(a_pack(vec![value(
                "x",
                ValueBody::Literal(LiteralText::new("a")),
            )])),
        };
        assert_eq!(
            emit_values(&Shelf::of("sample"), &format, "sample"),
            EmitOutcome::Refused { errors: 2 }
        );
    }

    #[test]
    fn warnings_do_not_stop_the_values_and_are_counted() {
        let format = Scripted {
            errors: 0,
            warnings: 3,
            pack: Some(a_pack(vec![value(
                "x",
                ValueBody::Literal(LiteralText::new("a")),
            )])),
        };
        let emission = emitted(emit_values(&Shelf::of("sample"), &format, "sample"));
        assert_eq!(emission.warnings, 3);
        assert_eq!(emission.values.len(), 1);
    }

    #[test]
    fn an_unknown_pack_says_so_rather_than_printing_an_empty_list() {
        let pack = a_pack(Vec::new());
        assert_eq!(
            emit_values(&Shelf::of("sample"), &scripted(pack), "no-such-pack"),
            EmitOutcome::NotFound
        );
    }

    #[test]
    fn a_pack_that_checks_clean_and_will_not_parse_is_refused() {
        let format = Scripted {
            errors: 0,
            warnings: 0,
            pack: None,
        };
        assert_eq!(
            emit_values(&Shelf::of("sample"), &format, "sample"),
            EmitOutcome::Refused { errors: 1 }
        );
    }

    #[test]
    fn a_value_above_the_ceiling_is_named_rather_than_dropped_or_panicked_on() {
        // Unreachable through `check`, and handled anyway: the two halves of the
        // format disagreeing is the fault this project keeps finding, and a
        // quietly shorter list is the worst possible answer to it.
        let unit = "a".repeat(2000);
        let pack = a_pack(vec![value(
            "bomb",
            ValueBody::Repeat {
                unit: LiteralText::new(unit),
                count: 1_000_000,
            },
        )]);
        match emit_values(&Shelf::of("sample"), &scripted(pack), "sample") {
            EmitOutcome::ValueTooLarge { id, code } => {
                assert_eq!(id, "bomb");
                assert_eq!(code, "E026");
            }
            other => panic!("expected the bomb to be named, got {other:?}"),
        }
    }

    #[test]
    fn a_pack_with_no_values_emits_nothing_and_is_not_an_error() {
        // Legal but degenerate. `E00x` refuses such a pack today, so this is
        // about not stumbling rather than about a case anyone will meet.
        let emission = emitted(emit_values(
            &Shelf::of("sample"),
            &scripted(a_pack(Vec::new())),
            "sample",
        ));
        assert!(emission.values.is_empty());
        assert_eq!(emission.code_points, 0);
    }
}
