//! What `macos/ax.rs` and `macos/pulse.rs` read through accessibility is a list, and this file holds it.
//!
//! # The promise this stands for
//!
//! Untouchable rule 17, second promise: the tool does not read the contents of
//! windows. On Windows `field_reads_only_kinds.rs` holds the closed list of
//! UI Automation properties. On macOS the same question is asked of the
//! accessibility API, whose attributes are strings, so a read of the contents is
//! one literal away from the read of the kind: `AXRole` and `AXValue` travel the
//! same call. Until `D139` the promise rested on the module text alone.
//!
//! # What this checks, and what it cannot
//!
//! - every `AX...` attribute name written as a C string literal in the code of
//!   `crates/nkb-sys/src/macos` is exactly the five below, so adding one is an
//!   edit to THIS file, with the commit message that goes with it,
//! - the literal `AXValue` is only ever handed to `AXUIElementIsAttributeSettable`,
//!   which answers yes or no and reads no value: every line that names it is its
//!   definition or that call,
//! - the value of an element is never copied: no other `AXUIElementCopy...` call
//!   than the ones below appears (the attribute NAMES of an element are asked for,
//!   to see whether a selected range exists, and the range itself never is),
//! - it drops everything from the first `#[cfg(test)]` line of a file, and strips
//!   `//` comments.
//!
//! It cannot see a read done through another crate - `unsafe_lives_here_only.rs`
//! is what keeps the accessibility API inside this package.

#![allow(clippy::panic, clippy::expect_used)]

use std::collections::BTreeSet;
use std::path::Path;

/// The attribute names the code may write, each with what is done with the answer.
const ATTRIBUTES: &[&str] = &[
    // The element that holds the keyboard focus: a reference, never read further
    // than the three facts below.
    "AXFocusedUIElement",
    // Whether the application has a window in front at all: a reference whose
    // only use is that it exists (the application shows its focus to nobody).
    "AXFocusedWindow",
    // The type name of the focused element (`AXTextField`), compared with a closed
    // list in `macos_rules.rs` and dropped.
    "AXRole",
    // Only asked whether it can be SET - never copied.
    "AXValue",
    // Only looked for among the NAMES of the attributes - never copied.
    "AXSelectedTextRange",
];

/// The calls that copy something out of an element, and how many times each may
/// appear in the code of `macos/`: the focus, its role, and the window.
const COPY_CALLS: &[(&str, usize)] = &[("ffi::AXUIElementCopyAttributeValue(", 4)];

/// The code of one file with test modules and `//` comments taken out.
fn code_of(source: &str) -> String {
    let mut out = String::new();
    for line in source.lines() {
        if line.trim_start().starts_with("#[cfg(test)]") {
            break;
        }
        let code = line.split("//").next().unwrap_or("");
        out.push_str(code);
        out.push('\n');
    }
    out
}

/// Every attribute name written as a string literal, `"AX..."` or `c"AX..."`.
fn attribute_literals(code: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let mut rest = code;
    while let Some(at) = rest.find("\"AX") {
        let tail = &rest[at + 1..];
        let end = tail.find('"').unwrap_or(tail.len());
        found.insert(tail[..end].to_owned());
        rest = &tail[end..];
    }
    found
}

/// What is wrong with the code of `crates/nkb-sys/src/macos`, joined.
fn findings(code: &str) -> Vec<String> {
    let mut problems = Vec::new();
    let literals = attribute_literals(code);
    let allowed: BTreeSet<String> = ATTRIBUTES.iter().map(|name| (*name).to_owned()).collect();
    if literals != allowed {
        problems.push(format!(
            "accessibility attributes named: {literals:?}, the list allows {allowed:?}"
        ));
    }
    for line in code.lines() {
        let defines = line.contains("let value_name") || line.contains("c\"AXValue\"");
        if line.contains("value_name") && !defines && !line.contains("IsAttributeSettable") {
            problems.push(format!(
                "the AXValue attribute is used outside the settable question: {}",
                line.trim()
            ));
        }
    }
    for (call, allowed_times) in COPY_CALLS {
        let times = code.matches(call).count();
        if times != *allowed_times {
            problems.push(format!(
                "{call} appears {times} times, the list allows {allowed_times}"
            ));
        }
    }
    problems
}

#[test]
fn accessibility_reads_only_the_kind_of_the_focused_control() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("macos");
    let mut code = String::new();
    for entry in std::fs::read_dir(&src).expect("src/macos is readable") {
        let path = entry.expect("an entry").path();
        if path.extension().is_some_and(|e| e == "rs") {
            code.push_str(&code_of(
                &std::fs::read_to_string(&path).expect("a source file is readable"),
            ));
        }
    }
    let problems = findings(&code);
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn the_scanner_sees_what_it_is_meant_to_forbid() {
    // A guard nobody has seen fail is indistinguishable from a broken one.
    let clean = "let a = ffi::cf_string(c\"AXFocusedUIElement\");\n\
                 let b = ffi::cf_string(c\"AXFocusedWindow\");\n\
                 let role_name = ffi::cf_string(c\"AXRole\");\n\
                 let value_name = ffi::cf_string(c\"AXValue\");\n\
                 let e = unsafe { ffi::AXUIElementIsAttributeSettable(focus.0, value_name.0, &mut settable) };\n\
                 has_attribute(focus.0, \"AXSelectedTextRange\");\n\
                 let r = ffi::cf_string(c\"AXSelectedTextRange\");\n\
                 unsafe { ffi::AXUIElementCopyAttributeValue(app, a.0, &mut f) };\n\
                 unsafe { ffi::AXUIElementCopyAttributeValue(app, b.0, &mut f) };\n\
                 unsafe { ffi::AXUIElementCopyAttributeValue(focus.0, role_name.0, &mut r) };\n\
                 unsafe { ffi::AXUIElementCopyAttributeValue(app, role.0, &mut value) };\n";
    assert!(findings(clean).is_empty(), "{:?}", findings(clean));

    let names_it = |code: &str, culprit: &str| {
        let found = findings(code);
        assert!(
            found.iter().any(|problem| problem.contains(culprit)),
            "expected a report naming {culprit}, got {found:?}"
        );
    };
    // The title of the element: one more attribute.
    names_it(
        &format!("{clean}let t = ffi::cf_string(c\"AXTitle\");\n"),
        "AXTitle",
    );
    // The value of the element, copied instead of asked about.
    names_it(
        &format!(
            "{clean}unsafe {{ ffi::AXUIElementCopyAttributeValue(focus.0, value_name.0, &mut v) }};\n"
        ),
        "outside the settable question",
    );
    // A fifth copy out of an element.
    names_it(
        &format!("{clean}unsafe {{ ffi::AXUIElementCopyAttributeValue(focus.0, x.0, &mut v) }};\n"),
        "ffi::AXUIElementCopyAttributeValue(",
    );
    // An attribute dropped from the list is also a change worth a look.
    names_it(
        &clean.replace("c\"AXFocusedWindow\"", "c\"AXFocusedWindoo\""),
        "AXFocusedWindoo",
    );
}
