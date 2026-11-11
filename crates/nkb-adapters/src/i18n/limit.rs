//! The words of the length limit window (`ux-spec.md` 6 Q, `D178`): the limit a
//! field states, typed as one or two numbers and a unit, and the values that
//! test it.
//!
//! # What is said here, and what is not
//!
//! The window's own words: its labels, what its keys do, why a pair of numbers
//! gives no values. What a value is called and what the set is called are not
//! here - those are `boundary_text`'s, English whatever language the palette
//! speaks, because they go into a ticket.
//!
//! # A count stands after its label
//!
//! `bytes: 50`, never "50 bytes": the window has no mechanism for plurals, and
//! a language with three forms would need one (`D80`).

use nkb_core::boundary::{Limit, LimitProblem, MAX_LIMIT, Unit};

use super::fill::fill;

/// The words of the window that carry nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimitLabel {
    /// The window's title, which the system shows in the task switcher.
    Title,
    /// The heading in the window's header, and the name of the row that opens
    /// the window from the value window.
    Heading,
    /// What the row in the value window says beside its name.
    RowDetail,
    Most,
    Least,
    /// The label of the switch between the four units.
    Unit,
    /// The four units, as the switch names them.
    UnitGraphemes,
    UnitCodePoints,
    UnitUtf16,
    UnitBytes,
    Use,
    /// What the button does, as UI Automation reads it.
    UseAction,
    KeyTab,
    /// What Tab does.
    OtherNumber,
    /// What the left and right arrows do.
    ChangeUnit,
    /// What the up and down arrows do.
    StartAt,
}

fn pattern_limit_label(label: LimitLabel) -> &'static str {
    match label {
        LimitLabel::Title => "Naughty Keyboard - length limit",
        LimitLabel::Heading => "Length limit",
        LimitLabel::RowDetail => "Values that test how much text a field takes",
        LimitLabel::Most => "At most",
        LimitLabel::Least => "At least",
        LimitLabel::Unit => "Counted in",
        LimitLabel::UnitGraphemes => "Graphemes",
        LimitLabel::UnitCodePoints => "Code points",
        LimitLabel::UnitUtf16 => "UTF-16 units",
        LimitLabel::UnitBytes => "Bytes",
        LimitLabel::Use => "Use these values",
        LimitLabel::UseAction => "Use these values in the palette",
        LimitLabel::KeyTab => "Tab",
        LimitLabel::OtherNumber => "Move between the two numbers",
        LimitLabel::ChangeUnit => "Change what the limit is counted in",
        LimitLabel::StartAt => "Choose the value to start from",
    }
}

/// A label of the window, ready to show.
#[must_use]
pub fn limit_label(label: LimitLabel) -> &'static str {
    pattern_limit_label(label)
}

/// A unit as the switch names it, ready to show.
#[must_use]
pub fn limit_unit(unit: Unit) -> &'static str {
    pattern_limit_label(match unit {
        Unit::Graphemes => LimitLabel::UnitGraphemes,
        Unit::CodePoints => LimitLabel::UnitCodePoints,
        Unit::Utf16 => LimitLabel::UnitUtf16,
        Unit::Bytes => LimitLabel::UnitBytes,
    })
}

/// How long a value is, counted the way its limit is: `bytes: 49`.
#[must_use]
pub fn limit_length(unit: Unit, length: u32) -> String {
    let line = match unit {
        Unit::Graphemes => LimitLine::LengthGraphemes,
        Unit::CodePoints => LimitLine::LengthCodePoints,
        Unit::Utf16 => LimitLine::LengthUtf16,
        Unit::Bytes => LimitLine::LengthBytes,
    };
    fill(pattern_limit_line(line), &[("length", &length.to_string())])
}

/// A line of the window that carries numbers, or says why there are no values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LimitLine {
    /// How much the values come to together.
    Account,
    /// The length of one value, in the unit of the limit.
    LengthGraphemes,
    LengthCodePoints,
    LengthUtf16,
    LengthBytes,
    /// Nothing is typed yet: what to type, and what comes of it.
    Nothing,
    MostOutOfRange,
    LeastOutOfRange,
    LeastAboveMost,
}

fn pattern_limit_line(line: LimitLine) -> &'static str {
    match line {
        // The dot itself, not its escape: `ux-spec.md` 6 holds this pattern
        // byte for byte, as it holds the arrows of the value window's keys.
        LimitLine::Account => "values: {values} · code points: {code_points} · bytes: {bytes}",
        LimitLine::LengthGraphemes => "graphemes: {length}",
        LimitLine::LengthCodePoints => "code points: {length}",
        LimitLine::LengthUtf16 => "UTF-16 units: {length}",
        LimitLine::LengthBytes => "bytes: {length}",
        LimitLine::Nothing => {
            "Type the limit the field states. The values worth trying are listed here: nothing, then one under, exactly at and one over each bound."
        }
        LimitLine::MostOutOfRange => "The most a field takes is a number from 1 to {limit}.",
        LimitLine::LeastOutOfRange => "The least a field takes is a number from 1 to {limit}.",
        LimitLine::LeastAboveMost => "The least is more than the most, so no length fits both.",
    }
}

/// How much the values of a limit come to together: how many there are, and
/// their sizes added up - what `nkb boundary` says before it prints them.
#[must_use]
pub fn limit_account(values: usize, code_points: usize, bytes: usize) -> String {
    fill(
        pattern_limit_line(LimitLine::Account),
        &[
            ("values", &values.to_string()),
            ("code_points", &code_points.to_string()),
            ("bytes", &bytes.to_string()),
        ],
    )
}

/// Why the two numbers give no values, where the values would be listed.
#[must_use]
pub fn limit_problem(problem: LimitProblem) -> String {
    let line = match problem {
        LimitProblem::NoBound => LimitLine::Nothing,
        LimitProblem::MaxOutOfRange => LimitLine::MostOutOfRange,
        LimitProblem::MinOutOfRange => LimitLine::LeastOutOfRange,
        LimitProblem::MinAboveMax => LimitLine::LeastAboveMost,
    };
    fill(
        pattern_limit_line(line),
        &[("limit", &MAX_LIMIT.to_string())],
    )
}

/// What the palette walks: a pack, named by its identifier, or the values
/// made for a limit. For the sentences that give the command printing the same
/// thing - a made set has no pack to name, and `nkb emit boundary` would send
/// the tester to a pack that does not exist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InPlay<'a> {
    Pack(&'a str),
    Limit(Limit),
}

impl<'a> From<&'a str> for InPlay<'a> {
    fn from(pack: &'a str) -> Self {
        Self::Pack(pack)
    }
}

impl<'a> From<&'a String> for InPlay<'a> {
    fn from(pack: &'a String) -> Self {
        Self::Pack(pack)
    }
}

/// What a command prints, of the two things a failed copy sends the tester
/// to the command line for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Printed {
    /// The facts of the report block.
    Facts,
    /// The value itself.
    Value,
}

fn pattern_printed(printed: Printed) -> &'static str {
    match printed {
        Printed::Facts => "nkb show {pack}",
        Printed::Value => "nkb emit {pack}",
    }
}

/// The command that prints what could not be copied: of a pack, the command
/// that shows or prints it, and of the values made for a limit, the one
/// command that makes them, which prints both.
pub(super) fn printed_by(in_play: InPlay<'_>, printed: Printed) -> String {
    match in_play {
        InPlay::Pack(pack) => fill(pattern_printed(printed), &[("pack", pack)]),
        InPlay::Limit(limit) => crate::boundary_text::command(limit),
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
    fn a_failed_copy_names_the_command_that_prints_what_is_in_play() {
        assert_eq!(
            printed_by(InPlay::Pack("whitespace"), Printed::Facts),
            "nkb show whitespace"
        );
        assert_eq!(
            printed_by("whitespace".into(), Printed::Value),
            "nkb emit whitespace"
        );
        let limit = Limit::new(Unit::Bytes, None, Some(50)).expect("a limit");
        for printed in [Printed::Facts, Printed::Value] {
            assert_eq!(
                printed_by(InPlay::Limit(limit), printed),
                "nkb boundary --max-length 50 --unit bytes"
            );
        }
    }

    #[test]
    fn a_length_stands_after_the_name_of_its_unit() {
        assert_eq!(limit_length(Unit::Bytes, 49), "bytes: 49");
        assert_eq!(limit_length(Unit::Graphemes, 1), "graphemes: 1");
        assert_eq!(limit_length(Unit::CodePoints, 0), "code points: 0");
        assert_eq!(limit_length(Unit::Utf16, 100), "UTF-16 units: 100");
    }

    #[test]
    fn the_account_names_all_three_counts() {
        assert_eq!(
            limit_account(34, 622, 1020),
            "values: 34 \u{b7} code points: 622 \u{b7} bytes: 1020"
        );
    }

    #[test]
    fn every_problem_has_a_sentence_of_its_own_with_every_place_filled() {
        let problems = [
            LimitProblem::NoBound,
            LimitProblem::MaxOutOfRange,
            LimitProblem::MinOutOfRange,
            LimitProblem::MinAboveMax,
        ];
        let said: Vec<String> = problems.into_iter().map(limit_problem).collect();
        for (at, sentence) in said.iter().enumerate() {
            assert!(!sentence.contains(['{', '}']), "{sentence}");
            assert!(
                !said[..at].contains(sentence),
                "two problems share a sentence: {sentence}"
            );
        }
        assert_eq!(
            limit_problem(LimitProblem::MaxOutOfRange),
            "The most a field takes is a number from 1 to 100000."
        );
        assert_eq!(
            limit_problem(LimitProblem::MinOutOfRange),
            "The least a field takes is a number from 1 to 100000."
        );
    }

    #[test]
    fn the_switch_names_the_four_units_apart() {
        let names: Vec<&str> = Unit::ALL.into_iter().map(limit_unit).collect();
        assert_eq!(names, ["Graphemes", "Code points", "UTF-16 units", "Bytes"]);
    }
}
