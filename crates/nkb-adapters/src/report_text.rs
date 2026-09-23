//! The report block as text: what `Ctrl+Alt+B` puts on the clipboard.
//!
//! `ux-spec.md` 7 draws it and `D68` fixes its shape. This module is the
//! adapter behind `nkb_app::ports::ReportText`, and it holds the WORDS - the
//! facts come from `nkb_core::report`.
//!
//! # English, always - and why that is not the palette's rule
//!
//! The palette is translated, this block is not. Its reader is not the tester
//! who pressed the shortcut but whoever opens the ticket, and a team's tracker
//! is one language no matter which language each tester's palette speaks. Labels
//! that followed the tester would put `Value:` in one ticket and `Wartość:` in
//! the next, and nobody could search for either. So these keys are like the CLI's
//! (`00-ODSTEPSTWA.md`): keys, because untouchable rule 9 says text is never a
//! literal at its call site, with exactly one value each, forever.
//!
//! That also settles plurals here without `OBS-120`: English has two forms, and
//! a text that is English by decision may use them.
//!
//! # A fixed shape
//!
//! Every line is always present - `Target:` and `Session:` included, today with
//! "not recorded" - so a block from this version and one from the version that
//! records a target have the same labels in the same order. The one exception is
//! `Delivery:`, present only when the value arrived in part, because that is an
//! abnormal event a reader must not be able to miss, not a field of every value.
//!
//! # Somebody else's prose is escaped before it goes out
//!
//! 🔴 `name`, `breaks` and `expect` come from a pack, and packs are untrusted
//! (untouchable rule 24). `E020` keeps control characters out of `value` only, so
//! a pack's prose may carry a line break - and a name holding
//! `"\nTarget:  ...bank login"` would forge a line in a ticket a developer
//! trusts. A direction override would reorder what the developer reads. So prose
//! goes through the same escape the value does: control, format and unusual
//! white-space characters become `\uXXXX`, visible and inert.

use nkb_app::ports::ReportText;
use nkb_core::preview::{ShapeFact, is_invisible};
use nkb_core::report::{Arrival, ReportBlock, Typed};
use nkb_core::text::{escaped_char, needs_escaping};

use crate::i18n::{fill, shape_line};

/// The report block in English, ready to paste.
#[derive(Debug, Clone, Copy, Default)]
pub struct EnglishReport;

impl ReportText for EnglishReport {
    fn report_text(&self, block: &ReportBlock) -> String {
        report_text(block)
    }
}

/// The label at the start of each line of the block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportLabel {
    Value,
    Name,
    Typed,
    Shape,
    Size,
    /// Only when the value arrived in part.
    Delivery,
    Breaks,
    Expected,
    Target,
    Session,
    Tool,
}

fn pattern_report_label(label: ReportLabel) -> &'static str {
    match label {
        ReportLabel::Value => "Value:",
        ReportLabel::Name => "Name:",
        ReportLabel::Typed => "Typed:",
        ReportLabel::Shape => "Shape:",
        ReportLabel::Size => "Size:",
        ReportLabel::Delivery => "Delivery:",
        ReportLabel::Breaks => "Breaks:",
        ReportLabel::Expected => "Expected:",
        ReportLabel::Target => "Target:",
        ReportLabel::Session => "Session:",
        ReportLabel::Tool => "Tool:",
    }
}

/// What stands after a label when it is not a plain fact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportPhrase {
    /// The value and the pack version it came from - `pack-format.md` 7.
    Reference,
    /// A generated value, exact at every length.
    Recipe,
    /// `value = ""` is a legal test value, and a label followed by nothing
    /// reads as a line that lost its content.
    EmptyValue,
    /// None of the kinds the shape line names.
    NoShape,
    /// `breaks` or `expect` absent from the pack.
    NotGiven,
    /// `TargetInspector` does not exist - `OBS-91`.
    TargetNotRecorded,
    /// Sessions are not saved yet - `W4`, `session-format.md`.
    SessionNotRecorded,
    /// The value arrived in part.
    Interrupted,
    /// This program and its version.
    Tool,
}

fn pattern_report_phrase(phrase: ReportPhrase) -> &'static str {
    match phrase {
        ReportPhrase::Reference => "{reference} @ pack {version}",
        ReportPhrase::Recipe => "{count} × \"{unit}\"",
        ReportPhrase::EmptyValue => "empty - the value has no characters",
        ReportPhrase::NoShape => "no invisible characters, no controls, no edge spaces",
        ReportPhrase::NotGiven => "not given in the pack",
        ReportPhrase::TargetNotRecorded => {
            "not recorded - this version does not read the target window"
        }
        ReportPhrase::SessionNotRecorded => "not recorded - this version does not save sessions",
        ReportPhrase::Interrupted => {
            "interrupted after {sent} of {expected} UTF-16 units - the field holds a fragment"
        }
        ReportPhrase::Tool => "Naughty Keyboard {version}",
    }
}

/// One of the three sizes on the `Size:` line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportCount {
    Graphemes,
    CodePoints,
    Bytes,
}

/// English inflects the noun, so each size has two patterns - the shape
/// `ChooseError::Refused` uses in `i18n`.
fn pattern_report_count(count: ReportCount, one: bool) -> &'static str {
    match count {
        ReportCount::Graphemes if one => "1 grapheme",
        ReportCount::Graphemes => "{n} graphemes",
        ReportCount::CodePoints if one => "1 code point",
        ReportCount::CodePoints => "{n} code points",
        ReportCount::Bytes if one => "1 byte",
        ReportCount::Bytes => "{n} bytes",
    }
}

/// How lines end. The system's own convention, because the block is pasted on
/// this machine: old Notepad and several Windows mail clients show a
/// line-feed-only text as one line. Safe to choose here and nowhere else - the
/// block's content is escaped, so every line break in it is one this module put
/// there. (A VALUE sent through clipboard mode must never be converted this way:
/// its line breaks are the test.)
#[cfg(windows)]
const LINE_END: &str = "\r\n";
#[cfg(not(windows))]
const LINE_END: &str = "\n";

/// The block, every line finished, without a line break after the last one -
/// pasting into a single-line field must not leave an empty line behind.
#[must_use]
pub fn report_text(block: &ReportBlock) -> String {
    let mut lines: Vec<(ReportLabel, String)> = vec![
        (
            ReportLabel::Value,
            fill(
                pattern_report_phrase(ReportPhrase::Reference),
                &[
                    ("reference", &prose(&block.reference)),
                    ("version", &prose(&block.pack_version)),
                ],
            ),
        ),
        (ReportLabel::Name, prose(&block.name)),
        (ReportLabel::Typed, typed(&block.typed)),
        (ReportLabel::Shape, shape(&block.shape)),
        (
            ReportLabel::Size,
            [
                count(ReportCount::Graphemes, block.graphemes),
                count(ReportCount::CodePoints, block.code_points),
                count(ReportCount::Bytes, block.bytes),
            ]
            .join(" · "),
        ),
    ];
    if let Arrival::Interrupted {
        units_sent,
        units_expected,
    } = block.arrival
    {
        lines.push((
            ReportLabel::Delivery,
            fill(
                pattern_report_phrase(ReportPhrase::Interrupted),
                &[
                    ("sent", &units_sent.to_string()),
                    ("expected", &units_expected.to_string()),
                ],
            ),
        ));
    }
    lines.extend([
        (ReportLabel::Breaks, given(block.breaks.as_deref())),
        (ReportLabel::Expected, given(block.expected.as_deref())),
        (
            ReportLabel::Target,
            pattern_report_phrase(ReportPhrase::TargetNotRecorded).to_owned(),
        ),
        (
            ReportLabel::Session,
            pattern_report_phrase(ReportPhrase::SessionNotRecorded).to_owned(),
        ),
        (
            ReportLabel::Tool,
            fill(
                pattern_report_phrase(ReportPhrase::Tool),
                &[("version", env!("CARGO_PKG_VERSION"))],
            ),
        ),
    ]);

    // Values start in one column: the widest label present, plus one space. The
    // width is worked out rather than written down, so a label that changes
    // length moves the column instead of breaking it.
    let width = lines
        .iter()
        .map(|(label, _)| pattern_report_label(*label).chars().count())
        .max()
        .unwrap_or(0)
        + 1;
    lines
        .iter()
        .map(|(label, text)| {
            let label = pattern_report_label(*label);
            let pad = width - label.chars().count();
            format!("{label}{}{text}", " ".repeat(pad))
        })
        .collect::<Vec<_>>()
        .join(LINE_END)
}

fn typed(typed: &Typed) -> String {
    match typed {
        Typed::Written(escaped) if escaped.as_str().is_empty() => {
            pattern_report_phrase(ReportPhrase::EmptyValue).to_owned()
        }
        Typed::Written(escaped) => escaped.as_str().to_owned(),
        Typed::Recipe { unit, count } => fill(
            pattern_report_phrase(ReportPhrase::Recipe),
            &[("count", &count.to_string()), ("unit", unit.as_str())],
        ),
    }
}

/// The shape line.
///
/// ⚠️ Borrowed from `i18n`, which is the palette's TRANSLATED table - correct
/// only because English is its one language today. The day `pattern_shape`
/// gains a language, this call has to ask for English explicitly (`D68`), and
/// the compiler will stop here to make somebody do it.
fn shape(facts: &[ShapeFact]) -> String {
    if facts.is_empty() {
        pattern_report_phrase(ReportPhrase::NoShape).to_owned()
    } else {
        shape_line(facts)
    }
}

fn count(unit: ReportCount, n: usize) -> String {
    fill(pattern_report_count(unit, n == 1), &[("n", &n.to_string())])
}

fn given(text: Option<&str>) -> String {
    text.map_or_else(
        || pattern_report_phrase(ReportPhrase::NotGiven).to_owned(),
        prose,
    )
}

/// Somebody else's text, made safe to put on a line of its own.
fn prose(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        if escapes_in_prose(character) {
            out.push_str(&escaped_char(character));
        } else {
            out.push(character);
        }
    }
    out
}

/// Whether a character of pack prose is written escaped in the block.
///
/// 🔴 The UNION of the core's two rules, minus the ordinary space - and the
/// union is not decoration. `needs_escaping` is the pack format's rule and
/// `is_invisible` the preview's, and the two drifted apart once: the format's
/// did not know the bidi isolates `U+2066`-`U+2069`, so a name holding `U+2067`
/// would have reordered a developer's ticket - half of what "Trojan Source" is
/// made of. `D69` aligned them and a core test now walks every code point, but
/// the union stays: it costs nothing, and a ticket is where a future drift
/// would do its damage before anyone read the test.
fn escapes_in_prose(character: char) -> bool {
    character != ' ' && (needs_escaping(character, false) || is_invisible(character))
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;
    use nkb_core::text::LiteralText;

    fn block() -> ReportBlock {
        ReportBlock {
            reference: "whitespace/zero-width-space-x3".to_owned(),
            pack_version: "1.0".to_owned(),
            name: "Three zero-width spaces".to_owned(),
            typed: Typed::Written(LiteralText::new("ab\u{200B}\u{200B}\u{200B}cd").escape()),
            shape: vec![ShapeFact::ZeroWidth(3)],
            graphemes: 7,
            code_points: 7,
            bytes: 13,
            breaks: Some("Counters disagree.".to_owned()),
            expected: None,
            arrival: Arrival::Whole,
        }
    }

    fn lines(text: &str) -> Vec<&str> {
        text.split(LINE_END).collect()
    }

    #[test]
    fn the_block_has_the_shape_d68_fixes_line_for_line() {
        let text = report_text(&block());
        let expected = [
            "Value:    whitespace/zero-width-space-x3 @ pack 1.0",
            "Name:     Three zero-width spaces",
            "Typed:    ab\\u200B\\u200B\\u200Bcd",
            "Shape:    zero-width × 3",
            "Size:     7 graphemes · 7 code points · 13 bytes",
            "Breaks:   Counters disagree.",
            "Expected: not given in the pack",
            "Target:   not recorded - this version does not read the target window",
            "Session:  not recorded - this version does not save sessions",
        ];
        let got = lines(&text);
        assert_eq!(&got[..expected.len()], &expected[..]);
        assert_eq!(
            got[expected.len()],
            format!("Tool:     Naughty Keyboard {}", env!("CARGO_PKG_VERSION"))
        );
        assert_eq!(got.len(), expected.len() + 1, "no line beyond Tool");
        assert!(
            !text.ends_with('\n'),
            "no break after the last line: a single-line field would keep it"
        );
    }

    #[test]
    fn lines_end_the_way_this_system_ends_them_and_nowhere_else() {
        let text = report_text(&block());
        assert_eq!(text.matches(LINE_END).count(), 9, "ten lines, nine breaks");
        if cfg!(windows) {
            assert_eq!(
                text.matches('\n').count(),
                text.matches("\r\n").count(),
                "every line feed is part of a CRLF on Windows"
            );
        }
    }

    #[test]
    fn one_of_anything_is_singular() {
        let mut one = block();
        one.typed = Typed::Written(LiteralText::new("\u{00A0}").escape());
        one.graphemes = 1;
        one.code_points = 1;
        one.bytes = 1;
        let text = report_text(&one);
        assert!(
            text.contains("Size:     1 grapheme · 1 code point · 1 byte"),
            "{text}"
        );
    }

    #[test]
    fn a_generated_value_is_typed_as_its_recipe() {
        let mut generated = block();
        generated.typed = Typed::Recipe {
            unit: LiteralText::new("a").escape(),
            count: 100_000,
        };
        generated.shape = Vec::new();
        let text = report_text(&generated);
        assert!(text.contains("Typed:    100000 × \"a\""), "{text}");
        assert!(
            text.contains("Shape:    no invisible characters, no controls, no edge spaces"),
            "{text}"
        );
    }

    #[test]
    fn an_empty_value_is_named_not_left_blank() {
        let mut empty = block();
        empty.typed = Typed::Written(LiteralText::new("").escape());
        let text = report_text(&empty);
        assert!(
            text.contains("Typed:    empty - the value has no characters"),
            "{text}"
        );
    }

    #[test]
    fn an_interrupted_value_gets_a_delivery_line_before_what_it_breaks() {
        let mut cut = block();
        cut.arrival = Arrival::Interrupted {
            units_sent: 3,
            units_expected: 7,
        };
        let text = report_text(&cut);
        let got = lines(&text);
        assert_eq!(
            got[5],
            "Delivery: interrupted after 3 of 7 UTF-16 units - the field holds a fragment"
        );
        assert!(got[6].starts_with("Breaks:   "));
    }

    #[test]
    fn a_hostile_pack_cannot_forge_a_line_or_reorder_the_ticket() {
        // Untouchable rule 24: pack prose is untrusted, and E020 guards only
        // `value`. A line break would forge a `Target:`, a direction override
        // would make the developer read the line backwards.
        let mut hostile = block();
        hostile.name = "Innocent\nTarget:   Bank - login".to_owned();
        hostile.breaks = Some("abc\u{202E}fed".to_owned());
        // The isolate is the half the pack format's own rule does not know
        // (OBS-125). A check on U+202E alone passed while this went through.
        hostile.expected = Some("abc\u{2067}fed".to_owned());
        let text = report_text(&hostile);

        assert_eq!(
            lines(&text)
                .iter()
                .filter(|line| line.starts_with("Target:"))
                .count(),
            1,
            "one Target: line - the forged one stays inside Name, escaped"
        );
        assert_eq!(lines(&text).len(), 10, "still ten lines");
        assert!(text.contains("Name:     Innocent\\u000ATarget:   Bank - login"));
        assert!(text.contains("Breaks:   abc\\u202Efed"));
        assert!(text.contains("Expected: abc\\u2067fed"));
        assert!(!text.contains('\u{202E}'));
        assert!(!text.contains('\u{2067}'));
    }
}
