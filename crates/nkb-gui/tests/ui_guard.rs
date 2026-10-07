//! The guard that makes views reuse the vocabulary instead of inventing it,
//! and say nothing of their own.
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
//!   ui/screens/*         product screens. Composition only, and NO SENTENCES.
//!   ui/gallery/*         the catalogue. Composition only, specimen text allowed.
//!   ui/all.slint         the build entry point, re-exports views to Rust.
//! ```
//!
//! # Why the catalogue has a directory of its own
//!
//! Untouchable rule 9 says a sentence shown to a person is a key, never a
//! literal at the place it is shown. The catalogue is the one view that must
//! break that, and not for convenience:
//!
//! - its labels NAME COMPONENTS, in the language the code is written in
//!   (untouchable rule 6). Translated, they stop naming them.
//! - fed from Rust, it would render in `appearance.rs` as a screen of empty
//!   labels - and that off-screen render is the only mechanism this project has
//!   for looking at its own interface.
//! - document 13 section 3 asks the catalogue to carry a very long label and an
//!   empty one BY NAME. Those are the catalogue's data, not the product's
//!   sentences.
//!
//! The exemption is therefore a DIRECTORY rather than a name in a list here. A
//! list of exempt files is a door that opens outward - the second catalogue gets
//! added to it, because that is what the list is for. A directory cannot be
//! widened without physically moving a file out of `ui/screens/`, which is
//! visible in any diff and impossible to do by accident. D58.
//!
//! ⚠️ `ui/screens/` may be EMPTY and the run still means something: every rule
//! below either scans the union of the two view directories, or scans the whole
//! of `ui/` minus the catalogue - which holds the dictionary, the door and the
//! components from day one. The sentence rules additionally carry their own
//! positive and negative control, so "nothing to scan" can never be mistaken for
//! "nothing to report".
//!
//! # What this guard does NOT catch, written down on purpose
//!
//! Document 13 section 2.2 asks for this list next to the guard, because a guard
//! whose blind spots are unwritten gets trusted for things it never checked.
//!
//! - a named value multiplied by a number: `Tokens.space-2 * 3` passes, and it
//!   is a hole in the closed scale of section 2.3.
//! - a value computed in Rust and pushed in through a property - the literal is
//!   then in a `.rs` file, and only SENTENCES are followed there, not lengths.
//! - the right component used in the wrong place. This checks vocabulary, not
//!   meaning, and no guard of this shape ever could.
//! - appearance literals inside `ui/components/*.slint`. Deliberate: components
//!   are where primitives are allowed to live. Sentences are NOT - see below.
//! - block comments spanning several lines. Line comments are stripped, `/* */`
//!   is not, so a colour or a sentence inside one would be reported. That is the
//!   loud direction, not the silent one.
//! - a built-in colour name such as `Colors.red`, which is a literal in spirit
//!   but not in syntax.
//! - on the Rust side: a sentence assembled into a variable and then pushed, a
//!   raw string (`r"..."`), and a char literal holding a quote (`'"'`). All three
//!   are absent from this crate today and all three would walk past.
//!
//! # The two sentence rules, and why there are two
//!
//! A guard that closes one of two doors is worse than one that closes neither,
//! because it looks like a complete answer. Refusing a literal in `.slint` moves
//! the sentence to `palette.set_title("...")` in Rust, which is the same
//! violation one file away. So both are checked, and `nkb-adapters::i18n` is the
//! only place a sentence is allowed to be written down.

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

fn src_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

/// The catalogue, the one view allowed to write its own labels.
fn catalogue_dir() -> PathBuf {
    ui_dir().join("gallery")
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
        // `Foo := Bar {` declares a component. `bar := Baz {` names an instance.
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

// ---------------------------------------------------------------------------
// Sentences
// ---------------------------------------------------------------------------

/// The contents of every quoted string on the line.
///
/// Escapes are honoured, so `"say \"no\""` is ONE literal rather than two plus a
/// stray. An unterminated quote is left to the Slint compiler, which names the
/// file and the line far better than this could.
fn string_literals(line: &str) -> Vec<&str> {
    let bytes = line.as_bytes();
    let mut found = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'"' {
            i += 1;
            continue;
        }
        let start = i + 1;
        let mut end = start;
        while end < bytes.len() && bytes[end] != b'"' {
            end += if bytes[end] == b'\\' { 2 } else { 1 };
        }
        if end >= bytes.len() {
            break;
        }
        found.push(&line[start..end]);
        i = end + 1;
    }
    found
}

/// The line with the path taken out of every `@image-url("...")`.
///
/// A path to a file is not a sentence - it is the other thing besides an import
/// that a `.slint` file names in quotes, and it is let through for the same
/// reason. Only the path goes: the rest of the line stays, so a sentence written
/// beside an asset is still a sentence, and a call whose argument is not a
/// plain literal is left whole for the rule to read.
fn without_asset_paths(line: &str) -> String {
    const CALL: &str = "@image-url(";
    let mut kept = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(at) = rest.find(CALL) {
        let after_call = at + CALL.len();
        kept.push_str(&rest[..after_call]);
        rest = &rest[after_call..];
        let spaces = rest.len() - rest.trim_start().len();
        kept.push_str(&rest[..spaces]);
        rest = &rest[spaces..];
        let Some(inside) = rest.strip_prefix('"') else {
            continue;
        };
        let bytes = inside.as_bytes();
        let mut end = 0;
        while end < bytes.len() && bytes[end] != b'"' {
            end += if bytes[end] == b'\\' { 2 } else { 1 };
        }
        if end >= bytes.len() {
            // An unterminated quote is the Slint compiler's to report.
            break;
        }
        rest = &inside[end + 1..];
    }
    kept.push_str(rest);
    kept
}

/// The name a line binds a value to, whether it declares a property or assigns
/// one: `out property <string> font-mono: "x"` binds `font-mono`, `text: "x"`
/// binds `text`.
fn bound_name(line: &str) -> Option<&str> {
    let colon = line.find(':')?;
    // `:=` declares an element, it does not bind a value to a name.
    if line.as_bytes().get(colon + 1) == Some(&b'=') {
        return None;
    }
    line[..colon]
        .rsplit(|c: char| c.is_whitespace() || c == '<' || c == '>')
        .find(|piece| !piece.is_empty())
}

/// True for a line that imports, which is the one place a `.slint` file names a
/// path rather than a sentence.
///
/// ⚠️ The trailing check is not decoration: `important: "x"` starts with the
/// word `import`, and a prefix test alone would wave it through.
fn is_import(line: &str) -> bool {
    line.trim_start()
        .strip_prefix("import")
        .is_some_and(|rest| {
            !rest.starts_with(|c: char| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        })
}

/// Sentences written into a `.slint` file, as (line number, text).
///
/// A pure function over the file's text, and that is deliberate: it can be shown
/// failing without a screen to fail on, which is what keeps the rule honest
/// while `ui/screens/` is still empty.
///
/// `allow_font_names` is true only for the dictionary. A typeface family IS a
/// design value and the dictionary is where design values live - but the
/// allowance is bound to the NAME, so `label-next: "Next"` in `tokens.slint` is
/// still a sentence hidden in the one file nobody re-reads.
fn sentences_written_into(body: &str, allow_font_names: bool) -> Vec<(usize, String)> {
    let mut found = Vec::new();
    // ⚠️ An import is a STATEMENT, not a line, and the first version of this
    // rule forgot it. `ui/components.slint` lists nine names one per line and
    // closes with `} from "components/typography.slint";` - so the path sat on a
    // line that does not begin with `import`, and the guard reported the door
    // itself. Measured the moment the rule first ran, which is the argument for
    // running a new guard against the tree before believing an estimate of its
    // false alarms.
    let mut inside_import = false;
    for (n, raw) in body.lines().enumerate() {
        let line = strip_line_comment(raw);
        if !inside_import && is_import(line) {
            inside_import = true;
        }
        if inside_import {
            inside_import = !line.contains(';');
            continue;
        }
        if allow_font_names && bound_name(line).is_some_and(|name| name.starts_with("font-")) {
            continue;
        }
        let line = without_asset_paths(line);
        for literal in string_literals(&line) {
            found.push((n + 1, literal.to_owned()));
        }
    }
    found
}

/// Sentences handed to the interface from Rust, as (line number, text).
///
/// Narrow by construction: only a string literal INSIDE a `set_*(...)` call
/// counts, because that is the one way a word reaches a Slint property. An
/// ordinary literal - a path, a panic message, a test fixture - is none of this
/// rule's business.
///
/// The scan crosses newlines on purpose. `rustfmt` breaks a long call over
/// several lines, and a rule that only ever looked at one line would go quiet on
/// exactly the long sentences it most needs to see.
fn sentences_pushed_from(body: &str) -> Vec<(usize, String)> {
    let stripped: String = body
        .lines()
        .map(strip_line_comment)
        .collect::<Vec<_>>()
        .join("\n");
    let mut found = Vec::new();

    for (at, _) in stripped.match_indices("set_") {
        // `reset_foo(` is not a setter.
        if at > 0
            && stripped[..at]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            continue;
        }
        let after = &stripped[at + "set_".len()..];
        let name_len = after
            .bytes()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == b'_')
            .count();
        let tail = &after[name_len..];
        if !tail.starts_with('(') {
            continue;
        }

        let mut depth = 0_usize;
        let mut walk = tail.char_indices();
        while let Some((offset, c)) = walk.next() {
            match c {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                '"' => {
                    let opened = at + "set_".len() + name_len + offset;
                    let literal = string_literals(&stripped[opened..])
                        .first()
                        .map_or_else(String::new, |s| (*s).to_owned());
                    let line = stripped[..opened].matches('\n').count() + 1;
                    found.push((line, literal));
                    break;
                }
                '\\' => {
                    walk.next();
                }
                _ => {}
            }
        }
    }
    found
}

// ---------------------------------------------------------------------------
// Walking the tree
// ---------------------------------------------------------------------------

fn files_in(dir: &Path, extension: &str) -> Vec<PathBuf> {
    if !dir.is_dir() {
        return Vec::new();
    }
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{} must be readable: {e}", dir.display()))
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == extension))
        .collect();
    out.sort();
    out
}

fn files_under(dir: &Path, extension: &str) -> Vec<PathBuf> {
    if !dir.is_dir() {
        return Vec::new();
    }
    let mut out = files_in(dir, extension);
    let mut sub: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{} must be readable: {e}", dir.display()))
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    sub.sort();
    for child in sub {
        out.extend(files_under(&child, extension));
    }
    out
}

/// Both view directories. The rules about appearance apply to the catalogue
/// exactly as they do to a product screen - only its LABELS are its own.
fn composed_views() -> Vec<PathBuf> {
    let mut out = files_in(&ui_dir().join("screens"), "slint");
    out.extend(files_in(&catalogue_dir(), "slint"));
    out
}

/// Every `.slint` file the product ships except the catalogue's.
fn files_that_may_not_speak() -> Vec<PathBuf> {
    files_under(&ui_dir(), "slint")
        .into_iter()
        .filter(|p| !p.starts_with(catalogue_dir()))
        .collect()
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("{} must be readable: {e}", path.display()))
}

fn shown(path: &Path) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

/// Reports every complaint at once. One failure per run teaches nothing about
/// the size of the problem.
fn report(problems: Vec<String>, closing: &str) {
    if problems.is_empty() {
        return;
    }
    let mut message = format!("\n{} problem(s):\n\n", problems.len());
    for p in &problems {
        message.push_str("  ");
        message.push_str(p);
        message.push('\n');
    }
    message.push('\n');
    message.push_str(closing);
    panic!("{message}");
}

const APPEARANCE_RULE: &str = "Views compose from the vocabulary behind ui/components.slint.\n\
     Appearance lives in ui/tokens.slint and nowhere else - document 13, section 2.1.\n";

const SENTENCE_RULE: &str = "A sentence shown to a person is a KEY, never a literal where it is shown -\n\
     untouchable rule 9. The sentences live in nkb-adapters::i18n, keyed by the\n\
     app type that carries them, and reach the view through a property.\n\
     Specimen labels belong in ui/gallery/, which is exempt by directory - D58.\n";

#[test]
fn views_hold_no_appearance_of_their_own() {
    let views = composed_views();
    assert!(
        !views.is_empty(),
        "neither ui/screens nor ui/gallery holds a .slint file - a guard with nothing to \
         guard passes for the wrong reason, which is indistinguishable from a broken guard"
    );

    let mut problems = Vec::new();
    for path in &views {
        let name = shown(path);
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
    report(problems, APPEARANCE_RULE);
}

#[test]
fn views_reach_the_vocabulary_only_through_the_door() {
    let mut problems = Vec::new();

    for path in &composed_views() {
        let name = shown(path);
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
                    "{name}:{}  imports \"{target}\" - views see the vocabulary only \
                     through ui/components.slint",
                    n + 1
                ));
            }
        }
    }
    report(problems, APPEARANCE_RULE);
}

/// The door has to list everything, or the next session cannot find it by
/// opening one file - which is the whole reason the door exists.
#[test]
fn the_door_re_exports_every_component() {
    let door_path = ui_dir().join("components.slint");
    let door = read(&door_path);
    let mut problems = Vec::new();

    let components = files_in(&ui_dir().join("components"), "slint");
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
    report(problems, APPEARANCE_RULE);
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

/// Untouchable rule 9, on the interface side of the seam.
#[test]
fn views_hold_no_sentence_of_their_own() {
    let files = files_that_may_not_speak();
    assert!(
        !files.is_empty(),
        "no .slint file outside ui/gallery - this rule would then pass by having nothing to read"
    );

    let dictionary = ui_dir().join("tokens.slint");
    let mut problems = Vec::new();
    for path in &files {
        let name = shown(path);
        for (line, text) in sentences_written_into(&read(path), *path == dictionary) {
            problems.push(format!("{name}:{line}  text written out: \"{text}\""));
        }
    }
    report(problems, SENTENCE_RULE);
}

/// Untouchable rule 9, on the Rust side of the same seam.
///
/// Without this, refusing a literal in `.slint` would simply move it to
/// `window.set_title("...")` - the same violation, one file away, and the guard
/// would look complete while the rule had a hole the width of a crate.
#[test]
fn no_sentence_reaches_the_interface_as_a_rust_literal() {
    let files = files_under(&src_dir(), "rs");
    assert!(
        !files.is_empty(),
        "crates/nkb-gui/src holds no .rs file, so this rule would have nothing to read"
    );

    let mut problems = Vec::new();
    for path in &files {
        let name = shown(path);
        for (line, text) in sentences_pushed_from(&read(path)) {
            problems.push(format!(
                "{name}:{line}  sentence handed to a property as a literal: \"{text}\""
            ));
        }
    }
    report(problems, SENTENCE_RULE);
}

/// The control that makes the two rules above mean something.
///
/// 🔴 `ui/screens/` is legitimately empty until the palette lands, so those
/// rules would then be reading only files that never held a sentence. A rule
/// that has never been seen firing is indistinguishable from a broken one -
/// document 05 section 2 - and this project has twice measured a guard that was
/// green for the wrong reason. So both directions are checked here directly,
/// against text rather than against the tree.
#[test]
fn the_sentence_rules_can_actually_fail() {
    // --- positive: it fires ------------------------------------------------
    let screen = "import { TextBody } from \"../components.slint\";\n\
                  export component Palette inherits Window {\n\
                  \x20   TextBody { text: \"End of pack\"; }\n\
                  }\n";
    let caught = sentences_written_into(screen, false);
    assert_eq!(
        caught.len(),
        1,
        "the .slint rule must report exactly the sentence and not the import path: {caught:?}"
    );
    assert_eq!(caught[0], (3, "End of pack".to_owned()));

    let rust = "fn wire(p: &Palette) {\n    p.set_message(\n        \"End of pack\",\n    );\n}\n";
    let pushed = sentences_pushed_from(rust);
    assert_eq!(
        pushed.len(),
        1,
        "the Rust rule must follow a setter across the line break rustfmt puts in: {pushed:?}"
    );
    assert_eq!(pushed[0], (3, "End of pack".to_owned()));

    // --- negative: it stays quiet where it must ----------------------------
    // A guard that fires on everything is a guard switched off within a week -
    // document 13 section 2.2.
    let quiet = "import { TextBody } from \"../components.slint\";\n\
                 import \"fonts/DejaVuSansMono.ttf\";\n\
                 export component Palette inherits Window {\n\
                 \x20   TextBody { text: root.message; }\n\
                 }\n";
    assert!(
        sentences_written_into(quiet, false).is_empty(),
        "an import path and a bound property are not sentences"
    );
    assert!(
        sentences_written_into(
            "    out property <string> font-mono: \"DejaVu Sans Mono\";",
            true
        )
        .is_empty(),
        "the dictionary may name a typeface"
    );
    assert!(
        !sentences_written_into("    out property <string> label-next: \"Next\";", true).is_empty(),
        "the dictionary allowance is bound to the NAME, or it is a hiding place"
    );
    assert!(
        sentences_written_into("    important: \"x\";", false).len() == 1,
        "a property whose name begins with the word `import` is not an import line"
    );
    // The door's own shape, and the false alarm this rule was born with.
    let door = "import {\n    TextName,\n    TextBody,\n} from \"components/typography.slint\";\n\
                export component Palette inherits Window {\n\
                \x20   TextBody { text: root.message; }\n\
                }\n";
    assert!(
        sentences_written_into(door, false).is_empty(),
        "an import is a statement, not a line - the path may sit under the names it brings in"
    );
    // A path to an asset is a name of a file, like an import. The icon is the
    // first one, and the rule had to learn the difference without going blind:
    // the sentence beside a path, and a path that is not the whole argument,
    // are still read.
    let icon = "    out property <image> app-icon: @image-url(\"../assets/edamame.svg\");";
    assert!(
        sentences_written_into(icon, true).is_empty()
            && sentences_written_into(icon, false).is_empty(),
        "an asset path is not a sentence, in the dictionary or anywhere else"
    );
    assert_eq!(
        sentences_written_into(
            "    Row { text: \"Next\";\n    icon: @image-url( \"../a.svg\" ); }",
            false
        ),
        vec![(1, "Next".to_owned())],
        "a sentence written beside an asset path is still a sentence"
    );
    assert_eq!(
        sentences_written_into(
            "    Row { icon: @image-url(\"a.svg\", nine-slice(1 2));\n    text: \"Next\"; }",
            false
        ),
        vec![(2, "Next".to_owned())],
        "only the path goes: what else the call carries is read as before"
    );
    assert_eq!(
        sentences_written_into("    text: \"see @image-url(\\\"x\\\")\";", false).len(),
        1,
        "a sentence that only talks about the call is still a sentence"
    );
    assert!(
        sentences_pushed_from("fn wire(p: &Palette) { p.set_message(sentence); }").is_empty(),
        "a value handed over as a variable came from the dictionary of sentences"
    );
    assert!(
        sentences_pushed_from("fn reset(p: &Palette) { p.reset_message(\"x\"); }").is_empty(),
        "`reset_` is not a setter"
    );
    assert!(
        sentences_pushed_from("let path = \"ui/all.slint\";").is_empty(),
        "an ordinary literal is none of this rule's business"
    );
}

// ---------------------------------------------------------------------------
// The frame every window stands in
// ---------------------------------------------------------------------------

const FRAME_RULE: &str = "A window of the product stands in the frame: `WindowFrame` with a\n\
     `WindowHeader`, its sections, and the footer and status line from\n\
     ui/components/frame.slint - D110, the owner's point 4 of 2026-10-07. The\n\
     door does not hand out `ChromeBand`, so a window cannot build chrome of its own.\n";

/// What a screen file lacks to stand in the frame, one complaint per missing
/// piece: as many `WindowFrame`s and `WindowHeader`s as it declares windows.
///
/// Counted rather than matched window by window: a screen file holds one
/// window today, and a count that comes out short for a second one is the
/// loud direction.
fn frame_problems(name: &str, body: &str) -> Vec<String> {
    let (mut windows, mut frames, mut headers) = (0, 0, 0);
    for raw in body.lines() {
        let line = strip_line_comment(raw);
        let elements = instantiated_elements(line);
        if line.contains("inherits") && elements.contains(&"Window") {
            windows += 1;
        }
        frames += elements.iter().filter(|e| **e == "WindowFrame").count();
        headers += elements.iter().filter(|e| **e == "WindowHeader").count();
    }
    let mut problems = Vec::new();
    if frames < windows {
        problems.push(format!(
            "{name}  declares {windows} window(s) and stands {frames} of them in a WindowFrame"
        ));
    }
    if headers < windows {
        problems.push(format!(
            "{name}  declares {windows} window(s) and gives {headers} of them a WindowHeader"
        ));
    }
    problems
}

/// Whether the door hands out the band chrome is built from. Comments may name
/// it - the door says why it does not.
fn door_hands_out_chrome(door: &str) -> bool {
    door.lines()
        .map(strip_line_comment)
        .any(|line| line.contains("ChromeBand"))
}

/// The owner's point 4 of 2026-10-07: four windows, built one at a time, each
/// copying the last one remembered, looked like four programs - and more
/// windows are coming. So every window of the product is assembled from the
/// frame's pieces, and this refuses one that is not (`D110`).
///
/// Carries its own controls, for the reason `the_sentence_rules_can_actually_fail`
/// gives: a rule never seen firing cannot be told from a broken one.
#[test]
fn every_window_stands_in_the_frame() {
    // --- the controls: it fires, and it stays quiet --------------------------
    let bare = "export component Spare inherits Window {\n    VerticalLayout { }\n}\n";
    assert_eq!(
        frame_problems("bare", bare).len(),
        2,
        "a window with no frame and no header must be reported twice"
    );
    let framed = "export component Spare inherits Window {\n\
                  \x20   WindowFrame {\n\
                  \x20       WindowHeader { title: root.heading; }\n\
                  \x20   }\n\
                  }\n";
    assert!(
        frame_problems("framed", framed).is_empty(),
        "a window in the frame was reported"
    );
    let named = "export component Spare inherits Window {\n\
                 \x20   body := WindowFrame {\n\
                 \x20       header := WindowHeader { }\n\
                 \x20   }\n\
                 }\n";
    assert!(
        frame_problems("named", named).is_empty(),
        "a frame and a header given names were not recognised"
    );
    assert!(
        door_hands_out_chrome("import { ChromeBand } from \"components/chrome.slint\";"),
        "a door importing ChromeBand was not noticed"
    );
    assert!(
        !door_hands_out_chrome("// `ChromeBand` is deliberately NOT re-exported"),
        "a comment naming ChromeBand was taken for an export"
    );

    // --- the product -----------------------------------------------------------
    let screens = files_in(&ui_dir().join("screens"), "slint");
    let mut windows = 0;
    let mut problems = Vec::new();
    for path in &screens {
        let body = read(path);
        windows += body
            .lines()
            .map(strip_line_comment)
            .filter(|line| {
                line.contains("inherits") && instantiated_elements(line).contains(&"Window")
            })
            .count();
        problems.extend(frame_problems(&shown(path), &body));
    }
    assert!(
        windows >= 4,
        "ui/screens declares {windows} window(s) - the palette, the welcome, the shortcuts and \
         the value window make four, so the rule is reading the wrong files"
    );
    if door_hands_out_chrome(&read(&ui_dir().join("components.slint"))) {
        problems.push(
            "components.slint  hands out ChromeBand - a screen could build a header of its own"
                .to_owned(),
        );
    }
    report(problems, FRAME_RULE);
}
