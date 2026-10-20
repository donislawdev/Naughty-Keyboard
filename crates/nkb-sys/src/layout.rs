//! What the keyboard layouts type for a key held with `Ctrl` and `Alt` (Windows)
//! or with `Option` (macOS).
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
//!
//! # macOS: `Option` is the key that types (`D141`)
//!
//! There `AltGr` does not exist, and the key that makes a layout type a second
//! character is `Option` (`Option+A` is `U+0105` on the Polish layout, `U+00E5`
//! on the U.S. one). Measured 2026-10-21 on macOS 27.0 with the layouts of the
//! system: on each of the 6 layouts asked, `Option` and `Option+Shift` type a
//! character on nearly every letter and digit (a few keys are dead keys, see
//! below) and on Space (`U+00A0`), while the function keys give `U+0010`, a control
//! character, and `Command+Option` gives nothing a field takes. Typed into a
//! field with the Polish layout, `Option+A` made `U+0105`, `Option+Shift+E`
//! `U+0118` and `Option+Space` a space, and a shortcut on `Option+A` or on
//! `Option+Shift+A` took the character from the field (3 presses of 3 each).
//!
//! `UCKeyTranslate` answers it from the layout's own table, with `Option` as the
//! modifier state and nothing pressed. A dead key (`Option+E` on the U.S. layout)
//! writes nothing and arms an accent, which the tester would lose with the
//! shortcut, so it counts, and its character is what Space makes of it. Every
//! ENABLED layout is asked, as on Windows. The layouts are read through Text
//! Input Sources, which the header says belongs to the main thread, so they are
//! copied there and the copy answers from any thread (`macos/layout.rs`).

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

/// The first character any enabled keyboard layout types for the key with the
/// virtual key `vk` held with `Option` - and `Shift` when `shift` - or `None`
/// when no layout types anything there or the layouts could not be read.
///
/// Like [`altgr_character`], `None` is "no reason to refuse", and on every
/// system but macOS it is the true answer: `Option` is the key of the Mac.
/// `vk` is the vocabulary of the shortcuts (`HotkeyKey`), as there.
#[must_use]
pub fn option_character(vk: u16, shift: bool) -> Option<char> {
    #[cfg(target_os = "macos")]
    {
        crate::macos::layout::option_character(vk, shift)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (vk, shift);
        None
    }
}

/// Reads the keyboard layouts again, where reading them belongs to one thread: on
/// macOS the main thread, which the next question from any thread then answers
/// from. A thread that is not the main one reads nothing, and elsewhere there is
/// nothing to read - the layouts are asked at every question.
///
/// Called where the tester is about to choose a shortcut, so a layout enabled
/// while the palette runs is among those asked.
pub fn refresh_layouts() {
    #[cfg(target_os = "macos")]
    crate::macos::layout::refresh();
}

/// What one `ToUnicodeEx` answer types, apart from the reading, so it can be
/// checked on every system.
///
/// `written` is the function's result: the number of UTF-16 units it wrote,
/// zero for nothing, negative for a dead key - which then left its own
/// character in the buffer, and counts, because the tester would lose the dead
/// key too. A control character is not typing: a layout without `AltGr` may
/// answer `Ctrl+Alt+A` with `U+0001`, which no application inserts.
#[cfg(any(windows, target_os = "macos", test))]
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

/// What one `UCKeyTranslate` answer was: the units it wrote and the dead key
/// state it left. A dead key writes nothing and leaves a state.
#[cfg(any(target_os = "macos", test))]
pub(crate) struct Translated {
    pub units: Vec<u16>,
    pub dead: u32,
}

/// The modifier state `UCKeyTranslate` takes for `Option`, and for `Option` with
/// `Shift`: the Carbon bits (`optionKey` 0x800, `shiftKey` 0x200) shifted down by
/// eight, as the header of `UnicodeUtilities.h` says.
#[cfg(any(target_os = "macos", test))]
const fn option_state(shift: bool) -> u32 {
    if shift { 0x0A } else { 0x08 }
}

/// The key code of Space, which completes a dead key into its accent.
#[cfg(any(target_os = "macos", test))]
const SPACE_KEY_CODE: u16 = 49;

/// The character `Option` types on the key with macOS key code `code`, from
/// `translate` - the layout's answer for a key code, a modifier state and the
/// dead key state going in. Apart from the reading, so every case can be checked
/// on every system.
///
/// A dead key counts: it writes nothing, arms an accent and the tester would
/// lose it with the shortcut, so the character is what Space makes of it. A
/// control character is not typing (the function keys answer `U+0010`), and
/// neither is a key the layout cannot answer for.
#[cfg(any(target_os = "macos", test))]
pub(crate) fn option_character_via(
    translate: &dyn Fn(u16, u32, u32) -> Option<Translated>,
    code: u16,
    shift: bool,
) -> Option<char> {
    let first = translate(code, option_state(shift), 0)?;
    let units = if first.dead == 0 {
        first.units
    } else {
        translate(SPACE_KEY_CODE, 0, first.dead)?.units
    };
    typed_character(i32::try_from(units.len()).unwrap_or(i32::MAX), &units)
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

    /// A layout as `UCKeyTranslate` is asked about it: only the answers listed
    /// are given, every other question gets no answer at all.
    fn answering(
        known: Vec<((u16, u32, u32), Translated)>,
    ) -> impl Fn(u16, u32, u32) -> Option<Translated> {
        move |code, state, dead| {
            known
                .iter()
                .find(|(key, _)| *key == (code, state, dead))
                .map(|(_, answer)| Translated {
                    units: answer.units.clone(),
                    dead: answer.dead,
                })
        }
    }

    fn typed(units: &[u16]) -> Translated {
        Translated {
            units: units.to_vec(),
            dead: 0,
        }
    }

    #[test]
    fn option_types_what_the_layout_gives_on_the_key_and_the_capital_with_shift() {
        // `A` is key code 0. The states are Carbon's `optionKey` and
        // `optionKey | shiftKey` shifted down by eight.
        let layout = answering(vec![
            ((0, 0x08, 0), typed(&[0x0105])),
            ((0, 0x0A, 0), typed(&[0x0104])),
        ]);
        assert_eq!(option_character_via(&layout, 0, false), Some('\u{105}'));
        assert_eq!(option_character_via(&layout, 0, true), Some('\u{104}'));
        assert_eq!(option_character_via(&layout, 11, false), None);
    }

    #[test]
    fn a_dead_key_counts_and_is_the_accent_space_makes_of_it() {
        // `Option+E` on the U.S. layout writes nothing and arms state 0x1000.
        // Space with no modifier completes it to `U+00B4`.
        let layout = answering(vec![
            (
                (14, 0x08, 0),
                Translated {
                    units: Vec::new(),
                    dead: 0x1000,
                },
            ),
            ((49, 0, 0x1000), typed(&[0x00B4])),
        ]);
        assert_eq!(option_character_via(&layout, 14, false), Some('\u{B4}'));
    }

    #[test]
    fn a_dead_key_nothing_completes_is_not_named() {
        let dead = || Translated {
            units: Vec::new(),
            dead: 0x1000,
        };
        let unanswered = answering(vec![((14, 0x08, 0), dead())]);
        assert_eq!(option_character_via(&unanswered, 14, false), None);
        let empty = answering(vec![((14, 0x08, 0), dead()), ((49, 0, 0x1000), typed(&[]))]);
        assert_eq!(option_character_via(&empty, 14, false), None);
    }

    #[test]
    fn a_function_key_a_control_character_and_silence_are_not_typing() {
        // The function keys answer `U+0010` under `Option` on every layout.
        let layout = answering(vec![
            ((122, 0x08, 0), typed(&[0x0010])),
            ((0, 0x08, 0), typed(&[])),
        ]);
        assert_eq!(option_character_via(&layout, 122, false), None);
        assert_eq!(option_character_via(&layout, 0, false), None);
    }

    #[test]
    fn a_character_that_is_no_dead_key_is_asked_once() {
        let asked = std::cell::Cell::new(0);
        let counting = |_: u16, _: u32, _: u32| {
            asked.set(asked.get() + 1);
            Some(typed(&[0x00E5]))
        };
        assert_eq!(option_character_via(&counting, 0, false), Some('\u{E5}'));
        assert_eq!(asked.get(), 1);
    }

    #[test]
    fn option_is_asked_of_no_system_but_the_mac() {
        if cfg!(target_os = "macos") {
            return;
        }
        for vk in 0x20u16..=0x87 {
            assert_eq!(option_character(vk, false), None, "vk 0x{vk:X}");
            assert_eq!(option_character(vk, true), None, "vk 0x{vk:X} with Shift");
        }
    }
}
