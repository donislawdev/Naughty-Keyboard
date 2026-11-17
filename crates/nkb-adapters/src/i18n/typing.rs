//! The sentences about the keys of a shortcut that depend on the keyboard in
//! front of the tester: a chord a keyboard layout types a character on
//! (`D88`, `D141`), where the key that types is `AltGr` on Windows and `Option`
//! on a Mac, and a chord that holds no modifier at all (`OBS-196`), where the
//! modifiers have other names on a Mac. Both are warnings: the chord is used,
//! and the tester is told what it takes from every application (`D189`).
//!
//! The sentence of the shortcuts window for the same risks is a
//! `ShortcutsLabel` (`windows.rs`), which says the same of a chord being
//! recorded.

use nkb_core::ChordRisk;
use nkb_core::hotkeys::Convention;

use crate::shortcuts::CONVENTION;

/// The sentence the palette says at start about a shortcut of the settings file
/// that is in use and takes something from every application. `{file}`,
/// `{action}`, `{wanted}`, `{character}` and `{modifiers}` are filled by the caller.
pub(super) fn pattern_file_risk(risk: ChordRisk) -> &'static str {
    match risk {
        ChordRisk::NoCommandModifier => {
            "The settings file {file} gives \"{action}\" the shortcut {wanted}. Without {modifiers}, it stops working in every application while the palette runs, the palette's own windows included."
        }
        ChordRisk::TypesCharacter(_) => {
            "The settings file {file} gives \"{action}\" the shortcut {wanted}. On a keyboard layout of this computer it types \"{character}\" as AltGr, so while the palette runs, \"{character}\" cannot be typed that way."
        }
        ChordRisk::OptionTypesCharacter(_) => {
            "The settings file {file} gives \"{action}\" the shortcut {wanted}. On a keyboard layout of this computer it types \"{character}\" with Option, so while the palette runs, \"{character}\" cannot be typed that way."
        }
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
#[allow(
    clippy::expect_used,
    reason = "a failed expectation in a test is a failed test"
)]
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

    /// The settings file is told what a shortcut in use takes from every application, in the
    /// words of the key of this system (`D189`). The shortcut in use is the one asked for.
    #[test]
    fn the_settings_file_is_told_what_a_shortcut_in_use_takes() {
        use nkb_app::keep_settings::SettingsMessage;
        use nkb_core::hotkeys::{Bindings, HotkeyAction, HotkeyChord, Risky};
        let bindings = Bindings::defaults(CONVENTION);
        let chord = |text: &str| HotkeyChord::parse(text).expect("reads");
        for (risk, named) in [
            // The modifiers are named by the keyboard of the system (`OBS-196`).
            (
                ChordRisk::NoCommandModifier,
                format!("Without {}, it stops working", command_modifiers()).as_str(),
            ),
            (
                ChordRisk::TypesCharacter('\u{105}'),
                "types \"\u{105}\" as AltGr",
            ),
            // A quote from a layout cannot close the quotes around it.
            (ChordRisk::TypesCharacter('"'), "types \"U+0022\" as AltGr"),
            // A combining mark is named, not drawn onto the quote before it.
            (
                ChordRisk::TypesCharacter('\u{301}'),
                "types \"U+0301\" as AltGr",
            ),
            // On the Mac the key is named Option.
            (
                ChordRisk::OptionTypesCharacter('\u{105}'),
                "types \"\u{105}\" with Option",
            ),
            (
                ChordRisk::OptionTypesCharacter('\u{301}'),
                "types \"U+0301\" with Option",
            ),
        ] {
            let out = super::super::settings_message(
                &SettingsMessage::ShortcutRisky(Risky {
                    action: HotkeyAction::NextValue,
                    chord: chord("Shift+M"),
                    risk,
                }),
                "f",
                &bindings,
            )
            .expect("said");
            assert!(!out.contains('{'), "{out}");
            assert!(
                out.contains("\"Type the value shown\" the shortcut Shift+M"),
                "{out}"
            );
            assert!(out.contains(named), "{out}");
            // The shortcut in use is the one asked for, not the default.
            assert!(
                !out.contains(&super::super::chord_text(
                    &bindings,
                    HotkeyAction::NextValue
                )),
                "no default is named: {out}"
            );
        }
    }
}
