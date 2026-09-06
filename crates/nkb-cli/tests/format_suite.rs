//! The format test suite: every file in `tests/packs`, judged.
//!
//! This is what turns the format specification from prose into something a
//! machine can disagree with. Each rejected file names the codes it must produce,
//! in a comment on its first line, so the file and its expectation cannot drift
//! apart the way a file and a separate table would.
//!
//! It is also the only guard that would survive the validator being rewritten -
//! or written a second time by somebody else - which is the point of publishing a
//! format at all.

// A failed expectation in a test is a failed test, and these run only under
// `cargo test`. The workspace denies both in shipped code, which is the setting
// that matters: a crash inside a validator is a crash inside somebody else's CI.
#![allow(clippy::panic, clippy::expect_used)]

use nkb_adapters::{DirectoryPackSource, TomlPackFormat};
use nkb_app::{LintOutcome, lint_pack};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn packs_dir(kind: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/packs")
        .join(kind)
}

fn toml_files(kind: &str) -> Vec<PathBuf> {
    let dir = packs_dir(kind);
    let entries = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("the {kind} set must exist at {}: {e}", dir.display()));

    let mut files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "toml"))
        .collect();
    files.sort();
    files
}

/// The codes a file produces, and whether the pack would be loaded.
fn judge(path: &Path) -> (BTreeSet<String>, bool) {
    let (source, id) = DirectoryPackSource::split(path)
        .unwrap_or_else(|| panic!("{} must name a file", path.display()));

    match lint_pack(&source, &TomlPackFormat, &id) {
        LintOutcome::Judged(report) => (
            report
                .problems
                .iter()
                .map(|p| p.code.as_str().to_owned())
                .collect(),
            report.accepted(),
        ),
        other => panic!("{} could not be read: {other:?}", path.display()),
    }
}

/// Reads the codes a rejected file declares on its first line.
fn expected_codes(path: &Path) -> BTreeSet<String> {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("{} must be readable: {e}", path.display()));

    // One file in this set opens with a byte order mark, because that is the rule
    // it exists to break. Reading its declaration has to look past the mark, or
    // the suite would fail to parse the very file it is meant to check.
    let first = text
        .lines()
        .next()
        .unwrap_or_default()
        .trim_start_matches('\u{FEFF}');

    let list = first.strip_prefix("# expect:").unwrap_or_else(|| {
        panic!(
            "{} must declare its expected codes on the first line, as `# expect: E001`",
            path.display()
        )
    });

    list.split(',')
        .map(|code| code.trim().to_owned())
        .filter(|code| !code.is_empty())
        .collect()
}

#[test]
fn every_accepted_pack_passes_every_rule_this_build_checks() {
    let files = toml_files("accepted");
    assert!(!files.is_empty(), "the accepted set must not be empty");

    for path in files {
        let (found, accepted) = judge(&path);
        assert!(
            found.is_empty(),
            "{} must be accepted, but produced {found:?}",
            path.display()
        );
        assert!(accepted, "{} must load", path.display());
    }
}

#[test]
fn every_rejected_pack_produces_exactly_the_codes_it_declares() {
    let files = toml_files("rejected");
    assert!(!files.is_empty(), "the rejected set must not be empty");

    for path in files {
        let expected = expected_codes(&path);
        let (found, accepted) = judge(&path);
        assert_eq!(
            found,
            expected,
            "{} declares {expected:?} and produced {found:?}",
            path.display()
        );

        // The severity of a rule has to reach the verdict, or the letter in front
        // of a code becomes decoration. A file whose codes all begin with W is a
        // pack that loads and carries warnings - one starting with E is a pack
        // that does not load at all, whole or absent.
        let blocking = expected.iter().any(|code| code.starts_with('E'));
        assert_eq!(
            accepted,
            !blocking,
            "{} declares {expected:?}, so accepted should be {}",
            path.display(),
            !blocking
        );
    }
}

/// The two files whose **bytes** are the test, guarded against quiet repair.
///
/// Both cover rules about what a file is made of rather than what it says, so an
/// editor that tidies them - or a lost exception in the repository's line ending
/// rules - would leave the suite green and blind. Checking the bytes here turns
/// that into a failure that names what happened, instead of a mismatch that names
/// a missing code and leaves the reader guessing why.
#[test]
fn the_two_byte_level_files_still_carry_the_bytes_they_are_about() {
    let mark = std::fs::read(packs_dir("rejected").join("byte-order-mark.toml"))
        .expect("the byte order mark file must exist");
    assert!(
        mark.starts_with(&[0xEF, 0xBB, 0xBF]),
        "byte-order-mark.toml lost its byte order mark, so it no longer tests E005"
    );

    let endings = std::fs::read(packs_dir("rejected").join("crlf-line-endings.toml"))
        .expect("the line ending file must exist");
    assert!(
        endings.windows(2).any(|pair| pair == b"\r\n"),
        "crlf-line-endings.toml lost its two-character line endings, so it no longer \
         tests E006. Check that .gitattributes still exempts this one path."
    );
}
