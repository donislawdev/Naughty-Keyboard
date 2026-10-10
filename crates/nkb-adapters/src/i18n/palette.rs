//! The palette's own words: what it writes beside its data, as opposed to the
//! sentences it says. A part of the `i18n` module, kept in a file of its own
//! so that a window's words are found in one place - see the module above for
//! why every one of them is a key.

use super::*;

// ---------------------------------------------------------------------------
// The palette's own labels
// ---------------------------------------------------------------------------

/// A word the palette writes BESIDE its data, as opposed to a sentence it says.
///
/// # Why these are keys at all
///
/// Untouchable rule 9 does not distinguish a sentence from a word: both are text
/// a person reads. `7 / 34` looks like pure data until one notices that the
/// slash, the spaces and the order are a decision - and that the view is exactly
/// where such a decision must not live, because `ui_guard.rs` refuses a quoted
/// string there. So the palette receives finished strings, and this is where
/// they are finished.
///
/// # Why the enum lives here and not in `app`
///
/// The other key sets match on types the application layer already owns, which
/// makes the compiler the completeness guard. These have no such producer: they
/// are presentation, and `app` neither knows nor should know that a counter is
/// drawn. `Startup` above is the same shape for the same reason - a key has to
/// be a variant rather than a bare function, so that `ux-spec.md` 6 and
/// `tools/sprawdz-kontrakt.py` see it like every other sentence.
///
/// ⚠️ The completeness that IS guarded here is the other direction: a variant
/// without a pattern does not compile, and a pattern whose placeholders do not
/// match its accessor is caught by the test at the bottom of this file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteLabel {
    /// The window's own title, which the system shows in the task switcher.
    Title,
    /// Where the sequence stands inside the pack.
    Counter,
    /// How much text the value that just went out actually was.
    Counts,
    /// The value is offensive - marked before a tester can wonder,
    /// `product-spec.md` 10.2.
    Offensive,
    /// The field was emptied before the value went in.
    Cleared,
    /// The value went to the clipboard for the tester to paste. Takes the place
    /// of `Cleared`, never stands beside it: on the clipboard route nothing
    /// presses a key, so nothing clears the field (`D71`).
    OnClipboard,
    /// Delivery stopped part-way and the field holds a fragment of the value
    /// shown. A risk, because what the tester sees above it is the whole value,
    /// not what the field holds (`OBS-126`).
    Interrupted,
    /// The pack loaded, and it loaded with warnings.
    Warnings,
    /// The standing bar of clipboard mode, `ux-spec.md` 2. Until `D71` it read
    /// `direct input refused`, because a bar saying `clipboard mode` while
    /// nothing put a value on the clipboard would have been worse than silence.
    ClipboardMode,
    /// The same bar, for one window: the one in front runs with higher
    /// privileges, and the bar goes when another window comes forward
    /// (`ux-spec.md` 8, `D72`). Its own words, so the tester can tell a mode
    /// they are in from a window they are looking at.
    ClipboardForWindow,
    /// The preview shows only part of the value, so the palette says how much.
    PreviewElided,
    /// Characters on screen that the shipped typeface does not draw by itself,
    /// so what a tester sees of them depends on the machine (`D52`).
    ///
    /// 🔴 "Not guaranteed", never "cannot be shown". The tool cannot find out
    /// what the machine's own fonts drew, and the note says the one thing it
    /// knows for certain: where its promise ends.
    NotGuaranteed,
    /// The same, when there are more of them than the note lists.
    NotGuaranteedMore,
    /// A generated value, written as what generates it: `255 × "a"`.
    ///
    /// It takes the preview's place rather than sitting beside it, because it is
    /// the preview - exact at every length, where a hundred characters of `a`
    /// would say nothing about whether there were 254 or 256 of them.
    Recipe,
    /// What the value band says before anything has gone out. The band is there
    /// from the start and never disappears (`D83`), so its empty state is a
    /// sentence that tells the tester the one thing to do next.
    NoValueYet,
    /// The palette's shortcuts are given back to the system while the
    /// shortcuts window is open (`ux-spec.md` 5.4), so a press reaches the
    /// application in front and no value goes out. Said in the message band
    /// for as long as it lasts - a palette that looks ready and answers
    /// nothing is the silence untouchable rule 1 forbids.
    ShortcutsPaused,
    /// The link under the hint bar that opens the shortcuts window - its words,
    /// and what a click does (`ux-spec.md` 5.4).
    ShortcutsLink,
    /// The band of a send in progress, its first word (`OBS-160`). Until that
    /// band a send of minutes left the palette showing the value before.
    Typing,
    /// How far the send in progress got, in the units the route counts - the
    /// same UTF-16 units the counts line of the value band names in full.
    TypingCounter,
    /// How to stop the send in progress (`D96`). In words, because `Escape` is
    /// not a shortcut a tester can look up in the hint bar: it is the palette's
    /// only key that works for the length of a send and no longer.
    StopTyping,
    /// The heading of the band that shows the value the next press sends,
    /// before it goes (`UX-GUI-001`) - which one, of how many.
    NextValue,
    /// The same heading when the next press only says the pack is finished.
    /// The numbers repeat the ones the end-of-pack message quotes.
    NextEndOfPack,
    /// The heading of the value band, which shows the value that went out last.
    /// Named, because the band above it shows another value - the next one.
    LastSent,
    /// The button that puts a value on the clipboard for the tester to paste
    /// by hand (`UX-GUI-003`, `D98`) - its words. The same word stands in the
    /// sentences that send the tester back to it.
    Copy,
    /// What the Copy button beside the next value does, for UI Automation: two
    /// buttons share one word, and this tells them apart.
    CopyNext,
    /// The same, for the Copy button beside the value that went out last.
    CopyLast,
    /// The button beside the next value that steps it back without typing
    /// (`D120`) - its word, after the drawn arrow.
    Back,
    /// What it does, for UI Automation.
    BackAction,
    /// The button that steps the next value forward without typing - its word,
    /// before the drawn arrow.
    Skip,
    /// What it does, for UI Automation.
    SkipAction,
    /// "Next pack" or "previous pack" with no other pack that opens: said
    /// rather than doing nothing (untouchable rule 1).
    NoOtherPack,
    /// The label of the switch under the hint bar that chooses how values
    /// travel (`UX-GUI-010`, `D99`, the owner's point 2 of 2026-10-07).
    SendBy,
    /// The switch's first way: values typed into the field.
    RouteKeyboard,
    /// Its second way: values put on the clipboard for the tester to paste.
    RouteClipboard,
    /// The button on the standing clipboard bar that turns the mode off - its
    /// words. Short, because the bar beside it already says which mode.
    TurnOff,
    /// What that button does, for UI Automation, where the bar's words do not
    /// stand beside it.
    TurnOffClipboard,
    /// The button at the end of the pack band while the palette is expanded -
    /// its words, which also say what a click does (`UX-GUI-004`).
    Collapse,
    /// The same button while the palette is compact. The one way back to the
    /// rest of the palette that the compact state shows (`UX-GUI-004`): until
    /// it, the compact palette said nothing about how to expand it again.
    Expand,
    /// The label of the switch that chooses how a typed value meets the field
    /// (`UX-GUI-005`, `D101`). Until the owner's point 2 of 2026-10-07 this
    /// was a sentence naming the way in effect beside a button naming the
    /// other, and which was which could not be told - the switch shows both.
    EachValue,
    /// The switch's first way: the line cleared, then the value typed.
    ClearLineFirst,
    /// Its second way: the value typed where the cursor stands.
    InsertAtCursor,
    /// What a click on the heading of the last value sent does, for UI
    /// Automation: it opens or folds the whole value (the owner's point 7).
    LastSentAction,
    /// Where the value that went out last was typed, under its heading (UX8,
    /// `UX-GUI-013`, `D104`): the program's file name and the kind of control,
    /// read at the press. Never for a value put on the clipboard.
    SentTo,
    /// The same, when the system would not give the program's name.
    SentToUnnamed,
    /// The kind of control in [`PaletteLabel::SentTo`] - `D73`, `D77`.
    ControlTextField,
    ControlTerminal,
    ControlUnconfirmed,
    ControlNotTextField,
}

pub(super) fn pattern_palette_label(label: PaletteLabel) -> &'static str {
    match label {
        PaletteLabel::Title => "Naughty Keyboard",
        PaletteLabel::Counter => "{done} / {total}",
        PaletteLabel::Counts => {
            "graphemes: {graphemes}, code points: {codepoints}, bytes: {bytes}, UTF-16 units: {utf16}"
        }
        PaletteLabel::Offensive => "offensive",
        PaletteLabel::Cleared => "cleared first",
        PaletteLabel::OnClipboard => "on the clipboard",
        PaletteLabel::Interrupted => "interrupted",
        PaletteLabel::Warnings => "pack warnings: {count}",
        PaletteLabel::ClipboardMode => "clipboard mode",
        PaletteLabel::ClipboardForWindow => "clipboard mode for this window",
        PaletteLabel::PreviewElided => "showing {shown} of {total} code points",
        PaletteLabel::NotGuaranteed => "not guaranteed by the bundled font: {list}",
        PaletteLabel::NotGuaranteedMore => {
            "not guaranteed by the bundled font: {list} and {rest} more"
        }
        PaletteLabel::Recipe => "{count} × \"{unit}\"",
        PaletteLabel::NoValueYet => {
            "Nothing sent yet. Put the cursor in any field and press {shortcut}."
        }
        PaletteLabel::ShortcutsPaused => "Shortcuts are paused while the Shortcuts window is open.",
        PaletteLabel::ShortcutsLink => "Change shortcuts",
        PaletteLabel::Typing => "Typing",
        PaletteLabel::TypingCounter => "{arrived} / {total} units",
        PaletteLabel::StopTyping => "Press Esc to stop.",
        PaletteLabel::NextValue => "Next: value {index} of {total}",
        PaletteLabel::NextEndOfPack => "Next: end of pack ({total}/{total})",
        PaletteLabel::LastSent => "Last sent",
        PaletteLabel::Copy => "Copy",
        PaletteLabel::CopyNext => "Copy the next value",
        PaletteLabel::CopyLast => "Copy the last sent value",
        PaletteLabel::Back => "Back",
        PaletteLabel::BackAction => "Back one value, without typing it",
        PaletteLabel::Skip => "Skip",
        PaletteLabel::SkipAction => "Skip this value, without typing it",
        PaletteLabel::NoOtherPack => "There is no other pack to switch to.",
        PaletteLabel::SendBy => "Send by",
        PaletteLabel::RouteKeyboard => "Keyboard",
        PaletteLabel::RouteClipboard => "Clipboard",
        PaletteLabel::TurnOff => "Turn off",
        PaletteLabel::TurnOffClipboard => "Turn off clipboard mode",
        PaletteLabel::Collapse => "Collapse",
        PaletteLabel::Expand => "Expand",
        PaletteLabel::EachValue => "Each value",
        PaletteLabel::ClearLineFirst => "Clear line first",
        PaletteLabel::InsertAtCursor => "Insert at cursor",
        PaletteLabel::LastSentAction => "Show or hide the whole value",
        PaletteLabel::SentTo => "to {program}, {control}",
        PaletteLabel::SentToUnnamed => "to a program that did not give its name, {control}",
        PaletteLabel::ControlTextField => "a text field",
        PaletteLabel::ControlTerminal => "a terminal",
        PaletteLabel::ControlUnconfirmed => "a control not confirmed as a field",
        PaletteLabel::ControlNotTextField => "not a text field",
    }
}

/// How much of a program's name the palette repeats. Longer than any program
/// name met so far, so a real one is shown whole - and short enough that a
/// name made up to push the palette off the screen cannot.
pub(super) const PROGRAM_SHOWN: usize = 64;

/// Where the value that went out last was typed: `to chrome.exe, a text field`
/// (UX8, `D104`).
///
/// The program's name comes from the system. Escaped and cut like a part of a
/// shortcut the tester wrote (`shown_part`), because a name is text the tool
/// did not write: a control character in it must not start a second line in
/// the value band.
#[must_use]
pub fn sent_to(target: &Target) -> String {
    let control = pattern_palette_label(match target.field {
        ControlKind::TextField => PaletteLabel::ControlTextField,
        ControlKind::Terminal => PaletteLabel::ControlTerminal,
        ControlKind::Unconfirmed => PaletteLabel::ControlUnconfirmed,
        ControlKind::NotTextField => PaletteLabel::ControlNotTextField,
    });
    match &target.program {
        Some(program) => {
            let mut shown: String = program
                .chars()
                .take(PROGRAM_SHOWN)
                .collect::<String>()
                .escape_debug()
                .to_string();
            if program.chars().count() > PROGRAM_SHOWN {
                shown.push_str("...");
            }
            fill(
                pattern_palette_label(PaletteLabel::SentTo),
                &[("program", &shown), ("control", control)],
            )
        }
        None => fill(
            pattern_palette_label(PaletteLabel::SentToUnnamed),
            &[("control", control)],
        ),
    }
}

/// The heading over the value the next press sends: `Next: value 4 of 12`.
#[must_use]
pub fn next_value(index: usize, total: usize) -> String {
    fill(
        pattern_palette_label(PaletteLabel::NextValue),
        &[("index", &index.to_string()), ("total", &total.to_string())],
    )
}

/// The same heading when the next press only says the pack is finished.
#[must_use]
pub fn next_end_of_pack(total: usize) -> String {
    fill(
        pattern_palette_label(PaletteLabel::NextEndOfPack),
        &[("total", &total.to_string())],
    )
}

/// A label that carries no number, ready to show.
///
/// ⚠️ Takes the enum rather than being seven functions, and the price is that a
/// caller can ask for a label that HAS placeholders. The test at the bottom of
/// this file refuses exactly that: a pattern with a brace must have a typed
/// accessor below, and one without must not.
#[must_use]
pub fn label(label: PaletteLabel) -> &'static str {
    pattern_palette_label(label)
}

/// How far a send in progress got: `1784 / 65535 units` (`OBS-160`).
#[must_use]
pub fn typing_counter(arrived: usize, total: usize) -> String {
    fill(
        pattern_palette_label(PaletteLabel::TypingCounter),
        &[
            ("arrived", &arrived.to_string()),
            ("total", &total.to_string()),
        ],
    )
}

/// Where the sequence stands: `7 / 34`.
#[must_use]
pub fn counter(done: usize, total: usize) -> String {
    fill(
        pattern_palette_label(PaletteLabel::Counter),
        &[("done", &done.to_string()), ("total", &total.to_string())],
    )
}

/// How much text went out, in the four units that differ from each other.
///
/// The four are not decoration. A tester reporting a bug needs the count the
/// receiving system will argue about, and which one that is depends on the
/// system - so the palette shows all of them rather than picking for them.
///
/// Graphemes come first because they are the only one a person can check by
/// looking: a family emoji is one of them, five code points and eighteen bytes,
/// and a field that accepted "one character" and stored five is the bug this
/// tool exists to find. `ux-spec.md` 2 puts them first for the same reason.
///
/// Every count stands AFTER its label, never before an inflected noun (`D80`).
/// "1 graphemes" stood here until 2026-09-24, and the English rule would not
/// have fixed it for long: this sentence is translated, and Polish has three
/// plural forms. `warnings` and the shape line (`D64`) chose the same form for
/// the same reason, so the palette needs no plural mechanism at all.
#[must_use]
pub fn counts(graphemes: usize, code_points: usize, bytes: usize, utf16_units: usize) -> String {
    fill(
        pattern_palette_label(PaletteLabel::Counts),
        &[
            ("graphemes", &graphemes.to_string()),
            ("codepoints", &code_points.to_string()),
            ("bytes", &bytes.to_string()),
            ("utf16", &utf16_units.to_string()),
        ],
    )
}

/// The value band before the first value: names the shortcut that sends one.
///
/// Takes the chord rather than looking it up, because the palette knows which
/// chord it registered and a default read here could be the wrong one.
#[must_use]
pub fn no_value_yet(next_value: HotkeyChord) -> String {
    fill(
        pattern_palette_label(PaletteLabel::NoValueYet),
        &[("shortcut", &chord(next_value))],
    )
}

/// How many warnings the pack carried when it loaded.
///
/// Written as `pack warnings: 1` rather than `1 pack warnings` on purpose: the
/// second needs a plural rule, and a plural rule is a mechanism this module does
/// not have and does not need for one label.
#[must_use]
pub fn warnings(count: usize) -> String {
    fill(
        pattern_palette_label(PaletteLabel::Warnings),
        &[("count", &count.to_string())],
    )
}
