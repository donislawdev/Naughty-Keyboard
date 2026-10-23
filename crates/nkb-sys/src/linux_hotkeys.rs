//! A global shortcut on X11, the decisions that need no server to make.
//!
//! Everything here is a function of its arguments, so it is tested on every
//! system while only Linux feeds it. The calls to the server are `linux/hotkey.rs`.
//!
//! # What was measured before this was written
//!
//! Probed 2026-10-23 on Xorg 21 with the `dummy` driver, a keyboard device made
//! with `uinput` (so the server saw the keys as it sees hardware) and
//! `tools/sonda-x11 grab`, the chord pressed with `tools/sondy/linux/xorg-fizyczny`
//! (`akord-powtarzanie.sh`, `akord-przelaczanie-ukladu.sh`, `akord-przelaczniki.sh`):
//!
//! - `XGrabKey` of `Alt+Shift+N` in the four combinations with Caps Lock and Num
//!   Lock is accepted, a tap is one `KeyPress` and one `KeyRelease` with the state
//!   `0x9`. With the second layout group locked the press still arrives, with the
//!   same state, so the group needs no grab of its own.
//! - A key held for 1.2 s was heard 8 times as pairs of `KeyRelease` and `KeyPress`
//!   that carry the SAME server time. With the XKB flag for detectable auto repeat
//!   it was heard 9 times as `KeyPress` alone. The pair with equal times is how a
//!   repeat is told from a person pressing again, and it needs no flag.
//! - With the XKB option `grp:alt_shift_toggle` the server takes `Shift` for the
//!   toggle. The press of `N` reaches a grab as `Alt` alone (state `0x8`), so a
//!   grab of `Alt+Shift+N` is never heard (0 of 1, against 1 of 1 without the
//!   option) and the layout group changes on every press. A grab of `Alt+N` heard
//!   it. The same was measured for `grp:lalt_lshift_toggle`, `grp:ctrl_shift_toggle`
//!   and `grp:ctrl_alt_toggle` on the chord of their own two modifiers, and for
//!   `grp:alt_space_toggle` on `Alt+Space`. A chord of two other modifiers was
//!   heard as always. The two toggles on `Space` of a modifier and `Shift` and
//!   `Super+Space` were heard AND changed the layout.
//! - The options are in the `_XKB_RULES_NAMES` property of the root window, five
//!   strings with a zero byte after each: rules, model, layouts, variants, options.

use crate::hotkey::{Hotkey, HotkeyId, HotkeyRegistration};
use crate::linux_rules::Keymap;

/// The `Shift`, `Lock` (Caps Lock) and `Control` bits of a modifier state. They are
/// the same on every server, unlike the five that follow them.
pub(crate) const SHIFT: u16 = 0x0001;
pub(crate) const LOCK: u16 = 0x0002;
pub(crate) const CONTROL: u16 = 0x0004;

/// The modifier bits of the state a key event carries. The bits above are the
/// buttons of the pointer and the layout group.
const MODIFIER_BITS: u16 = 0x00FF;

/// `BadAccess`: another client holds the grab.
const BAD_ACCESS: u8 = 10;

/// What `Failed` carries for a key the table has no symbol for. Not a code of the
/// protocol: the protocol's errors are small numbers.
pub(crate) const NO_SUCH_KEY: u32 = 0x4E4B_0001;
/// What `Failed` carries when the chord names `Alt` or `Super` and no key of the
/// keyboard is mapped to that modifier.
pub(crate) const NO_SUCH_MODIFIER: u32 = 0x4E4B_0002;
/// What `Failed` carries when the keyboard layout switches on keys of this chord, so
/// the server would take the chord and no grab would hear it.
pub(crate) const LAYOUT_TOGGLE: u32 = 0x4E4B_0003;

const KEYSYM_ALT_L: u32 = 0xFFE9;
const KEYSYM_ALT_R: u32 = 0xFFEA;
const KEYSYM_SUPER_L: u32 = 0xFFEB;
const KEYSYM_SUPER_R: u32 = 0xFFEC;
const KEYSYM_NUM_LOCK: u32 = 0xFF7F;
const KEYSYM_SCROLL_LOCK: u32 = 0xFF14;
const KEYSYM_F1: u32 = 0xFFBE;

/// The X11 keysym of a key of the shortcut vocabulary, given as the code the
/// vocabulary is written in: letters and digits are their ASCII codes, `F1` to
/// `F24` run from `0x70`, and Space is `0x20`.
///
/// A letter is its lower case keysym, which is the one a key types without `Shift`.
pub(crate) const fn keysym_for_vk(vk: u16) -> Option<u32> {
    match vk {
        0x41..=0x5A => Some(vk as u32 + 0x20),
        0x30..=0x39 | 0x20 => Some(vk as u32),
        0x70..=0x87 => Some(KEYSYM_F1 + (vk as u32 - 0x70)),
        _ => None,
    }
}

/// Which bits of a modifier state stand for the keys a chord is written with and for
/// the locks that must not matter, as the keyboard of THIS server has them. `Shift`,
/// `Lock` and `Control` are fixed. `Alt` is the modifier an `Alt` key is mapped to,
/// which on a keyboard with the keys swapped is not the one the name suggests.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ModifierBits {
    pub alt: u16,
    pub super_key: u16,
    pub num_lock: u16,
    pub scroll_lock: u16,
}

impl ModifierBits {
    /// Reads them from the server's modifier map: `keys_per_modifier` keycodes for
    /// each of the eight modifiers, zero where there is none. The lowest modifier
    /// that has a key of the kind is the one taken.
    pub(crate) fn read(modifier_keys: &[u8], keys_per_modifier: usize, keymap: &Keymap) -> Self {
        let mut bits = Self::default();
        if keys_per_modifier == 0 {
            return bits;
        }
        for (index, row) in modifier_keys.chunks(keys_per_modifier).take(8).enumerate() {
            let bit = 1u16 << index;
            let holds = |wanted: &[u32]| {
                row.iter().any(|keycode| {
                    *keycode != 0
                        && keymap
                            .keysyms_of(*keycode)
                            .iter()
                            .any(|keysym| wanted.contains(keysym))
                })
            };
            for (slot, wanted) in [
                (&mut bits.alt, &[KEYSYM_ALT_L, KEYSYM_ALT_R][..]),
                (&mut bits.super_key, &[KEYSYM_SUPER_L, KEYSYM_SUPER_R][..]),
                (&mut bits.num_lock, &[KEYSYM_NUM_LOCK][..]),
                (&mut bits.scroll_lock, &[KEYSYM_SCROLL_LOCK][..]),
            ] {
                if *slot == 0 && holds(wanted) {
                    *slot = bit;
                }
            }
        }
        bits
    }

    /// The bits of the locks, which must not change whether a chord is heard.
    pub(crate) const fn locks(&self) -> u16 {
        LOCK | self.num_lock | self.scroll_lock
    }
}

/// Every combination of the locks, since the server matches a grab against the exact
/// state and a chord pressed with Caps Lock on is another state (`D96`, the same
/// reasoning as the `Escape` of a send).
pub(crate) fn lock_variants(bits: &ModifierBits) -> Vec<u16> {
    let mut locks = vec![LOCK];
    for lock in [bits.num_lock, bits.scroll_lock] {
        if lock != 0 && !locks.contains(&lock) {
            locks.push(lock);
        }
    }
    let mut variants = vec![0u16];
    for lock in locks {
        let with: Vec<u16> = variants.iter().map(|variant| variant | lock).collect();
        variants.extend(with);
    }
    variants
}

/// A chord on this keyboard: the key to grab and the modifiers it is grabbed with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Binding {
    pub id: HotkeyId,
    pub keycode: u8,
    pub modifiers: u16,
}

/// The key and the modifier bits of `hotkey` on this keyboard, or what `Failed` says
/// of why it has none.
pub(crate) fn resolve(
    hotkey: &Hotkey,
    keymap: &Keymap,
    bits: &ModifierBits,
) -> Result<Binding, u32> {
    let keycode = keysym_for_vk(hotkey.vk)
        .and_then(|keysym| keymap.find_keycode_anywhere(keysym))
        .ok_or(NO_SUCH_KEY)?;
    let mut modifiers = 0;
    if hotkey.ctrl {
        modifiers |= CONTROL;
    }
    if hotkey.shift {
        modifiers |= SHIFT;
    }
    for (wanted, bit) in [(hotkey.alt, bits.alt), (hotkey.win, bits.super_key)] {
        if wanted {
            if bit == 0 {
                return Err(NO_SUCH_MODIFIER);
            }
            modifiers |= bit;
        }
    }
    Ok(Binding {
        id: hotkey.id,
        keycode,
        modifiers,
    })
}

/// Whether `held`, the shortcuts of one listener, hold the chord of `binding` already.
///
/// The server does not say so: a client that grabs a key it holds already replaces
/// its own grab and is told nothing. Windows and macOS refuse the chord a second
/// time, so the same listener is asked here to get the same answer from every system.
pub(crate) fn held_already(held: &[Binding], binding: &Binding) -> bool {
    held.iter()
        .any(|other| other.keycode == binding.keycode && other.modifiers == binding.modifiers)
}

/// What the answer of the server to one grab means. Only `BadAccess` is "taken", and
/// the measurement behind it is `OBS-200` point 4.
pub(crate) const fn registration_of(error_code: u8) -> HotkeyRegistration {
    if error_code == BAD_ACCESS {
        HotkeyRegistration::Taken
    } else {
        HotkeyRegistration::Failed {
            code: error_code as u32,
        }
    }
}

/// The shortcut a press stands for. `locks` are the bits that do not count, `state`
/// is the one the event carries.
pub(crate) fn matching(
    bindings: &[Binding],
    locks: u16,
    keycode: u8,
    state: u16,
) -> Option<HotkeyId> {
    let held = state & MODIFIER_BITS & !locks;
    bindings
        .iter()
        .find(|binding| binding.keycode == keycode && binding.modifiers == held)
        .map(|binding| binding.id)
}

/// Tells the server repeating a key from a person pressing it again.
///
/// A held key is heard as a `KeyRelease` followed by a `KeyPress` of the same key
/// with the same server time, and a person pressing again cannot do both in one
/// millisecond. So a release is remembered and the next press is a repeat when it
/// is that release's other half.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Repeats {
    released: Option<(u8, u32)>,
}

impl Repeats {
    pub(crate) fn released(&mut self, keycode: u8, time: u32) {
        self.released = Some((keycode, time));
    }

    /// Whether this press is the repeat of the key released a moment ago.
    pub(crate) fn is_repeat(&mut self, keycode: u8, time: u32) -> bool {
        let repeat = self.released == Some((keycode, time));
        self.released = None;
        repeat
    }
}

/// The options of the layout out of the `_XKB_RULES_NAMES` property, the fifth of its
/// strings. `None` when the property has fewer.
pub(crate) fn xkb_options(property: &[u8]) -> Option<String> {
    property
        .split(|byte| *byte == 0)
        .nth(4)
        .map(|options| String::from_utf8_lossy(options).into_owned())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Modifier {
    Ctrl,
    Alt,
    Shift,
    Win,
}

/// The modifier a part of the name of an option stands for, without the side.
fn modifier_of(part: &str) -> Option<Modifier> {
    let bare = part
        .strip_prefix('l')
        .or_else(|| part.strip_prefix('r'))
        .filter(|rest| matches!(*rest, "alt" | "shift" | "ctrl" | "win"))
        .unwrap_or(part);
    match bare {
        "alt" => Some(Modifier::Alt),
        "shift" => Some(Modifier::Shift),
        "ctrl" => Some(Modifier::Ctrl),
        "win" => Some(Modifier::Win),
        _ => None,
    }
}

fn holds(hotkey: &Hotkey, modifier: Modifier) -> bool {
    match modifier {
        Modifier::Ctrl => hotkey.ctrl,
        Modifier::Alt => hotkey.alt,
        Modifier::Shift => hotkey.shift,
        Modifier::Win => hotkey.win,
    }
}

/// Whether the option switches the layout on keys `hotkey` is pressed with, so that
/// the server takes the chord and no grab hears it.
///
/// Measured for `alt_shift`, `lalt_lshift`, `ctrl_shift`, `ctrl_alt` and for
/// `alt_space` on `Alt+Space` alone. The other sides of the same two modifiers
/// (`lctrl_lshift`, `rctrl_rshift` and so on) are the same mechanism and were not
/// measured. A toggle on `Space` with another modifier is heard together with the
/// chord and is not refused.
fn takes_chord(option: &str, hotkey: &Hotkey) -> bool {
    let Some(name) = option
        .strip_prefix("grp:")
        .and_then(|name| name.strip_suffix("_toggle"))
    else {
        return false;
    };
    let mut parts = name.split('_');
    let (Some(first), Some(second), None) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    let Some(first) = modifier_of(first) else {
        return false;
    };
    if second == "space" {
        return first == Modifier::Alt
            && hotkey.vk == 0x20
            && hotkey.alt
            && !hotkey.ctrl
            && !hotkey.shift
            && !hotkey.win;
    }
    let Some(second) = modifier_of(second) else {
        return false;
    };
    first != second && holds(hotkey, first) && holds(hotkey, second)
}

/// The first option of the layout, in the comma separated list the server keeps,
/// that takes the chord of `hotkey`.
pub(crate) fn layout_toggle<'a>(options: &'a str, hotkey: &Hotkey) -> Option<&'a str> {
    options
        .split(',')
        .map(str::trim)
        .find(|option| takes_chord(option, hotkey))
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;

    const ALT_L_KEY: u8 = 64;
    const ALT_R_KEY: u8 = 108;
    const NUM_KEY: u8 = 77;
    const SUPER_KEY: u8 = 133;
    const SCROLL_KEY: u8 = 78;
    const N_KEY: u8 = 57;
    const SPACE_KEY: u8 = 65;

    /// A keyboard of the shape `us` has: keys 8 to 255, four symbols for each.
    fn keymap(set: &[(u8, [u32; 4])]) -> Keymap {
        let mut keysyms = vec![0u32; (255 - 8 + 1) * 4];
        for (keycode, row) in set {
            let start = (usize::from(*keycode) - 8) * 4;
            keysyms[start..start + 4].copy_from_slice(row);
        }
        Keymap::new(8, 4, keysyms)
    }

    fn us_keymap() -> Keymap {
        keymap(&[
            (ALT_L_KEY, [KEYSYM_ALT_L, 0xFFE7, 0, 0]),
            (ALT_R_KEY, [KEYSYM_ALT_R, 0xFFE8, 0, 0]),
            (NUM_KEY, [KEYSYM_NUM_LOCK, 0, 0, 0]),
            (SCROLL_KEY, [KEYSYM_SCROLL_LOCK, 0, 0, 0]),
            (SUPER_KEY, [KEYSYM_SUPER_L, 0, 0, 0]),
            (N_KEY, [0x6E, 0x4E, 0, 0]),
            (SPACE_KEY, [0x20, 0x20, 0, 0]),
        ])
    }

    /// The modifier map of a server: Shift, Lock, Control, then `Mod1` to `Mod5`, two
    /// keycodes for each.
    fn modifier_map(rows: [[u8; 2]; 8]) -> Vec<u8> {
        rows.iter().flatten().copied().collect()
    }

    fn standard_bits() -> ModifierBits {
        ModifierBits::read(
            &modifier_map([
                [50, 62],
                [66, 0],
                [37, 105],
                [ALT_L_KEY, ALT_R_KEY],
                [NUM_KEY, 0],
                [0, 0],
                [SUPER_KEY, 134],
                [92, 0],
            ]),
            2,
            &us_keymap(),
        )
    }

    fn hotkey(ctrl: bool, alt: bool, shift: bool, win: bool, vk: u16) -> Hotkey {
        Hotkey {
            id: HotkeyId(7),
            ctrl,
            alt,
            shift,
            win,
            vk,
        }
    }

    #[test]
    fn a_key_of_the_vocabulary_is_the_keysym_it_types_without_shift() {
        assert_eq!(keysym_for_vk(0x41), Some(0x61), "A is the lower case a");
        assert_eq!(keysym_for_vk(0x5A), Some(0x7A));
        assert_eq!(keysym_for_vk(0x30), Some(0x30));
        assert_eq!(keysym_for_vk(0x39), Some(0x39));
        assert_eq!(keysym_for_vk(0x20), Some(0x20));
    }

    #[test]
    fn the_function_keys_run_from_f1_to_f24_of_the_keysym_table() {
        assert_eq!(keysym_for_vk(0x70), Some(0xFFBE), "F1");
        assert_eq!(keysym_for_vk(0x7B), Some(0xFFC9), "F12");
        assert_eq!(keysym_for_vk(0x87), Some(0xFFD5), "F24");
        assert_eq!(keysym_for_vk(0x88), None, "the table ends at F24");
    }

    #[test]
    fn a_code_outside_the_vocabulary_has_no_keysym() {
        for vk in [0x00, 0x1F, 0x21, 0x2F, 0x3A, 0x40, 0x5B, 0x6F] {
            assert_eq!(keysym_for_vk(vk), None, "{vk:#x}");
        }
    }

    #[test]
    fn the_modifier_bits_are_read_from_the_map_of_the_server() {
        let bits = standard_bits();
        assert_eq!(bits.alt, 0x08, "Mod1");
        assert_eq!(bits.num_lock, 0x10, "Mod2");
        assert_eq!(bits.super_key, 0x40, "Mod4");
        assert_eq!(bits.scroll_lock, 0, "no key of the map is Scroll Lock");
    }

    #[test]
    fn alt_is_the_modifier_an_alt_key_is_mapped_to_not_the_one_by_name() {
        // `altwin:swap_alt_win`: the Alt keys are on Mod4 and the Super keys on Mod1.
        let bits = ModifierBits::read(
            &modifier_map([
                [50, 62],
                [66, 0],
                [37, 105],
                [SUPER_KEY, 134],
                [NUM_KEY, 0],
                [0, 0],
                [ALT_L_KEY, ALT_R_KEY],
                [92, 0],
            ]),
            2,
            &us_keymap(),
        );
        assert_eq!(bits.alt, 0x40);
        assert_eq!(bits.super_key, 0x08);
    }

    #[test]
    fn a_modifier_with_two_rows_of_the_kind_takes_the_lowest() {
        let bits = ModifierBits::read(
            &modifier_map([
                [50, 62],
                [66, 0],
                [37, 105],
                [ALT_L_KEY, 0],
                [0, 0],
                [0, 0],
                [0, 0],
                [ALT_R_KEY, 0],
            ]),
            2,
            &us_keymap(),
        );
        assert_eq!(bits.alt, 0x08, "one bit, so the chord is one state");
    }

    #[test]
    fn a_keyboard_with_only_the_right_alt_and_super_keys_has_both_bits() {
        let keymap = keymap(&[
            (ALT_R_KEY, [KEYSYM_ALT_R, 0, 0, 0]),
            (134, [KEYSYM_SUPER_R, 0, 0, 0]),
            (NUM_KEY, [KEYSYM_NUM_LOCK, 0, 0, 0]),
            (SCROLL_KEY, [KEYSYM_SCROLL_LOCK, 0, 0, 0]),
        ]);
        let bits = ModifierBits::read(
            &modifier_map([
                [50, 62],
                [66, 0],
                [37, 105],
                [ALT_R_KEY, 0],
                [NUM_KEY, 0],
                [SCROLL_KEY, 0],
                [134, 0],
                [92, 0],
            ]),
            2,
            &keymap,
        );
        assert_eq!(bits.alt, 0x08);
        assert_eq!(bits.super_key, 0x40);
        assert_eq!(bits.num_lock, 0x10);
        assert_eq!(bits.scroll_lock, 0x20);
    }

    #[test]
    fn a_keycode_outside_the_map_has_no_symbols() {
        let map = Keymap::new(8, 4, vec![0x61; 8]);
        assert_eq!(map.keysyms_of(8), &[0x61; 4]);
        assert_eq!(map.keysyms_of(9), &[0x61; 4]);
        assert!(map.keysyms_of(10).is_empty(), "past the last key");
        assert!(map.keysyms_of(7).is_empty(), "below the first key");
        assert!(map.keysyms_of(0).is_empty());
    }

    #[test]
    fn a_map_with_no_keys_per_modifier_has_no_bits() {
        assert_eq!(
            ModifierBits::read(&[], 0, &us_keymap()),
            ModifierBits::default()
        );
    }

    #[test]
    fn the_locks_are_caps_and_every_lock_the_keyboard_has() {
        let bits = standard_bits();
        assert_eq!(bits.locks(), LOCK | 0x10);
        let with_scroll = ModifierBits {
            scroll_lock: 0x20,
            ..bits
        };
        assert_eq!(with_scroll.locks(), LOCK | 0x10 | 0x20);
    }

    #[test]
    fn a_grab_is_made_in_every_combination_of_the_locks() {
        let mut variants = lock_variants(&standard_bits());
        variants.sort_unstable();
        assert_eq!(variants, vec![0x00, 0x02, 0x10, 0x12]);
    }

    #[test]
    fn a_keyboard_with_a_third_lock_has_eight_combinations() {
        let bits = ModifierBits {
            scroll_lock: 0x20,
            ..standard_bits()
        };
        let mut variants = lock_variants(&bits);
        variants.sort_unstable();
        assert_eq!(
            variants,
            vec![0x00, 0x02, 0x10, 0x12, 0x20, 0x22, 0x30, 0x32]
        );
    }

    #[test]
    fn a_keyboard_with_no_num_lock_key_has_two_combinations() {
        let bits = ModifierBits {
            num_lock: 0,
            scroll_lock: 0,
            ..standard_bits()
        };
        let mut variants = lock_variants(&bits);
        variants.sort_unstable();
        assert_eq!(variants, vec![0x00, 0x02]);
    }

    #[test]
    fn two_locks_on_one_modifier_are_one_combination_of_it() {
        let bits = ModifierBits {
            scroll_lock: 0x10,
            ..standard_bits()
        };
        assert_eq!(lock_variants(&bits).len(), 4);
    }

    #[test]
    fn a_chord_is_the_key_and_the_modifier_bits_of_this_keyboard() {
        let binding = resolve(
            &hotkey(false, true, true, false, 0x4E),
            &us_keymap(),
            &standard_bits(),
        )
        .expect("the key and both modifiers exist");
        assert_eq!(binding.keycode, N_KEY);
        assert_eq!(binding.modifiers, SHIFT | 0x08);
        assert_eq!(binding.id, HotkeyId(7));
    }

    #[test]
    fn control_and_super_are_added_to_the_bits() {
        let binding = resolve(
            &hotkey(true, false, false, true, 0x20),
            &us_keymap(),
            &standard_bits(),
        )
        .expect("resolves");
        assert_eq!(binding.keycode, SPACE_KEY);
        assert_eq!(binding.modifiers, CONTROL | 0x40);
    }

    #[test]
    fn a_chord_with_no_modifier_is_the_bare_key() {
        let binding = resolve(
            &hotkey(false, false, false, false, 0x4E),
            &us_keymap(),
            &standard_bits(),
        )
        .expect("resolves");
        assert_eq!(binding.modifiers, 0);
    }

    #[test]
    fn a_key_the_keyboard_does_not_have_is_said_by_a_code_of_its_own() {
        // `Z` is on no key of the test keyboard, and F1 has none either.
        for vk in [0x5A, 0x70, 0x88] {
            let outcome = resolve(
                &hotkey(false, true, true, false, vk),
                &us_keymap(),
                &standard_bits(),
            );
            assert_eq!(outcome, Err(NO_SUCH_KEY), "{vk:#x}");
        }
    }

    #[test]
    fn alt_with_no_alt_key_mapped_is_said_by_a_code_of_its_own() {
        let outcome = resolve(
            &hotkey(false, true, false, false, 0x4E),
            &us_keymap(),
            &ModifierBits::default(),
        );
        assert_eq!(outcome, Err(NO_SUCH_MODIFIER));
        let outcome = resolve(
            &hotkey(false, false, false, true, 0x4E),
            &us_keymap(),
            &ModifierBits::default(),
        );
        assert_eq!(outcome, Err(NO_SUCH_MODIFIER), "and so is Super");
    }

    #[test]
    fn a_symbol_on_a_second_group_is_found_when_no_first_level_has_it() {
        // `ru,us`: the key N types a Cyrillic letter first and `n` in the second group.
        let russian_first = keymap(&[(N_KEY, [0x06D4, 0x06F4, 0x6E, 0x4E])]);
        assert_eq!(russian_first.find_keycode_anywhere(0x6E), Some(N_KEY));
        assert_eq!(russian_first.find_keycode(0x6E), None);
    }

    #[test]
    fn a_first_level_is_preferred_to_a_later_one() {
        let map = keymap(&[
            (20, [0x31, 0x21, 0, 0]),
            (30, [0x6D, 0x4D, 0x6E, 0x4E]),
            (N_KEY, [0x6E, 0x4E, 0, 0]),
        ]);
        assert_eq!(map.find_keycode_anywhere(0x6E), Some(N_KEY));
    }

    #[test]
    fn the_codes_of_our_own_differ_from_each_other_and_from_any_error_of_the_protocol() {
        let ours = [NO_SUCH_KEY, NO_SUCH_MODIFIER, LAYOUT_TOGGLE];
        for (index, code) in ours.iter().enumerate() {
            assert!(
                *code > u32::from(u8::MAX),
                "{code:#x} is a code of the protocol"
            );
            assert!(!ours[..index].contains(code), "{code:#x} appears twice");
        }
    }

    #[test]
    fn a_chord_a_listener_holds_is_held_already_whatever_its_id() {
        let held = bindings();
        let same_chord_other_id = Binding {
            id: HotkeyId(9),
            keycode: N_KEY,
            modifiers: SHIFT | 0x08,
        };
        assert!(held_already(&held, &same_chord_other_id));
        assert!(!held_already(
            &held,
            &Binding {
                keycode: 66,
                ..same_chord_other_id
            }
        ));
        assert!(!held_already(
            &held,
            &Binding {
                modifiers: 0x08,
                ..same_chord_other_id
            }
        ));
        assert!(!held_already(&[], &same_chord_other_id));
    }

    #[test]
    fn only_bad_access_means_taken() {
        assert_eq!(registration_of(10), HotkeyRegistration::Taken);
        assert_eq!(registration_of(1), HotkeyRegistration::Failed { code: 1 });
        assert_eq!(registration_of(9), HotkeyRegistration::Failed { code: 9 });
        assert_eq!(registration_of(11), HotkeyRegistration::Failed { code: 11 });
    }

    fn bindings() -> Vec<Binding> {
        vec![
            Binding {
                id: HotkeyId(1),
                keycode: N_KEY,
                modifiers: SHIFT | 0x08,
            },
            Binding {
                id: HotkeyId(2),
                keycode: SPACE_KEY,
                modifiers: SHIFT | 0x08,
            },
        ]
    }

    #[test]
    fn a_press_finds_its_shortcut_by_key_and_modifiers() {
        let found = |keycode, state| matching(&bindings(), 0x12, keycode, state);
        assert_eq!(found(N_KEY, 0x09), Some(HotkeyId(1)));
        assert_eq!(found(SPACE_KEY, 0x09), Some(HotkeyId(2)));
        assert_eq!(found(N_KEY, 0x08), None, "Alt alone is not the chord");
        assert_eq!(
            found(N_KEY, 0x0D),
            None,
            "an extra Control is not the chord"
        );
        assert_eq!(found(66, 0x09), None, "another key");
    }

    #[test]
    fn the_locks_do_not_change_which_shortcut_a_press_is() {
        let found = |state| matching(&bindings(), 0x12, N_KEY, state);
        for locks in [0x00, 0x02, 0x10, 0x12] {
            assert_eq!(found(0x09 | locks), Some(HotkeyId(1)), "{locks:#x}");
        }
    }

    #[test]
    fn the_layout_group_and_the_buttons_do_not_change_it_either() {
        let found = |state| matching(&bindings(), 0x12, N_KEY, state);
        assert_eq!(found(0x09 | 0x2000), Some(HotkeyId(1)), "group 2");
        assert_eq!(found(0x09 | 0x4000), Some(HotkeyId(1)), "group 3");
        assert_eq!(found(0x09 | 0x0100), Some(HotkeyId(1)), "a button held");
    }

    #[test]
    fn a_lock_that_is_not_one_of_the_keyboard_counts_as_a_modifier() {
        // Mod3 is not a lock here, so a press with it down is another chord.
        assert_eq!(matching(&bindings(), 0x12, N_KEY, 0x09 | 0x20), None);
    }

    #[test]
    fn a_held_key_is_a_release_and_a_press_with_the_same_time() {
        let mut repeats = Repeats::default();
        assert!(
            !repeats.is_repeat(N_KEY, 100),
            "the first press is the person's"
        );
        repeats.released(N_KEY, 760);
        assert!(repeats.is_repeat(N_KEY, 760), "the repeat of it");
        repeats.released(N_KEY, 800);
        assert!(repeats.is_repeat(N_KEY, 800), "and the next");
    }

    #[test]
    fn a_press_after_a_release_at_another_time_is_the_person_pressing_again() {
        let mut repeats = Repeats::default();
        repeats.released(N_KEY, 760);
        assert!(!repeats.is_repeat(N_KEY, 900));
    }

    #[test]
    fn a_press_of_another_key_is_not_the_repeat_of_the_release() {
        let mut repeats = Repeats::default();
        repeats.released(N_KEY, 760);
        assert!(!repeats.is_repeat(SPACE_KEY, 760));
    }

    #[test]
    fn a_release_is_the_other_half_of_one_press_only() {
        let mut repeats = Repeats::default();
        repeats.released(N_KEY, 760);
        assert!(repeats.is_repeat(N_KEY, 760));
        assert!(
            !repeats.is_repeat(N_KEY, 760),
            "the same pair is not taken twice"
        );
    }

    #[test]
    fn the_options_are_the_fifth_string_of_the_property() {
        let property = b"evdev\0pc105\0us,ru\0\0grp:alt_shift_toggle";
        assert_eq!(
            xkb_options(property).as_deref(),
            Some("grp:alt_shift_toggle")
        );
        let with_end = b"evdev\0pc105\0us\0\0grp:caps_toggle,ctrl:nocaps\0";
        assert_eq!(
            xkb_options(with_end).as_deref(),
            Some("grp:caps_toggle,ctrl:nocaps")
        );
    }

    #[test]
    fn a_property_with_no_options_string_says_none() {
        assert_eq!(xkb_options(b""), None);
        assert_eq!(xkb_options(b"evdev\0pc105\0us"), None);
        assert_eq!(xkb_options(b"evdev\0pc105\0us\0").as_deref(), None);
        assert_eq!(
            xkb_options(b"evdev\0pc105\0us\0\0").as_deref(),
            Some(""),
            "an empty one is no option"
        );
    }

    #[test]
    fn a_property_that_is_not_text_does_not_stop_the_reading() {
        let property = b"evdev\0pc105\0us\0\0grp:\xFF_toggle";
        assert!(xkb_options(property).is_some());
    }

    fn alt_shift_n() -> Hotkey {
        hotkey(false, true, true, false, 0x4E)
    }

    #[test]
    fn alt_shift_toggle_takes_every_chord_with_alt_and_shift() {
        for vk in [0x4E, 0x20, 0x30] {
            let chord = hotkey(false, true, true, false, vk);
            assert_eq!(
                layout_toggle("grp:alt_shift_toggle", &chord),
                Some("grp:alt_shift_toggle"),
                "{vk:#x}"
            );
        }
    }

    #[test]
    fn a_chord_is_taken_only_when_it_holds_both_modifiers_of_the_toggle() {
        let options = "grp:alt_shift_toggle";
        assert_eq!(
            layout_toggle(options, &hotkey(true, true, false, false, 0x4E)),
            None
        );
        assert_eq!(
            layout_toggle(options, &hotkey(false, true, false, false, 0x4E)),
            None
        );
        assert_eq!(
            layout_toggle(options, &hotkey(true, false, true, false, 0x4E)),
            None
        );
        assert!(
            layout_toggle(options, &hotkey(true, true, true, false, 0x4E)).is_some(),
            "a third modifier does not save a chord that holds both"
        );
    }

    #[test]
    fn the_toggles_measured_take_their_own_two_modifiers() {
        let cases = [
            (
                "grp:lalt_lshift_toggle",
                hotkey(false, true, true, false, 0x4E),
            ),
            (
                "grp:ctrl_shift_toggle",
                hotkey(true, false, true, false, 0x4E),
            ),
            (
                "grp:ctrl_alt_toggle",
                hotkey(true, true, false, false, 0x4E),
            ),
        ];
        for (option, chord) in cases {
            assert_eq!(layout_toggle(option, &chord), Some(option), "{option}");
        }
    }

    #[test]
    fn a_toggle_of_other_modifiers_leaves_the_chord_alone() {
        assert_eq!(layout_toggle("grp:ctrl_shift_toggle", &alt_shift_n()), None);
        assert_eq!(layout_toggle("grp:ctrl_alt_toggle", &alt_shift_n()), None);
        assert_eq!(layout_toggle("grp:win_space_toggle", &alt_shift_n()), None);
    }

    #[test]
    fn the_sides_of_a_toggle_are_the_same_mechanism() {
        assert!(
            layout_toggle(
                "grp:rctrl_rshift_toggle",
                &hotkey(true, false, true, false, 0x4E)
            )
            .is_some()
        );
        assert!(
            layout_toggle(
                "grp:lctrl_lshift_toggle",
                &hotkey(true, false, true, false, 0x4E)
            )
            .is_some()
        );
        assert!(layout_toggle("grp:ralt_rshift_toggle", &alt_shift_n()).is_some());
    }

    #[test]
    fn alt_space_toggle_takes_alt_and_space_alone() {
        let option = "grp:alt_space_toggle";
        assert_eq!(
            layout_toggle(option, &hotkey(false, true, false, false, 0x20)),
            Some(option)
        );
        assert_eq!(
            layout_toggle(option, &hotkey(false, true, true, false, 0x20)),
            None,
            "measured: with Shift the chord is heard, and the layout changes too"
        );
        assert_eq!(layout_toggle(option, &alt_shift_n()), None);
    }

    #[test]
    fn a_toggle_on_a_single_key_or_a_key_of_its_own_takes_no_chord() {
        for option in [
            "grp:caps_toggle",
            "grp:shifts_toggle",
            "grp:lwin_toggle",
            "grp:win_space_toggle",
            "grp:alt_altgr_toggle",
            "grp:toggle",
            "grp:_toggle",
            "grp:alt_shift_ctrl_toggle",
        ] {
            let chord = hotkey(true, true, true, true, 0x20);
            assert_eq!(layout_toggle(option, &chord), None, "{option}");
        }
    }

    #[test]
    fn options_that_are_not_about_the_layout_are_not_toggles() {
        for options in [
            "",
            "ctrl:nocaps",
            "caps:escape,compose:ralt",
            "alt_shift_toggle",
            "xgrp:alt_shift_toggle",
            "grp:alt_shift",
        ] {
            assert_eq!(layout_toggle(options, &alt_shift_n()), None, "{options:?}");
        }
    }

    #[test]
    fn the_toggle_is_found_among_other_options_with_spaces_around_it() {
        assert_eq!(
            layout_toggle(
                "ctrl:nocaps, grp:alt_shift_toggle ,compose:ralt",
                &alt_shift_n()
            ),
            Some("grp:alt_shift_toggle")
        );
    }

    #[test]
    fn the_first_toggle_that_takes_the_chord_is_the_one_named() {
        assert_eq!(
            layout_toggle("grp:ctrl_shift_toggle,grp:alt_shift_toggle", &alt_shift_n()),
            Some("grp:alt_shift_toggle")
        );
    }
}
