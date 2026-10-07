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
use nkb_app::keep_settings::{SettingsMessage, ShortcutChange};
use nkb_app::ports::{
    CatalogueCoverage, CatalogueSource, SaveError, SettingsNote, SettingsUnusable,
    ShortcutRegistration, ShortcutUnreadable, ShortcutsUnavailable, SourceSkipped, StopReason,
};
use nkb_core::hotkeys::{
    Bindings, ChordError, ChordProblem, HotkeyAction, HotkeyChord, Refusal, Refused,
};
use nkb_core::preview::ShapeFact;
use nkb_core::report::{ControlKind, Target};

use crate::settings_file::SCHEMA;

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
/// something the table does not: `ToggleVisibility` is "collapse or expand",
/// never "show or hide", because the palette never hides - `hide()` destroys the
/// window and loses `WS_EX_NOACTIVATE` (`OBS-80`). A name promising a behaviour
/// the product will not have is a defect in the name. Until `D83` it read "show
/// or dim", which stopped being true when the timer that dimmed it went.
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
        HotkeyAction::OpenPacks => "Find a value",
        HotkeyAction::ToggleVisibility => "Collapse or expand the palette",
    }
}

/// A shortcut written the way a tester reads it: `Alt+Shift+N`.
///
/// The words are the core's ([`HotkeyChord::text`]), the same text the settings
/// file holds, so a shortcut on the screen is exactly what the tester writes
/// to change it. Keyboards print these words untranslated, which is why no
/// language has its own entry here. `Space` is a word rather than a character
/// on purpose - a blank between two plus signs is not readable as a key.
///
/// ⚠️ This is the WINDOWS AND LINUX way of writing keys, and on macOS it is
/// wrong in a way worth naming rather than discovering. `ux-spec.md` 3 writes the
/// macOS defaults as `⌥⌘N`, and this function prints the same chord as
/// `Alt+Win+N` - words a Mac user does not use for those keys. Delivery on macOS
/// is blocked anyway (`OBS-70`), so the palette does not run there yet and this
/// has no victim today. `OBS-115`.
#[must_use]
pub fn chord(chord: HotkeyChord) -> String {
    chord.text()
}

/// The shortcut an action answers to in this run, as text.
///
/// Reads the bindings IN EFFECT - the defaults with the tester's own on top
/// (`K4`) - which every caller passes in. This is the one function every
/// sentence naming a shortcut goes through, so a shortcut changed in the
/// settings file is changed in every sentence at once.
fn chord_text(bindings: &Bindings, action: HotkeyAction) -> String {
    chord(bindings.chord(action))
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
        Message::ClipboardModeOff => {
            "Clipboard mode is off: from the next press, each value is typed into the field."
        }
        Message::HigherPrivileges => {
            "This window runs with higher privileges than Naughty Keyboard, so the system blocks typing into it - values for it now go to your clipboard, replacing what you had copied. Press your paste shortcut to insert each one, or start the tool with the same privileges."
        }
        Message::NoTextField => {
            "The system reports the keyboard focus outside a text field, so nothing was sent - keys there could press buttons or act on list items. Click into the field and press the shortcut again."
        }
        Message::ClearingSkipped => {
            "Not cleared first: the system does not report this focus as a text field, and the clearing keys could reach beyond one. The value went in on top of what was there."
        }
        Message::ClearingSkippedInTerminal => {
            "Not cleared first: this is a terminal, where the clearing keys go to the program running in it and could act beyond the line. The value went in on top of what was there."
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
            "Stopped after {sent} of {expected} UTF-16 units: {reason}. The field holds a partial value - clear it before the next test."
        }
        Message::NothingArrived { .. } => {
            "Nothing reached the field: {reason}. The counter did not move, so the same shortcut tries this value again."
        }
        Message::ClearedThenNothingArrived { .. } => {
            "The field was cleared, but none of the value reached it: {reason}. The counter did not move, so the same shortcut tries this value again."
        }
        Message::NotPaced => {
            "This application's pace could not be followed, so characters may be missing if it was busy. Check the field before testing."
        }
        Message::EndOfPack { .. } => "End of pack ({total}/{total}). Press again to start over.",
        Message::CounterKept { .. } => {
            "New field - the counter is still at {done}/{total}. Press {shortcut} to start this pack from the beginning."
        }
        Message::NoPack => {
            "No pack is chosen, so there is nothing to send. Press {shortcut} to choose one."
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
        Message::ValueCopied { .. } => {
            "Value {reference} is on the clipboard. Paste it where you need it - the counter did not move."
        }
        Message::CopyBusy => {
            "Another application is holding the clipboard, so the value was not copied. Click {button} again in a moment."
        }
        Message::CopyFailed { .. } => {
            "The value was not copied: {detail}. The same value is printed by: nkb emit {pack}"
        }
        Message::CopyGone { .. } => {
            "{reference} is not in the pack in use any more, so nothing was copied. Click {button} beside the value on screen now."
        }
    }
}

/// What the palette says about one message, ready to show.
///
/// `pack` names the pack in play, for the two sentences that mention one. A
/// caller with no pack chosen passes an empty string, which only reaches a
/// sentence that cannot occur without a pack.
///
/// `bindings` are the shortcuts in effect, for the sentences that name one.
#[must_use]
pub fn message(message: &Message, pack: &str, bindings: &Bindings) -> String {
    let pattern = pattern_message(message);
    match message {
        Message::ClipboardMode
        | Message::ClipboardModeOff
        | Message::HigherPrivileges
        | Message::NoTextField
        | Message::ClearingSkipped
        | Message::ClearingSkippedInTerminal
        | Message::ClipboardBusy
        | Message::NoTarget
        | Message::ClearingFailed
        | Message::NotPaced
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
            reason,
        } => fill(
            pattern,
            &[
                ("sent", &units_sent.to_string()),
                ("expected", &units_expected.to_string()),
                ("reason", stop_reason(*reason)),
            ],
        ),
        Message::NothingArrived { reason } | Message::ClearedThenNothingArrived { reason } => {
            fill(pattern, &[("reason", stop_reason(*reason))])
        }
        Message::EndOfPack { total } => fill(pattern, &[("total", &total.to_string())]),
        Message::CounterKept { done, total } => fill(
            pattern,
            &[
                ("done", &done.to_string()),
                ("total", &total.to_string()),
                ("shortcut", &chord_text(bindings, HotkeyAction::RestartPack)),
            ],
        ),
        // `UX-GUI-014`: the value window, by its shortcut in effect, where a
        // pack is chosen since K3 - not a restart with a name the tester at
        // the desktop has no command line to type.
        Message::NoPack => fill(
            pattern,
            &[("shortcut", &chord_text(bindings, HotkeyAction::OpenPacks))],
        ),
        Message::ModifierHeld { key } => fill(pattern, &[("key", key)]),
        Message::ValueTooLarge { id } => fill(pattern, &[("id", id), ("pack", pack)]),
        Message::Unhandled { action } | Message::PressedWhileBusy { action } => {
            fill(pattern, &[("action", action_name(*action))])
        }
        Message::ReportCopied { reference } => fill(pattern, &[("reference", reference)]),
        Message::ReportBusy => fill(
            pattern,
            &[("shortcut", &chord_text(bindings, HotkeyAction::CopyReport))],
        ),
        Message::ReportFailed { detail } => fill(pattern, &[("detail", detail), ("pack", pack)]),
        Message::ValueCopied { reference } => fill(pattern, &[("reference", reference)]),
        // The button's own words, so the sentence and the button cannot drift.
        Message::CopyBusy => fill(pattern, &[("button", label(PaletteLabel::Copy))]),
        Message::CopyFailed { detail } => fill(pattern, &[("detail", detail), ("pack", pack)]),
        Message::CopyGone { reference } => fill(
            pattern,
            &[
                ("reference", reference),
                ("button", label(PaletteLabel::Copy)),
            ],
        ),
    }
}

// ---------------------------------------------------------------------------
// StopReason - why a send stopped, inside the sentences above
// ---------------------------------------------------------------------------

/// The pattern for one reason a send stopped. `ux-spec.md` 6, section A, byte
/// for byte. A clause, not a sentence: it stands after a colon in
/// `Message::Interrupted`, `Message::NothingArrived` and
/// `Message::ClearedThenNothingArrived` (`D95`), the way `{detail}` does in
/// `Message::ClipboardFailed`.
fn pattern_stop_reason(reason: StopReason) -> &'static str {
    match reason {
        StopReason::Dropped => {
            "the system did not deliver the keys - another program may be blocking input"
        }
        StopReason::FocusMoved => "another window came to the front",
        StopReason::NotTaking => "the application stopped taking keys",
        StopReason::Escape => "Escape was pressed",
    }
}

/// Why a send stopped, ready to stand inside a sentence.
#[must_use]
pub fn stop_reason(reason: StopReason) -> &'static str {
    pattern_stop_reason(reason)
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
/// there was no settings screen. It changed again on 2026-09-29 (`K4`): it said
/// "This build cannot change it", and since the settings file holds shortcuts
/// that was no longer true. It does not send the tester to the file either -
/// a hand-edited file is not the place a sentence may point at for this. The
/// shortcuts tab (`K5`) will be.
fn pattern_registration(outcome: &ShortcutRegistration) -> Option<&'static str> {
    match outcome {
        ShortcutRegistration::Registered => None,
        ShortcutRegistration::Taken => Some(
            "{shortcut} is already taken by another application, so \"{action}\" will not respond. Close the other application or choose another combination under \"{link}\".",
        ),
        ShortcutRegistration::Failed { .. } => Some(
            "{shortcut} could not be registered, so \"{action}\" will not respond. The system returned code {code}.",
        ),
    }
}

/// What the palette says about one shortcut registration, or nothing when it
/// simply worked. `chord` is the one that was registered for `action`.
#[must_use]
pub fn registration(
    outcome: &ShortcutRegistration,
    action: HotkeyAction,
    registered: HotkeyChord,
) -> Option<String> {
    let pattern = pattern_registration(outcome)?;
    let shortcut = chord(registered);
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
            // The link's own words, so the sentence and the link it sends the
            // tester to cannot drift apart.
            ("link", pattern_palette_label(PaletteLabel::ShortcutsLink)),
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
/// which needs English plural agreement. Polish needs THREE forms for the same
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
    /// The pack window opened, and the system kept the keyboard somewhere
    /// else - the letters of the search would go to the application under
    /// test (`ux-spec.md` 5.2, measured by `tools/sonda-okno-paczek`).
    PacksFocusNotTaken,
    /// The pack window closed, and the keyboard could not be handed back to
    /// the window it was taken from (`ux-spec.md` 5.2).
    FocusNotReturned,
    /// The shortcuts window opened, and the system kept the keyboard somewhere
    /// else - a combination pressed to record it would go to the application
    /// under test, whose keys the palette does not hold meanwhile (`D90`).
    ShortcutsFocusNotTaken,
    /// The shortcuts window could not keep its menu from opening on
    /// `Alt+Space` (`D91`, `slint.md` 2.37).
    ShortcutsMenuOpen,
    /// The welcome window opened, and the system kept the keyboard somewhere
    /// else - the first value would go to the window in front, not the box
    /// (UX7, `D105`).
    WelcomeFocusNotTaken,
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
        Environment::PacksFocusNotTaken => {
            "The pack window could not take the keyboard focus: {reason}. Click the pack window before typing, or the letters go to the application you are testing."
        }
        Environment::FocusNotReturned => {
            "The keyboard focus could not be returned to the window you were in: {reason}. Click into the field you are testing before pressing a shortcut."
        }
        Environment::ShortcutsFocusNotTaken => {
            "The shortcuts window could not take the keyboard focus: {reason}. Click the shortcuts window before pressing a combination, or it goes to the application you are testing."
        }
        Environment::ShortcutsMenuOpen => {
            "The shortcuts window could not keep its menu shut: {reason}. Press Esc if a menu opens."
        }
        Environment::WelcomeFocusNotTaken => {
            "The welcome window could not take the keyboard focus: {reason}. Click the box in the welcome window before pressing the shortcut."
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
// Settings - what the palette remembers, and when it cannot
// ---------------------------------------------------------------------------

/// How many unknown keys a sentence names before it counts the rest.
///
/// A file of garbage that happens to be valid TOML would otherwise put a line
/// per key into the message band and push the palette off the screen. Five
/// catches the typo a tester made, which is what the sentence is for.
const UNKNOWN_KEYS_NAMED: usize = 5;

/// The pattern for a file that is there and cannot be used. `ux-spec.md` 6,
/// section J.
///
/// Every one of them ends the same way on purpose: the palette starts from its
/// defaults and does NOT save over the file. The tester needs to hear the
/// second half most, because it is why a collapse or a pack chosen now will not
/// be remembered, and why the file they wrote is still exactly as they wrote it.
fn pattern_settings_unusable(why: &SettingsUnusable) -> &'static str {
    match why {
        SettingsUnusable::Unreadable => {
            "The settings file {file} could not be read, so the palette starts from its defaults and saves nothing. Check that the file can be opened, then start the palette again."
        }
        SettingsUnusable::NotUtf8 => {
            "The settings file {file} is not valid UTF-8, so the palette starts from its defaults and will not save over it. Save the file as UTF-8, or remove it."
        }
        SettingsUnusable::TooLarge { .. } => {
            "The settings file {file} is larger than {limit} bytes, so the palette did not read it and will not save over it. Check that the path names the right file."
        }
        SettingsUnusable::NotToml { .. } => {
            "The settings file {file} cannot be read at line {line} ({detail}), so the palette starts from its defaults and will not save over it. Fix that line, or remove the file."
        }
        SettingsUnusable::SchemaNotDeclared => {
            "The settings file {file} does not declare its schema, so the palette starts from its defaults and will not save over it. Add the line schema = 1 at the top, or remove the file."
        }
        SettingsUnusable::SchemaTooNew { .. } => {
            "The settings file {file} was written for schema {found} and this version reads schema {supported}, so the palette starts from its defaults and will not save over it. Use the newer version, or remove the file."
        }
    }
}

/// The pattern for something in a readable file that was not used.
///
/// Two patterns for `UnknownKeys`, like `ChooseError::Refused`: one names every
/// key, the other names the first few and counts the rest. The count is never
/// 1 - see `unknown_keys` - so "1 more keys" cannot be written.
fn pattern_settings_note(note: &SettingsNote) -> &'static str {
    match note {
        SettingsNote::UnknownKeys { keys } if keys.len() <= UNKNOWN_KEYS_NAMED + 1 => {
            "The settings file {file} holds {keys}, which this version does not know - kept as written, with no effect. Check the spelling if you added them yourself."
        }
        SettingsNote::UnknownKeys { .. } => {
            "The settings file {file} holds {keys} and {more} more keys this version does not know - kept as written, with no effect. Check the spelling if you added them yourself."
        }
        SettingsNote::NotTrueOrFalse { .. } => {
            "In the settings file {file}, {key} should be true or false, so its default is used. Fix the value, or remove the line."
        }
        SettingsNote::NotAPackName { .. } => {
            "In the settings file {file}, {key} should name a pack such as whitespace, so the default pack is used. Fix the value, or remove the line."
        }
        SettingsNote::NotAClearing { .. } => {
            "In the settings file {file}, {key} should be \"line\" or \"none\", so the line is cleared before each value. Fix the value, or remove the line."
        }
        SettingsNote::NotATable { .. } => {
            "In the settings file {file}, {key} should be a table of settings, so every setting in it takes its default. Fix it, or remove the line."
        }
        SettingsNote::NotAShortcut { .. } => {
            "In the settings file {file}, {key} should be a shortcut such as Alt+Shift+N, but {reason}, so its default is used. Fix the value, or remove the line."
        }
        SettingsNote::NotAPosition { .. } => {
            "In the settings file {file}, {key} should name a layout of screens and give the palette's place on it as whole numbers x and y, so it is not used. Fix the line, or remove it."
        }
        SettingsNote::NotARecentList { .. } => {
            "In the settings file {file}, {key} should be a list of values written as pack/value, such as \"whitespace/trailing-space\", so the value window starts with no recent values. The list is written anew when the palette closes."
        }
    }
}

/// Why a shortcut in the settings file did not read - the `{reason}` of
/// `SettingsNote::NotAShortcut`, one per way the core's grammar refuses a text.
fn pattern_shortcut_unreadable(why: &ShortcutUnreadable) -> &'static str {
    match why {
        ShortcutUnreadable::NotText => "it is not text in quotes",
        ShortcutUnreadable::Grammar(error) => match error {
            ChordError::Empty => "it is empty",
            ChordError::EmptyPart => "a plus sign has nothing on one side of it",
            ChordError::Unknown(_) => "\"{part}\" is not the name of a key or a modifier",
            ChordError::NoKey => "it names no key after the modifiers",
            ChordError::TwoKeys(_) => "\"{part}\" is a second key",
            ChordError::KeyNotLast(_) => "\"{part}\" comes after the key",
            ChordError::RepeatedModifier(_) => "\"{part}\" repeats a modifier",
        },
    }
}

/// The part of a shortcut the tester wrote, fit for a sentence: at most
/// [`PART_SHOWN`] characters, control characters and quotes escaped.
///
/// The text comes from a file anybody can write. Escaped, a newline in it
/// cannot start a second, made-up line in the palette's message band, and a
/// line pasted by mistake cannot push the palette off the screen.
fn shown_part(part: &str) -> String {
    let mut shown: String = part.chars().take(PART_SHOWN).collect::<String>();
    shown = shown.escape_debug().to_string();
    if part.chars().count() > PART_SHOWN {
        shown.push_str("...");
    }
    shown
}

/// A character a keyboard layout types, fit for a sentence: as it is when it
/// prints as itself, otherwise as its code point - `U+0301`, `U+0022`.
///
/// The character comes from the system, not from us. A combining mark drawn
/// as it is lands on the quote before it, a quote would close the quotes
/// around it, and a control character would not show at all. The code point
/// is the one form that reads the same in every one of those cases - and it
/// has no brace, which a finished sentence must not hold.
fn shown_character(character: char) -> String {
    if character.escape_debug().eq(core::iter::once(character)) {
        character.to_string()
    } else {
        format!("U+{:04X}", u32::from(character))
    }
}

/// How much of a wrongly written part a sentence repeats. Longer than any
/// modifier or key name, so a typo is always shown whole.
const PART_SHOWN: usize = 32;

/// The pattern for a change that was not saved, or nothing.
///
/// ⚠️ `Nowhere` has no sentence: the load already said there is no place, and
/// `KeptSettings` does not even attempt a save after that. A sentence here
/// would be a second telling of one fact.
fn pattern_save_error(error: &SaveError) -> Option<&'static str> {
    match error {
        SaveError::Nowhere => None,
        SaveError::Unusable(_) => Some(
            "The settings file {file} can no longer be read, so it was not saved over and this change will be forgotten when the palette closes. Fix the file, then start the palette again.",
        ),
        SaveError::Unwritable => Some(
            "The settings could not be saved to {file}, so this change will be forgotten when the palette closes. Check that the folder can be written to and the file is not read-only.",
        ),
    }
}

/// The pattern for the two settings messages that carry no inner type.
///
/// 🔴 `RememberedPackUnavailable` names the shortcut of the pack window, and it
/// could only start doing so once that window opened (K3.2c). Until then it sent
/// the tester to the command line, the one way to choose a pack - the same rule
/// that made `Taken` stop naming a settings screen that did not exist. A pack
/// chosen there is remembered (`D84`), so the sentence needs no second half.
fn pattern_settings_message(message: &SettingsMessage) -> Option<&'static str> {
    match message {
        SettingsMessage::Nowhere { .. } => Some(
            "Settings cannot be kept because {variable} does not name a folder, so nothing will be remembered after the palette closes. Set {variable} to a full path and start the palette again.",
        ),
        SettingsMessage::RememberedPackUnavailable { .. } => Some(
            "The remembered pack \"{remembered}\" could not be opened, so the palette opened \"{opened}\" for now. Press {shortcut} to choose another.",
        ),
        SettingsMessage::ShortcutNotUsed(Refused {
            why: Refusal::Problem(ChordProblem::NoCommandModifier),
            ..
        }) => Some(
            "The settings file {file} gives \"{action}\" the shortcut {wanted}, which holds no Ctrl, Alt or Win - taken globally it would stop that key working in every application, so the default {shortcut} is used. Add Ctrl, Alt or Win to it.",
        ),
        SettingsMessage::ShortcutNotUsed(Refused {
            why: Refusal::TypesCharacter(_),
            ..
        }) => Some(
            "The settings file {file} gives \"{action}\" the shortcut {wanted}, which types \"{character}\" as AltGr on a keyboard layout of this computer - taken globally it would stop that character working in every application, so the default {shortcut} is used. Choose a combination without Ctrl+Alt.",
        ),
        SettingsMessage::ShortcutNotUsed(Refused {
            why: Refusal::SameAs(_),
            ..
        }) => Some(
            "The settings file {file} gives \"{action}\" the shortcut {wanted}, which is already the shortcut for \"{other}\", so the default {shortcut} is used. Choose another combination for one of them.",
        ),
        SettingsMessage::PositionOffScreen { .. } => Some(
            "The place remembered for the palette on these screens, {x} and {y}, is off all of them, so the system placed it. Move it where you want it - the place is remembered when the palette closes.",
        ),
        SettingsMessage::Unusable(_) | SettingsMessage::Note(_) | SettingsMessage::NotSaved(_) => {
            None
        }
    }
}

/// The keys an `UnknownKeys` sentence names, and how many it only counts.
///
/// All of them up to one past the limit, because naming six costs less than
/// "and 1 more keys". Past that, the first five and the count of the rest,
/// which is then at least two.
fn unknown_keys(keys: &[String]) -> (String, usize) {
    if keys.len() <= UNKNOWN_KEYS_NAMED + 1 {
        (keys.join(", "), 0)
    } else {
        (
            keys[..UNKNOWN_KEYS_NAMED].join(", "),
            keys.len() - UNKNOWN_KEYS_NAMED,
        )
    }
}

/// What the palette says about its settings, or nothing when there is nothing
/// to say. `file` is where the settings live, for the tester to find, and
/// `bindings` the shortcuts in effect.
#[must_use]
pub fn settings_message(
    message: &SettingsMessage,
    file: &str,
    bindings: &Bindings,
) -> Option<String> {
    match message {
        SettingsMessage::Unusable(why) => {
            let (limit, line, detail, found) = match why {
                SettingsUnusable::TooLarge { limit_bytes } => {
                    (limit_bytes.to_string(), String::new(), "", String::new())
                }
                SettingsUnusable::NotToml { line, detail } => (
                    String::new(),
                    line.to_string(),
                    detail.as_str(),
                    String::new(),
                ),
                SettingsUnusable::SchemaTooNew { found } => {
                    (String::new(), String::new(), "", found.to_string())
                }
                SettingsUnusable::Unreadable
                | SettingsUnusable::NotUtf8
                | SettingsUnusable::SchemaNotDeclared => {
                    (String::new(), String::new(), "", String::new())
                }
            };
            Some(fill(
                pattern_settings_unusable(why),
                &[
                    ("file", file),
                    ("limit", &limit),
                    ("line", &line),
                    ("detail", detail),
                    ("found", &found),
                    ("supported", &SCHEMA.to_string()),
                ],
            ))
        }
        SettingsMessage::Note(note) => {
            let (key, keys, more) = match note {
                SettingsNote::UnknownKeys { keys } => {
                    let (named, more) = unknown_keys(keys);
                    ("", named, more.to_string())
                }
                SettingsNote::NotTrueOrFalse { key }
                | SettingsNote::NotAPackName { key }
                | SettingsNote::NotAClearing { key }
                | SettingsNote::NotATable { key }
                | SettingsNote::NotAShortcut { key, .. }
                | SettingsNote::NotAPosition { key }
                | SettingsNote::NotARecentList { key } => {
                    (key.as_str(), String::new(), String::new())
                }
            };
            // The reason is a sentence part of its own, filled first: the
            // tester's text goes into `{part}` and never into the sentence
            // pattern, so a brace in it stays a brace.
            let reason = match note {
                SettingsNote::NotAShortcut { why, .. } => {
                    let part = match why {
                        ShortcutUnreadable::Grammar(
                            ChordError::Unknown(part)
                            | ChordError::TwoKeys(part)
                            | ChordError::KeyNotLast(part)
                            | ChordError::RepeatedModifier(part),
                        ) => shown_part(part),
                        ShortcutUnreadable::NotText
                        | ShortcutUnreadable::Grammar(
                            ChordError::Empty | ChordError::EmptyPart | ChordError::NoKey,
                        ) => String::new(),
                    };
                    fill(pattern_shortcut_unreadable(why), &[("part", &part)])
                }
                SettingsNote::UnknownKeys { .. }
                | SettingsNote::NotTrueOrFalse { .. }
                | SettingsNote::NotAPackName { .. }
                | SettingsNote::NotAClearing { .. }
                | SettingsNote::NotATable { .. }
                | SettingsNote::NotAPosition { .. }
                | SettingsNote::NotARecentList { .. } => String::new(),
            };
            Some(fill(
                pattern_settings_note(note),
                &[
                    ("file", file),
                    ("key", key),
                    ("keys", &keys),
                    ("more", &more),
                    ("reason", &reason),
                ],
            ))
        }
        SettingsMessage::NotSaved(error) => {
            pattern_save_error(error).map(|pattern| fill(pattern, &[("file", file)]))
        }
        SettingsMessage::Nowhere { missing } => {
            pattern_settings_message(message).map(|pattern| fill(pattern, &[("variable", missing)]))
        }
        SettingsMessage::RememberedPackUnavailable { remembered, opened } => {
            pattern_settings_message(message).map(|pattern| {
                fill(
                    pattern,
                    &[
                        ("remembered", remembered),
                        ("opened", opened),
                        ("shortcut", &chord_text(bindings, HotkeyAction::OpenPacks)),
                    ],
                )
            })
        }
        SettingsMessage::ShortcutNotUsed(refused) => {
            let (other, character) = match refused.why {
                Refusal::SameAs(other) => (action_name(other), String::new()),
                Refusal::TypesCharacter(character) => ("", shown_character(character)),
                Refusal::Problem(_) => ("", String::new()),
            };
            pattern_settings_message(message).map(|pattern| {
                fill(
                    pattern,
                    &[
                        ("file", file),
                        ("action", action_name(refused.action)),
                        ("wanted", &chord(refused.chord)),
                        ("other", other),
                        ("character", &character),
                        ("shortcut", &chord_text(bindings, refused.action)),
                    ],
                )
            })
        }
        SettingsMessage::PositionOffScreen { at } => {
            pattern_settings_message(message).map(|pattern| {
                fill(
                    pattern,
                    &[("x", &at.x.to_string()), ("y", &at.y.to_string())],
                )
            })
        }
    }
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

fn pattern_palette_label(label: PaletteLabel) -> &'static str {
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
const PROGRAM_SHOWN: usize = 64;

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

/// The words of the pack window (`ux-spec.md` 5.2 and 6 K).
///
/// Its own enum rather than more `PaletteLabel` variants: the pack window is a
/// second window with its own words, and a label is found by the window it
/// stands in. Like `PaletteLabel`, it lives here and not in `app`, because only
/// a window needs to know that a pack is listed on two lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PacksLabel {
    /// The window's title, which the system shows in the task switcher.
    Title,
    /// The heading above the list.
    Heading,
    /// How many values and packs the list shows, with nothing typed.
    Summary,
    /// The same, while a query hides some of them - so a short list is never
    /// mistaken for a small catalogue.
    SummaryFiltered,
    /// The label above the query line, saying what it searches.
    Search,
    /// The pill on the pack the palette holds now.
    InUse,
    /// The pill on the value the next press sends (`UX-GUI-001`).
    Next,
    /// The second line of a pack that loads: its id, how many values, and what
    /// it is for (`UX-GUI-008` - the query searches the description, so the row
    /// shows it).
    Detail,
    /// The second line of a value: the pack it is in and where.
    ValueDetail,
    /// The same, for a value a query found in a part of it the row does not
    /// show - its id, tags or fields (`UX-GUI-008`). Without it a value found
    /// by its id looked found at random, as a pack found by its description
    /// did before the row showed the description.
    ValueDetailFound,
    /// The second line of a pack a query found in its tags, which the row does
    /// not show. The place stands BEFORE the description, because the view
    /// elides the line at its end and a long description would take it along.
    DetailFound,
    /// The parts the two above name, one word each.
    PlaceId,
    PlaceTags,
    PlaceFields,
    /// Two places and three, as one phrase - whole shapes, so a translation
    /// is never built from an English joining rule.
    PlacesTwo,
    PlacesThree,
    /// The section heading over the values of the pack in use, with nothing
    /// typed.
    ValuesIn,
    /// The section heading over the values a query found, in every pack.
    FoundValues,
    /// The section heading over the values the tester sent or chose last, in
    /// packs other than the one in use (`UX-GUI-016`).
    Recent,
    /// The section heading over the packs.
    PacksSection,
    /// Said in the palette when the value chosen in the window is not in the
    /// pack the palette holds - the window read the catalogue on its own.
    ValueGone,
    /// The pill on a pack with any offensive value (`product-spec.md` 10.2).
    Offensive,
    /// The second line of a pack that is present and refused.
    Refused,
    /// The pill on it: how many problems block it.
    Problems,
    /// The second line of a pack whose file would not open.
    Unreadable,
    /// The list is empty because nothing matches the query.
    NoMatch,
    /// The catalogue holds no pack at all.
    NoPacks,
    /// The catalogue itself could not be read.
    ListUnavailable,
    /// Which sources the list did not read, and why. 🔴 Said on every opening
    /// while a source is unread: a list from one source of three looks exactly
    /// like a complete one (untouchable rule 1, the same rule `nkb packs` keeps).
    NotRead,
    /// Said instead of `NotRead` when the version simply has no team and own
    /// folders and the built-in packs were read (`UX-GUI-009`).
    BuiltInOnly,
    /// The words for the sources, inside `NotRead`.
    SourceBuiltIn,
    SourceTeam,
    SourceOwn,
    /// The reasons, inside `NotRead`.
    ReasonNotCarried,
    ReasonCannotBeSet,
    ReasonNotSet,
    ReasonUnreadable,
    /// The footer: each key, and what it does here.
    KeyEnter,
    UseSelected,
    KeyEscape,
    Close,
    KeyArrows,
    Move,
}

fn pattern_packs_label(label: PacksLabel) -> &'static str {
    match label {
        PacksLabel::Title => "Naughty Keyboard - find a value",
        PacksLabel::Heading => "Find a value",
        PacksLabel::Summary => "values: {values}, packs: {packs}",
        PacksLabel::SummaryFiltered => {
            "values: {values} of {all_values}, packs: {packs} of {all_packs}"
        }
        PacksLabel::Search => "Search values and packs by name, tag or id",
        PacksLabel::InUse => "in use",
        PacksLabel::Next => "next",
        PacksLabel::Detail => "{id}, values: {count} - {description}",
        PacksLabel::ValueDetail => "{pack} - value {index} of {total}",
        PacksLabel::ValueDetailFound => "{pack} - value {index} of {total} - in its {places}",
        PacksLabel::DetailFound => "{id}, values: {count} - in its {places} - {description}",
        PacksLabel::PlaceId => "id",
        PacksLabel::PlaceTags => "tags",
        PacksLabel::PlaceFields => "fields",
        PacksLabel::PlacesTwo => "{first} and {second}",
        PacksLabel::PlacesThree => "{first}, {second} and {third}",
        PacksLabel::ValuesIn => "Values in {pack}",
        PacksLabel::FoundValues => "Values",
        PacksLabel::Recent => "Recent",
        PacksLabel::PacksSection => "Packs",
        PacksLabel::ValueGone => {
            "Value \"{value}\" is not in {pack} any more, so the next value did not change."
        }
        PacksLabel::Offensive => "offensive",
        PacksLabel::Refused => "{id}, does not load",
        PacksLabel::Problems => "problems: {count}",
        PacksLabel::Unreadable => "{id}, cannot be read",
        PacksLabel::NoMatch => "Nothing matches \"{query}\". Press Backspace to widen the search.",
        PacksLabel::NoPacks => {
            "No pack was found, so there is nothing to choose from. The note below says which sources were read."
        }
        PacksLabel::ListUnavailable => {
            "The pack list could not be read, so there is nothing to choose from. Close this window and start the palette with a pack name."
        }
        PacksLabel::NotRead => "Not read: {sources} - {reason}.",
        PacksLabel::BuiltInOnly => {
            "This version lists the built-in packs only - team and own pack folders are not in it yet."
        }
        PacksLabel::SourceBuiltIn => "built-in packs",
        PacksLabel::SourceTeam => "team folder",
        PacksLabel::SourceOwn => "own folder",
        PacksLabel::ReasonNotCarried => "not carried by this build",
        PacksLabel::ReasonCannotBeSet => "cannot be set in this version",
        PacksLabel::ReasonNotSet => "no folder is set",
        PacksLabel::ReasonUnreadable => "the folder could not be read",
        PacksLabel::KeyEnter => "Enter",
        PacksLabel::UseSelected => "Use the selected value or pack",
        PacksLabel::KeyEscape => "Esc",
        PacksLabel::Close => "Close",
        PacksLabel::KeyArrows => "↑ ↓",
        PacksLabel::Move => "Move through the list",
    }
}

/// A pack window label that carries nothing, ready to show.
///
/// The same division as [`label`]: a pattern with a placeholder has a typed
/// function below, and the test at the bottom refuses one reached from here.
#[must_use]
pub fn packs_label(label: PacksLabel) -> &'static str {
    pattern_packs_label(label)
}

/// How many values and packs the list shows.
#[must_use]
pub fn packs_summary(values: usize, packs: usize) -> String {
    fill(
        pattern_packs_label(PacksLabel::Summary),
        &[
            ("values", &values.to_string()),
            ("packs", &packs.to_string()),
        ],
    )
}

/// The same while a query hides some: of how many in the whole catalogue.
#[must_use]
pub fn packs_summary_filtered(
    values: usize,
    all_values: usize,
    packs: usize,
    all_packs: usize,
) -> String {
    fill(
        pattern_packs_label(PacksLabel::SummaryFiltered),
        &[
            ("values", &values.to_string()),
            ("all_values", &all_values.to_string()),
            ("packs", &packs.to_string()),
            ("all_packs", &all_packs.to_string()),
        ],
    )
}

/// Where a query word was found, among the parts of a pack or a value that its
/// row does not show (`UX-GUI-008`), in the order a row names them.
///
/// Here and not in the window's code, for the reason `PacksLabel` gives: only
/// a window needs to know which parts of a row are on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MatchPlace {
    /// A value's id - a pack's stands in its row.
    Id,
    Tags,
    /// The field kinds a value narrows its pack's list to.
    Fields,
}

/// The places as one phrase: `id`, `id and tags`, `id, tags and fields`. In
/// the order of [`MatchPlace`] and each once, however the caller listed them.
fn places(found: &[MatchPlace]) -> String {
    let mut found = found.to_vec();
    found.sort_unstable();
    found.dedup();
    let word = |place: &MatchPlace| {
        pattern_packs_label(match place {
            MatchPlace::Id => PacksLabel::PlaceId,
            MatchPlace::Tags => PacksLabel::PlaceTags,
            MatchPlace::Fields => PacksLabel::PlaceFields,
        })
    };
    match found.as_slice() {
        [] => String::new(),
        [one] => word(one).to_owned(),
        [first, second] => fill(
            pattern_packs_label(PacksLabel::PlacesTwo),
            &[("first", word(first)), ("second", word(second))],
        ),
        // Three at most: `MatchPlace` has three, each kept once.
        [first, second, third, ..] => fill(
            pattern_packs_label(PacksLabel::PlacesThree),
            &[
                ("first", word(first)),
                ("second", word(second)),
                ("third", word(third)),
            ],
        ),
    }
}

/// The second line of a pack that loads: `whitespace, values: 12 - Spaces...`.
/// With `found`, the parts of the pack a query was found in that the row does
/// not show, named before the description: `locale-pl, values: 12 - in its
/// tags - Polish...`.
#[must_use]
pub fn pack_detail(id: &str, values: usize, description: &str, found: &[MatchPlace]) -> String {
    let count = values.to_string();
    if found.is_empty() {
        return fill(
            pattern_packs_label(PacksLabel::Detail),
            &[("id", id), ("count", &count), ("description", description)],
        );
    }
    fill(
        pattern_packs_label(PacksLabel::DetailFound),
        &[
            ("id", id),
            ("count", &count),
            ("places", &places(found)),
            ("description", description),
        ],
    )
}

/// The second line of a value: `Polish locale - value 3 of 12`. With `found`,
/// the parts of the value a query was found in that the row does not show:
/// `Whitespace - value 5 of 12 - in its id`.
#[must_use]
pub fn value_detail(pack: &str, index: usize, total: usize, found: &[MatchPlace]) -> String {
    let index = index.to_string();
    let total = total.to_string();
    if found.is_empty() {
        return fill(
            pattern_packs_label(PacksLabel::ValueDetail),
            &[("pack", pack), ("index", &index), ("total", &total)],
        );
    }
    fill(
        pattern_packs_label(PacksLabel::ValueDetailFound),
        &[
            ("pack", pack),
            ("index", &index),
            ("total", &total),
            ("places", &places(found)),
        ],
    )
}

/// The heading over the values of the pack in use.
#[must_use]
pub fn values_in(pack: &str) -> String {
    fill(pattern_packs_label(PacksLabel::ValuesIn), &[("pack", pack)])
}

/// Said when the value chosen in the window is not in the pack in use.
#[must_use]
pub fn value_gone(value: &str, pack: &str) -> String {
    fill(
        pattern_packs_label(PacksLabel::ValueGone),
        &[("value", value), ("pack", pack)],
    )
}

/// The second line of a pack that is present and refused.
#[must_use]
pub fn pack_refused(id: &str) -> String {
    fill(pattern_packs_label(PacksLabel::Refused), &[("id", id)])
}

/// The pill on a refused pack.
#[must_use]
pub fn pack_problems(count: usize) -> String {
    fill(
        pattern_packs_label(PacksLabel::Problems),
        &[("count", &count.to_string())],
    )
}

/// The second line of a pack whose file would not open.
#[must_use]
pub fn pack_unreadable(id: &str) -> String {
    fill(pattern_packs_label(PacksLabel::Unreadable), &[("id", id)])
}

/// What the list says when nothing matches the query.
///
/// The query is copied as typed and never substituted again - a query holding
/// braces is text, not a pattern.
#[must_use]
pub fn no_match(query: &str) -> String {
    fill(
        pattern_packs_label(PacksLabel::NoMatch),
        &[("query", query)],
    )
}

/// One line per reason a source went unread, naming the sources it covers.
///
/// Empty when every source was read. Grouped by reason, so the build today says
/// one line for the team and own folders rather than two lines that differ in
/// one word.
///
/// `UX-GUI-009`: when the only thing missing is that this version HAS no team
/// and own folders, while the built-in packs were read, the line says so in
/// the tester's words (`PacksLabel::BuiltInOnly`). "Not read: team folder, own
/// folder" stood under the list on every opening and read like a failure - and
/// a note that cries wolf teaches the tester to skip the one that will one day
/// name a folder that really could not be read. Every other case keeps "Not
/// read" (untouchable rule 1 asks for the truth, not for alarm).
#[must_use]
pub fn not_read(coverage: &CatalogueCoverage) -> Vec<String> {
    let mut groups: Vec<(PacksLabel, Vec<&'static str>)> = Vec::new();
    for (source, skipped) in &coverage.skipped {
        let reason = match (skipped, source) {
            (SourceSkipped::NotImplementedYet, CatalogueSource::BuiltIn) => {
                PacksLabel::ReasonNotCarried
            }
            (SourceSkipped::NotImplementedYet, CatalogueSource::Team | CatalogueSource::Own) => {
                PacksLabel::ReasonCannotBeSet
            }
            (SourceSkipped::NotConfigured, _) => PacksLabel::ReasonNotSet,
            (SourceSkipped::Unreadable, _) => PacksLabel::ReasonUnreadable,
        };
        let name = pattern_packs_label(match source {
            CatalogueSource::BuiltIn => PacksLabel::SourceBuiltIn,
            CatalogueSource::Team => PacksLabel::SourceTeam,
            CatalogueSource::Own => PacksLabel::SourceOwn,
        });
        match groups.iter_mut().find(|(known, _)| *known == reason) {
            Some((_, names)) => names.push(name),
            None => groups.push((reason, vec![name])),
        }
    }
    // Both folders, and only for the reason that they do not exist yet - the
    // group holds nothing else - with the built-in packs read.
    let built_in_read = coverage.consulted.contains(&CatalogueSource::BuiltIn);
    groups
        .into_iter()
        .map(|(reason, names)| {
            if reason == PacksLabel::ReasonCannotBeSet && names.len() == 2 && built_in_read {
                return pattern_packs_label(PacksLabel::BuiltInOnly).to_owned();
            }
            fill(
                pattern_packs_label(PacksLabel::NotRead),
                &[
                    ("sources", &names.join(", ")),
                    ("reason", pattern_packs_label(reason)),
                ],
            )
        })
        .collect()
}

// ---------------------------------------------------------------------------
// The shortcuts window - its words, and what a change came to
// ---------------------------------------------------------------------------

/// The words of the shortcuts window (`ux-spec.md` 5.4 and 6 L).
///
/// A third window with its own words, for the reason `PacksLabel` gives. The
/// sentences about a change are keys here too, rather than arms matched on
/// `ShortcutChange` and `Refusal`: those two types already key the sentences
/// the palette says about the settings file (`ux-spec.md` 6 J), and one key
/// may name one sentence. The completeness the compiler guards is kept all
/// the same - [`shortcut_answer`] matches `ShortcutChange` and `Refusal`
/// exhaustively, so a new variant does not compile without a sentence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutsLabel {
    /// The window's title, which the system shows in the task switcher.
    Title,
    /// The heading above the list.
    Heading,
    /// How many shortcuts are the tester's own.
    Summary,
    /// What the window does, and that the palette does not answer meanwhile.
    Intro,
    /// The pill on the row being recorded.
    Recording,
    /// The pill on a shortcut that is not the default.
    Changed,
    /// The pill on a shortcut another application held at the last
    /// registration.
    Taken,
    /// The pill on a shortcut the system refused for another reason.
    NotRegistered,
    /// The pill on an action this version does not carry out yet - its
    /// shortcut stays registered, so nobody else takes it (`UX-GUI-006`).
    NotAvailable,
    /// The footer: each key, and what it does here.
    KeyEnter,
    Record,
    KeyDelete,
    Restore,
    KeyArrows,
    Move,
    KeyEscape,
    Close,
    /// Recording has started: what to press.
    Invite,
    /// A recorded shortcut was kept.
    Recorded,
    /// The default was given back.
    Restored,
    /// The action answers the shortcut already.
    AlreadySo,
    /// Another action moved onto the shortcut the settings file wishes for it.
    Also,
    /// Kept, but whether another application holds it could not be asked.
    Unchecked,
    /// Refused: no `Ctrl`, `Alt` or `Win`.
    NoModifier,
    /// Refused: a keyboard layout types a character on it as `AltGr`.
    TypesCharacter,
    /// Refused: another action answers it.
    HeldBy,
    /// Refused: another application holds it.
    TakenElsewhere,
    /// Refused by the system for another reason.
    NotRegisteredCode,
    /// The key pressed cannot end a shortcut.
    NotAKey,
    /// Two keys of the vocabulary were held.
    SeveralKeys,
    /// This system cannot tell which key was pressed.
    CannotTell,
}

/// The words of the welcome window (`ux-spec.md` 5.1 and 6 N, UX7, `D105`).
///
/// A fourth window with its own words, for the reason `PacksLabel` gives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WelcomeLabel {
    /// The window's title, which the system shows in the task switcher.
    Title,
    /// The heading in the window's header.
    Heading,
    /// The first step's heading.
    TryHeading,
    /// The first step: `ux-spec.md` 5.1's own sentence, with the shortcut in
    /// effect.
    TryIt,
    /// What UI Automation calls the box.
    BoxName,
    /// Under the box while nothing has arrived in it.
    Empty,
    /// The second step's heading.
    DoesHeading,
    /// The second step: what the tool does to other windows and what it reads
    /// (`product-spec.md` 10.3).
    Does,
    /// `product-spec.md` 10.2 point 3, said once, here.
    Allowed,
    /// The third step's heading.
    ReadyHeading,
    /// The third step, naming the pack the palette is on.
    Ready,
    /// The same, before the palette has said which pack it is on.
    ReadyNoPack,
    /// The window's main action.
    Start,
    /// What it does, for UI Automation.
    StartAction,
}

fn pattern_welcome_label(label: WelcomeLabel) -> &'static str {
    match label {
        WelcomeLabel::Title => "Naughty Keyboard - welcome",
        WelcomeLabel::Heading => "Welcome to Naughty Keyboard",
        WelcomeLabel::TryHeading => "Try it here first",
        WelcomeLabel::TryIt => "Click the box below and press {shortcut}.",
        WelcomeLabel::BoxName => "The box for the first try",
        WelcomeLabel::Empty => "Nothing has arrived yet.",
        WelcomeLabel::DoesHeading => "What it does",
        WelcomeLabel::Does => {
            "Naughty Keyboard types into the window in front, the way a keyboard does. It does not read what the window shows - only the name of its program and the kind of control that has the keyboard."
        }
        WelcomeLabel::Allowed => "Use it only on systems you are allowed to test.",
        WelcomeLabel::ReadyHeading => "Ready",
        WelcomeLabel::Ready => {
            "The palette is on the pack {pack}. Press {shortcut} to choose another pack or value."
        }
        WelcomeLabel::ReadyNoPack => "Press {shortcut} to choose a pack or a value.",
        WelcomeLabel::Start => "Start testing",
        WelcomeLabel::StartAction => "Close the welcome and start testing",
    }
}

/// A welcome window label that carries nothing, ready to show. A pattern
/// with a placeholder has a typed function below.
#[must_use]
pub fn welcome_label(label: WelcomeLabel) -> &'static str {
    pattern_welcome_label(label)
}

/// The first step, naming the shortcut that sends the next value.
#[must_use]
pub fn welcome_try_it(next_value: HotkeyChord) -> String {
    fill(
        pattern_welcome_label(WelcomeLabel::TryIt),
        &[("shortcut", &chord(next_value))],
    )
}

/// The third step: the pack the palette is on, when it is known, and the
/// shortcut of the value window.
#[must_use]
pub fn welcome_ready(pack: Option<&str>, open_packs: HotkeyChord) -> String {
    let shortcut = chord(open_packs);
    match pack {
        Some(pack) => fill(
            pattern_welcome_label(WelcomeLabel::Ready),
            &[("pack", pack), ("shortcut", &shortcut)],
        ),
        None => fill(
            pattern_welcome_label(WelcomeLabel::ReadyNoPack),
            &[("shortcut", &shortcut)],
        ),
    }
}

fn pattern_shortcuts_label(label: ShortcutsLabel) -> &'static str {
    match label {
        ShortcutsLabel::Title => "Naughty Keyboard - shortcuts",
        ShortcutsLabel::Heading => "Shortcuts",
        ShortcutsLabel::Summary => "changed: {count} of {total}",
        ShortcutsLabel::Intro => {
            "The palette answers these in every application. They are paused while this window is open, and a change takes effect when it closes."
        }
        ShortcutsLabel::Recording => "recording",
        ShortcutsLabel::Changed => "changed",
        ShortcutsLabel::Taken => "taken",
        ShortcutsLabel::NotRegistered => "not registered",
        ShortcutsLabel::NotAvailable => "not available yet",
        ShortcutsLabel::KeyEnter => "Enter",
        ShortcutsLabel::Record => "Record a new shortcut",
        ShortcutsLabel::KeyDelete => "Delete",
        ShortcutsLabel::Restore => "Restore the default",
        ShortcutsLabel::KeyArrows => "↑ ↓",
        ShortcutsLabel::Move => "Move through the list",
        ShortcutsLabel::KeyEscape => "Esc",
        ShortcutsLabel::Close => "Close",
        ShortcutsLabel::Invite => {
            "Press the new shortcut for \"{action}\", or Esc to stop recording."
        }
        ShortcutsLabel::Recorded => "\"{action}\" answers {shortcut} from when this window closes.",
        ShortcutsLabel::Restored => {
            "\"{action}\" answers its default {shortcut} again from when this window closes."
        }
        ShortcutsLabel::AlreadySo => "\"{action}\" already answers {shortcut}.",
        ShortcutsLabel::Also => "\"{action}\" moves to {shortcut}, as the settings file asks.",
        ShortcutsLabel::Unchecked => {
            "Whether another application holds {shortcut} could not be checked: {reason}."
        }
        ShortcutsLabel::NoModifier => {
            "{shortcut} holds no Ctrl, Alt or Win - taken globally it would stop that key working in every application. Add Ctrl, Alt or Win to it."
        }
        ShortcutsLabel::TypesCharacter => {
            "{shortcut} types \"{character}\" as AltGr on a keyboard layout of this computer - taken globally it would stop that character working in every application. Choose a combination without Ctrl+Alt."
        }
        ShortcutsLabel::HeldBy => {
            "{shortcut} is already the shortcut for \"{other}\". Give \"{other}\" another combination first."
        }
        ShortcutsLabel::TakenElsewhere => {
            "{shortcut} is taken by another application, so it was not kept. Close that application or choose another combination."
        }
        ShortcutsLabel::NotRegisteredCode => {
            "{shortcut} could not be registered - the system returned code {code} - so it was not kept."
        }
        ShortcutsLabel::NotAKey => {
            "That key cannot end a shortcut. End it with a letter, a digit, F1 to F24 or Space, or press Esc to stop recording."
        }
        ShortcutsLabel::SeveralKeys => {
            "More than one key is held. Hold the modifiers and press a single key."
        }
        ShortcutsLabel::CannotTell => {
            "This system cannot tell which key was pressed, so a shortcut cannot be recorded here yet."
        }
    }
}

/// A shortcuts window label that carries nothing, ready to show.
///
/// The same division as [`label`]: a pattern with a placeholder has a typed
/// function below, and the test at the bottom refuses one reached from here.
#[must_use]
pub fn shortcuts_label(label: ShortcutsLabel) -> &'static str {
    pattern_shortcuts_label(label)
}

/// How many shortcuts are the tester's own.
#[must_use]
pub fn shortcuts_summary(changed: usize, total: usize) -> String {
    fill(
        pattern_shortcuts_label(ShortcutsLabel::Summary),
        &[
            ("count", &changed.to_string()),
            ("total", &total.to_string()),
        ],
    )
}

/// What to press, once recording has started for `action`.
#[must_use]
pub fn shortcut_invite(action: HotkeyAction) -> String {
    fill(
        pattern_shortcuts_label(ShortcutsLabel::Invite),
        &[("action", action_name(action))],
    )
}

/// What changing `action` came to, a line per fact.
///
/// `asked` is what was asked for - a chord recorded, or `None` for the
/// default back - and `answers` the chord the action answers now, which the
/// window knows and `AlreadySo` does not carry.
///
/// Matches `ShortcutChange` and `Refusal` exhaustively, so a new answer does
/// not compile without its sentence (the reason keys are types, see the top
/// of this file).
#[must_use]
pub fn shortcut_answer(
    action: HotkeyAction,
    asked: Option<HotkeyChord>,
    change: &ShortcutChange,
    answers: HotkeyChord,
) -> Vec<String> {
    let name = action_name(action);
    let about = |label: ShortcutsLabel, held: HotkeyChord| {
        fill(
            pattern_shortcuts_label(label),
            &[("action", name), ("shortcut", &chord(held))],
        )
    };
    match change {
        ShortcutChange::AlreadySo => vec![about(ShortcutsLabel::AlreadySo, answers)],
        ShortcutChange::Changed {
            bindings,
            also,
            unchecked,
        } => {
            let now = bindings.chord(action);
            let kept = if asked.is_some() {
                ShortcutsLabel::Recorded
            } else {
                ShortcutsLabel::Restored
            };
            let mut lines = vec![about(kept, now)];
            lines.extend(also.iter().map(|other| {
                fill(
                    pattern_shortcuts_label(ShortcutsLabel::Also),
                    &[
                        ("action", action_name(*other)),
                        ("shortcut", &chord(bindings.chord(*other))),
                    ],
                )
            }));
            if let Some(why) = unchecked {
                lines.push(fill(
                    pattern_shortcuts_label(ShortcutsLabel::Unchecked),
                    &[("shortcut", &chord(now)), ("reason", &why.to_string())],
                ));
            }
            lines
        }
        ShortcutChange::Refused(refused) => {
            let (label, other, character) = match refused.why {
                Refusal::Problem(ChordProblem::NoCommandModifier) => {
                    (ShortcutsLabel::NoModifier, "", String::new())
                }
                Refusal::TypesCharacter(character) => (
                    ShortcutsLabel::TypesCharacter,
                    "",
                    shown_character(character),
                ),
                Refusal::SameAs(other) => {
                    (ShortcutsLabel::HeldBy, action_name(other), String::new())
                }
            };
            vec![fill(
                pattern_shortcuts_label(label),
                &[
                    ("shortcut", &chord(refused.chord)),
                    ("other", other),
                    ("character", &character),
                ],
            )]
        }
        ShortcutChange::Taken { chord: held } => vec![fill(
            pattern_shortcuts_label(ShortcutsLabel::TakenElsewhere),
            &[("shortcut", &chord(*held))],
        )],
        ShortcutChange::NotRegistered { chord: held, code } => vec![fill(
            pattern_shortcuts_label(ShortcutsLabel::NotRegisteredCode),
            &[("shortcut", &chord(*held)), ("code", &code.to_string())],
        )],
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;
    use crate::shortcuts::CONVENTION;
    use nkb_core::hotkeys::{Convention, HotkeyKey};

    /// The shortcuts of a palette whose settings name none.
    fn defaults() -> Bindings {
        Bindings::defaults(CONVENTION)
    }

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
            Message::ClipboardModeOff,
            Message::HigherPrivileges,
            Message::NoTextField,
            Message::ClearingSkipped,
            Message::ClearingSkippedInTerminal,
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
                reason: StopReason::NotTaking,
            },
            Message::NothingArrived {
                reason: StopReason::Dropped,
            },
            Message::ClearedThenNothingArrived {
                reason: StopReason::FocusMoved,
            },
            Message::NotPaced,
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
            Message::ValueCopied {
                reference: "whitespace/nbsp".to_owned(),
            },
            Message::CopyBusy,
            Message::CopyFailed {
                detail: "the clipboard is not available".to_owned(),
            },
            Message::CopyGone {
                reference: "whitespace/nbsp".to_owned(),
            },
        ]
    }

    /// Every sentence this module can produce, for the rules that apply to all
    /// of them at once.
    fn every_sentence() -> Vec<String> {
        let mut out: Vec<String> = every_message()
            .iter()
            .map(|key| message(key, "unicode-text", &defaults()))
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
                registration(
                    &outcome,
                    HotkeyAction::NextValue,
                    defaults().chord(HotkeyAction::NextValue),
                )
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
        out.extend(
            every_settings_message()
                .iter()
                .filter_map(|message| settings_message(message, "the-settings-file", &defaults())),
        );
        out.push(no_match("zzz"));
        out.push(packs_label(PacksLabel::NoPacks).to_owned());
        out.push(packs_label(PacksLabel::ListUnavailable).to_owned());
        out.extend(not_read(&every_skip()));
        out
    }

    /// A coverage that skips every source for every reason it can be skipped
    /// for, so each reason word is exercised at least once.
    fn every_skip() -> CatalogueCoverage {
        CatalogueCoverage {
            consulted: Vec::new(),
            skipped: vec![
                (CatalogueSource::BuiltIn, SourceSkipped::NotImplementedYet),
                (CatalogueSource::Team, SourceSkipped::NotImplementedYet),
                (CatalogueSource::Own, SourceSkipped::NotConfigured),
                (CatalogueSource::Team, SourceSkipped::Unreadable),
            ],
        }
    }

    /// One of every settings message that has a sentence.
    ///
    /// The file and the keys carry no full stop on purpose, for the reason the
    /// startup line above gives: the sentence count is about OUR sentence, and
    /// `settings.toml` would add a stop that is not ours.
    fn every_settings_message() -> Vec<SettingsMessage> {
        let mut all = vec![
            SettingsMessage::Nowhere {
                missing: String::from("APPDATA"),
            },
            SettingsMessage::RememberedPackUnavailable {
                remembered: String::from("team-pack"),
                opened: String::from("whitespace"),
            },
            SettingsMessage::NotSaved(SaveError::Unwritable),
            SettingsMessage::NotSaved(SaveError::Unusable(SettingsUnusable::NotUtf8)),
        ];
        for why in [
            SettingsUnusable::Unreadable,
            SettingsUnusable::NotUtf8,
            SettingsUnusable::TooLarge { limit_bytes: 65536 },
            SettingsUnusable::NotToml {
                line: 3,
                detail: String::from("unclosed table, expected `]`"),
            },
            SettingsUnusable::SchemaNotDeclared,
            SettingsUnusable::SchemaTooNew { found: 2 },
        ] {
            all.push(SettingsMessage::Unusable(why));
        }
        for note in [
            SettingsNote::UnknownKeys {
                keys: vec![String::from("compct")],
            },
            SettingsNote::UnknownKeys {
                keys: (1..=9).map(|n| format!("key{n}")).collect(),
            },
            SettingsNote::NotTrueOrFalse {
                key: String::from("compact"),
            },
            SettingsNote::NotAPackName {
                key: String::from("pack"),
            },
            SettingsNote::NotAClearing {
                key: String::from("clearing"),
            },
            SettingsNote::NotATable {
                key: String::from("palette"),
            },
        ] {
            all.push(SettingsMessage::Note(note));
        }
        all
    }

    #[test]
    fn a_settings_sentence_says_where_the_file_is_and_what_went_wrong_in_it() {
        assert_eq!(
            settings_message(
                &SettingsMessage::Unusable(SettingsUnusable::NotToml {
                    line: 3,
                    detail: String::from("unclosed table, expected `]`"),
                }),
                "C:\\Users\\t\\AppData\\Roaming\\Naughty Keyboard\\settings.toml",
                &defaults(),
            )
            .as_deref(),
            Some(
                "The settings file C:\\Users\\t\\AppData\\Roaming\\Naughty Keyboard\\settings.toml cannot be read at line 3 (unclosed table, expected `]`), so the palette starts from its defaults and will not save over it. Fix that line, or remove the file."
            )
        );
        assert_eq!(
            settings_message(
                &SettingsMessage::Unusable(SettingsUnusable::SchemaTooNew { found: 2 }),
                "f",
                &defaults(),
            )
            .as_deref(),
            Some(
                "The settings file f was written for schema 2 and this version reads schema 1, so the palette starts from its defaults and will not save over it. Use the newer version, or remove the file."
            )
        );
    }

    #[test]
    fn unknown_keys_are_named_up_to_six_and_counted_past_that() {
        // Six are named rather than five and "1 more keys".
        let six: Vec<String> = (1..=6).map(|n| format!("k{n}")).collect();
        assert_eq!(
            settings_message(
                &SettingsMessage::Note(SettingsNote::UnknownKeys { keys: six }),
                "f",
                &defaults()
            )
            .as_deref(),
            Some(
                "The settings file f holds k1, k2, k3, k4, k5, k6, which this version does not know - kept as written, with no effect. Check the spelling if you added them yourself."
            )
        );
        let seven: Vec<String> = (1..=7).map(|n| format!("k{n}")).collect();
        assert_eq!(
            settings_message(
                &SettingsMessage::Note(SettingsNote::UnknownKeys { keys: seven }),
                "f",
                &defaults()
            )
            .as_deref(),
            Some(
                "The settings file f holds k1, k2, k3, k4, k5 and 2 more keys this version does not know - kept as written, with no effect. Check the spelling if you added them yourself."
            )
        );
    }

    #[test]
    fn no_place_for_settings_is_said_at_the_start_and_not_again_at_each_save() {
        assert!(
            settings_message(
                &SettingsMessage::NotSaved(SaveError::Nowhere),
                "f",
                &defaults()
            )
            .is_none(),
            "the load already said there is no place"
        );
        assert!(
            settings_message(
                &SettingsMessage::Nowhere {
                    missing: String::from("HOME")
                },
                "f",
                &defaults()
            )
            .is_some_and(|line| line.contains("HOME does not name a folder"))
        );
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
            Message::NoTextField => 20,
            Message::ClearingSkipped => 21,
            Message::ClearingSkippedInTerminal => 22,
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
            Message::NothingArrived { .. } => 23,
            Message::ClearedThenNothingArrived { .. } => 24,
            Message::NotPaced => 25,
            Message::ValueCopied { .. } => 26,
            Message::CopyBusy => 27,
            Message::CopyFailed { .. } => 28,
            Message::CopyGone { .. } => 29,
            Message::ClipboardModeOff => 30,
        }
    }

    const SLOTS: usize = 31;

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
    fn the_copy_sentences_name_the_value_the_button_and_the_way_round_it() {
        // `D98`: the button's word is fetched, never written into a sentence, so
        // a translated button and the sentence sending the tester to it agree.
        let said = |what: Message| message(&what, "whitespace", &defaults());
        assert_eq!(
            said(Message::ValueCopied {
                reference: "whitespace/nbsp".to_owned()
            }),
            "Value whitespace/nbsp is on the clipboard. Paste it where you need it - the counter did not move."
        );
        assert!(
            said(Message::CopyBusy).ends_with("Click Copy again in a moment."),
            "{}",
            said(Message::CopyBusy)
        );
        assert!(
            said(Message::CopyGone {
                reference: "locale-pl/pesel-valid".to_owned()
            })
            .starts_with("locale-pl/pesel-valid is not in the pack in use any more")
        );
        assert!(
            said(Message::CopyFailed {
                detail: "the display went away".to_owned()
            })
            .ends_with("printed by: nkb emit whitespace")
        );
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
            registration(
                &ShortcutRegistration::Registered,
                HotkeyAction::NextValue,
                defaults().chord(HotkeyAction::NextValue)
            )
            .is_none(),
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

            let text = chord_text(&defaults(), action);
            assert!(!text.is_empty(), "{action:?} has no shortcut text");
            if CONVENTION == Convention::WindowsAndLinux {
                assert!(
                    text.starts_with("Alt+Shift+"),
                    "{action:?} reads as {text}, and every default is Alt+Shift+<key> (D82)"
                );
            }
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
        if CONVENTION == Convention::WindowsAndLinux {
            assert_eq!(
                chord_text(&defaults(), HotkeyAction::NextValue),
                "Alt+Shift+N"
            );
            assert_eq!(
                chord_text(&defaults(), HotkeyAction::RestartPack),
                "Alt+Shift+0"
            );
            assert_eq!(
                chord_text(&defaults(), HotkeyAction::OpenPacks),
                "Alt+Shift+Space"
            );
        }
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
        let out = message(
            &Message::CounterKept { done: 7, total: 34 },
            "unicode-text",
            &defaults(),
        );
        assert_eq!(
            out,
            format!(
                "New field - the counter is still at 7/34. Press {} to start this pack from the beginning.",
                chord_text(&defaults(), HotkeyAction::RestartPack)
            )
        );
        assert_ne!(
            chord_text(&defaults(), HotkeyAction::RestartPack),
            chord_text(&defaults(), HotkeyAction::NextValue),
            "the check above would not tell RestartPack from the first entry"
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
            &defaults(),
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
                reason: StopReason::Dropped,
            },
            Message::EndOfPack { total: 0 },
            Message::ModifierHeld { key: String::new() },
            Message::ValueTooLarge { id: String::new() },
        ] {
            let out = super::message(&message, "", &defaults());
            assert!(!out.is_empty(), "{message:?} produced nothing");
            assert!(!out.contains('{'), "{message:?} left a placeholder: {out}");
        }
    }

    #[test]
    fn every_reason_a_send_stopped_reads_as_its_own_clause() {
        // `D95`, `D96`: the clause stands between a colon and the full stop its
        // sentence brings, so it carries no full stop of its own, and two
        // reasons that read the same would tell the tester nothing.
        let clauses: Vec<&str> = [
            StopReason::Dropped,
            StopReason::FocusMoved,
            StopReason::NotTaking,
            StopReason::Escape,
        ]
        .into_iter()
        .map(stop_reason)
        .collect();
        for (i, clause) in clauses.iter().enumerate() {
            assert!(!clause.is_empty() && !clause.ends_with('.'), "{clause}");
            assert!(!clauses[i + 1..].contains(clause), "\"{clause}\" twice");
        }
        assert_eq!(
            super::message(
                &Message::Interrupted {
                    units_sent: 12480,
                    units_expected: 100_000,
                    reason: StopReason::Escape,
                },
                "",
                &defaults(),
            ),
            "Stopped after 12480 of 100000 UTF-16 units: Escape was pressed. The field holds a partial value - clear it before the next test."
        );
    }

    /// The pack window's labels divide the same way as the palette's.
    ///
    /// Exhaustive on the takes-a-value side, so a new variant must be placed
    /// before this compiles. The array can still miss one, and the contract
    /// bridge (`sprawdz-kontrakt.py`) is the second net, as for the palette.
    #[test]
    fn a_pack_window_label_carrying_a_value_has_a_typed_function() {
        for label in [
            PacksLabel::Title,
            PacksLabel::Heading,
            PacksLabel::Summary,
            PacksLabel::SummaryFiltered,
            PacksLabel::Search,
            PacksLabel::InUse,
            PacksLabel::Next,
            PacksLabel::Detail,
            PacksLabel::ValueDetail,
            PacksLabel::ValueDetailFound,
            PacksLabel::DetailFound,
            PacksLabel::PlaceId,
            PacksLabel::PlaceTags,
            PacksLabel::PlaceFields,
            PacksLabel::PlacesTwo,
            PacksLabel::PlacesThree,
            PacksLabel::ValuesIn,
            PacksLabel::FoundValues,
            PacksLabel::Recent,
            PacksLabel::PacksSection,
            PacksLabel::ValueGone,
            PacksLabel::Offensive,
            PacksLabel::Refused,
            PacksLabel::Problems,
            PacksLabel::Unreadable,
            PacksLabel::NoMatch,
            PacksLabel::NoPacks,
            PacksLabel::ListUnavailable,
            PacksLabel::NotRead,
            PacksLabel::BuiltInOnly,
            PacksLabel::SourceBuiltIn,
            PacksLabel::SourceTeam,
            PacksLabel::SourceOwn,
            PacksLabel::ReasonNotCarried,
            PacksLabel::ReasonCannotBeSet,
            PacksLabel::ReasonNotSet,
            PacksLabel::ReasonUnreadable,
            PacksLabel::KeyEnter,
            PacksLabel::UseSelected,
            PacksLabel::KeyEscape,
            PacksLabel::Close,
            PacksLabel::KeyArrows,
            PacksLabel::Move,
        ] {
            let takes_values = match label {
                PacksLabel::Summary
                | PacksLabel::SummaryFiltered
                | PacksLabel::Detail
                | PacksLabel::ValueDetail
                | PacksLabel::ValueDetailFound
                | PacksLabel::DetailFound
                | PacksLabel::PlacesTwo
                | PacksLabel::PlacesThree
                | PacksLabel::ValuesIn
                | PacksLabel::ValueGone
                | PacksLabel::Refused
                | PacksLabel::Problems
                | PacksLabel::Unreadable
                | PacksLabel::NoMatch
                | PacksLabel::NotRead => true,
                PacksLabel::Title
                | PacksLabel::Heading
                | PacksLabel::Search
                | PacksLabel::InUse
                | PacksLabel::Next
                | PacksLabel::FoundValues
                | PacksLabel::Recent
                | PacksLabel::PlaceId
                | PacksLabel::PlaceTags
                | PacksLabel::PlaceFields
                | PacksLabel::PacksSection
                | PacksLabel::Offensive
                | PacksLabel::NoPacks
                | PacksLabel::ListUnavailable
                | PacksLabel::BuiltInOnly
                | PacksLabel::SourceBuiltIn
                | PacksLabel::SourceTeam
                | PacksLabel::SourceOwn
                | PacksLabel::ReasonNotCarried
                | PacksLabel::ReasonCannotBeSet
                | PacksLabel::ReasonNotSet
                | PacksLabel::ReasonUnreadable
                | PacksLabel::KeyEnter
                | PacksLabel::UseSelected
                | PacksLabel::KeyEscape
                | PacksLabel::Close
                | PacksLabel::KeyArrows
                | PacksLabel::Move => false,
            };
            let pattern = pattern_packs_label(label);
            assert_eq!(
                pattern.contains('{'),
                takes_values,
                "{label:?}: pattern was {pattern:?}"
            );
            assert!(!pattern.is_empty(), "{label:?} produced nothing");
        }
    }

    #[test]
    fn the_pack_window_says_how_many_and_of_how_many_while_filtered() {
        assert_eq!(packs_summary(34, 9), "values: 34, packs: 9");
        assert_eq!(
            packs_summary_filtered(2, 102, 0, 9),
            "values: 2 of 102, packs: 0 of 9"
        );
        assert_eq!(
            pack_detail("unicode-text", 34, "Text that breaks", &[]),
            "unicode-text, values: 34 - Text that breaks"
        );
        assert_eq!(
            value_detail("Polish locale", 3, 12, &[]),
            "Polish locale - value 3 of 12"
        );
        assert_eq!(values_in("Whitespace"), "Values in Whitespace");
        // A value id with braces is text, like a query (`fill` does not substitute twice).
        assert_eq!(
            value_gone("{pack}", "Whitespace"),
            "Value \"{pack}\" is not in Whitespace any more, so the next value did not change."
        );
        assert_eq!(pack_problems(3), "problems: 3");
        assert_eq!(pack_refused("broken"), "broken, does not load");
        assert_eq!(pack_unreadable("gone"), "gone, cannot be read");
    }

    /// `UX-GUI-008`: a row names the parts a query was found in that it does
    /// not show - each once, in one order, and for a pack before the
    /// description the view elides.
    #[test]
    fn a_row_says_where_a_query_was_found_when_it_does_not_show_it() {
        assert_eq!(
            value_detail("Whitespace", 5, 12, &[MatchPlace::Id]),
            "Whitespace - value 5 of 12 - in its id"
        );
        assert_eq!(
            value_detail("Whitespace", 5, 12, &[MatchPlace::Tags, MatchPlace::Id]),
            "Whitespace - value 5 of 12 - in its id and tags",
            "in the order of the places, not of the caller"
        );
        assert_eq!(
            value_detail(
                "Whitespace",
                5,
                12,
                &[
                    MatchPlace::Fields,
                    MatchPlace::Tags,
                    MatchPlace::Fields,
                    MatchPlace::Id
                ]
            ),
            "Whitespace - value 5 of 12 - in its id, tags and fields",
            "each place once"
        );
        assert_eq!(
            value_detail("Whitespace", 5, 12, &[MatchPlace::Tags, MatchPlace::Tags]),
            "Whitespace - value 5 of 12 - in its tags",
            "two words in tags are one place"
        );
        assert_eq!(
            pack_detail("locale-pl", 12, "Polish data", &[MatchPlace::Tags]),
            "locale-pl, values: 12 - in its tags - Polish data"
        );
        // A description with braces stays text after the places went in.
        assert_eq!(
            pack_detail("x", 1, "{places}", &[MatchPlace::Tags]),
            "x, values: 1 - in its tags - {places}"
        );
    }

    #[test]
    fn a_query_with_braces_is_copied_not_substituted() {
        assert_eq!(
            no_match("{id}"),
            "Nothing matches \"{id}\". Press Backspace to widen the search."
        );
    }

    #[test]
    fn unread_sources_are_one_line_per_reason_and_nothing_when_all_were_read() {
        // Today's build: the built-in catalogue's own coverage.
        let today = CatalogueCoverage {
            consulted: vec![CatalogueSource::BuiltIn],
            skipped: vec![
                (CatalogueSource::Team, SourceSkipped::NotImplementedYet),
                (CatalogueSource::Own, SourceSkipped::NotImplementedYet),
            ],
        };
        // `UX-GUI-009`: the folders this version does not have, in the
        // tester's words - not as a failure to read.
        assert_eq!(
            not_read(&today),
            vec![
                "This version lists the built-in packs only - team and own pack folders are not in it yet."
            ]
        );
        // One folder missing while the other is read is not "built-in only",
        // and neither is a version whose built-in packs went unread.
        let team_only = CatalogueCoverage {
            consulted: vec![CatalogueSource::BuiltIn, CatalogueSource::Own],
            skipped: vec![(CatalogueSource::Team, SourceSkipped::NotImplementedYet)],
        };
        assert_eq!(
            not_read(&team_only),
            vec!["Not read: team folder - cannot be set in this version."]
        );
        let nothing_built_in = CatalogueCoverage {
            consulted: Vec::new(),
            ..today.clone()
        };
        assert_eq!(
            not_read(&nothing_built_in),
            vec!["Not read: team folder, own folder - cannot be set in this version."]
        );
        let complete = CatalogueCoverage {
            consulted: vec![
                CatalogueSource::BuiltIn,
                CatalogueSource::Team,
                CatalogueSource::Own,
            ],
            skipped: Vec::new(),
        };
        assert!(not_read(&complete).is_empty());
        assert_eq!(not_read(&every_skip()).len(), 4);
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
            PaletteLabel::Interrupted,
            PaletteLabel::Warnings,
            PaletteLabel::ClipboardMode,
            PaletteLabel::ClipboardForWindow,
            PaletteLabel::PreviewElided,
            PaletteLabel::NotGuaranteed,
            PaletteLabel::NotGuaranteedMore,
            PaletteLabel::Recipe,
            PaletteLabel::NoValueYet,
            PaletteLabel::ShortcutsPaused,
            PaletteLabel::ShortcutsLink,
            PaletteLabel::Typing,
            PaletteLabel::TypingCounter,
            PaletteLabel::StopTyping,
            PaletteLabel::NextValue,
            PaletteLabel::NextEndOfPack,
            PaletteLabel::LastSent,
            PaletteLabel::Copy,
            PaletteLabel::CopyNext,
            PaletteLabel::CopyLast,
            PaletteLabel::SendBy,
            PaletteLabel::RouteKeyboard,
            PaletteLabel::RouteClipboard,
            PaletteLabel::TurnOff,
            PaletteLabel::TurnOffClipboard,
            PaletteLabel::Collapse,
            PaletteLabel::Expand,
            PaletteLabel::EachValue,
            PaletteLabel::ClearLineFirst,
            PaletteLabel::InsertAtCursor,
            PaletteLabel::LastSentAction,
            PaletteLabel::SentTo,
            PaletteLabel::SentToUnnamed,
            PaletteLabel::ControlTextField,
            PaletteLabel::ControlTerminal,
            PaletteLabel::ControlUnconfirmed,
            PaletteLabel::ControlNotTextField,
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
                | PaletteLabel::Recipe
                | PaletteLabel::NoValueYet
                | PaletteLabel::TypingCounter
                | PaletteLabel::NextValue
                | PaletteLabel::NextEndOfPack
                | PaletteLabel::SentTo
                | PaletteLabel::SentToUnnamed => true,
                PaletteLabel::Title
                | PaletteLabel::Offensive
                | PaletteLabel::Cleared
                | PaletteLabel::OnClipboard
                | PaletteLabel::Interrupted
                | PaletteLabel::ClipboardMode
                | PaletteLabel::ClipboardForWindow
                | PaletteLabel::ShortcutsPaused
                | PaletteLabel::ShortcutsLink
                | PaletteLabel::Typing
                | PaletteLabel::StopTyping
                | PaletteLabel::LastSent
                | PaletteLabel::Copy
                | PaletteLabel::CopyNext
                | PaletteLabel::CopyLast
                | PaletteLabel::SendBy
                | PaletteLabel::RouteKeyboard
                | PaletteLabel::RouteClipboard
                | PaletteLabel::TurnOff
                | PaletteLabel::TurnOffClipboard
                | PaletteLabel::Collapse
                | PaletteLabel::Expand
                | PaletteLabel::EachValue
                | PaletteLabel::ClearLineFirst
                | PaletteLabel::InsertAtCursor
                | PaletteLabel::LastSentAction
                | PaletteLabel::ControlTextField
                | PaletteLabel::ControlTerminal
                | PaletteLabel::ControlUnconfirmed
                | PaletteLabel::ControlNotTextField => false,
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
            no_value_yet(nkb_core::hotkeys::DEFAULT_BINDINGS[0].1),
        ] {
            assert!(!text.is_empty(), "a label produced nothing");
            assert!(!text.contains('{'), "a label left a placeholder: {text}");
        }
        assert_eq!(counter(7, 34), "7 / 34");
        assert_eq!(
            no_value_yet(nkb_core::hotkeys::DEFAULT_BINDINGS[0].1),
            "Nothing sent yet. Put the cursor in any field and press Alt+Shift+N."
        );
        // The count after the label, so one of anything reads right in every
        // language without a plural rule (`D80`, `OBS-120`).
        assert_eq!(
            counts(1, 2, 8, 4),
            "graphemes: 1, code points: 2, bytes: 8, UTF-16 units: 4"
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
    #[test]
    fn every_sentence_about_a_shortcut_in_the_settings_file_is_whole_and_names_it() {
        let bindings = defaults();
        let chord = |text: &str| HotkeyChord::parse(text).expect("reads");
        let part = String::from;
        let reasons = [
            ShortcutUnreadable::NotText,
            ShortcutUnreadable::Grammar(ChordError::Empty),
            ShortcutUnreadable::Grammar(ChordError::EmptyPart),
            ShortcutUnreadable::Grammar(ChordError::Unknown(part("Shfit"))),
            ShortcutUnreadable::Grammar(ChordError::NoKey),
            ShortcutUnreadable::Grammar(ChordError::TwoKeys(part("P"))),
            ShortcutUnreadable::Grammar(ChordError::KeyNotLast(part("Alt"))),
            ShortcutUnreadable::Grammar(ChordError::RepeatedModifier(part("Option"))),
        ];
        for why in reasons {
            let out = settings_message(
                &SettingsMessage::Note(SettingsNote::NotAShortcut {
                    key: String::from("shortcuts.next-value"),
                    why: why.clone(),
                }),
                "f",
                &bindings,
            )
            .expect("a note is always said");
            assert!(!out.contains('{'), "{why:?} left a placeholder: {out}");
            assert!(
                out.contains("shortcuts.next-value") && out.contains(" f"),
                "{out}"
            );
        }
        let unknown = settings_message(
            &SettingsMessage::Note(SettingsNote::NotAShortcut {
                key: String::from("shortcuts.next-value"),
                why: ShortcutUnreadable::Grammar(ChordError::Unknown(part("Shfit"))),
            }),
            "f",
            &bindings,
        )
        .expect("said");
        assert!(
            unknown.contains("but \"Shfit\" is not the name"),
            "{unknown}"
        );

        // The tester's text is escaped and cut short: no second line, no
        // placeholder filled from inside it, no screen of text.
        let hostile = format!("a\nb\"{{key}}{}", "x".repeat(100));
        let out = settings_message(
            &SettingsMessage::Note(SettingsNote::NotAShortcut {
                key: String::from("shortcuts.next-value"),
                why: ShortcutUnreadable::Grammar(ChordError::Unknown(hostile)),
            }),
            "f",
            &bindings,
        )
        .expect("said");
        assert!(!out.contains('\n'), "{out}");
        assert!(out.contains("a\\nb\\\"{key}"), "{out}");
        assert!(out.contains("...\" is not"), "{out}");
        assert!(out.len() < 300, "{out}");

        for (why, named) in [
            (
                Refusal::Problem(ChordProblem::NoCommandModifier),
                "no Ctrl, Alt or Win",
            ),
            (
                Refusal::SameAs(HotkeyAction::PreviousValue),
                "\"Previous value\"",
            ),
            (
                Refusal::TypesCharacter('\u{105}'),
                "types \"\u{105}\" as AltGr",
            ),
            // A quote from a layout cannot close the quotes around it.
            (Refusal::TypesCharacter('"'), "types \"U+0022\" as AltGr"),
            // A combining mark is named, not drawn onto the quote before it.
            (
                Refusal::TypesCharacter('\u{301}'),
                "types \"U+0301\" as AltGr",
            ),
        ] {
            let out = settings_message(
                &SettingsMessage::ShortcutNotUsed(Refused {
                    action: HotkeyAction::NextValue,
                    chord: chord("Shift+M"),
                    why,
                }),
                "f",
                &bindings,
            )
            .expect("said");
            assert!(!out.contains('{'), "{out}");
            assert!(out.contains("\"Next value\" the shortcut Shift+M"), "{out}");
            assert!(out.contains(named), "{out}");
            assert!(
                out.contains(&chord_text(&bindings, HotkeyAction::NextValue)),
                "the default in use is named: {out}"
            );
        }
    }

    #[test]
    fn a_shortcut_set_in_the_settings_file_is_the_one_every_sentence_names() {
        let (mine, refused) = defaults().with(
            &[
                (
                    HotkeyAction::RestartPack,
                    HotkeyChord::parse("Ctrl+Alt+Win+9").expect("reads"),
                ),
                (
                    HotkeyAction::OpenPacks,
                    HotkeyChord::parse("Ctrl+Alt+Win+F7").expect("reads"),
                ),
            ],
            &|_| None,
        );
        assert!(refused.is_empty());
        let out = message(&Message::CounterKept { done: 1, total: 2 }, "p", &mine);
        assert!(out.contains("Ctrl+Alt+Win+9"), "{out}");
        // `UX-GUI-014`: no pack sends the tester to the value window's shortcut.
        assert_eq!(
            message(&Message::NoPack, "", &mine),
            "No pack is chosen, so there is nothing to send. Press Ctrl+Alt+Win+F7 to choose one."
        );
        let taken = registration(
            &ShortcutRegistration::Taken,
            HotkeyAction::RestartPack,
            mine.chord(HotkeyAction::RestartPack),
        )
        .expect("said");
        assert!(
            taken.starts_with("Ctrl+Alt+Win+9 is already taken"),
            "{taken}"
        );
        assert!(
            !taken.contains("cannot change"),
            "the file can change it now: {taken}"
        );
    }

    // ---- the shortcuts window (K5.5) -----------------------------------------

    fn parsed(text: &str) -> HotkeyChord {
        HotkeyChord::parse(text).expect("a chord the test writes reads")
    }

    #[test]
    fn a_shortcuts_label_carrying_a_value_has_a_typed_function_and_one_without_does_not() {
        // The array below can drift from the enum, and the bridge to
        // `ux-spec.md` 6 L is the second net, as for the other windows.
        for label in [
            ShortcutsLabel::Title,
            ShortcutsLabel::Heading,
            ShortcutsLabel::Summary,
            ShortcutsLabel::Intro,
            ShortcutsLabel::Recording,
            ShortcutsLabel::Changed,
            ShortcutsLabel::Taken,
            ShortcutsLabel::NotRegistered,
            ShortcutsLabel::NotAvailable,
            ShortcutsLabel::KeyEnter,
            ShortcutsLabel::Record,
            ShortcutsLabel::KeyDelete,
            ShortcutsLabel::Restore,
            ShortcutsLabel::KeyArrows,
            ShortcutsLabel::Move,
            ShortcutsLabel::KeyEscape,
            ShortcutsLabel::Close,
            ShortcutsLabel::Invite,
            ShortcutsLabel::Recorded,
            ShortcutsLabel::Restored,
            ShortcutsLabel::AlreadySo,
            ShortcutsLabel::Also,
            ShortcutsLabel::Unchecked,
            ShortcutsLabel::NoModifier,
            ShortcutsLabel::TypesCharacter,
            ShortcutsLabel::HeldBy,
            ShortcutsLabel::TakenElsewhere,
            ShortcutsLabel::NotRegisteredCode,
            ShortcutsLabel::NotAKey,
            ShortcutsLabel::SeveralKeys,
            ShortcutsLabel::CannotTell,
        ] {
            let takes_values = match label {
                ShortcutsLabel::Summary
                | ShortcutsLabel::Invite
                | ShortcutsLabel::Recorded
                | ShortcutsLabel::Restored
                | ShortcutsLabel::AlreadySo
                | ShortcutsLabel::Also
                | ShortcutsLabel::Unchecked
                | ShortcutsLabel::NoModifier
                | ShortcutsLabel::TypesCharacter
                | ShortcutsLabel::HeldBy
                | ShortcutsLabel::TakenElsewhere
                | ShortcutsLabel::NotRegisteredCode => true,
                ShortcutsLabel::Title
                | ShortcutsLabel::Heading
                | ShortcutsLabel::Intro
                | ShortcutsLabel::Recording
                | ShortcutsLabel::Changed
                | ShortcutsLabel::Taken
                | ShortcutsLabel::NotRegistered
                | ShortcutsLabel::NotAvailable
                | ShortcutsLabel::KeyEnter
                | ShortcutsLabel::Record
                | ShortcutsLabel::KeyDelete
                | ShortcutsLabel::Restore
                | ShortcutsLabel::KeyArrows
                | ShortcutsLabel::Move
                | ShortcutsLabel::KeyEscape
                | ShortcutsLabel::Close
                | ShortcutsLabel::NotAKey
                | ShortcutsLabel::SeveralKeys
                | ShortcutsLabel::CannotTell => false,
            };
            let pattern = pattern_shortcuts_label(label);
            assert_eq!(
                pattern.contains('{'),
                takes_values,
                "{label:?}: pattern was {pattern:?}"
            );
            assert!(!pattern.is_empty(), "{label:?} produced nothing");
        }
    }

    #[test]
    fn a_change_is_told_with_the_action_the_shortcut_and_every_other_that_moved() {
        let recorded = shortcut_answer(
            HotkeyAction::NextValue,
            Some(parsed("Alt+Shift+M")),
            &ShortcutChange::Changed {
                bindings: Bindings::defaults(CONVENTION)
                    .with(
                        &[
                            (HotkeyAction::NextValue, parsed("Alt+Shift+M")),
                            (HotkeyAction::PreviousValue, parsed("Alt+Shift+N")),
                        ],
                        &|_| None,
                    )
                    .0,
                also: vec![HotkeyAction::PreviousValue],
                unchecked: Some(ShortcutsUnavailable::CouldNotStart),
            },
            parsed("Alt+Shift+N"),
        );
        let next = action_name(HotkeyAction::NextValue);
        let previous = action_name(HotkeyAction::PreviousValue);
        let m = chord(parsed("Alt+Shift+M"));
        let n = chord(parsed("Alt+Shift+N"));
        assert_eq!(
            recorded,
            vec![
                format!("\"{next}\" answers {m} from when this window closes."),
                format!("\"{previous}\" moves to {n}, as the settings file asks."),
                format!(
                    "Whether another application holds {m} could not be checked: the shortcut \
                     listener could not be started."
                ),
            ]
        );
    }

    #[test]
    fn a_restore_already_so_and_every_refusal_have_their_own_words() {
        let n = parsed("Alt+Shift+N");
        let shown = chord(n);
        let next = action_name(HotkeyAction::NextValue);
        let repeat = action_name(HotkeyAction::RepeatLast);
        let restored = shortcut_answer(
            HotkeyAction::NextValue,
            None,
            &ShortcutChange::Changed {
                bindings: Bindings::defaults(CONVENTION),
                also: Vec::new(),
                unchecked: None,
            },
            n,
        );
        assert_eq!(
            restored,
            vec![format!(
                "\"{next}\" answers its default {} again from when this window closes.",
                chord(Bindings::defaults(CONVENTION).chord(HotkeyAction::NextValue))
            )]
        );
        assert_eq!(
            shortcut_answer(
                HotkeyAction::NextValue,
                Some(n),
                &ShortcutChange::AlreadySo,
                n
            ),
            vec![format!("\"{next}\" already answers {shown}.")]
        );
        let refused = |why| {
            shortcut_answer(
                HotkeyAction::NextValue,
                Some(n),
                &ShortcutChange::Refused(Refused {
                    action: HotkeyAction::NextValue,
                    chord: n,
                    why,
                }),
                n,
            )
            .join(" ")
        };
        assert!(
            refused(Refusal::Problem(ChordProblem::NoCommandModifier))
                .contains("no Ctrl, Alt or Win")
        );
        assert!(refused(Refusal::TypesCharacter('\u{105}')).contains("types \"\u{105}\" as AltGr"));
        assert_eq!(
            refused(Refusal::SameAs(HotkeyAction::RepeatLast)),
            format!(
                "{shown} is already the shortcut for \"{repeat}\". Give \"{repeat}\" another \
                 combination first."
            )
        );
        assert_eq!(
            shortcut_answer(
                HotkeyAction::NextValue,
                Some(n),
                &ShortcutChange::Taken { chord: n },
                n
            ),
            vec![format!(
                "{shown} is taken by another application, so it was not kept. Close that \
                 application or choose another combination."
            )]
        );
        assert_eq!(
            shortcut_answer(
                HotkeyAction::NextValue,
                Some(n),
                &ShortcutChange::NotRegistered {
                    chord: n,
                    code: 1409
                },
                n
            ),
            vec![format!(
                "{shown} could not be registered - the system returned code 1409 - so it was \
                 not kept."
            )]
        );
    }

    #[test]
    fn the_shortcuts_window_counts_and_invites_in_its_own_words() {
        assert_eq!(shortcuts_summary(1, 10), "changed: 1 of 10");
        assert_eq!(
            shortcut_invite(HotkeyAction::CopyReport),
            format!(
                "Press the new shortcut for \"{}\", or Esc to stop recording.",
                action_name(HotkeyAction::CopyReport)
            )
        );
    }

    #[test]
    fn the_line_under_the_last_value_names_the_program_and_the_kind_of_control() {
        let at = |program: Option<&str>, field| Target {
            program: program.map(ToOwned::to_owned),
            field,
        };
        assert_eq!(
            sent_to(&at(Some("chrome.exe"), ControlKind::TextField)),
            "to chrome.exe, a text field"
        );
        assert_eq!(
            sent_to(&at(Some("WindowsTerminal.exe"), ControlKind::Terminal)),
            "to WindowsTerminal.exe, a terminal"
        );
        assert_eq!(
            sent_to(&at(None, ControlKind::Unconfirmed)),
            "to a program that did not give its name, a control not confirmed as a field"
        );
        assert_eq!(
            sent_to(&at(Some("x.exe"), ControlKind::NotTextField)),
            "to x.exe, not a text field"
        );
    }

    #[test]
    fn a_program_name_from_the_system_cannot_break_the_line_or_push_the_palette_off_the_screen() {
        let line = sent_to(&Target {
            program: Some("a\nb {control}".to_owned()),
            field: ControlKind::TextField,
        });
        assert_eq!(line, "to a\\nb {control}, a text field");
        let long = sent_to(&Target {
            program: Some("x".repeat(PROGRAM_SHOWN + 1)),
            field: ControlKind::TextField,
        });
        assert_eq!(
            long,
            format!("to {}..., a text field", "x".repeat(PROGRAM_SHOWN))
        );
    }
}
