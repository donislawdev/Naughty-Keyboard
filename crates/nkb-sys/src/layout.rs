//! What the keyboard layouts type for a key held with `Ctrl` and `Alt`.
//!
//! # Why the palette asks
//!
//! On Windows `AltGr` IS `Ctrl+Alt` (`D82`): a global shortcut `Ctrl+Alt+A`
//! takes the key combination from every application, and on a layout that
//! types a character there - `U+0105` on the Polish programmer's layout - the tester
//! loses that character everywhere, the field under test included. Whether a
//! layout types something there is a fact of the layout, not of the chord, so
//! the core asks and this module answers.
//!
//! # The measurement this stands on
//!
//! Measured 2026-09-29 on the Polish programmer's layout (`0x04150415`, the
//! only one installed): `Ctrl+Alt` with `A C E L N O S U X Z` types
//! nine letters with a diacritic from `U+0105` to `U+017C` and `U+20AC`, with `Shift` the
//! capitals, and digits, `F1`-`F24`
//! and Space type nothing - ten of the sixty-one shortcut keys. The same answer
//! came back when asked twice in a row, so the question leaves no trace.
//!
//! # How it asks, and what it does not touch
//!
//! `ToUnicodeEx` with bit 2 of its flags, which Microsoft documents as "the
//! keyboard state is not changed" (Windows 10 1607 and later): a dead key asked
//! about does not arm itself for the tester's next letter. The keyboard state
//! handed in is a local array - nothing is pressed and nothing is read from any
//! window. Every INSTALLED layout is asked, not only the active one, because the
//! tester can switch layouts while the palette runs and the shortcut would then
//! take the character of the other one.

/// The first character any installed layout types for the virtual key `vk`
/// held with `Ctrl` and `Alt` - and `Shift` when `shift` - or `None` when no
/// layout types anything there or the layouts could not be read.
///
/// `None` is "no reason to refuse", deliberately not split from "could not
/// tell": the caller refuses a shortcut only on a character it can name, never
/// on a guess. Where `AltGr` is not `Ctrl+Alt` - Linux, macOS - the answer is
/// always `None`, and true.
#[must_use]
pub fn altgr_character(vk: u16, shift: bool) -> Option<char> {
    platform::altgr_character(vk, shift)
}

/// What one `ToUnicodeEx` answer types, apart from the reading, so it can be
/// checked on every system.
///
/// `written` is the function's result: the number of UTF-16 units it wrote,
/// zero for nothing, negative for a dead key - which then left its own
/// character in the buffer, and counts, because the tester would lose the dead
/// key too. A control character is not typing: a layout without `AltGr` may
/// answer `Ctrl+Alt+A` with `U+0001`, which no application inserts.
fn typed_character(written: i32, buffer: &[u16]) -> Option<char> {
    let units = match written {
        0 => return None,
        written if written < 0 => 1,
        written => usize::try_from(written).unwrap_or(usize::MAX),
    };
    let units = buffer.get(..units.min(buffer.len()))?;
    let first = char::decode_utf16(units.iter().copied()).next()?.ok()?;
    (!first.is_control()).then_some(first)
}

#[cfg(windows)]
mod platform {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetKeyboardLayoutList, HKL, MAPVK_VK_TO_VSC, MapVirtualKeyExW, ToUnicodeEx, VK_CONTROL,
        VK_LCONTROL, VK_LSHIFT, VK_MENU, VK_RMENU, VK_SHIFT,
    };

    /// More layouts than anyone installs. A larger answer is not trusted as a
    /// length, the same rule as `privilege::LABEL_LIMIT`.
    const LAYOUT_LIMIT: i32 = 64;

    /// "Leave the keyboard state as it is" - bit 2 of `ToUnicodeEx`'s flags.
    const KEEP_STATE: u32 = 0x4;

    /// The state `AltGr` is: `Ctrl` and `Alt` held, the left `Ctrl` and the
    /// right `Alt` as the system reports `AltGr` itself - the combination the
    /// measurement in the module header used.
    fn altgr_state(shift: bool) -> [u8; 256] {
        let mut state = [0u8; 256];
        for key in [VK_CONTROL, VK_LCONTROL, VK_MENU, VK_RMENU] {
            state[usize::from(key)] = 0x80;
        }
        if shift {
            for key in [VK_SHIFT, VK_LSHIFT] {
                state[usize::from(key)] = 0x80;
            }
        }
        state
    }

    fn layouts() -> Vec<HKL> {
        let count = unsafe { GetKeyboardLayoutList(0, core::ptr::null_mut()) };
        if count <= 0 || count > LAYOUT_LIMIT {
            return Vec::new();
        }
        let mut list: Vec<HKL> = vec![core::ptr::null_mut(); usize::try_from(count).unwrap_or(0)];
        let filled = unsafe { GetKeyboardLayoutList(count, list.as_mut_ptr()) };
        list.truncate(usize::try_from(filled).unwrap_or(0));
        list
    }

    pub fn altgr_character(vk: u16, shift: bool) -> Option<char> {
        let state = altgr_state(shift);
        layouts().into_iter().find_map(|layout| {
            let scan = unsafe { MapVirtualKeyExW(u32::from(vk), MAPVK_VK_TO_VSC, layout) };
            let mut buffer = [0u16; 8];
            let written = unsafe {
                ToUnicodeEx(
                    u32::from(vk),
                    scan,
                    state.as_ptr(),
                    buffer.as_mut_ptr(),
                    8,
                    KEEP_STATE,
                    layout,
                )
            };
            super::typed_character(written, &buffer)
        })
    }
}

#[cfg(not(windows))]
mod platform {
    // `AltGr` is its own key here (`ISO_Level3_Shift` on Linux, `Option` on
    // macOS), not `Ctrl+Alt`, so a `Ctrl+Alt` shortcut takes no character from a
    // layout. `None` is the true answer, not a missing one.
    pub fn altgr_character(_vk: u16, _shift: bool) -> Option<char> {
        None
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

    #[test]
    fn a_character_a_dead_key_and_a_surrogate_pair_are_typing() {
        // `U+0105` as the Polish layout gives it for `Ctrl+Alt+A`.
        assert_eq!(typed_character(1, &[0x0105, 0]), Some('\u{105}'));
        // A dead key: the count is negative and its own character is left in
        // the buffer. The tester would lose it, so it counts.
        assert_eq!(typed_character(-1, &[0x00B4, 0]), Some('\u{B4}'));
        // A character outside the Basic Multilingual Plane is two units.
        assert_eq!(typed_character(2, &[0xD83D, 0xDE00, 0]), Some('\u{1F600}'));
        // Two characters for one key: the first names it.
        assert_eq!(typed_character(2, &[0x0061, 0x0062]), Some('a'));
    }

    #[test]
    fn nothing_a_control_character_and_a_broken_answer_are_not_typing() {
        assert_eq!(typed_character(0, &[0x0105]), None);
        // `Ctrl+Alt+A` on a layout without `AltGr` may give `U+0001`.
        assert_eq!(typed_character(1, &[0x0001]), None);
        // A lone surrogate is not a character.
        assert_eq!(typed_character(1, &[0xD83D]), None);
        // A count past the buffer reads only the buffer.
        assert_eq!(typed_character(9, &[0x0105]), Some('\u{105}'));
        assert_eq!(typed_character(1, &[]), None);
    }

    #[test]
    fn a_function_key_types_nothing_under_altgr_on_any_installed_layout() {
        // Reads this machine's layouts - no window, no key pressed. No layout
        // puts a character on F1-F24, so a `Some` here means the reading took
        // something else for a character.
        for vk in 0x70u16..=0x87 {
            assert_eq!(altgr_character(vk, false), None, "vk 0x{vk:X}");
            assert_eq!(altgr_character(vk, true), None, "vk 0x{vk:X} with Shift");
        }
    }

    #[test]
    fn asking_twice_gives_the_same_answer() {
        // Bit 2 keeps the keyboard state. Without it a dead key asked about
        // would arm itself, and the second answer would differ from the first.
        // ⚠️ It has teeth only where an installed layout has a dead key under
        // `AltGr` - the Polish programmer's layout has none, so on the machine
        // this was written on, the test passes with the bit removed too.
        for vk in (0x41u16..=0x5A).chain(0x30..=0x39) {
            assert_eq!(
                altgr_character(vk, false),
                altgr_character(vk, false),
                "vk 0x{vk:X}"
            );
        }
    }
}
