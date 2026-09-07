//! Turning a pack file into the pack it describes.
//!
//! # The other direction, and why it is a separate file
//!
//! `toml_pack.rs` reads a pack file in order to find fault with it. Everything
//! there is suspicious by design: it looks at the raw slice a span points at
//! rather than the parsed value, it walks both table shapes so that a field
//! cannot be smuggled past a safety rule, and it never builds anything.
//!
//! This reads the same file in order to obey it. The two habits do not mix, and
//! keeping them in one file would mean every future reader has to work out which
//! mode a given function is in.
//!
//! # This one follows the format exactly
//!
//! Deliberately unlike `entries_of` next door, which covers both the keyed and
//! the array shape because E050 is a safety property and a safety rule that only
//! looks where a field is allowed to be is a rule anybody can step around.
//!
//! Here the question is what the file MEANS, and the format says `[[values]]`
//! and `[[pairs]]` are arrays of tables. A parser that also accepted some other
//! spelling would be inventing a second format that no validator checks and no
//! specification describes.
//!
//! # Nothing here forgives anything
//!
//! A field of the wrong kind, a missing required field, a bad risk word: none of
//! it is repaired here and none of it is reported here either. The caller runs
//! `check` first and refuses the pack on any error - `pack-format.md` 11 loads
//! all of a pack or none of it. What this does when a field is absent is leave
//! the gap visible, because an invented sentence in a field a tester reads is
//! worse than a missing one.

use nkb_core::pack::{Pack, PackPair, PackValue, Risk};
use nkb_core::text::LiteralText;
use nkb_core::value::ValueBody;
use toml_edit::{Document, Item, Table};

/// Reads one string field, or nothing when it is absent or not a string.
fn text(table: &Table, key: &str) -> Option<String> {
    table.get(key).and_then(Item::as_str).map(ToOwned::to_owned)
}

/// The same, flattened to an empty string. Only for fields the format requires,
/// where absence is already an error somebody else reported.
fn text_or_empty(table: &Table, key: &str) -> String {
    text(table, key).unwrap_or_default()
}

/// Reads a field the format stores as a NATIVE DATE, not as a string.
///
/// 🔴 Its own reader because `updated` is a TOML date - `updated = 2026-09-07`,
/// unquoted - and asking a date for its string gives nothing. Measured the hard
/// way: the first run of `nkb show` printed "updated ," for every pack, with no
/// error anywhere, because a helper that turns absence into an empty string
/// cannot tell absence from a field it does not know how to read.
fn date(table: &Table, key: &str) -> Option<String> {
    let item = table.get(key)?;
    if let Some(stamp) = item.as_datetime() {
        return Some(stamp.to_string());
    }
    // A date written as a string is E009's finding rather than this one's, and
    // showing what the file says beats showing nothing.
    item.as_str().map(ToOwned::to_owned)
}

/// Reads an array of strings, skipping entries that are not strings.
///
/// Skipping rather than failing, because a non-string in a list is `E009`'s
/// finding and it has already been made. Returning nothing at all here would
/// turn one reported mistake into a second, silent one.
fn words(table: &Table, key: &str) -> Vec<String> {
    table
        .get(key)
        .and_then(Item::as_array)
        .map(|array| {
            array
                .iter()
                .filter_map(|entry| entry.as_str().map(ToOwned::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// The recipe a value describes.
///
/// `None` when the file says nothing usable - no `value`, or a `repeat` missing
/// a part. The caller has already refused such a pack, so this is the branch
/// that must not guess rather than the branch that must recover.
fn body_of(table: &Table) -> Option<ValueBody> {
    let declared = table.get("type").and_then(Item::as_str);

    if declared == Some("repeat") {
        let unit = table.get("unit").and_then(Item::as_str)?;
        let count = table.get("count").and_then(Item::as_integer)?;
        // A count outside machine range is E024's finding and the pack carrying
        // it never reaches here. Refusing to guess is still the right answer.
        let count = u32::try_from(count).ok()?;
        return Some(ValueBody::Repeat {
            unit: LiteralText::new(unit),
            count,
        });
    }

    // The format's default type is `literal`, so an absent `type` is one.
    let value = table.get("value").and_then(Item::as_str)?;
    Some(ValueBody::Literal(LiteralText::new(value)))
}

fn value_of(table: &Table) -> Option<PackValue> {
    Some(PackValue {
        id: text(table, "id")?,
        name: text_or_empty(table, "name"),
        body: body_of(table)?,
        breaks: text(table, "breaks"),
        expect: text(table, "expect"),
        // `None` and `Some(Normal)` are different facts and stay different:
        // W063 asks whether the value said anything at all.
        risk: text(table, "risk").as_deref().and_then(Risk::parse),
        fields: words(table, "fields"),
        tags: words(table, "tags"),
        deprecated: table
            .get("deprecated")
            .and_then(Item::as_bool)
            .unwrap_or(false),
        replaced_by: text(table, "replaced_by"),
    })
}

fn pair_of(table: &Table) -> Option<PackPair> {
    Some(PackPair {
        id: text(table, "id")?,
        name: text_or_empty(table, "name"),
        relation: text_or_empty(table, "relation"),
        a: text(table, "a")?,
        b: text(table, "b")?,
        breaks: text(table, "breaks"),
        expect: text(table, "expect"),
    })
}

/// Builds the pack a file describes, or nothing when it describes no pack.
///
/// `None` covers exactly two cases: text that is not TOML at all, and TOML with
/// no `[pack]` table. Both are `E004` or `E003` and have already been said.
pub(crate) fn parse(text_of_file: &str) -> Option<Pack> {
    let document: Document<String> = text_of_file.parse().ok()?;
    let root = document.as_table();
    let header = root.get("pack").and_then(Item::as_table)?;

    let values = root
        .get("values")
        .and_then(Item::as_array_of_tables)
        .map(|tables| tables.iter().filter_map(value_of).collect())
        .unwrap_or_default();

    let pairs = root
        .get("pairs")
        .and_then(Item::as_array_of_tables)
        .map(|tables| tables.iter().filter_map(pair_of).collect())
        .unwrap_or_default();

    Some(Pack {
        id: text_or_empty(header, "id"),
        name: text_or_empty(header, "name"),
        description: text_or_empty(header, "description"),
        version: text_or_empty(header, "version"),
        updated: date(header, "updated").unwrap_or_default(),
        license: text_or_empty(header, "license"),
        authors: words(header, "authors"),
        language: text_or_empty(header, "language"),
        risk: text(header, "risk")
            .as_deref()
            .and_then(Risk::parse)
            .unwrap_or_default(),
        tags: words(header, "tags"),
        fields: words(header, "fields"),
        values,
        pairs,
    })
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;

    /// The one pack the tool actually ships, read from the repository rather
    /// than retyped here. A fixture that is a copy of the real file stops being
    /// a copy of it within a month.
    fn shipped_whitespace() -> String {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packs/whitespace.toml");
        std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{} must be readable: {e}", path.display()))
    }

    #[test]
    fn the_shipped_pack_reads_back_with_every_value_it_declares() {
        let pack = parse(&shipped_whitespace()).expect("the shipped pack must parse");

        assert_eq!(pack.id, "whitespace");
        assert_eq!(pack.version, "1.0");
        // 🔴 Twelve is the number the catalogue declares for this pack, and the
        // reason this assertion is a count rather than a spot check: OBS-55
        // recorded a run where this file lost an escape, two values became
        // byte-identical and the pack shipped eleven while claiming twelve.
        assert_eq!(
            pack.values.len(),
            12,
            "the shipped whitespace pack declares twelve values"
        );
        assert!(pack.values.iter().all(|value| !value.id.is_empty()));
    }

    #[test]
    fn the_date_field_is_read_even_though_the_format_stores_it_as_a_date() {
        // The defect this test was written for: `updated` is an unquoted TOML
        // date, so reading it as a string yields nothing and the report printed
        // an empty gap with no error beside it.
        let pack = parse(&shipped_whitespace()).expect("the shipped pack must parse");
        assert!(
            !pack.updated.is_empty(),
            "the pack declares an `updated` date and it did not survive reading"
        );
        assert!(
            pack.updated.starts_with("2026-"),
            "expected a date, got {:?}",
            pack.updated
        );
    }

    #[test]
    fn an_invisible_value_arrives_expanded_rather_than_as_its_escape() {
        // The single most important property of this direction. The file stores
        // a zero width space as six visible characters, backslash included; what a
        // field must receive is one invisible character. Getting this backwards
        // is the exact silent falsehood
        // the tool exists to find in other people's software.
        let pack = parse(&shipped_whitespace()).expect("the shipped pack must parse");
        let value = pack.value("zwsp-only").expect("zwsp-only is in this pack");

        let ValueBody::Literal(literal) = &value.body else {
            panic!("zwsp-only is a literal value");
        };
        assert_eq!(literal.as_str().chars().count(), 1);
        assert_eq!(literal.as_str(), "\u{200B}");
        assert!(!literal.as_str().contains('u'));
    }

    #[test]
    fn a_generated_value_stays_a_recipe_and_is_never_expanded_here() {
        let pack = parse(&shipped_whitespace()).expect("the shipped pack must parse");
        let value = pack
            .value("len-1000-spaces")
            .expect("len-1000-spaces is in this pack");

        let ValueBody::Repeat { unit, count } = &value.body else {
            panic!("len-1000-spaces is a generated value");
        };
        assert_eq!(unit.as_str(), " ");
        assert_eq!(*count, 1000);
        // architektura.md 6.1: the size is known from the recipe, and nothing
        // here builds the thousand characters in order to find it out.
        let metrics = value.body.metrics().expect("a measurable recipe");
        assert_eq!(metrics.code_points, 1000);
    }

    #[test]
    fn text_that_is_not_a_pack_file_yields_nothing() {
        assert!(parse("this is not toml at all {{{").is_none());
        assert!(
            parse("format = 1\n").is_none(),
            "TOML with no [pack] table describes no pack"
        );
    }

    #[test]
    fn a_pair_is_read_with_both_halves() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/packs/accepted/look-alikes.toml");
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{} must be readable: {e}", path.display()));

        let pack = parse(&text).expect("the look-alike pack must parse");
        let pair = pack.pairs.first().expect("that pack carries a pair");
        assert!(!pair.a.is_empty() && !pair.b.is_empty());
        assert!(
            pack.value(&pair.a).is_some(),
            "a pair names values from its own pack - D34"
        );
        assert!(pack.value(&pair.b).is_some());
    }

    #[test]
    fn reading_the_same_file_twice_gives_the_same_pack() {
        // The listing goes into other people's scripts, so a parse that varied
        // between runs would turn a diff into noise.
        let text = shipped_whitespace();
        assert_eq!(parse(&text), parse(&text));
    }
}
