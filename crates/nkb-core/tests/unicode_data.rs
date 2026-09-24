//! The generated tables, judged against the files they were generated from.
//!
//! # The question this closes
//!
//! `src/graphemes/table.rs` is forty-seven kilobytes of hexadecimal that nobody
//! will ever read. It was produced once, by a script that is not part of this
//! repository, and a table nobody can check is a table that drifts the first
//! time somebody edits it "just to fix one character".
//!
//! So this file rebuilds the whole thing from the vendored Unicode files and
//! compares every code point in the standard - all 1 114 112 of them, not a
//! sample. If the two ever disagree, the failure names the code point.
//!
//! `src/normalization/table.rs` is judged the same way, further down, and goes
//! one step further: its generator IS the test. Every run writes the table the
//! file implies to `target/tmp/unicode/`, so an upgrade of the standard does not
//! depend on a script somebody kept.
//!
//! # Why it reads the table as TEXT rather than calling into the crate
//!
//! The lookup and the packing are private, and they should stay private: they
//! are an implementation detail of one module, not a surface. Opening them up
//! so that a test could reach them would trade a real property of the design
//! for a convenience. The table is data in a generated shape, so the test reads
//! it as data - which also means this check keeps working if the lookup is ever
//! rewritten.
//!
//! # What this does not check
//!
//! That the RULES are right. A perfect table with rule GB9c missing would pass
//! here and fail in `grapheme_conformance.rs`, which is the other half and runs
//! beside this one.

// A failed expectation in a test is a failed test. The workspace denies both in
// shipped code, which is the setting that matters.
#![allow(clippy::panic, clippy::expect_used)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The last code point in the standard.
const MAX_CODE_POINT: u32 = 0x10_FFFF;

/// The `Grapheme_Cluster_Break` names, in the order `graphemes.rs` numbers them.
///
/// 🔴 This list IS the contract between the table and the code that unpacks it.
/// Reordering it in one place and not the other is the kind of change that
/// compiles, runs, and quietly mislabels a fifth of the standard.
const BREAK_NAMES: [&str; 14] = [
    "Other",
    "CR",
    "LF",
    "Control",
    "Extend",
    "ZWJ",
    "Regional_Indicator",
    "Prepend",
    "SpacingMark",
    "L",
    "V",
    "T",
    "LV",
    "LVT",
];

/// The `Indic_Conjunct_Break` names, in the same kind of contract.
const CONJUNCT_NAMES: [&str; 4] = ["None", "Extend", "Linker", "Consonant"];

const PICTOGRAPHIC_BIT: u8 = 0x10;

fn unicode_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("unicode")
}

fn read(name: &str) -> String {
    let path = unicode_dir().join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("the vendored file must be readable at {path:?}: {e}"))
}

/// One line of a Unicode data file, split into its range and its fields.
fn data_lines(text: &str) -> impl Iterator<Item = (u32, u32, Vec<&str>)> {
    text.lines().filter_map(|line| {
        let body = line.split('#').next().unwrap_or("").trim();
        if body.is_empty() {
            return None;
        }
        let mut fields = body.split(';').map(str::trim);
        let range = fields.next()?;
        let rest: Vec<&str> = fields.collect();
        let (low, high) = match range.split_once("..") {
            Some((low, high)) => (low, high),
            None => (range, range),
        };
        let low = u32::from_str_radix(low, 16).ok()?;
        let high = u32::from_str_radix(high, 16).ok()?;
        Some((low, high, rest))
    })
}

fn index_of(names: &[&str], wanted: &str, file: &str) -> u8 {
    let position = names
        .iter()
        .position(|name| *name == wanted)
        .unwrap_or_else(|| {
            panic!(
                "{file} names a property value this table has no number for: {wanted}. \
                 A new value in the standard is a real change, not a typo - the enum in \
                 graphemes.rs and the list in this test both have to learn it."
            )
        });
    u8::try_from(position).unwrap_or_else(|e| panic!("property index out of range: {e}"))
}

/// Rebuilds the packed byte for every code point, straight from the files.
fn expected_table() -> Vec<u8> {
    let size = usize::try_from(MAX_CODE_POINT).unwrap_or_else(|e| panic!("{e}")) + 1;
    let mut packed = vec![0u8; size];

    let mut set = |low: u32, high: u32, apply: &dyn Fn(u8) -> u8| {
        for code in low..=high.min(MAX_CODE_POINT) {
            let index = usize::try_from(code).unwrap_or_else(|e| panic!("{e}"));
            if let Some(slot) = packed.get_mut(index) {
                *slot = apply(*slot);
            }
        }
    };

    let breaks = read("GraphemeBreakProperty.txt");
    for (low, high, fields) in data_lines(&breaks) {
        let Some(value) = fields.first() else {
            continue;
        };
        let number = index_of(&BREAK_NAMES, value, "GraphemeBreakProperty.txt");
        set(low, high, &|old| (old & !0x0F) | number);
    }

    let emoji = read("emoji-data.txt");
    for (low, high, fields) in data_lines(&emoji) {
        if fields.first() != Some(&"Extended_Pictographic") {
            continue;
        }
        set(low, high, &|old| old | PICTOGRAPHIC_BIT);
    }

    let derived = read("DerivedCoreProperties.txt");
    for (low, high, fields) in data_lines(&derived) {
        if fields.first() != Some(&"InCB") {
            continue;
        }
        let Some(value) = fields.get(1) else { continue };
        let number = index_of(&CONJUNCT_NAMES, value, "DerivedCoreProperties.txt");
        set(low, high, &|old| (old & !0x60) | (number << 5));
    }

    packed
}

/// Reads the committed table back out of its own source file.
fn committed_table() -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("graphemes")
        .join("table.rs");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("the generated table must be readable at {path:?}: {e}"));

    let size = usize::try_from(MAX_CODE_POINT).unwrap_or_else(|e| panic!("{e}")) + 1;
    let mut packed = vec![0u8; size];
    let mut entries = 0usize;

    for line in text.lines() {
        let line = line.trim();
        let Some(inner) = line.strip_prefix('(') else {
            continue;
        };
        let Some(inner) = inner.strip_suffix("),") else {
            continue;
        };
        let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
        let [low, high, value] = parts.as_slice() else {
            panic!("the table has an entry with the wrong number of fields: {line}");
        };
        let parse = |field: &str| {
            let digits = field.strip_prefix("0x").unwrap_or_else(|| {
                panic!("the table must be written in hexadecimal, found {field}")
            });
            u32::from_str_radix(digits, 16)
                .unwrap_or_else(|e| panic!("the table has a bad number {field}: {e}"))
        };
        let (low, high, value) = (parse(low), parse(high), parse(value));
        let value = u8::try_from(value)
            .unwrap_or_else(|e| panic!("a packed property must fit in a byte: {e}"));
        entries += 1;
        for code in low..=high {
            let index = usize::try_from(code).unwrap_or_else(|e| panic!("{e}"));
            if let Some(slot) = packed.get_mut(index) {
                *slot = value;
            }
        }
    }

    assert!(
        entries > 1000,
        "only {entries} ranges were read out of the table - the parser in this \
         test stopped matching the shape the generator writes, which would make \
         every comparison below pass for the wrong reason"
    );

    packed
}

#[test]
fn the_table_says_exactly_what_the_vendored_files_say() {
    let expected = expected_table();
    let committed = committed_table();

    let mut wrong: BTreeMap<u32, (u8, u8)> = BTreeMap::new();
    for code in 0..=MAX_CODE_POINT {
        let index = usize::try_from(code).unwrap_or_else(|e| panic!("{e}"));
        let (Some(&want), Some(&got)) = (expected.get(index), committed.get(index)) else {
            panic!("code point {code:04X} is missing from one of the two tables");
        };
        if want != got {
            wrong.insert(code, (want, got));
        }
    }

    let shown: Vec<String> = wrong
        .iter()
        .take(20)
        .map(|(code, (want, got))| {
            format!("  U+{code:04X}: files say {want:#04X}, table says {got:#04X}")
        })
        .collect();

    assert!(
        wrong.is_empty(),
        "{} code points disagree between the vendored files and the generated table \
         (first twenty shown):\n{}",
        wrong.len(),
        shown.join("\n")
    );
}

#[test]
fn every_packed_byte_stays_inside_the_numbers_the_code_can_unpack() {
    // This is what makes the two fallback arms in `graphemes.rs` provably dead
    // rather than merely unlikely. The module says so in as many words and
    // points here.
    let committed = committed_table();
    for (index, &value) in committed.iter().enumerate() {
        let breaks = value & 0x0F;
        let conjunct = (value >> 5) & 0x03;
        assert!(
            usize::from(breaks) < BREAK_NAMES.len(),
            "entry {index:04X} carries break value {breaks}, which no name covers"
        );
        assert!(
            usize::from(conjunct) < CONJUNCT_NAMES.len(),
            "entry {index:04X} carries conjunct value {conjunct}, which no name covers"
        );
        assert_eq!(
            value & 0x80,
            0,
            "entry {index:04X} sets a bit the packing does not define"
        );
    }
}

#[test]
fn the_vendored_files_are_the_version_the_code_claims() {
    // 🔴 The version is read out of the files rather than out of their names.
    // A file swapped for a newer one keeps its name and changes its header, and
    // the whole reason this number is pinned is that the victim application
    // this product is measured against implements exactly this version.
    let (major, minor, patch) = nkb_core::UNICODE_VERSION;
    let stamp = format!("{major}.{minor}.{patch}");
    let short = format!("{major}.{minor}");

    for (file, expected) in [
        ("GraphemeBreakProperty.txt", stamp.as_str()),
        ("DerivedCoreProperties.txt", stamp.as_str()),
        ("GraphemeBreakTest.txt", stamp.as_str()),
        ("DerivedNormalizationProps.txt", stamp.as_str()),
        ("DerivedCombiningClass.txt", stamp.as_str()),
    ] {
        let text = read(file);
        let first = text.lines().next().unwrap_or_default();
        assert!(
            first.contains(expected),
            "{file} announces itself as {first:?}, which is not Unicode {expected}"
        );
    }

    // emoji-data.txt states its version on a line of its own rather than in the
    // first line, and states it with two components rather than three.
    let emoji = read("emoji-data.txt");
    let version_line = emoji
        .lines()
        .find(|line| line.starts_with("# Version:"))
        .unwrap_or_else(|| panic!("emoji-data.txt must announce its version"));
    assert!(
        version_line.contains(&short),
        "emoji-data.txt announces {version_line:?}, which is not Unicode {short}"
    );
}

// ---------------------------------------------------------------------------
// Normalization - `src/normalization/table.rs`
// ---------------------------------------------------------------------------

/// A quick check answer, as `DerivedNormalizationProps.txt` spells it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Quick {
    Yes,
    No,
    Maybe,
}

fn every_code_point<T: Clone>(value: T) -> Vec<T> {
    let size = usize::try_from(MAX_CODE_POINT).unwrap_or_else(|e| panic!("{e}")) + 1;
    vec![value; size]
}

fn set_range<T: Copy>(slots: &mut [T], low: u32, high: u32, value: T) {
    for code in low..=high.min(MAX_CODE_POINT) {
        let index = usize::try_from(code).unwrap_or_else(|e| panic!("{e}"));
        if let Some(slot) = slots.get_mut(index) {
            *slot = value;
        }
    }
}

/// The answer of one quick check property for every code point.
///
/// The file lists only `No` and `Maybe`. A code point it does not name is `Yes`
/// by the file's own convention.
fn quick_check(property: &str) -> Vec<Quick> {
    let mut answers = every_code_point(Quick::Yes);
    let text = read("DerivedNormalizationProps.txt");
    let mut lines = 0usize;
    for (low, high, fields) in data_lines(&text) {
        if fields.first() != Some(&property) {
            continue;
        }
        let answer = match fields.get(1) {
            Some(&"N") => Quick::No,
            Some(&"M") => Quick::Maybe,
            other => panic!("{property} gives an answer this test does not know: {other:?}"),
        };
        lines += 1;
        set_range(&mut answers, low, high, answer);
    }
    assert!(
        lines > 100,
        "only {lines} lines of {property} were read - the parser stopped matching \
         the file, which would make every comparison below pass for the wrong reason"
    );
    answers
}

/// `Canonical_Combining_Class` for every code point.
///
/// The file's own `@missing` line makes an unlisted code point class 0.
fn combining_classes() -> Vec<u8> {
    let mut classes = every_code_point(0u8);
    let text = read("DerivedCombiningClass.txt");
    let mut lines = 0usize;
    for (low, high, fields) in data_lines(&text) {
        let class = fields
            .first()
            .and_then(|field| field.parse::<u8>().ok())
            .unwrap_or_else(|| panic!("a class that is not a number 0-255: {fields:?}"));
        lines += 1;
        set_range(&mut classes, low, high, class);
    }
    assert!(
        lines > 1000,
        "only {lines} lines of DerivedCombiningClass.txt were read - the parser \
         stopped matching the file"
    );
    classes
}

/// The table the files imply: every code point's `NFKC_QC` answer as "not
/// Yes", and its combining class.
struct Normalization {
    not_quick_yes: Vec<bool>,
    combining_class: Vec<u8>,
}

fn expected_normalization() -> Normalization {
    Normalization {
        not_quick_yes: quick_check("NFKC_QC")
            .iter()
            .map(|answer| *answer != Quick::Yes)
            .collect(),
        combining_class: combining_classes(),
    }
}

/// The inclusive ranges of equal, non-zero values, in code point order.
fn ranges_of(values: &[u8]) -> Vec<(u32, u32, u8)> {
    let mut ranges: Vec<(u32, u32, u8)> = Vec::new();
    for (index, &value) in values.iter().enumerate() {
        if value == 0 {
            continue;
        }
        let code = u32::try_from(index).unwrap_or_else(|e| panic!("{e}"));
        match ranges.last_mut() {
            Some((_, high, same)) if *high + 1 == code && *same == value => *high = code,
            _ => ranges.push((code, code, value)),
        }
    }
    ranges
}

/// The source of `src/normalization/table.rs`, exactly as it should read.
fn render_normalization_table(expected: &Normalization) -> String {
    let (major, minor, patch) = nkb_core::UNICODE_VERSION;
    let flagged: Vec<u8> = expected
        .not_quick_yes
        .iter()
        .map(|&on| u8::from(on))
        .collect();
    let not_yes = ranges_of(&flagged);
    let classes = ranges_of(&expected.combining_class);

    let mut out = format!(
        "//! Generated from the Unicode Character Database. Do not edit by hand.
//!
//! Source files, their exact bytes and the reason they are vendored:
//! `crates/nkb-core/unicode/README.md`. Unicode {major}.{minor}.{patch}.
//!
//! Two tables of inclusive code point ranges. A code point in no range of the
//! first answers `Yes` to `NFKC_Quick_Check`, and one in no range of the second
//! has combining class 0 - which is the common case for both, and why it is
//! left out.
//!
//! What keeps this honest is `tests/unicode_data.rs`, which rebuilds both
//! tables from those files and compares all 1 114 112 code points. It also
//! writes the source the files imply to
//! `target/tmp/unicode/normalization_table.rs` on every run, which is how this
//! file was made and how it is remade for the next version of the standard.

/// `NFKC_Quick_Check` is `No` or `Maybe` - which [`super::may_change`] treats
/// alike - from `DerivedNormalizationProps.txt`.
pub(super) static NOT_QUICK_YES: [(u32, u32); {}] = [
",
        not_yes.len()
    );
    for (low, high, _) in &not_yes {
        out.push_str(&format!("    (0x{low:04X}, 0x{high:04X}),\n"));
    }
    out.push_str(&format!(
        "];

/// `Canonical_Combining_Class` where it is not 0, from
/// `extracted/DerivedCombiningClass.txt`. The class is written in decimal, as
/// the standard writes it.
pub(super) static COMBINING_CLASS: [(u32, u32, u8); {}] = [
",
        classes.len()
    ));
    for (low, high, class) in &classes {
        out.push_str(&format!("    (0x{low:04X}, 0x{high:04X}, {class}),\n"));
    }
    out.push_str("];\n");
    out
}

/// Reads both tables back out of the committed source.
///
/// An entry belongs to the table whose `static` line came last before it, and
/// its number of fields must match that table - a two-field entry among the
/// classes is a broken file, not a class of zero.
fn committed_normalization() -> Normalization {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("normalization")
        .join("table.rs");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("the generated table must be readable at {path:?}: {e}"));

    let mut committed = Normalization {
        not_quick_yes: every_code_point(false),
        combining_class: every_code_point(0u8),
    };
    let mut table = "";
    let (mut not_yes_entries, mut class_entries) = (0usize, 0usize);
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with("pub(super) static NOT_QUICK_YES") {
            table = "NOT_QUICK_YES";
            continue;
        }
        if line.starts_with("pub(super) static COMBINING_CLASS") {
            table = "COMBINING_CLASS";
            continue;
        }
        let Some(inner) = line
            .strip_prefix('(')
            .and_then(|inner| inner.strip_suffix("),"))
        else {
            continue;
        };
        let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
        let hex = |field: &str| {
            let digits = field.strip_prefix("0x").unwrap_or_else(|| {
                panic!("a code point must be written in hexadecimal, found {field}")
            });
            u32::from_str_radix(digits, 16)
                .unwrap_or_else(|e| panic!("the table has a bad number {field}: {e}"))
        };
        match (table, parts.as_slice()) {
            ("NOT_QUICK_YES", [low, high]) => {
                not_yes_entries += 1;
                set_range(&mut committed.not_quick_yes, hex(low), hex(high), true);
            }
            ("COMBINING_CLASS", [low, high, class]) => {
                class_entries += 1;
                let class = class
                    .parse::<u8>()
                    .unwrap_or_else(|e| panic!("a class must be a decimal byte: {line}: {e}"));
                set_range(&mut committed.combining_class, hex(low), hex(high), class);
            }
            _ => panic!("an entry that fits no table in {table:?}: {line}"),
        }
    }
    assert!(
        not_yes_entries > 100 && class_entries > 100,
        "only {not_yes_entries} quick check and {class_entries} class ranges were read - \
         the parser stopped matching what the generator writes"
    );
    committed
}

#[test]
fn the_normalization_tables_say_exactly_what_the_vendored_files_say() {
    let expected = expected_normalization();

    // Written every time, before the comparison, so the generator's output is
    // there to read even when the committed table is missing or unreadable.
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("target")
        .join("tmp")
        .join("unicode");
    let path = dir.join("normalization_table.rs");
    std::fs::create_dir_all(&dir)
        .and_then(|()| std::fs::write(&path, render_normalization_table(&expected)))
        .unwrap_or_else(|e| panic!("the implied table must be writable at {path:?}: {e}"));

    let committed = committed_normalization();

    let mut wrong: Vec<String> = Vec::new();
    for (code, (want, got)) in expected
        .not_quick_yes
        .iter()
        .zip(&committed.not_quick_yes)
        .enumerate()
    {
        if want != got {
            let says = if *want { "not Yes" } else { "Yes" };
            wrong.push(format!(
                "  U+{code:04X}: NFKC_QC in the file is {says}, the table the opposite"
            ));
        }
    }
    for (code, (want, got)) in expected
        .combining_class
        .iter()
        .zip(&committed.combining_class)
        .enumerate()
    {
        if want != got {
            wrong.push(format!(
                "  U+{code:04X}: combining class {want} in the file, {got} in the table"
            ));
        }
    }

    assert!(
        wrong.is_empty(),
        "{} disagreements between the vendored files and the generated table \
         (first twenty shown):\n{}\nThe table the files imply is at {path:?}, ready \
         to replace src/normalization/table.rs.",
        wrong.len(),
        wrong
            .iter()
            .take(20)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn one_table_serves_both_forms_because_nfc_is_never_less_sure_than_nfkc() {
    // `normalization.rs` keeps ONE table, NFKC's, and answers for NFC with it.
    // That is correct only while every code point NFC does not answer Yes for
    // is also not Yes for NFKC - measured true on 17.0.0, and checked here for
    // every code point so a new standard cannot quietly make it false.
    let nfc = quick_check("NFC_QC");
    let nfkc = quick_check("NFKC_QC");
    let missed: Vec<String> = nfc
        .iter()
        .zip(&nfkc)
        .enumerate()
        .filter(|(_, (nfc, nfkc))| **nfc != Quick::Yes && **nfkc == Quick::Yes)
        .map(|(code, _)| format!("U+{code:04X}"))
        .collect();
    assert!(
        missed.is_empty(),
        "NFC may change these and the NFKC table says Yes, so the one table would \
         miss them: {missed:?}"
    );
}

/// `Default_Ignorable_Code_Point` for every code point, from the vendored file.
fn expected_ignorable() -> Vec<u8> {
    let mut flags = every_code_point(0u8);
    let text = read("DerivedCoreProperties.txt");
    let mut lines = 0usize;
    for (low, high, fields) in data_lines(&text) {
        if fields.first() != Some(&"Default_Ignorable_Code_Point") {
            continue;
        }
        lines += 1;
        set_range(&mut flags, low, high, 1);
    }
    assert!(
        lines > 10,
        "only {lines} lines of Default_Ignorable_Code_Point were read - the parser \
         stopped matching the file, which would make the comparison pass for the \
         wrong reason"
    );
    flags
}

/// The source of `src/ignorable/table.rs`, exactly as it should read.
fn render_ignorable_table(expected: &[u8]) -> String {
    let (major, minor, patch) = nkb_core::UNICODE_VERSION;
    let ranges = ranges_of(expected);
    let mut out = format!(
        "//! Generated from the Unicode Character Database. Do not edit by hand.
//!
//! Source file, its exact bytes and the reason it is vendored:
//! `crates/nkb-core/unicode/README.md`. Unicode {major}.{minor}.{patch}.
//!
//! Every code point with `Default_Ignorable_Code_Point` in
//! `DerivedCoreProperties.txt`, as inclusive ranges in code point order.
//!
//! What keeps this honest is `tests/unicode_data.rs`, which rebuilds the table
//! from that file and compares all 1 114 112 code points. It also writes the
//! source the file implies to `target/tmp/unicode/ignorable_table.rs` on every
//! run, which is how this file was made and how it is remade for the next
//! version of the standard.

pub(super) static DEFAULT_IGNORABLE: [(u32, u32); {}] = [
",
        ranges.len()
    );
    for (low, high, _) in &ranges {
        out.push_str(&format!("    (0x{low:04X}, 0x{high:04X}),\n"));
    }
    out.push_str("];\n");
    out
}

/// Reads the table back out of the committed source.
fn committed_ignorable() -> (Vec<u8>, usize) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("ignorable")
        .join("table.rs");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("the generated table must be readable at {path:?}: {e}"));
    let mut flags = every_code_point(0u8);
    let mut entries = 0usize;
    for line in text.lines() {
        let Some(inner) = line
            .trim()
            .strip_prefix('(')
            .and_then(|inner| inner.strip_suffix("),"))
        else {
            continue;
        };
        let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
        let [low, high] = parts.as_slice() else {
            panic!("an entry of the ignorable table must be a pair: {line}");
        };
        let hex = |field: &str| {
            let digits = field.strip_prefix("0x").unwrap_or_else(|| {
                panic!("a code point must be written in hexadecimal, found {field}")
            });
            u32::from_str_radix(digits, 16)
                .unwrap_or_else(|e| panic!("the table has a bad number {field}: {e}"))
        };
        entries += 1;
        set_range(&mut flags, hex(low), hex(high), 1);
    }
    (flags, entries)
}

#[test]
fn the_ignorable_table_says_exactly_what_the_vendored_file_says() {
    let expected = expected_ignorable();

    // Written every time, before the comparison, so the generator's output is
    // there to read even when the committed table is missing or empty.
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("target")
        .join("tmp")
        .join("unicode");
    let path = dir.join("ignorable_table.rs");
    std::fs::create_dir_all(&dir)
        .and_then(|()| std::fs::write(&path, render_ignorable_table(&expected)))
        .unwrap_or_else(|e| panic!("the implied table must be writable at {path:?}: {e}"));

    let (committed, entries) = committed_ignorable();
    let wrong: Vec<String> = expected
        .iter()
        .zip(&committed)
        .enumerate()
        .filter(|(_, (want, got))| want != got)
        .map(|(code, (want, _))| {
            let says = if *want == 1 {
                "ignorable"
            } else {
                "not ignorable"
            };
            format!("  U+{code:04X}: {says} in the file, the table the opposite")
        })
        .collect();
    assert!(
        wrong.is_empty(),
        "{} disagreements between DerivedCoreProperties.txt and the ignorable table \
         of {entries} ranges (first twenty shown):\n{}\nThe table the file implies is \
         at {path:?}, ready to replace src/ignorable/table.rs.",
        wrong.len(),
        wrong
            .iter()
            .take(20)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}
