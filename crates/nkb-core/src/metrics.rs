//! Counting text, in units that are named rather than assumed.
//!
//! Bytes, code points and graphemes are three different numbers for the same
//! value, and a limit stated in the wrong one is a bug that only shows up on
//! somebody else's data. A measured example from this project: the family emoji
//! is one grapheme, five code points, two UTF-16 units per emoji character and
//! eighteen bytes. Every one of those numbers is correct, and picking the wrong
//! one silently changes what a limit means.
//!
//! So nothing here is called `len`.

/// How long a piece of text is, in every unit the catalogue cares about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextMetrics {
    /// Unicode scalar values. This is the unit pack limits are stated in.
    pub code_points: usize,
    /// Bytes in UTF-8, which is what storage and byte-sized column limits see.
    pub bytes: usize,
}

impl TextMetrics {
    /// Measures text directly.
    #[must_use]
    pub fn of(text: &str) -> Self {
        Self {
            code_points: text.chars().count(),
            bytes: text.len(),
        }
    }

    /// Measures a repeated unit **without building the repeated text**.
    ///
    /// This is the whole reason the type exists. A generated value may describe
    /// a million characters, and the tool has to warn about its size *before*
    /// producing it - a warning that arrives after the memory has been eaten is
    /// not a warning. Building the text to measure it would defeat that.
    ///
    /// Returns `None` when the product does not fit in `usize`, which is itself
    /// an answer: such a value is out of range and belongs to the validator.
    #[must_use]
    pub fn of_repeated(unit: &str, count: u32) -> Option<Self> {
        let unit_metrics = Self::of(unit);
        let count = count as usize;
        Some(Self {
            code_points: unit_metrics.code_points.checked_mul(count)?,
            bytes: unit_metrics.bytes.checked_mul(count)?,
        })
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;

    #[test]
    fn ascii_has_the_same_count_in_both_units() {
        let metrics = TextMetrics::of("abc");
        assert_eq!(metrics.code_points, 3);
        assert_eq!(metrics.bytes, 3);
    }

    #[test]
    fn a_polish_letter_is_one_code_point_and_two_bytes() {
        // The value `bytes-vs-chars` in the catalogue exists for exactly this gap.
        let metrics = TextMetrics::of("\u{0105}");
        assert_eq!(metrics.code_points, 1);
        assert_eq!(metrics.bytes, 2);
    }

    #[test]
    fn an_emoji_outside_the_basic_plane_is_one_code_point_and_four_bytes() {
        let metrics = TextMetrics::of("\u{1F468}");
        assert_eq!(metrics.code_points, 1);
        assert_eq!(metrics.bytes, 4);
    }

    #[test]
    fn a_repeated_unit_is_measured_from_the_recipe_not_from_the_text() {
        let metrics = TextMetrics::of_repeated("ab", 500_000).expect("fits");
        assert_eq!(metrics.code_points, 1_000_000);
        assert_eq!(metrics.bytes, 1_000_000);
    }

    #[test]
    fn a_repeated_multi_byte_unit_counts_bytes_and_code_points_apart() {
        let metrics = TextMetrics::of_repeated("\u{1F468}", 64).expect("fits");
        assert_eq!(metrics.code_points, 64);
        assert_eq!(metrics.bytes, 256);
    }

    #[test]
    fn the_two_billion_character_case_is_counted_correctly_not_wrapped() {
        // This is the exact value that used to pass validation: a unit of 2000
        // characters repeated a million times. Counting it must not wrap into
        // something that looks harmless - that is what turns a bomb invisible.
        let unit = "a".repeat(2000);
        let metrics = TextMetrics::of_repeated(&unit, 1_000_000).expect("fits in usize");
        assert_eq!(metrics.code_points, 2_000_000_000);
    }
}
