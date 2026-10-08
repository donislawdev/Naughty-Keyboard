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
//! records a target have the same labels in the same order. Two exceptions, each
//! a warning rather than a field of every value: `Delivery:`, present only when
//! the value arrived in part, and `Unicode:`, present only when `Typed:` holds a
//! character a tracker that normalizes text could rewrite (`D70`).
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
use nkb_core::report::{Arrival, ControlKind, ReportBlock, SpelledOut, Target, Typed};
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
    /// Only when normalization could rewrite what `Typed` says.
    Unicode,
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
        ReportLabel::Unicode => "Unicode:",
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
    /// A generated value as code points: the unit's, and how many in a row.
    RecipeCodePoints,
    /// A list of code points cut at `CODE_POINTS_LISTED`.
    CodePointsMore,
    /// `value = ""` is a legal test value, and a label followed by nothing
    /// reads as a line that lost its content.
    EmptyValue,
    /// None of the kinds the shape line names.
    NoShape,
    /// `breaks` or `expect` absent from the pack.
    NotGiven,
    /// Where a typed value went (`D104`): the program's file name and the kind
    /// of control, as read at the press.
    Target,
    /// The same, when the system would not give the program's name.
    TargetUnnamed,
    /// The value went to the clipboard - it is wherever the tester pasted it.
    TargetPasted,
    /// A typed value whose target was not read. Not reached by the palette,
    /// which reads it at every press - said rather than left blank, because
    /// `D68` keeps every line.
    TargetNotRead,
    /// The kind of control in [`ReportPhrase::Target`] - `D73`, `D77`.
    ControlTextField,
    ControlTerminal,
    ControlUnconfirmed,
    ControlNotTextField,
    /// Sessions are not saved yet - `W4`, `session-format.md`.
    SessionNotRecorded,
    /// The value arrived in part.
    Interrupted,
    /// The value went to the clipboard and the tester pasted it (`D71`).
    /// Chosen by the owner on 2026-09-23: an application that checks every
    /// keystroke meets a paste as one event, so the reader has to know it was
    /// not typed.
    Clipboard,
    /// This program and its version.
    Tool,
}

fn pattern_report_phrase(phrase: ReportPhrase) -> &'static str {
    match phrase {
        ReportPhrase::Reference => "{reference} @ pack {version}",
        ReportPhrase::Recipe => "{count} × \"{unit}\"",
        ReportPhrase::RecipeCodePoints => "{count} × {list}",
        ReportPhrase::CodePointsMore => "{list} and {rest} more",
        ReportPhrase::EmptyValue => "empty - the value has no characters",
        ReportPhrase::NoShape => "no invisible characters, no controls, no edge spaces",
        ReportPhrase::NotGiven => "not given in the pack",
        ReportPhrase::Target => "{program} · {control}",
        ReportPhrase::TargetUnnamed => "a program that did not give its name · {control}",
        ReportPhrase::TargetPasted => "not recorded - the tester pasted the value",
        ReportPhrase::TargetNotRead => "not recorded - the target was not read",
        ReportPhrase::ControlTextField => "text field",
        ReportPhrase::ControlTerminal => "terminal",
        ReportPhrase::ControlUnconfirmed => "control not confirmed as a field",
        ReportPhrase::ControlNotTextField => "not a text field",
        ReportPhrase::SessionNotRecorded => "not recorded - this version does not save sessions",
        ReportPhrase::Interrupted => {
            "interrupted after {sent} of {expected} UTF-16 units - the field holds a fragment"
        }
        ReportPhrase::Clipboard => "placed on the clipboard - pasted by the tester, not typed",
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
    ];
    if let Some(spelled) = &block.spelled_out {
        lines.push((ReportLabel::Unicode, spelled_out(spelled)));
    }
    lines.extend([
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
    ]);
    // The conditional line: only when the value did NOT go into the field by
    // being typed whole - `D68` keeps it a warning rather than a field of
    // every value, and `D71` gives it its second reason.
    match block.arrival {
        Arrival::Whole => {}
        Arrival::Interrupted {
            units_sent,
            units_expected,
        } => lines.push((
            ReportLabel::Delivery,
            fill(
                pattern_report_phrase(ReportPhrase::Interrupted),
                &[
                    ("sent", &units_sent.to_string()),
                    ("expected", &units_expected.to_string()),
                ],
            ),
        )),
        Arrival::OnClipboard => lines.push((
            ReportLabel::Delivery,
            pattern_report_phrase(ReportPhrase::Clipboard).to_owned(),
        )),
    }
    lines.extend([
        (ReportLabel::Breaks, given(block.breaks.as_deref())),
        (ReportLabel::Expected, given(block.expected.as_deref())),
        (ReportLabel::Target, target(block)),
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

/// How many code points the `Unicode:` line lists before it says how many more.
///
/// The number the palette's preview starts eliding at (`D64`), so the two
/// surfaces agree on what "long" means. The longest value that earns the line in
/// the shipped packs has eleven, so every one of them is listed whole.
const CODE_POINTS_LISTED: usize = 100;

/// The `Unicode:` line: the value as code points.
///
/// Written `U+0065`, the standard's own notation and the one the palette's note
/// under the preview uses (`i18n::not_guaranteed`). It is ASCII, and ASCII is
/// what no normalization rewrites - which is the whole reason the line exists.
fn spelled_out(spelled: &SpelledOut) -> String {
    match spelled {
        SpelledOut::Written(characters) => code_points(characters),
        SpelledOut::Recipe { unit, count } => fill(
            pattern_report_phrase(ReportPhrase::RecipeCodePoints),
            &[("count", &count.to_string()), ("list", &code_points(unit))],
        ),
    }
}

fn code_points(characters: &[char]) -> String {
    let list = characters
        .iter()
        .take(CODE_POINTS_LISTED)
        .map(|c| format!("U+{:04X}", u32::from(*c)))
        .collect::<Vec<_>>()
        .join(" ");
    let rest = characters.len().saturating_sub(CODE_POINTS_LISTED);
    if rest == 0 {
        list
    } else {
        fill(
            pattern_report_phrase(ReportPhrase::CodePointsMore),
            &[("list", &list), ("rest", &rest.to_string())],
        )
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

/// The `Target:` line (`D104`). The program's name comes from the system and is
/// written as prose - a file name cannot hold a line break on Windows, and
/// escaping it anyway costs nothing on the one system where it could.
fn target(block: &ReportBlock) -> String {
    match &block.target {
        Some(Target { program, field }) => {
            let control = pattern_report_phrase(match field {
                ControlKind::TextField => ReportPhrase::ControlTextField,
                ControlKind::Terminal => ReportPhrase::ControlTerminal,
                ControlKind::Unconfirmed => ReportPhrase::ControlUnconfirmed,
                ControlKind::NotTextField => ReportPhrase::ControlNotTextField,
            });
            match program {
                Some(program) => fill(
                    pattern_report_phrase(ReportPhrase::Target),
                    &[("program", &prose(program)), ("control", control)],
                ),
                None => fill(
                    pattern_report_phrase(ReportPhrase::TargetUnnamed),
                    &[("control", control)],
                ),
            }
        }
        None if block.arrival == Arrival::OnClipboard => {
            pattern_report_phrase(ReportPhrase::TargetPasted).to_owned()
        }
        None => pattern_report_phrase(ReportPhrase::TargetNotRead).to_owned(),
    }
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
            spelled_out: None,
            shape: vec![ShapeFact::ZeroWidth(3)],
            graphemes: 7,
            code_points: 7,
            bytes: 13,
            breaks: Some("Counters disagree.".to_owned()),
            expected: None,
            arrival: Arrival::Whole,
            target: Some(Target {
                program: Some("notepad.exe".to_owned()),
                field: ControlKind::TextField,
            }),
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
            "Target:   notepad.exe · text field",
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
    fn a_pasted_value_gets_a_delivery_line_in_the_same_place() {
        // D71: same label, same place, one more reason. The rest of the block
        // keeps its shape - D68.
        let mut pasted = block();
        pasted.arrival = Arrival::OnClipboard;
        let text = report_text(&pasted);
        let got = lines(&text);
        assert_eq!(
            got[5],
            "Delivery: placed on the clipboard - pasted by the tester, not typed"
        );
        assert!(got[6].starts_with("Breaks:   "));
        assert_eq!(
            got.len(),
            lines(&report_text(&block())).len() + 1,
            "exactly one line more than a typed value"
        );
    }

    #[test]
    fn a_value_normalization_could_rewrite_is_spelled_out_right_after_typed() {
        let mut accented = block();
        accented.typed = Typed::Written(LiteralText::new("e\u{0301}").escape());
        accented.spelled_out = Some(SpelledOut::Written(vec!['e', '\u{0301}']));
        let text = report_text(&accented);
        let got = lines(&text);
        assert_eq!(got[2], "Typed:    e\u{0301}");
        assert_eq!(got[3], "Unicode:  U+0065 U+0301");
        assert!(
            got[4].starts_with("Shape:    "),
            "the value column does not move: {text}"
        );
        assert_eq!(got.len(), 11, "one line more than a value without it");
    }

    #[test]
    fn a_generated_value_is_spelled_out_as_its_recipe() {
        let mut generated = block();
        generated.typed = Typed::Recipe {
            unit: LiteralText::new("e\u{0301}").escape(),
            count: 100_000,
        };
        generated.spelled_out = Some(SpelledOut::Recipe {
            unit: vec!['e', '\u{0301}'],
            count: 100_000,
        });
        let text = report_text(&generated);
        assert!(text.contains("Unicode:  100000 × U+0065 U+0301"), "{text}");
    }

    #[test]
    fn a_long_list_says_how_many_it_left_out_and_lists_every_one_up_to_the_limit() {
        let mut long = block();
        long.spelled_out = Some(SpelledOut::Written(vec!['\u{FF41}'; CODE_POINTS_LISTED]));
        let whole = report_text(&long);
        assert_eq!(
            lines(&whole)[3].matches("U+FF41").count(),
            CODE_POINTS_LISTED
        );
        assert!(!whole.contains("more"), "at the limit nothing is left out");

        long.spelled_out = Some(SpelledOut::Written(vec![
            '\u{FF41}';
            CODE_POINTS_LISTED + 7
        ]));
        let cut = report_text(&long);
        let line = lines(&cut)[3];
        assert_eq!(line.matches("U+FF41").count(), CODE_POINTS_LISTED);
        assert!(line.ends_with("U+FF41 and 7 more"), "{line}");
    }

    #[test]
    fn a_code_point_beyond_the_basic_plane_is_written_with_every_digit() {
        let mut bold = block();
        bold.spelled_out = Some(SpelledOut::Written(vec!['\u{1D407}', 'a']));
        assert!(report_text(&bold).contains("Unicode:  U+1D407 U+0061"));
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

    fn target_line(block: &ReportBlock) -> String {
        let text = report_text(block);
        lines(&text)
            .into_iter()
            .find(|line| line.starts_with("Target:"))
            .expect("the line is always there")
            .to_owned()
    }

    #[test]
    fn the_target_line_names_the_program_and_the_kind_of_control_or_says_why_not() {
        let with = |program: Option<&str>, field| ReportBlock {
            target: Some(Target {
                program: program.map(ToOwned::to_owned),
                field,
            }),
            ..block()
        };
        assert_eq!(
            target_line(&with(Some("WindowsTerminal.exe"), ControlKind::Terminal)),
            "Target:   WindowsTerminal.exe · terminal"
        );
        assert_eq!(
            target_line(&with(Some("scalc.exe"), ControlKind::Unconfirmed)),
            "Target:   scalc.exe · control not confirmed as a field"
        );
        assert_eq!(
            target_line(&with(None, ControlKind::NotTextField)),
            "Target:   a program that did not give its name · not a text field"
        );
        let pasted = ReportBlock {
            target: None,
            arrival: Arrival::OnClipboard,
            ..block()
        };
        assert_eq!(
            target_line(&pasted),
            "Target:   not recorded - the tester pasted the value"
        );
        let unread = ReportBlock {
            target: None,
            ..block()
        };
        assert_eq!(
            target_line(&unread),
            "Target:   not recorded - the target was not read"
        );
    }

    #[test]
    fn a_program_name_cannot_forge_a_line_of_the_block() {
        // Not a Windows file name - the rule holds wherever the name comes from.
        let hostile = ReportBlock {
            target: Some(Target {
                program: Some("evil\nSession:  forged {control}".to_owned()),
                field: ControlKind::TextField,
            }),
            ..block()
        };
        let text = report_text(&hostile);
        assert_eq!(lines(&text).len(), 10, "still ten lines");
        assert_eq!(
            target_line(&hostile),
            "Target:   evil\\u000ASession:  forged {control} · text field",
            "escaped, and a placeholder in the name stays a placeholder"
        );
    }
}
