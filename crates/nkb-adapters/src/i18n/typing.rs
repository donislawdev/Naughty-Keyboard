//! The sentence for a shortcut that a keyboard layout types a character on
//! (`D88`, `D141`): the key that types is `AltGr` on Windows and `Option` on a
//! Mac, and the way out differs with it, so the two sentences stand side by side
//! and say it in the words of their own keyboard.
//!
//! The sentence of the shortcuts window for the same refusals is a
//! `ShortcutsLabel` (`windows.rs`), which says the same of a chord being
//! recorded.

use nkb_core::Refusal;

/// The sentence the settings file gets for a refusal about typing, or `None` for
/// a refusal that is not one. `{file}`, `{action}`, `{wanted}`, `{character}` and
/// `{shortcut}` are filled by the caller.
pub(super) fn pattern_typing_refusal(why: Refusal) -> Option<&'static str> {
    match why {
        Refusal::TypesCharacter(_) => Some(
            "The settings file {file} gives \"{action}\" the shortcut {wanted}, which types \"{character}\" as AltGr on a keyboard layout of this computer - taken globally it would stop that character working in every application, so the default {shortcut} is used. Choose a combination without Ctrl+Alt.",
        ),
        Refusal::OptionTypesCharacter(_) => Some(
            "The settings file {file} gives \"{action}\" the shortcut {wanted}, which types \"{character}\" with Option on a keyboard layout of this computer - taken globally it would stop that character working in every application, so the default {shortcut} is used. Choose a combination that also holds Command.",
        ),
        Refusal::Problem(_) | Refusal::SameAs(_) => None,
    }
}
