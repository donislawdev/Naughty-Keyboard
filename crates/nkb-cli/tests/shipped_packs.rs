//! Every pack this project ships, judged by its own validator.
//!
//! # Why this is a test and not a release step
//!
//! The format specification says a release whose own pack fails validation has
//! no right to exist. A step that runs only at release time learns that on the
//! day it is most expensive to hear, so the check runs on every commit instead.
//!
//! # How this differs from the format suite next to it
//!
//! `format_suite.rs` judges files written to break rules on purpose - it proves
//! the validator disagrees when it should. This one judges the real catalogue and
//! proves the validator agrees when it should, on content nobody wrote to please
//! it. The two fail for opposite reasons and neither replaces the other.
//!
//! # What this cannot check, and it matters
//!
//! 🔴 The pack files live in this repository. The document that decides their
//! content does not. Nothing here can tell whether a pack still matches the
//! catalogue it was written from - that drift is unguarded by construction, and
//! saying so is better than leaving a reader to assume otherwise.

// A failed expectation in a test is a failed test. The workspace denies both in
// shipped code, which is the setting that matters.
#![allow(clippy::panic, clippy::expect_used)]

use nkb_adapters::{DirectoryPackSource, TomlPackFormat};
use nkb_app::{LintOutcome, lint_pack};
use std::path::{Path, PathBuf};

fn packs_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packs")
}

fn shipped_packs() -> Vec<PathBuf> {
    let dir = packs_dir();
    let entries = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("the pack folder must exist at {}: {e}", dir.display()));

    let mut files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "toml"))
        .collect();
    files.sort();
    files
}

#[test]
fn every_pack_this_project_ships_passes_its_own_validator() {
    let files = shipped_packs();

    // The check that stops this from passing by finding nothing. A guard over an
    // empty folder is green for the same reason a guard over a perfect folder is,
    // and the two are worth telling apart.
    assert!(
        !files.is_empty(),
        "no packs found in {} - this test would otherwise pass by looking at nothing",
        packs_dir().display()
    );

    for path in files {
        let (source, id) = DirectoryPackSource::split(&path)
            .unwrap_or_else(|| panic!("{} must name a file", path.display()));

        let LintOutcome::Judged(report) = lint_pack(&source, &TomlPackFormat, &id) else {
            panic!("{} could not be read", path.display());
        };

        // Warnings included. A warning does not block somebody else's pack, and
        // that is the right call for a stranger's file - but a warning in our own
        // is the example every contributor copies, so it is a failure here.
        let found: Vec<String> = report
            .problems
            .iter()
            .map(|p| {
                format!(
                    "{}:{} {}",
                    path.display(),
                    p.line.unwrap_or(0),
                    p.code.as_str()
                )
            })
            .collect();
        assert!(
            found.is_empty(),
            "a pack this project ships must be clean, and {} is not: {found:?}",
            path.display()
        );

        // A pack of ours that left a rule unchecked is a pack we do not actually
        // know to be good - the same reasoning that keeps the accepted set free
        // of skips.
        let skipped: Vec<&str> = report.skipped.iter().map(|s| s.reason.as_str()).collect();
        assert!(
            skipped.is_empty(),
            "{} left rules unchecked: {skipped:?}",
            path.display()
        );
    }
}

#[test]
fn a_shipped_pack_carries_its_invisible_characters_escaped_rather_than_literally() {
    // 🔴 The bytes are the test, the way they are for two files in the format
    // suite. A pack about characters nobody can see is the one file where an
    // editor tidying a value, or a paste that dropped an escape, leaves a value
    // that still parses, still passes every rule, and no longer tests anything.
    //
    // Measured on 2026-09-07: writing this pack by hand produced seven literal
    // characters on the first attempt and again on the second, and one value lost
    // its carriage return and became a silent duplicate of the value above it.
    // That is why this checks the file rather than the parsed value.
    for path in shipped_packs() {
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{} must be readable: {e}", path.display()));

        for (number, line) in text.lines().enumerate() {
            let offending: Vec<char> = line
                .chars()
                .filter(|c| {
                    // Printable ASCII is what a pack file is written in. Anything
                    // else in a value has to arrive as an escape, and anything
                    // else in prose has no business being there either.
                    !c.is_ascii() || (c.is_control() && *c != '\t')
                })
                .collect();
            assert!(
                offending.is_empty(),
                "{}:{} carries {:?} literally. A character that cannot be seen has to \
                 be written as an escape, or the file stops testing what it claims to.",
                path.display(),
                number + 1,
                offending
                    .iter()
                    .map(|c| format!("U+{:04X}", *c as u32))
                    .collect::<Vec<_>>()
            );
        }
    }
}
