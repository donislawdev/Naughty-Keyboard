//! What a single catalogue value is, and how big it claims to be.
//!
//! A value is a **recipe**, not a piece of text. That distinction is load
//! bearing: a generated value can describe a million characters while occupying
//! one line of a pack file, and the tool has to know its size before producing
//! it. Code that builds the text in order to measure it breaks the promise of
//! warning about a length bomb, because by then the bomb has already gone off
//! inside the tool.

use crate::metrics::TextMetrics;
use crate::text::LiteralText;

/// Largest value a pack may describe, in code points.
///
/// Not a fresh number: it is the same ceiling the format already puts on the
/// repeat count, carrying the same reasoning - above this, inserting takes
/// longer than a tester will wait, and the value stops testing validation and
/// starts testing performance, which is somebody else's subject.
///
/// The unit is code points on purpose. Bytes would separate values a tester
/// sees as equally long - a million `a` would pass where a million `ą` would
/// not - and the difference between characters and bytes is the *subject* of
/// this catalogue, not a hidden rule inside it.
pub const MAX_VALUE_CODE_POINTS: usize = 1_000_000;

/// Largest repeat count the format accepts.
pub const MAX_REPEAT_COUNT: u32 = 1_000_000;

/// The body of a value: either text, or a recipe for text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValueBody {
    /// Text written out in the pack file.
    Literal(LiteralText),
    /// A unit repeated `count` times. Never stored expanded.
    Repeat { unit: LiteralText, count: u32 },
}

/// Why a value cannot be accepted. Carries the validator code from the moment
/// it is discovered, so no layer above has to guess it back from a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueProblem {
    /// `E024` - repeat count outside 1..=1_000_000.
    RepeatCountOutOfRange { count: u32 },
    /// `E026` - `len(unit) * count` exceeds the ceiling, in code points.
    RepeatProductTooLarge { code_points: usize },
    /// `E026` - the product does not even fit in machine arithmetic.
    RepeatProductUnmeasurable,
}

impl ValueProblem {
    /// The published rule code. These are a public contract: they travel into
    /// `--json` output and other people's pipelines, so they are never renamed.
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::RepeatCountOutOfRange { .. } => "E024",
            Self::RepeatProductTooLarge { .. } | Self::RepeatProductUnmeasurable => "E026",
        }
    }
}

impl ValueBody {
    /// Size of this value if it were materialised, computed from the recipe.
    ///
    /// Returns `None` only when the product overflows machine arithmetic, which
    /// the validator treats as out of range rather than as a crash.
    #[must_use]
    pub fn metrics(&self) -> Option<TextMetrics> {
        match self {
            Self::Literal(text) => Some(TextMetrics::of(text.as_str())),
            Self::Repeat { unit, count } => TextMetrics::of_repeated(unit.as_str(), *count),
        }
    }

    /// Builds the text this recipe describes.
    ///
    /// # The separate, explicit operation
    ///
    /// `architektura.md` 6.1 requires the size of a value to be known BEFORE the
    /// text exists, and everything else in this module obeys that by refusing to
    /// build anything. This is the one place allowed to build, and it is a
    /// method of its own precisely so that reaching it is a decision rather than
    /// a side effect of asking a question.
    ///
    /// Nothing calls this to measure. [`ValueBody::metrics`] answers that from
    /// the recipe, and code that materialises in order to count would set the
    /// length bomb off inside the tool - before the warning it was supposed to
    /// produce.
    ///
    /// # The ceiling is checked FIRST, and that ordering is the safety property
    ///
    /// A recipe describing two billion characters is refused here without a
    /// single byte being allocated. Were the check to come afterwards, the
    /// refusal would arrive from a process that had already tried to hold two
    /// gigabytes, which is not a refusal at all.
    ///
    /// # Errors
    ///
    /// Returns the same [`ValueProblem`] the validator reports, so a caller
    /// never has to guess a rule code back from a message.
    pub fn materialise(&self) -> Result<String, ValueProblem> {
        self.check_size()?;

        match self {
            Self::Literal(text) => Ok(text.as_str().to_owned()),
            Self::Repeat { unit, count } => {
                // The size is already known and already bounded, so the buffer
                // is asked for once at the right size instead of growing.
                let metrics = self
                    .metrics()
                    .ok_or(ValueProblem::RepeatProductUnmeasurable)?;
                let mut out = String::with_capacity(metrics.bytes);
                for _ in 0..*count {
                    out.push_str(unit.as_str());
                }
                Ok(out)
            }
        }
    }

    /// Checks the size rules that apply before anything is built.
    ///
    /// `E024` and `E026` are deliberately separate: the first bounds the count,
    /// the second bounds the product. Without the second, a unit of 2000
    /// characters repeated a million times passed validation and described two
    /// billion characters from a file of a few kilobytes.
    pub fn check_size(&self) -> Result<(), ValueProblem> {
        let Self::Repeat { count, .. } = self else {
            return Ok(());
        };
        let count = *count;

        if count == 0 || count > MAX_REPEAT_COUNT {
            return Err(ValueProblem::RepeatCountOutOfRange { count });
        }

        let Some(metrics) = self.metrics() else {
            return Err(ValueProblem::RepeatProductUnmeasurable);
        };

        if metrics.code_points > MAX_VALUE_CODE_POINTS {
            return Err(ValueProblem::RepeatProductTooLarge {
                code_points: metrics.code_points,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;

    fn repeat(unit: &str, count: u32) -> ValueBody {
        ValueBody::Repeat {
            unit: LiteralText::new(unit),
            count,
        }
    }

    #[test]
    fn a_plain_value_has_no_size_problem() {
        let body = ValueBody::Literal(LiteralText::new("Jan Kowalski"));
        assert_eq!(body.check_size(), Ok(()));
    }

    #[test]
    fn the_largest_value_in_the_shipped_catalogue_passes() {
        // `len-100000` is the biggest product in catalog v0.1, ten times below
        // the ceiling. If this ever fails, the rule broke the existing catalogue.
        assert_eq!(repeat("a", 100_000).check_size(), Ok(()));
    }

    #[test]
    fn a_multi_byte_unit_is_bounded_by_code_points_not_bytes() {
        // 64 emoji: 64 code points but 256 bytes. A byte-based ceiling would
        // treat this differently from 64 plain letters, and it must not.
        assert_eq!(repeat("\u{1F468}", 64).check_size(), Ok(()));
    }

    #[test]
    fn exactly_at_the_ceiling_is_accepted() {
        assert_eq!(repeat("ab", 500_000).check_size(), Ok(()));
    }

    #[test]
    fn one_code_point_over_the_ceiling_is_rejected() {
        let problem = repeat("ab", 500_001).check_size().expect_err("must reject");
        assert_eq!(problem.code(), "E026");
    }

    #[test]
    fn the_two_billion_character_bomb_is_rejected_by_e026() {
        // The case OBS-39 was written about: count is inside E024's range, so
        // only the product rule catches it.
        let unit = "a".repeat(2000);
        let body = repeat(&unit, 1_000_000);
        let problem = body.check_size().expect_err("must reject");
        assert_eq!(problem.code(), "E026");
    }

    #[test]
    fn a_count_above_the_limit_is_rejected_by_e024_not_e026() {
        // Order matters: the count rule is checked first, so the reported code
        // names the actual mistake instead of its consequence.
        let problem = repeat("a", MAX_REPEAT_COUNT + 1)
            .check_size()
            .expect_err("must reject");
        assert_eq!(problem.code(), "E024");
    }

    #[test]
    fn a_zero_count_is_rejected() {
        let problem = repeat("a", 0).check_size().expect_err("must reject");
        assert_eq!(problem.code(), "E024");
    }

    #[test]
    fn a_literal_materialises_to_itself() {
        let body = ValueBody::Literal(LiteralText::new("Jan\u{200B}Kowalski"));
        assert_eq!(body.materialise().as_deref(), Ok("Jan\u{200B}Kowalski"));
    }

    #[test]
    fn a_recipe_materialises_to_the_text_it_describes() {
        let built = repeat("ab", 3)
            .materialise()
            .expect("well within the ceiling");
        assert_eq!(built, "ababab");
    }

    #[test]
    fn a_multi_byte_unit_repeats_whole_characters_not_bytes() {
        // The mistake a byte-oriented buffer invites: half an emoji is not a
        // character, and this catalogue is made of exactly such edges.
        let built = repeat("\u{1F468}", 3).materialise().expect("fits");
        assert_eq!(built.chars().count(), 3);
        assert_eq!(built.len(), 12);
    }

    #[test]
    fn what_comes_out_is_exactly_as_long_as_the_recipe_promised() {
        // The property emit rests on: the size warning printed BEFORE building
        // has to describe the thing that gets built, or it is not a warning.
        let body = repeat("\u{0105}b", 1000);
        let promised = body.metrics().expect("measurable");
        let built = body.materialise().expect("fits");
        assert_eq!(built.chars().count(), promised.code_points);
        assert_eq!(built.len(), promised.bytes);
    }

    #[test]
    fn the_two_billion_character_bomb_is_refused_without_being_built() {
        // 🔴 The ordering this method exists to guarantee. A unit of 2000
        // characters repeated a million times is refused by the ceiling BEFORE
        // any allocation, so this test returns in microseconds. If the check
        // ever moves after the building, this test does not merely fail - it
        // tries to hold two gigabytes first, and the run time IS the assertion.
        let unit = "a".repeat(2000);
        let problem = repeat(&unit, 1_000_000)
            .materialise()
            .expect_err("the ceiling must refuse this");
        assert_eq!(problem.code(), "E026");
    }

    #[test]
    fn a_count_outside_the_range_is_refused_by_materialising_too() {
        // materialise() must not be a way around check_size(). Anything the
        // validator refuses, this refuses, with the same code.
        let problem = repeat("a", 0)
            .materialise()
            .expect_err("zero is not a count");
        assert_eq!(problem.code(), "E024");
    }

    #[test]
    fn the_largest_value_in_the_shipped_catalogue_really_can_be_built() {
        // The positive control for the test above: a ceiling that refused
        // everything would pass every refusal test here and be useless.
        let built = repeat("a", 100_000).materialise().expect("must build");
        assert_eq!(built.len(), 100_000);
    }
}
