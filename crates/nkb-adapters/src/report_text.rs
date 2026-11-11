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
//! the value did not go in typed whole and followed into the application, or
//! when the system's text substitutions could have changed it - every warning
//! the palette gives about a value that went in - and `Unicode:`, present only
//! when `Typed:` holds a character a tracker that normalizes text could rewrite
//! (`D70`).
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
use nkb_core::preview::ShapeFact;
use nkb_core::report::{Arrival, ControlKind, PackFolder, ReportBlock, SpelledOut, Target, Typed};
use nkb_core::text::escaped_prose;

use crate::i18n::{fill, shape_line, substitution_names};

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
    /// The same, for a pack from a team's folder (`D173`): a developer looking
    /// the reference up in the published catalogue would find another pack,
    /// or none. The folder by kind, never by path - a path names the tester.
    ReferenceTeam,
    /// The same, for a pack from the tester's own folder.
    ReferenceOwn,
    /// A value the tool made for a limit (`D178`): no pack and no version to
    /// look up, so the line gives the command that makes the same values.
    ReferenceMade,
    /// A generated value, exact at every length.
    Recipe,
    /// A generated value as code points: the unit's, and how many in a row.
    RecipeCodePoints,
    /// A generated value of two parts, each written as a recipe is: the
    /// padding, and what follows it.
    PaddedRecipe,
    /// A counterstring, as a recipe a reader can make again - with what one
    /// is, because the word alone does not say.
    Counterstring,
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
    /// `D125`: Firefox keeping accessibility services out.
    ControlClosedToAccessibility,
    /// Sessions are not saved yet - `W4`, `session-format.md`.
    SessionNotRecorded,
    /// The value arrived in part.
    Interrupted,
    /// The send stopped with keys still on their way, so the count is a floor
    /// (`D158`).
    InFlight,
    /// The value went to the clipboard and the tester pasted it (`D71`).
    /// Chosen by the owner on 2026-09-23: an application that checks every
    /// keystroke meets a paste as one event, so the reader has to know it was
    /// not typed.
    Clipboard,
    /// Every key pressed, and only the system confirmed them - the palette's
    /// `NotPaced` (`D95`, `OBS-158`).
    TakenBySystem,
    /// Handed to the desktop's input method - the palette's `HandedOver`
    /// (`D145`).
    HandedOver,
    /// The system's text substitutions that were on could have changed the
    /// value - the palette's `MayBeAltered` (`D139`). Could, never did: the
    /// tool does not look into the field.
    MayBeAltered,
    /// This program and its version.
    Tool,
}

fn pattern_report_phrase(phrase: ReportPhrase) -> &'static str {
    match phrase {
        ReportPhrase::Reference => "{reference} @ pack {version}",
        ReportPhrase::ReferenceTeam => "{reference} @ pack {version}, from a team pack folder",
        ReportPhrase::ReferenceOwn => {
            "{reference} @ pack {version}, from the tester's own pack folder"
        }
        ReportPhrase::ReferenceMade => "{reference}, from {command}",
        ReportPhrase::Recipe => "{count} × \"{unit}\"",
        ReportPhrase::RecipeCodePoints => "{count} × {list}",
        ReportPhrase::PaddedRecipe => "{padding} + {rest}",
        ReportPhrase::Counterstring => {
            "counterstring of {length} characters - every star stands at the position the number before it says"
        }
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
        ReportPhrase::ControlClosedToAccessibility => "window closed to accessibility",
        ReportPhrase::SessionNotRecorded => "not recorded - this version does not save sessions",
        ReportPhrase::Interrupted => {
            "interrupted after {sent} of {expected} UTF-16 units - the field holds a fragment"
        }
        ReportPhrase::InFlight => {
            "interrupted after at least {sent} of {expected} UTF-16 units - more may have arrived after the stop"
        }
        ReportPhrase::Clipboard => "placed on the clipboard - pasted by the tester, not typed",
        ReportPhrase::TakenBySystem => {
            "every key pressed, but only the system confirmed them - characters may be missing if the application was busy"
        }
        ReportPhrase::HandedOver => {
            "handed to the desktop's input method, which gives it to whatever has the keyboard focus - nothing confirms that a field received it"
        }
        ReportPhrase::MayBeAltered => {
            "the system's text substitutions were on and may have changed the value: {what}"
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
        (ReportLabel::Value, reference(block)),
        (ReportLabel::Name, escaped_prose(&block.name)),
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
    if let Some(delivery) = delivery(block) {
        lines.push((ReportLabel::Delivery, delivery));
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

/// The `Value:` line: which value, and where a reader finds it again - a pack
/// and its version, or the command that makes the values of a limit.
fn reference(block: &ReportBlock) -> String {
    let reference = escaped_prose(&block.reference);
    if let Some(limit) = block.made_for {
        return fill(
            pattern_report_phrase(ReportPhrase::ReferenceMade),
            &[
                ("reference", &reference),
                ("command", &crate::boundary_text::command(limit)),
            ],
        );
    }
    fill(
        pattern_report_phrase(match block.folder {
            None => ReportPhrase::Reference,
            Some(PackFolder::Team) => ReportPhrase::ReferenceTeam,
            Some(PackFolder::Own) => ReportPhrase::ReferenceOwn,
        }),
        &[
            ("reference", &reference),
            ("version", &escaped_prose(&block.pack_version)),
        ],
    )
}

fn typed(typed: &Typed) -> String {
    match typed {
        Typed::Written(escaped) if escaped.as_str().is_empty() => {
            pattern_report_phrase(ReportPhrase::EmptyValue).to_owned()
        }
        Typed::Written(escaped) => escaped.as_str().to_owned(),
        Typed::Recipe { unit, count } => recipe(unit.as_str(), *count),
        Typed::Padded {
            padding,
            padding_count,
            unit,
            count,
        } => fill(
            pattern_report_phrase(ReportPhrase::PaddedRecipe),
            &[
                ("padding", &recipe(padding.as_str(), *padding_count)),
                ("rest", &recipe(unit.as_str(), *count)),
            ],
        ),
        Typed::Counterstring { length } => fill(
            pattern_report_phrase(ReportPhrase::Counterstring),
            &[("length", &length.to_string())],
        ),
    }
}

/// A unit and how many of it in a row, the unit already escaped.
fn recipe(unit: &str, count: u32) -> String {
    fill(
        pattern_report_phrase(ReportPhrase::Recipe),
        &[("count", &count.to_string()), ("unit", unit)],
    )
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
        SpelledOut::Recipe { unit, count } => recipe_code_points(unit, *count),
        SpelledOut::Padded {
            padding,
            padding_count,
            unit,
            count,
        } => fill(
            pattern_report_phrase(ReportPhrase::PaddedRecipe),
            &[
                ("padding", &recipe_code_points(padding, *padding_count)),
                ("rest", &recipe_code_points(unit, *count)),
            ],
        ),
    }
}

/// A unit as code points and how many of it in a row.
fn recipe_code_points(unit: &[char], count: u32) -> String {
    fill(
        pattern_report_phrase(ReportPhrase::RecipeCodePoints),
        &[("count", &count.to_string()), ("list", &code_points(unit))],
    )
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
                ControlKind::ClosedToAccessibility => ReportPhrase::ControlClosedToAccessibility,
            });
            match program {
                Some(program) => fill(
                    pattern_report_phrase(ReportPhrase::Target),
                    &[("program", &escaped_prose(program)), ("control", control)],
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
        escaped_prose,
    )
}

/// The `Delivery:` line of a value cut short, exact or a floor (`D158`).
/// The conditional `Delivery:` line: only when the value did NOT go into the
/// field by being typed whole and followed into the application, untouched by
/// the system on the way - `D68` keeps it a warning rather than a field of every
/// value. `D71` gave it the clipboard, `D158` the keys on their way, and the
/// palette's three warnings of a value that went in give it the rest, so that a
/// ticket never says less than the palette did. More than one is said in one
/// line, the way of `Shape:`.
fn delivery(block: &ReportBlock) -> Option<String> {
    let said = |phrase| pattern_report_phrase(phrase).to_owned();
    let arrived = match block.arrival {
        Arrival::Whole => None,
        Arrival::TakenBySystem => Some(said(ReportPhrase::TakenBySystem)),
        Arrival::HandedOver => Some(said(ReportPhrase::HandedOver)),
        Arrival::OnClipboard => Some(said(ReportPhrase::Clipboard)),
        Arrival::Interrupted {
            units_sent,
            units_expected,
        } => Some(cut_short(
            ReportPhrase::Interrupted,
            units_sent,
            units_expected,
        )),
        Arrival::InFlight {
            units_sent,
            units_expected,
        } => Some(cut_short(
            ReportPhrase::InFlight,
            units_sent,
            units_expected,
        )),
    };
    let altered = (!block.may_alter.is_empty()).then(|| {
        fill(
            pattern_report_phrase(ReportPhrase::MayBeAltered),
            &[("what", &substitution_names(block.may_alter))],
        )
    });
    let parts: Vec<String> = arrived.into_iter().chain(altered).collect();
    (!parts.is_empty()).then(|| parts.join(" · "))
}

fn cut_short(phrase: ReportPhrase, units_sent: usize, units_expected: usize) -> String {
    fill(
        pattern_report_phrase(phrase),
        &[
            ("sent", &units_sent.to_string()),
            ("expected", &units_expected.to_string()),
        ],
    )
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;
    use nkb_core::substitutions::Substitutions;
    use nkb_core::text::LiteralText;

    fn block() -> ReportBlock {
        ReportBlock {
            reference: "whitespace/zero-width-space-x3".to_owned(),
            pack_version: "1.0".to_owned(),
            folder: None,
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
            may_alter: Substitutions::NONE,
            target: Some(Target {
                program: Some("notepad.exe".to_owned()),
                field: ControlKind::TextField,
            }),
            made_for: None,
        }
    }

    #[test]
    fn a_value_made_for_a_limit_gives_the_command_that_makes_it_and_nothing_else_moves() {
        // `D178`: no pack and no version, so the first line says how to have
        // the same values again - every option written out.
        use nkb_core::boundary::{Limit, Unit};
        let built_in = report_text(&block());
        let limit = Limit::new(Unit::Bytes, Some(8), Some(50)).expect("in range");
        let text = report_text(&ReportBlock {
            reference: "boundary/two-byte-51".to_owned(),
            made_for: Some(limit),
            ..block()
        });
        assert_eq!(
            lines(&text)[0],
            "Value:    boundary/two-byte-51, from nkb boundary --min-length 8 --max-length 50 --unit bytes"
        );
        assert_eq!(lines(&text)[1..], lines(&built_in)[1..]);
    }

    fn lines(text: &str) -> Vec<&str> {
        text.split(LINE_END).collect()
    }

    #[test]
    fn a_pack_from_a_folder_is_named_by_kind_on_the_value_line_and_nothing_else_moves() {
        // `D173`: the folder by kind, never by path - and the block keeps its
        // shape, so only the first line differs.
        let built_in = report_text(&block());
        for (folder, said) in [
            (
                PackFolder::Team,
                "Value:    whitespace/zero-width-space-x3 @ pack 1.0, from a team pack folder",
            ),
            (
                PackFolder::Own,
                "Value:    whitespace/zero-width-space-x3 @ pack 1.0, from the tester's own pack folder",
            ),
        ] {
            let text = report_text(&ReportBlock {
                folder: Some(folder),
                ..block()
            });
            assert_eq!(lines(&text)[0], said);
            assert_eq!(lines(&text)[1..], lines(&built_in)[1..]);
        }
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
    fn a_padded_value_is_typed_as_both_of_its_parts() {
        // Seven letters in front of three families: the join of a family is
        // written as its escape, as it is in any value.
        let mut padded = block();
        padded.typed = Typed::Padded {
            padding: LiteralText::new("a").escape(),
            padding_count: 7,
            unit: LiteralText::new("\u{1F468}\u{200D}\u{1F469}").escape(),
            count: 3,
        };
        let text = report_text(&padded);
        assert!(
            text.contains("Typed:    7 × \"a\" + 3 × \"\u{1F468}\\u200D\u{1F469}\""),
            "{text}"
        );
    }

    #[test]
    fn a_padded_value_is_spelled_out_as_both_of_its_parts() {
        let mut padded = block();
        padded.spelled_out = Some(SpelledOut::Padded {
            padding: vec!['a'],
            padding_count: 2,
            unit: vec!['e', '\u{0301}'],
            count: 5,
        });
        let text = report_text(&padded);
        assert!(
            text.contains("Unicode:  2 × U+0061 + 5 × U+0065 U+0301"),
            "{text}"
        );
    }

    #[test]
    fn a_counterstring_is_typed_as_a_recipe_that_says_what_one_is() {
        // Never as its text: a hundred thousand characters of a ticket.
        let mut counted = block();
        counted.typed = Typed::Counterstring { length: 100_000 };
        let text = report_text(&counted);
        assert!(
            text.contains(
                "Typed:    counterstring of 100000 characters - every star stands at the position the number before it says"
            ),
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
    fn keys_still_on_their_way_say_the_count_is_a_floor_in_the_same_place() {
        // D158: the developer may find more of the value than the count.
        let mut cut = block();
        cut.arrival = Arrival::InFlight {
            units_sent: 0,
            units_expected: 7,
        };
        let text = report_text(&cut);
        let got = lines(&text);
        assert_eq!(
            got[5],
            "Delivery: interrupted after at least 0 of 7 UTF-16 units - more may have arrived after the stop"
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
    fn a_value_the_palette_warned_about_gets_the_same_warning_in_the_same_place() {
        // Typed whole, and yet not known to be in the field as typed: the palette
        // says so, and a ticket that said less would describe a value the field
        // may not hold. One line, the label and the place of D68.
        let delivery = |arrival, may_alter| {
            let text = report_text(&ReportBlock {
                arrival,
                may_alter,
                ..block()
            });
            let got: Vec<String> = lines(&text).into_iter().map(str::to_owned).collect();
            assert!(got[6].starts_with("Breaks:   "), "{text}");
            got[5].clone()
        };
        let capital = Substitutions {
            capitalization: true,
            ..Substitutions::NONE
        };
        assert_eq!(
            delivery(Arrival::TakenBySystem, Substitutions::NONE),
            "Delivery: every key pressed, but only the system confirmed them - characters may be missing if the application was busy"
        );
        assert_eq!(
            delivery(Arrival::HandedOver, Substitutions::NONE),
            "Delivery: handed to the desktop's input method, which gives it to whatever has the keyboard focus - nothing confirms that a field received it"
        );
        assert_eq!(
            delivery(Arrival::Whole, capital),
            "Delivery: the system's text substitutions were on and may have changed the value: the first letter of a sentence made a capital"
        );
        assert_eq!(
            delivery(Arrival::TakenBySystem, capital),
            "Delivery: every key pressed, but only the system confirmed them - characters may be missing if the application was busy · the system's text substitutions were on and may have changed the value: the first letter of a sentence made a capital"
        );
        // Followed into the application and untouched on the way: no line at all.
        assert!(!report_text(&block()).contains("Delivery:"));
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
        assert_eq!(
            target_line(&with(
                Some("firefox.exe"),
                ControlKind::ClosedToAccessibility
            )),
            "Target:   firefox.exe · window closed to accessibility"
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
