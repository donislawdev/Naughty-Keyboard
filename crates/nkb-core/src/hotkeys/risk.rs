//! What a chord takes from every application while the palette holds it as a
//! global shortcut, and the one rule that says it (`D189`): a key, when the chord
//! holds no `Ctrl`, `Alt` or `Win`, or the character a keyboard layout of the
//! system types with it. The settings file and the shortcuts window both ask
//! [`Bindings::risk`], and both say the answer as a warning - the chord is used.

use super::{Bindings, Convention, HotkeyAction, HotkeyChord};

/// What a chord takes from every application while the palette holds it as a
/// global shortcut ([`Bindings::risk`]).
///
/// Said as a warning, never as a refusal (`D189`, the owner's choice of
/// 2026-11-16): a tester may want a single key or a combination their own
/// keyboard types a character with, and only they know the applications they
/// test. A shortcut is refused only when it cannot work at all - another
/// action or another program holds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChordRisk {
    /// No `Ctrl`, `Alt` or `Win` - see [`HotkeyChord::holds_command_modifier`].
    NoCommandModifier,
    /// A `Ctrl+Alt` chord that is `AltGr` for this character on a layout the
    /// system has - while the palette holds it, the character cannot be typed
    /// in any application.
    TypesCharacter(char),
    /// A chord with `Option` that types this character on a layout the system
    /// has (`D141`) - the same loss, on macOS.
    OptionTypesCharacter(char),
}

/// A chord in use that takes something from every application - said in the
/// colour of a risk, where the chord is named (`D189`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Risky {
    pub action: HotkeyAction,
    pub chord: HotkeyChord,
    pub risk: ChordRisk,
}

impl Bindings {
    /// What `chord` takes from every application as a global shortcut, if
    /// anything: a key, when it holds no `Ctrl`, `Alt` or `Win`, or the
    /// character a keyboard layout of this system types with it
    /// ([`Bindings::typing_risk`]). The one place this rule stands, for the
    /// settings file and for the window that records a chord. A risk is said,
    /// and the chord is used all the same (`D189`).
    #[must_use]
    pub fn risk(
        &self,
        chord: HotkeyChord,
        types: &dyn Fn(HotkeyChord) -> Option<char>,
    ) -> Option<ChordRisk> {
        if !chord.holds_command_modifier() {
            return Some(ChordRisk::NoCommandModifier);
        }
        self.typing_risk(chord, types)
    }

    /// Every action whose chord in these bindings takes something from every
    /// application, with what it takes, in the order of [`HotkeyAction::ALL`].
    #[must_use]
    pub fn risky(&self, types: &dyn Fn(HotkeyChord) -> Option<char>) -> Vec<Risky> {
        self.table
            .iter()
            .filter_map(|(action, chord)| {
                self.risk(*chord, types).map(|risk| Risky {
                    action: *action,
                    chord: *chord,
                    risk,
                })
            })
            .collect()
    }

    /// The character a keyboard layout of this system makes `chord` type, as a
    /// risk, if it does: taking the chord would take that character from every
    /// application.
    ///
    /// `types` is the system's answer (the core knows no layout) and it is
    /// asked ONLY for a chord the convention says can type ([`Convention::can_type`]):
    /// an `Alt+Shift` chord is never `AltGr` whatever a layout does with
    /// `Ctrl+Alt` on its key, and a `Cmd+Option` chord types nothing. `None`
    /// from `types` means "no risk to name", so a risk is named on a character
    /// the answer can name, never on a guess.
    #[must_use]
    pub fn typing_risk(
        &self,
        chord: HotkeyChord,
        types: &dyn Fn(HotkeyChord) -> Option<char>,
    ) -> Option<ChordRisk> {
        let may_type = self.convention.can_type(&chord);
        let character = may_type.then(|| types(chord)).flatten()?;
        Some(match self.convention {
            // Linux never gets here: `can_type` is false there, so `character` is
            // `None`. The arm names the risk that would be true of `Ctrl+Alt`.
            Convention::Windows | Convention::Linux => ChordRisk::TypesCharacter(character),
            Convention::MacOs => ChordRisk::OptionTypesCharacter(character),
        })
    }
}
