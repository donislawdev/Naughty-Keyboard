//! What `linux/atspi.rs` says to the bus of accessibility is a list, and this file holds it.
//!
//! # The promise this stands for
//!
//! Untouchable rule 17, second promise: the tool does not read the contents of windows. On
//! Windows `field_reads_only_kinds.rs` holds the closed list of UI Automation properties, on
//! macOS `ax_reads_only_kinds.rs` the list of attributes. On Linux the questions are method
//! names of the bus, so a read of the contents is one word away from the read of the kind:
//! `GetState` and `GetName` travel the same call, and `GetItems` of the cache of an
//! application brings the name of every element in one answer.
//!
//! # What this checks, and what it cannot
//!
//! - every string written in the code of the two files that talk to or decide for the bus
//!   (`src/linux/atspi.rs` and `src/linux_atspi_rules.rs`) is one of the words below, so
//!   adding a question, an interface or a property is an edit to THIS file, with the commit
//!   message that goes with it,
//! - the bus is called in exactly the three places below,
//! - it drops everything from the first `#[cfg(test)]` line of a file, strips `//` comments
//!   and the attribute lines (their words are lint reasons and conditions, not questions).
//!
//! It cannot see a question built from pieces of words, and it cannot see a read done in
//! another file: `src/linux/atspi.rs` is the only file of the package that opens the bus of
//! accessibility, and `unsafe_lives_here_only.rs` keeps the rest of the program away from
//! the system.

#![allow(clippy::panic, clippy::expect_used)]

use std::collections::BTreeSet;
use std::path::Path;

/// Every string the code of the two files may write, each with what it is for.
const WORDS: &[&str] = &[
    // Where the bus of accessibility is, and whether it is switched on.
    "org.a11y.Bus",
    "/org/a11y/bus",
    "GetAddress",
    "org.a11y.Status",
    "IsEnabled",
    "org.freedesktop.DBus.Properties",
    "Get",
    // The tree: the root of the registry, and the three questions of an element. The
    // children of an element, its states and its role. Never a name, a description, a text
    // or an attribute.
    "org.a11y.atspi.Registry",
    "/org/a11y/atspi/accessible/root",
    "org.a11y.atspi.Accessible",
    "GetChildren",
    "GetState",
    "GetRole",
    // Where the session bus is.
    "DBUS_SESSION_BUS_ADDRESS",
    "XDG_RUNTIME_DIR",
];

/// The places the bus is called in `src/linux/atspi.rs`: the address and the switch from the
/// session bus, and the one question of an element.
const CALLS: usize = 3;

/// The code of one file with test modules, `//` comments and attribute lines taken out. An
/// attribute may run over several lines: it ends at the line that ends in `]`.
fn code_of(source: &str) -> String {
    let mut out = String::new();
    let mut in_attribute = false;
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("#[cfg(test)]") {
            break;
        }
        if in_attribute || trimmed.starts_with("#[") || trimmed.starts_with("#![") {
            in_attribute = !trimmed.ends_with(']');
            continue;
        }
        out.push_str(line.split("//").next().unwrap_or(""));
        out.push('\n');
    }
    out
}

/// Every string literal written in `code`.
fn literals(code: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let mut rest = code;
    while let Some(at) = rest.find('"') {
        let tail = &rest[at + 1..];
        let end = tail.find('"').unwrap_or(tail.len());
        found.insert(tail[..end].to_owned());
        rest = tail.get(end + 1..).unwrap_or("");
    }
    found
}

/// What is wrong with the code of `atspi` (the file that calls) and `rules` (the one that
/// names), each already cleaned.
fn findings(atspi: &str, rules: &str) -> Vec<String> {
    let mut problems = Vec::new();
    let mut written = literals(atspi);
    written.extend(literals(rules));
    let allowed: BTreeSet<String> = WORDS.iter().map(|word| (*word).to_owned()).collect();
    for extra in written.difference(&allowed) {
        problems.push(format!(
            "the bus is spoken to in a word not on the list: {extra:?}"
        ));
    }
    for missing in allowed.difference(&written) {
        problems.push(format!(
            "a word on the list is no longer written: {missing:?}"
        ));
    }
    let calls = atspi.matches(".call_method(").count();
    if calls != CALLS {
        problems.push(format!(
            "the bus is called {calls} times in atspi.rs, the list allows {CALLS}"
        ));
    }
    problems
}

fn read(path: &str) -> String {
    let full = Path::new(env!("CARGO_MANIFEST_DIR")).join(path);
    code_of(&std::fs::read_to_string(&full).expect("a source file is readable"))
}

#[test]
fn the_bus_of_accessibility_is_asked_the_kind_of_the_focused_control_and_nothing_else() {
    let problems = findings(
        &read("src/linux/atspi.rs"),
        &read("src/linux_atspi_rules.rs"),
    );
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn the_scanner_sees_what_it_is_meant_to_forbid() {
    // A guard nobody has seen fail is indistinguishable from a broken one.
    let atspi = read("src/linux/atspi.rs");
    let rules = read("src/linux_atspi_rules.rs");
    assert!(findings(&atspi, &rules).is_empty());

    let names_it = |atspi: &str, rules: &str, culprit: &str| {
        let found = findings(atspi, rules);
        assert!(
            found.iter().any(|problem| problem.contains(culprit)),
            "expected a report naming {culprit}, got {found:?}"
        );
    };
    // The name of the element: one more word.
    names_it(
        &format!("{atspi}\nlet n = self.ask(node, \"GetName\");\n"),
        &rules,
        "GetName",
    );
    // The cache of an application, which brings every name in one answer.
    names_it(
        &format!("{atspi}\nlet c = self.ask(node, \"GetItems\");\n"),
        &rules,
        "GetItems",
    );
    // A property read as a text.
    names_it(
        &atspi,
        &format!("{rules}\npub(crate) const TEXT: &str = \"org.a11y.atspi.Text\";\n"),
        "org.a11y.atspi.Text",
    );
    // A fourth place that calls the bus.
    names_it(
        &format!("{atspi}\nself.connection.call_method(a, b, c, d, &());\n"),
        &rules,
        "times in atspi.rs",
    );
    // A word dropped from the code is also a change worth a look.
    names_it(
        &atspi.replace("\"GetRole\"", "\"GetRol\""),
        &rules,
        "GetRol",
    );
    // A word in a comment is not a question.
    let commented = format!("{atspi}\n// self.ask(node, \"GetName\");\n");
    assert!(findings(&code_of(&commented), &rules).is_empty());
}
