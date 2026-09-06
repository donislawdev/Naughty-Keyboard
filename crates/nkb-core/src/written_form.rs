//! How a value is **written**, as opposed to what it means.
//!
//! # Why this exists at all
//!
//! The escaping rules of this format are rules about spelling. "This value must
//! be written with an escape" cannot be answered by looking at the value after a
//! parser has expanded it, because by then the escape and the character it stood
//! for are the same thing.
//!
//! Measured against toml_edit 0.25.13, not assumed: rendering a parsed value back
//! out returns single quotes as double ones and an escape as the character it
//! stood for. So these rules read the raw slice of the file instead, and this
//! module is the part that understands what that slice says.
//!
//! # What it is not
//!
//! Not a TOML parser. It answers three questions about a piece of text that a
//! parser has already accepted - which quoting was used, which characters were
//! written out, and which escapes appear - and nothing else.

use crate::lint::{LintProblem, RuleCode};
use crate::text::needs_escaping;

/// Longest a value may be written out before the format suggests a recipe.
///
/// The specification records this as a number taken from judgement rather than
/// from data, and says so. It is repeated here rather than reinvented.
pub const LITERAL_LENGTH_HINT: usize = 2000;

/// The four ways TOML lets a string be written, and the one thing that matters
/// about the difference: whether a backslash starts an escape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quoting {
    /// `"..."` - backslash starts an escape.
    Basic,
    /// `"""..."""` - same, across lines.
    BasicMultiline,
    /// `'...'` - nothing is an escape, including a backslash.
    Literal,
    /// `'''...'''` - same, across lines.
    LiteralMultiline,
}

impl Quoting {
    /// Whether a backslash in this kind of string begins an escape sequence.
    ///
    /// The whole reason the type exists. In a literal string `\x41` is four
    /// characters and breaks no rule, while in a basic string it is an escape
    /// this format does not accept - and telling those apart by searching for
    /// the text `\x` would report the first as the second.
    #[must_use]
    pub fn escapes(self) -> bool {
        matches!(self, Self::Basic | Self::BasicMultiline)
    }

    /// Reads the quoting off the raw slice, longest marker first.
    ///
    /// Returns the quoting and the text between the markers. Returns nothing for
    /// a slice that is not a quoted string at all - a number, a date, a boolean -
    /// which is how a caller learns the value was not text to begin with.
    #[must_use]
    pub fn of(raw: &str) -> Option<(Self, &str)> {
        // Order matters: `"""` starts with `"`, so the longest marker is tried
        // first or every multiline string would be read as an empty basic one.
        for (marker, quoting) in [
            ("\"\"\"", Self::BasicMultiline),
            ("'''", Self::LiteralMultiline),
            ("\"", Self::Basic),
            ("'", Self::Literal),
        ] {
            if let Some(inner) = raw
                .strip_prefix(marker)
                .and_then(|rest| rest.strip_suffix(marker))
            {
                return Some((quoting, inner));
            }
        }
        None
    }
}

/// Checks how one value was written: E020, E022 and E029.
///
/// `raw` is the slice of the file the value occupies, quotes included. `expanded`
/// is the same value after the parser expanded it, needed only for its length -
/// a space is meaningful at the first and last position of a value, and there is
/// no way to know which position a written character lands on without it.
#[must_use]
pub fn check(raw: &str, expanded: &str) -> Vec<LintProblem> {
    let Some((quoting, inner)) = Quoting::of(raw) else {
        return Vec::new();
    };

    let mut problems = Vec::new();
    let length = expanded.chars().count();
    let mut position = 0usize;
    let mut characters = inner.chars().peekable();
    let mut reported_unescaped = false;

    while let Some(character) = characters.next() {
        if character == '\\' && quoting.escapes() {
            // An escape stands for exactly one character of the value, whatever
            // its spelling, so the position advances by one either way.
            if let Some(&next) = characters.peek() {
                if matches!(next, 'x' | 'e') {
                    problems.push(
                        LintProblem::new(RuleCode::EscapeOutsideCommonSubset)
                            .about(format!("\\{next}")),
                    );
                }
                // Consumed whole. A doubled backslash must not leave the second
                // one looking like the start of another escape, which is the case
                // that makes searching for the text `\x` wrong.
                characters.next();
            }
            position += 1;
            continue;
        }

        let at_edge = position == 0 || position + 1 == length;
        if needs_escaping(character, at_edge) && !reported_unescaped {
            // One fault, one code. In a literal string the repair is to change
            // the quoting, which is what E022 says; escaping in place is not
            // available there, so reporting E020 would name an impossible fix.
            let code = if quoting.escapes() {
                RuleCode::UnescapedCharacter
            } else {
                RuleCode::LiteralStringForEscapableValue
            };
            problems.push(LintProblem::new(code).about(describe(character)));
            // Reported once per value. A value made of forty invisible characters
            // is one mistake, and forty identical lines would bury every other
            // problem in the file.
            reported_unescaped = true;
        }
        position += 1;
    }

    problems
}

/// W026: a value written out at length, where a recipe would read better.
///
/// A warning and never an error. The value is correct - the file is merely
/// harder to review than it needs to be.
#[must_use]
pub fn check_length(expanded: &str) -> Option<LintProblem> {
    (expanded.chars().count() > LITERAL_LENGTH_HINT).then(|| {
        LintProblem::new(RuleCode::LiteralValueVeryLong).about(expanded.chars().count().to_string())
    })
}

/// Names a character by its code point, so a report can say which one it means
/// without printing something invisible into somebody's terminal.
fn describe(character: char) -> String {
    let code = character as u32;
    if code <= 0xFFFF {
        format!("\\u{code:04X}")
    } else {
        format!("\\U{code:08X}")
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;

    fn codes(raw: &str, expanded: &str) -> Vec<&'static str> {
        check(raw, expanded)
            .iter()
            .map(|p| p.code.as_str())
            .collect()
    }

    #[test]
    fn the_four_kinds_of_string_are_told_apart_longest_marker_first() {
        assert_eq!(Quoting::of("\"a\""), Some((Quoting::Basic, "a")));
        assert_eq!(Quoting::of("'a'"), Some((Quoting::Literal, "a")));
        assert_eq!(
            Quoting::of("\"\"\"a\"\"\""),
            Some((Quoting::BasicMultiline, "a"))
        );
        assert_eq!(
            Quoting::of("'''a'''"),
            Some((Quoting::LiteralMultiline, "a"))
        );
    }

    #[test]
    fn a_multiline_string_is_not_read_as_an_empty_basic_one() {
        // The failure mode of checking the shortest marker first: `"""abc"""`
        // would come back as an empty basic string and every rule would pass.
        let (quoting, inner) = Quoting::of("\"\"\"abc\"\"\"").expect("multiline");
        assert_eq!(quoting, Quoting::BasicMultiline);
        assert_eq!(inner, "abc");
    }

    #[test]
    fn something_that_is_not_a_string_is_not_mistaken_for_one() {
        // `count = 100000` and `updated = 2026-09-06` reach the same code path.
        assert_eq!(Quoting::of("100000"), None);
        assert_eq!(Quoting::of("2026-09-06"), None);
    }

    #[test]
    fn a_plain_value_breaks_nothing() {
        assert!(codes("\"Jan Kowalski\"", "Jan Kowalski").is_empty());
    }

    #[test]
    fn an_escaped_invisible_character_is_correct_and_stays_silent() {
        // The whole point of the escaping rule: written this way it is fine.
        assert!(codes("\"ab\\u200Bcd\"", "ab\u{200B}cd").is_empty());
    }

    #[test]
    fn an_invisible_character_written_out_is_caught_and_named_by_code_point() {
        let problems = check("\"ab\u{200B}cd\"", "ab\u{200B}cd");
        assert_eq!(problems.len(), 1);
        assert_eq!(problems[0].code.as_str(), "E020");
        // Named rather than printed: a report must not push something invisible
        // into a terminal and call it an explanation.
        assert_eq!(problems[0].subject.as_deref(), Some("\\u200B"));
    }

    #[test]
    fn a_space_at_the_edge_must_be_escaped_and_one_in_the_middle_must_not() {
        assert_eq!(codes("\"Kowalski \"", "Kowalski "), vec!["E020"]);
        assert_eq!(codes("\" Kowalski\"", " Kowalski"), vec!["E020"]);
        assert!(codes("\"Jan Kowalski\"", "Jan Kowalski").is_empty());
    }

    #[test]
    fn a_value_that_is_a_single_space_is_both_edges_at_once() {
        // `space-only` in the catalogue. Its one character is first and last, so
        // an implementation checking only one of the two would let it through.
        assert_eq!(codes("\" \"", " "), vec!["E020"]);
        assert!(codes("\"\\u0020\"", " ").is_empty());
    }

    #[test]
    fn an_empty_value_has_nothing_to_check_and_does_not_stumble() {
        assert!(codes("\"\"", "").is_empty());
    }

    #[test]
    fn a_literal_string_holding_something_invisible_is_e022_not_e020() {
        // One fault, one code. Escaping in place is not available inside single
        // quotes, so naming E020 there would point at a repair that does not
        // exist - the fix is to change the quoting, which is what E022 says.
        assert_eq!(codes("'ab\u{200B}cd'", "ab\u{200B}cd"), vec!["E022"]);
    }

    #[test]
    fn a_literal_string_with_nothing_to_escape_is_exactly_why_it_exists() {
        // Paths full of backslashes are the reason the format keeps this quoting.
        assert!(codes("'C:\\Windows\\System32'", "C:\\Windows\\System32").is_empty());
    }

    #[test]
    fn the_escape_forbidden_by_the_common_subset_is_caught() {
        // Valid in TOML 1.1.0 and an error in 1.0.0, and this parser accepts it -
        // measured. Nothing else would ever raise it.
        assert_eq!(codes("\"A\\x41\"", "AA"), vec!["E029"]);
        assert_eq!(codes("\"\\e[0m\"", "\u{1B}[0m"), vec!["E029"]);
    }

    #[test]
    fn a_doubled_backslash_does_not_turn_the_next_letter_into_an_escape() {
        // The case that makes searching for the text `\x` wrong: here the value
        // is a backslash followed by the three characters x, 4, 1.
        assert!(codes("\"\\\\x41\"", "\\x41").is_empty());
    }

    #[test]
    fn a_literal_string_containing_backslash_x_breaks_no_rule() {
        // Inside single quotes there are no escapes at all, so this is four
        // ordinary characters and E029 must stay quiet.
        assert!(codes("'\\x41'", "\\x41").is_empty());
    }

    #[test]
    fn an_escape_counts_as_one_character_when_deciding_what_is_at_the_edge() {
        // `\u0041` is six written characters and one value character. Counting
        // written ones would put the following character at the wrong position
        // and the edge rule would fire on the wrong thing.
        assert!(codes("\"\\u0041 b\"", "A b").is_empty());
    }

    #[test]
    fn a_value_full_of_invisible_characters_is_reported_once() {
        // One mistake, one line. Forty identical lines would bury every other
        // problem in the file.
        let raw = format!("\"{}\"", "\u{200B}".repeat(40));
        let expanded = "\u{200B}".repeat(40);
        assert_eq!(codes(&raw, &expanded).len(), 1);
    }

    #[test]
    fn an_emoji_stays_literal_and_raises_nothing() {
        // It has a visible shape and pretends to be nothing, so the format wants
        // it written out. A rule that escaped it would make the file less clear.
        assert!(codes("\"\u{1F468}\"", "\u{1F468}").is_empty());
    }

    #[test]
    fn a_character_outside_the_basic_plane_is_named_with_the_eight_digit_form() {
        // A tag character: invisible, beyond the basic plane, and a known way to
        // smuggle data through text. The four digit form cannot express it.
        let problems = check("\"a\u{E0041}b\"", "a\u{E0041}b");
        assert_eq!(problems[0].subject.as_deref(), Some("\\U000E0041"));
    }

    #[test]
    fn a_long_value_is_a_warning_and_a_short_one_is_nothing() {
        assert!(check_length(&"a".repeat(LITERAL_LENGTH_HINT)).is_none());
        let warning = check_length(&"a".repeat(LITERAL_LENGTH_HINT + 1)).expect("warns");
        assert_eq!(warning.code.as_str(), "W026");
        assert_eq!(warning.subject.as_deref(), Some("2001"));
    }
}
