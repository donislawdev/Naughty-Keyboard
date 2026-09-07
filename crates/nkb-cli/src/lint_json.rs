//! The machine readable verdict: `nkb lint --json`.
//!
//! # This is a published contract
//!
//! Contribution checks read this, and so will anything anybody writes around
//! the catalogue. A field removed or given a new meaning breaks those without a
//! single change on their side, so the shape below is settled deliberately and
//! grows only by addition. A consumer must ignore members it does not know.
//!
//! # Three decisions worth knowing before changing anything here
//!
//! **`files` is a list even for one file.** The command takes one path today and
//! will take a set of them, because a contribution check reviews a change rather
//! than a file. Flattening one file to the top would mean moving four members
//! down a level later, which is a breaking change bought for nothing.
//!
//! **There is no timestamp**, although a lasting artifact is otherwise expected
//! to carry the metadata of the run that produced it. Two runs over one file
//! have to produce the same bytes so that a check can compare its own output,
//! and the date is in the build log anyway. Determinism is the harder of the two
//! to get back.
//!
//! **The verdict appears twice**, once per file and once for the whole run.
//! Continuous integration needs one bit and a person needs the detail, and
//! computing the first from the second is exactly the step a caller forgets.

use crate::json::Json;
use crate::lint_report;
use nkb_adapters::toml_pack::SUPPORTED_FORMAT;
use nkb_app::LintOutcome;
use nkb_core::lint::{
    LintProblem, LintReport, RULES, RuleCoverage, RuleStatus, Severity, SkippedRule,
};

/// Version of **this** output, not of the tool and not of the pack format.
///
/// Raised only by a breaking change: a member removed, or one whose meaning
/// changed. Adding a member does not raise it, because a consumer that ignores
/// what it does not recognise is unaffected - and the contract says it must.
const SCHEMA: i64 = 1;

/// Builds the whole document.
#[must_use]
pub fn document(files: &[(String, LintOutcome)], tool_version: &str) -> Json {
    let judged: Vec<&LintReport> = files
        .iter()
        .filter_map(|(_, outcome)| match outcome {
            LintOutcome::Judged(report) => Some(report),
            _ => None,
        })
        .collect();

    // True only when every file was judged and every judgement passed. A file
    // that could not be read is not a pass, and folding it into one would let a
    // wrong path report success.
    let accepted = files.len() == judged.len() && judged.iter().all(|r| r.accepted());

    Json::Object(vec![
        ("schema".to_owned(), Json::Number(SCHEMA)),
        ("tool_version".to_owned(), Json::text(tool_version)),
        ("pack_format".to_owned(), Json::Number(SUPPORTED_FORMAT)),
        // Stated rather than implied: the format is still changeable, and silence
        // about that reads as stability to anybody building on it.
        ("format_frozen".to_owned(), Json::Bool(false)),
        ("accepted".to_owned(), Json::Bool(accepted)),
        (
            "counts".to_owned(),
            counts(
                judged.iter().map(|r| r.errors()).sum(),
                judged.iter().map(|r| r.warnings()).sum(),
            ),
        ),
        (
            "files".to_owned(),
            Json::Array(files.iter().map(|(path, o)| file(path, o)).collect()),
        ),
        ("rules".to_owned(), rules()),
    ])
}

fn counts(errors: usize, warnings: usize) -> Json {
    Json::Object(vec![
        ("errors".to_owned(), Json::count(errors)),
        ("warnings".to_owned(), Json::count(warnings)),
    ])
}

/// One file's verdict.
///
/// `accepted` is null when the file was never judged. "Nobody looked" is a third
/// answer beside pass and fail, and rendering it as either would be the tool
/// stating something it does not know.
fn file(path: &str, outcome: &LintOutcome) -> Json {
    let (status, accepted, report) = match outcome {
        LintOutcome::Judged(report) => ("judged", Json::Bool(report.accepted()), Some(report)),
        LintOutcome::NotFound => ("not-found", Json::Null, None),
        LintOutcome::Unreadable => ("unreadable", Json::Null, None),
    };

    Json::Object(vec![
        ("file".to_owned(), Json::text(path)),
        ("status".to_owned(), Json::text(status)),
        ("accepted".to_owned(), accepted),
        (
            "counts".to_owned(),
            counts(
                report.map_or(0, LintReport::errors),
                report.map_or(0, LintReport::warnings),
            ),
        ),
        (
            "problems".to_owned(),
            Json::Array(
                report
                    .map(|r| r.problems.iter().map(|p| problem(p, path)).collect())
                    .unwrap_or_default(),
            ),
        ),
        (
            "rules_not_checked_here".to_owned(),
            Json::Array(
                report
                    .map(|r| r.skipped.iter().map(skipped_rule).collect())
                    .unwrap_or_default(),
            ),
        ),
    ])
}

/// One rule this build runs and could not run over this file.
///
/// # Why this is not folded into `rules.not_fully_checked`
///
/// That list is about the build and is identical in every document this build
/// writes. This one is about the file beside it. A consumer reading the wrong one
/// would conclude that this build checks all but one rule, which is the failure
/// the whole `rules` block exists to prevent.
///
/// So the two are told apart by shape as well as by position: an entry here
/// carries `reason` and `subject`, an entry there carries `status`. A consumer
/// that reaches for the wrong member fails where it can be seen, rather than
/// reading a plausible wrong answer.
fn skipped_rule(skipped: &SkippedRule) -> Json {
    Json::Object(vec![
        ("rule".to_owned(), Json::text(skipped.code.as_str())),
        ("reason".to_owned(), Json::text(skipped.reason.as_str())),
        (
            "subject".to_owned(),
            skipped.subject.as_deref().map_or(Json::Null, Json::text),
        ),
        (
            "message".to_owned(),
            Json::text(lint_report::skip_sentence(skipped)),
        ),
    ])
}

/// One problem.
///
/// `subject` and `owner` sit beside the message rather than inside it, so that a
/// check can match on a field instead of reading a sentence. The sentence is for
/// a person and may be reworded; the two fields beside it may not.
fn problem(problem: &LintProblem, path: &str) -> Json {
    Json::Object(vec![
        ("rule".to_owned(), Json::text(problem.code.as_str())),
        (
            "severity".to_owned(),
            Json::text(match problem.severity() {
                Severity::Error => "error",
                Severity::Warning => "warning",
            }),
        ),
        ("file".to_owned(), Json::text(path)),
        (
            "line".to_owned(),
            problem
                .line
                .map_or(Json::Null, |line| Json::Number(i64::from(line))),
        ),
        (
            "subject".to_owned(),
            problem.subject.as_deref().map_or(Json::Null, Json::text),
        ),
        (
            "owner".to_owned(),
            problem.owner.as_deref().map_or(Json::Null, Json::text),
        ),
        (
            "message".to_owned(),
            Json::text(lint_report::sentence(problem)),
        ),
    ])
}

/// What this build checked, and what it did not.
///
/// The list is the half that keeps an empty `problems` honest. Without it a
/// consumer reads no findings as a clean pack, when the truth may be that
/// twenty four rules were never run.
fn rules() -> Json {
    let coverage = RuleCoverage::measure();
    let not_fully_checked: Vec<Json> = RULES
        .iter()
        .filter(|rule| rule.status != RuleStatus::Checked)
        .map(|rule| {
            Json::Object(vec![
                ("rule".to_owned(), Json::text(rule.code.as_str())),
                ("status".to_owned(), Json::text(status_name(rule.status))),
            ])
        })
        .collect();

    Json::Object(vec![
        ("total".to_owned(), Json::count(coverage.total)),
        ("checked".to_owned(), Json::count(coverage.checked)),
        ("partly_checked".to_owned(), Json::count(coverage.partly)),
        ("unchecked".to_owned(), Json::count(coverage.unchecked())),
        (
            "not_fully_checked".to_owned(),
            Json::Array(not_fully_checked),
        ),
    ])
}

/// The published name of a rule status. Part of the contract, like a rule code.
fn status_name(status: RuleStatus) -> &'static str {
    match status {
        RuleStatus::Checked => "checked",
        RuleStatus::PartlyChecked => "partly-checked",
        RuleStatus::NotImplemented => "not-implemented",
        RuleStatus::AwaitingDecision => "awaiting-decision",
        RuleStatus::RequiresPublishedVersion => "requires-published-version",
        RuleStatus::PreemptedByEarlierRule => "preempted-by-earlier-rule",
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;
    use nkb_core::lint::{RuleCode, SkipReason};

    fn judged(problems: Vec<LintProblem>) -> LintOutcome {
        let mut report = LintReport::default();
        report.extend(problems);
        LintOutcome::Judged(report)
    }

    fn rendered(files: &[(String, LintOutcome)]) -> String {
        document(files, "0.1.0").render()
    }

    fn one(outcome: LintOutcome) -> String {
        rendered(&[("packs/a.toml".to_owned(), outcome)])
    }

    #[test]
    fn a_clean_run_says_so_at_the_top_and_per_file() {
        let text = one(judged(Vec::new()));
        assert!(text.contains("\"schema\": 1"), "{text}");
        assert!(text.contains("\"accepted\": true"), "{text}");
        assert!(text.contains("\"status\": \"judged\""), "{text}");
        assert!(text.contains("\"problems\": []"), "{text}");
    }

    #[test]
    fn one_file_still_arrives_inside_a_list() {
        // The decision that keeps a batch mode from being a breaking change. If
        // this ever renders flat, every consumer written against the list breaks.
        let text = one(judged(Vec::new()));
        assert!(text.contains("\"files\": ["), "{text}");
    }

    #[test]
    fn a_problem_carries_its_parts_beside_the_sentence_not_only_inside_it() {
        // A check must be able to match on a field. The wording may be improved
        // at any time - the fields beside it may not.
        let text = one(judged(vec![
            LintProblem::new(RuleCode::MissingRequiredField)
                .at(12)
                .about("breaks")
                .owned_by("bool-no"),
        ]));
        assert!(text.contains("\"rule\": \"E003\""), "{text}");
        assert!(text.contains("\"severity\": \"error\""), "{text}");
        assert!(text.contains("\"line\": 12"), "{text}");
        assert!(text.contains("\"subject\": \"breaks\""), "{text}");
        assert!(text.contains("\"owner\": \"bool-no\""), "{text}");
        assert!(text.contains("\"message\": \"value `bool-no`"), "{text}");
    }

    #[test]
    fn a_problem_about_the_whole_file_reports_a_null_line_not_a_zero() {
        // Line zero exists in no file, and an editor sent there lands nowhere.
        let text = one(judged(vec![LintProblem::new(
            RuleCode::MissingOrUnsupportedFormat,
        )]));
        assert!(text.contains("\"line\": null"), "{text}");
    }

    #[test]
    fn a_warning_alone_leaves_the_run_accepted() {
        let text = one(judged(vec![LintProblem::new(RuleCode::PackWithoutTags)]));
        assert!(text.contains("\"accepted\": true"), "{text}");
        assert!(text.contains("\"warnings\": 1"), "{text}");
        assert!(text.contains("\"severity\": \"warning\""), "{text}");
    }

    #[test]
    fn a_file_that_was_never_judged_is_neither_accepted_nor_rejected() {
        // Third answer beside pass and fail. Rendering it as either would have the
        // tool state something it does not know.
        let text = one(LintOutcome::NotFound);
        assert!(text.contains("\"status\": \"not-found\""), "{text}");
        assert!(text.contains("\"accepted\": null"), "{text}");
        // And the run as a whole must not report success because nothing failed.
        assert!(text.contains("\"accepted\": false"), "{text}");
    }

    #[test]
    fn an_unreadable_file_is_told_apart_from_a_missing_one() {
        let text = one(LintOutcome::Unreadable);
        assert!(text.contains("\"status\": \"unreadable\""), "{text}");
    }

    #[test]
    fn counts_are_summed_across_files_and_kept_per_file() {
        let text = rendered(&[
            (
                "a.toml".to_owned(),
                judged(vec![LintProblem::new(RuleCode::PackWithoutValues)]),
            ),
            (
                "b.toml".to_owned(),
                judged(vec![LintProblem::new(RuleCode::UnknownKey)]),
            ),
        ]);
        assert!(text.contains("\"errors\": 2"), "top level sum: {text}");
        assert_eq!(text.matches("\"errors\": 1").count(), 2, "{text}");
        assert!(text.contains("\"accepted\": false"), "{text}");
    }

    #[test]
    fn one_bad_file_among_good_ones_fails_the_whole_run() {
        // The one bit continuous integration reads. Computing it from the list is
        // the step a caller forgets, which is why it is stated.
        let text = rendered(&[
            ("good.toml".to_owned(), judged(Vec::new())),
            (
                "bad.toml".to_owned(),
                judged(vec![LintProblem::new(RuleCode::PackWithoutValues)]),
            ),
        ]);
        let top = text.split("\"files\"").next().expect("header exists");
        assert!(top.contains("\"accepted\": false"), "{top}");
    }

    #[test]
    fn the_document_says_what_was_not_checked_and_why() {
        // Without this an empty `problems` reads as a clean pack, when the truth
        // may be that two dozen rules never ran.
        let text = one(judged(Vec::new()));
        assert!(text.contains("\"not_fully_checked\""), "{text}");
        assert!(text.contains("\"status\": \"partly-checked\""), "{text}");
        assert!(text.contains("\"status\": \"awaiting-decision\""), "{text}");
        assert!(
            text.contains("\"status\": \"requires-published-version\""),
            "{text}"
        );
        assert!(text.contains("\"status\": \"not-implemented\""), "{text}");
    }

    #[test]
    fn the_rule_counts_agree_with_the_register() {
        let coverage = RuleCoverage::measure();
        let text = one(judged(Vec::new()));
        assert!(
            text.contains(&format!("\"total\": {}", coverage.total)),
            "{text}"
        );
        assert!(
            text.contains(&format!("\"checked\": {}", coverage.checked)),
            "{text}"
        );
        assert!(
            text.contains(&format!("\"partly_checked\": {}", coverage.partly)),
            "{text}"
        );
    }

    #[test]
    fn the_listed_rules_are_exactly_the_ones_not_fully_checked() {
        // A count that disagrees with its own list is worse than either alone.
        let coverage = RuleCoverage::measure();
        let text = one(judged(Vec::new()));
        let listed = text.matches("\"rule\": \"").count();
        assert_eq!(listed, coverage.partly + coverage.unchecked(), "{text}");
    }

    #[test]
    fn two_runs_over_the_same_input_produce_the_same_bytes() {
        // What the missing timestamp buys: a check can store this output and
        // compare it later without every run looking like a change.
        let first = one(judged(vec![LintProblem::new(RuleCode::PackWithoutValues)]));
        let second = one(judged(vec![LintProblem::new(RuleCode::PackWithoutValues)]));
        assert_eq!(first, second);
    }

    #[test]
    fn a_file_says_which_rules_could_not_be_checked_over_it() {
        // The half `rules` at the top cannot carry. That block is identical in
        // every document this build writes, so a rule that gave up on this one
        // file would be invisible there - and an empty `problems` beside a
        // "checked 32 of 42" would read as a clean pack.
        let mut report = LintReport::default();
        report.skip(
            SkippedRule::new(
                RuleCode::TranslationRefersToMissingId,
                SkipReason::TranslatedPackNotFound,
            )
            .about("unicode-text"),
        );
        let text = one(LintOutcome::Judged(report));

        assert!(text.contains("\"rules_not_checked_here\""), "{text}");
        assert!(text.contains("\"rule\": \"W052\""), "{text}");
        assert!(
            text.contains("\"reason\": \"translated-pack-not-found\""),
            "{text}"
        );
        assert!(text.contains("\"subject\": \"unicode-text\""), "{text}");
        // Acceptance is untouched: a rule that could not run has found nothing to
        // block on. That is exactly why the record has to be visible beside it.
        assert!(text.contains("\"accepted\": true"), "{text}");
    }

    #[test]
    fn the_per_file_list_and_the_build_wide_list_cannot_be_read_as_each_other() {
        // Two lists of rules in one document, and a consumer reaching for the
        // wrong one would conclude this build checks all but one rule. They are
        // told apart by shape as well as by position: `reason` here, `status`
        // there. Reading the wrong member fails where it can be seen.
        let mut report = LintReport::default();
        report.skip(SkippedRule::new(
            RuleCode::TranslationRefersToMissingId,
            SkipReason::TranslatedPackNotNamed,
        ));
        let text = one(LintOutcome::Judged(report));

        let (per_file, build_wide) = text
            .split_once("\"rules\":")
            .expect("the document carries both lists");
        assert!(per_file.contains("\"reason\":"), "{per_file}");
        assert!(!per_file.contains("\"status\": \"not-implemented\""));
        assert!(build_wide.contains("\"status\":"), "{build_wide}");
        assert!(!build_wide.contains("\"reason\":"), "{build_wide}");
    }

    #[test]
    fn a_file_with_nothing_skipped_still_carries_the_member_as_an_empty_list() {
        // A member that appears only sometimes makes a consumer write a presence
        // check before it can write anything else, and the one that forgets reads
        // an absent member as a value it never saw.
        let text = one(judged(Vec::new()));
        assert!(text.contains("\"rules_not_checked_here\": []"), "{text}");
    }

    #[test]
    fn an_empty_run_still_produces_the_same_shape() {
        // `--explain --json` asks about the tool rather than about a file, and a
        // second document shape for that case would double every consumer's work.
        let text = rendered(&[]);
        assert!(text.contains("\"files\": []"), "{text}");
        assert!(text.contains("\"rules\""), "{text}");
    }
}
