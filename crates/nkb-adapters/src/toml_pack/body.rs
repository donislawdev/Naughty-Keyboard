//! What one value inserts, read from the file: the kind, the rules each kind
//! answers to, and the key two values are compared by (E016).
//!
//! Out of `toml_pack.rs` so that a new kind of value grows this file and its
//! own, not the file every rule of the format lives in (`D113`). The rules
//! themselves are unchanged by the move.

use nkb_core::lint::{LintProblem, RuleCode};
use nkb_core::source_text::SourceText;
use nkb_core::text::LiteralText;
use nkb_core::value::ValueBody;
use nkb_core::written_form;
use toml_edit::{Item, Table};

use super::raw_slice;

/// The kinds of value this build understands.
///
/// Six more names are reserved by the format and deliberately unsupported, so
/// that nobody claims them for something else before they are designed. A file
/// using one is refused with the same code as a typo, because to this build the
/// two are the same thing: a kind it cannot produce.
const SUPPORTED_TYPES: [&str; 3] = ["literal", "repeat", "next-date"];

/// The rules about what one value is and how it is written.
///
/// E023 comes first and stops the rest: for a kind this build does not know,
/// there is no telling which fields belong to it, so every further rule would be
/// guessing about a shape nobody has defined.
pub(super) fn check_body(text: &SourceText<'_>, table: &Table) -> Vec<LintProblem> {
    let declared = table.get("type").and_then(Item::as_str);

    if let Some(kind) = declared
        && !SUPPORTED_TYPES.contains(&kind)
    {
        return vec![LintProblem::new(RuleCode::UnknownValueType).about(kind)];
    }

    if declared == Some("repeat") {
        return check_repeat(text, table);
    }
    if declared == Some("next-date") {
        return super::next_date::check(text, table);
    }
    check_literal(text, table)
}

/// What a value inserts, in a form two values can be compared by.
///
/// Three parts rather than one string, so that no separator has to be invented
/// and no value can be built that collides with another by containing it: the
/// kind, the text, and the repeat count (zero for a literal, which is not a
/// legal count and so cannot be confused with one).
pub(super) type BodyKey = (&'static str, String, i64);

/// The key for one value, or nothing when the body is not usable.
///
/// # Why this compares the recipe and not the text it produces
///
/// A value is a recipe, not a piece of text - a repeat can describe a million
/// characters from one line, and building it in order to compare it would defeat
/// the promise that the tool measures a length bomb without setting it off
/// (`architektura.md` 6.1). So two values are the same when they are *written*
/// the same, after parsing. A newline written as a four digit escape and the
/// same newline written as the one letter escape are one value spelled two
/// ways, and do compare equal. A literal `aaa` and a repeat of `a` three times
/// produce the same text and do not: comparing those would mean building them.
/// The rule says what it checks rather than claiming the wider thing and doing
/// the narrower one.
///
/// Nothing is returned for a body some other rule already refuses. A value with
/// no `value` is E003's, a bad `count` is E024's, an unknown `type` is E023's -
/// and a rule that also spoke there would give one mistake two codes.
pub(super) fn body_key(table: &Table) -> Option<BodyKey> {
    let declared = table.get("type").and_then(Item::as_str);
    if let Some(kind) = declared
        && !SUPPORTED_TYPES.contains(&kind)
    {
        return None;
    }

    if declared == Some("repeat") {
        let unit = table.get("unit").and_then(Item::as_str)?;
        let count = table.get("count").and_then(Item::as_integer)?;
        return Some(("repeat", unit.to_owned(), count));
    }

    if declared == Some("next-date") {
        let (words, month) = super::next_date::key(table)?;
        return Some(("next-date", words, month));
    }

    let value = table.get("value").and_then(Item::as_str)?;
    Some(("literal", value.to_owned(), 0))
}

/// A value written out in the file: how it is spelled, and how long it is.
fn check_literal(text: &SourceText<'_>, table: &Table) -> Vec<LintProblem> {
    let Some(item) = table.get("value") else {
        // Its absence is already reported as a missing required field. Saying so
        // twice would give one mistake two codes.
        return Vec::new();
    };
    let Some(expanded) = item.as_str() else {
        return Vec::new();
    };

    let mut problems = written_form::check(raw_slice(text, item), expanded);
    problems.extend(written_form::check_length(expanded));
    problems
}

/// A value described by a recipe: the recipe's parts, its size, and the spelling
/// of the unit it repeats.
fn check_repeat(text: &SourceText<'_>, table: &Table) -> Vec<LintProblem> {
    let mut problems = Vec::new();

    if table.contains_key("value") {
        // A recipe that also carries text has two answers to the same question,
        // and nothing says which one the tool would send.
        problems.push(LintProblem::new(RuleCode::ValuePresentForNonLiteralType).about("value"));
    }

    let unit = table.get("unit").and_then(Item::as_str);
    let count = table
        .get("count")
        .and_then(|item| item.as_value()?.as_integer());

    let (Some(unit), Some(count)) = (unit, count) else {
        // Either part missing leaves a recipe that describes nothing. A part that
        // is present but of the wrong kind is E009's finding, and reporting it
        // here as well would tell the reader to add a field they can see.
        let absent = if unit.is_none() { "unit" } else { "count" };
        if !table.contains_key(absent) {
            problems.push(LintProblem::new(RuleCode::RepeatCountOutOfRange).about(absent));
        }
        return problems;
    };

    // A count outside the machine word is out of range like any other, and saying
    // so beats a silent conversion that turns a negative number into a large
    // positive one - the kind of quiet wrap this catalogue exists to find.
    let Ok(count) = u32::try_from(count) else {
        problems.push(LintProblem::new(RuleCode::RepeatCountOutOfRange).about(count.to_string()));
        return problems;
    };

    let body = ValueBody::Repeat {
        unit: LiteralText::new(unit),
        count,
    };
    if let Err(problem) = body.check_size() {
        // The code travels from where the fault was found rather than being
        // guessed back here, so E024 and E026 keep meaning what they mean.
        problems.push(LintProblem::from(problem).about(count.to_string()));
    }

    if let Some(item) = table.get("unit") {
        problems.extend(written_form::check(raw_slice(text, item), unit));
    }
    problems
}
