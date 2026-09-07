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

use crate::ports::{PackFormat, PackSource, SourceError, TranslationCheck, TranslationTarget};
use nkb_core::lint::{LintProblem, LintReport, RuleCode, SkipReason, SkippedRule};
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
    report.extend(format.check(&text, id));

    // Everything above needed one file. This needs two, and it is the only place
    // in the validator that does.
    resolve_translation(source, format, &text, &mut report);

    report.sort();
    LintOutcome::Judged(report)
}

/// Resolves the pack a translation names, and reports what could not be checked.
///
/// # Why the layer above the parser owns this
///
/// Reading a second file is addressing, and addressing belongs to the source
/// port. The parser knows what `translates` says; only this layer knows how to
/// turn an identifier into a pack. Letting the parser fetch the file would put a
/// file system behind a trait that is supposed to be satisfiable by a string.
///
/// # Every way this can fail, written out
///
/// Four, and they can be listed rather than generalised: the three variants of
/// [`SourceError`] plus a pack that does not parse. Each sends the reader to a
/// different repair, so each keeps its own reason. Only the first is a finding
/// about **this** file - the rest are findings about the neighbour, and reporting
/// them against this file would send a contributor to fix the wrong one.
fn resolve_translation(
    source: &dyn PackSource,
    format: &dyn PackFormat,
    text: &str,
    report: &mut LintReport,
) {
    let target = match format.translated_pack(text) {
        // Not a translation, so the translation rules had nothing to look at.
        // That is a rule that ran and found nothing, not a rule that was skipped.
        TranslationTarget::NotATranslation => return,
        TranslationTarget::Unusable => {
            // E051 has already been reported against the file itself: a name
            // outside the identifier alphabet names no pack. What is left to say
            // is that the rule needing that pack could not run.
            report.skip(SkippedRule::new(
                RuleCode::TranslationRefersToMissingId,
                SkipReason::TranslatedPackNotNamed,
            ));
            return;
        }
        TranslationTarget::Pack(id) => id,
    };

    let translated = match source.read(&target) {
        Ok(translated) => translated,
        Err(error) => {
            if error == SourceError::NotFound {
                report.push(LintProblem::new(RuleCode::TranslatesUnknownPack).about(&target));
            }
            report.skip(
                SkippedRule::new(
                    RuleCode::TranslationRefersToMissingId,
                    match error {
                        SourceError::NotFound => SkipReason::TranslatedPackNotFound,
                        SourceError::Unreadable => SkipReason::TranslatedPackUnreadable,
                        SourceError::NotUtf8 => SkipReason::TranslatedPackNotUtf8,
                    },
                )
                .about(&target),
            );
            return;
        }
    };

    // 🔴 Before comparing anything: is the thing being translated a pack at all?
    //
    // A translation names its entries by key and a source pack names them in a
    // field, so a file that is itself a translation offers no identifiers to
    // compare against - and the comparison would come back saying every entry had
    // vanished. That sentence is not merely unhelpful, it is false, and it sends a
    // contributor to delete entries which are correct. A tool that guesses wrong
    // out loud is worse than one that says nothing, so this is checked first.
    if format.translated_pack(&translated) != TranslationTarget::NotATranslation {
        report.push(LintProblem::new(RuleCode::TranslatesNonPack).about(&target));
        report.skip(
            SkippedRule::new(
                RuleCode::TranslationRefersToMissingId,
                SkipReason::TranslatedPackIsATranslation,
            )
            .about(&target),
        );
        return;
    }

    match format.check_translation(text, &translated) {
        TranslationCheck::Compared(problems) => report.extend(problems),
        TranslationCheck::TranslatedPackDidNotParse => report.skip(
            SkippedRule::new(
                RuleCode::TranslationRefersToMissingId,
                SkipReason::TranslatedPackDidNotParse,
            )
            .about(&target),
        ),
    }
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
    struct Says {
        codes: Vec<RuleCode>,
        /// The text of the file under check.
        ///
        /// Kept so the double can answer `translated_pack` differently for the
        /// two texts it is asked about. `lint_pack` asks once about the file in
        /// hand and once about the pack that file translates, and an answer
        /// shared between them would make every translated pack look like a
        /// translation of its own - hiding the very rule that asks whether it is.
        under_check: String,
        target: TranslationTarget,
        comparison: TranslationCheck,
    }

    impl Says {
        fn nothing() -> Self {
            Self {
                codes: Vec::new(),
                under_check: String::new(),
                target: TranslationTarget::NotATranslation,
                comparison: TranslationCheck::Compared(Vec::new()),
            }
        }

        fn finding(codes: Vec<RuleCode>) -> Self {
            Self {
                codes,
                ..Self::nothing()
            }
        }

        fn translating(under_check: &str, target: TranslationTarget) -> Self {
            Self {
                under_check: under_check.to_owned(),
                target,
                ..Self::nothing()
            }
        }

        fn comparing(mut self, comparison: TranslationCheck) -> Self {
            self.comparison = comparison;
            self
        }
    }

    impl PackFormat for Says {
        // Loud rather than `None`. This double exists for a use case that never
        // parses, so a silent empty answer would let a future caller reach a
        // stub and believe it read a pack.
        fn parse(&self, _text: &str) -> Option<nkb_core::pack::Pack> {
            unimplemented!("Says is a double for a use case that does not parse packs")
        }

        fn check(&self, _text: &str, _expected_id: &str) -> Vec<LintProblem> {
            self.codes
                .iter()
                .map(|code| LintProblem::new(*code).at(5))
                .collect()
        }

        fn translated_pack(&self, text: &str) -> TranslationTarget {
            if text == self.under_check {
                self.target.clone()
            } else {
                TranslationTarget::NotATranslation
            }
        }

        fn check_translation(&self, _text: &str, _translated: &str) -> TranslationCheck {
            self.comparison.clone()
        }

        fn skeleton(&self, _id: &str, _today: crate::ports::Date) -> String {
            unreachable!("linting never asks for a skeleton")
        }

        fn canonical(&self, _text: &str) -> Option<String> {
            unreachable!("linting never asks for the canonical form")
        }

        fn same_insertions(&self, _before: &str, _after: &str) -> bool {
            unreachable!("linting never writes a file back")
        }
    }

    fn nothing() -> Says {
        Says::nothing()
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
        let format = Says::finding(vec![RuleCode::PackWithoutValues]);

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
        let format = Says::finding(vec![RuleCode::PackWithoutValues]);

        let LintOutcome::Judged(report) = lint_pack(&source, &format, "ordered") else {
            panic!("must be judged");
        };
        let lines: Vec<Option<u32>> = report.problems.iter().map(|p| p.line).collect();
        assert_eq!(lines, vec![Some(1), Some(5)]);
    }

    /// The whole of a report in one shape, so a test can assert on both halves.
    fn judged(outcome: LintOutcome) -> LintReport {
        let LintOutcome::Judged(report) = outcome else {
            panic!("must be judged");
        };
        report
    }

    fn codes(report: &LintReport) -> Vec<&str> {
        report.problems.iter().map(|p| p.code.as_str()).collect()
    }

    fn skips(report: &LintReport) -> Vec<(&str, &str)> {
        report
            .skipped
            .iter()
            .map(|s| (s.code.as_str(), s.reason.as_str()))
            .collect()
    }

    #[test]
    fn a_pack_that_is_not_a_translation_skips_nothing() {
        // The boundary that keeps the record meaningful. A rule about
        // translations looked at an ordinary pack and had nothing to judge, which
        // is "somebody looked and found nothing" - already the quiet answer. A
        // skip here would put a line in every report the tool ever writes.
        let source = InMemory::holding("ordinary", "format = 1\n");
        let report = judged(lint_pack(&source, &nothing(), "ordinary"));
        assert_eq!(skips(&report).len(), 0);
    }

    #[test]
    fn a_translation_of_a_pack_that_is_not_there_reports_it_and_records_what_it_could_not_check() {
        // Two answers, not one. E051 is the finding; the skip is the honesty about
        // W052, which this build runs and could not run here. Reporting only the
        // first leaves a summary saying every rule was checked.
        let source = InMemory::holding("unicode-text.pl", "format = 1\n");
        let format = Says::translating(
            "format = 1\n",
            TranslationTarget::Pack("unicode-text".to_owned()),
        );

        let report = judged(lint_pack(&source, &format, "unicode-text.pl"));
        assert_eq!(codes(&report), vec!["E051"]);
        assert_eq!(skips(&report), vec![("W052", "translated-pack-not-found")]);
        assert_eq!(
            report.skipped[0].subject.as_deref(),
            Some("unicode-text"),
            "the record has to name what was missing, or it sends nobody anywhere"
        );
    }

    #[test]
    fn a_translated_pack_that_will_not_open_is_not_reported_as_missing() {
        // The distinction the source port already draws, carried all the way to
        // the record. A pack that exists and will not open is a permission to fix,
        // not a name to correct - and it is a fault of that file, not this one, so
        // no E051 is raised against the file in hand.
        let mut packs = HashMap::new();
        packs.insert("locale-cz.pl".to_owned(), Ok("format = 1\n".to_owned()));
        packs.insert("locale-cz".to_owned(), Err(SourceError::Unreadable));
        let source = InMemory(packs);
        let format = Says::translating(
            "format = 1\n",
            TranslationTarget::Pack("locale-cz".to_owned()),
        );

        let report = judged(lint_pack(&source, &format, "locale-cz.pl"));
        assert_eq!(codes(&report).len(), 0, "the neighbour is the broken one");
        assert_eq!(skips(&report), vec![("W052", "translated-pack-unreadable")]);
        assert!(report.accepted(), "nothing here blocks this pack");
    }

    #[test]
    fn a_translated_pack_that_is_not_text_is_told_apart_from_one_that_will_not_open() {
        let mut packs = HashMap::new();
        packs.insert("mojibake.pl".to_owned(), Ok("format = 1\n".to_owned()));
        packs.insert("mojibake".to_owned(), Err(SourceError::NotUtf8));
        let source = InMemory(packs);
        let format = Says::translating(
            "format = 1\n",
            TranslationTarget::Pack("mojibake".to_owned()),
        );

        let report = judged(lint_pack(&source, &format, "mojibake.pl"));
        assert_eq!(skips(&report), vec![("W052", "translated-pack-not-utf8")]);
    }

    #[test]
    fn a_translated_pack_that_does_not_parse_records_that_nothing_was_compared() {
        // The fourth failure, and the only one the source port cannot see: the
        // file read fine and holds no identifiers. Told apart from an empty
        // comparison, which is the pair of answers this mechanism exists for.
        let mut packs = HashMap::new();
        packs.insert("broken.pl".to_owned(), Ok("format = 1\n".to_owned()));
        packs.insert("broken".to_owned(), Ok("format = = 1\n".to_owned()));
        let source = InMemory(packs);
        let format =
            Says::translating("format = 1\n", TranslationTarget::Pack("broken".to_owned()))
                .comparing(TranslationCheck::TranslatedPackDidNotParse);

        let report = judged(lint_pack(&source, &format, "broken.pl"));
        assert_eq!(
            skips(&report),
            vec![("W052", "translated-pack-did-not-parse")]
        );
    }

    #[test]
    fn a_translation_naming_something_that_is_not_a_pack_never_reaches_the_source() {
        // 🔴 The security half. A name outside the identifier alphabet is refused
        // by the port before it becomes a path, so no read is attempted at all -
        // and this test proves the absence rather than trusting the comment.
        struct Counting(std::cell::Cell<usize>);
        impl PackSource for Counting {
            fn read(&self, _id: &str) -> Result<String, SourceError> {
                self.0.set(self.0.get() + 1);
                Ok("format = 1\n".to_owned())
            }
        }

        let source = Counting(std::cell::Cell::new(0));
        let format = Says::translating("format = 1\n", TranslationTarget::Unusable);
        let report = judged(lint_pack(&source, &format, "hostile.pl"));

        assert_eq!(
            source.0.get(),
            1,
            "exactly the one read of the file under check, and none for the name it carried"
        );
        assert_eq!(skips(&report), vec![("W052", "translated-pack-not-named")]);
        assert_eq!(
            report.skipped[0].subject, None,
            "there is no usable name to repeat back, and echoing the raw one would put a stranger's text where a pack identifier belongs"
        );
    }

    #[test]
    fn a_comparison_that_happened_and_found_nothing_is_not_a_skip() {
        // The pair of answers, from the other side. An empty comparison means the
        // rule ran, so the report says nothing at all - and a mechanism that
        // recorded a skip here would make every correct translation look partly
        // unchecked.
        // The two files carry different text on purpose. They are two different
        // files - a translation and the pack it follows - and giving them the same
        // bytes would make the double answer the same for both, which is the one
        // thing the rule about a translation of a translation asks about.
        let mut packs = HashMap::new();
        packs.insert("good.pl".to_owned(), Ok("the translation\n".to_owned()));
        packs.insert("good".to_owned(), Ok("the pack it follows\n".to_owned()));
        let source = InMemory(packs);
        let format = Says::translating(
            "the translation\n",
            TranslationTarget::Pack("good".to_owned()),
        );

        let report = judged(lint_pack(&source, &format, "good.pl"));
        assert_eq!(codes(&report).len(), 0);
        assert_eq!(skips(&report).len(), 0);
    }

    #[test]
    fn a_translation_of_a_translation_is_refused_rather_than_answered_wrongly() {
        // 🔴 The case where this build used to say something untrue. A chain of
        // translations has no pack at the end, so the comparison would report
        // every entry as vanished - and that sentence sends a contributor to
        // delete entries which are correct.
        //
        // Both halves matter: the finding says what is wrong, and the skip says
        // that the rule needing the pack did not run. Reporting only the first
        // would leave a summary claiming every rule was checked.
        let mut packs = HashMap::new();
        packs.insert(
            "chain.cs".to_owned(),
            Ok("the outer translation\n".to_owned()),
        );
        packs.insert(
            "chain.pl".to_owned(),
            Ok("the inner translation\n".to_owned()),
        );
        let source = InMemory(packs);

        // The double answers for both texts here, which is exactly the shape of a
        // chain: each file names something that is itself a translation.
        struct EverythingTranslates;
        impl PackFormat for EverythingTranslates {
            // Loud rather than `None`. This double exists for a use case that never
            // parses, so a silent empty answer would let a future caller reach a
            // stub and believe it read a pack.
            fn parse(&self, _text: &str) -> Option<nkb_core::pack::Pack> {
                unimplemented!(
                    "EverythingTranslates is a double for a use case that does not parse packs"
                )
            }

            fn check(&self, _text: &str, _expected_id: &str) -> Vec<LintProblem> {
                Vec::new()
            }
            fn translated_pack(&self, _text: &str) -> TranslationTarget {
                TranslationTarget::Pack("chain.pl".to_owned())
            }
            fn check_translation(&self, _text: &str, _translated: &str) -> TranslationCheck {
                panic!("a chain must be refused before anything is compared")
            }
            fn skeleton(&self, _id: &str, _today: crate::ports::Date) -> String {
                unreachable!("linting never asks for a skeleton")
            }
            fn canonical(&self, _text: &str) -> Option<String> {
                unreachable!("linting never asks for the canonical form")
            }
            fn same_insertions(&self, _before: &str, _after: &str) -> bool {
                unreachable!("linting never writes a file back")
            }
        }

        let report = judged(lint_pack(&source, &EverythingTranslates, "chain.cs"));
        assert_eq!(codes(&report), vec!["E053"]);
        assert_eq!(
            skips(&report),
            vec![("W052", "translated-pack-is-a-translation")]
        );
    }

    #[test]
    fn a_warning_alone_leaves_the_pack_acceptable() {
        // Warnings are reported and passed through: a pack with warnings loads
        // normally and its contents are unchanged.
        let source = InMemory::holding("warned", "format = 1\n");
        let format = Says::finding(vec![RuleCode::PackWithoutTags]);

        let LintOutcome::Judged(report) = lint_pack(&source, &format, "warned") else {
            panic!("must be judged");
        };
        assert!(report.accepted());
        assert_eq!(report.warnings(), 1);
    }
}
