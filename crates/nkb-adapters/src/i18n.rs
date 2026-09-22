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
fn fill(pattern: &str, values: &[(&str, &str)]) -> String {
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
/// 🔴 `Message::Degraded` is the one sentence here that is DELIBERATELY not the
/// document's target text. The target says "switching to clipboard mode. Press
/// your paste shortcut" - and clipboard mode does not exist: measured on the
/// whole tree, the word appears in comments and in the `degraded` state of the
/// machine, with no port, no adapter and nothing writing to a clipboard. A
/// tester following that sentence would press paste and paste whatever was in
/// the clipboard before. The target text goes in WITH the port, and the document
/// carries both texts so the swap is not a rediscovery.
fn pattern_message(message: &Message) -> &'static str {
    match message {
        Message::Degraded => {
            "Nothing reached the field. This app does not accept simulated input, so values cannot be sent here - try another field or application."
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
        Message::Degraded | Message::NoTarget | Message::NoPack | Message::ClearingFailed => {
            pattern.to_owned()
        }
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
            Message::Degraded,
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

    #[test]
    fn every_message_variant_is_listed_here() {
        // The compiler guarantees `pattern_message` covers every variant. What
        // it cannot guarantee is that the LIST above grew with it, and a content
        // check over a stale list passes for the wrong reason. Counting is the
        // cheapest honest link: the number below changes in the same commit as
        // the variant.
        assert_eq!(
            every_message().len(),
            11,
            "a Message variant was added or removed - add it to every_message() too"
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
}
