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

/// What one file produces: the codes, the rules that could not be checked over
/// it, and whether the pack would be loaded.
struct Verdict {
    codes: BTreeSet<String>,
    /// Written as `CODE=reason`, the same shape a file declares them in, so that
    /// a mismatch prints two lists a person can read side by side.
    skips: BTreeSet<String>,
    accepted: bool,
}

fn judge(path: &Path) -> Verdict {
    let (source, id) = DirectoryPackSource::split(path)
        .unwrap_or_else(|| panic!("{} must name a file", path.display()));

    match lint_pack(&source, &TomlPackFormat, &id) {
        LintOutcome::Judged(report) => Verdict {
            codes: report
                .problems
                .iter()
                .map(|p| p.code.as_str().to_owned())
                .collect(),
            skips: report
                .skipped
                .iter()
                .map(|s| format!("{}={}", s.code.as_str(), s.reason.as_str()))
                .collect(),
            accepted: report.accepted(),
        },
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

/// Reads the rules a file declares as impossible to check over it, written
/// `# skipped: W052=translated-pack-not-found` among the opening comments.
///
/// # Why the absence of the line is the strong half
///
/// A file with no such line must produce **no** skips at all. That turns every
/// file already in this set into a guard for a mechanism written years after
/// them: the day a rule starts quietly giving up on ordinary packs, thirty six
/// files say so at once.
///
/// Without this, the suite compares a set of codes and nothing else - so a rule
/// that reported the right code while checking nothing would pass, which is the
/// blind spot this declaration exists to close.
fn expected_skips(path: &Path) -> BTreeSet<String> {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("{} must be readable: {e}", path.display()));

    text.lines()
        .map(|line| line.trim_start_matches('\u{FEFF}'))
        // The opening comment block only. A line further down is prose or data,
        // and a declaration hidden there would be one nobody reading the top of
        // the file could see.
        .take_while(|line| line.starts_with('#') || line.trim().is_empty())
        .find_map(|line| line.strip_prefix("# skipped:"))
        .map(|list| {
            list.split(',')
                .map(|entry| entry.trim().to_owned())
                .filter(|entry| !entry.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn every_accepted_pack_passes_every_rule_this_build_checks() {
    let files = toml_files("accepted");
    assert!(!files.is_empty(), "the accepted set must not be empty");

    for path in files {
        let verdict = judge(&path);
        assert!(
            verdict.codes.is_empty(),
            "{} must be accepted, but produced {:?}",
            path.display(),
            verdict.codes
        );
        assert!(verdict.accepted, "{} must load", path.display());
        // A pack in this set asserts that it is good. A rule that could not be
        // checked over it leaves that assertion resting on a rule nobody ran, so
        // the accepted set carries no skips at all - not even declared ones.
        assert!(
            verdict.skips.is_empty(),
            "{} is in the accepted set and left {:?} unchecked, so it is not \
             known to pass everything this build checks",
            path.display(),
            verdict.skips
        );
    }
}

#[test]
fn every_rejected_pack_produces_exactly_the_codes_it_declares() {
    let files = toml_files("rejected");
    assert!(!files.is_empty(), "the rejected set must not be empty");

    for path in files {
        let expected = expected_codes(&path);
        let verdict = judge(&path);
        let found = verdict.codes;
        let accepted = verdict.accepted;
        assert_eq!(
            found,
            expected,
            "{} declares {expected:?} and produced {found:?}",
            path.display()
        );

        // The half a set of codes cannot see. A rule can report the right code
        // and still have checked nothing, and a rule can quietly start giving up
        // on files it used to judge - neither shows in the codes above.
        let expected_skips = expected_skips(&path);
        assert_eq!(
            verdict.skips,
            expected_skips,
            "{} declares the skips {expected_skips:?} and produced {:?}. A file \
             with no `# skipped:` line must leave nothing unchecked",
            path.display(),
            verdict.skips
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
