//! Validating one pack file: the use case behind `nkb lint`.
//!
//! This is the same code path that contribution checks run, that a contributor
//! runs on their own machine, and that a release build runs over the packs it is
//! about to ship. One implementation rather than three is the point: a rule that
//! holds locally and not in review would make the review unpredictable, which is
//! the fastest way to lose contributors.
//!
//! Nothing here knows what a path is, which parser exists, or how a problem will
//! be worded. It knows the order of the checks and what the verdict means.

use crate::ports::{PackFormat, PackSource, SourceError};
use nkb_core::lint::{LintProblem, LintReport, RuleCode};
use nkb_core::source_text;

/// What came of asking about one pack.
///
/// The two failures are kept apart because they send the reader to fix different
/// things: one is a name that does not exist, the other is a file that exists and
/// will not open. Collapsing them into "could not lint" would be the tool making
/// the same mistake it exists to catch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LintOutcome {
    /// The file was read and judged. Whether it passed is inside the report.
    Judged(LintReport),
    /// There is no such pack to read.
    NotFound,
    /// The pack is there and could not be read: permissions, a device, a handle.
    Unreadable,
}

/// Validates one pack file.
///
/// Takes both of its collaborators as parameters, so the whole use case runs in
/// a test with no disk and no parser.
#[must_use]
pub fn lint_pack(source: &dyn PackSource, format: &dyn PackFormat, id: &str) -> LintOutcome {
    let text = match source.read(id) {
        Ok(text) => text,
        Err(SourceError::NotFound) => return LintOutcome::NotFound,
        Err(SourceError::Unreadable) => return LintOutcome::Unreadable,
        Err(SourceError::NotUtf8) => {
            // Not a failure of the tool - a finding about the pack, and the format
            // has a rule with its name on it. Reporting this as a read error would
            // hide a rule violation behind an apology.
            let mut report = LintReport::default();
            report.push(LintProblem::new(RuleCode::NotUtf8OrByteOrderMark));
            return LintOutcome::Judged(report);
        }
    };

    let mut report = LintReport::default();

    // The raw file first. These three rules ask about bytes that every parser
    // measured accepts without a word, so nothing downstream would ever raise
    // them on our behalf.
    report.extend(source_text::check(&text));
    report.extend(format.check(&text));

    report.sort();
    LintOutcome::Judged(report)
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct InMemory(HashMap<String, Result<String, SourceError>>);

    impl InMemory {
        fn holding(id: &str, text: &str) -> Self {
            let mut packs = HashMap::new();
            packs.insert(id.to_owned(), Ok(text.to_owned()));
            Self(packs)
        }

        fn failing(id: &str, error: SourceError) -> Self {
            let mut packs = HashMap::new();
            packs.insert(id.to_owned(), Err(error));
            Self(packs)
        }
    }

    impl PackSource for InMemory {
        fn read(&self, id: &str) -> Result<String, SourceError> {
            self.0
                .get(id)
                .cloned()
                .unwrap_or(Err(SourceError::NotFound))
        }
    }

    /// A parser that finds whatever the test tells it to, so that this use case
    /// can be exercised without a real one. That is the reason the port exists.
    struct Says(Vec<RuleCode>);

    impl PackFormat for Says {
        fn check(&self, _text: &str) -> Vec<LintProblem> {
            self.0
                .iter()
                .map(|code| LintProblem::new(*code).at(5))
                .collect()
        }
    }

    fn nothing() -> Says {
        Says(Vec::new())
    }

    #[test]
    fn a_clean_pack_is_judged_and_accepted() {
        let source = InMemory::holding("whitespace", "format = 1\n");
        let outcome = lint_pack(&source, &nothing(), "whitespace");

        let LintOutcome::Judged(report) = outcome else {
            panic!("a readable pack must be judged");
        };
        assert!(report.accepted());
        assert_eq!(report.problems.len(), 0);
    }

    #[test]
    fn an_absent_pack_is_not_the_same_answer_as_an_unreadable_one() {
        // Two different repairs: a typo in a name, or a permission. A tool that
        // renders them identically sends the reader to fix the wrong thing.
        let source = InMemory::holding("present", "format = 1\n");
        assert_eq!(
            lint_pack(&source, &nothing(), "absent"),
            LintOutcome::NotFound
        );

        let source = InMemory::failing("locked", SourceError::Unreadable);
        assert_eq!(
            lint_pack(&source, &nothing(), "locked"),
            LintOutcome::Unreadable
        );
    }

    #[test]
    fn a_file_that_is_not_utf8_is_a_finding_about_the_pack_not_a_tool_failure() {
        let source = InMemory::failing("mojibake", SourceError::NotUtf8);
        let LintOutcome::Judged(report) = lint_pack(&source, &nothing(), "mojibake") else {
            panic!("this is a rule violation, not a read error");
        };
        assert!(!report.accepted());
        assert_eq!(report.problems[0].code.as_str(), "E005");
    }

    #[test]
    fn findings_from_the_raw_file_and_from_the_parser_arrive_together() {
        // One run, everything found. Five minutes of fixing rather than five
        // rounds of it - and the rules come from two different places, so this is
        // where a naive implementation returns only half of them.
        let source = InMemory::holding("mixed", "[pack]\r\nid = \"a\"\n");
        let format = Says(vec![RuleCode::PackWithoutValues]);

        let LintOutcome::Judged(report) = lint_pack(&source, &format, "mixed") else {
            panic!("must be judged");
        };
        let codes: Vec<&str> = report.problems.iter().map(|p| p.code.as_str()).collect();
        assert_eq!(codes, vec!["E006", "E008"]);
    }

    #[test]
    fn problems_come_back_ordered_by_place() {
        // Two runs over one file must agree, or a contribution check that diffs
        // its own output turns red for no reason.
        let source = InMemory::holding("ordered", "\u{FEFF}format = 1\n");
        let format = Says(vec![RuleCode::PackWithoutValues]);

        let LintOutcome::Judged(report) = lint_pack(&source, &format, "ordered") else {
            panic!("must be judged");
        };
        let lines: Vec<Option<u32>> = report.problems.iter().map(|p| p.line).collect();
        assert_eq!(lines, vec![Some(1), Some(5)]);
    }

    #[test]
    fn a_warning_alone_leaves_the_pack_acceptable() {
        // Warnings are reported and passed through: a pack with warnings loads
        // normally and its contents are unchanged.
        let source = InMemory::holding("warned", "format = 1\n");
        let format = Says(vec![RuleCode::PackWithoutTags]);

        let LintOutcome::Judged(report) = lint_pack(&source, &format, "warned") else {
            panic!("must be judged");
        };
        assert!(report.accepted());
        assert_eq!(report.warnings(), 1);
    }
}
