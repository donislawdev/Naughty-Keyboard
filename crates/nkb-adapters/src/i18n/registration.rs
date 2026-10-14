//! What the palette says about holding its shortcuts: one registration,
//! a whole one, having none at all, and the listener going away. A part of
//! the `i18n` module, kept in a file of its own as the windows' words are -
//! see the module above for why every one of them is a key.

use super::*;

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

/// What a whole registration came to, when one sentence says it better than
/// a sentence for each shortcut. A key with one variant for the reason
/// [`Startup`] gives: there is no type to match on, and the bridge in
/// `tools/sprawdz-kontrakt.py` reads a sentence by its variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RegistrationSummary {
    /// Every shortcut is held by somebody else - what a second palette meets.
    AllTaken,
}

/// The pattern for a whole registration. `ux-spec.md` 6, section E.
///
/// The likely holder is named, because a set taken to the last chord is what
/// a palette already running does to the next one (`OBS-176`), and the remedy
/// is closing it - not looking for the application the `Taken` sentence
/// sends the tester after.
fn pattern_registration_summary(summary: RegistrationSummary) -> &'static str {
    match summary {
        RegistrationSummary::AllTaken => {
            "Every shortcut is already taken by another application, so none of them will respond. If another Naughty Keyboard palette is open, close it and restart this one, or choose other combinations under \"{link}\"."
        }
    }
}

/// What the palette says about one whole registration: one sentence when
/// every shortcut is taken, else a sentence for each that did not take.
///
/// Fourteen `Taken` sentences naming one cause would bury the only thing worth
/// knowing, so they become one. A set where only some are taken keeps a
/// sentence each: the chords differ, and so may the holders. `registered`
/// pairs each action with the chord registered for it, as the palette keeps it.
#[must_use]
pub fn registrations(
    registered: &[(HotkeyAction, HotkeyChord, ShortcutRegistration)],
) -> Vec<String> {
    let all_taken = !registered.is_empty()
        && registered
            .iter()
            .all(|(.., outcome)| *outcome == ShortcutRegistration::Taken);
    if all_taken {
        let pattern = pattern_registration_summary(RegistrationSummary::AllTaken);
        let link = pattern_palette_label(PaletteLabel::ShortcutsLink);
        return vec![fill(pattern, &[("link", link)])];
    }
    registered
        .iter()
        .filter_map(|(action, chord, outcome)| registration(outcome, *action, *chord))
        .collect()
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
