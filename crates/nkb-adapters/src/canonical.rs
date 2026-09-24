//! The one shape a pack file is written in, and the check that it is in it.
//!
//! # Why a format edited by hand needs a formatter
//!
//! A pack file is written by a person and rewritten by a tool, and a format in
//! that position needs a single canonical shape or every machine write produces
//! a whole-file difference and people stop committing it. Review of a pull
//! request is then about field order and the column an equals sign sits in,
//! rather than about the value and the sentence beside it - which is the only
//! part worth a reviewer's attention.
//!
//! # What it changes, and the much longer list of what it does not
//!
//! Two things: the order of the fields inside a table, and the width of the gap
//! before each equals sign. Nothing else. Comments stay, blank lines stay, and
//! the written form of every value stays byte for byte - measured on the shipped
//! pack, where seventeen escape sequences went in and seventeen came out.
//!
//! 🔴 **The order of `[[values]]` is never touched.** In this format the order of
//! values in the file is the order a tester meets them, so sorting them would be
//! a change to what the product does rather than a change to how the file looks.
//!
//! # The one that has to be said out loud
//!
//! A key that introduces a **table** is left completely alone, decor included.
//! Measured: writing a decor suffix onto such a key renders it inside the header,
//! turning `[pack]` into `[pack ]` - a different table, not different whitespace.

use toml_edit::{Decor, DocumentMut, Item, Key, Table};

/// Order of the keys at the top of a file.
///
/// A source pack declares the first alone. A translation declares all three.
const ROOT_ORDER: [&str; 3] = ["format", "translates", "language"];

/// Order of the keys in `[pack]`, from the data model in `pack-format.md` 4.
const PACK_ORDER: [&str; 12] = [
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

/// Order of the keys in a value, from `pack-format.md` 11.
///
/// The specification lists fifteen and the format defines sixteen: `source` is
/// missing there. It goes here beside `fields` and `risk` because those three
/// are exactly the fields a value inherits from its pack, and because the
/// `[pack]` table lists it in the same block. Recorded rather than slipped in.
const VALUE_ORDER: [&str; 16] = [
    "id",
    "name",
    "type",
    "value",
    "unit",
    "count",
    "breaks",
    "expect",
    "fields",
    "tags",
    "risk",
    "source",
    "since",
    "shape",
    "deprecated",
    "replaced_by",
];

/// Order of the keys in a pair, from the example in `pack-format.md` 9.
const PAIR_ORDER: [&str; 8] = [
    "id", "name", "relation", "a", "b", "breaks", "expect", "since",
];

/// The fields that decide what the tool sends into somebody else's application.
///
/// Compared before and after formatting, by the caller that writes the result to
/// a disk. A formatter is the one piece of this tool that rewrites a curated
/// file, and the pack it would damage first is the one made of characters nobody
/// can see - so the check is a refusal to write rather than a note in a log.
const INSERTION_FIELDS: [&str; 5] = ["id", "value", "unit", "count", "type"];

/// The canonical text of a pack file, or `None` when it is not TOML at all.
///
/// `None` is not a failure to report: a file that does not parse already has
/// E004 against it, and there is nothing inside to put in order.
#[must_use]
pub fn render(text: &str) -> Option<String> {
    let mut doc: DocumentMut = text.parse().ok()?;
    order_document(&mut doc);
    Some(doc.to_string())
}

/// Whether the file is already written the way this module would write it.
///
/// A file that does not parse is not judged: the answer is `true`, because
/// saying "and it is badly formatted" about a file nobody could read would be
/// a second finding invented out of the first one.
#[must_use]
pub fn is_canonical(text: &str) -> bool {
    render(text).is_none_or(|canonical| canonical == text)
}

/// Whether two versions of a file describe the same insertions.
///
/// The guard on the write path, and deliberately blunt: it compares both the
/// written form of every deciding field and the text that form parses to. The
/// first catches an escape that turned into the character it stood for, the
/// second catches a value that changed without its spelling changing.
#[must_use]
pub fn same_insertions(before: &str, after: &str) -> bool {
    match (before.parse::<DocumentMut>(), after.parse::<DocumentMut>()) {
        (Ok(before), Ok(after)) => insertions(&before) == insertions(&after),
        // Formatting produced something that will not parse. Whatever else is
        // true, that is not the same file.
        _ => false,
    }
}

/// Every deciding field of every entry, in a form two documents can be compared
/// in. Position is part of the key, so a reordered file is not a matching one.
fn insertions(doc: &DocumentMut) -> Vec<String> {
    let mut found = Vec::new();

    for collection in ["values", "pairs"] {
        let Some(item) = doc.get(collection) else {
            continue;
        };
        if let Some(array) = item.as_array_of_tables() {
            for (index, table) in array.iter().enumerate() {
                collect_insertions(&format!("{collection}[{index}]"), table, &mut found);
            }
        } else if let Some(table) = item.as_table() {
            for (name, entry) in table.iter() {
                if let Some(entry) = entry.as_table() {
                    collect_insertions(&format!("{collection}.{name}"), entry, &mut found);
                }
            }
        }
    }
    found.sort();
    found
}

fn collect_insertions(owner: &str, table: &Table, found: &mut Vec<String>) {
    for field in INSERTION_FIELDS {
        let Some(value) = table.get(field).and_then(Item::as_value) else {
            continue;
        };
        // The written form, decor stripped so that alignment is not mistaken for
        // a change of content, and then what it parses to.
        found.push(format!("{owner}/{field}/raw={}", value.to_string().trim()));
        if let Some(text) = value.as_str() {
            found.push(format!("{owner}/{field}/text={text}"));
        }
    }
}

/// Puts every table of the document into canonical shape.
fn order_document(doc: &mut DocumentMut) {
    order_table(doc.as_table_mut(), &ROOT_ORDER);

    if let Some(pack) = doc.get_mut("pack").and_then(Item::as_table_mut) {
        order_table(pack, &PACK_ORDER);
    }

    for (collection, order) in [
        ("values", VALUE_ORDER.as_slice()),
        ("pairs", PAIR_ORDER.as_slice()),
    ] {
        let Some(item) = doc.get_mut(collection) else {
            continue;
        };
        if let Some(array) = item.as_array_of_tables_mut() {
            for table in array.iter_mut() {
                order_table(table, order);
            }
        } else if let Some(table) = item.as_table_mut() {
            // The keyed shape, which is what a translation uses. The table above
            // holds no fields of its own, so ordering it is a no-op that costs
            // nothing and saves a branch nobody would remember to add.
            let names: Vec<String> = table.iter().map(|(name, _)| name.to_owned()).collect();
            for name in names {
                if let Some(entry) = table.get_mut(&name).and_then(Item::as_table_mut) {
                    order_table(entry, order);
                }
            }
        }
    }
}

/// Reorders and realigns the fields of one table, leaving everything else alone.
fn order_table(table: &mut Table, order: &[&str]) {
    // Only fields line up. A key introducing a sub table renders as a header and
    // has no equals sign, and its decor belongs to the header rather than to a
    // gap - see the note at the top of this module about `[pack ]`.
    let width = table
        .iter()
        .filter(|(_, item)| item.is_value())
        .filter_map(|(key, _)| table.key(key))
        .map(written_width)
        .max()
        .unwrap_or(0);

    let mut wanted: Vec<String> = Vec::new();
    for name in order {
        if table.get(name).is_some_and(Item::is_value) {
            wanted.push((*name).to_owned());
        }
    }
    // A field the format does not define keeps its relative place, after the
    // ones that are named. It is already reported as an unknown key. Moving it
    // somewhere surprising on top of that would help nobody.
    for (key, item) in table.iter() {
        if item.is_value() && !order.contains(&key) {
            wanted.push(key.to_owned());
        }
    }

    let mut taken: Vec<(Key, Item)> = Vec::new();
    for name in &wanted {
        if let Some(entry) = table.remove_entry(name) {
            taken.push(entry);
        }
    }

    for (key, item) in taken {
        // The prefix carries the blank lines and the comments a person wrote
        // above this field, so it is copied across untouched. Only the gap
        // before the equals sign is ours to decide.
        let prefix = key
            .leaf_decor()
            .prefix()
            .and_then(|raw| raw.as_str())
            .unwrap_or("")
            .to_owned();
        let gap = " ".repeat(width.saturating_sub(written_width(&key)) + 1);
        let key = key.with_leaf_decor(Decor::new(prefix, gap));
        table.insert_formatted(&key, item);
    }
}

/// How many columns a key takes up in the file.
///
/// The written form, not the name it stands for. Measured on a quoted key
/// carrying one escape sequence: fifteen columns in the file, eight characters
/// in the name it stands for. Lining up on the name would put the equals signs
/// of such a file in three different places.
///
/// Every key this format defines is a bare word, where the two numbers agree -
/// which is exactly why the difference would have gone unnoticed until somebody
/// else's pack did something unusual.
fn written_width(key: &Key) -> usize {
    key.display_repr().chars().count()
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;

    /// A pack with one value, written the way the tool writes it.
    const CANONICAL: &str = "\
format = 1

[pack]
id          = \"probe\"
name        = \"Probe\"
description = \"A pack used by the tests of the formatter.\"
version     = \"1.0\"
updated     = 2026-09-07
license     = \"CC-BY-4.0\"
authors     = [\"Naughty Keyboard\"]
language    = \"en\"
tags        = [\"probe\"]

[[values]]
id     = \"one\"
name   = \"One\"
value  = \"x\"
breaks = \"A description long enough to satisfy the sentence rule in the format.\"
expect = \"Stored and returned unchanged, the way any other value would be.\"
since  = \"1.0\"
";

    #[test]
    fn a_file_already_in_shape_comes_back_byte_for_byte() {
        assert_eq!(render(CANONICAL).as_deref(), Some(CANONICAL));
        assert!(is_canonical(CANONICAL));
    }

    #[test]
    fn fields_out_of_order_are_put_in_order_and_lined_up() {
        let scrambled = "\
format = 1

[[values]]
since = \"1.0\"
breaks = \"long enough to be a sentence about what this value breaks and why\"
id = \"one\"
name = \"One\"
";
        let out = render(scrambled).expect("parses");
        let keys: Vec<&str> = out
            .lines()
            .filter_map(|line| line.split('=').next())
            .map(str::trim)
            .filter(|word| ["id", "name", "breaks", "since"].contains(word))
            .collect();
        assert_eq!(keys, ["id", "name", "breaks", "since"], "{out}");
        assert!(out.contains("id     = "), "{out}");
        assert!(out.contains("breaks = "), "{out}");
    }

    #[test]
    fn the_order_of_values_is_never_touched() {
        // The order of values in a file is the order a tester meets them, so
        // sorting them would change what the product does rather than how the
        // file looks. This is the one thing a formatter must not tidy.
        let two = "\
format = 1

[[values]]
id = \"zebra\"

[[values]]
id = \"aardvark\"
";
        let out = render(two).expect("parses");
        let zebra = out.find("zebra").expect("zebra survives");
        let aardvark = out.find("aardvark").expect("aardvark survives");
        assert!(zebra < aardvark, "{out}");
    }

    #[test]
    fn a_comment_travels_with_the_field_it_belongs_to() {
        let text = "\
format = 1

[[values]]
breaks = \"the description this comment is about\"
# This comment belongs to breaks.
id = \"one\"
";
        let out = render(text).expect("parses");
        let comment = out.find("# This comment").expect("the comment survives");
        let breaks = out.find("breaks").expect("breaks survives");
        assert!(
            comment < breaks,
            "the comment must still introduce it: {out}"
        );
    }

    #[test]
    fn blank_lines_and_comments_are_not_thrown_away() {
        let text = "\
# A comment at the top of the file.
format = 1


# Two blank lines above this table.
[[values]]
name = \"One\"

id = \"one\"
";
        let out = render(text).expect("parses");
        assert_eq!(out.matches('#').count(), 2, "{out}");
        assert_eq!(
            out.lines().filter(|line| line.trim().is_empty()).count(),
            text.lines().filter(|line| line.trim().is_empty()).count(),
            "{out}"
        );
    }

    #[test]
    fn a_key_that_introduces_a_table_keeps_its_header_intact() {
        // The measured trap: a decor suffix on such a key renders inside the
        // brackets, so `[pack]` silently becomes the table `[pack ]`.
        let out = render(CANONICAL).expect("parses");
        assert!(out.contains("[pack]"), "{out}");
        assert!(!out.contains("[pack "), "{out}");
        assert!(out.contains("[[values]]"), "{out}");
        assert!(!out.contains("[[values "), "{out}");
    }

    #[test]
    fn a_quoted_key_lines_up_by_what_it_looks_like_and_not_by_what_it_means() {
        // The format defines only bare keys, where the two are the same number,
        // so this only shows up on a file somebody else wrote. Built from a
        // backslash rather than written out: an escape typed into source has a
        // habit of arriving as the character it stands for.
        let slash = char::from(92u8);
        let text = format!("\"a{slash}u0041b\" = 1\nplain = 2\n");
        let out = render(&text).expect("parses");

        let columns: Vec<usize> = out.lines().filter_map(|line| line.find('=')).collect();
        assert_eq!(columns.len(), 2, "{out}");
        assert_eq!(
            columns[0], columns[1],
            "the equals signs do not line up:\n{out}"
        );
    }

    #[test]
    fn formatting_twice_is_formatting_once() {
        // The property the rule rests on: without a fixed point, "this file is
        // not canonical" names no shape to reach.
        let messy = "\
format=1
[[values]]
since=\"1.0\"
id=\"one\"
";
        let once = render(messy).expect("parses");
        let twice = render(&once).expect("parses again");
        assert_eq!(once, twice);
    }

    #[test]
    fn a_file_that_is_not_toml_produces_nothing_and_is_not_called_badly_formatted() {
        assert_eq!(render("format = ="), None);
        assert!(is_canonical("format = ="));
    }

    #[test]
    fn an_unknown_field_is_kept_rather_than_dropped() {
        let text = "\
format = 1

[[values]]
tgas = \"a typo for tags\"
id = \"one\"
";
        let out = render(text).expect("parses");
        assert!(out.contains("tgas"), "{out}");
        assert!(out.contains("id"), "{out}");
    }

    #[test]
    fn the_escapes_in_a_value_survive_being_moved() {
        // The whole reason the write path has a guard. The catalogue's first
        // pack is made of characters nobody can see, and an escape lost here
        // leaves a value that still parses and no longer tests anything.
        let text = "\
format = 1

[[values]]
since = \"1.0\"
value = \"Kowalski\\u0020\"
id = \"trailing-space\"
";
        let out = render(text).expect("parses");
        assert!(out.contains("Kowalski\\u0020"), "{out}");
        assert!(!out.contains("Kowalski \""), "the escape expanded: {out}");
        assert!(same_insertions(text, &out));
    }

    #[test]
    fn the_write_guard_can_actually_fail() {
        // A check nobody has seen fail is indistinguishable from a broken one.
        let before = "\
format = 1

[[values]]
id = \"one\"
value = \"a\"
";
        let changed = "\
format = 1

[[values]]
id = \"one\"
value = \"b\"
";
        assert!(same_insertions(before, before));
        assert!(!same_insertions(before, changed));

        // The failure it exists for: the same text, spelled without the escape.
        let escaped = "format = 1\n\n[[values]]\nvalue = \"a\\u0020\"\n";
        let expanded = "format = 1\n\n[[values]]\nvalue = \"a \"\n";
        assert!(!same_insertions(escaped, expanded));

        // And what it must not call a change: a different gap before the equals,
        // which is the one thing the formatter is actually allowed to rewrite.
        let same_but_aligned = "format = 1\n\n[[values]]\nid    = \"one\"\nvalue = \"a\"\n";
        assert!(same_insertions(before, same_but_aligned));

        // Nor a field that merely moved: where a field sits inside its entry is
        // not part of what gets inserted anywhere.
        let reordered = "format = 1\n\n[[values]]\nvalue = \"a\"\nid = \"one\"\n";
        assert!(same_insertions(before, reordered));
    }

    #[test]
    fn a_translation_is_put_in_shape_too() {
        let text = "\
language = \"pl\"
format = 1
translates = \"probe\"

[values.one]
breaks = \"opis wpisu\"
name = \"Jeden\"
";
        let out = render(text).expect("parses");
        assert!(out.contains("format     = 1"), "{out}");
        assert!(out.contains("translates = "), "{out}");
        assert!(out.contains("language   = "), "{out}");
        let name = out.find("name").expect("name survives");
        let breaks = out.find("breaks").expect("breaks survives");
        assert!(name < breaks, "{out}");
    }

    #[test]
    fn every_field_the_format_defines_has_a_place_in_the_order() {
        // The guard that makes a new field a decision rather than an omission:
        // the order and the vocabulary have to be the same set. A field with no
        // place in the order would drift to the end of the block in silence.
        use crate::toml_pack::{PACK_KEYS, PAIR_KEYS, VALUE_KEYS};

        for (order, vocabulary, what) in [
            (VALUE_ORDER.as_slice(), VALUE_KEYS.as_slice(), "a value"),
            (PACK_ORDER.as_slice(), PACK_KEYS.as_slice(), "the pack"),
            (PAIR_ORDER.as_slice(), PAIR_KEYS.as_slice(), "a pair"),
        ] {
            let mut ordered: Vec<&str> = order.to_vec();
            let mut known: Vec<&str> = vocabulary.to_vec();
            ordered.sort_unstable();
            known.sort_unstable();
            assert_eq!(
                ordered, known,
                "the canonical order of {what} is not the set of fields it may hold"
            );
        }
    }
}
