//! The three rules that have to be checked before a parser touches the file.
//!
//! Measured, not assumed: a TOML parser accepts a byte order mark, accepts the
//! two-character line ending and accepts a tab as indentation, all without a
//! word. Every one of those three is a value in this catalogue, which is exactly
//! why they must not be the punctuation of a source file - and why nothing
//! downstream will ever complain about them on our behalf.
//!
//! This module also owns the translation from a byte offset to a line number,
//! because every place that reports a problem needs it and two implementations
//! of it would disagree on the first file containing something unusual.

use crate::lint::{LintProblem, RuleCode};

/// The byte order mark, which is a value in the catalogue and never punctuation.
const BYTE_ORDER_MARK: char = '\u{FEFF}';

/// Turns a byte offset into a line number counted from one.
///
/// Offsets that fall inside a character, or past the end of the text, return the
/// last line rather than nothing: a problem with a slightly wrong line number is
/// still findable, while a problem with no line number at all sends the reader
/// to search a file by eye.
#[must_use]
pub fn line_of(text: &str, byte_offset: usize) -> u32 {
    let head = text.get(..byte_offset).unwrap_or(text);
    let lines = head.matches('\n').count() + 1;
    u32::try_from(lines).unwrap_or(u32::MAX)
}

/// Checks what only the raw file can answer: E005, E006 and E007.
///
/// The rule about the file not being valid UTF-8 - the other half of E005 - is
/// answered before this point, by whatever produced the text. By the time there
/// is a `&str` to look at, that question has already been settled.
#[must_use]
pub fn check(text: &str) -> Vec<LintProblem> {
    let mut problems = Vec::new();

    if text.starts_with(BYTE_ORDER_MARK) {
        problems.push(LintProblem::new(RuleCode::NotUtf8OrByteOrderMark).at(1));
    }

    if let Some(line) = first_foreign_line_ending(text) {
        problems.push(LintProblem::new(RuleCode::LineEndingNotNewline).at(line));
    }

    if let Some(line) = first_tab_indent(text) {
        problems.push(LintProblem::new(RuleCode::TabUsedForIndentation).at(line));
    }

    problems
}

/// The first line ending that is not a single newline, or nothing.
///
/// Reports the first one rather than all of them on purpose. A file saved by an
/// editor with the wrong setting has the wrong ending on every line, and a
/// thousand identical problems bury the one that matters.
fn first_foreign_line_ending(text: &str) -> Option<u32> {
    let mut line = 1u32;
    let mut previous_was_carriage_return = false;

    for character in text.chars() {
        if previous_was_carriage_return {
            // A carriage return is foreign whether or not a newline follows it:
            // paired it is the two-character ending, alone it is the ending used
            // by no current system and by several test values.
            return Some(line);
        }
        match character {
            '\r' => previous_was_carriage_return = true,
            '\n' => line = line.saturating_add(1),
            _ => {}
        }
    }

    previous_was_carriage_return.then_some(line)
}

/// The first line indented with a tab, or nothing.
///
/// Only leading whitespace counts. A tab inside a value is the catalogue doing
/// its job, and a rule that cannot tell the two apart would make the pack about
/// tab characters impossible to write.
fn first_tab_indent(text: &str) -> Option<u32> {
    text.lines()
        .enumerate()
        .find(|(_, line)| line.starts_with('\t'))
        .map(|(index, _)| u32::try_from(index + 1).unwrap_or(u32::MAX))
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;

    fn codes(text: &str) -> Vec<&'static str> {
        check(text).iter().map(|p| p.code.as_str()).collect()
    }

    #[test]
    fn a_clean_file_produces_nothing() {
        assert!(codes("format = 1\n\n[pack]\nid = \"a\"\n").is_empty());
    }

    #[test]
    fn a_byte_order_mark_is_caught_because_the_parser_accepts_it_silently() {
        // Measured against toml_edit 0.25.13: a file starting with this mark
        // parses without a word. If nothing here catches it, nothing does.
        let text = format!("{BYTE_ORDER_MARK}format = 1\n");
        assert_eq!(codes(&text), vec!["E005"]);
    }

    #[test]
    fn a_byte_order_mark_anywhere_else_is_a_value_not_a_problem() {
        // The same character in the middle of a line is content. Escaping rules
        // deal with it there - this rule is about the start of the file only.
        let text = format!("value = \"a{BYTE_ORDER_MARK}b\"\n");
        assert!(codes(&text).is_empty());
    }

    #[test]
    fn the_two_character_line_ending_is_caught_and_located() {
        let problems = check("format = 1\nid = \"a\"\r\nname = \"b\"\n");
        assert_eq!(problems.len(), 1);
        assert_eq!(problems[0].code.as_str(), "E006");
        assert_eq!(problems[0].line, Some(2));
    }

    #[test]
    fn a_lone_carriage_return_is_caught_too() {
        // Not paired with a newline, so a check written as a search for the pair
        // would miss it - and a lone carriage return is itself a test value.
        assert_eq!(codes("format = 1\rid = \"a\"\n"), vec!["E006"]);
    }

    #[test]
    fn a_carriage_return_at_the_very_end_is_not_lost() {
        // The end of input is where a loop that only reacts on the next character
        // quietly drops the last one.
        assert_eq!(codes("format = 1\n\r"), vec!["E006"]);
    }

    #[test]
    fn only_the_first_foreign_line_ending_is_reported() {
        // A file saved with the wrong setting has every line wrong. Reporting all
        // of them would bury every other problem in the file.
        let problems = check("a = 1\r\nb = 2\r\nc = 3\r\n");
        assert_eq!(problems.len(), 1);
        assert_eq!(problems[0].line, Some(1));
    }

    #[test]
    fn a_tab_used_as_indentation_is_caught_and_located() {
        let problems = check("[pack]\n\tid = \"a\"\n");
        assert_eq!(problems.len(), 1);
        assert_eq!(problems[0].code.as_str(), "E007");
        assert_eq!(problems[0].line, Some(2));
    }

    #[test]
    fn a_tab_inside_a_value_is_content_and_must_not_be_reported() {
        // The catalogue contains a tab character as a test value. A rule that
        // cannot tell indentation from content would make that value unwritable.
        assert!(codes("value = \"a\tb\"\n").is_empty());
    }

    #[test]
    fn all_three_can_be_reported_from_one_run() {
        // The validator reports everything it finds in one pass. Three separate
        // runs would turn five minutes of fixing into three rounds of it.
        let text = format!("{BYTE_ORDER_MARK}[pack]\r\n\tid = \"a\"\n");
        let mut found = codes(&text);
        found.sort_unstable();
        assert_eq!(found, vec!["E005", "E006", "E007"]);
    }

    #[test]
    fn a_line_number_is_counted_from_one_not_from_zero() {
        let text = "alpha\nbeta\ngamma\n";
        assert_eq!(line_of(text, 0), 1);
        assert_eq!(line_of(text, 6), 2);
        assert_eq!(line_of(text, 11), 3);
    }

    #[test]
    fn an_offset_past_the_end_returns_the_last_line_rather_than_nothing() {
        // A wrong line number is findable. No line number sends the reader to
        // search the file by eye, which is the outcome worth avoiding.
        let text = "alpha\nbeta\n";
        assert_eq!(line_of(text, 9_000), 3);
    }

    #[test]
    fn an_offset_inside_a_character_does_not_lose_the_position() {
        // A multi-byte character means offsets that are not character boundaries
        // exist, and slicing on one would otherwise return nothing at all.
        let text = "\u{1F468}\nsecond\n";
        assert_eq!(line_of(text, 2), 3);
    }
}
