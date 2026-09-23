//! Every sentence the product says to a person, behind a key.
//!
//! Untouchable rule 9: user-visible text is never a literal at the place it is
//! shown - it is a translation key, and the keys are English too. This module is
//! the other end of that rule, and `architektura.md` 2.3 already names it: `i18n`
//! is an ADAPTER, present in both executables, sharing one MECHANISM while the
//! sets of keys differ.
//!
//! # The key is a Rust type path, not a second naming scheme
//!
//! `OBS-14` said `ux-spec.md` 6 carried no key column, so the keys would be
//! invented by whichever session implemented them first - differently each time.
//! The answer chosen is the cheapest one that cannot drift: THE KEY IS THE TYPE
//! PATH. `Message::EndOfPack` in the document is `Message::EndOfPack` here.
//! There is no table mapping document names to code names, because there is one
//! name.
//!
//! # Why a function per type instead of one enum of keys
//!
//! The tempting shape is a single `TextKey` enum covering everything. It was
//! rejected: it would DUPLICATE the variants of `Message`, `ChooseError` and the
//! rest, and the conversion into it can always be written with a `_ =>` arm - so
//! a new variant would compile, and would quietly render as somebody else's
//! sentence.
//!
//! Matching on the app's own types instead makes the COMPILER the completeness
//! guard: a new `Message` variant stops this file from building until a sentence
//! is written for it. That is stronger than any test, and it costs nothing.
//!
//! # Pattern and substitution are separate, and that is load-bearing
//!
//! Each `pattern_*` function is a pure table - key on the left, pattern on the
//! right, one per line - and the `*` functions substitute. The split keeps two
//! responsibilities apart, and it buys something specific: the table is exactly
//! what `tools/sprawdz-kontrakt.py` reads, comparing it to `ux-spec.md` 6 BYTE
//! FOR BYTE. Were the sentences assembled with `format!`, the bridge would have
//! to run the code or guess the arguments, and a bridge that guesses is a bridge
//! that goes green while the document drifts.
//!
//! So the patterns here carry `{name}` placeholders in the same places the
//! document does. That is why `fill` exists instead of `format!`: `format!`
//! needs its literal inline, which would put the sentence back at the call site.
//!
//! # One language, and no scaffolding pretending otherwise
//!
//! Document `08` section 2 asks that a new language be a new FILE with no code
//! change, and that languages come from scanning a directory rather than a list
//! written into the code. Today there is one language and zero translation
//! files, so there is nothing to scan - and `architektura.md` 8 refuses an
//! extension point that has no second case. There is therefore no `Language`
//! enum here: it would be precisely the list-in-code that document warns about.
//!
//! What this module does instead is keep the seam in ONE place. Every sentence
//! is reached through a `pattern_*` function, so the day a second language
//! arrives, those functions gain a source - and nothing at any call site moves.
//! Recorded with its condition for return in `docs/00-ODSTEPSTWA.md`.
//!
//! # What this module does NOT cover, said plainly
//!
//! The CLI still prints literals. The gap between untouchable rules 8 and 9 was
//! settled in `00-ODSTEPSTWA.md` - CLI messages go through keys too, with
//! exactly one value per key - and that move is a piece of work of its own, not
//! part of the palette. `OBS-14` carries it.

use nkb_app::advance_sequence::{ChooseError, Message};
use nkb_app::drive_sequence::Ended;
use nkb_app::ports::{ShortcutRegistration, ShortcutsUnavailable};
use nkb_core::hotkeys::{HotkeyAction, HotkeyChord, HotkeyKey, default_chord};
use nkb_core::preview::ShapeFact;

/// Substitutes `{name}` placeholders in one pass over the pattern.
///
/// One pass, not a chain of `replace` calls, and the difference is not style: a
/// chain re-scans what it has just substituted, so a value that happens to
/// contain braces would be substituted again. Here a substituted value is
/// copied into the output and never looked at.
///
/// An unknown placeholder is left standing rather than swallowed. That is
/// deliberate: a `{sent}` visible in the palette is ugly and gets fixed, while a
/// silently emptied sentence reads as finished. The test below refuses any
/// pattern that still holds a brace after its own arguments are applied, so the
/// ugly version never reaches a tester.
pub(crate) fn fill(pattern: &str, values: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(pattern.len());
    let mut rest = pattern;

    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else {
            // No closing brace at all: the rest is literal text.
            out.push_str(&rest[open..]);
            return out;
        };
        let name = &after[..close];
        match values.iter().find(|(key, _)| *key == name) {
            Some((_, value)) => out.push_str(value),
            None => {
                out.push('{');
                out.push_str(name);
                out.push('}');
            }
        }
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    out
}

// ---------------------------------------------------------------------------
// The pieces a sentence is built from
// ---------------------------------------------------------------------------

/// What one of the ten global actions is called, in the tester's words.
///
/// A component of a sentence rather than a sentence, so it returns a borrowed
/// string: it is substituted into `{action}`, and allocating ten times per press
/// to hand a name to `fill` would be waste for nothing.
///
/// The names come from the shortcut table in `ux-spec.md` 3. One of them says
/// something the table does not: `ToggleVisibility` is "show or DIM", never
/// "show or hide", because the palette never hides - `hide()` destroys the
/// window and loses `WS_EX_NOACTIVATE` (`OBS-80`). A name promising a behaviour
/// the product will not have is a defect in the name.
#[must_use]
pub fn action_name(action: HotkeyAction) -> &'static str {
    match action {
        HotkeyAction::NextValue => "Next value",
        HotkeyAction::PreviousValue => "Previous value",
        HotkeyAction::RepeatLast => "Repeat last value",
        HotkeyAction::RestartPack => "Restart pack",
        HotkeyAction::CopyReport => "Copy report block",
        HotkeyAction::MarkOk => "Mark as working",
        HotkeyAction::MarkProblem => "Mark as a problem",
        HotkeyAction::MarkSuspect => "Mark as suspect",
        HotkeyAction::OpenPacks => "Open pack search",
        HotkeyAction::ToggleVisibility => "Show or dim the palette",
    }
}

/// What one key of a shortcut is called.
///
/// `Space` is a word rather than a character on purpose - a blank between two
/// plus signs is not readable as a key.
fn key_name(key: HotkeyKey) -> &'static str {
    match key {
        HotkeyKey::N => "N",
        HotkeyKey::P => "P",
        HotkeyKey::R => "R",
        HotkeyKey::B => "B",
        HotkeyKey::H => "H",
        HotkeyKey::Space => "Space",
        HotkeyKey::Digit0 => "0",
        HotkeyKey::Digit1 => "1",
        HotkeyKey::Digit2 => "2",
        HotkeyKey::Digit3 => "3",
    }
}

/// A shortcut written the way a tester reads it: `Ctrl+Alt+N`.
///
/// ⚠️ This is the WINDOWS AND LINUX convention, and on macOS it is wrong in a
/// way worth naming rather than discovering. `ux-spec.md` 3 writes the macOS
/// defaults as `⌥⌘N`; the adapter that registers there swaps the primary
/// modifier for the Command bit, so this function would print `Alt+Win+N` -
/// three things a Mac user does not call those keys. Delivery on macOS is
/// blocked anyway (`OBS-70`), so the palette does not run there yet and this has
/// no victim today. `OBS-115`.
#[must_use]
pub fn chord(chord: HotkeyChord) -> String {
    let mut out = String::new();
    if chord.ctrl {
        out.push_str("Ctrl+");
    }
    if chord.alt {
        out.push_str("Alt+");
    }
    if chord.shift {
        out.push_str("Shift+");
    }
    if chord.win {
        out.push_str("Win+");
    }
    out.push_str(key_name(chord.key));
    out
}

/// The shortcut bound to an action, as text, or an empty string if it has none.
///
/// Reads the DEFAULTS, because configurable shortcuts need `SettingsStore` and
/// that does not exist. When it does, this is the one function that gains a
/// source - not every sentence that mentions a shortcut.
fn chord_text(action: HotkeyAction) -> String {
    default_chord(action).map(chord).unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Message - what the palette says after one action
// ---------------------------------------------------------------------------

/// The pattern for one message key. `ux-spec.md` 6, section A, byte for byte.
///
/// 🔴 The two sentences that enter clipboard mode say the tester's clipboard is
/// replaced, and they are the only place that says it: `ux-spec.md` 12 item 2
/// warns once and never restores. They arrived with the clipboard route itself
/// (`D71`) and not a day before - a sentence telling a tester to press paste,
/// written while nothing put a value on the clipboard, would have pasted
/// whatever was there before.
fn pattern_message(message: &Message) -> &'static str {
    match message {
        Message::NoDirectRoute { .. } => {
            "This build cannot type into other applications on {system}, so values now go to your clipboard, replacing what you had copied. Press your paste shortcut to insert each one."
        }
        Message::ClipboardMode => {
            "Clipboard mode: each value goes to your clipboard, replacing what you had copied. Press your paste shortcut to insert each one."
        }
        Message::HigherPrivileges => {
            "This window runs with higher privileges than Naughty Keyboard, so the system blocks typing into it - values for it now go to your clipboard, replacing what you had copied. Press your paste shortcut to insert each one, or start the tool with the same privileges."
        }
        Message::ClipboardBusy => {
            "Another application is holding the clipboard, so the value was not placed on it. Press the shortcut again in a moment."
        }
        Message::ClipboardFailed { .. } => {
            "The value was not placed on the clipboard: {detail}. The counter did not move, so the same shortcut tries this value again."
        }
        Message::NotForClipboard { .. } => {
            "Value \"{id}\" contains {character}, where clipboard text would be cut short, so it was not placed on the clipboard. Only direct input can deliver it whole."
        }
        Message::NoTarget => "The target window is gone. Click into a field and try again.",
        Message::Interrupted { .. } => {
            "Stopped after {sent} of {expected} characters. The field holds a partial value - clear it before the next test."
        }
        Message::EndOfPack { .. } => "End of pack ({total}/{total}). Press again to start over.",
        Message::CounterKept { .. } => {
            "New field - the counter is still at {done}/{total}. Press {shortcut} to start this pack from the beginning."
        }
        Message::NoPack => {
            "No pack is chosen, so there is nothing to send. Restart the palette with a pack name."
        }
        Message::ModifierHeld { .. } => {
            "{key} is still held down, so nothing was sent. Release it and press the shortcut again."
        }
        Message::ClearingFailed => {
            "The field could not be cleared, so nothing was sent. It may hold part of its old content - check it before trying again."
        }
        Message::ValueTooLarge { .. } => {
            "Value \"{id}\" describes more text than this build can hold, so it was not sent. Run: nkb lint {pack}"
        }
        Message::Unhandled { .. } => {
            "\"{action}\" is not wired in this build yet, so nothing happened. The shortcut stays reserved so another application cannot take it."
        }
        Message::PressedWhileBusy { .. } => {
            "\"{action}\" arrived while the previous value was still going out, so it was dropped rather than queued. Press it again."
        }
        Message::ReportCopied { .. } => {
            "Report block for {reference} copied. Paste it into the ticket."
        }
        Message::NothingToReport => {
            "Nothing has been sent yet, so there is no report block to copy. Send a value first."
        }
        Message::ReportBusy => {
            "Another application is holding the clipboard, so the report block was not copied. Press {shortcut} again in a moment."
        }
        Message::ReportFailed { .. } => {
            "The report block was not copied: {detail}. The same facts are printed by: nkb show {pack}"
        }
    }
}

/// What the palette says about one message, ready to show.
///
/// `pack` names the pack in play, for the two sentences that mention one. A
/// caller with no pack chosen passes an empty string, which only reaches a
/// sentence that cannot occur without a pack.
#[must_use]
pub fn message(message: &Message, pack: &str) -> String {
    let pattern = pattern_message(message);
    match message {
        Message::ClipboardMode
        | Message::HigherPrivileges
        | Message::ClipboardBusy
        | Message::NoTarget
        | Message::NoPack
        | Message::ClearingFailed
        | Message::NothingToReport => pattern.to_owned(),
        Message::NoDirectRoute { system } => fill(pattern, &[("system", system)]),
        Message::ClipboardFailed { detail } => fill(pattern, &[("detail", detail)]),
        Message::NotForClipboard { id, character } => fill(
            pattern,
            &[
                ("id", id),
                // As a code point, never the character itself: the one this
                // names today is U+0000, and a NUL in the palette's own
                // sentence would cut the sentence short the same way.
                ("character", &format!("U+{:04X}", u32::from(*character))),
            ],
        ),
        Message::Interrupted {
            units_sent,
            units_expected,
        } => fill(
            pattern,
            &[
                ("sent", &units_sent.to_string()),
                ("expected", &units_expected.to_string()),
            ],
        ),
        Message::EndOfPack { total } => fill(pattern, &[("total", &total.to_string())]),
        Message::CounterKept { done, total } => fill(
            pattern,
            &[
                ("done", &done.to_string()),
                ("total", &total.to_string()),
                ("shortcut", &chord_text(HotkeyAction::RestartPack)),
            ],
        ),
        Message::ModifierHeld { key } => fill(pattern, &[("key", key)]),
        Message::ValueTooLarge { id } => fill(pattern, &[("id", id), ("pack", pack)]),
        Message::Unhandled { action } | Message::PressedWhileBusy { action } => {
            fill(pattern, &[("action", action_name(*action))])
        }
        Message::ReportCopied { reference } => fill(pattern, &[("reference", reference)]),
        Message::ReportBusy => fill(
            pattern,
            &[("shortcut", &chord_text(HotkeyAction::CopyReport))],
        ),
        Message::ReportFailed { detail } => fill(pattern, &[("detail", detail), ("pack", pack)]),
    }
}

// ---------------------------------------------------------------------------
// ChooseError - why a pack could not be chosen
// ---------------------------------------------------------------------------

/// The pattern for one pack-choosing failure. `ux-spec.md` 6, section D.
///
/// ⚠️ `Refused` has TWO patterns, because English inflects the noun: one problem,
/// three problems. It is the only sentence in today's set that needs it, and it
/// is handled head-on rather than with a parenthesised "(s)", which reads as an
/// apology for not having handled it. Languages with more than two plural forms
/// - Polish among them - need rules, and those arrive with the second language.
fn pattern_choose_error(error: &ChooseError) -> &'static str {
    match error {
        ChooseError::NotFound => "Pack \"{pack}\" was not found. Run: nkb packs",
        ChooseError::Unreadable => {
            "Pack \"{pack}\" could not be read. Check that the file is there and is valid UTF-8."
        }
        ChooseError::Refused { errors } if *errors == 1 => {
            "Pack \"{pack}\" has 1 problem. Run: nkb lint {pack}"
        }
        ChooseError::Refused { .. } => {
            "Pack \"{pack}\" has {errors} problems. Run: nkb lint {pack}"
        }
    }
}

/// What the palette says when a pack could not be chosen.
#[must_use]
pub fn choose_error(error: &ChooseError, pack: &str) -> String {
    let pattern = pattern_choose_error(error);
    match error {
        ChooseError::NotFound | ChooseError::Unreadable => fill(pattern, &[("pack", pack)]),
        ChooseError::Refused { errors } => {
            fill(pattern, &[("pack", pack), ("errors", &errors.to_string())])
        }
    }
}

// ---------------------------------------------------------------------------
// Shortcuts - registration, absence, and the listener going away
// ---------------------------------------------------------------------------

/// The pattern for a registration outcome that has something to say.
///
/// ⚠️ `Registered` is absent and that is not an oversight: a shortcut that took
/// is the normal case, and the tool does not report what worked. The return type
/// says so, rather than a caller having to know.
///
/// 🔴 The `Taken` sentence changed on 2026-09-22 for the same reason as
/// `Degraded`. It used to end "Pick a different shortcut in Settings." - and
/// there is no settings screen, nor are shortcuts configurable: `nkb_core::
/// hotkeys` says so at `HotkeyKey`. The sentence sent a tester to a place they
/// could not find. It returns in that form with `SettingsStore`.
fn pattern_registration(outcome: &ShortcutRegistration) -> Option<&'static str> {
    match outcome {
        ShortcutRegistration::Registered => None,
        ShortcutRegistration::Taken => Some(
            "{shortcut} is already taken by another application, so \"{action}\" will not respond. This build cannot change it - close the other application to free the combination.",
        ),
        ShortcutRegistration::Failed { .. } => Some(
            "{shortcut} could not be registered, so \"{action}\" will not respond. The system returned code {code}.",
        ),
    }
}

/// What the palette says about one shortcut registration, or nothing when it
/// simply worked.
#[must_use]
pub fn registration(outcome: &ShortcutRegistration, action: HotkeyAction) -> Option<String> {
    let pattern = pattern_registration(outcome)?;
    let shortcut = chord_text(action);
    // ⚠️ This match supplies ARGUMENTS and decides nothing about whether there is
    // a sentence at all - that decision belongs to `pattern_registration` and to
    // nowhere else. The first version repeated it here with an early `return
    // None` for `Registered`, and a mutation proved the cost immediately:
    // changing `pattern_registration` to speak for a successful registration did
    // not change what this function returned, so the guard could not see it. Two
    // places holding one decision is the shape that drifts.
    let code = match outcome {
        ShortcutRegistration::Registered | ShortcutRegistration::Taken => String::new(),
        ShortcutRegistration::Failed { code } => code.to_string(),
    };
    Some(fill(
        pattern,
        &[
            ("shortcut", &shortcut),
            ("action", action_name(action)),
            ("code", &code),
        ],
    ))
}

/// The pattern for having no shortcuts at all. `ux-spec.md` 6, section E.
fn pattern_unavailable(reason: &ShortcutsUnavailable) -> &'static str {
    match reason {
        ShortcutsUnavailable::Unsupported { .. } => {
            "This build has no way to register global shortcuts on {system}, so the palette cannot be driven from the keyboard here."
        }
        ShortcutsUnavailable::CouldNotStart => {
            "The shortcut listener could not start, so no shortcut will respond. Restart the palette to try again."
        }
    }
}

/// What the palette says when no shortcut could be registered at all.
#[must_use]
pub fn shortcuts_unavailable(reason: &ShortcutsUnavailable) -> String {
    let pattern = pattern_unavailable(reason);
    match reason {
        ShortcutsUnavailable::Unsupported { system } => fill(pattern, &[("system", system)]),
        ShortcutsUnavailable::CouldNotStart => pattern.to_owned(),
    }
}

/// The pattern for the loop ending, when the ending is worth saying.
///
/// `Stopped` is absent for the same reason `Registered` is: closing on request
/// is not an event anybody needs told. `ShortcutsGone` is the opposite - the
/// window would sit there looking alive while no press can ever arrive again.
fn pattern_ended(ended: Ended) -> Option<&'static str> {
    match ended {
        Ended::Stopped => None,
        Ended::ShortcutsGone => Some(
            "The shortcut listener has stopped, so no shortcut will respond. Restart the palette.",
        ),
    }
}

/// What the palette says when the loop ends, or nothing for an ordinary close.
#[must_use]
pub fn ended(ended: Ended) -> Option<String> {
    pattern_ended(ended).map(ToOwned::to_owned)
}

// ---------------------------------------------------------------------------
// Startup - the one thing said before any screen exists
// ---------------------------------------------------------------------------

/// A failure that happens before the palette exists.
///
/// The only key in this module declared HERE rather than matched from `app` or
/// `core`, and the reason is narrow: there is no type to match on. The failure
/// comes out of the graphics library as its own error, and the KINDS of startup
/// failure are not something this product models - it models packs, values and
/// sequences. A key still has to be a variant rather than a bare function, so
/// that `ux-spec.md` 6 and `tools/sprawdz-kontrakt.py` see it the same shape as
/// every other sentence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Startup {
    /// The window could not be created at all.
    WindowFailed,
}

/// The pattern for a startup failure. `ux-spec.md` 6, section B.
///
/// ⚠️ The sentence names an environment variable, which document `08` section 3
/// would normally call an implementation detail. It stays because the same
/// document's section 4 is stronger here: an option that will do nothing in
/// somebody's setup MUST say so, and this is the only thing a tester can
/// actually do today. Slint has no runtime fallback from its hardware renderer
/// to the software one (`OBS-103`), so without the variable the answer would be
/// "it does not work", full stop.
fn pattern_startup(failure: Startup) -> &'static str {
    match failure {
        Startup::WindowFailed => {
            "Naughty Keyboard could not open its window: {reason}. If this machine has no graphics acceleration, set SLINT_BACKEND=winit-software and start it again."
        }
    }
}

/// What the tool says when it could not start.
///
/// `reason` is the graphics library's own error text: English, untranslatable,
/// and in the sentence because without it a tester has nothing to put in a bug
/// report.
#[must_use]
pub fn startup_failure(failure: Startup, reason: &str) -> String {
    fill(pattern_startup(failure), &[("reason", reason)])
}

// ---------------------------------------------------------------------------
// The value's shape, and the preview around it
// ---------------------------------------------------------------------------

/// The pattern for one shape fact. `ux-spec.md` 6, section G.
///
/// # Why the counted facts read `name × N` rather than `N names`
///
/// 🔴 Because there is no plural mechanism here, and two patterns would not be
/// enough to build one. The sketch in `ux-spec.md` 2 draws `3 zero-width spaces`,
/// which needs English plural agreement; Polish needs THREE forms for the same
/// sentence, and the next language may need more. A form that carries the number
/// beside an uninflected name is the only one that survives translation without
/// a mechanism we do not have. The sketch says of itself that it is "content, not
/// appearance", so this is a rendering of it rather than a departure from it.
///
/// `×` is `U+00D7`, already in the document's own sketch (`100 000 × "a"`), and
/// present in the shipped typeface.
fn pattern_shape(fact: ShapeFact) -> &'static str {
    match fact {
        // Reordering first in the list the core builds, and named plainly here:
        // a tester who misses this reads the field backwards rather than seeing
        // a defect.
        ShapeFact::BidiControl(_) => "reordering control × {count}",
        ShapeFact::ZeroWidth(_) => "zero-width × {count}",
        ShapeFact::UnusualSpace(_) => "unusual space × {count}",
        ShapeFact::SoftHyphen(_) => "soft hyphen × {count}",
        ShapeFact::LineBreak(_) => "line break × {count}",
        ShapeFact::Tab(_) => "tab × {count}",
        ShapeFact::OtherControl(_) => "control character × {count}",
        // No count: these are about WHERE, and there is only one of each end.
        ShapeFact::LeadingSpace => "leading space",
        ShapeFact::TrailingSpace => "trailing space",
    }
}

/// One shape fact as a person reads it.
#[must_use]
pub fn shape_fact(fact: ShapeFact) -> String {
    let count = match fact {
        ShapeFact::BidiControl(n)
        | ShapeFact::ZeroWidth(n)
        | ShapeFact::UnusualSpace(n)
        | ShapeFact::SoftHyphen(n)
        | ShapeFact::LineBreak(n)
        | ShapeFact::Tab(n)
        | ShapeFact::OtherControl(n) => n,
        ShapeFact::LeadingSpace | ShapeFact::TrailingSpace => 0,
    };
    fill(pattern_shape(fact), &[("count", &count.to_string())])
}

/// Every fact about the value, on one line, in the order the core put them.
///
/// The separator is `·` (`U+00B7`), from the sketch in `ux-spec.md` 2. Joining
/// happens here rather than in the core because a separator is text, and
/// untouchable rule 9 keeps text out of the layer below.
#[must_use]
pub fn shape_line(facts: &[ShapeFact]) -> String {
    facts
        .iter()
        .map(|fact| shape_fact(*fact))
        .collect::<Vec<_>>()
        .join(" · ")
}

/// What the palette says when the preview is only part of the value.
///
/// 🔴 Said out loud, never implied. A preview showing a hundred characters of a
/// million without a word would be the tool answering "what did I send" with a
/// fragment presented as the whole - which is the silence untouchable rule 1
/// forbids.
#[must_use]
pub fn preview_elided(shown: usize, total: usize) -> String {
    fill(
        pattern_palette_label(PaletteLabel::PreviewElided),
        &[("shown", &shown.to_string()), ("total", &total.to_string())],
    )
}

/// A generated value as the preview shows it: `255 × "a"`.
///
/// The count is written in plain digits. `ux-spec.md` 2 sketches `100 000`,
/// and grouping digits is a locale's decision this module has no mechanism
/// for, the same gap as the plural in `OBS-120`. Plain digits also match the
/// counters on the line below, which print them that way too.
///
/// `unit` arrives with its invisible characters already replaced by the marker.
/// A unit holding a quotation mark reads as `3 × """`: ambiguous to the eye and
/// still true, and no catalogue value has one.
#[must_use]
pub fn recipe(count: u32, unit: &str) -> String {
    fill(
        pattern_palette_label(PaletteLabel::Recipe),
        &[("count", &count.to_string()), ("unit", unit)],
    )
}

/// How many characters the typeface note names before it counts the rest.
///
/// Eight, because the longest list any shipped value produces is eight - the
/// full-width Latin one - so every shipped value is named whole. Past that the
/// note grows by a line for roughly every eight more and pushes the counters,
/// which a tester came to read, down the palette. Measured 2026-09-23 in the
/// palette render: six code points take two lines, the first holding five
/// after the words.
const NOT_GUARANTEED_LISTED: usize = 8;

/// The note under the preview naming what the shipped typeface does not draw.
///
/// `None` for an empty list, so "nothing to say" has one representation and a
/// caller cannot show a note that ends in a colon.
///
/// The characters are written as `U+30B9`, never as themselves, and that is the
/// whole point of the note: a character outside the guarantee may draw as the
/// empty rectangle the note exists to explain. The notation is the Unicode
/// standard's own and reads the same in every language, which is why it is
/// written here rather than behind a key of its own.
#[must_use]
pub fn not_guaranteed(outside: &[char]) -> Option<String> {
    if outside.is_empty() {
        return None;
    }
    let list = outside
        .iter()
        .take(NOT_GUARANTEED_LISTED)
        .map(|c| format!("U+{:04X}", u32::from(*c)))
        .collect::<Vec<_>>()
        .join(" ");
    let rest = outside.len().saturating_sub(NOT_GUARANTEED_LISTED);
    Some(if rest == 0 {
        fill(
            pattern_palette_label(PaletteLabel::NotGuaranteed),
            &[("list", &list)],
        )
    } else {
        fill(
            pattern_palette_label(PaletteLabel::NotGuaranteedMore),
            &[("list", &list), ("rest", &rest.to_string())],
        )
    })
}

// ---------------------------------------------------------------------------
// What the environment would not let the tool do
// ---------------------------------------------------------------------------

/// Something about the machine or the session that stops the tool doing its job.
///
/// `ux-spec.md` 6 section B announces four of these. One stands today, and the
/// other three keep their announcement mark there rather than being invented
/// here: a key with a sentence nobody can reach is a promise, and the bridge in
/// `tools/sprawdz-kontrakt.py` would not tell the difference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Environment {
    /// The palette could not be made to refuse the keyboard focus.
    FocusNotRefused,
}

/// The pattern for an environment limit. `ux-spec.md` 6, section B.
///
/// The sentence has two halves and both are load-bearing. The first says what
/// the tool could not do, because untouchable rule 1 forbids a run that did less
/// than it promised from looking whole. The second says what the tester can do
/// INSTEAD, and it is a real remedy rather than a consolation: clicking into the
/// field under test moves the focus there, and measured 2026-09-22 the palette
/// does not take it back on its own.
fn pattern_environment(limit: Environment) -> &'static str {
    match limit {
        Environment::FocusNotRefused => {
            "The palette could not refuse the keyboard focus: {reason}. Click into the field you are testing before pressing a shortcut."
        }
    }
}

/// What the tool says when the environment limited it.
///
/// `reason` is the system layer's own wording, for the same reason
/// `startup_failure` passes the graphics library's through: it is the one
/// specific thing in the sentence, and shortening it here would leave a tester
/// with nothing to put in a bug report.
#[must_use]
pub fn environment(limit: Environment, reason: &str) -> String {
    fill(pattern_environment(limit), &[("reason", reason)])
}

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
}

fn pattern_palette_label(label: PaletteLabel) -> &'static str {
    match label {
        PaletteLabel::Title => "Naughty Keyboard",
        PaletteLabel::Counter => "{done} / {total}",
        PaletteLabel::Counts => {
            "{graphemes} graphemes, {codepoints} code points, {bytes} bytes, {utf16} UTF-16 units"
        }
        PaletteLabel::Offensive => "offensive",
        PaletteLabel::Cleared => "cleared first",
        PaletteLabel::OnClipboard => "on the clipboard",
        PaletteLabel::Warnings => "pack warnings: {count}",
        PaletteLabel::ClipboardMode => "clipboard mode",
        PaletteLabel::ClipboardForWindow => "clipboard mode for this window",
        PaletteLabel::PreviewElided => "showing {shown} of {total} code points",
        PaletteLabel::NotGuaranteed => "not guaranteed by the bundled font: {list}",
        PaletteLabel::NotGuaranteedMore => {
            "not guaranteed by the bundled font: {list} and {rest} more"
        }
        PaletteLabel::Recipe => "{count} × \"{unit}\"",
    }
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

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;

    /// One of every message, with arguments chosen so the test can check the
    /// substitution as well as the sentence.
    ///
    /// The COMPLETENESS of this list is not what makes the module complete - the
    /// compiler does that, by refusing a `pattern_message` that misses a
    /// variant. This list exists so the CONTENT of each sentence is checked, and
    /// it is kept honest by `every_message_variant_is_listed_here` below.
    fn every_message() -> Vec<Message> {
        vec![
            Message::NoDirectRoute {
                system: "macOS".to_owned(),
            },
            Message::ClipboardMode,
            Message::HigherPrivileges,
            Message::ClipboardBusy,
            Message::ClipboardFailed {
                detail: "the display went away".to_owned(),
            },
            Message::NotForClipboard {
                id: "nul-in-text".to_owned(),
                character: '\0',
            },
            Message::NoTarget,
            Message::Interrupted {
                units_sent: 12480,
                units_expected: 100_000,
            },
            Message::EndOfPack { total: 34 },
            Message::CounterKept { done: 7, total: 34 },
            Message::NoPack,
            Message::ModifierHeld {
                key: "Ctrl".to_owned(),
            },
            Message::ClearingFailed,
            Message::ValueTooLarge {
                id: "len-100000".to_owned(),
            },
            Message::Unhandled {
                action: HotkeyAction::CopyReport,
            },
            Message::PressedWhileBusy {
                action: HotkeyAction::NextValue,
            },
            Message::ReportCopied {
                reference: "whitespace/nbsp".to_owned(),
            },
            Message::NothingToReport,
            Message::ReportBusy,
            Message::ReportFailed {
                detail: "the clipboard is not available".to_owned(),
            },
        ]
    }

    /// Every sentence this module can produce, for the rules that apply to all
    /// of them at once.
    fn every_sentence() -> Vec<String> {
        let mut out: Vec<String> = every_message()
            .iter()
            .map(|key| message(key, "unicode-text"))
            .collect();

        for error in [
            ChooseError::NotFound,
            ChooseError::Unreadable,
            ChooseError::Refused { errors: 1 },
            ChooseError::Refused { errors: 3 },
        ] {
            out.push(choose_error(&error, "locale-cz"));
        }
        for outcome in [
            ShortcutRegistration::Taken,
            ShortcutRegistration::Failed { code: 1409 },
        ] {
            out.push(
                registration(&outcome, HotkeyAction::NextValue)
                    .expect("both of these outcomes have a sentence"),
            );
        }
        out.push(shortcuts_unavailable(&ShortcutsUnavailable::Unsupported {
            system: "macOS".to_owned(),
        }));
        out.push(shortcuts_unavailable(&ShortcutsUnavailable::CouldNotStart));
        out.push(ended(Ended::ShortcutsGone).expect("the listener going away is said"));
        // A reason without a full stop of its own: the sentence-count rule below
        // is about OUR sentence, and a library error text carrying three full
        // stops would fail it for something we did not write.
        out.push(startup_failure(Startup::WindowFailed, "no OpenGL context"));
        out
    }

    #[test]
    fn no_sentence_leaves_a_placeholder_standing() {
        // The one failure `fill` cannot prevent on its own: a pattern naming an
        // argument the caller does not pass. It would reach a tester as a raw
        // `{sent}` in the palette.
        for sentence in every_sentence() {
            assert!(
                !sentence.contains('{') && !sentence.contains('}'),
                "a placeholder was left unfilled: {sentence}"
            );
        }
    }

    #[test]
    fn every_sentence_obeys_the_rules_the_document_sets_for_them() {
        for sentence in every_sentence() {
            assert!(!sentence.is_empty(), "a key with no sentence");
            // Untouchable rule 13: these strings live in the product repository.
            assert!(
                !sentence.contains('\u{2014}') && !sentence.contains('\u{2013}'),
                "long dash in a product string: {sentence}"
            );
            // The macOS modifier glyphs the document used to carry. On Windows
            // they name keys the tester does not have.
            assert!(
                !sentence.contains('\u{2303}') && !sentence.contains('\u{2325}'),
                "macOS modifier glyph in a sentence: {sentence}"
            );
            assert!(
                !sentence.to_lowercase().contains("sorry"),
                "an apology stands where an instruction belongs: {sentence}"
            );
            // "One to two sentences. A third means it belongs in help."
            let stops = sentence
                .split_terminator(['.', '?'])
                .filter(|part| !part.trim().is_empty())
                .count();
            assert!(
                stops <= 2,
                "{stops} sentences, the document allows two: {sentence}"
            );
        }
    }

    /// Each variant's slot in [`every_message`].
    ///
    /// 🔴 Exhaustive on purpose - no `_` arm - and that is the whole guard. Until
    /// 2026-09-23 the test below compared the list's length with a number
    /// written beside it, and four new variants went in with both unchanged and
    /// the test green: it counted its own list, never the variants. Now a new
    /// variant stops this file compiling until it has a slot, and the test then
    /// demands the list fill that slot.
    fn slot(message: &Message) -> usize {
        match message {
            Message::NoDirectRoute { .. } => 0,
            Message::ClipboardMode => 15,
            Message::ClipboardBusy => 16,
            Message::ClipboardFailed { .. } => 17,
            Message::NotForClipboard { .. } => 18,
            Message::HigherPrivileges => 19,
            Message::NoTarget => 1,
            Message::Interrupted { .. } => 2,
            Message::EndOfPack { .. } => 3,
            Message::CounterKept { .. } => 4,
            Message::NoPack => 5,
            Message::ModifierHeld { .. } => 6,
            Message::ClearingFailed => 7,
            Message::ValueTooLarge { .. } => 8,
            Message::Unhandled { .. } => 9,
            Message::PressedWhileBusy { .. } => 10,
            Message::ReportCopied { .. } => 11,
            Message::NothingToReport => 12,
            Message::ReportBusy => 13,
            Message::ReportFailed { .. } => 14,
        }
    }

    const SLOTS: usize = 20;

    #[test]
    fn every_message_variant_is_listed_here() {
        // The compiler guarantees `pattern_message` covers every variant. What
        // it cannot guarantee is that the LIST above grew with it, and a content
        // check over a stale list passes for the wrong reason.
        let mut listed = [false; SLOTS];
        for message in every_message() {
            listed[slot(&message)] = true;
        }
        for (slot, present) in listed.iter().enumerate() {
            assert!(
                present,
                "the Message variant in slot {slot} is missing from every_message()"
            );
        }
    }

    #[test]
    fn fill_does_not_rescan_what_it_substituted() {
        // A value carrying braces must be copied out, not treated as a new
        // placeholder. Today no argument can contain one - identifiers come from
        // a narrow alphabet - but the property is what makes that safe to stop
        // thinking about.
        let out = fill("a {x} b", &[("x", "{y}"), ("y", "NO")]);
        assert_eq!(out, "a {y} b");
    }

    #[test]
    fn fill_leaves_an_unknown_placeholder_visible() {
        assert_eq!(fill("a {x} b", &[]), "a {x} b");
        assert_eq!(fill("a { b", &[]), "a { b");
        assert_eq!(fill("{x}", &[("x", "")]), "");
    }

    #[test]
    fn the_two_silent_outcomes_stay_silent() {
        assert!(
            registration(&ShortcutRegistration::Registered, HotkeyAction::NextValue).is_none(),
            "a registration that worked is not an event"
        );
        assert!(
            ended(Ended::Stopped).is_none(),
            "closing on request is not an event"
        );
    }

    #[test]
    fn every_action_has_a_name_and_a_readable_shortcut() {
        let mut names = Vec::new();
        for action in HotkeyAction::ALL {
            let name = action_name(action);
            assert!(!name.is_empty(), "{action:?} has no name");
            names.push(name);

            let text = chord_text(action);
            assert!(
                text.starts_with("Ctrl+Alt+"),
                "{action:?} reads as {text}, and every default is Ctrl+Alt+<key>"
            );
        }
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(
            before,
            names.len(),
            "two actions share a name, so a message naming one would be ambiguous"
        );
    }

    #[test]
    fn a_shortcut_reads_the_way_a_tester_writes_it() {
        assert_eq!(chord_text(HotkeyAction::NextValue), "Ctrl+Alt+N");
        assert_eq!(chord_text(HotkeyAction::RestartPack), "Ctrl+Alt+0");
        assert_eq!(chord_text(HotkeyAction::OpenPacks), "Ctrl+Alt+Space");
        assert_eq!(
            chord(HotkeyChord {
                ctrl: true,
                alt: true,
                shift: true,
                win: true,
                key: HotkeyKey::H,
            }),
            "Ctrl+Alt+Shift+Win+H"
        );
    }

    #[test]
    fn the_counter_sentence_names_the_shortcut_it_asks_for() {
        // The one sentence that quotes a shortcut it did not receive as an
        // argument. It must name RestartPack, not whatever is first in the
        // table - the document says "press this to start the pack again".
        let out = message(&Message::CounterKept { done: 7, total: 34 }, "unicode-text");
        assert_eq!(
            out,
            "New field - the counter is still at 7/34. Press Ctrl+Alt+0 to start this pack from the beginning."
        );
    }

    #[test]
    fn the_plural_of_a_refused_pack_is_handled_rather_than_parenthesised() {
        assert_eq!(
            choose_error(&ChooseError::Refused { errors: 1 }, "locale-cz"),
            "Pack \"locale-cz\" has 1 problem. Run: nkb lint locale-cz"
        );
        assert_eq!(
            choose_error(&ChooseError::Refused { errors: 3 }, "locale-cz"),
            "Pack \"locale-cz\" has 3 problems. Run: nkb lint locale-cz"
        );
    }

    #[test]
    fn the_character_the_clipboard_refuses_is_named_as_a_code_point() {
        // The character itself would do in the palette what it does in the
        // clipboard: a NUL cuts the sentence short.
        let out = message(
            &Message::NotForClipboard {
                id: "nul-in-text".to_owned(),
                character: '\0',
            },
            "p",
        );
        assert_eq!(
            out,
            "Value \"nul-in-text\" contains U+0000, where clipboard text would be cut short, so it was not placed on the clipboard. Only direct input can deliver it whole."
        );
        assert!(!out.contains('\0'));
    }

    #[test]
    fn a_degenerate_but_legal_argument_still_reads_as_a_sentence() {
        // Zero of zero, an empty modifier name, an empty identifier: none of
        // these should occur, and all of them are reachable by a caller. The
        // sentence must not fall apart or go empty.
        for message in [
            Message::Interrupted {
                units_sent: 0,
                units_expected: 0,
            },
            Message::EndOfPack { total: 0 },
            Message::ModifierHeld { key: String::new() },
            Message::ValueTooLarge { id: String::new() },
        ] {
            let out = super::message(&message, "");
            assert!(!out.is_empty(), "{message:?} produced nothing");
            assert!(!out.contains('{'), "{message:?} left a placeholder: {out}");
        }
    }

    /// The palette's labels divide in two, and the division has to hold.
    ///
    /// ⚠️ The array below is the one thing here that can drift from the enum: a
    /// new variant left out of it is simply not visited. That is tolerable
    /// because it is not the only net - `tools/sprawdz-kontrakt.py` reads every
    /// `pattern_*` function and goes red for a key with no row in `ux-spec.md` 6,
    /// so a variant added in silence fails a gate either way.
    #[test]
    fn a_label_carrying_a_number_has_a_typed_accessor_and_one_without_does_not() {
        for label in [
            PaletteLabel::Title,
            PaletteLabel::Counter,
            PaletteLabel::Counts,
            PaletteLabel::Offensive,
            PaletteLabel::Cleared,
            PaletteLabel::OnClipboard,
            PaletteLabel::Warnings,
            PaletteLabel::ClipboardMode,
            PaletteLabel::ClipboardForWindow,
            PaletteLabel::PreviewElided,
            PaletteLabel::NotGuaranteed,
            PaletteLabel::NotGuaranteedMore,
            PaletteLabel::Recipe,
        ] {
            // Exhaustive, so a new variant must be put on one side or the other
            // before this file compiles.
            let takes_numbers = match label {
                PaletteLabel::Counter
                | PaletteLabel::Counts
                | PaletteLabel::Warnings
                | PaletteLabel::PreviewElided
                | PaletteLabel::NotGuaranteed
                | PaletteLabel::NotGuaranteedMore
                | PaletteLabel::Recipe => true,
                PaletteLabel::Title
                | PaletteLabel::Offensive
                | PaletteLabel::Cleared
                | PaletteLabel::OnClipboard
                | PaletteLabel::ClipboardMode
                | PaletteLabel::ClipboardForWindow => false,
            };
            let pattern = pattern_palette_label(label);
            assert_eq!(
                pattern.contains('{'),
                takes_numbers,
                "{label:?}: a pattern with a placeholder needs a typed accessor, and one \
                 without must be reachable through `label()` - pattern was {pattern:?}"
            );
            if !takes_numbers {
                assert!(
                    !super::label(label).is_empty(),
                    "{label:?} produced nothing"
                );
            }
        }
    }

    #[test]
    fn the_numbered_labels_leave_no_placeholder_standing() {
        // Zero, one and a value big enough to differ in all four units: every
        // one of these is reachable from a real pack.
        for text in [
            counter(0, 0),
            counter(7, 34),
            counts(0, 0, 0, 0),
            counts(1, 2, 8, 4),
            warnings(0),
            warnings(1),
            preview_elided(100, 100_000),
            recipe(1, "a"),
            recipe(100_000, "\u{2423}"),
        ] {
            assert!(!text.is_empty(), "a label produced nothing");
            assert!(!text.contains('{'), "a label left a placeholder: {text}");
        }
        assert_eq!(counter(7, 34), "7 / 34");
        assert_eq!(
            counts(1, 2, 8, 4),
            "1 graphemes, 2 code points, 8 bytes, 4 UTF-16 units"
        );
    }

    #[test]
    fn a_recipe_reads_the_way_the_sketch_draws_it() {
        // The sketch in `ux-spec.md` 2, less the digit grouping - see `recipe`.
        assert_eq!(recipe(100_000, "a"), "100000 \u{D7} \"a\"");
        // A unit holding braces is copied, never substituted a second time.
        assert_eq!(recipe(2, "{count}"), "2 \u{D7} \"{count}\"");
    }

    #[test]
    fn the_typeface_note_names_code_points_and_counts_what_it_does_not_name() {
        // Nothing outside the guarantee: nothing to say, and no way to say it.
        assert_eq!(not_guaranteed(&[]), None);

        // Written as code points, padded to four digits in the basic plane and
        // five above it - the form a tester can paste into a search.
        assert_eq!(
            not_guaranteed(&['\u{1F600}']).as_deref(),
            Some("not guaranteed by the bundled font: U+1F600")
        );
        assert_eq!(
            not_guaranteed(&['\u{E9}', '\u{1D407}']).as_deref(),
            Some("not guaranteed by the bundled font: U+00E9 U+1D407")
        );

        // Exactly as many as the note lists: named whole, no tail.
        let eight: Vec<char> = ('\u{FF41}'..='\u{FF48}').collect();
        let whole = not_guaranteed(&eight).expect("eight characters are something to say");
        assert!(whole.ends_with("U+FF48"), "{whole}");
        assert!(!whole.contains("more"), "{whole}");

        // One past it: the ninth is counted, not named.
        let nine: Vec<char> = ('\u{FF41}'..='\u{FF49}').collect();
        let counted = not_guaranteed(&nine).expect("nine characters are something to say");
        assert!(counted.ends_with("U+FF48 and 1 more"), "{counted}");
        assert!(!counted.contains("U+FF49"), "{counted}");
        assert!(
            !counted.contains('{'),
            "a placeholder was left standing: {counted}"
        );
    }
}
