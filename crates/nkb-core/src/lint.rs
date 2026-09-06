//! The validator's rules, as data rather than as a pile of `if` statements.
//!
//! # Why the whole set lives here, including the rules nothing checks yet
//!
//! A validator that runs eight rules and says nothing about the other thirty
//! three reports a clean pack, and the reader takes that to mean the pack is
//! fine. "Nobody looked" and "somebody looked and found nothing" are two
//! different answers, and a tool that renders them identically is lying by
//! omission - which is the exact failure this product exists to find in other
//! people's software.
//!
//! So every rule is registered from the first version, carrying its status.
//! A rule nobody has written yet is visible as unwritten, and the count of
//! unchecked rules is part of the output rather than a footnote.
//!
//! # There is no text in this module
//!
//! A rule carries a code, a severity and a status. The sentence a person reads
//! is assembled one layer out, from a translation key. A core that returns a
//! finished sentence drags the text layer downwards and quietly removes the
//! ability to translate anything.

use crate::value::ValueProblem;

/// Whether a rule blocks a pack or merely reports on it.
///
/// This decides the exit code and nothing else. It is not a priority and not an
/// ordering: the validator reports everything it finds in one run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Blocks the pack. The pack is not loaded at all - whole or absent.
    Error,
    /// Reported and passed through. Does not change what the pack contains.
    Warning,
}

/// Whether a rule was actually run, and if not, why not.
///
/// The distinction between the last three is the point of the type. "Not written
/// yet" is work waiting to be done, "waiting on a decision" is work that cannot
/// begin, and "needs the published version" is work that is impossible here and
/// possible in contribution checks. Collapsing them into one would turn three
/// different answers into the same silence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleStatus {
    /// Run against every pack.
    Checked,
    /// Run, but not in full: one part of the rule needs something that has not
    /// been decided, while the rest works today. Kept apart from both of its
    /// neighbours because reporting it as checked would overstate the verdict
    /// and reporting it as unchecked would contradict the findings it produces -
    /// a reader seeing the code in the output and "not checked" in the register
    /// learns only that one of the two is lying.
    PartlyChecked,
    /// Not implemented yet. Nothing blocks it beyond the work itself.
    NotImplemented,
    /// Cannot be implemented until an open question is settled. Today: which
    /// published set defines a look-alike character, and which Unicode version
    /// it is pinned to.
    AwaitingDecision,
    /// Needs the previously published pack to compare against, which does not
    /// exist on a local disk. Checked where the main branch is available.
    RequiresPublishedVersion,
}

/// Every rule the pack format defines. The names are ours, the codes are public.
///
/// 🔴 The codes are a contract. They travel into `--json` output and other
/// people's pipelines, so a code is assigned once and never renamed, never
/// reused and never renumbered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RuleCode {
    // File and structure.
    MissingOrUnsupportedFormat,
    UnknownTopLevelKey,
    MissingRequiredField,
    NotValidToml,
    NotUtf8OrByteOrderMark,
    LineEndingNotNewline,
    TabUsedForIndentation,
    PackWithoutValues,
    // Identity.
    PackIdMismatch,
    DuplicateValueId,
    ValueIdMalformed,
    PublishedValueChanged,
    ValueIdVanished,
    ReplacedByUnknownId,
    // Value.
    UnescapedCharacter,
    InvalidEscapeSequence,
    LiteralStringForEscapableValue,
    UnknownValueType,
    RepeatCountOutOfRange,
    ValuePresentForNonLiteralType,
    RepeatProductTooLarge,
    UnescapedAmbiguousCharacter,
    FieldOutsideVocabulary,
    EscapeOutsideCommonSubset,
    LiteralValueVeryLong,
    // Description.
    MissingBreaks,
    BreaksTooShortOrEchoesName,
    MissingExpect,
    ValueWithoutSourceInSourcedPack,
    BreaksProbablyNotEnglish,
    // Pairs.
    PairEndpointUnknown,
    UnknownRelation,
    PairEndpointsIdentical,
    // Translations.
    TranslationCarriesValue,
    TranslatesUnknownPack,
    TranslationRefersToMissingId,
    // Style.
    RedundantShape,
    PackWithoutTags,
    PackTooLarge,
    OffensiveRiskNotMentionedInBreaks,
    FileNotCanonical,
}

impl RuleCode {
    /// The published code. Never renamed, never reused, never renumbered.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MissingOrUnsupportedFormat => "E001",
            Self::UnknownTopLevelKey => "E002",
            Self::MissingRequiredField => "E003",
            Self::NotValidToml => "E004",
            Self::NotUtf8OrByteOrderMark => "E005",
            Self::LineEndingNotNewline => "E006",
            Self::TabUsedForIndentation => "E007",
            Self::PackWithoutValues => "E008",
            Self::PackIdMismatch => "E010",
            Self::DuplicateValueId => "E011",
            Self::ValueIdMalformed => "E012",
            Self::PublishedValueChanged => "E013",
            Self::ValueIdVanished => "E014",
            Self::ReplacedByUnknownId => "E015",
            Self::UnescapedCharacter => "E020",
            Self::InvalidEscapeSequence => "E021",
            Self::LiteralStringForEscapableValue => "E022",
            Self::UnknownValueType => "E023",
            Self::RepeatCountOutOfRange => "E024",
            Self::ValuePresentForNonLiteralType => "E025",
            Self::RepeatProductTooLarge => "E026",
            Self::UnescapedAmbiguousCharacter => "E027",
            Self::FieldOutsideVocabulary => "E028",
            Self::EscapeOutsideCommonSubset => "E029",
            Self::LiteralValueVeryLong => "W026",
            Self::MissingBreaks => "E030",
            Self::BreaksTooShortOrEchoesName => "E031",
            Self::MissingExpect => "W032",
            Self::ValueWithoutSourceInSourcedPack => "W033",
            Self::BreaksProbablyNotEnglish => "W034",
            Self::PairEndpointUnknown => "E040",
            Self::UnknownRelation => "E041",
            Self::PairEndpointsIdentical => "E042",
            Self::TranslationCarriesValue => "E050",
            Self::TranslatesUnknownPack => "E051",
            Self::TranslationRefersToMissingId => "W052",
            Self::RedundantShape => "W060",
            Self::PackWithoutTags => "W061",
            Self::PackTooLarge => "W062",
            Self::OffensiveRiskNotMentionedInBreaks => "W063",
            Self::FileNotCanonical => "W064",
        }
    }
}

/// One rule of the pack format: its code, what it costs to break, and whether
/// this build actually runs it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LintRule {
    pub code: RuleCode,
    pub severity: Severity,
    pub status: RuleStatus,
}

use RuleCode as C;
use RuleStatus::{
    AwaitingDecision, Checked, NotImplemented, PartlyChecked, RequiresPublishedVersion,
};
use Severity::{Error, Warning};

const fn rule(code: RuleCode, severity: Severity, status: RuleStatus) -> LintRule {
    LintRule {
        code,
        severity,
        status,
    }
}

/// The complete register. Its length is the number the specification states, and
/// the test below is what keeps the two from drifting apart.
pub const RULES: [LintRule; 41] = [
    rule(C::MissingOrUnsupportedFormat, Error, Checked),
    rule(C::UnknownTopLevelKey, Error, Checked),
    rule(C::MissingRequiredField, Error, Checked),
    rule(C::NotValidToml, Error, Checked),
    rule(C::NotUtf8OrByteOrderMark, Error, Checked),
    rule(C::LineEndingNotNewline, Error, Checked),
    rule(C::TabUsedForIndentation, Error, Checked),
    rule(C::PackWithoutValues, Error, Checked),
    rule(C::PackIdMismatch, Error, NotImplemented),
    rule(C::DuplicateValueId, Error, NotImplemented),
    rule(C::ValueIdMalformed, Error, NotImplemented),
    rule(C::PublishedValueChanged, Error, RequiresPublishedVersion),
    rule(C::ValueIdVanished, Error, RequiresPublishedVersion),
    rule(C::ReplacedByUnknownId, Error, NotImplemented),
    // Control characters, format characters, whitespace other than a plain space
    // and a space at either edge are checked. The look-alike half needs a
    // published set of confusable characters pinned to a Unicode version, and
    // that set has not been chosen - so the rule runs, but not in full.
    rule(C::UnescapedCharacter, Error, PartlyChecked),
    // Unreachable rather than unwritten: a parser that follows the specification
    // rejects a malformed escape as a syntax error, so E004 always arrives first.
    // Whether this rule should exist at all is a question for the format.
    rule(C::InvalidEscapeSequence, Error, AwaitingDecision),
    rule(C::LiteralStringForEscapableValue, Error, Checked),
    rule(C::UnknownValueType, Error, Checked),
    rule(C::RepeatCountOutOfRange, Error, Checked),
    rule(C::ValuePresentForNonLiteralType, Error, Checked),
    rule(C::RepeatProductTooLarge, Error, Checked),
    // Needs canonical decomposition data, which is either a dependency or a
    // generated table pinned to a Unicode version. Neither has been decided.
    rule(C::UnescapedAmbiguousCharacter, Error, AwaitingDecision),
    rule(C::FieldOutsideVocabulary, Error, Checked),
    rule(C::EscapeOutsideCommonSubset, Error, Checked),
    rule(C::LiteralValueVeryLong, Warning, Checked),
    rule(C::MissingBreaks, Error, NotImplemented),
    rule(C::BreaksTooShortOrEchoesName, Error, NotImplemented),
    rule(C::MissingExpect, Warning, NotImplemented),
    rule(C::ValueWithoutSourceInSourcedPack, Warning, NotImplemented),
    rule(C::BreaksProbablyNotEnglish, Warning, NotImplemented),
    rule(C::PairEndpointUnknown, Error, NotImplemented),
    rule(C::UnknownRelation, Error, NotImplemented),
    rule(C::PairEndpointsIdentical, Error, NotImplemented),
    rule(C::TranslationCarriesValue, Error, NotImplemented),
    rule(C::TranslatesUnknownPack, Error, NotImplemented),
    rule(C::TranslationRefersToMissingId, Warning, NotImplemented),
    rule(C::RedundantShape, Warning, NotImplemented),
    rule(C::PackWithoutTags, Warning, NotImplemented),
    rule(C::PackTooLarge, Warning, NotImplemented),
    rule(
        C::OffensiveRiskNotMentionedInBreaks,
        Warning,
        NotImplemented,
    ),
    rule(C::FileNotCanonical, Warning, NotImplemented),
];

/// Looks a rule up by its code.
#[must_use]
pub fn rule_for(code: RuleCode) -> LintRule {
    // Every code has an entry - the test below proves it - so the fallback is
    // unreachable in practice. It is written as a value rather than a panic
    // because a crash inside a validator is a crash inside somebody else's CI.
    let mut index = 0;
    while index < RULES.len() {
        if RULES[index].code as u8 == code as u8 {
            return RULES[index];
        }
        index += 1;
    }
    rule(code, Error, NotImplemented)
}

/// How many rules this build actually runs, and how many it does not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuleCoverage {
    pub checked: usize,
    /// Rules that run but not in full. Counted apart from both neighbours:
    /// folding them into `checked` overstates the verdict, folding them into the
    /// remainder contradicts the findings they produce.
    pub partly: usize,
    pub total: usize,
}

impl RuleCoverage {
    /// Counts the register. Not a stored number: a stored one would drift from
    /// the table it claims to describe.
    #[must_use]
    pub fn measure() -> Self {
        let mut checked = 0;
        let mut partly = 0;
        let mut index = 0;
        while index < RULES.len() {
            match RULES[index].status {
                RuleStatus::Checked => checked += 1,
                RuleStatus::PartlyChecked => partly += 1,
                _ => {}
            }
            index += 1;
        }
        Self {
            checked,
            partly,
            total: RULES.len(),
        }
    }

    #[must_use]
    pub fn unchecked(self) -> usize {
        self.total - self.checked - self.partly
    }
}

/// One rule violation found in one pack file.
///
/// Carries facts and nothing else: a code, a place, and the piece of the file the
/// problem is about. The sentence a person reads is built from these, one layer
/// out, so that this type stays translatable and machine readable at once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LintProblem {
    pub code: RuleCode,
    /// Line in the pack file, counted from one. Absent when the problem is about
    /// the file as a whole rather than a place inside it.
    pub line: Option<u32>,
    /// What the problem is about, taken verbatim from the file or from the
    /// parser: a key name, a field name, a value id, a syntax message.
    ///
    /// Never a sentence we composed. That distinction is what keeps this type
    /// out of the translation layer.
    pub subject: Option<String>,
    /// The value the subject belongs to, when the subject alone would not say
    /// which one. Empty for problems about the pack itself.
    pub owner: Option<String>,
}

impl LintProblem {
    #[must_use]
    pub fn new(code: RuleCode) -> Self {
        Self {
            code,
            line: None,
            subject: None,
            owner: None,
        }
    }

    #[must_use]
    pub fn at(mut self, line: u32) -> Self {
        self.line = Some(line);
        self
    }

    #[must_use]
    pub fn about(mut self, subject: impl Into<String>) -> Self {
        self.subject = Some(subject.into());
        self
    }

    #[must_use]
    pub fn owned_by(mut self, owner: impl Into<String>) -> Self {
        self.owner = Some(owner.into());
        self
    }

    /// The severity this problem carries, taken from the register rather than
    /// decided at the point the problem is built.
    #[must_use]
    pub fn severity(&self) -> Severity {
        rule_for(self.code).severity
    }
}

/// Turns a size problem found by the value model into a lint problem.
///
/// The mapping lives here rather than in the adapter so that the value model
/// keeps knowing nothing about the validator, while the codes it already
/// publishes stay the same ones.
impl From<ValueProblem> for LintProblem {
    fn from(problem: ValueProblem) -> Self {
        let code = match problem {
            ValueProblem::RepeatCountOutOfRange { .. } => RuleCode::RepeatCountOutOfRange,
            ValueProblem::RepeatProductTooLarge { .. }
            | ValueProblem::RepeatProductUnmeasurable => RuleCode::RepeatProductTooLarge,
        };
        Self::new(code)
    }
}

/// The verdict on one pack file.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LintReport {
    pub problems: Vec<LintProblem>,
}

impl LintReport {
    #[must_use]
    pub fn errors(&self) -> usize {
        self.problems
            .iter()
            .filter(|p| p.severity() == Severity::Error)
            .count()
    }

    #[must_use]
    pub fn warnings(&self) -> usize {
        self.problems
            .iter()
            .filter(|p| p.severity() == Severity::Warning)
            .count()
    }

    /// Whether the pack may be loaded. Warnings never make this false: a pack
    /// with warnings loads normally and the warnings are shown alongside it.
    #[must_use]
    pub fn accepted(&self) -> bool {
        self.errors() == 0
    }

    pub fn push(&mut self, problem: LintProblem) {
        self.problems.push(problem);
    }

    pub fn extend(&mut self, problems: impl IntoIterator<Item = LintProblem>) {
        self.problems.extend(problems);
    }

    /// Orders problems the way a person reads a file: top to bottom, and within
    /// one line by rule code so that two runs over the same file agree.
    pub fn sort(&mut self) {
        self.problems
            .sort_by(|a, b| a.line.cmp(&b.line).then(a.code.cmp(&b.code)));
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn the_register_holds_the_number_of_rules_the_specification_states() {
        // The specification says forty one. If this number has to change, the
        // specification changed too, and that is a decision rather than a tidy up.
        assert_eq!(RULES.len(), 41);
    }

    #[test]
    fn no_published_code_is_used_twice() {
        // A duplicated code would make two different problems indistinguishable
        // in `--json`, which is the one place where they must not be.
        let codes: HashSet<&str> = RULES.iter().map(|r| r.code.as_str()).collect();
        assert_eq!(codes.len(), RULES.len());
    }

    #[test]
    fn every_code_starts_with_the_letter_its_severity_implies() {
        // `E` blocks, `W` passes through. A rule whose letter disagrees with its
        // severity would tell a contributor one thing and the exit code another.
        for r in &RULES {
            let first = r.code.as_str().chars().next().expect("codes are not empty");
            match r.severity {
                Severity::Error => assert_eq!(first, 'E', "{}", r.code.as_str()),
                Severity::Warning => assert_eq!(first, 'W', "{}", r.code.as_str()),
            }
        }
    }

    #[test]
    fn looking_up_a_rule_returns_the_registered_one() {
        let found = rule_for(RuleCode::NotValidToml);
        assert_eq!(found.code.as_str(), "E004");
        assert_eq!(found.severity, Severity::Error);
        assert_eq!(found.status, RuleStatus::Checked);
    }

    #[test]
    fn coverage_counts_the_register_rather_than_repeating_a_stored_number() {
        let coverage = RuleCoverage::measure();
        assert_eq!(coverage.total, 41);
        // Every rule falls in exactly one bucket. If this ever fails, some rule
        // is being counted twice or not at all, and the summary that a reader
        // trusts to say what was not looked at has quietly stopped adding up.
        assert_eq!(
            coverage.checked + coverage.partly + coverage.unchecked(),
            coverage.total
        );
        assert_eq!(coverage.checked, 16);
        assert_eq!(coverage.partly, 1);
    }

    #[test]
    fn a_rule_that_runs_only_in_part_is_not_reported_as_either_neighbour() {
        // E020 finds control characters, format characters and edge spaces today,
        // and cannot find look-alikes until a confusable set is chosen. Calling it
        // checked would overstate the verdict; calling it unchecked would
        // contradict every E020 the tool actually reports.
        assert_eq!(
            rule_for(RuleCode::UnescapedCharacter).status,
            RuleStatus::PartlyChecked
        );
    }

    #[test]
    fn the_rules_blocked_on_an_open_question_say_so_rather_than_looking_unwritten() {
        // Not waiting for somebody to type them out - waiting for a decision.
        // Reporting them as merely unimplemented hides a blocked question behind
        // a to-do, and nobody goes looking for a to-do.
        for code in [
            RuleCode::UnescapedAmbiguousCharacter,
            RuleCode::InvalidEscapeSequence,
        ] {
            assert_eq!(
                rule_for(code).status,
                RuleStatus::AwaitingDecision,
                "{}",
                code.as_str()
            );
        }
    }

    #[test]
    fn the_two_rules_needing_the_published_pack_are_not_confused_with_unwritten_ones() {
        assert_eq!(
            rule_for(RuleCode::PublishedValueChanged).status,
            RuleStatus::RequiresPublishedVersion
        );
        assert_eq!(
            rule_for(RuleCode::ValueIdVanished).status,
            RuleStatus::RequiresPublishedVersion
        );
    }

    #[test]
    fn warnings_do_not_block_a_pack_and_errors_do() {
        let mut report = LintReport::default();
        report.push(LintProblem::new(RuleCode::PackWithoutTags));
        assert!(report.accepted(), "a warning alone must not block a pack");
        assert_eq!(report.warnings(), 1);

        report.push(LintProblem::new(RuleCode::PackWithoutValues));
        assert!(!report.accepted());
        assert_eq!(report.errors(), 1);
    }

    #[test]
    fn a_size_problem_keeps_the_code_the_value_model_already_publishes() {
        let problem: LintProblem = ValueProblem::RepeatCountOutOfRange { count: 0 }.into();
        assert_eq!(problem.code.as_str(), "E024");

        let problem: LintProblem = ValueProblem::RepeatProductUnmeasurable.into();
        assert_eq!(problem.code.as_str(), "E026");
    }

    #[test]
    fn problems_are_ordered_by_place_so_two_runs_agree() {
        let mut report = LintReport::default();
        report.push(LintProblem::new(RuleCode::PackWithoutValues).at(9));
        report.push(LintProblem::new(RuleCode::NotValidToml).at(2));
        report.push(LintProblem::new(RuleCode::UnknownTopLevelKey));
        report.sort();

        let order: Vec<Option<u32>> = report.problems.iter().map(|p| p.line).collect();
        assert_eq!(order, vec![None, Some(2), Some(9)]);
    }
}
