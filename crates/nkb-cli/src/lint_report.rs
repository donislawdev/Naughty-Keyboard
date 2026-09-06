//! Turning a verdict into the lines a person reads.
//!
//! # Why the sentences live here and not next to the rules
//!
//! A rule carries a code, a severity and a status. It carries no words, because
//! words belong to a surface and the same rule is reported by two of them. This
//! module is the command line surface's half of that.
//!
//! Every sentence below is reached through a key with exactly one value, and it
//! will never get a second one: the command line speaks English only, while the
//! graphical interface is the part that gets translated. The mechanism is shared,
//! the key space is not.
//!
//! # What the shape of a line is for
//!
//! `file:line  CODE  sentence` is the shape editors and build logs already know
//! how to jump to. Getting that right costs nothing and saves a contributor from
//! searching a file by eye.

use nkb_core::lint::{
    LintProblem, LintReport, RULES, RuleCode, RuleCoverage, RuleStatus, Severity,
};

/// The format version this build reads, quoted in the message about newer ones.
const SUPPORTED_FORMAT: &str = "1";

/// One line per problem, in the shape a build log can be jumped through.
#[must_use]
pub fn lines(report: &LintReport, file: &str) -> Vec<String> {
    report
        .problems
        .iter()
        .map(|problem| {
            let place = problem
                .line
                .map_or_else(|| file.to_owned(), |line| format!("{file}:{line}"));
            format!("{place}  {}  {}", problem.code.as_str(), sentence(problem))
        })
        .collect()
}

/// The closing lines: what was found, and what was not even looked for.
///
/// The second half is the point. A validator that runs eight rules and reports a
/// clean file invites the reader to conclude the pack is fine, and "nobody
/// looked" is a different answer from "somebody looked and found nothing".
#[must_use]
pub fn summary(report: &LintReport) -> Vec<String> {
    let coverage = RuleCoverage::measure();
    let mut lines = vec![
        format!(
            "{}, {}.",
            count(report.errors(), "error"),
            count(report.warnings(), "warning")
        ),
        format!(
            "Checked {} of {} rules. {} not checked - run `nkb lint --explain` to see which and why.",
            coverage.checked,
            coverage.total,
            coverage.unchecked()
        ),
    ];

    // Required by the format specification, which asks the tool to say this out
    // loud rather than let silence read as stability.
    lines.push(
        "The pack format is not frozen yet: it freezes with the first public release that ships packs."
            .to_owned(),
    );
    lines
}

/// Every rule, its code, and whether this build runs it.
#[must_use]
pub fn explanation() -> Vec<String> {
    let mut lines = vec![
        "Every rule the pack format defines, and what this build does with it.".to_owned(),
        String::new(),
    ];
    lines.extend(RULES.iter().map(|rule| {
        format!(
            "{}  {:<8}  {}",
            rule.code.as_str(),
            match rule.severity {
                Severity::Error => "blocking",
                Severity::Warning => "warning",
            },
            match rule.status {
                RuleStatus::Checked => "checked",
                RuleStatus::NotImplemented => "not checked - not implemented yet",
                RuleStatus::AwaitingDecision =>
                    "not checked - waiting on an unsettled question in the format",
                RuleStatus::RequiresPublishedVersion =>
                    "not checked here - needs the published pack to compare against",
            }
        )
    }));
    lines
}

/// English plural, used only for the two words this module counts.
fn count(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("{n} {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

/// The sentence for one problem. Says what happened and what to do next - a
/// message with only the first half is unfinished.
fn sentence(problem: &LintProblem) -> String {
    let subject = problem.subject.as_deref().unwrap_or("");
    let owner = problem.owner.as_deref().unwrap_or("");

    match problem.code {
        RuleCode::MissingOrUnsupportedFormat if subject.is_empty() => {
            "no `format` key. Add `format = 1` as the first line, so a later version can refuse this file instead of misreading it.".to_owned()
        }
        RuleCode::MissingOrUnsupportedFormat => format!(
            "format {subject} is newer than this build reads ({SUPPORTED_FORMAT}). Update Naughty Keyboard, or write the pack in format {SUPPORTED_FORMAT}."
        ),
        RuleCode::UnknownTopLevelKey => format!(
            "unknown top level key `{subject}`. The format has format, pack, values and pairs - check for a typo, because a key nobody reads is a setting nobody applied."
        ),
        RuleCode::MissingRequiredField if owner.is_empty() => format!(
            "no `[{subject}]` table. A pack declares its identity, version and licence there."
        ),
        RuleCode::MissingRequiredField if owner == "pack" => format!(
            "the pack declares no `{subject}`. The format requires it of every pack."
        ),
        RuleCode::MissingRequiredField => format!(
            "value `{owner}` declares no `{subject}`. The format requires it of every value."
        ),
        RuleCode::NotValidToml => format!(
            "not valid TOML: {subject}. Nothing else in this file could be checked."
        ),
        RuleCode::NotUtf8OrByteOrderMark => {
            "the file is not UTF-8, or starts with a byte order mark. Save it as UTF-8 with no mark - the mark is a value in this catalogue, not punctuation.".to_owned()
        }
        RuleCode::LineEndingNotNewline => {
            "a line ending other than a single newline. Save the file with newline endings - the two-character ending is a test value here, not a file style.".to_owned()
        }
        RuleCode::TabUsedForIndentation => {
            "a tab is used to indent. Indent with spaces - the tab character is a value in this catalogue and must not also be its punctuation.".to_owned()
        }
        RuleCode::PackWithoutValues => {
            "the pack declares no values. It would load, appear in the palette and insert nothing.".to_owned()
        }
        // Every other rule is registered and not yet run, so no problem carrying
        // its code can reach this point. Answering with the code rather than with
        // a crash keeps a validator from taking somebody's build down with it.
        other => format!("{} reported with no message written yet.", other.as_str()),
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;
    use nkb_core::lint::LintProblem;

    fn one(problem: LintProblem) -> String {
        let mut report = LintReport::default();
        report.push(problem);
        lines(&report, "whitespace.toml")
            .first()
            .cloned()
            .unwrap_or_default()
    }

    #[test]
    fn a_line_carries_the_file_the_place_and_the_code() {
        let line = one(LintProblem::new(RuleCode::PackWithoutValues).at(12));
        assert!(line.starts_with("whitespace.toml:12  E008  "), "{line}");
    }

    #[test]
    fn a_problem_about_the_whole_file_names_the_file_without_a_line() {
        // A colon with nothing after it would send an editor to line zero, which
        // exists in no file.
        let line = one(LintProblem::new(RuleCode::MissingOrUnsupportedFormat));
        assert!(line.starts_with("whitespace.toml  E001  "), "{line}");
    }

    #[test]
    fn a_missing_field_names_the_value_it_belongs_to() {
        let line = one(LintProblem::new(RuleCode::MissingRequiredField)
            .at(20)
            .about("since")
            .owned_by("trailing-space"));
        assert!(
            line.contains("value `trailing-space` declares no `since`"),
            "{line}"
        );
    }

    #[test]
    fn a_missing_pack_field_is_worded_differently_from_a_missing_value_field() {
        // Same code, two owners, two repairs. One sentence for both would make
        // the reader open the file to find out which half is wrong.
        let pack = one(LintProblem::new(RuleCode::MissingRequiredField)
            .at(3)
            .about("license")
            .owned_by("pack"));
        let value = one(LintProblem::new(RuleCode::MissingRequiredField)
            .at(9)
            .about("license")
            .owned_by("bool-no"));
        assert_ne!(pack, value);
        assert!(pack.contains("the pack declares no `license`"), "{pack}");
    }

    #[test]
    fn a_newer_format_says_which_version_was_found_and_which_is_read() {
        let line = one(LintProblem::new(RuleCode::MissingOrUnsupportedFormat)
            .at(1)
            .about("2"));
        assert!(line.contains("format 2 is newer"), "{line}");
        assert!(line.contains('1'), "{line}");
    }

    #[test]
    fn the_parser_message_survives_into_the_line() {
        let line = one(LintProblem::new(RuleCode::NotValidToml)
            .at(4)
            .about("invalid float, expected `nan`"));
        assert!(line.contains("invalid float"), "{line}");
    }

    #[test]
    fn the_summary_says_how_much_was_not_even_looked_at() {
        // The half that keeps a clean verdict honest.
        let text = summary(&LintReport::default()).join("\n");
        assert!(text.contains("0 errors, 0 warnings."), "{text}");
        assert!(text.contains("Checked 8 of 41 rules"), "{text}");
        assert!(text.contains("33 not checked"), "{text}");
    }

    #[test]
    fn the_summary_says_the_format_is_still_changeable() {
        // Required by the format specification: silence reads as stability, and
        // this format is not stable until the first public release with packs.
        let text = summary(&LintReport::default()).join("\n");
        assert!(text.contains("not frozen yet"), "{text}");
    }

    #[test]
    fn one_problem_is_singular_and_two_are_plural() {
        let mut report = LintReport::default();
        report.push(LintProblem::new(RuleCode::PackWithoutValues));
        assert!(summary(&report).join("\n").contains("1 error,"));

        report.push(LintProblem::new(RuleCode::UnknownTopLevelKey));
        assert!(summary(&report).join("\n").contains("2 errors,"));
    }

    #[test]
    fn the_explanation_lists_every_rule_and_distinguishes_the_reasons() {
        let text = explanation().join("\n");
        for rule in &RULES {
            assert!(
                text.contains(rule.code.as_str()),
                "missing {}",
                rule.code.as_str()
            );
        }
        // The three reasons a rule is not run must read differently, or the
        // register collapses back into one undifferentiated silence.
        assert!(text.contains("not implemented yet"), "{text}");
        assert!(text.contains("waiting on an unsettled question"), "{text}");
        assert!(text.contains("needs the published pack"), "{text}");
    }

    #[test]
    fn every_checked_rule_has_a_sentence_written_for_it() {
        // The guard that keeps the fallback from becoming a hiding place: a rule
        // this build runs must be able to say what it found.
        for rule in RULES.iter().filter(|r| r.status == RuleStatus::Checked) {
            let text = sentence(&LintProblem::new(rule.code));
            assert!(
                !text.contains("no message written yet"),
                "{} has no sentence",
                rule.code.as_str()
            );
        }
    }
}
