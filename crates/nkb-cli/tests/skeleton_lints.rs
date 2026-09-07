//! The two halves of the public contract, tied together.
//!
//! # Why this test is the point of `nkb new-pack` rather than an extra
//!
//! `nkb new-pack` and `nkb lint` are the whole published contract of the pack
//! format - there is no English specification and there is deliberately not
//! going to be one (`decision-log.md` D20). A contributor meets the format
//! through the skeleton one command writes and the verdict the other gives.
//!
//! So a skeleton that does not pass the linter is worse than no skeleton at all:
//! step one of the contributor path would hand somebody a file that step three
//! refuses, and the first thing they learn about this project is that its two
//! commands disagree.
//!
//! Nothing else keeps them in step. Every rule added to the validator is a
//! constraint the skeleton has to satisfy, and rules get added - five of them on
//! the day this test was written. Without this the two drift apart silently, and
//! the drift shows up in front of a stranger.

// A failed expectation in a test is a failed test.
#![allow(clippy::panic, clippy::expect_used)]

// The skeleton is TOML, and what TOML looks like is the adapter's knowledge -
// so it is asked for there rather than through the use case above it.
use nkb_adapters::{DirectoryPackSource, TomlPackFormat, skeleton};
use nkb_app::{LintOutcome, lint_pack};

/// A date that is a real date, so the skeleton it produces is one too.
const TODAY: (i32, u32, u32) = (2026, 9, 7);

#[test]
fn the_skeleton_new_pack_writes_passes_the_linter_with_nothing_to_say() {
    let dir = std::env::temp_dir().join(format!("nkb-skeleton-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a temporary directory");

    let id = "locale-cz";
    let path = dir.join(format!("{id}.toml"));
    std::fs::write(&path, skeleton::for_pack(id, TODAY)).expect("the skeleton is written");

    let (source, read_as) = DirectoryPackSource::split(&path).expect("the path names a file");
    let LintOutcome::Judged(report) = lint_pack(&source, &TomlPackFormat, &read_as) else {
        panic!("the skeleton must be readable");
    };

    let found: Vec<String> = report
        .problems
        .iter()
        .map(|p| format!("{} at line {:?}", p.code.as_str(), p.line))
        .collect();
    assert!(
        found.is_empty(),
        "the skeleton must be clean, and it produced {found:?}. A contributor runs \
         `nkb lint` on this file as the next step, and the two commands are the whole \
         published contract of the format"
    );

    // Warnings included. A warning does not block a stranger's pack and should
    // not - but the skeleton is the example every contributor edits, so a warning
    // in it is a warning propagated into every pack written from it.
    assert_eq!(report.warnings(), 0, "the skeleton carries no warnings");

    let skipped: Vec<&str> = report.skipped.iter().map(|s| s.reason.as_str()).collect();
    assert!(
        skipped.is_empty(),
        "the skeleton left rules unchecked: {skipped:?}"
    );

    std::fs::remove_file(&path).ok();
    std::fs::remove_dir(&dir).ok();
}

#[test]
fn the_skeleton_names_the_pack_it_was_asked_for() {
    // E010 requires the declared identifier and the file name to agree, so this
    // is the one part of the skeleton that cannot be a fixed string.
    let text = skeleton::for_pack("magic-values", TODAY);
    assert!(text.contains("id          = \"magic-values\""), "{text}");
}

#[test]
fn the_skeleton_says_the_format_is_not_frozen_yet() {
    // Required of both halves of the contract by the specification: silence about
    // a changeable format reads as stability to whoever builds on it.
    let text = skeleton::for_pack("locale-cz", TODAY);
    assert!(
        text.to_lowercase().contains("not frozen"),
        "the skeleton has to say so where the person editing it will read it: {text}"
    );
}

#[test]
fn the_skeleton_is_written_in_printable_ascii() {
    // The same check the shipped packs get, for the same reason: this file is the
    // template every contributed pack starts from, so a character nobody can see
    // in it is a character nobody can see in all of them.
    for (number, line) in skeleton::for_pack("locale-cz", TODAY).lines().enumerate() {
        let offending: Vec<char> = line
            .chars()
            .filter(|c| !c.is_ascii() || (c.is_control() && *c != '\t'))
            .collect();
        assert!(
            offending.is_empty(),
            "line {} of the skeleton carries {:?} literally",
            number + 1,
            offending
        );
    }
}
