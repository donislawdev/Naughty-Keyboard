//! The clipboard is written from the places named here and from nowhere else.
//!
//! # The promise this stands for
//!
//! Untouchable rule 17, third promise: the tool uses the clipboard in exactly two
//! places - copying the report block, and clipboard mode delivering a value.
//! architektura.md 5 turns that into a count, and until the report block arrived
//! the row read "nothing" in the column of what goes red, because there was no
//! port. This file is the guard that row promised, written together with the
//! port, as the row said it would be. Since `D71` the list has both doors: the
//! report block in `app`, and the clipboard route in `nkb-adapters` - which
//! presses no key, so it is a door of this list and of no other.
//!
//! # Three ways round a port, and one check for each
//!
//! A count of port calls alone is easy to walk around, so this guards all three
//! routes to a clipboard this workspace has:
//!
//! 1. the port - `.put_text(` called from the named doors and nowhere else.
//! 2. the library - `arboard` named in ONE manifest and ONE source file, the
//!    adapter. Another package reaching for it directly would write the
//!    clipboard without passing a door.
//! 3. the toolkit - Slint copies and pastes on its own inside a text field.
//!    The palette has none, and a view gaining one opens a clipboard path no
//!    Rust call shows. When the pack search (step 7) brings its field, it
//!    brings a deliberate edit here with it.
//!
//! # What this checks, and what it cannot
//!
//! The same limits as `keystrokes_have_named_doors.rs`: text, not execution
//! paths. `src/` only. Test modules and `//` comments dropped. A helper wrapping
//! the call would be one door with many callers, and this would not see them.

// A failed expectation in a test is a failed test.
#![allow(clippy::panic, clippy::expect_used)]

use std::path::{Path, PathBuf};

/// The doors, as `crates/<package>/src/<file>` with the number of calls each
/// may hold: the report block, and the clipboard route of clipboard mode.
const DOORS: &[(&str, usize)] = &[
    ("nkb-app/src/advance_sequence.rs", 1),
    ("nkb-adapters/src/clipboard_delivery.rs", 1),
];

const THE_CALL: &str = ".put_text(";

/// The one source file allowed to name the clipboard library.
const THE_ADAPTER: &str = "nkb-gui/src/clipboard.rs";

/// The one manifest allowed to depend on it (besides the workspace's own list
/// of versions, which declares and does not use).
const THE_MANIFEST: &str = "nkb-gui/Cargo.toml";

const THE_LIBRARY: &str = "arboard";

/// Slint elements that carry their own copy and paste.
const TEXT_FIELDS: &[&str] = &["TextInput", "LineEdit", "TextEdit"];

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the workspace root must be two levels above this package")
        .to_path_buf()
}

fn files_with(dir: &Path, extension: &str, into: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            files_with(&path, extension, into);
        } else if path.extension().is_some_and(|ext| ext == extension) {
            into.push(path);
        }
    }
}

/// Files under `crates/*/<sub>` with `extension`, as (relative path, text).
fn under_every_package(sub: &str, extension: &str) -> Vec<(String, String)> {
    let crates = workspace_root().join("crates");
    let mut files = Vec::new();
    for entry in std::fs::read_dir(&crates)
        .expect("crates/ must exist")
        .flatten()
    {
        let dir = entry.path().join(sub);
        if dir.is_dir() {
            files_with(&dir, extension, &mut files);
        }
    }
    files.sort();
    files
        .into_iter()
        .map(|path| {
            let relative = path
                .strip_prefix(&crates)
                .expect("under crates/")
                .to_string_lossy()
                .replace('\\', "/");
            let text = std::fs::read_to_string(&path).expect("a file under crates/ is readable");
            (relative, text)
        })
        .collect()
}

fn manifests() -> Vec<(String, String)> {
    let crates = workspace_root().join("crates");
    let mut found = Vec::new();
    for entry in std::fs::read_dir(&crates)
        .expect("crates/ must exist")
        .flatten()
    {
        let manifest = entry.path().join("Cargo.toml");
        if let Ok(text) = std::fs::read_to_string(&manifest) {
            let name = entry.file_name().to_string_lossy().into_owned();
            found.push((format!("{name}/Cargo.toml"), text));
        }
    }
    found.sort();
    found
}

/// The part of a file that is not a test module and not a line comment.
fn production_code(text: &str) -> String {
    let before_tests = match text.find("#[cfg(test)]") {
        Some(at) => &text[..at],
        None => text,
    };
    without_line_comments(before_tests, "//")
}

fn without_line_comments(text: &str, marker: &str) -> String {
    text.lines()
        .map(|line| match line.find(marker) {
            Some(at) => &line[..at],
            None => line,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn calls_in(text: &str) -> usize {
    production_code(text).matches(THE_CALL).count()
}

fn names_the_library(text: &str) -> bool {
    production_code(text).contains(&format!("{THE_LIBRARY}::"))
}

fn depends_on_the_library(manifest: &str) -> bool {
    without_line_comments(manifest, "#")
        .lines()
        .any(|line| line.trim_start().starts_with(THE_LIBRARY))
}

fn has_a_text_field(view: &str) -> bool {
    let code = without_line_comments(view, "//");
    TEXT_FIELDS.iter().any(|field| {
        code.match_indices(field).any(|(at, _)| {
            // A whole word: `TextInputLike` or `MyTextInput` is not the element.
            let before = code[..at].chars().next_back();
            let after = code[at + field.len()..].chars().next();
            !before.is_some_and(|c| c.is_alphanumeric() || c == '-' || c == '_')
                && !after.is_some_and(|c| c.is_alphanumeric() || c == '-' || c == '_')
        })
    })
}

#[test]
fn the_clipboard_port_is_called_only_through_the_named_doors() {
    let mut problems = Vec::new();
    let mut seen_doors = 0usize;
    for (relative, text) in under_every_package("src", "rs") {
        let calls = calls_in(&text);
        match DOORS.iter().find(|(door, _)| *door == relative) {
            Some((_, allowed)) => {
                seen_doors += 1;
                if calls != *allowed {
                    problems.push(format!(
                        "{relative}: {calls} calls to `put_text`, the door allows {allowed}"
                    ));
                }
            }
            None if calls > 0 => problems.push(format!(
                "{relative}: {calls} calls to `put_text` outside the named doors"
            )),
            None => {}
        }
    }
    assert_eq!(
        seen_doors,
        DOORS.len(),
        "every named door must exist as a file. A renamed file would otherwise \
         silently drop its door and this guard would guard a list of nothing"
    );
    assert!(
        problems.is_empty(),
        "the clipboard is written from unexpected places:\n  {}\n\n\
         Untouchable rule 17: the report block and clipboard mode, nowhere else. \
         Add a door to DOORS on purpose, with the reason.",
        problems.join("\n  ")
    );
}

#[test]
fn only_the_adapter_names_the_clipboard_library() {
    let naming: Vec<String> = under_every_package("src", "rs")
        .into_iter()
        .filter(|(_, text)| names_the_library(text))
        .map(|(relative, _)| relative)
        .collect();
    assert_eq!(
        naming,
        vec![THE_ADAPTER.to_owned()],
        "`{THE_LIBRARY}` reached from outside the adapter writes the clipboard \
         without passing a door"
    );

    let depending: Vec<String> = manifests()
        .into_iter()
        .filter(|(_, text)| depends_on_the_library(text))
        .map(|(relative, _)| relative)
        .collect();
    assert_eq!(
        depending,
        vec![THE_MANIFEST.to_owned()],
        "only the GUI package may depend on `{THE_LIBRARY}` - in any package `nkb` \
         links it would also break D25"
    );
}

#[test]
fn no_view_carries_a_text_field_of_its_own() {
    let with_fields: Vec<String> = under_every_package("ui", "slint")
        .into_iter()
        .filter(|(_, text)| has_a_text_field(text))
        .map(|(relative, _)| relative)
        .collect();
    assert!(
        with_fields.is_empty(),
        "a Slint text field copies and pastes on its own - a clipboard path no \
         Rust call shows: {with_fields:?}"
    );
}

#[test]
fn the_scans_see_what_they_look_for_when_it_is_there() {
    // Negative controls for all three scanners: a guard that found nothing
    // anywhere would pass the tests above for the wrong reason.
    let total: usize = under_every_package("src", "rs")
        .iter()
        .map(|(_, text)| calls_in(text))
        .sum();
    let expected: usize = DOORS.iter().map(|(_, n)| n).sum();
    assert_eq!(
        total, expected,
        "zero calls found means the scanner is blind"
    );

    assert_eq!(
        calls_in("    fn put_text(&self, text: &str)\n"),
        0,
        "a definition"
    );
    assert_eq!(calls_in("  ports.clipboard.put_text(&text)\n"), 1);
    assert_eq!(calls_in("  // ports.clipboard.put_text(&text)\n"), 0);
    assert_eq!(
        calls_in("fn a() {}\n#[cfg(test)]\nmod t { fn b() { c.put_text(\"x\"); } }\n"),
        0,
        "a test module is not a door"
    );

    assert!(names_the_library("let c = arboard::Clipboard::new();\n"));
    assert!(!names_the_library(
        "// arboard::Clipboard is used by the adapter\n"
    ));
    assert!(depends_on_the_library(
        "[dependencies]\narboard.workspace = true\n"
    ));
    assert!(!depends_on_the_library(
        "# arboard is not here\nslint = \"1\"\n"
    ));

    assert!(has_a_text_field("    TextInput { text: root.query; }\n"));
    assert!(has_a_text_field("  field := LineEdit { }\n"));
    assert!(!has_a_text_field("    TextBody { text: root.name; }\n"));
    assert!(!has_a_text_field(
        "    // a TextInput would open a clipboard path\n"
    ));
}
