//! The shape of the code, held to ceilings that may only come down.
//!
//! # Two halves, and this is the second
//!
//! How long a function is, how much it branches, how deep it nests and how many
//! arguments it takes is measured by clippy, and `.github/scripts/shape_gate.py`
//! asks it, for the product and for all the code. Nothing here measures a
//! function again - a second metric beside clippy's would be a second thing to
//! get wrong.
//!
//! What this file guards is everything around those numbers that no clippy run
//! can see:
//!
//! - that every package still switches the four lints on, the two that keep a
//!   lint table of their own included,
//! - that the four are named alike in every place that lists them,
//! - that CI still asks for them, on every system, and pins them on Windows,
//! - that the ways out of the lints only ever get rarer.
//!
//! A threshold that is configured and never asked is a gate nobody runs, and a
//! gate that has vanished cannot fail to announce itself.
//!
//! # The one axis clippy has no lint for
//!
//! The size of a file, measured here. A file is a unit a person opens and has to
//! hold, and splitting a function does not make the file around it any smaller.
//!
//! The unit is a line that holds code. A blank line and a comment are free, so
//! the cheapest way back under a ceiling is never to delete the paragraph that
//! says why something is the way it is. A line inside a string literal is code,
//! because it is: the data a test feeds in and the words a message prints. The
//! lexer below knows Rust's comments, nested block comments included, its strings,
//! raw strings, and character literals apart from lifetimes. A metric that miscounts
//! is worse than none, because the suite is green and somebody believes it, so the
//! metric has tests of its own at the end of this file.
//!
//! Three ceilings, each with a count of the files near it:
//!
//! - the part of a Rust file under `src` before its test module, which is what
//!   the programs are made of,
//! - a whole Rust file, its tests included,
//! - a Slint file, except the gallery. The gallery grows with every component by
//!   design (it has to show each one in every state), so a ceiling on it would be
//!   a ceiling on the rule that keeps the components honest.
//!
//! The four tables generated from the Unicode data and from the typeface are data,
//! not code, and are not measured. Each still has to say that it is generated.
//!
//! The numbers live in `.github/shape.toml`. Each must BE today's measurement:
//! a ceiling above the largest file grants room nobody decided to grant. So when
//! the largest file shrinks, this fails until its number comes down too. That is
//! the routine door, and it is meant to be routine. Up is the owner's decision.

// A failed expectation in a test is a failed test.
#![allow(clippy::panic, clippy::expect_used)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use toml_edit::{DocumentMut, Item};

/// The four lints, as the manifests name them.
///
/// Recorded here as well as in the manifests, clippy.toml, shape.toml and the
/// gate. Growing the list means adding it in all of them. Shrinking it reddens,
/// which is the point: nothing else stops a change from deleting a lint to make
/// a red build green, and the check that vanished cannot fail.
const SHAPE_LINTS: [&str; 4] = [
    "too_many_lines",
    "cognitive_complexity",
    "excessive_nesting",
    "too_many_arguments",
];

/// The packages that keep a lint table of their own, because Cargo's lint
/// inheritance is all or nothing. Each has its reason in its manifest.
const OWN_LINT_TABLES: [&str; 2] = ["crates/nkb-gui", "crates/nkb-sys"];

/// How many times each way out of a shape lint stands in code, outside comments.
///
/// The four lints by name, and the groups that would silence them: `all` and
/// `complexity` hold nesting and arguments, `pedantic` holds length,
/// `restriction` holds complexity. The ones are the module of code generated from
/// the Slint files in nkb-gui/src/lib.rs, which nobody writes and nobody can
/// edit. `type_complexity` is not a shape lint here, and its two ways out are on
/// thread-start signatures in nkb-sys/src/hotkey.rs. It is counted for the same
/// reason as the others.
///
/// Pinned exactly, not bounded: a count left above the truth grants a free
/// allowance nobody decided to grant. Fewer means lowering the number in the
/// same change. More means a signature or a block reached for an allowance
/// instead of getting smaller.
const WAYS_OUT: [(&str, usize); 9] = [
    ("too_many_lines", 0),
    ("cognitive_complexity", 1),
    ("excessive_nesting", 0),
    ("too_many_arguments", 0),
    ("all", 1),
    ("complexity", 0),
    ("pedantic", 1),
    ("restriction", 0),
    ("type_complexity", 2),
];

/// Files generated from data. Not measured, because they are data, and checked
/// for still saying so, so that this list cannot hide a file somebody writes.
const GENERATED: [&str; 4] = [
    "crates/nkb-core/src/graphemes/table.rs",
    "crates/nkb-core/src/ignorable/table.rs",
    "crates/nkb-core/src/normalization/table.rs",
    "crates/nkb-gui/src/typeface/table.rs",
];

/// The first words of a generated file.
const GENERATED_MARK: &str = "//! Generated from ";

/// The gallery of components, which grows with every component by design.
const GALLERY: &str = "crates/nkb-gui/ui/gallery/";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the workspace root is two levels above this package")
        .to_path_buf()
}

fn read(relative: &str) -> String {
    let path = root().join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

fn document(relative: &str) -> DocumentMut {
    read(relative)
        .parse()
        .unwrap_or_else(|e| panic!("{relative} is not valid TOML: {e}"))
}

/// The item at `path`, or `None` where any step of it is missing. Indexing a
/// `toml_edit` document panics on a missing key, which here would read as a
/// broken test rather than as the finding it is.
fn at<'a>(document: &'a DocumentMut, path: &[&str]) -> Option<&'a Item> {
    path.iter()
        .try_fold(document.as_item(), |item, step| item.get(step))
}

fn text_at<'a>(document: &'a DocumentMut, path: &[&str]) -> Option<&'a str> {
    at(document, path).and_then(Item::as_str)
}

fn members() -> Vec<String> {
    let manifest = document("Cargo.toml");
    let members: Vec<String> = at(&manifest, &["workspace", "members"])
        .and_then(Item::as_array)
        .expect("the workspace lists its members")
        .iter()
        .filter_map(|m| m.as_str().map(str::to_owned))
        .collect();
    // An empty list satisfies every loop below and looks exactly like a guard
    // that works.
    assert!(
        members.len() >= 6,
        "the workspace lists {} members, so the scan read the wrong file",
        members.len()
    );
    members
}

// ---- the lints are on, everywhere, and named alike --------------------------

#[test]
fn every_package_switches_every_shape_lint_on() {
    let workspace = document("Cargo.toml");
    let mut problems = Vec::new();
    for lint in SHAPE_LINTS {
        let level = text_at(&workspace, &["workspace", "lints", "clippy", lint]);
        if level != Some("warn") {
            problems.push(format!(
                "Cargo.toml [workspace.lints.clippy] {lint} is {level:?}"
            ));
        }
    }

    for member in members() {
        let manifest = document(&format!("{member}/Cargo.toml"));
        let own = OWN_LINT_TABLES.contains(&member.as_str());
        problems.extend(package_problems(&member, &manifest, own));
    }

    // "warn" and not "deny": `-D warnings` in CI and in the local gates is what
    // makes all of them a gate at once, and a local `cargo clippy` must still run
    // to the end in the middle of an edit.
    assert!(
        problems.is_empty(),
        "a shape lint is not switched on where it has to be, so nothing measures that axis \
         there and nothing would say so:\n{}",
        problems.join("\n")
    );
}

/// What is wrong with one package's lints: inheriting when it is listed as
/// keeping its own table or the other way round, and a shape lint its own table
/// does not switch on.
fn package_problems(member: &str, manifest: &DocumentMut, listed_as_own: bool) -> Vec<String> {
    let mut problems = Vec::new();
    let inherits = at(manifest, &["lints", "workspace"]).and_then(Item::as_bool) == Some(true);
    if listed_as_own == inherits {
        problems.push(format!(
            "{member} {} - update OWN_LINT_TABLES with the reason",
            if inherits {
                "inherits the workspace lints but is listed as keeping its own"
            } else {
                "keeps a lint table of its own and is not listed"
            }
        ));
    }
    if !inherits {
        for lint in SHAPE_LINTS {
            let level = text_at(manifest, &["lints", "clippy", lint]);
            if level != Some("warn") {
                problems.push(format!("{member} [lints.clippy] {lint} is {level:?}"));
            }
        }
    }
    problems
}

/// The keys of a table, or none where there is no table.
fn keys(document: &DocumentMut, path: &[&str]) -> BTreeSet<String> {
    at(document, path)
        .and_then(Item::as_table_like)
        .map(|t| t.iter().map(|(k, _)| k.to_owned()).collect())
        .unwrap_or_default()
}

fn threshold_key(lint: &str) -> String {
    format!("{}-threshold", lint.replace('_', "-"))
}

#[test]
fn the_shape_lints_are_named_alike_wherever_they_are_listed() {
    let wanted: BTreeSet<String> = SHAPE_LINTS.iter().map(|l| threshold_key(l)).collect();

    let clippy: BTreeSet<String> = read("clippy.toml")
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .filter_map(|l| l.split_once('=').map(|(k, _)| k.trim().to_owned()))
        .filter(|k| k.ends_with("-threshold"))
        .collect();

    let shape = document(".github/shape.toml");
    // The first string after each `Axis(`, wherever a formatter put it.
    let gate: BTreeSet<String> = read(".github/scripts/shape_gate.py")
        .split("Axis(")
        .skip(1)
        .filter_map(|rest| rest.split('"').nth(1).map(str::to_owned))
        .collect();

    for (place, found) in [
        ("clippy.toml", clippy),
        (
            ".github/shape.toml [clippy.product]",
            keys(&shape, &["clippy", "product"]),
        ),
        (
            ".github/shape.toml [clippy.near.product]",
            keys(&shape, &["clippy", "near", "product"]),
        ),
        (
            ".github/shape.toml [clippy.near.all]",
            keys(&shape, &["clippy", "near", "all"]),
        ),
        ("the AXES of .github/scripts/shape_gate.py", gate),
    ] {
        assert_eq!(
            found, wanted,
            "{place} does not name the shape thresholds this file names. A lint added or \
             dropped in one place is a ceiling nobody asks for in another - change all of them"
        );
    }
}

// ---- CI asks ------------------------------------------------------------------

/// The code of the workflow step holding `needle`: from its `- name:` line to
/// the next step, with every comment line dropped, so that prose about the step
/// cannot stand in for the step.
fn step_around(workflow: &str, needle: &str) -> String {
    let lines: Vec<&str> = workflow.lines().collect();
    let at = lines
        .iter()
        .position(|l| !l.trim_start().starts_with('#') && l.contains(needle))
        .unwrap_or_else(|| panic!("no step of the CI workflow runs `{needle}`"));
    let start = lines[..at]
        .iter()
        .rposition(|l| l.trim_start().starts_with("- name:"))
        .expect("the needle sits inside a named step");
    let end = lines[at..]
        .iter()
        .position(|l| l.trim_start().starts_with("- name:"))
        .map_or(lines.len(), |i| at + i);
    lines[start..end]
        .iter()
        .filter(|l| !l.trim_start().starts_with('#'))
        .copied()
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn ci_asks_for_both_scopes_on_every_system_and_pins_them_on_windows() {
    let workflow = read(".github/workflows/ci.yml");

    // The scope of all the code is clippy.toml, and it is held by the ordinary
    // clippy run of every system, as long as that run denies warnings and covers
    // every target.
    let clippy = step_around(&workflow, "cargo clippy --workspace");
    assert!(
        clippy.contains("--all-targets") && clippy.contains("-D warnings"),
        "the clippy step no longer covers every target with warnings denied, so the ceilings in \
         clippy.toml hold nothing:\n{clippy}"
    );

    // The product, the pinning and the crowd are the gate's. Its tests run
    // first, so that a gate nobody has seen block is not trusted.
    let gate = step_around(&workflow, "shape_gate.py");
    for (needle, why) in [
        (
            "python -m unittest discover -s .github/scripts -p \"test_shape_gate.py\"",
            "the gate is not tested before it is trusted",
        ),
        (
            "[ \"$RUNNER_OS\" = \"Windows\" ]",
            "the pinning is not tied to the one system where every line compiles",
        ),
        (
            "python .github/scripts/shape_gate.py --pinned",
            "no system asks whether each ceiling is the measurement",
        ),
    ] {
        assert!(gate.contains(needle), "{why}:\n{gate}");
    }
    assert!(
        gate.lines()
            .any(|l| l.trim() == "python .github/scripts/shape_gate.py"),
        "Linux and macOS no longer hold the product to its ceilings:\n{gate}"
    );

    // And the job that holds the step runs on the three systems.
    assert!(
        workflow.contains("os: [ubuntu-latest, windows-latest, macos-latest]"),
        "the test job no longer runs on all three systems"
    );
}

// ---- the ways out only get rarer ----------------------------------------------

/// Every file under `dir` with one of `extensions`, build output skipped.
fn files_under(dir: &Path, extensions: &[&str], into: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().and_then(|n| n.to_str()) != Some("target") {
                files_under(&path, extensions, into);
            }
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| extensions.contains(&e))
        {
            into.push(path);
        }
    }
}

/// The path relative to the root, with forward slashes on every system.
fn relative(path: &Path) -> String {
    path.strip_prefix(root())
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// How often `clippy::<name>` stands in the code part of `text`, as a whole path.
fn mentions(text: &str, name: &str) -> usize {
    let needle = format!("clippy::{name}");
    text.lines()
        .map(|line| line.split("//").next().unwrap_or(""))
        .map(|code| {
            code.match_indices(&needle)
                .filter(|(at, _)| {
                    let after = code[at + needle.len()..].chars().next();
                    !after.is_some_and(|c| c.is_alphanumeric() || c == '_')
                })
                .count()
        })
        .sum()
}

#[test]
fn the_ways_out_of_the_shape_lints_only_ever_get_rarer() {
    let mut files = Vec::new();
    files_under(&root().join("crates"), &["rs"], &mut files);
    assert!(
        files.len() >= 100,
        "the scan read {} files, and an empty scan counts zero of anything",
        files.len()
    );

    let mut counted: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for path in &files {
        // This file names what it counts, so counting it would count the rule as
        // a breach of itself.
        if path.ends_with("nkb-adapters/tests/shape.rs") {
            continue;
        }
        let text = std::fs::read_to_string(path).unwrap_or_default();
        for (name, _) in WAYS_OUT {
            for _ in 0..mentions(&text, name) {
                counted.entry(name).or_default().push(relative(path));
            }
        }
    }

    let mut problems = Vec::new();
    for (name, frozen) in WAYS_OUT {
        let found = counted.get(name).map_or(&[][..], Vec::as_slice);
        if found.len() != frozen {
            problems.push(format!(
                "clippy::{name} stands {} times, frozen at {frozen}: {found:?}",
                found.len()
            ));
        }
    }
    assert!(
        problems.is_empty(),
        "the ways out of the shape lints changed. Fewer means lowering the number in the same \
         change. More means something reached for an allowance instead of getting smaller:\n{}",
        problems.join("\n")
    );
}

// ---- the size of a file ---------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Syntax {
    Rust,
    Slint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Code,
    LineComment,
    BlockComment(usize),
    Text,
    RawText(usize),
}

fn starts(chars: &[char], at: usize, with: &str) -> bool {
    with.chars()
        .enumerate()
        .all(|(k, c)| chars.get(at + k) == Some(&c))
}

fn is_identifier(c: Option<&char>) -> bool {
    c.is_some_and(|c| c.is_alphanumeric() || *c == '_')
}

/// If a raw string starts at `at` (`r"`, `r#"`, `br"`, `cr##"` and so on), how
/// many `#` close it and how many characters open it.
fn raw_string_at(chars: &[char], at: usize) -> Option<(usize, usize)> {
    if at > 0 && is_identifier(chars.get(at - 1)) {
        return None;
    }
    let after_r = match chars.get(at) {
        Some('r') => at + 1,
        Some('b' | 'c') if chars.get(at + 1) == Some(&'r') => at + 2,
        _ => return None,
    };
    let hashes = chars[after_r..].iter().take_while(|c| **c == '#').count();
    (chars.get(after_r + hashes) == Some(&'"')).then_some((hashes, after_r + hashes + 1 - at))
}

/// Where the code goes on after a quote at `at`: past a character literal
/// (`'x'`, `'\n'`, `'\''`, `'\u{1F600}'`), or just past the quote of a lifetime
/// or a label, which is code like any other.
fn past_quote(chars: &[char], at: usize) -> usize {
    if chars.get(at + 1) == Some(&'\\') {
        let mut end = at + 3;
        while end < chars.len() && chars[end] != '\'' {
            end += 1;
        }
        return end + 1;
    }
    if chars.get(at + 2) == Some(&'\'') {
        return at + 3;
    }
    at + 1
}

/// The numbers of the lines, counted from 1, that hold anything outside a comment.
fn code_lines(text: &str, syntax: Syntax) -> BTreeSet<usize> {
    let rust = syntax == Syntax::Rust;
    let chars: Vec<char> = text.chars().collect();
    let mut lines = BTreeSet::new();
    let mut state = State::Code;
    let mut line = 1;
    let mut at = 0;
    while at < chars.len() {
        let c = chars[at];
        if c == '\n' {
            line += 1;
            if state == State::LineComment {
                state = State::Code;
            }
            at += 1;
            continue;
        }
        match state {
            State::LineComment => at += 1,
            State::BlockComment(depth) => {
                if starts(&chars, at, "*/") {
                    state = if depth == 1 {
                        State::Code
                    } else {
                        State::BlockComment(depth - 1)
                    };
                    at += 2;
                } else if rust && starts(&chars, at, "/*") {
                    state = State::BlockComment(depth + 1);
                    at += 2;
                } else {
                    at += 1;
                }
            }
            State::Text => {
                if !c.is_whitespace() {
                    lines.insert(line);
                }
                if c == '\\' {
                    // The escaped character is skipped, unless it is the end of
                    // the line - a string continued on the next line still moves
                    // to the next line.
                    at += 1;
                    if chars.get(at).is_some_and(|n| *n != '\n') {
                        at += 1;
                    }
                    continue;
                }
                if c == '"' {
                    state = State::Code;
                }
                at += 1;
            }
            State::RawText(hashes) => {
                if !c.is_whitespace() {
                    lines.insert(line);
                }
                if c == '"' && (1..=hashes).all(|k| chars.get(at + k) == Some(&'#')) {
                    state = State::Code;
                    at += 1 + hashes;
                } else {
                    at += 1;
                }
            }
            State::Code => {
                if starts(&chars, at, "//") {
                    state = State::LineComment;
                    at += 2;
                    continue;
                }
                if starts(&chars, at, "/*") {
                    state = State::BlockComment(1);
                    at += 2;
                    continue;
                }
                if c.is_whitespace() {
                    at += 1;
                    continue;
                }
                lines.insert(line);
                if c == '"' {
                    state = State::Text;
                    at += 1;
                } else if let Some((hashes, opening)) = raw_string_at(&chars, at).filter(|_| rust) {
                    state = State::RawText(hashes);
                    at += opening;
                } else if rust && c == '\'' {
                    at = past_quote(&chars, at);
                } else {
                    at += 1;
                }
            }
        }
    }
    lines
}

/// Where the test module of a source file begins: the index of the
/// `#[cfg(test)]` line above an inline `mod ... {`, and of the `mod` line.
fn test_module(lines: &[&str]) -> Option<(usize, usize)> {
    lines.iter().enumerate().find_map(|(at, line)| {
        if *line != "#[cfg(test)]" {
            return None;
        }
        // Past the attributes that may stand between the two, one line or many.
        let next = lines[at + 1..].iter().position(|l| {
            !(l.is_empty() || l.starts_with('#') || l.starts_with(')') || l.starts_with(' '))
        })?;
        let module = at + 1 + next;
        let opens = lines[module];
        let is_module = ["mod ", "pub mod ", "pub(crate) mod "]
            .iter()
            .any(|p| opens.starts_with(p))
            && opens.ends_with('{');
        is_module.then_some((at, module))
    })
}

/// Whether anything but blank lines follows the test module of a source file,
/// or `None` when it has none. A module whose end cannot be found counts as
/// followed by code, because then the cut cannot be trusted either.
fn code_after_test_module(lines: &[&str]) -> Option<bool> {
    let (_, module) = test_module(lines)?;
    let end = lines[module..].iter().position(|l| *l == "}");
    Some(end.is_none_or(|e| lines[module + e + 1..].iter().any(|l| !l.trim().is_empty())))
}

#[derive(Debug)]
struct Measured {
    product: Vec<(usize, String)>,
    whole: Vec<(usize, String)>,
    slint: Vec<(usize, String)>,
}

fn measure() -> Measured {
    let mut rust = Vec::new();
    files_under(&root().join("crates"), &["rs"], &mut rust);
    let mut slint = Vec::new();
    files_under(&root().join("crates"), &["slint"], &mut slint);

    let mut measured = Measured {
        product: Vec::new(),
        whole: Vec::new(),
        slint: Vec::new(),
    };
    for path in rust {
        let name = relative(&path);
        if GENERATED.contains(&name.as_str()) {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{name}: {e}"));
        let code = code_lines(&text, Syntax::Rust);
        measured.whole.push((code.len(), name.clone()));
        if name.contains("/src/") {
            let lines: Vec<&str> = text.lines().collect();
            let product = test_module(&lines).map_or(code.len(), |(cfg, _)| {
                code.iter().filter(|line| **line <= cfg).count()
            });
            measured.product.push((product, name));
        }
    }
    for path in slint {
        let name = relative(&path);
        if name.starts_with(GALLERY) {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{name}: {e}"));
        measured
            .slint
            .push((code_lines(&text, Syntax::Slint).len(), name));
    }
    for list in [
        &mut measured.product,
        &mut measured.whole,
        &mut measured.slint,
    ] {
        list.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    }
    measured
}

/// `ceiling * band`, rounded up, in exact arithmetic: the band is read as the
/// decimal it is written as, because floating point moves the edge for some
/// bands. Measured 2026-10-08: never at 0.7 for a ceiling below 5000, but
/// 100 * 0.55 is a hair above 55, and twelve of the 99 two-digit bands do that
/// somewhere.
fn near_edge(ceiling: usize, band: &str) -> usize {
    let digits = band
        .strip_prefix("0.")
        .unwrap_or_else(|| panic!("the band {band} is not a fraction written as 0.x"));
    let numerator: usize = digits
        .parse()
        .unwrap_or_else(|e| panic!("the band {band}: {e}"));
    let denominator = 10_usize.pow(u32::try_from(digits.len()).expect("a short fraction"));
    (ceiling * numerator).div_ceil(denominator)
}

#[test]
fn no_file_has_grown_past_its_ceiling_and_each_ceiling_is_the_measurement() {
    let measured = measure();
    // The canaries: a walk that found nothing satisfies every ceiling ever set.
    assert!(
        measured.product.len() >= 60,
        "measured {} source files",
        measured.product.len()
    );
    assert!(
        measured.whole.len() >= 100,
        "measured {} Rust files",
        measured.whole.len()
    );
    assert!(
        measured.slint.len() >= 10,
        "measured {} Slint files",
        measured.slint.len()
    );

    let shape = document(".github/shape.toml");
    // The band as it is written, not as a float, for the reason on `near_edge`.
    let band = at(&shape, &["files", "band"])
        .and_then(Item::as_value)
        .map(|v| v.to_string().trim().to_owned())
        .expect("[files] has a band");

    let mut problems = Vec::new();
    for (axis, sizes) in [
        ("rust-product", &measured.product),
        ("rust-whole", &measured.whole),
        ("slint", &measured.slint),
    ] {
        let number = |key: &str| {
            at(&shape, &["files", axis, key])
                .and_then(Item::as_integer)
                .and_then(|n| usize::try_from(n).ok())
                .unwrap_or_else(|| panic!("[files.{axis}] has no whole {key}"))
        };
        let (ceiling, frozen) = (number("ceiling"), number("near"));
        let edge = near_edge(ceiling, &band);
        println!(
            "{axis}: ceiling {ceiling}, near from {edge}: {:?}",
            sizes
                .iter()
                .take_while(|(size, _)| *size >= edge)
                .collect::<Vec<_>>()
        );
        problems.extend(file_problems(axis, sizes, ceiling, frozen, edge));
    }
    assert!(problems.is_empty(), "\n{}\n", problems.join("\n"));
}

/// What is wrong with one file axis: the largest file over its ceiling or below
/// it, and a count near the ceiling that is not the frozen one. `sizes` is
/// largest first.
fn file_problems(
    axis: &str,
    sizes: &[(usize, String)],
    ceiling: usize,
    frozen: usize,
    edge: usize,
) -> Vec<String> {
    let mut problems = Vec::new();
    let Some((largest, holder)) = sizes.first() else {
        return vec![format!("{axis}: no file was measured")];
    };
    if *largest > ceiling {
        problems.push(format!(
            "{axis}: {holder} has {largest} lines of code, over the ceiling of {ceiling} - \
             move code out, do not raise the number"
        ));
    } else if *largest < ceiling {
        problems.push(format!(
            "{axis}: the ceiling {ceiling} has room in it, the largest is {holder} at \
             {largest} - lower the ceiling to {largest}"
        ));
    }
    let near: Vec<String> = sizes
        .iter()
        .filter(|(size, _)| *size >= edge)
        .map(|(size, name)| format!("{name} ({size})"))
        .collect();
    if near.len() != frozen {
        problems.push(format!(
            "{axis}: {} files stand at {edge} lines or more, frozen at {frozen} - {}: {near:?}",
            near.len(),
            if near.len() > frozen {
                "shrink one before another joins"
            } else {
                "lower the frozen count to the measurement"
            }
        ));
    }
    problems
}

#[test]
fn every_generated_file_still_says_it_is_generated() {
    for name in GENERATED {
        let text = read(name);
        assert!(
            text.starts_with(GENERATED_MARK),
            "{name} is not measured because it is generated, and it no longer says so - either it \
             is written by hand now and comes off the list, or its first line is restored"
        );
    }
}

#[test]
fn the_test_module_is_the_last_thing_in_every_source_file() {
    // The product part of a file ends where its test module begins. Code after
    // the module would be counted as tests, so it would be free, and the cut
    // would be a way out.
    let mut files = Vec::new();
    files_under(&root().join("crates"), &["rs"], &mut files);
    let mut cut = 0;
    let mut problems = Vec::new();
    for path in files {
        let name = relative(&path);
        if !name.contains("/src/") {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        let lines: Vec<&str> = text.lines().collect();
        match code_after_test_module(&lines) {
            None => continue,
            Some(false) => cut += 1,
            Some(true) => {
                cut += 1;
                problems.push(name);
            }
        }
    }
    assert!(
        cut >= 30,
        "found {cut} test modules, so the scan looked in the wrong place"
    );
    assert!(
        problems.is_empty(),
        "these files have code after their test module, which the file ceiling would count as \
         tests: {problems:?}"
    );
}

// ---- the metric, measured --------------------------------------------------------
//
// A ceiling standing on a metric that lies is worse than no ceiling, because the
// suite is green and somebody believes it. Each test below has a counterpart that
// keeps it honest: "comments are free" is satisfied perfectly by a metric that
// counts nothing, and "strings are code" by one that counts everything.

fn count(text: &str) -> usize {
    code_lines(text, Syntax::Rust).len()
}

#[test]
fn comments_and_blank_lines_are_free() {
    let bare = "fn f() {\n    let x = 1;\n}\n";
    let explained = format!(
        "//! A module.\n\n/// What f does.\nfn f() {{\n{}    /* a block\n       of two */\n    let x = 1; // why\n\n}}\n",
        "    // a reason\n".repeat(90)
    );
    assert_eq!(count(bare), 3);
    assert_eq!(count(&explained), 3);
}

#[test]
fn code_counts_line_for_line() {
    let bare = "fn f() {\n    let x = 1;\n}\n";
    let longer = format!("fn f() {{\n    let x = 1;\n{}}}\n", "    g();\n".repeat(7));
    assert_eq!(count(&longer) - count(bare), 7);
}

#[test]
fn a_string_is_code_even_when_it_holds_comment_marks() {
    // Built from pieces, so that this file itself does not hold the marks it
    // is about inside a string the reader would have to untangle.
    let quote = '"';
    let text = format!(
        "let a = {quote}//not a comment{quote};\nlet b = r#{quote}\n// a line of data\n/* more */\n{quote}#;\nlet c = {quote}one \\\n// two{quote};\n"
    );
    // Seven lines, and each holds code or the inside of a string: the raw
    // string's two lines of data, and the line a backslash continues onto.
    assert_eq!(count(&text), 7, "{text}");
}

#[test]
fn a_raw_string_ends_only_at_its_own_hashes() {
    // A lexer that took `r#"` for a plain string would close it at the quote on
    // the second line, and read the rest of the file as the inside of a string.
    let quote = '"';
    let text =
        format!("let a = r#{quote}\n{quote} // a quote inside\n{quote}#;\n// a comment after\n");
    assert_eq!(count(&text), 3, "{text}");
}

#[test]
fn a_block_comment_nests_in_rust() {
    let text = "/* outer /* inner */ still a comment */\nlet x = 1;\n";
    assert_eq!(count(text), 1);
}

#[test]
fn a_quote_is_a_character_or_a_lifetime_and_never_opens_a_string() {
    // `'"'` would open a string to a lexer that does not know characters, and
    // `'a` would open a character to one that does not know lifetimes.
    let text = "let q = '\"';\n// gone\nfn f<'a>(x: &'a str) -> &'a str { x }\n// gone too\nlet e = '\\'';\n";
    assert_eq!(count(text), 3, "{text}");
}

#[test]
fn slint_comments_are_free_and_its_strings_are_code() {
    let text = "// a comment\nText {\n    /* a block */\n    text: \"// shown\";\n}\n";
    assert_eq!(code_lines(text, Syntax::Slint).len(), 3);
}

#[test]
fn the_test_module_is_found_after_its_attributes_and_not_at_a_declaration() {
    let inline = [
        "fn f() {}",
        "#[cfg(test)]",
        "#[allow(",
        "    clippy::panic,",
        ")]",
        "mod tests {",
        "}",
    ];
    assert_eq!(test_module(&inline), Some((1, 5)));
    let declared = ["#[cfg(test)]", "pub(crate) mod test_support;", "fn f() {}"];
    assert_eq!(test_module(&declared), None);
}

// ---- the judging, on made-up input ----------------------------------------------
//
// The checks over the real tree pass while the tree is in order, so they alone
// cannot show that each of them would fail. These feed each judgement a case it
// must refuse.

fn toml(text: &str) -> DocumentMut {
    text.parse().expect("the made-up manifest is valid TOML")
}

#[test]
fn a_package_whose_lints_drift_is_named() {
    let own = toml(
        "[lints.clippy]\ntoo_many_lines = \"warn\"\ncognitive_complexity = \"warn\"\n\
         excessive_nesting = \"warn\"\ntoo_many_arguments = \"warn\"\n",
    );
    assert_eq!(
        package_problems("crates/a", &own, true),
        Vec::<String>::new()
    );
    let unlisted = package_problems("crates/a", &own, false);
    assert!(
        unlisted.iter().any(|p| p.contains("is not listed")),
        "{unlisted:?}"
    );

    let silenced = toml("[lints.clippy]\ntoo_many_lines = \"allow\"\n");
    let found = package_problems("crates/a", &silenced, true);
    assert_eq!(
        found.len(),
        4,
        "one sentence for each lint not on: {found:?}"
    );

    let inherits = toml("[lints]\nworkspace = true\n");
    assert_eq!(
        package_problems("crates/b", &inherits, false),
        Vec::<String>::new()
    );
    let listed = package_problems("crates/b", &inherits, true);
    assert!(
        listed.iter().any(|p| p.contains("but is listed")),
        "{listed:?}"
    );
}

#[test]
fn a_file_ceiling_over_or_with_room_and_a_wrong_count_are_each_named() {
    let sizes = |list: &[usize]| -> Vec<(usize, String)> {
        list.iter()
            .enumerate()
            .map(|(k, size)| (*size, format!("f{k}")))
            .collect()
    };
    let judge = |list: &[usize]| file_problems("x", &sizes(list), 100, 2, 70);

    // At the measurement: the largest at the ceiling, two at the edge or above.
    assert_eq!(judge(&[100, 70, 69]), Vec::<String>::new());
    for (list, words) in [
        (&[101, 70][..], "over the ceiling"),
        (&[99, 70][..], "has room in it"),
        (&[100, 70, 70][..], "shrink one"),
        (&[100, 69][..], "lower the frozen count"),
    ] {
        let found = judge(list);
        assert!(
            found.len() == 1 && found[0].contains(words),
            "{list:?} should say only that it is {words}: {found:?}"
        );
    }
}

#[test]
fn code_after_the_test_module_is_found_and_blank_lines_are_not_code() {
    let last = ["fn f() {}", "#[cfg(test)]", "mod tests {", "}", ""];
    assert_eq!(code_after_test_module(&last), Some(false));
    let followed = ["#[cfg(test)]", "mod tests {", "}", "fn g() {}"];
    assert_eq!(code_after_test_module(&followed), Some(true));
    let unclosed = ["#[cfg(test)]", "mod tests {", "    fn t() {}"];
    assert_eq!(code_after_test_module(&unclosed), Some(true));
    assert_eq!(code_after_test_module(&["fn f() {}"]), None);
}

#[test]
fn the_edge_of_the_band_is_exact() {
    // In floating point this one is 56.
    assert_eq!(near_edge(100, "0.55"), 55);
    assert_eq!(near_edge(100, "0.7"), 70);
    assert_eq!(near_edge(1440, "0.7"), 1008);
    assert_eq!(near_edge(2907, "0.7"), 2035);
    assert_eq!(near_edge(346, "0.7"), 243);
}
