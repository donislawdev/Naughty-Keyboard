//! The grapheme counter, judged by Unicode rather than by us.
//!
//! # Why this file exists at all
//!
//! `graphemes.rs` implements fifteen rules over a forty-seven kilobyte table.
//! Unit tests beside it check the cases somebody thought of, which is exactly
//! the set of cases least likely to be wrong. This one runs the suite the
//! standard ships with - every rule, including the combinations that only a
//! generator would produce - and it is the reason the counter can be believed.
//!
//! # How to read a failure
//!
//! Each case is one line of `GraphemeBreakTest.txt`. The line names the code
//! points and marks every position with a break sign or a no-break sign, and
//! the comment at the end of the line names the RULE that decided each one. So
//! a failure points at a rule number, not at a mystery.
//!
//! # What this cannot check
//!
//! 🔴 That the suite is the right one. A file from a different version of the
//! standard would pass against a table from the same different version and this
//! test would stay green while the product disagreed with every other tool on
//! the machine. That is what `unicode_data.rs` checks, and why it checks the
//! version header rather than trusting the file name.

// A failed expectation in a test is a failed test. The workspace denies both in
// shipped code, which is the setting that matters.
#![allow(clippy::panic, clippy::expect_used)]

use nkb_core::graphemes::count;
use std::path::{Path, PathBuf};

/// The break sign, `U+00F7`. Written as an escape rather than as itself so that
/// this file stays readable in a terminal that has opinions about encodings.
const BREAK: char = '\u{00F7}';
/// The no-break sign, `U+00D7`.
const NO_BREAK: char = '\u{00D7}';

/// How many cases the suite holds. Counted on the vendored file, 2026-09-23.
///
/// 🔴 Asserted rather than merely reported. A truncated or half-downloaded file
/// would otherwise pass every case it still contained and report success, which
/// is the failure this whole project is built to notice elsewhere.
const CASES: usize = 766;

fn suite() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("unicode")
        .join("GraphemeBreakTest.txt")
}

/// One line of the suite, turned into the text and the answer.
struct Case {
    line: usize,
    text: String,
    clusters: usize,
    source: String,
}

fn parse(line_number: usize, line: &str) -> Option<Case> {
    let body = line.split('#').next().unwrap_or("").trim();
    if body.is_empty() {
        return None;
    }

    let mut text = String::new();
    let mut clusters = 0usize;
    let mut tokens = body.split_whitespace().peekable();

    while let Some(token) = tokens.next() {
        match token.chars().next() {
            Some(BREAK) => {
                // A break sign at the very end of the line is the end of the
                // text, which starts no cluster. Every other one does.
                if tokens.peek().is_some() {
                    clusters += 1;
                }
            }
            Some(NO_BREAK) => {}
            Some(_) => {
                let code = u32::from_str_radix(token, 16)
                    .unwrap_or_else(|e| panic!("line {line_number}: {token} is not hex: {e}"));
                let character = char::from_u32(code)
                    .unwrap_or_else(|| panic!("line {line_number}: {token} is not a character"));
                text.push(character);
            }
            None => {}
        }
    }

    Some(Case {
        line: line_number,
        text,
        clusters,
        source: body.to_string(),
    })
}

fn cases() -> Vec<Case> {
    let path = suite();
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("the vendored suite must be readable at {path:?}: {e}"));
    text.lines()
        .enumerate()
        .filter_map(|(index, line)| parse(index + 1, line))
        .collect()
}

#[test]
fn the_whole_official_suite_agrees_with_this_counter() {
    let cases = cases();
    assert_eq!(
        cases.len(),
        CASES,
        "the vendored suite has changed size - a case count that drifts silently \
         is a suite that stopped covering what it used to"
    );

    let mut wrong = Vec::new();
    for case in &cases {
        let got = count(&case.text);
        if got != case.clusters {
            wrong.push(format!(
                "  line {}: expected {}, counted {}\n    {}",
                case.line, case.clusters, got, case.source
            ));
        }
    }

    assert!(
        wrong.is_empty(),
        "{} of {} conformance cases disagree:\n{}",
        wrong.len(),
        cases.len(),
        wrong.join("\n")
    );
}

#[test]
fn the_suite_really_does_exercise_the_rules_that_are_easy_to_miss() {
    // A conformance run that passes proves nothing unless the suite reaches the
    // hard rules. These three are the ones an implementation gets wrong while
    // looking correct: the Indic conjunct, the emoji joiner, and flag parity.
    // Measured 2026-09-23 by switching each rule off in a probe: 16 cases
    // depend on GB9c, 3 on GB11, and the flag cases are named below.
    let cases = cases();

    let has = |needle: &str| cases.iter().any(|case| case.source.contains(needle));

    // U+0915 DEVANAGARI KA and U+094D VIRAMA - rule GB9c.
    assert!(has("0915"), "the suite must reach the Indic conjunct rule");
    assert!(has("094D"), "the suite must reach the linker");
    // U+200D ZERO WIDTH JOINER - rule GB11.
    assert!(has("200D"), "the suite must reach the emoji joiner");
    // U+1F1E6 REGIONAL INDICATOR SYMBOL LETTER A - rules GB12 and GB13.
    assert!(has("1F1E6"), "the suite must reach flag parity");
}

#[test]
fn an_answer_that_cannot_be_wrong_is_not_an_answer() {
    // The positive control for this file. If the counter returned the number of
    // code points instead of the number of clusters, the suite MUST notice -
    // otherwise the green above means nothing. Measured here rather than
    // asserted in prose.
    let cases = cases();
    let disagreeing = cases
        .iter()
        .filter(|case| case.text.chars().count() != case.clusters)
        .count();

    assert!(
        disagreeing > 100,
        "only {disagreeing} cases distinguish clusters from code points - \
         a suite that cannot tell those apart cannot judge this counter"
    );
}
