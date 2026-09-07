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

use nkb_core::description::MIN_BREAKS_CODE_POINTS;
use nkb_core::identity::is_pack_id;
use nkb_core::lint::{
    LintProblem, LintReport, RULES, RuleCode, RuleCoverage, RuleStatus, Severity, SkipReason,
    SkippedRule,
};
use nkb_core::schema::kind_of;

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
            "Checked {} of {} rules{}. {} not checked - run `nkb lint --explain` to see which and why.",
            coverage.checked,
            coverage.total,
            if coverage.partly == 0 {
                String::new()
            } else {
                format!(", {} of them only in part", coverage.partly)
            },
            coverage.unchecked()
        ),
    ];

    // The line above counts what this build does with the rules, which is the
    // same number for every file. It stops being the whole truth the moment a
    // rule this build runs could not be run over the file in hand, and the
    // difference is invisible unless it is said here, next to the claim it
    // qualifies.
    if !report.skipped.is_empty() {
        lines.push(format!(
            "{} could not be checked for this file:",
            count(report.skipped.len(), "more rule")
        ));
        lines.extend(
            report
                .skipped
                .iter()
                .map(|skipped| format!("  {}  {}", skipped.code.as_str(), skip_sentence(skipped))),
        );
    }

    // Required by the format specification, which asks the tool to say this out
    // loud rather than let silence read as stability.
    lines.push(
        "The pack format is not frozen yet: it freezes with the first public release that ships packs."
            .to_owned(),
    );
    lines
}

/// Why one rule could not be run over the file in hand.
///
/// Each reason names a different repair, which is the whole argument for keeping
/// them apart: a reader told only "not checked" learns that something is wrong
/// and nothing about what to do next.
#[must_use]
pub fn skip_sentence(skipped: &SkippedRule) -> String {
    let subject = skipped.subject.as_deref().unwrap_or("");
    match skipped.reason {
        SkipReason::TranslatedPackNotNamed => {
            "`translates` names no pack, so there was nothing to compare against.".to_owned()
        }
        SkipReason::TranslatedPackNotFound => format!(
            "the pack this file translates, `{subject}`, is not beside it, so its identifiers could not be read."
        ),
        SkipReason::TranslatedPackUnreadable => format!(
            "`{subject}.toml` is there and would not open. Check its permissions - this is about that file, not this one."
        ),
        SkipReason::TranslatedPackNotUtf8 => format!(
            "`{subject}.toml` is not UTF-8, so it holds no identifiers to compare against. Fix that file, then run this one again."
        ),
        SkipReason::TranslatedPackDidNotParse => format!(
            "`{subject}.toml` is not valid TOML, so nothing could be read out of it. Lint that file first."
        ),
        SkipReason::TranslatedPackIsATranslation => format!(
            "`{subject}` is a translation and not a pack, so it names no entries of its own to compare against. Point `translates` at the pack itself."
        ),
    }
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
                RuleStatus::PartlyChecked =>
                    "checked in part - one half of the rule waits on an unsettled question",
                RuleStatus::NotImplemented => "not checked - not implemented yet",
                RuleStatus::AwaitingDecision =>
                    "not checked - waiting on an unsettled question in the format",
                RuleStatus::RequiresPublishedVersion =>
                    "not checked here - needs the published pack to compare against",
                RuleStatus::PreemptedByEarlierRule =>
                    "not checked here - a parser following the specification refuses the file first, so E004 arrives instead",
            }
        )
    }));
    lines
}

/// What a field should have held, in words rather than in parser vocabulary.
///
/// Looked up in the core rather than repeated here, so that the message and the
/// check cannot disagree about what the format says.
fn expected(field: &str) -> &'static str {
    kind_of(field).map_or("a value of another kind", |kind| kind.as_str())
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
pub fn sentence(problem: &LintProblem) -> String {
    let subject = problem.subject.as_deref().unwrap_or("");
    let owner = problem.owner.as_deref().unwrap_or("");

    match problem.code {
        RuleCode::MissingOrUnsupportedFormat if subject.is_empty() => {
            "no `format` key. Add `format = 1` as the first line, so a later version can refuse this file instead of misreading it.".to_owned()
        }
        RuleCode::MissingOrUnsupportedFormat => format!(
            "format {subject} is newer than this build reads ({SUPPORTED_FORMAT}). Update Naughty Keyboard, or write the pack in format {SUPPORTED_FORMAT}."
        ),
        RuleCode::UnknownKey if owner.is_empty() => format!(
            "unknown top level key `{subject}`. The format has format, pack, values and pairs - check for a typo, because a key nobody reads is a setting nobody applied."
        ),
        RuleCode::UnknownKey if owner == "pack" => format!(
            "the pack declares `{subject}`, which the format does not define. A key nobody reads is a setting nobody applied - check for a typo."
        ),
        RuleCode::UnknownKey => format!(
            "`{owner}` declares `{subject}`, which the format does not define. A key nobody reads is a setting nobody applied - check for a typo."
        ),
        RuleCode::RiskOutsideVocabulary if owner == "pack" => format!(
            "the pack sets `risk` to `{subject}`. The format has normal and offensive, and anything else is read as normal - so a pack meaning to warn about its values would ship them unmarked."
        ),
        RuleCode::RiskOutsideVocabulary => format!(
            "value `{owner}` sets `risk` to `{subject}`. The format has normal and offensive, and anything else is read as normal - so a value meaning to warn would be inserted with no warning shown."
        ),
        RuleCode::TranslatesNonPack => format!(
            "`translates` names `{subject}`, which is itself a translation rather than a source pack. A translation follows a pack, and a chain of them has no pack at the end to follow."
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
        RuleCode::FieldOfUnusableType if owner == "pack" => format!(
            "the pack gives `{subject}` a value the format cannot read as {}. A field of the wrong kind is read as absent everywhere after this, so the setting written here is one nobody applied.",
            expected(subject)
        ),
        RuleCode::FieldOfUnusableType => format!(
            "value `{owner}` gives `{subject}` a value the format cannot read as {}. A field of the wrong kind is read as absent everywhere after this, so what was written here is silently missing.",
            expected(subject)
        ),
        RuleCode::PackWithoutValues => {
            "the pack declares no values. It would load, appear in the palette and insert nothing.".to_owned()
        }
        RuleCode::UnescapedCharacter => format!(
            "value `{owner}` writes {subject} out instead of escaping it. A reviewer cannot see it and an editor can drop it - write it as {subject}."
        ),
        RuleCode::LiteralStringForEscapableValue => format!(
            "value `{owner}` needs {subject} escaped, but is written in single quotes where nothing is an escape. Use double quotes and write {subject}."
        ),
        RuleCode::UnknownValueType => format!(
            "value `{owner}` declares type `{subject}`. This build produces `literal` and `repeat` - the other names are reserved and not yet defined."
        ),
        RuleCode::RepeatCountOutOfRange if subject.parse::<i64>().is_err() => format!(
            "value `{owner}` repeats and declares no `{subject}`. A recipe needs both a unit and a count to describe anything."
        ),
        RuleCode::RepeatCountOutOfRange => format!(
            "value `{owner}` repeats {subject} times, outside the range 1 to 1000000. Above that, inserting takes longer than a tester will wait."
        ),
        RuleCode::ValuePresentForNonLiteralType => format!(
            "value `{owner}` carries a recipe and a `{subject}` at once. Remove one - nothing says which of the two the tool would send."
        ),
        RuleCode::RepeatProductTooLarge => format!(
            "value `{owner}` describes more than 1000000 code points. The count alone is in range - it is the unit multiplied by the count that is not."
        ),
        RuleCode::FieldOutsideVocabulary => format!(
            "value `{owner}` names field kind `{subject}`, which is not in the format's closed list. Pick the nearest listed kind rather than coining one."
        ),
        RuleCode::EscapeOutsideCommonSubset => format!(
            "value `{owner}` uses the escape `{subject}`, which is valid in TOML 1.1.0 and an error in 1.0.0. Write the character itself, so the pack parses for everyone who receives it."
        ),
        RuleCode::LiteralValueVeryLong => format!(
            "value `{owner}` writes out {subject} characters. Consider `type = \"repeat\"` - a recipe stays reviewable where a wall of text does not."
        ),
        // E010 carries one code for two faults, and they send the reader to do
        // different things. Which one it is comes from the same function that
        // decided it in the adapter, so the two cannot drift apart.
        RuleCode::PackIdMismatch if !is_pack_id(subject) => format!(
            "the pack declares id `{subject}`, which is not the shape of a pack identifier. Use lower case letters, digits and hyphens, two to forty characters, starting with a letter."
        ),
        RuleCode::PackIdMismatch => format!(
            "the pack declares id `{subject}`, which is not this file's name. A pack is addressed by its file name, so rename one of the two to match the other."
        ),
        RuleCode::DuplicateValueId => format!(
            "value `{owner}` takes an id a value above it already uses. Ids are unique within a pack and are never reused, or a report citing `{subject}` resolves to two different values."
        ),
        RuleCode::ValueIdMalformed => format!(
            "value `{owner}` has an id outside the format's alphabet. Use lower case letters, digits and hyphens, up to forty eight characters, starting with a letter or a digit."
        ),
        RuleCode::ReplacedByUnknownId => format!(
            "value `{owner}` names `{subject}` as its successor and no value in this pack has that id. A retired value is still cited by reports written years ago, so its successor has to resolve."
        ),
        RuleCode::MissingBreaks => format!(
            "value `{owner}` declares no `breaks`. It is the field this catalogue exists for: say what this value usually breaks and why, or the value is a curiosity rather than a test case."
        ),
        RuleCode::BreaksTooShortOrEchoesName if subject == "name" => format!(
            "value `{owner}` has a `breaks` that only says its name again. The name is already on the screen beside it, so this value arrives with no explanation at all."
        ),
        RuleCode::BreaksTooShortOrEchoesName => format!(
            "value `{owner}` has a `breaks` of {subject} characters, where the format asks for {MIN_BREAKS_CODE_POINTS}. Say what this value usually breaks and why - the sentence is the work this catalogue is made of."
        ),
        RuleCode::MissingExpect => format!(
            "value `{owner}` declares no `expect`. Without it a tester sees what happened and has nothing to compare it against, so a bug and correct behaviour look the same."
        ),
        RuleCode::ValueWithoutSourceInSourcedPack => format!(
            "value `{owner}` names no `source`, in a pack whose other values name theirs. It inherits the pack's, which credits somebody with work that may not be theirs."
        ),
        RuleCode::BreaksProbablyNotEnglish => format!(
            "value `{owner}` has a `breaks` carrying no English word. A source pack is written in English and translated in a file of its own - if this one is English after all, this line is the whole cost."
        ),
        RuleCode::PairEndpointUnknown => format!(
            "pair `{owner}` names `{subject}` as one of its two halves, and no value in this pack has that id. A pair joins values that already exist, so that each half keeps its own description and can be cited on its own."
        ),
        RuleCode::UnknownRelation => format!(
            "pair `{owner}` declares relation `{subject}`. The format has look-alike, range and identity - a relation nothing recognises is a pair the palette cannot present."
        ),
        RuleCode::PairEndpointsIdentical => format!(
            "pair `{owner}` names `{subject}` on both sides. A pair is two values put next to each other, and one value twice compares nothing with nothing."
        ),
        RuleCode::TranslationCarriesValue if owner.is_empty() => format!(
            "this translation carries `{subject}`, which decides what gets inserted into somebody else's application. A translation may change prose and nothing else - that is a safety property of the format, not a matter of tidiness."
        ),
        RuleCode::TranslationCarriesValue => format!(
            "the translation of `{owner}` carries `{subject}`, which decides what gets inserted into somebody else's application. A translation may change prose and nothing else - that is a safety property of the format, not a matter of tidiness."
        ),
        RuleCode::DuplicateValueBody => format!(
            "value `{owner}` is written exactly like `{subject}` earlier in this pack, so pressing the shortcut twice inserts the same thing twice. Usually an escape was lost: check that this value still says what its description claims."
        ),
        RuleCode::TranslatesUnknownPack if is_pack_id(subject) => format!(
            "this file translates `{subject}`, and there is no `{subject}.toml` beside it. A translation follows its pack and cannot be read on its own."
        ),
        RuleCode::TranslatesUnknownPack => format!(
            "`translates` is set to `{subject}`, which is not the shape a pack identifier has: lower case letters, digits and hyphens, starting with a letter. Nothing was looked for on disk under that name."
        ),
        RuleCode::TranslationRefersToMissingId => format!(
            "this translation describes `{subject}`, which the pack it translates does not have. Translations carry no version of their own, so an entry outlives whatever it described - delete it, or point it at the identifier that replaced it."
        ),
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
        //
        // Checked against the register rather than against a number written here.
        // A remembered number turns every new rule into a red test for the wrong
        // reason, and worse, it can stay green while the sentence and the
        // register disagree - which is the one failure this line exists to stop.
        let coverage = RuleCoverage::measure();
        let text = summary(&LintReport::default()).join("\n");

        assert!(text.contains("0 errors, 0 warnings."), "{text}");
        assert!(
            text.contains(&format!(
                "Checked {} of {} rules",
                coverage.checked, coverage.total
            )),
            "{text}"
        );
        assert!(
            text.contains(&format!("{} not checked", coverage.unchecked())),
            "{text}"
        );
        assert!(
            text.contains(&format!("{} of them only in part", coverage.partly)),
            "a rule that runs in part must be visible in the summary: {text}"
        );
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

        report.push(LintProblem::new(RuleCode::UnknownKey));
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
    fn every_reason_a_rule_can_be_skipped_for_says_something_different() {
        // The same guard as the one below, for the other half of the mechanism.
        // A reason added later with no sentence would print an empty line where
        // the explanation belongs, and a reason that reused an existing sentence
        // would send the reader to the wrong repair - which is the only thing
        // these reasons exist to get right.
        let reasons = [
            SkipReason::TranslatedPackNotNamed,
            SkipReason::TranslatedPackNotFound,
            SkipReason::TranslatedPackUnreadable,
            SkipReason::TranslatedPackNotUtf8,
            SkipReason::TranslatedPackDidNotParse,
            SkipReason::TranslatedPackIsATranslation,
        ];
        let mut written = std::collections::HashSet::new();
        for reason in reasons {
            let text = skip_sentence(
                &SkippedRule::new(RuleCode::TranslationRefersToMissingId, reason)
                    .about("unicode-text"),
            );
            assert!(
                text.len() > 20,
                "{} has no sentence worth reading",
                reason.as_str()
            );
            assert!(
                written.insert(text.clone()),
                "{} repeats a sentence another reason already uses: {text}",
                reason.as_str()
            );
        }
    }

    #[test]
    fn the_summary_qualifies_its_own_count_when_a_rule_could_not_be_run_here() {
        // The count above it is a property of the build and is the same in every
        // report. Left alone beside a skipped rule it overstates what happened to
        // the file in hand, which is the one place the overstatement is invisible.
        let mut report = LintReport::default();
        report.skip(
            SkippedRule::new(
                RuleCode::TranslationRefersToMissingId,
                SkipReason::TranslatedPackNotFound,
            )
            .about("unicode-text"),
        );
        let text = summary(&report).join("\n");
        assert!(text.contains("Checked 35 of 45 rules"), "{text}");
        assert!(
            text.contains("1 more rule could not be checked for this file"),
            "{text}"
        );
        assert!(text.contains("W052"), "{text}");

        // And an ordinary report says none of that, or every run would carry a
        // line about nothing.
        let quiet = summary(&LintReport::default()).join("\n");
        assert!(
            !quiet.contains("could not be checked for this file"),
            "{quiet}"
        );
    }

    #[test]
    fn every_rule_that_can_report_has_a_sentence_written_for_it() {
        // The guard that keeps the fallback from becoming a hiding place: a rule
        // this build runs must be able to say what it found.
        //
        // A rule running only in part belongs here too, and did not at first.
        // It reports findings like any other, so leaving it out let a whole
        // status slip past the check that exists to catch exactly this.
        for rule in RULES
            .iter()
            .filter(|r| matches!(r.status, RuleStatus::Checked | RuleStatus::PartlyChecked))
        {
            let text = sentence(&LintProblem::new(rule.code));
            assert!(
                !text.contains("no message written yet"),
                "{} has no sentence",
                rule.code.as_str()
            );
        }
    }
}
