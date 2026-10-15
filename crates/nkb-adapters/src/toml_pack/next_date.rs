//! The validator's side of `type = "next-date"` (`pack-format.md` 6, `D133`).
//!
//! Its own file because it is a recipe of its own: the four fields, what makes
//! them name a day, and how two of them are compared. The reader in
//! `read_pack` takes the rule from [`rule_of`] here as well, so that the day the
//! program types is the day the validator judged.

use nkb_core::calendar::Weekday;
use nkb_core::lint::{LintProblem, RuleCode};
use nkb_core::next_date::{NextDate, TimeOfDay, Week};
use nkb_core::source_text::SourceText;
use nkb_core::text::LiteralText;
use toml_edit::{Item, Table};

use super::span_line;

/// A value whose date is worked out on the day it is inserted: E018 for a rule
/// that names no day, E025 for text written beside it.
///
/// A part of the wrong TOML kind - `month = "3"` - is E009's finding, the way a
/// count written as text is for a repeat, and is not reported here a second
/// time. A part that is absent, or present and outside its vocabulary, is this
/// rule's: the subject is the field's name when it is missing and the field as
/// written when it is wrong, so the sentence can say which.
pub(super) fn check(text: &SourceText<'_>, table: &Table) -> Vec<LintProblem> {
    let mut problems = Vec::new();
    if table.contains_key("value") {
        problems.push(LintProblem::new(RuleCode::ValuePresentForNonLiteralType).about("value"));
    }

    for field in ["month", "week", "weekday"] {
        if !table.contains_key(field) {
            problems.push(LintProblem::new(RuleCode::NextDateRuleIncomplete).about(field));
        }
    }

    let wrong = |field: &str, shown: String, item: &Item| {
        LintProblem::new(RuleCode::NextDateRuleIncomplete)
            .at(span_line(text, item))
            .about(format!("{field} = {shown}"))
    };
    if let Some(item) = table.get("month")
        && let Some(month) = item.as_integer()
        && !(1..=12).contains(&month)
    {
        problems.push(wrong("month", month.to_string(), item));
    }
    for field in ["week", "weekday", "time"] {
        if let Some(item) = table.get(field)
            && let Some(word) = item.as_str()
            && !in_vocabulary(field, word)
        {
            // Escaped, so that a word carrying an invisible character shows
            // the reader why `sunday` was not read as Sunday.
            let shown = format!("\"{}\"", LiteralText::new(word).escape().as_str());
            problems.push(wrong(field, shown, item));
        }
    }
    problems
}

/// Whether `word` is one the format allows for `field`: the week and weekday
/// vocabularies, and a time written `HH:MM`.
fn in_vocabulary(field: &str, word: &str) -> bool {
    match field {
        "week" => Week::from_name(word).is_some(),
        "weekday" => Weekday::from_name(word).is_some(),
        _ => TimeOfDay::parse(word).is_some(),
    }
}

/// The rule a `next-date` value names, or nothing when it names no day.
///
/// The one reading of the four fields, shared by E016 here and by the reader in
/// `read_pack`, so that the validator and the program cannot disagree about
/// which day a file means.
pub(crate) fn rule_of(table: &Table) -> Option<NextDate> {
    let month = table.get("month").and_then(Item::as_integer)?;
    let week = Week::from_name(table.get("week").and_then(Item::as_str)?)?;
    let weekday = Weekday::from_name(table.get("weekday").and_then(Item::as_str)?)?;
    let time = match table.get("time") {
        None => None,
        Some(item) => Some(TimeOfDay::parse(item.as_str()?)?),
    };
    NextDate::new(u32::try_from(month).ok()?, week, weekday, time)
}

/// What E016 compares for a `next-date` value: the words as written, and the
/// month as the number. Two rules naming the same day are one value, and a rule
/// that names no day is E018's, so it has no key.
pub(super) fn key(table: &Table) -> Option<(String, i64)> {
    let rule = rule_of(table)?;
    let time = rule.time().map(TimeOfDay::written).unwrap_or_default();
    let words = format!("{} {} {time}", rule.week().name(), rule.weekday().name());
    Some((words, i64::from(rule.month())))
}
