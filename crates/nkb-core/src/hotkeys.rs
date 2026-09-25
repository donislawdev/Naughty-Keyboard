//! The global shortcuts the palette listens for, as data.
//!
//! `ux-spec.md` 3 fixes ten global actions and a default combination for each.
//! Those combinations are DATA, in one place, the same way the clearing recipe
//! in [`crate::keys`] is - so that the palette, the registrar and the hint bar
//! all read one source rather than three that drift.
//!
//! # Platform-neutral on purpose
//!
//! A key here is a LOGICAL key, not an operating-system virtual-key code. The
//! mapping to a `VK_*` (or a macOS key code) belongs to the layer that talks to
//! the system, exactly as `chord_for` in the keyboard adapter maps the clearing
//! keys. This module knows nothing of any system, which is what keeps it in the
//! core.
//!
//! # Why the family is `Alt+Shift` and not `Ctrl+Alt`
//!
//! Until `D82` every default was `Ctrl+Alt+<key>`, as `ux-spec.md` 3 first
//! proposed. On Windows `Ctrl+Alt` IS `AltGr`, and a global shortcut matches the
//! modifiers exactly, so `Ctrl+Alt+N` swallowed `AltGr+N` - the letter `ń` on the
//! Polish layout - in every application for as long as the palette ran. Measured
//! on the owner's machine (layout `0415`) with the system's own layout table:
//! `Ctrl+Alt` types a character on ten letters there, and `Ctrl+Alt+Space` was
//! held by another application. Microsoft's keyboard guidelines say the same in
//! one line: do not use `Ctrl+Alt` combinations.
//!
//! `Alt+Shift` cannot collide with `AltGr` on ANY layout, because `AltGr` adds
//! `Ctrl` and the combination then no longer matches. Measured free of other
//! applications on the same machine. What it still costs is named in `D82`.
//!
//! # Two conventions, both as data
//!
//! macOS keeps `Cmd+Alt+<key>` (`ux-spec.md` 3): there `Option` types
//! characters and `Command` does not. Until `D82` the macOS table did not exist
//! and the registering adapter swapped `ctrl` for the Command bit on every
//! chord. That worked for a table whose every entry held `ctrl`, and it would
//! have rewritten a chord the tester chose. So both tables live here, and the
//! layer that knows the system picks one - this crate has no `cfg` and keeps
//! none.

/// One of the ten global actions the palette answers to.
///
/// Only a subset is wired to the sequence in the first pass of step 3 - moving
/// through a pack. The rest name shortcuts the later steps fill in (marking a
/// result, copying the report block, opening the pack search), and they are
/// here now so the combinations can be RESERVED at registration rather than
/// left free to be taken by something else in the meantime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HotkeyAction {
    /// Clear the field and send the next value in the pack.
    NextValue,
    /// Send the previous value.
    PreviousValue,
    /// Send the current value again.
    RepeatLast,
    /// Return to the first value of the pack.
    RestartPack,
    /// Copy the bug-report block for the last value to the clipboard.
    CopyReport,
    /// Mark the last result as working.
    MarkOk,
    /// Mark the last result as a problem.
    MarkProblem,
    /// Mark the last result as suspect.
    MarkSuspect,
    /// Open the pack search.
    OpenPacks,
    /// Collapse the palette to its pack band, or expand it again - never
    /// `hide()`, `OBS-80`. The tester's choice, never a timer's (`D83`).
    ToggleVisibility,
}

impl HotkeyAction {
    /// Every action, so a test can state the size of the set and check that the
    /// default table covers it. Rust has no way to enumerate variants, so this
    /// list is the source of truth and the test guards that it stays complete.
    pub const ALL: [HotkeyAction; 10] = [
        HotkeyAction::NextValue,
        HotkeyAction::PreviousValue,
        HotkeyAction::RepeatLast,
        HotkeyAction::RestartPack,
        HotkeyAction::CopyReport,
        HotkeyAction::MarkOk,
        HotkeyAction::MarkProblem,
        HotkeyAction::MarkSuspect,
        HotkeyAction::OpenPacks,
        HotkeyAction::ToggleVisibility,
    ];
}

/// A logical key that a default shortcut uses.
///
/// Narrow on purpose, the way [`crate::keys::Key`] is: it holds exactly the keys
/// the `ux-spec.md` 3 defaults need and no more. When configurable shortcuts
/// arrive (they need `SettingsStore`, which does not exist yet) this widens, and
/// widening it is a visible change to a type whose documentation says why it is
/// narrow - not a quiet edit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HotkeyKey {
    N,
    P,
    R,
    B,
    H,
    Space,
    Digit0,
    Digit1,
    Digit2,
    Digit3,
}

/// A shortcut: its modifiers and its key.
///
/// `win` is the Windows/Command key. The defaults do not use it, but the type
/// can express it so that a configured shortcut - and the macOS convention the
/// adapter builds - has somewhere to live without widening later.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HotkeyChord {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub win: bool,
    pub key: HotkeyKey,
}

impl HotkeyChord {
    /// The `Alt+Shift+<key>` shape every default uses on Windows and Linux.
    const fn alt_shift(key: HotkeyKey) -> Self {
        Self {
            ctrl: false,
            alt: true,
            shift: true,
            win: false,
            key,
        }
    }

    /// The `Cmd+Alt+<key>` shape every default uses on macOS. `win` is the
    /// Command key there.
    const fn command_alt(key: HotkeyKey) -> Self {
        Self {
            ctrl: false,
            alt: true,
            shift: false,
            win: true,
            key,
        }
    }
}

/// Which set of default shortcuts a system follows.
///
/// Chosen by the layer that knows the system. The core only holds the tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Convention {
    /// `Alt+Shift+<key>` - Windows and Linux.
    WindowsAndLinux,
    /// `Cmd+Alt+<key>` - macOS.
    MacOs,
}

/// The key each action is bound to by default, the same on every system.
///
/// One entry per action, in the order of [`HotkeyAction::ALL`]. Only the
/// modifiers differ between the conventions, so a tester who moves between
/// systems keeps the letters. The letters are the ones `ux-spec.md` 3 chose, so
/// the move from `Ctrl+Alt` changed the family and nothing else.
const DEFAULT_KEYS: [(HotkeyAction, HotkeyKey); 10] = [
    (HotkeyAction::NextValue, HotkeyKey::N),
    (HotkeyAction::PreviousValue, HotkeyKey::P),
    (HotkeyAction::RepeatLast, HotkeyKey::R),
    (HotkeyAction::RestartPack, HotkeyKey::Digit0),
    (HotkeyAction::CopyReport, HotkeyKey::B),
    (HotkeyAction::MarkOk, HotkeyKey::Digit1),
    (HotkeyAction::MarkProblem, HotkeyKey::Digit2),
    (HotkeyAction::MarkSuspect, HotkeyKey::Digit3),
    (HotkeyAction::OpenPacks, HotkeyKey::Space),
    (HotkeyAction::ToggleVisibility, HotkeyKey::H),
];

/// One convention's table, built from [`DEFAULT_KEYS`] so the two cannot
/// disagree about which letter belongs to which action.
const fn table(convention: Convention) -> [(HotkeyAction, HotkeyChord); 10] {
    let mut out = [(
        HotkeyAction::NextValue,
        HotkeyChord::alt_shift(HotkeyKey::N),
    ); 10];
    let mut index = 0;
    while index < DEFAULT_KEYS.len() {
        let (action, key) = DEFAULT_KEYS[index];
        let chord = match convention {
            Convention::WindowsAndLinux => HotkeyChord::alt_shift(key),
            Convention::MacOs => HotkeyChord::command_alt(key),
        };
        out[index] = (action, chord);
        index += 1;
    }
    out
}

/// The default shortcut for every action, in the Windows and Linux convention.
///
/// Every default is `Alt+Shift+<key>`: `ux-spec.md` 3 keeps them one family so
/// a tester learns them as a set rather than as ten separate things. The table
/// is checked before any code depends on it - every action present exactly once,
/// and no two actions on the same combination - because a duplicate combination
/// would be a registration that reports the second action as `Taken` against the
/// first.
pub const DEFAULT_BINDINGS: [(HotkeyAction, HotkeyChord); 10] = table(Convention::WindowsAndLinux);

/// The same table in the macOS convention, `Cmd+Alt+<key>`.
pub const MACOS_DEFAULT_BINDINGS: [(HotkeyAction, HotkeyChord); 10] = table(Convention::MacOs);

/// The default table a convention uses.
#[must_use]
pub const fn default_bindings(
    convention: Convention,
) -> &'static [(HotkeyAction, HotkeyChord); 10] {
    match convention {
        Convention::WindowsAndLinux => &DEFAULT_BINDINGS,
        Convention::MacOs => &MACOS_DEFAULT_BINDINGS,
    }
}

/// The default chord for one action, if the default table names it.
///
/// Total over [`HotkeyAction::ALL`] by the test below, so the `Option` is really
/// only `None` for a hypothetical action added without a default - which the
/// test refuses.
#[must_use]
pub fn default_chord(convention: Convention, action: HotkeyAction) -> Option<HotkeyChord> {
    default_bindings(convention)
        .iter()
        .find_map(|(candidate, chord)| (*candidate == action).then_some(*chord))
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;

    const CONVENTIONS: [Convention; 2] = [Convention::WindowsAndLinux, Convention::MacOs];

    #[test]
    fn every_action_has_exactly_one_default() {
        // The registrar reserves a combination per action, so a missing default
        // is an action that can never be pressed, and a doubled one is a table
        // that disagrees with itself.
        for convention in CONVENTIONS {
            let table = default_bindings(convention);
            for action in HotkeyAction::ALL {
                let count = table
                    .iter()
                    .filter(|(candidate, _)| *candidate == action)
                    .count();
                assert_eq!(
                    count, 1,
                    "{convention:?}: {action:?} must have exactly one default binding"
                );
            }
        }
    }

    #[test]
    fn no_two_actions_share_a_combination() {
        // Applied to this table before any code leaned on it - the lesson from
        // OBS-48. Two actions on one combination would make the second register
        // as Taken against the first, silently losing a shortcut.
        for convention in CONVENTIONS {
            let table = default_bindings(convention);
            for (i, (action_a, chord_a)) in table.iter().enumerate() {
                for (action_b, chord_b) in &table[i + 1..] {
                    assert_ne!(
                        chord_a, chord_b,
                        "{convention:?}: {action_a:?} and {action_b:?} share a combination"
                    );
                }
            }
        }
    }

    #[test]
    fn every_default_on_windows_and_linux_is_alt_shift_and_never_ctrl_alt() {
        // D82: Ctrl+Alt is AltGr on Windows, so a default holding both would
        // swallow a letter of somebody's alphabet in every application while the
        // palette runs. One family, so a tester can guess the rest from one.
        for (action, chord) in DEFAULT_BINDINGS {
            assert!(
                chord.alt && chord.shift && !chord.ctrl && !chord.win,
                "{action:?} default is not Alt+Shift: {chord:?}"
            );
        }
    }

    #[test]
    fn every_default_on_macos_is_command_alt() {
        // Option alone types characters on macOS, Command does not.
        for (action, chord) in MACOS_DEFAULT_BINDINGS {
            assert!(
                chord.win && chord.alt && !chord.ctrl && !chord.shift,
                "{action:?} macOS default is not Cmd+Alt: {chord:?}"
            );
        }
    }

    #[test]
    fn both_conventions_bind_the_same_key_to_the_same_action() {
        // Only the modifiers differ, so a tester moving between systems keeps
        // the letters - and a table edited on one side only is caught here.
        for ((action_a, chord_a), (action_b, chord_b)) in
            DEFAULT_BINDINGS.iter().zip(MACOS_DEFAULT_BINDINGS.iter())
        {
            assert_eq!(action_a, action_b);
            assert_eq!(chord_a.key, chord_b.key, "{action_a:?}");
        }
    }

    #[test]
    fn default_chord_is_total_over_the_action_set() {
        for convention in CONVENTIONS {
            for action in HotkeyAction::ALL {
                assert!(
                    default_chord(convention, action).is_some(),
                    "{convention:?}: {action:?} has no default chord"
                );
            }
        }
    }
}
