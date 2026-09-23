//! The generated table, judged against the files it was generated from.
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
