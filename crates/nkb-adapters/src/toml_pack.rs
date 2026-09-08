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

use crate::canonical;
use nkb_app::{Date, PackFormat, TranslationCheck, TranslationTarget};
use nkb_core::description::{self, BreaksFault};
use nkb_core::identity::{is_pack_id, is_value_id};
use nkb_core::lint::{LintProblem, MAX_VALUES_PER_PACK, RuleCode};
use nkb_core::schema::{FieldKind, kind_of};
use nkb_core::source_text::SourceText;
use nkb_core::text::LiteralText;
use nkb_core::value::ValueBody;
use nkb_core::written_form;
use std::collections::{HashMap, HashSet};
use toml_edit::{Document, Item, Table, TableLike};

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

/// Every key the format defines inside `[pack]`, from the data model in
/// `pack-format.md` 4.
///
/// # Why these lists exist at all
///
/// E002 used to ask its question only at the top of the file, and a misspelling
/// is more likely the deeper it sits: `verison` in a pack header and `tgas`
/// beside a value are the same mistake, and neither was reported. Measured
/// 2026-09-07 across all six key spaces the format has - five were silent.
///
/// 🔴 The lists are taken from the specification's tables, not from what the code
/// happens to read. A list short by one field turns a correct pack into a refused
/// one, which is a worse failure than the silence it replaces.
pub(crate) const PACK_KEYS: [&str; 12] = [
    "id",
    "name",
    "description",
    "version",
    "updated",
    "license",
    "authors",
    "language",
    "tags",
    "fields",
    "risk",
    "source",
];

/// Every key the format defines inside a value: the data model in
/// `pack-format.md` 5, plus the three that describe a generated value from 6, and
/// `source` from the inheritance rule in 5.
pub(crate) const VALUE_KEYS: [&str; 16] = [
    "id",
    "name",
    "value",
    "breaks",
    "expect",
    "fields",
    "tags",
    "risk",
    "since",
    "shape",
    "deprecated",
    "replaced_by",
    "source",
    "type",
    "unit",
    "count",
];

/// Every key the format defines inside a pair, from `pack-format.md` 9.
///
/// Shorter than a value's on purpose: a pair names two values and carries no
/// value of its own, so the fields that describe one are absent rather than
/// optional.
pub(crate) const PAIR_KEYS: [&str; 8] = [
    "id", "name", "relation", "a", "b", "breaks", "expect", "since",
];

/// What a translation may carry in `[pack]`, from `pack-format.md` 10.
const TRANSLATION_PACK_KEYS: [&str; 2] = ["name", "description"];

/// What a translation may carry per entry.
///
/// Three, and the shortness is the rule rather than an oversight: a translation
/// changes prose and nothing else. E050 refuses the four fields that decide what
/// gets inserted; this refuses everything else the format did not put here,
/// including fields that are harmless in a source pack.
const TRANSLATION_ENTRY_KEYS: [&str; 3] = ["name", "breaks", "expect"];

/// The two values `risk` may hold, from `pack-format.md` 4 and 5.
///
/// Closed like the field vocabulary, and until 2026-09-07 the only closed set in
/// the format with no rule behind it. It decides whether a value is marked as
/// offensive before a tester sends it, so a misspelling here does not degrade to
/// a smaller feature - it degrades to no warning at all.
const RISK_LEVELS: [&str; 2] = ["normal", "offensive"];

/// The risk level that marks a value in the palette before a tester sends it.
///
/// Named rather than spelled out at each use: W063 and the vocabulary above have
/// to mean the same word, and two literals eventually stop doing that.
const OFFENSIVE: &str = "offensive";

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

/// Fields every pair must declare.
///
/// A pair names two values that already exist, so `a` and `b` are as necessary
/// as the identifier: without them there is no pair, only a description of one.
const REQUIRED_PAIR_FIELDS: [&str; 6] = ["id", "name", "since", "relation", "a", "b"];

/// The three relations the format defines.
///
/// Closed for the same reason the field vocabulary is closed: left open, the
/// palette cannot tell a contributor what a pair will do, and neither can the
/// contributor.
const RELATIONS: [&str; 3] = ["look-alike", "range", "identity"];

/// The fields that decide what the tool sends into somebody else's application.
///
/// A translation file may carry none of them. That is a safety property rather
/// than a tidiness rule: a reviewer reading a translation is reading prose, and
/// nothing in that file can change what a tester inserts somewhere.
///
/// Wider than the specification's wording, which names `value` alone. `unit`,
/// `count` and `type` decide the same thing for a generated value, so a rule
/// covering only the first would state a safety property and leave three doors
/// open. Recorded as a widening rather than made quietly - `decision-log.md` D35.
const INSERTION_FIELDS: [&str; 4] = ["value", "unit", "count", "type"];

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
pub fn check(file: &str, expected_id: &str) -> Vec<LintProblem> {
    // The index is built once, here, and everything below is handed it. Counting
    // newlines per question made the validator quadratic - OBS-98.
    let text = &SourceText::new(file);

    let document: Document<String> = match file.parse() {
        Ok(document) => document,
        Err(error) => {
            let line = error.span().map_or(1, |span| text.line(span.start));
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

    // W064 asks about the file rather than about the pack inside it, so it is
    // answered before the shapes part company: a translation is written by the
    // same people, reviewed in the same pull request, and formatted by the same
    // command.
    problems.extend(check_canonical(text));

    // A translation file has a different shape on purpose: it carries no values
    // and only the prose half of the pack table. Applying the source pack rules
    // to it would report a correct file as broken - a partial translation is
    // normal, and a missing description falls back to English in silence.
    if root.contains_key("translates") {
        problems.extend(check_translation_file(text, root));
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
    problems.extend(check_style(text, root));
    let context = pack_context(root);
    // One namespace for both, gathered before either walk. A pair is cited the
    // same way a value is, so an identifier taken by one may not be taken by the
    // other - and a pair may be written above the values it joins.
    let mut seen: HashSet<&str> = HashSet::new();
    problems.extend(check_values(text, root, &context, &mut seen));
    problems.extend(check_pairs(text, root, &context, &mut seen));
    problems
}

/// E001: the file format version, missing or newer than this build.
fn check_format(text: &SourceText<'_>, root: &Table) -> Vec<LintProblem> {
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
fn check_top_level_keys(text: &SourceText<'_>, root: &Table) -> Vec<LintProblem> {
    root.iter()
        .filter(|(key, _)| !TOP_LEVEL_KEYS.contains(key))
        .map(|(key, item)| {
            LintProblem::new(RuleCode::UnknownKey)
                .at(span_line(text, item))
                .about(key)
        })
        .collect()
}

/// E003, pack half: the table itself, and the fields it must carry.
fn check_pack_table(text: &SourceText<'_>, root: &Table) -> Vec<LintProblem> {
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

/// W064: the file is not written the way `nkb fmt` writes it.
///
/// The point is what it buys a reviewer, not tidiness: with it, a difference in
/// a pull request is always a difference of content. Without it, half the
/// comments on a contribution are about where an equals sign sits.
///
/// Reported against the first line that differs rather than against the file, so
/// a build log stays jumpable - and because "somewhere in this file" is the kind
/// of message people learn to skip.
fn check_canonical(text: &SourceText<'_>) -> Vec<LintProblem> {
    let Some(canonical) = canonical::render(text.as_str()) else {
        // Not TOML at all. E004 has already said so, and calling a file nobody
        // could read badly formatted would invent a second finding out of the
        // first one.
        return Vec::new();
    };
    if canonical == text.as_str() {
        return Vec::new();
    }

    // Counting from one, like every other line number in this validator. A file
    // that only differs past its last common line points at that line: the
    // difference is there, in what does or does not follow it.
    let line = text
        .as_str()
        .lines()
        .zip(canonical.lines())
        .position(|(had, wanted)| had != wanted)
        .unwrap_or_else(|| text.as_str().lines().count().saturating_sub(1));

    vec![LintProblem::new(RuleCode::FileNotCanonical).at(u32::try_from(line + 1).unwrap_or(1))]
}

/// W061, W062 and W063: the style rules that need the pack as a whole.
///
/// # Why W063 asks about the pack and not about the prose
///
/// The specification worded it as "marked offensive with no mention of it in
/// `breaks`", and that rule cannot be written. Measured on 2026-09-07 across the
/// catalogue's 242 descriptions: a word list narrow enough to raise no false
/// alarm matched **none** of the twelve offensive values, and the widest list
/// tried still missed a third of them while flagging three ordinary ones. The
/// cause is not a poor list. The catalogue's own editorial rule says a
/// description states what should happen to a value and not what can be done
/// with it, so the rule was asking for the sentence the catalogue forbids.
///
/// What is left is the case that actually costs something and is decidable
/// without reading prose: a value marked offensive inside a pack that is not.
/// There the pack around it says nothing about danger, so the mark arrives in
/// the palette with no explanation anywhere near it. Same shape as W033, which
/// warns about attribution that has become inconsistent rather than about
/// inheritance - `decision-log.md` D27.
fn check_style(text: &SourceText<'_>, root: &Table) -> Vec<LintProblem> {
    let Some(pack) = root.get("pack").and_then(Item::as_table_like) else {
        // No pack table at all is E003's finding. Style questions about a table
        // that is not there would be noise on top of it.
        return Vec::new();
    };
    let pack_line = root.get("pack").map_or(1, |item| span_line(text, item));
    let mut problems = Vec::new();

    // W061. Asks whether the pack declares tags, not whether they are usable: a
    // `tags` of the wrong kind is E009's finding, and two codes for one mistake
    // send a contributor to fix it twice.
    if !pack.contains_key("tags") {
        problems.push(LintProblem::new(RuleCode::PackWithoutTags).at(pack_line));
    }

    let values = root.get("values").and_then(Item::as_array_of_tables);

    let Some(values) = values else {
        // No values array at all is E008's finding. The two style rules below
        // are both about the values, so there is nothing left to say.
        return problems;
    };

    // W062.
    if values.len() > MAX_VALUES_PER_PACK {
        problems.push(
            LintProblem::new(RuleCode::PackTooLarge)
                .at(pack_line)
                .about(values.len().to_string()),
        );
    }

    // W063. Silent when the pack itself is already offensive: a pack that
    // declares it has said so in the one place a reader looks first.
    if pack.get("risk").and_then(Item::as_str) == Some(OFFENSIVE) {
        return problems;
    }

    for (index, table) in values.iter().enumerate() {
        if table.get("risk").and_then(Item::as_str) != Some(OFFENSIVE) {
            continue;
        }
        let identity = table
            .get("id")
            .and_then(Item::as_str)
            .map_or_else(|| format!("values[{index}]"), ToOwned::to_owned);
        problems.push(
            LintProblem::new(RuleCode::OffensiveValueInOrdinaryPack)
                .at(table.span().map_or(1, |span| text.line(span.start)))
                .owned_by(identity),
        );
    }

    problems
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
    text: &SourceText<'_>,
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

/// E009 and E028 over the pack table, which needs an owner the value tables get
/// from the walk that visits them.
///
/// # Why E028 belongs here as well as in the value walk
///
/// `fields` is declared in both places, and the pack table is where the
/// catalogue actually uses it - twenty packs out of twenty, measured 2026-09-07.
/// The value level is the exception, written only where one value differs from
/// the pack's default.
///
/// So a rule that walked only the values was blind at the wider of the two
/// scopes: a misspelling in the pack table mislabels every value in the pack at
/// once. The whole argument for a closed vocabulary is that `email`, `e-mail`
/// and `mail` must not all become field kinds, and a rule that does not look
/// where the vocabulary is used does not close it. Found by writing the first
/// real pack - `D41`.
fn check_pack_kinds(text: &SourceText<'_>, root: &Table) -> Vec<LintProblem> {
    let Some(pack) = root.get("pack").and_then(Item::as_table_like) else {
        return Vec::new();
    };
    let mut problems = check_keys(text, pack, &PACK_KEYS, "pack");
    problems.extend(
        check_field_kinds(text, pack.iter())
            .into_iter()
            .chain(check_fields_of(pack))
            .chain(check_risk(text, pack))
            .map(|problem| problem.owned_by("pack")),
    );
    problems
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
fn check_pack_id(text: &SourceText<'_>, root: &Table, expected_id: &str) -> Vec<LintProblem> {
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
fn check_values<'a>(
    text: &SourceText<'_>,
    root: &'a Table,
    pack: &PackContext,
    seen: &mut HashSet<&'a str>,
) -> Vec<LintProblem> {
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

    // What each value inserts, against the first value that inserted it. Filled
    // as the walk goes, so the value reported is always the later of the two -
    // the one somebody added, rather than the one that was already right.
    let mut bodies: HashMap<BodyKey, String> = HashMap::new();

    let mut problems = Vec::new();
    for (index, table) in values.iter().enumerate() {
        let line = table.span().map_or(1, |span| text.line(span.start));
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

        // E016: this value inserts exactly what an earlier one inserts.
        if let Some(key) = body_key(table)
            && let Some(earlier) = bodies.insert(key, identity.clone())
        {
            identity_problems.push(LintProblem::new(RuleCode::DuplicateValueBody).about(earlier));
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
            .chain(check_keys(text, table, &VALUE_KEYS, ""))
            .chain(check_risk(text, table))
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
fn check_body(text: &SourceText<'_>, table: &Table) -> Vec<LintProblem> {
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

/// What a value inserts, in a form two values can be compared by.
///
/// Three parts rather than one string, so that no separator has to be invented
/// and no value can be built that collides with another by containing it: the
/// kind, the text, and the repeat count (zero for a literal, which is not a
/// legal count and so cannot be confused with one).
type BodyKey = (&'static str, String, i64);

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
fn body_key(table: &Table) -> Option<BodyKey> {
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

/// E040, E041 and E042, and the rules a pair shares with a value.
///
/// A pair is two values that already exist, joined by a relation. It defines no
/// value of its own, so that each half keeps its own description and can be
/// cited on its own - which is why the endpoints are checked against the values
/// this file declares rather than taken on trust.
fn check_pairs<'a>(
    text: &SourceText<'_>,
    root: &'a Table,
    pack: &PackContext,
    seen: &mut HashSet<&'a str>,
) -> Vec<LintProblem> {
    let Some(pairs) = root.get("pairs").and_then(Item::as_array_of_tables) else {
        // Pairs are optional. A pack without them is the ordinary case.
        return Vec::new();
    };

    // The endpoints must name values, not other pairs: a pair joining two pairs
    // is a shape the format does not define and the palette could not present.
    let values: HashSet<&str> = root
        .get("values")
        .and_then(Item::as_array_of_tables)
        .into_iter()
        .flat_map(|values| values.iter())
        .filter_map(|table| table.get("id").and_then(Item::as_str))
        .collect();

    let mut problems = Vec::new();
    for (index, table) in pairs.iter().enumerate() {
        let line = table.span().map_or(1, |span| text.line(span.start));
        let declared_id = table.get("id").and_then(Item::as_str);
        let identity = declared_id.map_or_else(|| format!("pairs[{index}]"), ToOwned::to_owned);

        let mut found = check_field_kinds(text, table.iter());
        found.extend(check_keys(text, table, &PAIR_KEYS, ""));

        if let Some(id) = declared_id {
            if !seen.insert(id) {
                found.push(LintProblem::new(RuleCode::DuplicateValueId).about(id));
            }
            if !is_value_id(id) {
                found.push(LintProblem::new(RuleCode::ValueIdMalformed).about(id));
            }
        }

        for field in REQUIRED_PAIR_FIELDS {
            if !table.contains_key(field) {
                found.push(LintProblem::new(RuleCode::MissingRequiredField).about(field));
            }
        }

        if let Some(relation) = table.get("relation").and_then(Item::as_str)
            && !RELATIONS.contains(&relation)
        {
            found.push(LintProblem::new(RuleCode::UnknownRelation).about(relation));
        }

        let a = table.get("a").and_then(Item::as_str);
        let b = table.get("b").and_then(Item::as_str);
        if let Some(endpoint) = a
            && a == b
        {
            // Reported before the endpoints are looked up, and the lookup below
            // then sees one endpoint rather than the same one twice.
            found.push(LintProblem::new(RuleCode::PairEndpointsIdentical).about(endpoint));
        }

        for endpoint in [a, b].into_iter().flatten().collect::<HashSet<&str>>() {
            if !values.contains(endpoint) {
                found.push(LintProblem::new(RuleCode::PairEndpointUnknown).about(endpoint));
            }
        }

        // A pair carries a description for the same reason a value does, and the
        // sentence rule reaches it for the same reason. W033 does not: crediting
        // a source belongs to the values a pair joins, not to the joining.
        found.extend(check_description(table, pack, false));

        for mut problem in found {
            problem.line = problem.line.or(Some(line));
            problem.owner = Some(identity.clone());
            problems.push(problem);
        }
    }
    problems
}

/// E050, E051 and the field kinds, over a translation file.
///
/// Walked by shape rather than by searching the whole document for a key name:
/// a value whose identifier happens to be `count` would make a blind search
/// report a rule violation that is not there.
///
/// # Both tables, not only `values`
///
/// A pair carries prose of its own, so a translation may carry a `[pairs]` table
/// too, and E050 has to reach it. Until 2026-09-07 it did not: a `value` beside a
/// translated pair passed in silence, measured. A pair holds no value to insert
/// today, so nothing was reachable through that gap - but the rule is written
/// without a condition, the reviewer's guarantee is that only prose can change,
/// and a format that is not frozen may yet give a pair something to carry.
/// Closing a door while it is still inert is the cheap moment - `D40`.
fn check_translation_file(text: &SourceText<'_>, root: &Table) -> Vec<LintProblem> {
    // The root of a translation is the one table nothing walked, so `language`
    // and `translates` were never judged for their kind. E009 has to arrive
    // before the rule that reads a field, or that rule reads a field the format
    // cannot use - `D32`, and the reason this call is first.
    let mut problems: Vec<LintProblem> = check_field_kinds(
        text,
        root.iter().filter(|(key, _)| TOP_LEVEL_KEYS.contains(key)),
    );

    problems.extend(check_translates_names_a_pack(text, root));

    if let Some(pack) = root.get("pack").and_then(Item::as_table_like) {
        problems.extend(
            check_field_kinds(text, pack.iter())
                .into_iter()
                .chain(check_insertion_fields(text, pack.iter()))
                .chain(check_translation_keys(text, pack, &TRANSLATION_PACK_KEYS))
                .map(|problem| problem.owned_by("pack")),
        );
    }

    for (owner, table) in entries_of(root, "values").chain(entries_of(root, "pairs")) {
        for mut problem in check_field_kinds(text, table.iter())
            .into_iter()
            .chain(check_insertion_fields(text, table.iter()))
            .chain(check_translation_keys(text, table, &TRANSLATION_ENTRY_KEYS))
        {
            problem.owner = Some(owner.clone());
            problems.push(problem);
        }
    }
    problems
}

/// Every entry under `values` or `pairs`, whichever of the two shapes was used.
///
/// # Why both shapes, when the format defines one for each kind of file
///
/// A source pack writes `[[values]]` and a translation writes `[values.<id>]`.
/// Nothing in the format lets a translation use the array shape - but E050 is a
/// safety property, and a safety rule that only looks where the format says a
/// field should be is a rule anybody can step around by putting the field
/// somewhere else. So the walk covers both, and a translation written in the
/// wrong shape is caught rather than skipped in silence.
///
/// The identity rules do the opposite and follow the format exactly, because
/// there the question is what the file means rather than what it could smuggle.
fn entries_of<'a>(
    root: &'a Table,
    table_name: &str,
) -> Box<dyn Iterator<Item = (String, &'a dyn TableLike)> + 'a> {
    let Some(item) = root.get(table_name) else {
        return Box::new(std::iter::empty());
    };

    if let Some(keyed) = item.as_table_like() {
        return Box::new(
            keyed
                .iter()
                .filter_map(|(id, entry)| entry.as_table_like().map(|t| (id.to_owned(), t))),
        );
    }

    if let Some(listed) = item.as_array_of_tables() {
        return Box::new(listed.iter().enumerate().map(|(index, table)| {
            let id = table
                .get("id")
                .and_then(Item::as_str)
                .map_or_else(|| format!("#{}", index + 1), ToOwned::to_owned);
            (id, table as &dyn TableLike)
        }));
    }

    Box::new(std::iter::empty())
}

/// E051, the half that needs no second file: a `translates` naming nothing that
/// could be a pack.
///
/// 🔴 This runs before anything goes looking on a disk, and that ordering is the
/// rule rather than an implementation detail. The content of this field comes
/// from a stranger and is turned into a path by joining it to a directory, so a
/// value like `../../secrets` would read outside the folder the pack lives in.
/// The pack identifier alphabet has no dot and no separator in it, which is what
/// makes the check a refusal rather than a sanitisation.
fn check_translates_names_a_pack(text: &SourceText<'_>, root: &Table) -> Vec<LintProblem> {
    let Some(item) = root.get("translates") else {
        return Vec::new();
    };

    // A field of the wrong kind is E009's to report, and two codes for one
    // mistake help nobody.
    let Some(named) = item.as_str() else {
        return Vec::new();
    };

    if is_pack_id(named) {
        return Vec::new();
    }
    vec![
        LintProblem::new(RuleCode::TranslatesUnknownPack)
            .at(span_line(text, item))
            .about(named),
    ]
}

/// The identifiers a pack file offers a translation to translate.
///
/// Values and pairs together, because the two share one namespace (`D34`): a
/// translation citing a pair cites it exactly the way it cites a value, so a
/// check that knew only about values would report every translated pair as
/// vanished.
/// Read from the shape a **source pack** uses: `[[values]]` and `[[pairs]]`,
/// each carrying its identifier in an `id` field. A translation keys its tables
/// by the identifier instead, and reading one shape as the other would report
/// every translated entry as vanished.
fn translatable_ids(root: &Table) -> HashSet<String> {
    let mut ids = HashSet::new();
    for table_name in ["values", "pairs"] {
        let Some(entries) = root.get(table_name).and_then(Item::as_array_of_tables) else {
            continue;
        };
        ids.extend(
            entries
                .iter()
                .filter_map(|table| table.get("id").and_then(Item::as_str))
                .map(ToOwned::to_owned),
        );
    }
    ids
}

/// W052: a translation describing an identifier the pack no longer has.
///
/// A warning rather than an error, and deliberately so: a translation follows the
/// pack without a version of its own, so a stale entry is stale prose, not a
/// broken pack. It is reported and passed through.
fn check_translated_ids(
    text: &SourceText<'_>,
    root: &Table,
    known: &HashSet<String>,
) -> Vec<LintProblem> {
    let mut problems = Vec::new();
    for table_name in ["values", "pairs"] {
        // The keyed shape only. This is an identity question rather than a safety
        // one: a file written in the other shape declares no identifiers a
        // translation could be following, so there is nothing here to compare.
        let Some(entries) = root.get(table_name).and_then(Item::as_table_like) else {
            continue;
        };
        for (id, item) in entries.iter() {
            if !known.contains(id) {
                problems.push(
                    LintProblem::new(RuleCode::TranslationRefersToMissingId)
                        .at(span_line(text, item))
                        .about(id),
                );
            }
        }
    }
    problems
}

/// E050: a field that decides what gets inserted, in a file that may only carry
/// prose.
fn check_insertion_fields<'a>(
    text: &SourceText<'_>,
    fields: impl Iterator<Item = (&'a str, &'a Item)>,
) -> Vec<LintProblem> {
    fields
        .filter(|(key, _)| INSERTION_FIELDS.contains(key))
        .map(|(key, item)| {
            LintProblem::new(RuleCode::TranslationCarriesValue)
                .at(span_line(text, item))
                .about(key)
        })
        .collect()
}

/// E002: a key the format does not define, inside a table.
///
/// Reported against the table it sits in rather than the file, so that `owner` in
/// the machine readable output says where - the same shape E003 uses for its two
/// levels, and the reason neither of them needed a second code (`D30`).
fn check_keys(
    text: &SourceText<'_>,
    table: &dyn TableLike,
    allowed: &[&str],
    owner: &str,
) -> Vec<LintProblem> {
    table
        .iter()
        .filter(|(key, _)| !allowed.contains(key))
        .map(|(key, item)| {
            LintProblem::new(RuleCode::UnknownKey)
                .at(span_line(text, item))
                .about(key)
                .owned_by(owner)
        })
        .collect()
}

/// E002 over a translation table, minus the keys E050 already speaks about.
///
/// A `value` in a translation is both "a field that decides what gets inserted"
/// and "a key a translation may not carry", and they are one mistake with one
/// repair: delete the field. Reporting both would hand a contributor two codes
/// and one thing to do, which is the objection that kept E003 from splitting in
/// two (`D30`). E050 keeps it, because it is the one that says why it matters.
fn check_translation_keys(
    text: &SourceText<'_>,
    table: &dyn TableLike,
    allowed: &[&str],
) -> Vec<LintProblem> {
    check_keys(text, table, allowed, "")
        .into_iter()
        .filter(|problem| {
            problem
                .subject
                .as_deref()
                .is_none_or(|key| !INSERTION_FIELDS.contains(&key))
        })
        .collect()
}

/// E017: `risk` outside the two levels the format defines.
///
/// Anything unrecognised is read as `normal`, which is the quiet answer: a pack
/// that meant to warn about its values ships them unmarked, and nothing on screen
/// says so.
fn check_risk(text: &SourceText<'_>, table: &dyn TableLike) -> Vec<LintProblem> {
    let Some(item) = table.get("risk") else {
        return Vec::new();
    };
    // A `risk` of the wrong kind is E009's, and two codes for one mistake help
    // nobody.
    let Some(level) = item.as_str() else {
        return Vec::new();
    };
    if RISK_LEVELS.contains(&level) {
        return Vec::new();
    }
    vec![
        LintProblem::new(RuleCode::RiskOutsideVocabulary)
            .at(span_line(text, item))
            .about(level),
    ]
}

/// E028: field kinds outside the closed vocabulary, in a value table.
fn check_fields(table: &Table) -> Vec<LintProblem> {
    check_fields_of(table)
}

/// The same rule over anything that carries a `fields` array.
///
/// Written against the shared trait rather than the concrete table so that the
/// pack table and a value table cannot drift into two readings of one rule - the
/// drift that let the pack table go unchecked in the first place.
fn check_fields_of(table: &dyn TableLike) -> Vec<LintProblem> {
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
fn raw_slice<'a>(text: &SourceText<'a>, item: &Item) -> &'a str {
    item.span().map_or("", |span| text.slice(span))
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
    /// The pack this file describes. Lives in `read_pack.rs`, because reading a
    /// file to obey it and reading a file to fault it are two different habits
    /// and mixing them in one module makes both harder to trust.
    fn parse(&self, text: &str) -> Option<nkb_core::pack::Pack> {
        crate::read_pack::parse(text)
    }

    fn check(&self, text: &str, expected_id: &str) -> Vec<LintProblem> {
        check(text, expected_id)
    }

    fn translated_pack(&self, text: &str) -> TranslationTarget {
        let Ok(document) = text.parse::<Document<String>>() else {
            // A file that does not parse is reported as E004 and returned alone,
            // and the sentence for E004 already says nothing else in the file
            // could be checked. Recording a skip per rule on top of that would
            // repeat one answer forty times.
            return TranslationTarget::NotATranslation;
        };
        let root = document.as_table();
        let Some(item) = root.get("translates") else {
            return TranslationTarget::NotATranslation;
        };

        // The shape is checked here rather than by the caller, so that no name
        // leaves this port unless it is one the format could have written. The
        // caller joins it to a directory.
        match item.as_str() {
            Some(named) if is_pack_id(named) => TranslationTarget::Pack(named.to_owned()),
            _ => TranslationTarget::Unusable,
        }
    }

    fn canonical(&self, text: &str) -> Option<String> {
        canonical::render(text)
    }

    fn same_insertions(&self, before: &str, after: &str) -> bool {
        canonical::same_insertions(before, after)
    }

    fn skeleton(&self, id: &str, today: Date) -> String {
        crate::skeleton::for_pack(id, today)
    }

    fn check_translation(&self, text: &str, translated: &str) -> TranslationCheck {
        let Ok(translated_document) = translated.parse::<Document<String>>() else {
            return TranslationCheck::TranslatedPackDidNotParse;
        };
        let Ok(document) = text.parse::<Document<String>>() else {
            // Unreachable in the flow that calls this - the caller has already
            // parsed this file. Written as an answer rather than as a panic
            // because a crash inside a validator is a crash inside somebody
            // else's continuous integration.
            return TranslationCheck::Compared(Vec::new());
        };

        let known = translatable_ids(translated_document.as_table());
        TranslationCheck::Compared(check_translated_ids(
            &SourceText::new(text),
            document.as_table(),
            &known,
        ))
    }
}

/// The line an item starts on, or the first line when the parser kept no span.
fn span_line(text: &SourceText<'_>, item: &Item) -> u32 {
    item.span().map_or(1, |span| text.line(span.start))
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
        "tags = [\"probe\"]\n",
        "\n",
        "[[values]]\n",
        "id = \"trailing-space\"\n",
        "name = \"Trailing space\"\n",
        "value = \"Kowalski\\u0020\"\n",
        "breaks = \"Sorting puts the record at the top of every list, and an exact match against the trimmed name finds nothing.\"\n",
        "expect = \"Trimmed on save, or preserved and found by a search for the trimmed name.\"\n",
        "since = \"1.0\"\n",
    );

    /// Every rule the file breaks, apart from the one about its shape.
    ///
    /// # Why W064 is dropped here and nowhere else
    ///
    /// The fixtures in this module are snippets: one good file and a substitution
    /// that breaks a single thing. Their **shape** is not the subject, and
    /// requiring each of them to be lined up would make every test about a rule
    /// carry a second claim it was never written to make.
    ///
    /// This is a filter, so it is the kind of thing that hides a dead rule. It
    /// does not, and the reason is that W064 is covered where it belongs: the
    /// `canonical` module tests it directly, the format suite carries a file
    /// written for it, and two more files there declare it because their bytes
    /// can never be canonical. The test below keeps this honest by asserting the
    /// rule still fires through the unfiltered path.
    fn codes(text: &str) -> Vec<&'static str> {
        check(text, PACK_ID)
            .iter()
            .map(|p| p.code.as_str())
            .filter(|code| *code != "W064")
            .collect()
    }

    /// Every rule, W064 included. Used by the tests that are about the shape.
    fn all_codes(text: &str) -> Vec<&'static str> {
        check(text, PACK_ID)
            .iter()
            .map(|p| p.code.as_str())
            .collect()
    }

    #[test]
    fn the_filter_above_hides_a_rule_that_is_still_alive() {
        // 🔴 The guard on the helper. A filter in a test helper is exactly how a
        // rule quietly stops working, so the rule it filters is asserted here
        // through the path that does not filter.
        //
        // GOOD is written with one space before each equals sign rather than
        // lined up, so it is not canonical - which is what makes it usable as a
        // fixture for everything else and what makes it the right witness here.
        assert!(
            all_codes(GOOD).contains(&"W064"),
            "GOOD stopped being a witness for W064: {:?}",
            all_codes(GOOD)
        );
        assert!(
            !codes(GOOD).contains(&"W064"),
            "the filter stopped filtering"
        );
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
            .find(|p| p.code == RuleCode::UnknownKey)
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
            "tags = [\"probe\"]\n",
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
        // Seven rules from five different families, out of one file, in one run.
        assert_eq!(
            found,
            vec!["E001", "E002", "E003", "E010", "E030", "W032", "W061"]
        );
    }
    /// Replaces the value line of GOOD with whatever a test needs.
    fn with_value(replacement: &str) -> String {
        GOOD.replace("value = \"Kowalski\\u0020\"\n", replacement)
    }

    /// Adds one line to the `[pack]` table, and refuses to hand back a string it
    /// did not actually change.
    ///
    /// A `replace` that matches nothing is silent, and a test built on one passes
    /// or fails for a reason that has nothing to do with the rule it names. That
    /// happened here: the anchor carried padding the constant does not, so the
    /// test ran against a file with no `fields` at all.
    fn with_pack_field(line: &str) -> String {
        const ANCHOR: &str = "language = \"en\"\n";
        assert!(
            GOOD.contains(ANCHOR),
            "the pack table anchor moved - fix this helper rather than the tests using it"
        );
        GOOD.replace(ANCHOR, &format!("{ANCHOR}{line}"))
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

    /// Appends a second value to the good pack, so two bodies can be compared.
    fn with_second_value(body: &str) -> String {
        format!(
            "{GOOD}\n[[values]]\nid = \"second\"\nname = \"Second\"\n{body}\
             breaks = \"A second entry, present so that two bodies exist to compare with each other.\"\n\
             expect = \"Whatever the rule under test says about it.\"\n\
             since = \"1.0\"\n"
        )
    }

    #[test]
    fn two_values_that_insert_the_same_thing_are_refused_and_the_later_one_is_named() {
        // 🔴 Found by accident rather than by review: writing the first real pack,
        // one value lost an escape and became byte-identical to the value above
        // it. The validator said nothing, and a pack claiming twelve test values
        // carried eleven. E011 is about identifiers; this is about what they
        // stand for.
        let text = with_second_value("value = \"Kowalski\\u0020\"\n");
        let problems = check(&text, PACK_ID);
        let found = problems
            .iter()
            .find(|p| p.code == RuleCode::DuplicateValueBody)
            .expect("E016 must be reported");
        assert_eq!(
            found.owner.as_deref(),
            Some("second"),
            "the later value is the one somebody added, so it is the one to fix"
        );
        assert_eq!(found.subject.as_deref(), Some("trailing-space"));
    }

    #[test]
    fn the_same_value_written_two_ways_is_still_the_same_value() {
        // The decision this test exists for: the comparison happens after
        // parsing. What reaches somebody else's field is what decides, not how it
        // was typed - so the four digit escape for a newline and the one letter
        // escape for the same newline are one value spelled twice.
        //
        // Both spellings are escapes, so nothing else fires and the E016 below is
        // the only reason this file is refused.
        let text = with_second_value("value = \"a\\nb\"\n")
            .replace("value = \"Kowalski\\u0020\"\n", "value = \"a\\u000Ab\"\n");
        assert_eq!(codes(&text), vec!["E016"], "{:?}", check(&text, PACK_ID));
    }

    #[test]
    fn two_generated_values_are_compared_by_their_recipe() {
        let same = with_second_value("type = \"repeat\"\nunit = \"a\"\ncount = 5\n").replace(
            "value = \"Kowalski\\u0020\"\n",
            "type = \"repeat\"\nunit = \"a\"\ncount = 5\n",
        );
        assert!(
            codes(&same).contains(&"E016"),
            "{:?}",
            check(&same, PACK_ID)
        );

        let different = with_second_value("type = \"repeat\"\nunit = \"a\"\ncount = 6\n").replace(
            "value = \"Kowalski\\u0020\"\n",
            "type = \"repeat\"\nunit = \"a\"\ncount = 5\n",
        );
        assert!(
            !codes(&different).contains(&"E016"),
            "{:?}",
            check(&different, PACK_ID)
        );
    }

    #[test]
    fn a_literal_and_a_generator_producing_the_same_text_are_not_compared() {
        // The limit of the rule, stated as a test rather than left to be
        // discovered. Comparing these would mean building the text, and a
        // generator describes up to a million characters from one line - the tool
        // measures a length bomb without setting it off, and this rule does not
        // get to be the exception.
        let text = with_second_value("type = \"repeat\"\nunit = \"a\"\ncount = 3\n")
            .replace("value = \"Kowalski\\u0020\"\n", "value = \"aaa\"\n");
        assert!(
            !codes(&text).contains(&"E016"),
            "{:?}",
            check(&text, PACK_ID)
        );
    }

    #[test]
    fn two_values_with_no_body_are_two_missing_fields_and_not_a_duplicate() {
        // Both of them, not one. A single bodiless value has nothing to collide
        // with, so a test written that way passes even when the rule treats an
        // absent value as an empty one - measured by mutation, which survived it.
        //
        // The distinction matters because the repairs are opposite: E003 says
        // fill this in, E016 says delete one of the two. A reader sent to delete
        // a value that only needed writing loses the value.
        let text = with_second_value("")
            + "\n[[values]]\nid = \"third\"\nname = \"Third\"\n\
             breaks = \"A third entry with nothing to insert, so that two of them are missing it.\"\n\
             expect = \"Reported as a missing field, once for each.\"\nsince = \"1.0\"\n";
        let reported = codes(&text);
        assert!(reported.contains(&"E003"), "{reported:?}");
        assert!(!reported.contains(&"E016"), "{reported:?}");
    }

    #[test]
    fn two_deliberately_empty_values_are_a_duplicate() {
        // The other side of the same line. An empty value is a real test case -
        // the boundary between "field empty" and "field filled" is one of the
        // most often confused things in validation, and the format says so - and
        // two of them are as redundant as any other pair.
        let text = with_second_value("value = \"\"\n")
            .replace("value = \"Kowalski\\u0020\"\n", "value = \"\"\n");
        assert!(
            codes(&text).contains(&"E016"),
            "{:?}",
            check(&text, PACK_ID)
        );
    }

    #[test]
    fn an_unknown_key_is_refused_in_every_table_the_format_defines() {
        // 🔴 One test per level, and the reason is the blind spot rather than
        // thoroughness. The file suite compares a set of codes, and E002 already
        // fires from the value level in another file - so narrowing this rule back
        // to one table would pass the whole suite unnoticed. Measured by mutation:
        // three of the four levels survived until this test existed.
        //
        // The vocabularies come from the specification's own tables. A list short
        // by one field turns a correct pack into a refused one, which is the worse
        // of the two failures.
        let pack_level = with_pack_field("verison = \"1.0\"\n");
        let found = one_of(&pack_level, RuleCode::UnknownKey);
        assert_eq!(found.subject.as_deref(), Some("verison"));
        assert_eq!(found.owner.as_deref(), Some("pack"));

        let value_level = GOOD.replace("since = \"1.0\"\n", "tgas = [\"yaml\"]\nsince = \"1.0\"\n");
        let found = one_of(&value_level, RuleCode::UnknownKey);
        assert_eq!(found.subject.as_deref(), Some("tgas"));
        assert_eq!(found.owner.as_deref(), Some("trailing-space"));

        let pair_level = format!(
            "{GOOD}\n[[pairs]]\nid = \"the-pair\"\nname = \"The pair\"\nrelation = \"identity\"\n\
             a = \"trailing-space\"\nb = \"trailing-space\"\nsince = \"1.0\"\nnotes = \"x\"\n\
             breaks = \"Two spellings that nobody can tell apart anywhere in the interface.\"\n\
             expect = \"Refused, or kept apart consistently.\"\n"
        );
        let found = one_of(&pair_level, RuleCode::UnknownKey);
        assert_eq!(found.subject.as_deref(), Some("notes"));
        assert_eq!(found.owner.as_deref(), Some("the-pair"));
    }

    #[test]
    fn an_unknown_key_is_refused_in_both_tables_of_a_translation() {
        // A translation carries prose and nothing else, so its vocabulary is the
        // shortest in the format - which makes a stray key there the easiest to
        // miss and the least excusable to allow.
        let pack_level = A_TRANSLATION.replace(
            "description = \"Znaki, ktore zajmuja miejsce.\"\n",
            "description = \"Znaki, ktore zajmuja miejsce.\"\nversion = \"1.0\"\n",
        );
        let found = one_of(&pack_level, RuleCode::UnknownKey);
        assert_eq!(found.subject.as_deref(), Some("version"));
        assert_eq!(found.owner.as_deref(), Some("pack"));

        let entry_level = A_TRANSLATION.replace(
            "name = \"Spacja na koncu\"\n",
            "name = \"Spacja na koncu\"\nsince = \"1.0\"\n",
        );
        let found = one_of(&entry_level, RuleCode::UnknownKey);
        assert_eq!(
            found.subject.as_deref(),
            Some("since"),
            "a field that is ordinary in a source pack is still not prose"
        );
        assert_eq!(found.owner.as_deref(), Some("trailing-space"));
    }

    #[test]
    fn a_field_kind_outside_the_closed_list_is_refused_in_the_pack_table_too() {
        // 🔴 Found by writing the first real pack, 2026-09-07. The rule walked
        // the values and not the pack table - and the pack table is where the
        // catalogue actually declares this: twenty packs out of twenty, measured.
        // A typo there mislabels every value in the pack at once, which is the
        // wider mistake of the two, not the narrower one.
        //
        // The whole argument for a closed vocabulary is that `email`, `e-mail`
        // and `mail` must not all become field kinds. A rule blind to the place
        // the vocabulary is used is not a closed vocabulary.
        let text = with_pack_field("fields = [\"e-mail\"]\n");
        let problems = check(&text, PACK_ID);
        let found = problems
            .iter()
            .find(|p| p.code == RuleCode::FieldOutsideVocabulary)
            .expect("E028 must be reported for the pack table");
        assert_eq!(found.subject.as_deref(), Some("e-mail"));
        assert_eq!(
            found.owner.as_deref(),
            Some("pack"),
            "the reader has to be told which of the two places carries it"
        );
    }

    #[test]
    fn a_field_kind_inside_the_list_passes_in_the_pack_table_too() {
        let text = with_pack_field("fields = [\"any\"]\n");
        assert!(codes(&text).is_empty(), "{:?}", check(&text, PACK_ID));
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
    /// The tags line the good file carries, so a test can swap it rather than
    /// add a second one.
    const GOOD_TAGS: &str = "tags = [\"probe\"]\n";
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
        // Replaces the good file's tags rather than adding a second line: two
        // `tags` keys in one table is a duplicate key, which the parser refuses
        // before any rule about kinds gets a look at it.
        let text = GOOD.replace(GOOD_TAGS, TAGS_AS_TEXT);
        assert_ne!(text, GOOD, "the tags line of GOOD moved");
        let found = one_of(&text, RuleCode::FieldOfUnusableType);
        assert_eq!(found.subject.as_deref(), Some("tags"));
    }

    const A_PAIR: &str = "\n[[pairs]]\nid = \"the-pair\"\nname = \"The pair\"\nrelation = \"look-alike\"\na = \"trailing-space\"\nb = \"trailing-space\"\nbreaks = \"Nothing on its own - this pair only exists so the file is otherwise complete.\"\nexpect = \"Nothing at all.\"\nsince = \"1.0\"\n";
    const PAIR_TAKING_A_VALUE_ID: &str = "\n[[pairs]]\nid = \"trailing-space\"\nname = \"The pair\"\nrelation = \"look-alike\"\na = \"trailing-space\"\nb = \"trailing-space\"\nbreaks = \"Nothing on its own - this pair only exists so the file is otherwise complete.\"\nexpect = \"Nothing at all.\"\nsince = \"1.0\"\n";
    const PAIR_WITHOUT_AN_ENDPOINT: &str = "\n[[pairs]]\nid = \"half-a-pair\"\nname = \"Half a pair\"\nrelation = \"look-alike\"\nb = \"trailing-space\"\nbreaks = \"Nothing on its own - this pair only exists so the file is otherwise complete.\"\nexpect = \"Nothing at all.\"\nsince = \"1.0\"\n";
    const PAIR_WITHOUT_A_DESCRIPTION: &str = "\n[[pairs]]\nid = \"undescribed-pair\"\nname = \"Undescribed pair\"\nrelation = \"look-alike\"\na = \"trailing-space\"\nb = \"trailing-space\"\nexpect = \"Nothing at all.\"\nsince = \"1.0\"\n";
    const A_TRANSLATION: &str = "format = 1\ntranslates = \"whitespace\"\nlanguage = \"pl\"\n\n[pack]\nname = \"Biale znaki\"\ndescription = \"Znaki, ktore zajmuja miejsce.\"\n\n[values.trailing-space]\nname = \"Spacja na koncu\"\nbreaks = \"Sortowanie stawia rekord na poczatku listy.\"\n";
    const TRANSLATION_OF_A_VALUE_CALLED_COUNT: &str = "format = 1\ntranslates = \"whitespace\"\nlanguage = \"pl\"\n\n[values.count]\nname = \"Licznik\"\nbreaks = \"Opis wartosci o takim wlasnie identyfikatorze.\"\n";

    #[test]
    fn a_pair_endpoint_must_name_a_value_and_not_another_pair() {
        // The endpoints are looked up among the values on purpose. A pair joining
        // two pairs is a shape the format does not define and the palette could
        // not present, and taking the identifier on trust would let one in.
        let text = GOOD.to_owned()
            + A_PAIR
            + &A_PAIR
                .replace("the-pair", "second-pair")
                .replace("a = \"trailing-space\"", "a = \"the-pair\"");
        let found = one_of(&text, RuleCode::PairEndpointUnknown);
        assert_eq!(found.subject.as_deref(), Some("the-pair"));
        assert_eq!(found.owner.as_deref(), Some("second-pair"));
    }

    #[test]
    fn a_pair_may_not_take_an_identifier_a_value_already_uses() {
        // One namespace for both, because a pair is cited exactly the way a value
        // is. Two namespaces would give the report block two syntaxes.
        let text = GOOD.to_owned() + PAIR_TAKING_A_VALUE_ID;
        let found = one_of(&text, RuleCode::DuplicateValueId);
        assert_eq!(found.subject.as_deref(), Some("trailing-space"));
    }

    #[test]
    fn a_pair_missing_an_endpoint_is_told_that_and_not_that_the_endpoint_is_unknown() {
        // E040 would say "names something nobody declared" about a field the
        // author never wrote. Two different repairs, so two different rules.
        let text = GOOD.to_owned() + PAIR_WITHOUT_AN_ENDPOINT;
        let reported = codes(&text);
        assert!(reported.contains(&"E003"), "{reported:?}");
        assert!(!reported.contains(&"E040"), "{reported:?}");
    }

    #[test]
    fn the_sentence_rule_reaches_a_pair_as_well_as_a_value() {
        // A pair without a sentence is a curiosity for the same reason a value
        // without one is - and a pair exists only for the sentence that a single
        // value cannot carry.
        let text = GOOD.to_owned() + PAIR_WITHOUT_A_DESCRIPTION;
        let found = one_of(&text, RuleCode::MissingBreaks);
        assert_eq!(found.owner.as_deref(), Some("undescribed-pair"));
    }

    #[test]
    fn a_translation_is_not_judged_by_the_rules_of_a_source_pack() {
        // It carries no identifier, no licence and no values of its own, and a
        // partial translation is the normal case rather than a broken file.
        assert!(
            codes(A_TRANSLATION).is_empty(),
            "{:?}",
            check(A_TRANSLATION, "whitespace.pl")
        );
    }

    #[test]
    fn a_translation_carrying_a_field_that_decides_what_is_inserted_is_refused() {
        let text = A_TRANSLATION.to_owned() + "value = \"anything\"\n";
        let found = one_of(&text, RuleCode::TranslationCarriesValue);
        assert_eq!(found.subject.as_deref(), Some("value"));
        assert_eq!(found.owner.as_deref(), Some("trailing-space"));
    }

    #[test]
    fn a_translation_carrying_only_a_count_is_refused_just_the_same() {
        // The guard on the widening. The specification names `value` alone, and a
        // rule covering only that would state a safety property while leaving
        // `unit`, `count` and `type` able to change what a tester inserts.
        //
        // The file in the rejected set cannot prove this on its own: it carries
        // both fields, so it reports E050 either way and a narrowing would pass
        // it unnoticed.
        let text = A_TRANSLATION.to_owned()
            + "count = 5
";
        let found = one_of(&text, RuleCode::TranslationCarriesValue);
        assert_eq!(found.subject.as_deref(), Some("count"));
    }

    #[test]
    fn a_translated_value_whose_identifier_is_a_field_name_is_not_a_false_alarm() {
        // The trap the structural walk exists to avoid. Searching the document
        // for a key called `count` would find this value's own identifier and
        // report a rule violation that is not there - on a file that is correct.
        let reported = codes(TRANSLATION_OF_A_VALUE_CALLED_COUNT);
        assert!(reported.is_empty(), "{reported:?}");
    }

    /// A source pack in the shape the format defines: entries in an array of
    /// tables, each carrying its identifier in an `id` field. Deliberately not
    /// the shape a translation uses, because reading one as the other is the
    /// mistake these tests exist to catch.
    const A_PACK_WITH_TWO_ENTRIES: &str = concat!(
        "format = 1\n\n[pack]\nid = \"whitespace\"\nname = \"Whitespace\"\n",
        "description = \"Characters that take up room.\"\nversion = \"1.0\"\n",
        "updated = 2026-09-06\nlicense = \"CC-BY-4.0\"\nauthors = [\"n\"]\n",
        "language = \"en\"\n\n",
        "[[values]]\nid = \"trailing-space\"\nname = \"Trailing space\"\nvalue = \"a \"\n",
        "breaks = \"Sorting puts the record at the top of the list, where nobody looks.\"\n",
        "since = \"1.0\"\n\n",
        "[[pairs]]\nid = \"space-pair\"\nname = \"Space pair\"\nrelation = \"identity\"\n",
        "a = \"trailing-space\"\nb = \"trailing-space\"\n",
        "breaks = \"Two spellings that a person cannot tell apart in any list.\"\nsince = \"1.0\"\n"
    );

    #[test]
    fn the_pack_a_translation_names_is_read_only_when_it_could_be_a_pack() {
        // The name leaves this port or it does not, and nothing in between. What
        // makes this a refusal rather than a cleaning step is that the identifier
        // alphabet has no dot and no separator in it.
        assert_eq!(
            TomlPackFormat.translated_pack(A_TRANSLATION),
            TranslationTarget::Pack("whitespace".to_owned())
        );
        assert_eq!(
            TomlPackFormat.translated_pack(A_PACK_WITH_TWO_ENTRIES),
            TranslationTarget::NotATranslation
        );

        for hostile in [
            "../../../etc/passwd",
            "..",
            "/etc/passwd",
            "whitespace.pl",
            "C:/Windows/win.ini",
            "",
        ] {
            let text = format!("format = 1\ntranslates = \"{hostile}\"\nlanguage = \"pl\"\n");
            assert_eq!(
                TomlPackFormat.translated_pack(&text),
                TranslationTarget::Unusable,
                "{hostile} must never become a path"
            );
        }
    }

    #[test]
    fn a_translates_that_names_no_pack_is_reported_rather_than_passed_along() {
        // E051 has a half that needs no second file, and this is it. Reported here
        // so that the reader is told why nothing was looked for, instead of seeing
        // a rule quietly do nothing.
        let text = "format = 1\ntranslates = \"../secrets\"\nlanguage = \"pl\"\n";
        let found = one_of(text, RuleCode::TranslatesUnknownPack);
        assert_eq!(found.subject.as_deref(), Some("../secrets"));
        assert_eq!(found.line, Some(2), "the line the field is written on");
    }

    #[test]
    fn a_translates_of_the_wrong_kind_is_the_field_kind_rule_and_not_two_rules() {
        // E009 runs before the rules that read a field, so a `translates` that is
        // not text is refused by the rule that owns kinds. Reporting E051 as well
        // would be two codes for one mistake, and the second would be wrong: the
        // file does not name a pack that is missing, it names nothing at all.
        let text = "format = 1\ntranslates = 123\nlanguage = \"pl\"\n";
        let reported = codes(text);
        assert_eq!(reported, vec!["E009"], "{reported:?}");
    }

    #[test]
    fn the_insertion_fields_are_refused_in_a_translated_pair_as_well_as_a_value() {
        // 🔴 The fourth door. Until 2026-09-07 the walk covered `values` and not
        // `pairs`, so both fields below passed in silence - measured, not
        // suspected. Narrowing the walk back to one table has to fail here.
        //
        // The file suite cannot see this: it compares a set of codes, and E050
        // fires from the `values` table of another file either way.
        let text = concat!(
            "format = 1\ntranslates = \"whitespace\"\nlanguage = \"pl\"\n\n",
            "[pairs.space-pair]\nname = \"Para spacji\"\n",
            "breaks = \"Dwa zapisy, ktorych nikt nie odroznia na liscie.\"\n",
            "value = \"podstawiona wartosc\"\ncount = 9\n"
        );
        let reported = codes(text);
        assert_eq!(reported, vec!["E050", "E050"], "{reported:?}");

        let found = one_of(text, RuleCode::TranslationCarriesValue);
        assert_eq!(found.owner.as_deref(), Some("space-pair"));
    }

    #[test]
    fn an_insertion_field_is_refused_in_a_translation_written_in_the_other_shape() {
        // A translation writes `[values.<id>]`, so `[[values]]` is a shape the
        // format does not define for one - which is exactly why the safety rule
        // has to look there too. A rule that only checks where a field is supposed
        // to be is a rule anybody can step around by moving the field.
        let text = concat!(
            "format = 1\ntranslates = \"whitespace\"\nlanguage = \"pl\"\n\n",
            "[[values]]\nid = \"trailing-space\"\nname = \"Spacja\"\n",
            "value = \"podstawiona wartosc\"\n"
        );
        let found = one_of(text, RuleCode::TranslationCarriesValue);
        assert_eq!(found.subject.as_deref(), Some("value"));
        assert_eq!(found.owner.as_deref(), Some("trailing-space"));
    }

    #[test]
    fn a_translation_is_compared_against_the_shape_a_source_pack_actually_uses() {
        // The two files are written differently and the comparison has to know it.
        // Reading the pack in the translation's own shape would find no
        // identifiers at all and report every entry as vanished - a check that is
        // wrong in the direction nobody questions, because it only ever fires.
        let text = concat!(
            "format = 1\ntranslates = \"whitespace\"\nlanguage = \"pl\"\n\n",
            "[values.trailing-space]\nname = \"Spacja na koncu\"\n",
            "breaks = \"Sortowanie stawia rekord na poczatku listy.\"\n\n",
            "[pairs.space-pair]\nname = \"Para spacji\"\n",
            "breaks = \"Dwa zapisy, ktorych nikt nie odroznia.\"\n\n",
            "[values.gone-last-year]\nname = \"Wycofana\"\n",
            "breaks = \"Wpis, ktorego paczka juz nie ma.\"\n"
        );

        let TranslationCheck::Compared(problems) =
            TomlPackFormat.check_translation(text, A_PACK_WITH_TWO_ENTRIES)
        else {
            panic!("the pack parses, so the two must be compared");
        };

        let reported: Vec<&str> = problems
            .iter()
            .map(|p| p.subject.as_deref().unwrap_or(""))
            .collect();
        assert_eq!(
            reported,
            vec!["gone-last-year"],
            "the value and the pair that exist must both be recognised"
        );
        assert_eq!(problems[0].code, RuleCode::TranslationRefersToMissingId);
    }

    #[test]
    fn a_pack_that_does_not_parse_is_an_impossible_comparison_and_not_an_empty_one() {
        // The pair of answers this whole mechanism turns on. An empty list would
        // say the translation was checked and is fine, when nothing was read.
        assert_eq!(
            TomlPackFormat.check_translation(A_TRANSLATION, "format = = 1\n"),
            TranslationCheck::TranslatedPackDidNotParse
        );

        let TranslationCheck::Compared(problems) =
            TomlPackFormat.check_translation(A_TRANSLATION, A_PACK_WITH_TWO_ENTRIES)
        else {
            panic!("a pack that parses is compared");
        };
        assert!(problems.is_empty(), "{problems:?}");
    }
}
