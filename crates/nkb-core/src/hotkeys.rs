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
//! # The one thing this file does NOT settle: the modifier on macOS
//!
//! `ux-spec.md` 3 writes the Windows and Linux defaults as `Ctrl+Alt+<key>` and
//! the macOS defaults as `Cmd+Alt+<key>` - the same key, a different primary
//! modifier. The tables below carry the Windows and Linux convention, because
//! that is the platform the core loop is wired on first and delivering a value
//! on macOS is blocked anyway (`OBS-70`). The adapter that registers on macOS
//! swaps `ctrl` for `win` (the Command bit); doing that here would mean a `cfg`
//! in a crate that has none, and the swap is one line where the system is known.

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
    /// Show the palette if hidden by opacity, or dim it - never `hide()`,
    /// `OBS-80`.
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
    /// The `Ctrl+Alt+<key>` shape every default uses on Windows and Linux.
    const fn ctrl_alt(key: HotkeyKey) -> Self {
        Self {
            ctrl: true,
            alt: true,
            shift: false,
            win: false,
            key,
        }
    }
}

/// The default shortcut for every action, in the Windows and Linux convention.
///
/// One entry per action, in the order of [`HotkeyAction::ALL`]. Every default is
/// `Ctrl+Alt+<key>`: `ux-spec.md` 3 keeps them one family so a tester learns
/// them as a set rather than as ten separate things. The table is checked
/// against the catalogue of its own kind before any code depended on it - every
/// action present exactly once, and no two actions on the same combination -
/// because a duplicate combination would be a registration that reports the
/// second action as `Taken` against the first.
pub const DEFAULT_BINDINGS: [(HotkeyAction, HotkeyChord); 10] = [
    (HotkeyAction::NextValue, HotkeyChord::ctrl_alt(HotkeyKey::N)),
    (
        HotkeyAction::PreviousValue,
        HotkeyChord::ctrl_alt(HotkeyKey::P),
    ),
    (
        HotkeyAction::RepeatLast,
        HotkeyChord::ctrl_alt(HotkeyKey::R),
    ),
    (
        HotkeyAction::RestartPack,
        HotkeyChord::ctrl_alt(HotkeyKey::Digit0),
    ),
    (
        HotkeyAction::CopyReport,
        HotkeyChord::ctrl_alt(HotkeyKey::B),
    ),
    (
        HotkeyAction::MarkOk,
        HotkeyChord::ctrl_alt(HotkeyKey::Digit1),
    ),
    (
        HotkeyAction::MarkProblem,
        HotkeyChord::ctrl_alt(HotkeyKey::Digit2),
    ),
    (
        HotkeyAction::MarkSuspect,
        HotkeyChord::ctrl_alt(HotkeyKey::Digit3),
    ),
    (
        HotkeyAction::OpenPacks,
        HotkeyChord::ctrl_alt(HotkeyKey::Space),
    ),
    (
        HotkeyAction::ToggleVisibility,
        HotkeyChord::ctrl_alt(HotkeyKey::H),
    ),
];

/// The default chord for one action, if the default table names it.
///
/// Total over [`HotkeyAction::ALL`] by the test below, so the `Option` is really
/// only `None` for a hypothetical action added without a default - which the
/// test refuses.
#[must_use]
pub fn default_chord(action: HotkeyAction) -> Option<HotkeyChord> {
    DEFAULT_BINDINGS
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

    #[test]
    fn every_action_has_exactly_one_default() {
        // The registrar reserves a combination per action, so a missing default
        // is an action that can never be pressed, and a doubled one is a table
        // that disagrees with itself.
        for action in HotkeyAction::ALL {
            let count = DEFAULT_BINDINGS
                .iter()
                .filter(|(candidate, _)| *candidate == action)
                .count();
            assert_eq!(count, 1, "{action:?} must have exactly one default binding");
        }
        assert_eq!(
            DEFAULT_BINDINGS.len(),
            HotkeyAction::ALL.len(),
            "the default table and the action set must be the same size"
        );
    }

    #[test]
    fn no_two_actions_share_a_combination() {
        // Applied to this table before any code leaned on it - the lesson from
        // OBS-48. Two actions on one combination would make the second register
        // as Taken against the first, silently losing a shortcut.
        for (i, (action_a, chord_a)) in DEFAULT_BINDINGS.iter().enumerate() {
            for (action_b, chord_b) in &DEFAULT_BINDINGS[i + 1..] {
                assert_ne!(
                    chord_a, chord_b,
                    "{action_a:?} and {action_b:?} share a combination"
                );
            }
        }
    }

    #[test]
    fn every_default_is_ctrl_alt_and_nothing_stranger() {
        // ux-spec.md 3: one family, Ctrl+Alt plus a key. A default that grew a
        // Shift or a Win bit would be a shortcut a tester cannot guess from the
        // others, so the family property is guarded rather than trusted.
        for (action, chord) in DEFAULT_BINDINGS {
            assert!(
                chord.ctrl && chord.alt && !chord.shift && !chord.win,
                "{action:?} default is not Ctrl+Alt: {chord:?}"
            );
        }
    }

    #[test]
    fn default_chord_is_total_over_the_action_set() {
        for action in HotkeyAction::ALL {
            assert!(
                default_chord(action).is_some(),
                "{action:?} has no default chord"
            );
        }
    }
}
