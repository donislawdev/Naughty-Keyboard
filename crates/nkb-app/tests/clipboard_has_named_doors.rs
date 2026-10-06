//! The clipboard is written from the places named here and from nowhere else.
//!
//! # The promise this stands for
//!
//! Untouchable rule 17, third promise: the tool uses the clipboard in exactly
//! three places - copying the report block, clipboard mode delivering a value
//! (for one window with higher privileges too), and the palette's Copy buttons -
//! and only after a press or a click of the tester's (`D98`, two places until
//! then). architektura.md 5 turns that into a count, and until the report block
//! arrived the row read "nothing" in the column of what goes red, because there
//! was no port. This file is the guard that row promised, written together with
//! the port, as the row said it would be.
//!
//! # Doors, and the places that take a door
//!
//! The port has two DOORS - calls to `put_text`: the report block in `app`, and
//! the clipboard route in `nkb-adapters`, which presses no key. Three PLACES use
//! them, and two of the places share the second door: clipboard mode and the
//! Copy buttons both take the clipboard route, so a value copied by hand stays
//! out of the history and off the cloud exactly like one of clipboard mode, and
//! a value the clipboard cannot carry is refused in one place.
//!
//! A shared door is where a count of doors stops seeing places - the route is a
//! helper with callers. So the route has its own count: the field that lends it,
//! `Ports::by_clipboard`, is read only where the three places live, and the
//! route is built once, where the palette builds its ports.
//!
//! # Four ways round a port, and one check for each
//!
//! 1. the port - `.put_text(` called from the named doors and nowhere else.
//! 2. the route - `.by_clipboard` read only by the named places, and
//!    `ClipboardDelivery::new(` written only where the ports are built. A fourth
//!    place taking the route would otherwise pass the first check untouched.
//! 3. the library - `arboard` named in ONE manifest and ONE source file, the
//!    adapter. Another package reaching for it directly would write the
//!    clipboard without passing a door.
//! 4. the toolkit - Slint copies and pastes on its own inside a text field.
//!    No view has one, and a view gaining one opens a clipboard path no Rust
//!    call shows. The pack search (step 7) was built to this rule rather than
//!    given an exception to it: its query line takes key presses in a focus
//!    scope and draws the query as plain text (`OBS-145`, `D85`).
//!
//! # What this checks, and what it cannot
//!
//! The same limits as `keystrokes_have_named_doors.rs`: text, not execution
//! paths. `src/` only. Test modules and `//` comments dropped - and a module
//! its package declares only under `#[cfg(test)]` is a test module as a whole.
//! A local holding the route under another name and called directly would not
//! be seen: the check counts the field read and the construction, not every
//! later call on what they produce.

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

/// The places that take the clipboard route, as `crates/<package>/src/<file>`
/// with the number of reads of [`THE_ROUTE`] each may hold: the sequence, where
/// clipboard mode sends a value, the window with higher privileges gets the
/// same value in the same press (`D72`), and a Copy button copies one (`D98`).
const ROUTE_TAKERS: &[(&str, usize)] = &[("nkb-app/src/advance_sequence.rs", 3)];

/// The field of `Ports` that lends the clipboard route.
const THE_ROUTE: &str = ".by_clipboard";

/// Where the clipboard route is built, and how many times: once, with the
/// palette's ports.
const ROUTE_BUILDERS: &[(&str, usize)] = &[("nkb-gui/src/live.rs", 1)];

const THE_BUILD: &str = "ClipboardDelivery::new(";

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

/// Whether the package's `lib.rs` declares this file's module only under
/// `#[cfg(test)]`, which makes the whole file a test module - the shared fakes
/// lend the route to tests, and that is not a place in the product.
fn declared_for_tests_only(relative: &str, files: &[(String, String)]) -> bool {
    let Some((package, module)) = relative.split_once("/src/") else {
        return false;
    };
    let Some(stem) = module
        .strip_suffix(".rs")
        .filter(|stem| !stem.contains('/'))
    else {
        return false;
    };
    let lib = format!("{package}/src/lib.rs");
    let declaration = format!("mod {stem};");
    files
        .iter()
        .find(|(file, _)| *file == lib)
        .is_some_and(|(_, text)| {
            let lines: Vec<&str> = text.lines().map(str::trim).collect();
            lines
                .windows(2)
                .any(|pair| pair[0] == "#[cfg(test)]" && pair[1].ends_with(&declaration))
        })
}

/// What one count of `needle` per file came to against `named`: the problems,
/// and how many named files were seen at all.
fn count_against(
    files: &[(String, String)],
    named: &[(&str, usize)],
    needle: &str,
) -> (Vec<String>, usize) {
    let mut problems = Vec::new();
    let mut seen = 0usize;
    for (relative, text) in files {
        if declared_for_tests_only(relative, files) {
            continue;
        }
        let found = production_code(text).matches(needle).count();
        match named.iter().find(|(place, _)| *place == relative) {
            Some((_, allowed)) => {
                seen += 1;
                if found != *allowed {
                    problems.push(format!(
                        "{relative}: {found} of `{needle}`, the list allows {allowed}"
                    ));
                }
            }
            None if found > 0 => problems.push(format!(
                "{relative}: {found} of `{needle}` outside the named places"
            )),
            None => {}
        }
    }
    (problems, seen)
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
         Untouchable rule 17: the report block, clipboard mode and the Copy \
         buttons, nowhere else. Add a door to DOORS on purpose, with the reason.",
        problems.join("\n  ")
    );
}

#[test]
fn the_clipboard_route_is_taken_only_by_the_named_places_and_built_once() {
    let files = under_every_package("src", "rs");
    let mut problems = Vec::new();
    for (named, needle) in [(ROUTE_TAKERS, THE_ROUTE), (ROUTE_BUILDERS, THE_BUILD)] {
        let (found, seen) = count_against(&files, named, needle);
        assert_eq!(
            seen,
            named.len(),
            "every named place of `{needle}` must exist as a file. A renamed file \
             would otherwise drop its place and this would guard nothing"
        );
        problems.extend(found);
    }
    assert!(
        problems.is_empty(),
        "the clipboard route is reached from unexpected places:\n  {}\n\n\
         Untouchable rule 17: three places use the clipboard (D98). A fourth \
         taking the route is a fourth place, whatever door it passes - add it \
         to ROUTE_TAKERS on purpose, with a decision behind it.",
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

    // The route: the reads and the construction the workspace holds today, a
    // read written into a file, and the one test-only module told apart.
    let files = under_every_package("src", "rs");
    for (named, needle) in [(ROUTE_TAKERS, THE_ROUTE), (ROUTE_BUILDERS, THE_BUILD)] {
        let found: usize = files
            .iter()
            .filter(|(relative, _)| !declared_for_tests_only(relative, &files))
            .map(|(_, text)| production_code(text).matches(needle).count())
            .sum();
        let expected: usize = named.iter().map(|(_, n)| n).sum();
        assert_eq!(found, expected, "`{needle}`: zero found means a blind scan");
    }
    let extra = vec![(
        "nkb-gui/src/elsewhere.rs".to_owned(),
        "fn f(ports: &Ports) { let _ = ports.by_clipboard.deliver(\"x\", &mut |_| {}); }\n"
            .to_owned(),
    )];
    let (problems, _) = count_against(&extra, ROUTE_TAKERS, THE_ROUTE);
    assert_eq!(problems.len(), 1, "a fourth place is seen: {problems:?}");
    let declared = vec![(
        "p/src/lib.rs".to_owned(),
        "pub mod shown;\n#[cfg(test)]\npub(crate) mod fakes;\n".to_owned(),
    )];
    assert!(declared_for_tests_only("p/src/fakes.rs", &declared));
    assert!(!declared_for_tests_only("p/src/shown.rs", &declared));
    assert!(!declared_for_tests_only("p/src/absent.rs", &declared));
    assert!(
        declared_for_tests_only("nkb-app/src/test_support.rs", &files),
        "the shared fakes are recognised as the test module they are"
    );
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
