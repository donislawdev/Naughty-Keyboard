//! The keyboard events of the Windows route, built and not yet sent: the keys of
//! the clearing recipe and the steps of one UTF-16 unit of a value. `lib.rs`
//! sends them, one at a time, through `SendInput` (`confirm.rs`).

use crate::confirm::Step;
use crate::text_keys::{TextKey, text_key};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP,
    KEYEVENTF_UNICODE, VIRTUAL_KEY, VK_PACKET, VK_RETURN, VK_SHIFT, VK_TAB,
};

/// One virtual-key event. `KEYEVENTF_EXTENDEDKEY` marks the navigation
/// cluster, so an application that tells `Home` from the numeric keypad's
/// `7` sees the former.
fn key_event(key: VIRTUAL_KEY, flags: u32) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: key,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

/// The event for `key`, down or up. Navigation keys carry
/// `KEYEVENTF_EXTENDEDKEY`, `Shift` does not.
pub(crate) fn chord_event(key: VIRTUAL_KEY, release: bool) -> INPUT {
    let extended = if key == VK_SHIFT {
        0
    } else {
        KEYEVENTF_EXTENDEDKEY
    };
    key_event(key, extended | if release { KEYEVENTF_KEYUP } else { 0 })
}

/// One UTF-16 unit as the steps that press it. A tab and a line feed are the
/// Tab and Return keys (`text_keys.rs`), which the person's `Shift` changes, so
/// it holds them back. Every other unit is a character, which arrives as itself
/// under `Shift` (`OBS-233`).
pub(crate) fn steps_for(unit: u16) -> [Step<INPUT>; 2] {
    let (key, [down, up], shift_counts) = match text_key(u32::from(unit)) {
        Some(TextKey::Tab) => (VK_TAB, key_events(VK_TAB), true),
        Some(TextKey::Return) => (VK_RETURN, key_events(VK_RETURN), true),
        None => (VK_PACKET, events_for(unit), false),
    };
    [
        Step {
            event: down,
            key,
            down: true,
            shift_counts,
        },
        Step {
            event: up,
            key,
            down: false,
            shift_counts,
        },
    ]
}

/// The main Tab or Return key, down and up. Neither is of the navigation
/// cluster, so neither is extended (the Return of the numeric keypad is).
fn key_events(key: VIRTUAL_KEY) -> [INPUT; 2] {
    [key_event(key, 0), key_event(key, KEYEVENTF_KEYUP)]
}

/// One UTF-16 unit as the pair of events the system expects.
fn events_for(unit: u16) -> [INPUT; 2] {
    let event = |flags: u32| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                // Zero: the unit below is a character, not a key. That is
                // what KEYEVENTF_UNICODE means, and it is why layout does
                // not matter - the probe sent CJK on a Polish keyboard.
                wVk: 0,
                wScan: unit,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    [
        event(KEYEVENTF_UNICODE),
        event(KEYEVENTF_UNICODE | KEYEVENTF_KEYUP),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tab_and_a_line_feed_are_their_keys_and_wait_for_the_person_s_shift() {
        for (unit, key) in [(0x09, VK_TAB), (0x0A, VK_RETURN)] {
            let [down, up] = steps_for(unit);
            assert_eq!((down.key, up.key), (key, key), "{unit:#x}");
            assert!(down.down && !up.down);
            assert!(down.shift_counts && up.shift_counts, "{unit:#x}");
        }
    }

    #[test]
    fn every_other_unit_is_a_character_the_person_s_shift_does_not_change() {
        for unit in [0x0D, 0x20, 0x41, 0x0105, 0x2028, 0xD83D] {
            let [down, up] = steps_for(unit);
            assert_eq!((down.key, up.key), (VK_PACKET, VK_PACKET), "{unit:#x}");
            assert!(!down.shift_counts && !up.shift_counts, "{unit:#x}");
        }
    }
}
