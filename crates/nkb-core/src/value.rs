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
}
