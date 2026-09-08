//! Is there still exactly one way for a pack file to become a pack?
//!
//! # What this is guarding
//!
//! `architektura.md` 3 calls the order binding: `check` first, refuse on any
//! error, `parse` after - because `pack-format.md` 11 loads all of a pack or
//! none of it. Until 2026-09-08 that order was written out once per use case,
//! three times, and it grew with them (OBS-96).
//!
//! Three copies of a safety rule are three chances to write the fourth one
//! differently. And the fourth would not look wrong: `parse` returns a pack, so
//! a caller reaching for it directly gets exactly what it asked for.
//!
//! 🔴 What it would not get is the version check. `PackFormat::parse` never
//! looks at `format`; only `check` does. A caller skipping straight to `parse`
//! would load a pack written for a format this build does not understand, and
//! report nothing.
//!
//! # Why a text scan rather than something the compiler enforces
//!
//! Making the order unrepresentable needs a witness type - `parse` accepting
//! proof that `check` ran - and that is an abstraction built for a single shape
//! nobody has asked to vary. `architektura.md` 8 says the extension point waits
//! for the second real case. A scan plus one function is the cheap half of that
//! trade, and it fails loudly in CI, which is where the fourth caller would
//! appear.

// A failed expectation in a test is a failed test, and the workspace forbids
// these everywhere else on purpose.
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::path::{Path, PathBuf};

/// The one module allowed to call it.
const HOME_OF_THE_CALL: &str = "load_pack.rs";

/// How the call looks in source. A method call, so that `fn parse` in a test
/// double - a definition, not a call - is not mistaken for one.
const THE_CALL: &str = ".parse(";

fn source_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

/// Every `.rs` in this layer, as (file name, contents).
fn sources() -> Vec<(String, String)> {
    let mut found = Vec::new();
    let entries = std::fs::read_dir(source_dir()).expect("this layer's src must be readable");
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "rs") {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .expect("a file name")
                .to_owned();
            let text = std::fs::read_to_string(&path).expect("a source file must be readable");
            found.push((name, text));
        }
    }
    found.sort();
    found
}

/// The part of a file that ships, with the tests cut off.
///
/// Test doubles legitimately implement the port and call each other, and none of
/// that reaches a user.
fn shipping_part(text: &str) -> &str {
    text.split("#[cfg(test)]").next().unwrap_or(text)
}

#[test]
fn only_one_module_turns_a_file_into_a_pack() {
    let mut offenders = Vec::new();

    for (name, text) in sources() {
        if name == HOME_OF_THE_CALL {
            continue;
        }
        let lines: Vec<&str> = shipping_part(&text)
            .lines()
            .filter(|line| line.contains(THE_CALL))
            .map(str::trim)
            .collect();
        if !lines.is_empty() {
            offenders.push(format!("{name}: {lines:?}"));
        }
    }

    assert!(
        offenders.is_empty(),
        "these files in this layer turn a file into a pack on their own: {offenders:?}.\n\
         The order `check` then `parse` lives in {HOME_OF_THE_CALL} and nowhere else, because \
         `parse` does not check the format version and a caller that skipped `check` would load \
         a pack this build does not understand without reporting it. Call \
         `crate::load_pack::load` instead."
    );
}

#[test]
fn the_module_that_may_call_it_actually_does() {
    // 🔴 The negative control, and it is not decoration. If the call were ever
    // spelled differently - renamed, wrapped, reached through another name - the
    // scan above would find nothing anywhere and pass while guarding nothing.
    let home = sources()
        .into_iter()
        .find(|(name, _)| name == HOME_OF_THE_CALL)
        .map(|(_, text)| text)
        .unwrap_or_else(|| panic!("{HOME_OF_THE_CALL} must exist in this layer"));

    let calls = shipping_part(&home)
        .lines()
        .filter(|line| line.contains(THE_CALL))
        .count();

    assert!(
        calls > 0,
        "the scan found no `{THE_CALL}` in {HOME_OF_THE_CALL}, so it is no longer looking for \
         what it thinks it is looking for, and the test above proves nothing"
    );
}

#[test]
fn the_scan_looks_at_more_than_one_file() {
    // Two ways this suite could pass by accident: the folder unreadable, or the
    // extension filter matching nothing. Both leave an empty list, and an empty
    // list satisfies every assertion above.
    let names: Vec<String> = sources().into_iter().map(|(name, _)| name).collect();

    assert!(
        names.len() > 3,
        "found only {names:?} in this layer - the scan is not reading the sources it claims to"
    );
    assert!(
        names.iter().any(|name| name == HOME_OF_THE_CALL),
        "{HOME_OF_THE_CALL} is not among {names:?}"
    );
}
