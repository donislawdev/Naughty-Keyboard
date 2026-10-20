//! A global shortcut on macOS, the decisions that need no system to make.
//!
//! Everything here is a function of its arguments, so it is tested on every
//! system, while only macOS feeds it. The calls themselves are `macos/hotkey.rs`.
//!
//! # What was measured before this was written
//!
//! Probed 2026-10-20 on macOS 27.0 (arm64) with `tools/sonda-paleta-macos`, a
//! Slint 1.18.1 window on winit 0.30.13 in a bundle of its own, the chord posted
//! by `tools/sondy/wpisywanie-macos.m` (`akord`) at the HID tap, a victim window
//! that logs every key it receives (`D138`):
//!
//! - `RegisterEventHotKey` called before the application loop exists, or from a
//!   thread that is not the main one, is heard: 3 presses of 3 either way. The
//!   handler runs on the thread that pulls events, and with the winit loop on the
//!   main thread that is the main thread - not the thread that registered.
//! - the victim got none of the 3 chords when the palette held them and all 3
//!   when the palette held another key (`U+006E` six times, down and up).
//! - 1 press and 6 auto-repeat key-downs of the same key made ONE event. There is
//!   no `MOD_NOREPEAT` to ask for and none is needed.
//! - a second process registering the same chord is told `noErr`, and BOTH
//!   processes hear every press (3 of 3 in each). Registration cannot say that an
//!   application of somebody else's holds the chord: `Taken` is only ever the
//!   answer for a chord this process holds already.
//!
//! The key codes are those of the SDK header (`Events.h`, `kVK_ANSI_*` and
//! `kVK_F*`), printed by a C program on the same machine and written out in the
//! tests - not typed from memory.

use crate::hotkey::HotkeyRegistration;

/// The two high bytes of the signature of every hot key a listener registers,
/// `NK` in capitals. The two low bytes are the serial of the listener that
/// registered it, so a press finds its listener without any state kept beside the
/// system's own. Not `nk`: the `Escape` reservation signs with `nkbe`
/// (`escape.rs`), which would then read as the listener numbered 25 189 and its
/// press would be taken for a shortcut.
const SIGNATURE_PREFIX: u32 = 0x4E4B_0000;

/// `eventHotKeyExistsErr`: this process holds the chord already.
const EXISTS: i32 = -9878;

/// What `Failed` carries for a key the macOS table has no code for - not a
/// system status, and out of the range of any (`OSStatus` values of Carbon are
/// small negative numbers).
pub(crate) const NO_SUCH_KEY: u32 = 0x4E4B_0001;

/// The signature a listener registers its hot keys with.
pub(crate) const fn signature(serial: u16) -> u32 {
    SIGNATURE_PREFIX | serial as u32
}

/// The serial of the listener a signature belongs to, or `None` for a hot key
/// that is not ours (the `Escape` reservation, another library's).
pub(crate) const fn serial_of(signature: u32) -> Option<u16> {
    if signature & 0xFFFF_0000 == SIGNATURE_PREFIX {
        Some((signature & 0xFFFF) as u16)
    } else {
        None
    }
}

/// `cmdKey`, `shiftKey`, `optionKey` and `controlKey` of `Events.h`, combined.
/// `win` is the Command key: the vocabulary of the shortcuts has one name for
/// the key a system puts next to the space bar (`D82`).
pub(crate) const fn modifiers(ctrl: bool, alt: bool, shift: bool, win: bool) -> u32 {
    let mut bits = 0;
    if win {
        bits |= 0x0100;
    }
    if shift {
        bits |= 0x0200;
    }
    if alt {
        bits |= 0x0800;
    }
    if ctrl {
        bits |= 0x1000;
    }
    bits
}

/// The virtual key codes of `A` to `Z`, in that order.
const LETTERS: [u32; 26] = [
    0, 11, 8, 2, 14, 3, 5, 4, 34, 38, 40, 37, 46, 45, 31, 35, 12, 15, 1, 17, 32, 9, 13, 7, 16, 6,
];
/// The virtual key codes of `0` to `9`, in that order.
const DIGITS: [u32; 10] = [29, 18, 19, 20, 21, 23, 22, 26, 28, 25];
/// The virtual key codes of `F1` to `F20`, in that order. The header has none
/// for the four above.
const FUNCTION: [u32; 20] = [
    122, 120, 99, 118, 96, 97, 98, 100, 101, 109, 103, 111, 105, 107, 113, 106, 64, 79, 80, 90,
];
/// `kVK_Space`.
const SPACE: u32 = 49;

/// The macOS virtual key code for a key of the shortcut vocabulary, given as the
/// code the vocabulary is written in: letters and digits are their ASCII codes,
/// `F1` to `F24` run from `0x70`, and Space is `0x20`.
///
/// The codes are those of an ANSI keyboard, by POSITION: a shortcut on `N` is the
/// key where an ANSI `N` is, on every layout. `None` for a key the table has no
/// code for - `F21` to `F24` - which registration reports instead of guessing.
pub(crate) const fn key_code(vk: u16) -> Option<u32> {
    match vk {
        0x41..=0x5A => Some(LETTERS[(vk - 0x41) as usize]),
        0x30..=0x39 => Some(DIGITS[(vk - 0x30) as usize]),
        0x70..=0x83 => Some(FUNCTION[(vk - 0x70) as usize]),
        0x20 => Some(SPACE),
        _ => None,
    }
}

/// What the system's answer to registering one hot key means.
pub(crate) const fn registration(status: i32) -> HotkeyRegistration {
    match status {
        0 => HotkeyRegistration::Registered,
        EXISTS => HotkeyRegistration::Taken,
        other => HotkeyRegistration::Failed { code: other as u32 },
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

    /// Name, the code the vocabulary gives the key, and the code the SDK header
    /// gives it: printed on the Mac by a C program from `kVK_ANSI_A` and the
    /// rest, 2026-10-20.
    const HEADER: [(&str, u16, u32); 57] = [
        ("A", 0x41, 0),
        ("B", 0x42, 11),
        ("C", 0x43, 8),
        ("D", 0x44, 2),
        ("E", 0x45, 14),
        ("F", 0x46, 3),
        ("G", 0x47, 5),
        ("H", 0x48, 4),
        ("I", 0x49, 34),
        ("J", 0x4A, 38),
        ("K", 0x4B, 40),
        ("L", 0x4C, 37),
        ("M", 0x4D, 46),
        ("N", 0x4E, 45),
        ("O", 0x4F, 31),
        ("P", 0x50, 35),
        ("Q", 0x51, 12),
        ("R", 0x52, 15),
        ("S", 0x53, 1),
        ("T", 0x54, 17),
        ("U", 0x55, 32),
        ("V", 0x56, 9),
        ("W", 0x57, 13),
        ("X", 0x58, 7),
        ("Y", 0x59, 16),
        ("Z", 0x5A, 6),
        ("0", 0x30, 29),
        ("1", 0x31, 18),
        ("2", 0x32, 19),
        ("3", 0x33, 20),
        ("4", 0x34, 21),
        ("5", 0x35, 23),
        ("6", 0x36, 22),
        ("7", 0x37, 26),
        ("8", 0x38, 28),
        ("9", 0x39, 25),
        ("F1", 0x70, 122),
        ("F2", 0x71, 120),
        ("F3", 0x72, 99),
        ("F4", 0x73, 118),
        ("F5", 0x74, 96),
        ("F6", 0x75, 97),
        ("F7", 0x76, 98),
        ("F8", 0x77, 100),
        ("F9", 0x78, 101),
        ("F10", 0x79, 109),
        ("F11", 0x7A, 103),
        ("F12", 0x7B, 111),
        ("F13", 0x7C, 105),
        ("F14", 0x7D, 107),
        ("F15", 0x7E, 113),
        ("F16", 0x7F, 106),
        ("F17", 0x80, 64),
        ("F18", 0x81, 79),
        ("F19", 0x82, 80),
        ("F20", 0x83, 90),
        ("Space", 0x20, 49),
    ];

    #[test]
    fn every_key_has_the_code_the_sdk_header_gives_it() {
        for (name, vk, code) in HEADER {
            assert_eq!(key_code(vk), Some(code), "{name}");
        }
    }

    #[test]
    fn no_other_key_has_a_code_and_no_two_share_one() {
        let mut seen = std::collections::HashSet::new();
        for vk in 0..=u16::MAX {
            let Some(code) = key_code(vk) else {
                continue;
            };
            assert!(
                HEADER.iter().any(|(_, known, _)| *known == vk),
                "{vk:#x} has a code the header table does not list"
            );
            assert!(seen.insert(code), "{vk:#x} shares code {code}");
        }
        assert_eq!(seen.len(), HEADER.len());
    }

    #[test]
    fn the_four_function_keys_above_f20_have_no_code_to_register() {
        for vk in 0x84..=0x87 {
            assert_eq!(key_code(vk), None, "{vk:#x}");
        }
    }

    #[test]
    fn each_modifier_is_its_own_bit_and_the_command_key_is_win() {
        assert_eq!(modifiers(true, false, false, false), 0x1000);
        assert_eq!(modifiers(false, true, false, false), 0x0800);
        assert_eq!(modifiers(false, false, true, false), 0x0200);
        assert_eq!(modifiers(false, false, false, true), 0x0100);
        assert_eq!(modifiers(false, false, false, false), 0);
        assert_eq!(modifiers(true, true, true, true), 0x1B00);
        // Command with Option, the macOS default of every action (`D82`).
        assert_eq!(modifiers(false, true, false, true), 0x0900);
    }

    #[test]
    fn a_signature_finds_its_listener_and_nobody_elses_does() {
        for serial in [0u16, 1, 0x00FF, 0xFFFF] {
            assert_eq!(serial_of(signature(serial)), Some(serial));
        }
        // The `Escape` reservation (`nkbe`) and a stranger's signature.
        assert_eq!(serial_of(u32::from_be_bytes(*b"nkbe")), None);
        assert_eq!(serial_of(u32::from_be_bytes(*b"abcd")), None);
        assert_eq!(serial_of(0), None);
    }

    #[test]
    fn only_a_chord_this_process_holds_is_taken_and_every_other_failure_keeps_its_number() {
        assert_eq!(registration(0), HotkeyRegistration::Registered);
        assert_eq!(registration(-9878), HotkeyRegistration::Taken);
        assert_eq!(
            registration(-9879),
            HotkeyRegistration::Failed {
                code: (-9879i32) as u32
            }
        );
        assert_eq!(
            registration(-50),
            HotkeyRegistration::Failed {
                code: (-50i32) as u32
            }
        );
        assert_ne!(NO_SUCH_KEY, (-9878i32) as u32);
    }
}
