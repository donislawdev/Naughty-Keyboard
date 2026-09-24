//! The facts of a report block: what a tester pastes into a ticket.
//!
//! `ux-spec.md` 7 draws the block and `D68` fixes its shape. This module holds
//! the part of it that is FACT - which value, from which pack version, typed as
//! what, how large, and whether all of it arrived. The words beside the facts
//! (`Value:`, `Size:`, the plural of "byte") are text, and untouchable rule 9
//! keeps text one layer out, in `nkb-adapters::report_text`.
//!
//! # Built from the recipe, never from what the palette drew
//!
//! The palette shows a value with invisible characters replaced by a marker and
//! long values cut at both ends. None of that may reach a ticket: the block is
//! read by somebody who has to REPRODUCE the value, and a marker or an ellipsis
//! is exactly the thing they cannot type back. So [`Typed`] carries the escaped
//! form `pack-format.md` 2 defines - the one `nkb show` prints - and a generated
//! value as its recipe, which is exact at every length.
//!
//! # Spelled out when a tracker could rewrite what `Typed` says
//!
//! The escaped form keeps visible characters as themselves - `e` followed by a
//! combining acute, fullwidth and mathematical letters. A tracker that
//! normalizes what it stores to NFC or NFKC rewrites exactly those, silently,
//! which is the very class of defect this tool finds in other people's software
//! (`OBS-123`). For such a value the block also carries [`SpelledOut`]: the same
//! value as code points, which no normalization touches (`D70`).
//!
//! # Measured with the same functions as the palette
//!
//! The counts come from [`ValueBody::metrics`], [`crate::graphemes::count`] and
//! [`crate::preview::shape`] - the three the palette's numbers come from. Two
//! surfaces describing one value with two counting routines would sooner or
//! later disagree, and a ticket quoting a size the palette never showed is a
//! ticket nobody trusts.

use crate::normalization;
use crate::pack::{Pack, PackValue};
use crate::preview::{ShapeFact, shape};
use crate::text::EscapedText;
use crate::value::{ValueBody, ValueProblem};

/// Everything a report block states about one value that went out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportBlock {
    /// `pack-id/value-id`, the same shape `nkb emit` prints.
    pub reference: String,
    /// The pack's own version. Two years on, this is what lets a ticket be
    /// reproduced against the value it was about - `pack-format.md` 7.
    pub pack_version: String,
    pub name: String,
    pub typed: Typed,
    /// The value as code points - present only when [`Typed`] holds a
    /// character that normalization could rewrite on the way to a ticket.
    pub spelled_out: Option<SpelledOut>,
    /// What the value is made of, in the core's fixed order.
    pub shape: Vec<ShapeFact>,
    pub graphemes: usize,
    pub code_points: usize,
    pub bytes: usize,
    /// What the value usually breaks, from the pack. Optional there, so
    /// optional here - and the text layer says "not given" rather than dropping
    /// the line, because `D68` keeps the block's shape fixed.
    pub breaks: Option<String>,
    pub expected: Option<String>,
    pub arrival: Arrival,
}

/// The value as a person can type it back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Typed {
    /// A written value, escaped. May be empty: `value = ""` is a legal test
    /// value (`pack-format.md` 5), and the text layer names that case rather
    /// than printing a label with nothing after it.
    Written(EscapedText),
    /// A generated value: `unit` repeated `count` times, the unit escaped.
    Recipe { unit: EscapedText, count: u32 },
}

/// A value written as code points, in the same two shapes as [`Typed`].
///
/// Every character, never a cut: whether a long list is shortened is the text
/// layer's choice, the way the palette's note under the preview is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpelledOut {
    Written(Vec<char>),
    /// The unit's code points. The value is `count` of them in a row.
    Recipe {
        unit: Vec<char>,
        count: u32,
    },
}

/// How the value reached the field, and whether all of it did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arrival {
    /// Typed into the field whole.
    Whole,
    /// Delivery stopped part-way. The field holds a fragment, and a ticket that
    /// did not say so would describe a value the application never received.
    Interrupted {
        units_sent: usize,
        units_expected: usize,
    },
    /// Put on the clipboard whole, for the tester to paste (`D71`).
    ///
    /// Told apart from [`Arrival::Whole`] because an application that checks
    /// its input on every keystroke meets a paste as ONE event, and behaves
    /// differently - a ticket that did not say how the value got in would send
    /// the developer after a defect that typing never shows.
    OnClipboard,
}

impl ReportBlock {
    /// Describes `value` from `pack` as it went out.
    ///
    /// Materialises the value to count its graphemes, because grapheme clusters
    /// change at the joins of a repeated unit and cannot be counted from the
    /// recipe - `architektura.md` 2.1. The size is checked from the recipe
    /// FIRST, so a value too large to build is refused before anything is
    /// allocated for it.
    ///
    /// # Errors
    ///
    /// [`ValueProblem`] when the recipe describes more text than the format
    /// allows. A validated pack cannot reach it, and a caller says so rather than
    /// swallowing it.
    pub fn describe(
        pack: &Pack,
        value: &PackValue,
        arrival: Arrival,
    ) -> Result<Self, ValueProblem> {
        let metrics = value
            .body
            .metrics()
            .ok_or(ValueProblem::RepeatProductUnmeasurable)?;
        let literal = value.body.materialise()?;
        let typed = typed_of(&value.body);
        Ok(Self {
            reference: format!("{}/{}", pack.id, value.id),
            pack_version: pack.version.clone(),
            name: value.name.clone(),
            spelled_out: spelled_out(&value.body, &typed),
            typed,
            shape: shape(&literal),
            graphemes: crate::graphemes::count(&literal),
            code_points: metrics.code_points,
            bytes: metrics.bytes,
            breaks: value.breaks.clone(),
            expected: value.expect.clone(),
            arrival,
        })
    }
}

/// The typed form of a value body.
#[must_use]
pub fn typed_of(body: &ValueBody) -> Typed {
    match body {
        ValueBody::Literal(text) => Typed::Written(text.escape()),
        ValueBody::Repeat { unit, count } => Typed::Recipe {
            unit: unit.escape(),
            count: *count,
        },
    }
}

/// The value as code points, when its typed form could be rewritten on the way.
///
/// 🔴 Asked of the TYPED form, never of the value. `Typed` already escapes what
/// a person cannot see, and an escape is ASCII no normalization touches - asked
/// of the value, a no-break space would earn a line for a character the block
/// already wrote as an escape. Measured on `whitespace/nbsp-between-words`,
/// `D70`. A recipe's typed form shows its unit once, between plain quotes, so
/// the unit is what is asked.
fn spelled_out(body: &ValueBody, typed: &Typed) -> Option<SpelledOut> {
    let at_risk = match typed {
        Typed::Written(escaped) => normalization::may_change(escaped.as_str()),
        Typed::Recipe { unit, .. } => normalization::may_change(unit.as_str()),
    };
    if !at_risk {
        return None;
    }
    Some(match body {
        ValueBody::Literal(text) => SpelledOut::Written(text.as_str().chars().collect()),
        ValueBody::Repeat { unit, count } => SpelledOut::Recipe {
            unit: unit.as_str().chars().collect(),
            count: *count,
        },
    })
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;
    use crate::pack::Risk;
    use crate::text::LiteralText;

    fn value(id: &str, body: ValueBody) -> PackValue {
        PackValue {
            id: id.to_owned(),
            name: "A value".to_owned(),
            body,
            breaks: Some("Counters disagree.".to_owned()),
            expect: None,
            risk: None,
            fields: Vec::new(),
            tags: Vec::new(),
            source: None,
            since: None,
            shape: None,
            deprecated: false,
            replaced_by: None,
        }
    }

    fn pack() -> Pack {
        Pack {
            id: "whitespace".to_owned(),
            name: "Whitespace".to_owned(),
            description: String::new(),
            version: "1.0".to_owned(),
            updated: "2026-09-07".to_owned(),
            license: "CC-BY-4.0".to_owned(),
            authors: Vec::new(),
            language: "en".to_owned(),
            risk: Risk::Normal,
            tags: Vec::new(),
            fields: Vec::new(),
            source: None,
            values: Vec::new(),
            pairs: Vec::new(),
        }
    }

    fn literal(text: &str) -> ValueBody {
        ValueBody::Literal(LiteralText::new(text))
    }

    #[test]
    fn a_written_value_is_typed_escaped_never_as_the_palette_draws_it() {
        let block = ReportBlock::describe(
            &pack(),
            &value(
                "zero-width-space-x3",
                literal("ab\u{200B}\u{200B}\u{200B}cd"),
            ),
            Arrival::Whole,
        )
        .expect("a small literal describes");

        assert_eq!(block.reference, "whitespace/zero-width-space-x3");
        assert_eq!(block.pack_version, "1.0");
        assert_eq!(
            block.typed,
            Typed::Written(LiteralText::new("ab\u{200B}\u{200B}\u{200B}cd").escape())
        );
        let Typed::Written(escaped) = &block.typed else {
            panic!("a literal is typed as written");
        };
        assert_eq!(escaped.as_str(), "ab\\u200B\\u200B\\u200Bcd");
        assert_eq!(block.shape, vec![ShapeFact::ZeroWidth(3)]);
        assert_eq!(
            (block.graphemes, block.code_points, block.bytes),
            (7, 7, 13),
            "the numbers ux-spec.md 7 prints for this very value"
        );
        assert_eq!(block.breaks.as_deref(), Some("Counters disagree."));
        assert_eq!(block.expected, None, "absent in the pack, absent here");
        assert_eq!(block.spelled_out, None, "every character is escaped");
    }

    #[test]
    fn a_value_normalization_could_rewrite_is_spelled_out_in_code_points() {
        let block = ReportBlock::describe(
            &pack(),
            &value("combining-acute", literal("e\u{0301}")),
            Arrival::Whole,
        )
        .expect("a small literal describes");

        assert_eq!(
            block.spelled_out,
            Some(SpelledOut::Written(vec!['e', '\u{0301}']))
        );
    }

    #[test]
    fn a_character_the_typed_form_already_escapes_earns_no_spelling() {
        // The correction `D70` made to the first design: asked of the VALUE, a
        // no-break space answers No for NFKC and would earn the line. Asked of
        // the typed form, it is an escape - ASCII, which nothing rewrites.
        let text = "Jan\u{00A0}Kowalski";
        assert!(
            crate::normalization::may_change(text),
            "the value itself is one NFKC changes"
        );
        let block = ReportBlock::describe(&pack(), &value("nbsp", literal(text)), Arrival::Whole)
            .expect("a small literal describes");

        assert_eq!(block.spelled_out, None);
    }

    #[test]
    fn a_generated_value_is_spelled_out_as_its_recipe() {
        let body = ValueBody::Repeat {
            unit: LiteralText::new("e\u{0301}"),
            count: 3,
        };
        let block = ReportBlock::describe(&pack(), &value("accents", body), Arrival::Whole)
            .expect("three units describe");

        assert_eq!(
            block.spelled_out,
            Some(SpelledOut::Recipe {
                unit: vec!['e', '\u{0301}'],
                count: 3
            })
        );
    }

    #[test]
    fn a_generated_value_is_typed_as_its_recipe_with_the_unit_escaped() {
        let body = ValueBody::Repeat {
            unit: LiteralText::new("\u{200B}"),
            count: 1000,
        };
        let block = ReportBlock::describe(&pack(), &value("zw-1000", body), Arrival::Whole)
            .expect("a thousand units describe");

        assert_eq!(
            block.typed,
            Typed::Recipe {
                unit: LiteralText::new("\u{200B}").escape(),
                count: 1000
            }
        );
        assert_eq!(block.code_points, 1000);
        assert_eq!(block.bytes, 3000);
    }

    #[test]
    fn graphemes_are_counted_on_the_built_value_not_multiplied_from_the_unit() {
        // A regional indicator repeated three times is TWO clusters, not three:
        // the pairs form at the joins. A count taken from the recipe would say
        // three, and that is exactly why the value is built here.
        let body = ValueBody::Repeat {
            unit: LiteralText::new("\u{1F1F5}"),
            count: 3,
        };
        let block = ReportBlock::describe(&pack(), &value("flags", body), Arrival::Whole)
            .expect("three indicators describe");

        assert_eq!(block.code_points, 3);
        assert_eq!(block.graphemes, 2);
    }

    #[test]
    fn an_empty_value_is_typed_as_empty_rather_than_refused() {
        let block = ReportBlock::describe(&pack(), &value("empty", literal("")), Arrival::Whole)
            .expect("an empty value is a legal test value");

        assert_eq!(block.typed, Typed::Written(LiteralText::new("").escape()));
        assert_eq!((block.graphemes, block.code_points, block.bytes), (0, 0, 0));
        assert!(block.shape.is_empty());
    }

    #[test]
    fn an_interrupted_arrival_travels_with_the_block() {
        let arrival = Arrival::Interrupted {
            units_sent: 12,
            units_expected: 40,
        };
        let block = ReportBlock::describe(&pack(), &value("v", literal("abc")), arrival)
            .expect("a literal describes");

        assert_eq!(block.arrival, arrival);
    }

    #[test]
    fn a_recipe_too_large_to_build_is_refused_before_anything_is_built() {
        let body = ValueBody::Repeat {
            unit: LiteralText::new("ab"),
            count: 1_000_000,
        };

        assert!(
            ReportBlock::describe(&pack(), &value("huge", body), Arrival::Whole).is_err(),
            "two million code points is over the ceiling and must not be allocated"
        );
    }
}
