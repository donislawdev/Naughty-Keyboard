//! The guard that makes screens reuse the vocabulary instead of inventing it.
//!
//! # Why this exists before the first screen
//!
//! Document 13 section 5 gives an order that is counter-intuitive and binding:
//! guard first, dictionary second, screen last. The reason is that a screen
//! written before its guard decides what the guard will have to tolerate, and a
//! guard written afterwards stops being a guard and becomes a repair job.
//!
//! Two failures are being prevented, and they are the same failure seen twice:
//! a session starts with no memory of this project, so it does not know that a
//! button already exists, and it has no way to see that the pixels it produced
//! look wrong. This file answers the first. The screenshot binary answers the
//! second.
//!
//! # The shape of the rule
//!
//! Appearance is DATA and lives in exactly one place - document 13 section 2.1.
//! So the files are layered, and the layering is what is checked:
//!
//! ```text
//!   ui/tokens.slint      the dictionary. The only file allowed literal values.
//!   ui/components/*      the vocabulary. May use raw primitives - that is its job.
//!   ui/components.slint  THE DOOR. Re-exports the tokens and every component.
//!   ui/screens/*         composition only. Imports through the door, nothing else.
//!   ui/all.slint         the build entry point, re-exports screens to Rust.
//! ```
//!
//! # What this guard does NOT catch, written down on purpose
//!
//! Document 13 section 2.2 asks for this list next to the guard, because a guard
//! whose blind spots are unwritten gets trusted for things it never checked.
//!
//! - a named value multiplied by a number: `Tokens.space-2 * 3` passes, and it
//!   is a hole in the closed scale of section 2.3;
//! - a value computed in Rust and pushed in through a property - the literal is
//!   then in a `.rs` file, which this guard does not read;
//! - the right component used in the wrong place. This checks vocabulary, not
//!   meaning, and no guard of this shape ever could;
//! - literals inside `ui/components/*.slint`. Deliberate: components are where
//!   primitives are allowed to live. The line is drawn at the screen boundary;
//! - block comments spanning several lines. Line comments are stripped, `/* */`
//!   is not, so a colour inside one would be reported;
//! - a built-in colour name such as `Colors.red`, which is a literal in spirit
//!   but not in syntax.

// A failed expectation in a test is a failed test. The workspace denies both in
// product code, where a panic lands in someone else's CI.
#![allow(clippy::panic, clippy::expect_used)]

use std::path::{Path, PathBuf};

/// Visual primitives. A screen that reaches for one of these is building a
/// control instead of using one, which is exactly the drift this guard exists
/// to stop. Layout containers are deliberately absent: arranging things is
/// composition, not appearance.
const RAW_PRIMITIVES: &[&str] = &[
    "Rectangle",
    "Text",
    "TouchArea",
    "Image",
    "Path",
    "Flickable",
    "TextInput",
];

/// Properties whose value is a design decision rather than a structural one.
/// Cut narrow on purpose - a guard that flags every number in the file is the
/// guard that gets switched off within a week.
const DESIGN_PROPERTIES: &[&str] = &[
    "padding",
    "padding-left",
    "padding-right",
    "padding-top",
    "padding-bottom",
    "spacing",
    "font-size",
    "letter-spacing",
    "line-height",
    "border-radius",
    "border-width",
    "width",
    "height",
    "min-width",
    "min-height",
    "max-width",
    "max-height",
];

/// Absolute units. `%` is missing on purpose: a percentage is relative to the
/// parent, so it survives a different system scale and a longer translation,
/// which is the actual thing section 2.1 is protecting.
const ABSOLUTE_UNITS: &[&str] = &["px", "pt", "phx", "in", "mm", "cm"];

fn ui_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("ui")
}

/// Everything before a `//` that is not inside a string literal.
fn strip_line_comment(line: &str) -> &str {
    let bytes = line.as_bytes();
    let mut in_string = false;
    let mut i = 0;
    while i + 1 < bytes.len() {
        match bytes[i] {
            b'\\' if in_string => i += 1,
            b'"' => in_string = !in_string,
            b'/' if !in_string && bytes[i + 1] == b'/' => return &line[..i],
            _ => {}
        }
        i += 1;
    }
    line
}

/// A `#` followed only by hex digits is a colour literal in Slint.
fn holds_colour_literal(line: &str) -> bool {
    let bytes = line.as_bytes();
    for (i, b) in bytes.iter().enumerate() {
        if *b != b'#' {
            continue;
        }
        let digits = bytes[i + 1..]
            .iter()
            .take_while(|c| c.is_ascii_hexdigit())
            .count();
        if matches!(digits, 3 | 4 | 6 | 8) {
            return true;
        }
    }
    ["rgb(", "rgba(", "hsv("].iter().any(|f| line.contains(f))
}

/// Every property assignment on the line.
///
/// ⚠️ This returns a list, not one hit, and that is not tidiness. The first
/// version looked at the line as a whole and so saw nothing at all in
/// `HorizontalLayout { padding: 7px; }` - legal Slint, and a mutation of exactly
/// that shape walked past the guard. Assignments are separated by `;` and by the
/// brace that opens an element, so both are split on.
fn assigned_properties(line: &str) -> Vec<(&str, &str)> {
    let mut found = Vec::new();
    for segment in line.split([';', '{', '}']) {
        let trimmed = segment.trim();
        let Some(colon) = trimmed.find(':') else {
            continue;
        };
        // `:=` declares an element, it does not assign a property.
        if trimmed.as_bytes().get(colon + 1) == Some(&b'=') {
            continue;
        }
        let name = trimmed[..colon].trim();
        if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            continue;
        }
        found.push((name, trimmed[colon + 1..].trim()));
    }
    found
}

/// True when the value is a bare number followed by an absolute unit.
fn holds_absolute_length(value: &str) -> bool {
    let bytes = value.as_bytes();
    for (i, _) in bytes.iter().enumerate() {
        if !bytes[i].is_ascii_digit() {
            continue;
        }
        // Only look at the start of a number, so `Tokens.space-2` is not a hit
        // through its digit.
        if i > 0 {
            let prev = bytes[i - 1];
            if prev.is_ascii_alphanumeric() || prev == b'-' || prev == b'.' || prev == b'_' {
                continue;
            }
        }
        let rest = &value[i..];
        let after_digits = rest.trim_start_matches(|c: char| c.is_ascii_digit() || c == '.');
        if ABSOLUTE_UNITS.iter().any(|u| {
            after_digits.starts_with(u)
                && !after_digits[u.len()..].starts_with(|c: char| c.is_ascii_alphanumeric())
        }) {
            return true;
        }
    }
    false
}

/// Every element the line instantiates.
///
/// ⚠️ Also a list, and for the same measured reason: the first version required
/// the line to END with `{`, so `Text { text: "x"; }` on one line was invisible
/// to it. Worse, that hole stayed hidden because the only mutation that reached
/// this rule was `Rectangle { background: #FF0000; }` - which the colour rule
/// caught first, making a dead rule look alive.
fn instantiated_elements(line: &str) -> Vec<&str> {
    let mut found = Vec::new();
    for (i, _) in line.match_indices('{') {
        let head = line[..i].trim_end();
        // `Foo := Bar {` declares a component; `bar := Baz {` names an instance.
        let head = head
            .rsplit_once(":=")
            .map_or(head, |(_, right)| right)
            .trim_start();
        let name = head
            .rsplit(|c: char| c.is_whitespace() || c == ';' || c == '{' || c == '}')
            .next()
            .unwrap_or("");
        if name.is_empty() || !name.starts_with(|c: char| c.is_ascii_uppercase()) {
            continue;
        }
        if !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            continue;
        }
        found.push(name);
    }
    found
}

fn slint_files_in(dir: &Path) -> Vec<PathBuf> {
    if !dir.is_dir() {
        return Vec::new();
    }
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{} must be readable: {e}", dir.display()))
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "slint"))
        .collect();
    out.sort();
    out
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("{} must be readable: {e}", path.display()))
}

/// Reports every complaint at once. One failure per run teaches nothing about
/// the size of the problem.
fn report(problems: Vec<String>) {
    if problems.is_empty() {
        return;
    }
    let mut message = format!("\n{} screen problem(s):\n\n", problems.len());
    for p in &problems {
        message.push_str("  ");
        message.push_str(p);
        message.push('\n');
    }
    message.push_str(
        "\nScreens compose from the vocabulary behind ui/components.slint.\n\
         Appearance lives in ui/tokens.slint and nowhere else - document 13, section 2.1.\n",
    );
    panic!("{message}");
}

#[test]
fn screens_hold_no_appearance_of_their_own() {
    let screens = slint_files_in(&ui_dir().join("screens"));
    assert!(
        !screens.is_empty(),
        "ui/screens holds no .slint file - a guard with nothing to guard passes for the \
         wrong reason, which is indistinguishable from a broken guard"
    );

    let mut problems = Vec::new();
    for path in &screens {
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        for (n, raw) in read(path).lines().enumerate() {
            let line = strip_line_comment(raw);
            let at = format!("{name}:{}", n + 1);

            if holds_colour_literal(line) {
                problems.push(format!("{at}  colour written out: {}", line.trim()));
            }
            for (property, value) in assigned_properties(line) {
                if DESIGN_PROPERTIES.contains(&property) && holds_absolute_length(value) {
                    problems.push(format!(
                        "{at}  `{property}` set to an absolute length: {value}"
                    ));
                }
            }
            for element in instantiated_elements(line) {
                if RAW_PRIMITIVES.contains(&element) {
                    problems.push(format!(
                        "{at}  raw `{element}` - build it once in ui/components/ and use it here"
                    ));
                }
            }
        }
    }
    report(problems);
}

#[test]
fn screens_reach_the_vocabulary_only_through_the_door() {
    let screens = slint_files_in(&ui_dir().join("screens"));
    let mut problems = Vec::new();

    for path in &screens {
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        for (n, raw) in read(path).lines().enumerate() {
            let line = strip_line_comment(raw);
            if !line.trim_start().starts_with("import ") {
                continue;
            }
            let Some(from) = line.split(" from ").nth(1) else {
                continue;
            };
            let target = from.trim().trim_end_matches(';').trim_matches('"');
            if !target.ends_with("components.slint") {
                problems.push(format!(
                    "{name}:{}  imports \"{target}\" - screens see the vocabulary only \
                     through ui/components.slint",
                    n + 1
                ));
            }
        }
    }
    report(problems);
}

/// The door has to list everything, or the next session cannot find it by
/// opening one file - which is the whole reason the door exists.
#[test]
fn the_door_re_exports_every_component() {
    let door_path = ui_dir().join("components.slint");
    let door = read(&door_path);
    let mut problems = Vec::new();

    let components = slint_files_in(&ui_dir().join("components"));
    assert!(
        !components.is_empty(),
        "ui/components holds no .slint file, so this test would pass by having nothing to check"
    );

    for path in &components {
        let stem = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let expected = format!("components/{stem}.slint");
        if !door.contains(&expected) {
            problems.push(format!(
                "ui/components/{stem}.slint exists but ui/components.slint does not import it - \
                 a component nobody can find is a component that gets written again"
            ));
        }
    }

    // And the reverse: the door must not promise what is not there.
    for line in door.lines().map(strip_line_comment) {
        let Some(from) = line.split(" from ").nth(1) else {
            continue;
        };
        let target = from.trim().trim_end_matches(';').trim_matches('"');
        if let Some(stem) = target
            .strip_prefix("components/")
            .and_then(|t| t.strip_suffix(".slint"))
            && !ui_dir()
                .join("components")
                .join(format!("{stem}.slint"))
                .exists()
        {
            problems.push(format!(
                "ui/components.slint imports components/{stem}.slint, which does not exist"
            ));
        }
    }
    report(problems);
}

/// Only the dictionary is allowed to hold a value out of thin air.
#[test]
fn the_dictionary_is_the_only_file_that_may_hold_raw_values() {
    let tokens = ui_dir().join("tokens.slint");
    assert!(
        tokens.is_file(),
        "ui/tokens.slint must exist - it is the one place appearance is allowed to live"
    );
    let body = read(&tokens);
    assert!(
        holds_colour_literal(&body),
        "ui/tokens.slint holds no colour literal at all, which means the dictionary is empty \
         and every other check in this file passes for the wrong reason"
    );
}
