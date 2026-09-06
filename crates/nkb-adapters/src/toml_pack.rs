//! Reading a pack file, and the rules that only a parsed file can answer.
//!
//! # Why this parses into the immutable document and not the editable one
//!
//! Measured against toml_edit 0.25.13, not recalled: the editable document
//! reports no source position at all, while the immutable one reports a byte
//! range for every item. A validator whose output has no line numbers sends a
//! contributor to find the problem by eye, so the choice between the two types
//! is the difference between a usable report and a shrug.
//!
//! The editable document is the right one for writing a pack back out. It is the
//! wrong one for reading a pack in, and the two jobs live in different places.
//!
//! # Why some rules are checked against the raw file rather than the parsed value
//!
//! Also measured: rendering a parsed value back out rewrites it. A value written
//! with single quotes comes back with double ones, and an escape sequence comes
//! back as the character it stood for. Both are correct behaviour for an editor
//! and useless for a validator, because the escaping rules of this format are
//! rules about how a value is *written*. Those rules therefore read the raw slice
//! of the file that a span points at - which was measured to be faithful.

use nkb_app::PackFormat;
use nkb_core::description::{self, BreaksFault};
use nkb_core::identity::{is_pack_id, is_value_id};
use nkb_core::lint::{LintProblem, RuleCode};
use nkb_core::schema::{FieldKind, kind_of};
use nkb_core::source_text::line_of;
use nkb_core::text::LiteralText;
use nkb_core::value::ValueBody;
use nkb_core::written_form;
use std::collections::HashSet;
use toml_edit::{Document, Item, Table};

/// The file format version this build understands.
///
/// A file declaring a newer one is refused rather than read hopefully. That is
/// the entire purpose of the field, and it costs one line to honour.
pub const SUPPORTED_FORMAT: i64 = 1;

/// Keys allowed at the top level of any pack file.
///
/// A source pack uses the first three. A translation file uses `translates` and
/// `language` instead of `pack` and `values`, so both shapes appear here - the
/// rules that tell the two apart are the translation rules, which are registered
/// and not yet written.
const TOP_LEVEL_KEYS: [&str; 6] = [
    "format",
    "pack",
    "values",
    "pairs",
    "translates",
    "language",
];

/// Fields a source pack must declare. Absent means the pack cannot be cited,
/// versioned, or redistributed by whoever receives it.
const REQUIRED_PACK_FIELDS: [&str; 8] = [
    "id",
    "name",
    "description",
    "version",
    "updated",
    "license",
    "authors",
    "language",
];

/// Fields every value must declare, whatever its type.
///
/// `breaks` is deliberately absent from this list: the format gives it a rule of
/// its own, E030, and reporting it here as well would give one mistake two codes.
const REQUIRED_VALUE_FIELDS: [&str; 3] = ["id", "name", "since"];

/// The kinds of value this build understands.
///
/// Six more names are reserved by the format and deliberately unsupported, so
/// that nobody claims them for something else before they are designed. A file
/// using one is refused with the same code as a typo, because to this build the
/// two are the same thing: a kind it cannot produce.
const SUPPORTED_TYPES: [&str; 2] = ["literal", "repeat"];

/// The closed vocabulary of field kinds.
///
/// Closed on purpose. Left open, fifty contributions would bring `email`,
/// `e-mail`, `mail` and `email-address` for one thing, matching packs to a field
/// would stop working, and the repair would mean editing every pack at once.
const FIELD_VOCABULARY: [&str; 22] = [
    "any",
    "text",
    "multiline",
    "name",
    "username",
    "password",
    "email",
    "url",
    "search",
    "number",
    "integer",
    "money",
    "date",
    "time",
    "datetime",
    "phone",
    "postal-code",
    "country-code",
    "address",
    "filename",
    "path",
    "id-number",
];

/// Checks everything that needs the file parsed.
///
/// Returns every problem it finds rather than the first, because a contributor
/// who gets one problem per run turns five minutes of fixing into five rounds of
/// it. The single exception is a file that does not parse: there is nothing to
/// look inside, so that one problem is returned alone.
#[must_use]
pub fn check(text: &str, expected_id: &str) -> Vec<LintProblem> {
    let document: Document<String> = match text.parse() {
        Ok(document) => document,
        Err(error) => {
            let line = error.span().map_or(1, |span| line_of(text, span.start));
            return vec![
                LintProblem::new(RuleCode::NotValidToml)
                    .at(line)
                    .about(error.message().replace('\n', " ")),
            ];
        }
    };

    let root = document.as_table();
    let mut problems = Vec::new();

    problems.extend(check_format(text, root));
    problems.extend(check_top_level_keys(text, root));

    // A translation file has a different shape on purpose: it carries no values
    // and only the prose half of the pack table. Applying the source pack rules
    // to it would report a correct file as broken.
    if root.contains_key("translates") {
        return problems;
    }

    // The kinds first, and the reason is the ordering rather than the rule. A
    // field of a kind the format cannot read is invisible to every check after
    // this one, so without E009 those checks would either stay silent or report
    // the field as missing - and a field written down is not a field forgotten.
    problems.extend(check_field_kinds(
        text,
        root.iter().filter(|(key, _)| TOP_LEVEL_KEYS.contains(key)),
    ));
    problems.extend(check_pack_table(text, root));
    problems.extend(check_pack_kinds(text, root));
    problems.extend(check_pack_id(text, root, expected_id));
    problems.extend(check_values(text, root, &pack_context(root)));
    problems
}

/// E001: the file format version, missing or newer than this build.
fn check_format(text: &str, root: &Table) -> Vec<LintProblem> {
    let Some(item) = root.get("format") else {
        // No position to point at - the key is missing from the whole file - so
        // this problem names the file rather than a line inside it.
        return vec![LintProblem::new(RuleCode::MissingOrUnsupportedFormat)];
    };

    let line = span_line(text, item);
    let declared = item.as_value().and_then(toml_edit::Value::as_integer);

    match declared {
        Some(SUPPORTED_FORMAT) => Vec::new(),
        Some(other) => vec![
            LintProblem::new(RuleCode::MissingOrUnsupportedFormat)
                .at(line)
                .about(other.to_string()),
        ],
        None => vec![LintProblem::new(RuleCode::MissingOrUnsupportedFormat).at(line)],
    }
}

/// E002: a top level key that is not part of the format.
///
/// Almost always a typo in a key that is part of it, which is why silence here is
/// worse than noise: the field the author meant to set was never set, and the
/// pack ships with a default nobody chose.
fn check_top_level_keys(text: &str, root: &Table) -> Vec<LintProblem> {
    root.iter()
        .filter(|(key, _)| !TOP_LEVEL_KEYS.contains(key))
        .map(|(key, item)| {
            LintProblem::new(RuleCode::UnknownTopLevelKey)
                .at(span_line(text, item))
                .about(key)
        })
        .collect()
}

/// E003, pack half: the table itself, and the fields it must carry.
fn check_pack_table(text: &str, root: &Table) -> Vec<LintProblem> {
    let Some(pack) = root.get("pack").and_then(Item::as_table_like) else {
        return vec![LintProblem::new(RuleCode::MissingRequiredField).about("pack")];
    };

    let line = root.get("pack").map_or(1, |item| span_line(text, item));

    REQUIRED_PACK_FIELDS
        .iter()
        .filter(|field| !pack.contains_key(field))
        .map(|field| {
            LintProblem::new(RuleCode::MissingRequiredField)
                .at(line)
                .about(*field)
                .owned_by("pack")
        })
        .collect()
}

/// What the pack table says that its values are judged against.
///
/// Gathered once rather than per value: these are properties of the file, and
/// reading them thirty four times invites two readings of one file to disagree.
struct PackContext<'a> {
    /// Whether `[pack]` names where its values came from. Only a pack that does
    /// can raise W033 at all.
    declares_source: bool,
    /// The language the descriptions in this file claim to be written in.
    language: Option<&'a str>,
}

fn pack_context(root: &Table) -> PackContext<'_> {
    let pack = root.get("pack").and_then(Item::as_table_like);
    PackContext {
        declares_source: pack.is_some_and(|pack| pack.contains_key("source")),
        language: pack
            .and_then(|pack| pack.get("language"))
            .and_then(Item::as_str),
    }
}

/// E009: fields whose value is of a kind the format cannot use.
///
/// Takes an iterator rather than a table so that the top level can be filtered
/// down to the keys that belong there. A stray top level key is E002's finding,
/// and judging its kind as well would put two codes on one typo.
fn check_field_kinds<'a>(
    text: &str,
    fields: impl Iterator<Item = (&'a str, &'a Item)>,
) -> Vec<LintProblem> {
    fields
        .filter(|(key, item)| kind_of(key).is_some_and(|kind| !holds(item, kind)))
        .map(|(key, item)| {
            LintProblem::new(RuleCode::FieldOfUnusableType)
                .at(span_line(text, item))
                .about(key)
        })
        .collect()
}

/// The same rule over the pack table, which needs an owner the value tables get
/// from the walk that visits them.
fn check_pack_kinds(text: &str, root: &Table) -> Vec<LintProblem> {
    let Some(pack) = root.get("pack").and_then(Item::as_table_like) else {
        return Vec::new();
    };
    check_field_kinds(text, pack.iter())
        .into_iter()
        .map(|problem| problem.owned_by("pack"))
        .collect()
}

/// Whether an item is of the kind the format gives that field.
///
/// A field holding a table rather than a value fails every kind, which is the
/// answer wanted: `id = { }` is not an identifier by any reading.
fn holds(item: &Item, kind: FieldKind) -> bool {
    let Some(value) = item.as_value() else {
        return false;
    };
    match kind {
        FieldKind::Text => value.as_str().is_some(),
        FieldKind::WholeNumber => value.as_integer().is_some(),
        FieldKind::Date => value.as_datetime().is_some(),
        FieldKind::List => value.as_array().is_some(),
        FieldKind::TrueOrFalse => value.as_bool().is_some(),
    }
}

/// E010: the pack identifier, against its own pattern and against the file name.
///
/// Silent when there is no `[pack]` table and when it declares no `id`. Both are
/// already E003's finding, and one mistake wearing two codes sends a contributor
/// hunting for a second problem that is not there.
fn check_pack_id(text: &str, root: &Table, expected_id: &str) -> Vec<LintProblem> {
    let Some(item) = root
        .get("pack")
        .and_then(Item::as_table_like)
        .and_then(|pack| pack.get("id"))
    else {
        return Vec::new();
    };
    let Some(declared) = item.as_str() else {
        return Vec::new();
    };

    // One problem for both halves of the rule. An identifier outside the alphabet
    // cannot match a file name either, so reporting them separately would put the
    // same repair on the screen twice. Which half failed is decided again where
    // the sentence is written, by the same function that decided it here.
    if is_pack_id(declared) && declared == expected_id {
        return Vec::new();
    }
    vec![
        LintProblem::new(RuleCode::PackIdMismatch)
            .at(span_line(text, item))
            .about(declared),
    ]
}

/// E030, E031, W032, W033 and W034: the half of a value that explains it.
///
/// 🔴 E031 is the sentence rule from `product-spec.md`, and it is the reason this
/// module exists at all. Everywhere else that rule is editorial policy, which is
/// another way of saying it holds while somebody remembers it.
fn check_description(
    table: &Table,
    pack: &PackContext,
    values_are_attributed_one_by_one: bool,
) -> Vec<LintProblem> {
    let mut problems = Vec::new();

    if table.contains_key("breaks") {
        // A `breaks` that is present but is not text goes unjudged here. No rule
        // in the set covers a field of the wrong type, for any field, and
        // inventing a code for this one would be inventing a rule.
        if let Some(breaks) = table.get("breaks").and_then(Item::as_str) {
            let name = table.get("name").and_then(Item::as_str);
            match description::check_breaks(breaks, name) {
                Some(BreaksFault::TooShort { code_points }) => problems.push(
                    LintProblem::new(RuleCode::BreaksTooShortOrEchoesName)
                        .about(code_points.to_string()),
                ),
                Some(BreaksFault::EchoesName) => problems
                    .push(LintProblem::new(RuleCode::BreaksTooShortOrEchoesName).about("name")),
                None => {}
            }

            // A file that declares another language has already said its
            // descriptions are not English. Warning there would be arguing with
            // the author about something the author already told us.
            if pack.language.is_none_or(|language| language == "en")
                && !description::looks_english(breaks)
            {
                problems.push(LintProblem::new(RuleCode::BreaksProbablyNotEnglish).about("breaks"));
            }
        }
    } else {
        problems.push(LintProblem::new(RuleCode::MissingBreaks).about("breaks"));
    }

    if !table.contains_key("expect") {
        problems.push(LintProblem::new(RuleCode::MissingExpect).about("expect"));
    }

    // W033 asks about attribution that has gone patchy. A pack naming one source
    // for everything is already attributed, and the format says so: an absent
    // `source` on a value inherits the pack's. Warning on every value of such a
    // pack would argue with that default. Warning is worth it once the pack has
    // started attributing values one by one, because from that point inheritance
    // credits somebody's work for values that may not have come from them.
    if pack.declares_source && values_are_attributed_one_by_one && !table.contains_key("source") {
        problems.push(LintProblem::new(RuleCode::ValueWithoutSourceInSourcedPack).about("source"));
    }

    problems
}

/// E008 and E003, value half.
///
/// The format lists five required fields for a value and the rule set names only
/// one of them. Until that gap is settled the other four are reported under the
/// rule that already means "a required field is missing", which is recorded as an
/// observation rather than decided here.
fn check_values(text: &str, root: &Table, pack: &PackContext) -> Vec<LintProblem> {
    let values = root.get("values").and_then(Item::as_array_of_tables);

    let Some(values) = values.filter(|values| !values.is_empty()) else {
        // A pack with no values loads, appears in the palette and inserts
        // nothing. The counter would read 0/0 and say nothing about why.
        return vec![LintProblem::new(RuleCode::PackWithoutValues)];
    };

    // Every identifier in the file, gathered before the walk. A retired value may
    // point at a successor written below it, and a check that only looked
    // backwards would report a correct file as broken.
    let known: HashSet<&str> = values
        .iter()
        .filter_map(|table| table.get("id").and_then(Item::as_str))
        .collect();

    // Whether this pack has begun crediting its values one at a time - the
    // condition W033 turns on. See the reasoning where that rule is applied.
    let attributed_one_by_one = values.iter().any(|table| table.contains_key("source"));

    let mut seen: HashSet<&str> = HashSet::new();
    let mut problems = Vec::new();
    for (index, table) in values.iter().enumerate() {
        let line = table.span().map_or(1, |span| line_of(text, span.start));
        let declared_id = table.get("id").and_then(Item::as_str);
        // A value with no identifier is named by its place in the file. Counting
        // problems here instead of values, as this once did, produces a number
        // that looks like a position and is not one.
        let identity = declared_id.map_or_else(|| format!("values[{index}]"), ToOwned::to_owned);

        let mut identity_problems = Vec::new();
        if let Some(id) = declared_id {
            if !seen.insert(id) {
                identity_problems.push(LintProblem::new(RuleCode::DuplicateValueId).about(id));
            }
            if !is_value_id(id) {
                identity_problems.push(LintProblem::new(RuleCode::ValueIdMalformed).about(id));
            }
        }

        // The successor of a retired value is named by a bare identifier from this
        // same pack. The format defines succession nowhere else, so a reference
        // carrying a pack name is an unknown identifier here rather than a
        // reference this build silently declines to follow.
        if let Some(target) = table.get("replaced_by").and_then(Item::as_str)
            && !known.contains(target)
        {
            identity_problems.push(LintProblem::new(RuleCode::ReplacedByUnknownId).about(target));
        }

        for field in REQUIRED_VALUE_FIELDS {
            if !table.contains_key(field) {
                problems.push(
                    LintProblem::new(RuleCode::MissingRequiredField)
                        .at(line)
                        .about(field)
                        .owned_by(identity.clone()),
                );
            }
        }

        // A value written out in the file must carry the text. A generated value
        // carries a recipe instead, and the rules covering that live elsewhere in
        // the register - so this asks only about the default kind.
        let is_literal = table
            .get("type")
            .and_then(Item::as_str)
            .is_none_or(|declared| declared == "literal");
        if is_literal && !table.contains_key("value") {
            problems.push(
                LintProblem::new(RuleCode::MissingRequiredField)
                    .at(line)
                    .about("value")
                    .owned_by(identity.clone()),
            );
        }

        for mut problem in check_field_kinds(text, table.iter())
            .into_iter()
            .chain(identity_problems)
            .chain(check_body(text, table))
            .chain(check_fields(table))
            .chain(check_description(table, pack, attributed_one_by_one))
        {
            problem.line = problem.line.or(Some(line));
            problem.owner = Some(identity.clone());
            problems.push(problem);
        }
    }
    problems
}

/// The rules about what one value is and how it is written.
///
/// E023 comes first and stops the rest: for a kind this build does not know,
/// there is no telling which fields belong to it, so every further rule would be
/// guessing about a shape nobody has defined.
fn check_body(text: &str, table: &Table) -> Vec<LintProblem> {
    let declared = table.get("type").and_then(Item::as_str);

    if let Some(kind) = declared
        && !SUPPORTED_TYPES.contains(&kind)
    {
        return vec![LintProblem::new(RuleCode::UnknownValueType).about(kind)];
    }

    if declared == Some("repeat") {
        return check_repeat(text, table);
    }
    check_literal(text, table)
}

/// A value written out in the file: how it is spelled, and how long it is.
fn check_literal(text: &str, table: &Table) -> Vec<LintProblem> {
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
fn check_repeat(text: &str, table: &Table) -> Vec<LintProblem> {
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

/// E028: field kinds outside the closed vocabulary.
fn check_fields(table: &Table) -> Vec<LintProblem> {
    let Some(fields) = table.get("fields").and_then(Item::as_array) else {
        return Vec::new();
    };

    fields
        .iter()
        .filter_map(|entry| entry.as_str())
        .filter(|kind| !FIELD_VOCABULARY.contains(kind))
        .map(|kind| LintProblem::new(RuleCode::FieldOutsideVocabulary).about(kind))
        .collect()
}

/// The slice of the file an item occupies, quotes and all.
///
/// Empty when the parser kept no span, which loses the spelling rules for that
/// one value rather than reporting a fault that is not there.
fn raw_slice<'a>(text: &'a str, item: &Item) -> &'a str {
    item.span().and_then(|span| text.get(span)).unwrap_or("")
}

/// The pack format as this build reads it, satisfying the port the layer above
/// declares.
///
/// A unit struct because there is nothing to configure: which parser, which
/// version of the specification and which subset of it are properties of this
/// build rather than of a run.
#[derive(Debug, Clone, Copy, Default)]
pub struct TomlPackFormat;

impl PackFormat for TomlPackFormat {
    fn check(&self, text: &str, expected_id: &str) -> Vec<LintProblem> {
        check(text, expected_id)
    }
}

/// The line an item starts on, or the first line when the parser kept no span.
fn span_line(text: &str, item: &Item) -> u32 {
    item.span().map_or(1, |span| line_of(text, span.start))
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;

    /// The name the good file below is read under. Every test here reads a file
    /// whose pack identifier is this one, so the identity rules stay silent and
    /// each test breaks the single thing it is about.
    const PACK_ID: &str = "whitespace";

    /// A file that passes every rule this module checks, used as the base for
    /// tests that break exactly one thing.
    const GOOD: &str = concat!(
        "format = 1\n",
        "\n",
        "[pack]\n",
        "id = \"whitespace\"\n",
        "name = \"Whitespace\"\n",
        "description = \"Characters that occupy space, or claim to.\"\n",
        "version = \"1.0\"\n",
        "updated = 2026-09-06\n",
        "license = \"CC-BY-4.0\"\n",
        "authors = [\"Naughty Keyboard\"]\n",
        "language = \"en\"\n",
        "\n",
        "[[values]]\n",
        "id = \"trailing-space\"\n",
        "name = \"Trailing space\"\n",
        "value = \"Kowalski\\u0020\"\n",
        "breaks = \"Sorting puts the record at the top of every list, and an exact match against the trimmed name finds nothing.\"\n",
        "expect = \"Trimmed on save, or preserved and found by a search for the trimmed name.\"\n",
        "since = \"1.0\"\n",
    );

    fn codes(text: &str) -> Vec<&'static str> {
        check(text, PACK_ID)
            .iter()
            .map(|p| p.code.as_str())
            .collect()
    }

    #[test]
    fn a_complete_file_produces_nothing() {
        assert!(codes(GOOD).is_empty(), "{:?}", check(GOOD, PACK_ID));
    }

    #[test]
    fn a_file_that_does_not_parse_reports_that_alone_and_says_where() {
        // The one rule that stops the run: there is nothing to look inside.
        let text = "format = 1\n\n[[values]]\nvalue = no\n";
        let problems = check(text, PACK_ID);
        assert_eq!(problems.len(), 1);
        assert_eq!(problems[0].code.as_str(), "E004");
        assert_eq!(problems[0].line, Some(4));
        assert!(problems[0].subject.is_some(), "the parser message is kept");
    }

    #[test]
    fn a_missing_format_key_is_reported_against_the_file_not_a_line() {
        let text = GOOD.replace("format = 1\n", "");
        let problems = check(&text, PACK_ID);
        let format_problem = problems
            .iter()
            .find(|p| p.code == RuleCode::MissingOrUnsupportedFormat)
            .expect("E001 must be reported");
        assert_eq!(format_problem.line, None);
    }

    #[test]
    fn a_newer_format_is_refused_and_the_number_is_kept() {
        let text = GOOD.replace("format = 1", "format = 2");
        let problems = check(&text, PACK_ID);
        let found = problems
            .iter()
            .find(|p| p.code == RuleCode::MissingOrUnsupportedFormat)
            .expect("E001 must be reported");
        assert_eq!(found.subject.as_deref(), Some("2"));
        assert_eq!(found.line, Some(1));
    }

    #[test]
    fn a_format_that_is_not_a_number_is_refused_too() {
        // Quoting the version turns it into text, which no comparison against a
        // supported version can answer. Accepting it would mean reading a file of
        // unknown shape as if it were ours.
        let text = GOOD.replace("format = 1", "format = \"1\"");
        assert!(codes(&text).contains(&"E001"));
    }

    #[test]
    fn an_unknown_top_level_key_is_named() {
        let text = GOOD.replace("format = 1\n", "format = 1\npacks = \"typo\"\n");
        let problems = check(&text, PACK_ID);
        let found = problems
            .iter()
            .find(|p| p.code == RuleCode::UnknownTopLevelKey)
            .expect("E002 must be reported");
        assert_eq!(found.subject.as_deref(), Some("packs"));
        assert_eq!(found.line, Some(2));
    }

    #[test]
    fn a_missing_pack_field_names_the_field_and_its_owner() {
        let text = GOOD.replace("license = \"CC-BY-4.0\"\n", "");
        let problems = check(&text, PACK_ID);
        let found = problems
            .iter()
            .find(|p| p.subject.as_deref() == Some("license"))
            .expect("the missing licence must be reported");
        assert_eq!(found.code.as_str(), "E003");
        assert_eq!(found.owner.as_deref(), Some("pack"));
    }

    #[test]
    fn a_missing_pack_table_is_one_problem_not_eight() {
        // Reporting every field of a table that is not there would bury the one
        // fact that matters behind seven repetitions of it.
        let text = "format = 1\n";
        let missing: Vec<_> = check(text, PACK_ID)
            .into_iter()
            .filter(|p| p.code == RuleCode::MissingRequiredField)
            .collect();
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].subject.as_deref(), Some("pack"));
    }

    #[test]
    fn a_pack_with_no_values_is_reported() {
        let text = GOOD.replace("[[values]]", "[[unused]]");
        assert!(codes(&text).contains(&"E008"));
    }

    #[test]
    fn an_empty_values_array_counts_as_no_values() {
        // The degenerate but legal shape: the key exists and holds nothing. A
        // check written as "is the key present" would pass this and load a pack
        // that inserts nothing.
        //
        // Written out rather than patched from GOOD on purpose. Replacing the
        // table header there moves every following key into the pack table, which
        // produces a duplicate key and a parse error - so the test would have
        // passed or failed for a reason that has nothing to do with this rule.
        let text = concat!(
            "format = 1\n",
            "values = []\n",
            "\n",
            "[pack]\n",
            "id = \"whitespace\"\n",
            "name = \"Whitespace\"\n",
            "description = \"Characters that occupy space, or claim to.\"\n",
            "version = \"1.0\"\n",
            "updated = 2026-09-06\n",
            "license = \"CC-BY-4.0\"\n",
            "authors = [\"Naughty Keyboard\"]\n",
            "language = \"en\"\n",
        );
        assert_eq!(codes(text), vec!["E008"], "{:?}", check(text, PACK_ID));
    }

    #[test]
    fn a_value_missing_a_required_field_is_reported_against_its_id() {
        let text = GOOD.replace("name = \"Trailing space\"\n", "");
        let problems = check(&text, PACK_ID);
        let found = problems
            .iter()
            .find(|p| p.owner.as_deref() == Some("trailing-space"))
            .expect("the value's missing name must be reported");
        assert_eq!(found.code.as_str(), "E003");
        assert_eq!(found.subject.as_deref(), Some("name"));
    }

    #[test]
    fn a_literal_value_without_the_value_field_is_reported() {
        let text = GOOD.replace("value = \"Kowalski\\u0020\"\n", "");
        let problems = check(&text, PACK_ID);
        assert!(
            problems
                .iter()
                .any(|p| p.subject.as_deref() == Some("value")),
            "{problems:?}"
        );
    }

    #[test]
    fn an_empty_string_is_a_value_and_not_a_missing_field() {
        // The boundary between "left blank" and "filled in" is one of the most
        // confused things in validation, which makes the empty string a real test
        // case. A validator written by reflex rejects it.
        let text = GOOD.replace("value = \"Kowalski\\u0020\"", "value = \"\"");
        assert!(codes(&text).is_empty(), "{:?}", check(&text, PACK_ID));
    }

    #[test]
    fn a_generated_value_is_not_asked_for_a_value_field() {
        // A repeat value carries a recipe instead of text, and demanding the text
        // would make every length bomb in the catalogue unwritable.
        let text = GOOD.replace(
            "value = \"Kowalski\\u0020\"\n",
            "type = \"repeat\"\nunit = \"a\"\ncount = 100000\n",
        );
        assert!(codes(&text).is_empty(), "{:?}", check(&text, PACK_ID));
    }

    #[test]
    fn a_value_without_an_id_is_still_reported_rather_than_skipped() {
        // The identity used for reporting comes from the value itself, so a value
        // missing exactly that field is the case where a naive implementation
        // reports nothing at all.
        let text = GOOD.replace("id = \"trailing-space\"\n", "");
        let problems = check(&text, PACK_ID);
        assert!(
            problems.iter().any(|p| p.subject.as_deref() == Some("id")),
            "{problems:?}"
        );
    }

    #[test]
    fn a_translation_file_is_not_judged_by_the_rules_of_a_source_pack() {
        // A translation carries no values and only the prose half of the pack
        // table. Applying the source rules to it would report a correct file as
        // broken in five places.
        let text = concat!(
            "format = 1\n",
            "translates = \"unicode-text\"\n",
            "language = \"pl\"\n",
            "\n",
            "[pack]\n",
            "name = \"Unicode i tekst\"\n",
        );
        assert!(codes(text).is_empty(), "{:?}", check(text, PACK_ID));
    }

    #[test]
    fn every_problem_it_can_find_comes_out_of_one_run() {
        // Five minutes of fixing rather than five rounds of it.
        let text = concat!(
            "stray = 1\n",
            "[pack]\n",
            "id = \"a\"\n",
            "[[values]]\n",
            "id = \"b\"\n",
        );
        let mut found = codes(text);
        found.sort_unstable();
        found.dedup();
        // Six rules from four different families, out of one file, in one run.
        assert_eq!(found, vec!["E001", "E002", "E003", "E010", "E030", "W032"]);
    }
    /// Replaces the value line of GOOD with whatever a test needs.
    fn with_value(replacement: &str) -> String {
        GOOD.replace("value = \"Kowalski\\u0020\"\n", replacement)
    }

    #[test]
    fn a_reserved_type_is_refused_with_the_same_code_as_a_typo() {
        // To this build a reserved name and a typo are the same thing: a kind it
        // cannot produce.
        let text = with_value("type   = \"random\"\n");
        let problems = check(&text, PACK_ID);
        let found = problems
            .iter()
            .find(|p| p.code == RuleCode::UnknownValueType)
            .expect("E023 must be reported");
        assert_eq!(found.subject.as_deref(), Some("random"));
        assert_eq!(found.owner.as_deref(), Some("trailing-space"));
    }

    #[test]
    fn an_unknown_type_stops_the_other_body_rules() {
        // For a kind nobody has defined there is no telling which fields belong
        // to it, so every further rule would be guessing about a shape that does
        // not exist. One clear problem beats five speculative ones.
        let text = with_value("type   = \"random\"\nvalue  = \"a\"\ncount  = 0\n");
        assert_eq!(codes(&text), vec!["E023"], "{:?}", check(&text, PACK_ID));
    }

    #[test]
    fn a_recipe_missing_half_of_itself_is_out_of_range() {
        let text = with_value("type   = \"repeat\"\nunit   = \"a\"\n");
        let problems = check(&text, PACK_ID);
        let found = problems
            .iter()
            .find(|p| p.code == RuleCode::RepeatCountOutOfRange)
            .expect("E024 must be reported");
        assert_eq!(found.subject.as_deref(), Some("count"));
    }

    #[test]
    fn a_negative_count_is_out_of_range_rather_than_a_silent_wrap() {
        // A conversion turning this into a large positive number is the exact
        // class of quiet wrap this catalogue exists to find in other software.
        let text = with_value("type   = \"repeat\"\nunit   = \"a\"\ncount  = -5\n");
        assert!(
            codes(&text).contains(&"E024"),
            "{:?}",
            check(&text, PACK_ID)
        );

        let huge = with_value("type   = \"repeat\"\nunit   = \"a\"\ncount  = 5000000000\n");
        assert!(
            codes(&huge).contains(&"E024"),
            "{:?}",
            check(&huge, PACK_ID)
        );
    }

    #[test]
    fn a_zero_count_is_out_of_range() {
        let text = with_value("type   = \"repeat\"\nunit   = \"a\"\ncount  = 0\n");
        assert!(codes(&text).contains(&"E024"));
    }

    #[test]
    fn a_recipe_that_also_carries_text_has_two_answers_to_one_question() {
        let text = with_value("type   = \"repeat\"\nunit   = \"a\"\ncount  = 5\nvalue  = \"b\"\n");
        assert!(
            codes(&text).contains(&"E025"),
            "{:?}",
            check(&text, PACK_ID)
        );
    }

    #[test]
    fn the_two_billion_character_bomb_is_caught_by_the_product_rule() {
        // The count alone is inside the range E024 allows. Only the product rule
        // sees this, and the code travels from the core rather than being guessed
        // back here.
        let unit = "a".repeat(2000);
        let text = with_value(&format!(
            "type   = \"repeat\"\nunit   = \"{unit}\"\ncount  = 1000000\n"
        ));
        assert!(
            codes(&text).contains(&"E026"),
            "{:?}",
            check(&text, PACK_ID)
        );
    }

    #[test]
    fn the_largest_recipe_in_the_shipped_catalogue_still_passes() {
        // `len-100000`, ten times below the ceiling. If this ever fails, a size
        // rule broke the catalogue it was written to protect.
        let text = with_value("type   = \"repeat\"\nunit   = \"a\"\ncount  = 100000\n");
        assert!(codes(&text).is_empty(), "{:?}", check(&text, PACK_ID));
    }

    #[test]
    fn a_field_kind_outside_the_closed_list_is_refused() {
        let text = GOOD.replace(
            "since = \"1.0\"\n",
            "fields = [\"e-mail\"]\nsince = \"1.0\"\n",
        );
        let problems = check(&text, PACK_ID);
        let found = problems
            .iter()
            .find(|p| p.code == RuleCode::FieldOutsideVocabulary)
            .expect("E028 must be reported");
        assert_eq!(found.subject.as_deref(), Some("e-mail"));
    }

    #[test]
    fn a_field_kind_inside_the_list_passes() {
        let text = GOOD.replace(
            "since = \"1.0\"\n",
            "fields = [\"email\", \"any\"]\nsince = \"1.0\"\n",
        );
        assert!(codes(&text).is_empty(), "{:?}", check(&text, PACK_ID));
    }

    #[test]
    fn an_escape_outside_the_common_subset_is_caught_where_the_parser_accepts_it() {
        // Measured: this parser reads TOML 1.1.0 and takes this escape without a
        // word. Nothing but this rule stands between it and a pack that fails to
        // parse in somebody else's continuous integration.
        //
        // The doubled backslash below is deliberate. A single one would be an
        // escape of this source file, and the pack would then hold the letter A
        // rather than the sequence the rule is about - which is how this very
        // test passed for the wrong reason once already.
        let text = with_value("value  = \"A\\x41\"\n");
        let problems = check(&text, PACK_ID);
        let found = problems
            .iter()
            .find(|p| p.code == RuleCode::EscapeOutsideCommonSubset)
            .expect("E029 must be reported");
        assert_eq!(found.subject.as_deref(), Some("\\x"));
        assert_eq!(found.owner.as_deref(), Some("trailing-space"));
    }

    #[test]
    fn an_invisible_character_written_out_is_caught_and_attributed_to_its_value() {
        let text = with_value("value  = \"ab\u{200B}cd\"\n");
        let problems = check(&text, PACK_ID);
        let found = problems
            .iter()
            .find(|p| p.code == RuleCode::UnescapedCharacter)
            .expect("E020 must be reported");
        assert_eq!(found.subject.as_deref(), Some("\\u200B"));
        assert_eq!(found.owner.as_deref(), Some("trailing-space"));
        assert!(found.line.is_some(), "a problem must say where it is");
    }

    #[test]
    fn the_same_character_inside_single_quotes_is_e022_instead() {
        let text = with_value("value  = 'ab\u{200B}cd'\n");
        assert!(
            codes(&text).contains(&"E022"),
            "{:?}",
            check(&text, PACK_ID)
        );
        assert!(!codes(&text).contains(&"E020"), "one fault, one code");
    }

    #[test]
    fn a_long_written_out_value_is_a_warning_and_does_not_block_the_pack() {
        let long = "a".repeat(2001);
        let text = with_value(&format!("value  = \"{long}\"\n"));
        assert_eq!(codes(&text), vec!["W026"], "{:?}", check(&text, PACK_ID));
    }

    #[test]
    fn the_unit_of_a_recipe_is_spelled_by_the_same_rules_as_a_value() {
        // A recipe repeating an invisible character has to escape it too.
        // Checking only the `value` field would leave every generated value
        // unspelled.
        let text = with_value("type   = \"repeat\"\nunit   = \"\u{200B}\"\ncount  = 5\n");
        assert!(
            codes(&text).contains(&"E020"),
            "{:?}",
            check(&text, PACK_ID)
        );
    }

    #[test]
    fn the_escaped_unit_in_the_shipped_catalogue_passes() {
        // `len-100000-spaces` repeats an escaped plain space. Written this way it
        // is correct, and a rule that could not tell would break the catalogue.
        let text = with_value("type   = \"repeat\"\nunit   = \"\\u0020\"\ncount  = 100000\n");
        assert!(codes(&text).is_empty(), "{:?}", check(&text, PACK_ID));
    }

    /// Fragments of a pack file, kept here so that each test below reads as
    /// the one thing it changes rather than as a wall of quoting.
    const QID: &str = "id = \"";
    const QQ: &str = "\"";
    const QLANG: &str = "language = \"";
    const NL: &str = "\n";
    const SINCE: &str = "since = \"1.0\"\n";
    const LANG_LINE: &str = "language = \"en\"\n";
    const BREAKS_LINE: &str = "breaks = \"Sorting puts the record at the top of every list, and an exact match against the trimmed name finds nothing.\"\n";
    const SHORT_BREAKS: &str = "breaks = \"It breaks the export.\"\n";
    const POLISH_BREAKS: &str = "breaks = \"Parsery czytaja niecytowane no jako wartosc logiczna falsz, przez co lista krajow zamienia Norwegie.\"\n";
    const UPDATED_LINE: &str = "updated = 2026-09-06\n";
    const UPDATED_AS_TEXT: &str = "updated = \"2026-09-06\"\n";
    const TAGS_AS_TEXT: &str = "tags = \"yaml\"\n";
    const RECIPE_WITH_TEXT_COUNT: &str = "type   = \"repeat\"\nunit   = \"a\"\ncount  = \"5\"\n";
    const RECIPE_WITHOUT_COUNT: &str = "type   = \"repeat\"\nunit   = \"a\"\n";
    const NUMERIC_BREAKS: &str = "breaks = 42\n";
    const PACK_SOURCE: &str = "source = \"https://example.invalid/collection\"\n";
    const REPLACED_BY_LATER: &str = "replaced_by = \"successor\"\n";
    const REPLACED_BY_OTHER_PACK: &str = "replaced_by = \"other-pack/successor\"\n";
    const SECOND_VALUE: &str = "\n[[values]]\nid = \"trailing-space\"\nname = \"Again\"\nvalue = \"again\"\nbreaks = \"Nothing on its own - this value only exists so the file is otherwise complete.\"\nexpect = \"Nothing at all.\"\nsince = \"1.0\"\n";
    const SECOND_VALUE_WITH_SOURCE: &str = "\n[[values]]\nid = \"credited\"\nname = \"Credited\"\nvalue = \"credited\"\nbreaks = \"Nothing on its own - this value only exists so the file is otherwise complete.\"\nexpect = \"Nothing at all.\"\nsince = \"1.0\"\nsource = \"https://example.invalid/other\"\n";
    const LATER_VALUE: &str = "\n[[values]]\nid = \"successor\"\nname = \"Successor\"\nvalue = \"successor\"\nbreaks = \"Nothing on its own - this value only exists so the file is otherwise complete.\"\nexpect = \"Nothing at all.\"\nsince = \"1.0\"\n";

    /// The one problem of a given code, or a failure naming what was found.
    fn one_of(text: &str, code: RuleCode) -> LintProblem {
        let problems = check(text, PACK_ID);
        problems
            .into_iter()
            .find(|problem| problem.code == code)
            .unwrap_or_else(|| {
                panic!(
                    "{} must be reported, found {:?}",
                    code.as_str(),
                    check(text, PACK_ID)
                )
            })
    }

    #[test]
    fn a_pack_identifier_that_is_not_the_file_name_is_reported_where_it_stands() {
        let text = GOOD.replace(
            &format!("{QID}whitespace{QQ}"),
            &format!("{QID}elsewhere{QQ}"),
        );
        let found = one_of(&text, RuleCode::PackIdMismatch);
        assert_eq!(found.subject.as_deref(), Some("elsewhere"));
        assert_eq!(found.line, Some(4), "the line of the id, not of the table");
    }

    #[test]
    fn a_pack_identifier_outside_the_alphabet_is_one_problem_and_not_two() {
        // It fails the pattern and fails to match the file name at once. Two
        // lines would send a contributor looking for a second mistake.
        let text = GOOD.replace(
            &format!("{QID}whitespace{QQ}"),
            &format!("{QID}Whitespace{QQ}"),
        );
        assert_eq!(codes(&text), vec!["E010"], "{:?}", check(&text, PACK_ID));
    }

    #[test]
    fn a_pack_with_no_identifier_is_reported_once_by_the_rule_about_missing_fields() {
        // E003 already says the field is absent. E010 saying it again would give
        // one mistake two codes and two different repairs.
        let text = GOOD.replace(&format!("{QID}whitespace{QQ}{NL}"), "");
        assert_eq!(codes(&text), vec!["E003"], "{:?}", check(&text, PACK_ID));
    }

    #[test]
    fn the_second_value_to_take_an_identifier_is_the_one_reported() {
        // The first use is not the mistake. Reporting it would send the reader to
        // rename the value that was there first.
        let text = GOOD.to_owned() + SECOND_VALUE;
        let found = one_of(&text, RuleCode::DuplicateValueId);
        assert_eq!(found.subject.as_deref(), Some("trailing-space"));
        assert!(found.line.is_some_and(|line| line > 15), "{found:?}");
    }

    #[test]
    fn a_successor_declared_further_down_the_file_resolves() {
        // The reason the identifiers are gathered before the walk. A check that
        // only looked backwards would report a correct file as broken.
        let text = GOOD.replace(SINCE, &format!("{SINCE}{REPLACED_BY_LATER}")) + LATER_VALUE;
        assert!(
            !codes(&text).contains(&"E015"),
            "{:?}",
            check(&text, PACK_ID)
        );
    }

    #[test]
    fn a_successor_named_with_another_pack_is_an_unknown_identifier_here() {
        // The format defines succession within a pack and nowhere else. Letting
        // this form through would make the rule unenforceable in silence, which
        // is the failure this tool exists to find in other people's software.
        let text = GOOD.replace(SINCE, &format!("{SINCE}{REPLACED_BY_OTHER_PACK}"));
        let found = one_of(&text, RuleCode::ReplacedByUnknownId);
        assert_eq!(found.subject.as_deref(), Some("other-pack/successor"));
    }

    #[test]
    fn a_description_too_short_names_the_value_and_the_length_it_found() {
        let text = GOOD.replace(BREAKS_LINE, SHORT_BREAKS);
        let found = one_of(&text, RuleCode::BreaksTooShortOrEchoesName);
        assert_eq!(found.owner.as_deref(), Some("trailing-space"));
        assert_eq!(found.subject.as_deref(), Some("21"));
    }

    #[test]
    fn a_description_is_only_judged_for_english_where_the_pack_claims_english() {
        // A file declaring another language has already said its descriptions are
        // not English. Warning there argues with the author about something the
        // author already told us.
        let text = GOOD.replace(BREAKS_LINE, POLISH_BREAKS);
        assert!(
            codes(&text).contains(&"W034"),
            "{:?}",
            check(&text, PACK_ID)
        );

        let declared = text.replace(&format!("{QLANG}en{QQ}"), &format!("{QLANG}pl{QQ}"));
        assert!(
            !codes(&declared).contains(&"W034"),
            "{:?}",
            check(&declared, PACK_ID)
        );
    }

    #[test]
    fn a_pack_naming_one_source_for_everything_is_not_warned_about() {
        // The format says an absent `source` on a value inherits the pack's.
        // Warning on every value of such a pack would argue with that default.
        let text = GOOD.replace(LANG_LINE, &format!("{LANG_LINE}{PACK_SOURCE}"));
        assert!(
            !codes(&text).contains(&"W033"),
            "{:?}",
            check(&text, PACK_ID)
        );

        // Once one value is credited on its own, inheritance starts crediting
        // somebody for work that may not be theirs, and the warning earns itself.
        let patchy = text.clone() + SECOND_VALUE_WITH_SOURCE;
        assert!(
            codes(&patchy).contains(&"W033"),
            "{:?}",
            check(&patchy, PACK_ID)
        );
    }

    #[test]
    fn a_description_that_is_not_text_is_reported_instead_of_passed_over() {
        // This test used to record a silence: `breaks = 42` produced nothing at
        // all. A field written down and unreadable is worse than a field left
        // out, because it looks filled in.
        let text = GOOD.replace(BREAKS_LINE, NUMERIC_BREAKS);
        let found = one_of(&text, RuleCode::FieldOfUnusableType);
        assert_eq!(found.subject.as_deref(), Some("breaks"));
        assert_eq!(found.owner.as_deref(), Some("trailing-space"));

        // And the rule that reads the field stays quiet, because as far as it
        // can tell the field is not there. One mistake, one code.
        assert!(
            !codes(&text).contains(&"E030"),
            "{:?}",
            check(&text, PACK_ID)
        );
    }

    #[test]
    fn a_pack_field_of_the_wrong_kind_is_named_against_the_pack() {
        // Same code, two owners. Without the owner a reader cannot tell which
        // half of the file to open.
        let text = GOOD.replace(UPDATED_LINE, UPDATED_AS_TEXT);
        let found = one_of(&text, RuleCode::FieldOfUnusableType);
        assert_eq!(found.subject.as_deref(), Some("updated"));
        assert_eq!(found.owner.as_deref(), Some("pack"));
    }

    #[test]
    fn a_recipe_count_written_as_text_is_one_finding_and_not_two() {
        // E024 would say "declares no count" about a count the author can see on
        // the screen. E009 says the true thing, and E024 stays out of it.
        let text = with_value(RECIPE_WITH_TEXT_COUNT);
        let reported = codes(&text);
        assert!(reported.contains(&"E009"), "{reported:?}");
        assert!(!reported.contains(&"E024"), "{reported:?}");
    }

    #[test]
    fn a_recipe_that_really_is_missing_a_part_still_says_so() {
        // The other side of the precedence above. Suppressing E024 for a part
        // that is absent rather than mistyped would trade one silence for another.
        let text = with_value(RECIPE_WITHOUT_COUNT);
        assert!(
            codes(&text).contains(&"E024"),
            "{:?}",
            check(&text, PACK_ID)
        );
    }

    #[test]
    fn a_list_written_as_a_single_word_is_refused() {
        // The quietest of them all: `tags = "yaml"` reads as a tag list of one
        // in no parser, so the tags simply vanish.
        let text = GOOD.replace(LANG_LINE, &format!("{LANG_LINE}{TAGS_AS_TEXT}"));
        let found = one_of(&text, RuleCode::FieldOfUnusableType);
        assert_eq!(found.subject.as_deref(), Some("tags"));
    }
}
