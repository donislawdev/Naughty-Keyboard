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

/// How many values a pack may hold before the format suggests splitting it.
///
/// A warning and never a refusal: a large pack is awkward, not wrong.
///
/// The specification called this number a hunch and asked for it to be confirmed
/// or dropped. Measured against the catalogue on 2026-09-07: the packs hold
/// between ten and seventeen values, median twelve, and the largest is
/// `look-alike-pairs`. Sixty is three and a half times that - far enough not to
/// nag a curator, close enough to catch the failure it exists for, which is a
/// word list emptied into one file.
///
/// Counts `[[values]]` only. A pair joins two values that are already counted.
pub const MAX_VALUES_PER_PACK: usize = 60;

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
    /// Cannot be implemented until an open question is settled. Today two of
    /// them: which published set defines a look-alike character and which
    /// Unicode version it is pinned to, and what the description a tool works
    /// out for an invisible character actually says.
    AwaitingDecision,
    /// Needs the previously published pack to compare against, which does not
    /// exist on a local disk. Checked where the main branch is available.
    RequiresPublishedVersion,
    /// A rule of the format that this build can never raise, because another
    /// rule always reports the same file first. Today: a parser that follows the
    /// specification refuses a malformed escape as a syntax error, so E004
    /// arrives instead of E021, and no file exists that would produce E021 here.
    ///
    /// Kept apart from every neighbour on purpose. It is not unwritten work, it
    /// is not waiting on a decision, and it is not a rule needing the published
    /// pack - it is a constraint of the format that a different implementation,
    /// with a more forgiving parser, would have to check itself.
    PreemptedByEarlierRule,
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
    UnknownKey,
    MissingRequiredField,
    NotValidToml,
    NotUtf8OrByteOrderMark,
    LineEndingNotNewline,
    TabUsedForIndentation,
    PackWithoutValues,
    FieldOfUnusableType,
    // Identity.
    PackIdMismatch,
    DuplicateValueId,
    ValueIdMalformed,
    PublishedValueChanged,
    ValueIdVanished,
    ReplacedByUnknownId,
    DuplicateValueBody,
    RiskOutsideVocabulary,
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
    TranslatesNonPack,
    // Style.
    RedundantShape,
    PackWithoutTags,
    PackTooLarge,
    OffensiveValueInOrdinaryPack,
    FileNotCanonical,
}

impl RuleCode {
    /// The published code. Never renamed, never reused, never renumbered.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MissingOrUnsupportedFormat => "E001",
            Self::UnknownKey => "E002",
            Self::MissingRequiredField => "E003",
            Self::NotValidToml => "E004",
            Self::NotUtf8OrByteOrderMark => "E005",
            Self::LineEndingNotNewline => "E006",
            Self::TabUsedForIndentation => "E007",
            Self::PackWithoutValues => "E008",
            Self::FieldOfUnusableType => "E009",
            Self::PackIdMismatch => "E010",
            Self::DuplicateValueId => "E011",
            Self::ValueIdMalformed => "E012",
            Self::PublishedValueChanged => "E013",
            Self::ValueIdVanished => "E014",
            Self::ReplacedByUnknownId => "E015",
            Self::DuplicateValueBody => "E016",
            Self::RiskOutsideVocabulary => "E017",
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
            Self::TranslatesNonPack => "E053",
            Self::RedundantShape => "W060",
            Self::PackWithoutTags => "W061",
            Self::PackTooLarge => "W062",
            Self::OffensiveValueInOrdinaryPack => "W063",
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
    AwaitingDecision, Checked, NotImplemented, PartlyChecked, PreemptedByEarlierRule,
    RequiresPublishedVersion,
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
pub const RULES: [LintRule; 45] = [
    rule(C::MissingOrUnsupportedFormat, Error, Checked),
    rule(C::UnknownKey, Error, Checked),
    rule(C::MissingRequiredField, Error, Checked),
    rule(C::NotValidToml, Error, Checked),
    rule(C::NotUtf8OrByteOrderMark, Error, Checked),
    rule(C::LineEndingNotNewline, Error, Checked),
    rule(C::TabUsedForIndentation, Error, Checked),
    rule(C::PackWithoutValues, Error, Checked),
    // Cuts across every table rather than belonging to one, which is why it sits
    // with the file and structure rules: a field of the wrong kind is read as
    // absent by everything downstream, so the setting the author wrote is one
    // nobody applied.
    rule(C::FieldOfUnusableType, Error, Checked),
    rule(C::PackIdMismatch, Error, Checked),
    rule(C::DuplicateValueId, Error, Checked),
    rule(C::ValueIdMalformed, Error, Checked),
    rule(C::PublishedValueChanged, Error, RequiresPublishedVersion),
    rule(C::ValueIdVanished, Error, RequiresPublishedVersion),
    rule(C::ReplacedByUnknownId, Error, Checked),
    // Sits with identity rather than with the value rules, beside E011. Both
    // ask whether two entries in one pack are really one entry: E011 about the
    // name, this one about what the name stands for.
    rule(C::DuplicateValueBody, Error, Checked),
    // A value rule carrying an identity band number, because E020-E029 filled
    // up. Only two things about a code are contract: that it is unique and that
    // it is never reused. Which ten it falls in was a reading aid, and this is
    // the point where it stopped being achievable - said out loud rather than
    // left for the next reader to puzzle over.
    rule(C::RiskOutsideVocabulary, Error, Checked),
    // Control characters, format characters, whitespace other than a plain space
    // and a space at either edge are checked. The look-alike half needs a
    // published set of confusable characters pinned to a Unicode version, and
    // that set has not been chosen - so the rule runs, but not in full.
    rule(C::UnescapedCharacter, Error, PartlyChecked),
    // Stays in the set, and can never fire here. The set describes the format,
    // not this validator: "a malformed escape makes the file invalid" is a
    // constraint of the format, while "our parser refuses the file first" is a
    // property of our parser. An implementation with a more forgiving parser has
    // to check this itself. Decided rather than deferred - `decision-log.md` D29.
    rule(C::InvalidEscapeSequence, Error, PreemptedByEarlierRule),
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
    rule(C::MissingBreaks, Error, Checked),
    rule(C::BreaksTooShortOrEchoesName, Error, Checked),
    rule(C::MissingExpect, Warning, Checked),
    rule(C::ValueWithoutSourceInSourcedPack, Warning, Checked),
    rule(C::BreaksProbablyNotEnglish, Warning, Checked),
    rule(C::PairEndpointUnknown, Error, Checked),
    rule(C::UnknownRelation, Error, Checked),
    rule(C::PairEndpointsIdentical, Error, Checked),
    rule(C::TranslationCarriesValue, Error, Checked),
    rule(C::TranslatesUnknownPack, Error, Checked),
    rule(C::TranslationRefersToMissingId, Warning, Checked),
    // The rule that stops this build from saying something untrue. Without it a
    // translation of a translation is reported as a translation of a pack that
    // lost every one of its values - a sentence that is false, and that sends a
    // contributor to delete entries which are correct.
    rule(C::TranslatesNonPack, Error, Checked),
    // Compares `shape` against the description the tool works out on its own,
    // and that description does not exist: nothing in this build turns a value
    // into "three zero width spaces and a trailing space". Writing one here
    // would be designing user facing text - a translated surface, with a wording
    // nobody has settled - inside a set of style rules. Registered as waiting on
    // a question rather than as unwritten work, because the work is not the part
    // that is missing.
    rule(C::RedundantShape, Warning, AwaitingDecision),
    rule(C::PackWithoutTags, Warning, Checked),
    rule(C::PackTooLarge, Warning, Checked),
    rule(C::OffensiveValueInOrdinaryPack, Warning, Checked),
    rule(C::FileNotCanonical, Warning, Checked),
];

/// Looks a rule up by its code.
#[must_use]
pub fn rule_for(code: RuleCode) -> LintRule {
    // Every code has an entry, and
    // `the_register_holds_an_entry_for_every_code_the_enum_can_spell` is
    // what proves it - until 2026-09-08 this sentence promised a proof that
    // did not exist, and a rule with no entry passed every check there was
    // (OBS-95). So the fallback is unreachable in a build whose tests ran. It
    // is written as a value rather than a panic because a crash inside a
    // validator is a crash inside somebody else's CI.
    let mut index = 0;
    while index < RULES.len() {
        // Compared directly rather than through a cast to a byte. The cast
        // was safe at forty five variants and would have stayed safe for a
        // long time, but it is a silent truncation in the one place where
        // two rules becoming indistinguishable is exactly what the test
        // about unique codes exists to prevent. OBS-99.
        if RULES[index].code == code {
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

/// Why one rule could not be checked **for one particular file**.
///
/// # Why this is not a seventh `RuleStatus`
///
/// The two types answer questions on different axes and only look alike. A
/// [`RuleStatus`] is a property of this build: it is the same for every file and
/// is known before any file is read. A `SkipReason` is a property of one run over
/// one file, decided by something outside that file - whether the pack it needs
/// is there, opens, and parses.
///
/// Folding them together would force one of two lies. Either a rule this build
/// runs is registered as unchecked because one file could not be compared, or a
/// file that was never compared is reported under a status that says the rule
/// runs everywhere. Both are the silence that rule 1 of the project forbids, in
/// the one place nobody would go looking for it.
///
/// 🔴 These names are published in `--json`. A consumer must treat a reason it
/// does not recognise as "not checked" rather than stop, and the list grows only
/// by addition - `ux-spec.md` 10.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// `translates` names no pack at all: not text, or text outside the pack
    /// identifier alphabet. Nothing can be resolved from it, and no file system
    /// is touched - a pack file comes from a stranger, and a name that is not an
    /// identifier is not a name we go looking for on a disk.
    TranslatedPackNotNamed,
    /// The pack named by `translates` is not beside this file.
    TranslatedPackNotFound,
    /// It is there and will not open: permissions, a device, a handle.
    TranslatedPackUnreadable,
    /// It opened and is not UTF-8, so it holds no identifiers to compare against.
    TranslatedPackNotUtf8,
    /// It is text and is not TOML, so nothing could be read out of it.
    TranslatedPackDidNotParse,
    /// It parses and is itself a translation, so it holds no identifiers of its
    /// own to be followed. Kept apart from the four above because the repair is
    /// in **this** file - point `translates` at the pack, not at a translation
    /// of it - while the others are repairs to the neighbour.
    TranslatedPackIsATranslation,
}

impl SkipReason {
    /// The published name of a reason. Part of the contract, like a rule code.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TranslatedPackNotNamed => "translated-pack-not-named",
            Self::TranslatedPackNotFound => "translated-pack-not-found",
            Self::TranslatedPackUnreadable => "translated-pack-unreadable",
            Self::TranslatedPackNotUtf8 => "translated-pack-not-utf8",
            Self::TranslatedPackDidNotParse => "translated-pack-did-not-parse",
            Self::TranslatedPackIsATranslation => "translated-pack-is-a-translation",
        }
    }
}

/// One rule that this build runs and could not run over one file.
///
/// Recorded only when the rule **had something to check and could not**, never
/// when it had nothing to say. A rule about translations is not skipped over a
/// source pack: it looked, and there was no translation to judge. That is
/// "somebody looked and found nothing", which is already the quiet answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedRule {
    pub code: RuleCode,
    pub reason: SkipReason,
    /// What the rule needed and did not get - the pack identifier, when there
    /// was a usable one. Absent when the file named nothing that could be looked
    /// for.
    pub subject: Option<String>,
}

impl SkippedRule {
    #[must_use]
    pub fn new(code: RuleCode, reason: SkipReason) -> Self {
        Self {
            code,
            reason,
            subject: None,
        }
    }

    #[must_use]
    pub fn about(mut self, subject: impl Into<String>) -> Self {
        self.subject = Some(subject.into());
        self
    }
}

/// The verdict on one pack file.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LintReport {
    pub problems: Vec<LintProblem>,
    /// Rules this build runs that could not be run over this file. Empty for
    /// almost every file, and the whole point of the type when it is not.
    pub skipped: Vec<SkippedRule>,
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

    /// Records that a rule this build runs could not be run over this file.
    pub fn skip(&mut self, skipped: SkippedRule) {
        self.skipped.push(skipped);
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
        // The specification says forty five. If this number has to change, the
        // specification changed too, and that is a decision rather than a tidy up.
        assert_eq!(RULES.len(), 45);
    }

    /// Every code the enum can spell has an entry in the register.
    ///
    /// # Why this reads its own source instead of asking the type
    ///
    /// Rust has no reflection over an enum, so there is no way to ask `RuleCode`
    /// for its variants. The compiler does force `as_str` to cover all of them,
    /// because that match is exhaustive - but it cannot force a line in `RULES`,
    /// because a table of data is not a match. That gap is the whole reason this
    /// test exists.
    ///
    /// # What it was measured to catch
    ///
    /// A forty sixth variant was added on 2026-09-08 with a public code and no
    /// entry here, and then written into `pack-format.md` 11 as well - the
    /// natural order of work, document before code. Every check passed:
    /// `cargo test` was green, `sprawdz-spojnosc.py` said everything agreed, and
    /// `nkb lint` reported "Checked 39 of 45 rules" while the format had 46. The
    /// rule appeared nowhere: not among the checked, not among the skipped, not
    /// in the total.
    ///
    /// 🔴 The cause was not the fallback in `rule_for`. It was `RuleCoverage`
    /// counting `RULES.len()`, so a rule outside the table does not exist in the
    /// account before anybody asks about its status. OBS-95.
    #[test]
    fn the_register_holds_an_entry_for_every_code_the_enum_can_spell() {
        // The arrow as written in `as_str`. Built as a constant so that this
        // line does not itself look like one of the arms it is looking for.
        const ARROW: &str = " => \"";

        let source = include_str!("lint.rs");
        let spelled: HashSet<&str> = source
            .lines()
            .filter(|line| line.contains("Self::") && line.contains(ARROW))
            .filter_map(|line| {
                let after = line.split_once(ARROW)?.1;
                let code = after.split_once('"')?.0;
                let mut characters = code.chars();
                // A published code is a letter and three digits. Anything else
                // on such a line belongs to some other match and is not ours.
                match characters.next() {
                    Some('E' | 'W') => {}
                    _ => return None,
                }
                let digits: String = characters.collect();
                (digits.len() == 3 && digits.chars().all(|c| c.is_ascii_digit())).then_some(code)
            })
            .collect();

        // The negative control. Were the pattern above to stop matching - a
        // reformatted `as_str`, a different arrow - both sets could end up empty
        // and this test would pass by comparing nothing with nothing.
        assert!(
            spelled.len() > 40,
            "the scan found {} codes in this file's own source, which means it is no longer \
             reading `as_str` and proves nothing",
            spelled.len()
        );

        let registered: HashSet<&str> = RULES.iter().map(|r| r.code.as_str()).collect();

        let mut unregistered: Vec<&&str> = spelled.difference(&registered).collect();
        unregistered.sort_unstable();
        assert!(
            unregistered.is_empty(),
            "these codes exist and have no entry in RULES: {unregistered:?}. A rule outside the \
             table is counted by nothing: `nkb lint` would report a total that leaves it out, and \
             it would appear neither among the checked rules nor among the skipped ones."
        );

        let mut unspelled: Vec<&&str> = registered.difference(&spelled).collect();
        unspelled.sort_unstable();
        assert!(
            unspelled.is_empty(),
            "these codes are registered and the scan did not find them in `as_str`: {unspelled:?}. \
             Either a code lost its spelling, or this test stopped reading the source correctly - \
             and the second would make the check above worthless."
        );
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
        assert_eq!(coverage.total, 45);
        // Every rule falls in exactly one bucket. If this ever fails, some rule
        // is being counted twice or not at all, and the summary that a reader
        // trusts to say what was not looked at has quietly stopped adding up.
        assert_eq!(
            coverage.checked + coverage.partly + coverage.unchecked(),
            coverage.total
        );
        // Written out rather than computed, deliberately. A number derived from
        // the register would agree with the register whatever the register said,
        // and this is the line that makes somebody look when coverage moves.
        assert_eq!(coverage.checked, 39);
        assert_eq!(coverage.partly, 1);
    }

    #[test]
    fn a_skipped_rule_is_only_ever_one_this_build_actually_runs() {
        // The invariant that keeps the two axes from collapsing into one. A rule
        // nobody wrote is unchecked for every file, and saying so per file would
        // be the same silence recorded twice - while a rule this build runs and
        // could not run here is the one case the per file record exists for.
        //
        // Written as a check over the register rather than over a sample, because
        // the mistake it guards against arrives with a rule added later.
        for reason in [
            SkipReason::TranslatedPackNotNamed,
            SkipReason::TranslatedPackNotFound,
            SkipReason::TranslatedPackUnreadable,
            SkipReason::TranslatedPackNotUtf8,
            SkipReason::TranslatedPackDidNotParse,
            SkipReason::TranslatedPackIsATranslation,
        ] {
            let skipped = SkippedRule::new(RuleCode::TranslationRefersToMissingId, reason);
            let status = rule_for(skipped.code).status;
            assert!(
                matches!(status, RuleStatus::Checked | RuleStatus::PartlyChecked),
                "{} is registered as {status:?}, so it cannot be skipped for one file",
                skipped.code.as_str()
            );
        }
    }

    #[test]
    fn every_skip_reason_has_a_published_name_and_no_two_share_one() {
        // The names travel into `--json` beside the rule codes and are read the
        // same way. Two reasons rendering as one string would make two different
        // repairs indistinguishable to a check that matches on the field.
        let reasons = [
            SkipReason::TranslatedPackNotNamed,
            SkipReason::TranslatedPackNotFound,
            SkipReason::TranslatedPackUnreadable,
            SkipReason::TranslatedPackNotUtf8,
            SkipReason::TranslatedPackDidNotParse,
            SkipReason::TranslatedPackIsATranslation,
        ];
        let names: HashSet<&str> = reasons.iter().map(|r| r.as_str()).collect();
        assert_eq!(names.len(), reasons.len());
        for name in names {
            assert!(!name.is_empty());
            assert!(
                name.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
                "{name} is not in the shape the other published names use"
            );
        }
    }

    #[test]
    fn a_report_with_no_findings_and_a_skipped_rule_is_not_a_clean_report() {
        // The failure this whole mechanism exists to prevent: empty `problems`
        // reading as "the pack is fine" when the truth is that a rule never ran.
        // The verdict is still acceptance - a rule that could not run has found
        // nothing to block on - so the skip has to be visible beside it or it is
        // invisible altogether.
        let mut report = LintReport::default();
        report.skip(
            SkippedRule::new(
                RuleCode::TranslationRefersToMissingId,
                SkipReason::TranslatedPackNotFound,
            )
            .about("unicode-text"),
        );
        assert!(report.accepted());
        assert_eq!(report.problems.len(), 0);
        assert_eq!(report.skipped.len(), 1);
        assert_eq!(report.skipped[0].subject.as_deref(), Some("unicode-text"));
    }

    #[test]
    fn a_rule_that_runs_only_in_part_is_not_reported_as_either_neighbour() {
        // E020 finds control characters, format characters and edge spaces today,
        // and cannot find look-alikes until a confusable set is chosen. Calling it
        // checked would overstate the verdict. Calling it unchecked would
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
        assert_eq!(
            rule_for(RuleCode::UnescapedAmbiguousCharacter).status,
            RuleStatus::AwaitingDecision
        );
    }

    #[test]
    fn the_rule_another_rule_always_beats_says_that_rather_than_looking_deferred() {
        // E021 waits for nothing. A parser that follows the specification refuses
        // a malformed escape outright, so E004 arrives in its place and no file
        // can be written that would produce E021 here. Reporting it as waiting on
        // a decision would be a sentence that stopped being true the day the
        // decision was taken.
        assert_eq!(
            rule_for(RuleCode::InvalidEscapeSequence).status,
            RuleStatus::PreemptedByEarlierRule
        );
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
        report.push(LintProblem::new(RuleCode::UnknownKey));
        report.sort();

        let order: Vec<Option<u32>> = report.problems.iter().map(|p| p.line).collect();
        assert_eq!(order, vec![None, Some(2), Some(9)]);
    }
}
