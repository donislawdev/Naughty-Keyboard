//! `KeystrokeSender` is called from the places named here and from nowhere else.
//!
//! # The promise this stands for
//!
//! Untouchable rule 17, fourth promise: the tool does not send keys other than
//! the value's own characters. `ux-spec.md` 4 narrows that to ONE exception,
//! clearing the field, and architektura.md 5 turns the exception into a list
//! of call sites with ONE entry. Until the port existed that row read
//! "nothing" in the column of what goes red; this file is the guard the row
//! promised, written together with the port.
//!
//! ⚠️ Until 2026-09-23 this header said there would be TWO doors, the second
//! being the paste of the clipboard route. `D71` corrected it: clipboard mode
//! presses nothing - the tester pastes - because every reason that leads there
//! also blocks the channel a synthetic paste would take. The clipboard route
//! is a door of `clipboard_has_named_doors.rs`, not of this list.
//!
//! # Why a list of doors rather than a number
//!
//! A bare "at most two" would have let a second call site in for free - and
//! the second door it was holding a slot for turned out never to be one
//! (`D71`). Naming the doors makes any new one a deliberate edit to THIS list,
//! with the commit message that goes with it, rather than a free slot.
//!
//! # What this checks, and what it cannot
//!
//! - it reads `.rs` files under `crates/*/src` (not `tests/`: a test that
//!   calls the port through a spy is not a door into somebody's window);
//! - it drops everything from the first `#[cfg(test)]` line to the end of a
//!   file, because this workspace keeps test modules at the bottom, and it
//!   strips `//` comments. It does NOT strip `/* */` blocks or string
//!   literals; a file using either around the method name would be a false
//!   positive, loudly rather than quietly;
//! - it looks for the CALL shape `.send_keystrokes(`. The definition in
//!   `ports.rs` and the `fn send_keystrokes(` of an implementation have no
//!   leading dot and are not counted;
//! - it counts textual call sites, not execution paths: a helper function that
//!   wrapped the call would be one door with many callers, and this guard would
//!   not see the callers. architektura.md 5 says so of every guard of this
//!   kind.

// A failed expectation in a test is a failed test.
#![allow(clippy::panic, clippy::expect_used)]

use std::path::{Path, PathBuf};

/// The doors, as `crates/<package>/src/<file>` with the number of calls each
/// may hold. One: clearing the field. A second comes only with an automatic
/// paste as a separate route the tester chooses (step 7), and a session adding
/// it edits this list on purpose.
const DOORS: &[(&str, usize)] = &[("nkb-app/src/send_value.rs", 1)];

const THE_CALL: &str = ".send_keystrokes(";

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the workspace root must be two levels above this package")
        .to_path_buf()
}

fn rust_files(dir: &Path, into: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, into);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            into.push(path);
        }
    }
}

/// Source files of every package's `src/`, as (relative path, text).
fn sources() -> Vec<(String, String)> {
    let crates = workspace_root().join("crates");
    let mut files = Vec::new();
    for entry in std::fs::read_dir(&crates)
        .expect("crates/ must exist")
        .flatten()
    {
        let src = entry.path().join("src");
        if src.is_dir() {
            rust_files(&src, &mut files);
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
            let text = std::fs::read_to_string(&path).expect("a source file is readable");
            (relative, text)
        })
        .collect()
}

/// The part of a file that is not a test module and not a line comment.
fn production_code(text: &str) -> String {
    let before_tests = match text.find("#[cfg(test)]") {
        Some(at) => &text[..at],
        None => text,
    };
    before_tests
        .lines()
        .map(|line| match line.find("//") {
            Some(at) => &line[..at],
            None => line,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn calls_in(text: &str) -> usize {
    production_code(text).matches(THE_CALL).count()
}

#[test]
fn the_keystroke_port_is_called_only_through_the_named_doors() {
    let mut problems = Vec::new();
    let mut seen_doors = 0usize;
    for (relative, text) in sources() {
        let calls = calls_in(&text);
        match DOORS.iter().find(|(door, _)| *door == relative) {
            Some((_, allowed)) => {
                seen_doors += 1;
                if calls != *allowed {
                    problems.push(format!(
                        "{relative}: {calls} calls to `send_keystrokes`, the door allows {allowed}"
                    ));
                }
            }
            None if calls > 0 => problems.push(format!(
                "{relative}: {calls} calls to `send_keystrokes` outside the named doors - \
                 the tool may press keys other than the value from exactly the places in DOORS"
            )),
            None => {}
        }
    }
    assert_eq!(
        seen_doors,
        DOORS.len(),
        "every named door must exist as a file; a renamed file would otherwise \
         silently drop its door and this guard would guard a list of nothing"
    );
    assert!(
        problems.is_empty(),
        "KeystrokeSender is called from unexpected places:\n  {}\n\n\
         ux-spec.md 4: clearing the field is the only place the tool presses keys \
         other than the value. Add a door to DOORS on purpose, with the reason.",
        problems.join("\n  ")
    );
}

#[test]
fn the_scan_sees_a_call_when_there_is_one() {
    // Negative control for the scanner itself: a guard that found zero calls
    // everywhere would pass the test above for the wrong reason, so this
    // proves the shape it looks for is the shape the code actually has.
    let total: usize = sources().iter().map(|(_, text)| calls_in(text)).sum();
    let expected: usize = DOORS.iter().map(|(_, n)| n).sum();
    assert_eq!(
        total, expected,
        "the scanner must find exactly the doors' calls; zero means it is blind"
    );
    assert!(expected > 0, "there is at least one door today");
}

#[test]
fn the_definition_and_the_implementations_are_not_counted_as_doors() {
    // `fn send_keystrokes(` has no leading dot; a scanner matching the bare
    // name would count ports.rs and every adapter as a door.
    let sample = "    fn send_keystrokes(&self, chords: &[KeyChord]) -> Result<(), KeystrokeError> {\n        keys.send_keystrokes(&x)\n";
    assert_eq!(calls_in(sample), 1);
    let comment = "    // keys.send_keystrokes(&x) is what the door does\n";
    assert_eq!(calls_in(comment), 0, "a line comment is not a call");
    let in_tests = "fn a() {}\n#[cfg(test)]\nmod t { fn b() { spy.send_keystrokes(&x); } }\n";
    assert_eq!(calls_in(in_tests), 0, "a test module is not a door");
}
