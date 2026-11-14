//! What the engine of a send does with a shortcut of the palette that the `Shift` of the
//! clearing changed (`OBS-260`), decided without a bus so that it is tested on every system.
//!
//! # What was measured before this was written
//!
//! GNOME 50.1, 2026-11-14, the loop `petla-palety-pisze-wayland.py` and its trace of keys:
//! the recipe pressed `Shift` through the portal for `Shift+End`, and the hand pressed the
//! shortcut of the palette again within the 0.1 s `Shift` was down. `Shift` pressed through
//! the portal is a modifier of the seat, so the compositor saw `Ctrl+Alt+Shift+N`, which is
//! no shortcut, and handed the key to the application with the three modifiers. The palette
//! never heard of the press, and the application got a chord nobody pressed. A chord met
//! after the `Shift` came up was the shortcut again.
//!
//! The engine is handed that key before the application, as it is every key while it is the
//! global one, and it knows when the recipe holds `Shift`: the key the recipe waits for then
//! carries it (`Expected::shifted`). So a key that is a shortcut of the palette with `Shift`
//! added, met while the recipe holds `Shift`, is the press the compositor would have handed
//! the palette. The engine takes it, so the application never sees it, and says of it what
//! the portal would have said: `Activated` for the press and for each repeat of the held key,
//! `Deactivated` when it is let go. The words go through the same `Held` as the portal's, so a
//! chord held from before the clearing, whose repeat the compositor hands on while `Shift` is
//! down, stays one press (`trzymany`, measured 2 to 9 repeats in 1.5 s), and a press of its
//! own is dropped by the palette with words, as every press during a send is (`W1`).
//!
//! # How the key is recognised
//!
//! The keyval the engine is given is the key as the layout reads it with the modifiers held,
//! so `Shift` changes it: a letter becomes its capital, and a digit becomes what the layout
//! has above it, which differs between layouts. A letter is therefore compared by its small
//! form, and a digit by the key of the number row it is on (the code of the kernel, which is
//! what the engine is given). Space and the function keys are what they are with `Shift`. A
//! layout that does not write Latin letters gives keyvals of its own script, and a shortcut on
//! a letter is then not recognised: the key goes to the application as before.

use crate::hotkey::{Hotkey, HotkeyId};
use crate::linux_hotkeys::keysym_for_vk;
use crate::linux_ibus_rules::{IBUS_MODIFIER_MASK, is_release};

/// The bits of the state of a key that a shortcut of the palette is written with.
const SHIFT: u32 = 1 << 0;
const CTRL: u32 = 1 << 2;
const ALT: u32 = 1 << 3;
/// `Super` comes as `Mod4`, as `Super`, or as both.
const SUPER: u32 = (1 << 6) | (1 << 26);
/// `AltGr` on the layouts that have one. A chord with it held is another chord.
const ALT_GR: u32 = 1 << 7;

/// The code of the kernel of the key of the number row that has the digit `digit`:
/// `KEY_1` is 2 and `KEY_9` is 10, and `KEY_0` comes after them.
const fn number_row(digit: u32) -> u32 {
    if digit == 0 { 11 } else { digit + 1 }
}

/// How the key of a shortcut is recognised with `Shift` down.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChordKey {
    /// A letter, by the keysym of its small form.
    Letter(u32),
    /// A digit, by the code of its key in the number row.
    Digit(u32),
    /// Space or a function key, by its keysym.
    Unshifted(u32),
}

impl ChordKey {
    fn matches(self, keyval: u32, keycode: u32) -> bool {
        match self {
            // `Caps Lock` with `Shift` gives the small letter, and `Shift` alone the capital.
            Self::Letter(small) => keyval == small || keyval == small - 0x20,
            Self::Digit(code) => keycode == code,
            Self::Unshifted(keysym) => keyval == keysym,
        }
    }
}

/// A shortcut of the palette as the engine may meet it while the recipe holds `Shift`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ShiftedChord {
    id: HotkeyId,
    key: ChordKey,
    ctrl: bool,
    alt: bool,
    super_key: bool,
}

impl ShiftedChord {
    /// The chord `hotkey` with `Shift` added. `None` for a chord that has `Shift` already,
    /// which the compositor takes with the `Shift` of the recipe down as well, and for a key
    /// the palette has no keysym for.
    pub(crate) fn of(hotkey: &Hotkey) -> Option<Self> {
        if hotkey.shift {
            return None;
        }
        let keysym = keysym_for_vk(hotkey.vk)?;
        let key = match keysym {
            0x61..=0x7a => ChordKey::Letter(keysym),
            0x30..=0x39 => ChordKey::Digit(number_row(keysym - 0x30)),
            _ => ChordKey::Unshifted(keysym),
        };
        Some(Self {
            id: hotkey.id,
            key,
            ctrl: hotkey.ctrl,
            alt: hotkey.alt,
            super_key: hotkey.win,
        })
    }

    /// Whether the press of `keyval` on key `keycode` with `state` is this chord with `Shift`
    /// added. `Hyper` and `Meta` are not looked at, because a desktop may report one of them
    /// with `Alt` or `Super`, and the locks are not modifiers of a chord.
    fn pressed(&self, keyval: u32, keycode: u32, state: u32) -> bool {
        let held = state & (IBUS_MODIFIER_MASK | ALT_GR);
        held & SHIFT != 0
            && held & ALT_GR == 0
            && (held & CTRL != 0) == self.ctrl
            && (held & ALT != 0) == self.alt
            && (held & SUPER != 0) == self.super_key
            && self.key.matches(keyval, keycode)
    }
}

/// What the portal would have said of a shortcut, in its words.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Word {
    /// The keys of the shortcut went down, or the key of a shortcut held down came again.
    Activated(HotkeyId),
    /// The keys of the shortcut came up.
    Deactivated(HotkeyId),
}

/// What the engine does with one event of a key: whether it takes it, and what the portal
/// would have said of it, when it was a shortcut of the palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Met {
    pub taken: bool,
    pub said: Option<Word>,
}

impl Met {
    /// Handed to the application, and nothing said.
    const PASSED: Self = Self {
        taken: false,
        said: None,
    };

    const fn took(word: Word) -> Self {
        Self {
            taken: true,
            said: Some(word),
        }
    }
}

/// The keys of the shortcuts the engine took, by their code and with the shortcut each is,
/// until each is let go. A release goes where its press went (`OBS-255`): the press was taken,
/// so the release is too, and a press that comes again while the key is down is the key held,
/// repeated, whether `Shift` is still down or not.
#[derive(Debug, Default)]
pub(crate) struct ShiftedChords {
    down: Vec<(u32, HotkeyId)>,
}

impl ShiftedChords {
    /// What the engine does with the event of key `keyval` on key `keycode` whose state is
    /// `state`. `ours` is whether the recipe holds `Shift` now, and `chords` are the shortcuts
    /// of the palette, empty when no palette listens for them (`nkb send`).
    pub(crate) fn handle(
        &mut self,
        keyval: u32,
        keycode: u32,
        state: u32,
        ours: bool,
        chords: &[ShiftedChord],
    ) -> Met {
        let held = self.down.iter().position(|(down, _)| *down == keycode);
        if is_release(state) {
            return match held {
                Some(index) => Met::took(Word::Deactivated(self.down.swap_remove(index).1)),
                None => Met::PASSED,
            };
        }
        if let Some(index) = held {
            return Met::took(Word::Activated(self.down[index].1));
        }
        if !ours {
            return Met::PASSED;
        }
        match chords
            .iter()
            .find(|chord| chord.pressed(keyval, keycode, state))
        {
            Some(chord) => {
                self.down.push((keycode, chord.id));
                Met::took(Word::Activated(chord.id))
            }
            None => Met::PASSED,
        }
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
    use crate::linux_ibus_rules::IBUS_RELEASE_MASK;

    const LOCK: u32 = 1 << 1;
    const NUM_LOCK: u32 = 1 << 4;
    const META: u32 = 1 << 28;
    /// The code of the kernel of the key `N` on a QWERTY board, and of the number row's `1`.
    const KEY_N: u32 = 49;
    const KEY_1: u32 = 2;

    fn hotkey(id: u32, vk: u16, ctrl: bool, alt: bool, shift: bool, win: bool) -> Hotkey {
        Hotkey {
            id: HotkeyId(id),
            ctrl,
            alt,
            shift,
            win,
            vk,
        }
    }

    /// The defaults of Linux that the cases use: `Ctrl+Alt+N`, `Ctrl+Alt+1`, `Ctrl+Alt+Space`.
    fn defaults() -> Vec<ShiftedChord> {
        [
            hotkey(1, 0x4e, true, true, false, false),
            hotkey(2, 0x31, true, true, false, false),
            hotkey(3, 0x20, true, true, false, false),
        ]
        .iter()
        .filter_map(ShiftedChord::of)
        .collect()
    }

    fn press(chords: &mut ShiftedChords, keyval: u32, keycode: u32, state: u32) -> Met {
        chords.handle(keyval, keycode, state, true, &defaults())
    }

    /// The shortcut a taken key activated, if it did.
    fn activated(met: Met) -> Option<HotkeyId> {
        match met {
            Met {
                taken: true,
                said: Some(Word::Activated(id)),
            } => Some(id),
            _ => None,
        }
    }

    #[test]
    fn the_shortcut_with_the_shift_of_the_recipe_is_taken_and_activated() {
        // The trace of OBS-260: N came down with Shift, Alt and Ctrl. With `Shift` it is the
        // capital letter.
        let mut down = ShiftedChords::default();
        let met = press(&mut down, 0x4e, KEY_N, SHIFT | CTRL | ALT);
        assert_eq!(met, Met::took(Word::Activated(HotkeyId(1))));
    }

    #[test]
    fn caps_lock_with_shift_gives_the_small_letter_and_it_is_the_same_chord() {
        let mut down = ShiftedChords::default();
        let met = press(&mut down, 0x6e, KEY_N, SHIFT | CTRL | ALT | LOCK);
        assert_eq!(activated(met), Some(HotkeyId(1)));
    }

    #[test]
    fn a_digit_is_known_by_its_key_whatever_the_layout_has_above_it() {
        // `Shift+1` is `!` on a US board and something else on others, so the keyval does not
        // say which digit it was. The number row does.
        for shifted in [0x21_u32, 0x40, 0xa7] {
            let mut down = ShiftedChords::default();
            let met = press(&mut down, shifted, KEY_1, SHIFT | CTRL | ALT);
            assert_eq!(activated(met), Some(HotkeyId(2)), "keyval {shifted:#x}");
        }
        let mut down = ShiftedChords::default();
        assert_eq!(press(&mut down, 0x21, 3, SHIFT | CTRL | ALT), Met::PASSED);
    }

    #[test]
    fn the_number_row_runs_from_one_to_nine_and_then_zero() {
        assert_eq!(number_row(1), 2);
        assert_eq!(number_row(9), 10);
        assert_eq!(number_row(0), 11);
    }

    #[test]
    fn space_is_space_with_shift() {
        let mut down = ShiftedChords::default();
        let met = press(&mut down, 0x20, 57, SHIFT | CTRL | ALT);
        assert_eq!(activated(met), Some(HotkeyId(3)));
    }

    #[test]
    fn the_locks_and_meta_change_nothing() {
        let mut down = ShiftedChords::default();
        let met = press(&mut down, 0x4e, KEY_N, SHIFT | CTRL | ALT | NUM_LOCK | META);
        assert_eq!(activated(met), Some(HotkeyId(1)));
    }

    #[test]
    fn without_shift_the_key_is_not_ours_to_take() {
        // The compositor gives the palette the plain chord itself, so the engine never meets
        // it, and a key that came here without `Shift` is somebody else's.
        let mut down = ShiftedChords::default();
        assert_eq!(press(&mut down, 0x6e, KEY_N, CTRL | ALT), Met::PASSED);
    }

    #[test]
    fn a_chord_with_other_modifiers_is_another_chord() {
        for state in [
            SHIFT | CTRL,
            SHIFT | ALT,
            SHIFT | CTRL | ALT | (1 << 6),
            SHIFT | CTRL | ALT | (1 << 26),
            SHIFT | CTRL | ALT | ALT_GR,
            SHIFT,
        ] {
            let mut down = ShiftedChords::default();
            assert_eq!(
                press(&mut down, 0x4e, KEY_N, state),
                Met::PASSED,
                "state {state:#x}"
            );
        }
    }

    #[test]
    fn another_key_is_not_the_chord() {
        let mut down = ShiftedChords::default();
        assert_eq!(press(&mut down, 0x4d, 50, SHIFT | CTRL | ALT), Met::PASSED);
    }

    #[test]
    fn nothing_is_taken_when_the_recipe_does_not_hold_shift() {
        // A person who presses `Ctrl+Alt+Shift+N` outside the `Shift` of the recipe means it.
        let mut down = ShiftedChords::default();
        let met = down.handle(0x4e, KEY_N, SHIFT | CTRL | ALT, false, &defaults());
        assert_eq!(met, Met::PASSED);
    }

    #[test]
    fn nothing_is_taken_when_no_palette_listens() {
        // `nkb send --clear` has no shortcuts.
        let mut down = ShiftedChords::default();
        let met = down.handle(0x4e, KEY_N, SHIFT | CTRL | ALT, true, &[]);
        assert_eq!(met, Met::PASSED);
    }

    #[test]
    fn a_chord_with_shift_of_its_own_is_left_to_the_compositor() {
        // `Ctrl+Alt+Shift+N` is a shortcut with the `Shift` of the recipe down too.
        assert_eq!(
            ShiftedChord::of(&hotkey(4, 0x4e, true, true, true, false)),
            None
        );
    }

    #[test]
    fn a_chord_with_super_is_known_by_either_bit_of_super() {
        let chords: Vec<ShiftedChord> = [hotkey(5, 0x4b, true, false, false, true)]
            .iter()
            .filter_map(ShiftedChord::of)
            .collect();
        for super_bit in [1 << 6, 1 << 26, (1 << 6) | (1 << 26)] {
            let mut down = ShiftedChords::default();
            let met = down.handle(0x4b, 37, SHIFT | CTRL | super_bit, true, &chords);
            assert_eq!(activated(met), Some(HotkeyId(5)), "bit {super_bit:#x}");
        }
    }

    #[test]
    fn the_release_of_a_taken_press_is_taken_whatever_the_modifiers_are_by_then() {
        // The `Shift` of the recipe may be up when the hand lets go, and the letter small
        // again. The release goes where the press went (OBS-255).
        let mut down = ShiftedChords::default();
        press(&mut down, 0x4e, KEY_N, SHIFT | CTRL | ALT);
        let released = down.handle(0x6e, KEY_N, CTRL | IBUS_RELEASE_MASK, false, &[]);
        assert_eq!(released, Met::took(Word::Deactivated(HotkeyId(1))));
        // And once let go, the key is nobody's: its next release goes to the application.
        let again = down.handle(0x6e, KEY_N, IBUS_RELEASE_MASK, false, &[]);
        assert_eq!(again, Met::PASSED);
    }

    #[test]
    fn a_release_whose_press_was_not_taken_goes_to_the_application() {
        // The plain chord was the compositor's, which hands the application its release only.
        let mut down = ShiftedChords::default();
        let met = down.handle(
            0x6e,
            KEY_N,
            CTRL | ALT | IBUS_RELEASE_MASK,
            true,
            &defaults(),
        );
        assert_eq!(met, Met::PASSED);
    }

    #[test]
    fn a_press_repeated_while_the_key_is_down_is_announced_again_as_the_portal_does() {
        // The portal says `Activated` for every repeat of a held key, and `Held` makes them one
        // press. The engine says the same of the repeats it takes.
        let mut down = ShiftedChords::default();
        press(&mut down, 0x4e, KEY_N, SHIFT | CTRL | ALT);
        let repeated = down.handle(0x4e, KEY_N, SHIFT | CTRL | ALT, true, &defaults());
        assert_eq!(repeated, Met::took(Word::Activated(HotkeyId(1))));
        // Taken even when the `Shift` of the recipe is up by then, so that the application
        // does not get a key it never saw go down.
        let later = down.handle(0x6e, KEY_N, CTRL | ALT, false, &defaults());
        assert_eq!(later, Met::took(Word::Activated(HotkeyId(1))));
    }

    #[test]
    fn two_chords_held_together_are_each_let_go_on_their_own() {
        let mut down = ShiftedChords::default();
        press(&mut down, 0x4e, KEY_N, SHIFT | CTRL | ALT);
        press(&mut down, 0x21, KEY_1, SHIFT | CTRL | ALT);
        assert_eq!(
            down.handle(0x6e, KEY_N, IBUS_RELEASE_MASK, false, &[]),
            Met::took(Word::Deactivated(HotkeyId(1)))
        );
        assert_eq!(
            down.handle(0x31, KEY_1, IBUS_RELEASE_MASK, false, &[]),
            Met::took(Word::Deactivated(HotkeyId(2)))
        );
        assert_eq!(
            down.handle(0x31, KEY_1, IBUS_RELEASE_MASK, false, &[]),
            Met::PASSED
        );
    }

    #[test]
    fn a_key_with_no_keysym_is_no_shifted_chord() {
        assert_eq!(
            ShiftedChord::of(&hotkey(6, 0xff, true, true, false, false)),
            None
        );
    }
}
