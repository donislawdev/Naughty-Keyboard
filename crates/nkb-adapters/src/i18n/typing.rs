//! The sentences about the keys of a shortcut that depend on the keyboard in
//! front of the tester: a chord a keyboard layout types a character on
//! (`D88`, `D141`), where the key that types is `AltGr` on Windows and `Option`
//! on a Mac and the way out differs with it, and a chord that holds no modifier
//! at all (`OBS-196`), where the modifiers have other names on a Mac.
//!
//! The sentence of the shortcuts window for the same refusals is a
//! `ShortcutsLabel` (`windows.rs`), which says the same of a chord being
//! recorded.

use nkb_core::Refusal;
use nkb_core::hotkeys::Convention;

use crate::shortcuts::CONVENTION;

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

/// The modifiers a global shortcut has to hold one of, named as the keyboard of
/// this system names them - what fills `{modifiers}` in the two sentences about a
/// chord with none. Windows prints `Ctrl`, `Alt` and `Win`, Linux `Ctrl`, `Alt`
/// and `Super`, a Mac `Control`, `Option` and `Command` (`OBS-196`: the Windows
/// words were said to a Mac, which has no key called either `Alt` or `Win`).
pub(super) const fn command_modifiers() -> &'static str {
    match CONVENTION {
        Convention::Windows => "Ctrl, Alt or Win",
        Convention::Linux => "Ctrl, Alt or Super",
        Convention::MacOs => "Control, Option or Command",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_modifiers_are_named_by_the_keyboard_of_the_system() {
        let named = command_modifiers();
        match CONVENTION {
            Convention::Windows => assert_eq!(named, "Ctrl, Alt or Win"),
            Convention::Linux => assert_eq!(named, "Ctrl, Alt or Super"),
            Convention::MacOs => assert_eq!(named, "Control, Option or Command"),
        }
        // Whatever the system, every word is one the chord text prints, so the
        // sentence and the shortcut written beside it agree.
        let chord = nkb_core::hotkeys::HotkeyChord {
            ctrl: true,
            alt: true,
            shift: false,
            win: true,
            key: nkb_core::hotkeys::HotkeyKey::N,
        };
        let text = chord.display(CONVENTION);
        for word in named.split(", ").flat_map(|part| part.split(" or ")) {
            assert!(text.contains(word), "{word} is not in {text}");
        }
    }
}
