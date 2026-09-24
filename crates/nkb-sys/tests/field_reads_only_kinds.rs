//! What `field.rs` reads through UI Automation is a list, and this file holds it.
//!
//! # The promise this stands for
//!
//! Untouchable rule 17, second promise: the tool does not read the contents of
//! windows. Until `D73` it held because nothing could read at all -
//! architektura.md 5, "no port to read a field". `D73` gave `nkb-sys` ONE read,
//! before a send: what KIND of control holds the keyboard focus. That is not
//! content, but it is one property away from it - `Name` and `Value` travel the
//! same call. So the read is a closed list, and adding to it is an edit to THIS
//! file, with the commit message that goes with it.
//!
//! # What this checks, and what it cannot
//!
//! - every `const PROPERTY_...: i32 = N` in `crates/nkb-sys/src` - the ids the
//!   COM call is handed - is exactly the seven below, name and number,
//! - every `const SLOT_...: usize = N` - the COM methods called - is exactly the
//!   five below. A pattern object (the way to a value or a text range) needs a
//!   method that is not on the list,
//! - no number between 30000 and 30199 - the property id range - appears in
//!   code outside the test modules unless it is one of the seven, so a raw id
//!   handed to the call is caught too,
//! - it drops everything from the first `#[cfg(test)]` line of a file, where
//!   the platform's own names are pinned, and strips `//` comments.
//!
//! It cannot see a read done through another crate - `unsafe_lives_here_only.rs`
//! is what keeps COM inside this package.

#![allow(clippy::panic, clippy::expect_used)]

use std::collections::BTreeSet;
use std::path::Path;

const PROPERTIES: &[(&str, u32)] = &[
    ("PROCESS_ID", 30002),
    ("CONTROL_TYPE", 30003),
    // The class name, added 2026-09-24 (`OBS-141`), and the one entry that is a
    // string: whoever wrote the control named it, the tester's typing never
    // does. field.rs compares it with its list of terminal classes where it is
    // read and keeps only the answer - a terminal takes the value, never the
    // clearing recipe, and nothing else tells it from a text field.
    ("CLASS_NAME", 30012),
    // Whether the element is a window of its own - compared with zero, for the
    // second look at a browser that is still building its tree (field.rs).
    ("NATIVE_WINDOW_HANDLE", 30020),
    ("IS_TEXT_PATTERN_AVAILABLE", 30040),
    ("IS_VALUE_PATTERN_AVAILABLE", 30043),
    ("VALUE_IS_READ_ONLY", 30046),
];

const SLOTS: &[(&str, u32)] = &[
    ("RELEASE", 2),
    ("GET_FOCUSED_ELEMENT", 8),
    ("GET_CURRENT_PROPERTY_VALUE", 10),
    ("SET_CONNECTION_TIMEOUT", 61),
    ("SET_TRANSACTION_TIMEOUT", 63),
];

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

/// Every `const <PREFIX><NAME>: <ty> = <number>` in `code`.
fn constants(code: &str, prefix: &str) -> BTreeSet<(String, u32)> {
    let mut found = BTreeSet::new();
    for line in code.lines() {
        let Some(at) = line.find(&format!("const {prefix}")) else {
            continue;
        };
        let rest = &line[at + "const ".len() + prefix.len()..];
        let Some((name, tail)) = rest.split_once(':') else {
            continue;
        };
        let Some((_, number)) = tail.split_once('=') else {
            continue;
        };
        let digits: String = number
            .trim()
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        if let Ok(value) = digits.parse() {
            found.insert((name.trim().to_owned(), value));
        }
    }
    found
}

/// Every whole number from the property id range written in `code`.
fn property_range_numbers(code: &str) -> BTreeSet<u32> {
    let mut found = BTreeSet::new();
    let mut digits = String::new();
    for c in code.chars().chain(std::iter::once(' ')) {
        if c.is_ascii_digit() || (c == '_' && !digits.is_empty()) {
            if c != '_' {
                digits.push(c);
            }
        } else {
            if let Ok(number) = digits.parse::<u32>()
                && (30000..30200).contains(&number)
            {
                found.insert(number);
            }
            digits.clear();
        }
    }
    found
}

fn listed(list: &[(&str, u32)]) -> BTreeSet<(String, u32)> {
    list.iter().map(|(n, v)| ((*n).to_owned(), *v)).collect()
}

/// What is wrong with the code of `crates/nkb-sys/src`, joined.
fn findings(code: &str) -> Vec<String> {
    let mut problems = Vec::new();
    let properties = constants(code, "PROPERTY_");
    if properties != listed(PROPERTIES) {
        problems.push(format!(
            "UI Automation properties read: {properties:?}, the list allows {PROPERTIES:?}"
        ));
    }
    let slots = constants(code, "SLOT_");
    if slots != listed(SLOTS) {
        problems.push(format!(
            "COM methods called: {slots:?}, the list allows {SLOTS:?}"
        ));
    }
    let allowed: BTreeSet<u32> = PROPERTIES.iter().map(|(_, v)| *v).collect();
    let stray: Vec<u32> = property_range_numbers(code)
        .difference(&allowed)
        .copied()
        .collect();
    if !stray.is_empty() {
        problems.push(format!("property ids outside the list: {stray:?}"));
    }
    problems
}

#[test]
fn ui_automation_reads_only_the_kind_of_the_focused_control() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut code = String::new();
    for entry in std::fs::read_dir(&src).expect("src is readable") {
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
    let clean = "    pub(super) const PROPERTY_PROCESS_ID: i32 = 30002;\n\
                 pub(super) const PROPERTY_CONTROL_TYPE: i32 = 30003;\n\
                 pub(super) const PROPERTY_CLASS_NAME: i32 = 30012;\n\
                 pub(super) const PROPERTY_NATIVE_WINDOW_HANDLE: i32 = 30020;\n\
                 pub(super) const PROPERTY_IS_TEXT_PATTERN_AVAILABLE: i32 = 30040;\n\
                 pub(super) const PROPERTY_IS_VALUE_PATTERN_AVAILABLE: i32 = 30043;\n\
                 pub(super) const PROPERTY_VALUE_IS_READ_ONLY: i32 = 30046;\n\
                 const SLOT_RELEASE: usize = 2;\n\
                 const SLOT_GET_FOCUSED_ELEMENT: usize = 8;\n\
                 const SLOT_GET_CURRENT_PROPERTY_VALUE: usize = 10;\n\
                 const SLOT_SET_CONNECTION_TIMEOUT: usize = 61;\n\
                 const SLOT_SET_TRANSACTION_TIMEOUT: usize = 63;\n";
    assert!(findings(clean).is_empty(), "{:?}", findings(clean));

    // Each forbidden read must be reported, and the report must name it. A
    // named constant is caught twice - by the list and by the number range.
    let names_it = |code: &str, culprit: &str| {
        let found = findings(code);
        assert!(
            found.iter().any(|problem| problem.contains(culprit)),
            "expected a report naming {culprit}, got {found:?}"
        );
    };
    // The name of the element: one more property.
    names_it(
        &format!("{clean}const PROPERTY_NAME: i32 = 30005;\n"),
        "30005",
    );
    // The value of the element, handed to the call as a raw number.
    names_it(
        &format!("{clean}let v = property(&element, 30_045);\n"),
        "30045",
    );
    // A pattern object - the way to a text range - through a new method.
    names_it(
        &format!("{clean}const SLOT_GET_CURRENT_PATTERN: usize = 16;\n"),
        "GET_CURRENT_PATTERN",
    );
    // A property hidden in a test module is not code of the product.
    let in_test = format!("{clean}#[cfg(test)]\nconst PROPERTY_NAME: i32 = 30005;\n");
    assert!(findings(&code_of(&in_test)).is_empty());
}
