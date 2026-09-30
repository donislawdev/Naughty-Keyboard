//! Which keys are held down while a window of ours handles a key press.
//!
//! # Why the window asks the system
//!
//! The shortcuts window records a global shortcut, and a global shortcut is a
//! KEY - a virtual-key code - with modifiers, never a character. Slint 1.18.1
//! hands a press over as text only (`slint.md` 2.29): `Alt+Shift+1` arrives as
//! `!`, and the left `Ctrl` with `Alt` over a letter a layout types under
//! `AltGr` arrives as that letter with no modifier at all. Its modifier flags
//! can also be stale, after the system menu swallowed a release
//! (`slint.md` 2.37). So the key is asked of the system, at the moment the
//! press is handled.
//!
//! # What it reads, and what it does not
//!
//! `GetKeyState`: the key state of THIS THREAD, as of the keyboard message the
//! thread is handling now. It changes only as this thread reads keyboard
//! messages, which is input sent to a window of ours. 🔴 Never
//! `GetAsyncKeyState`, which answers for the whole system at any moment: that
//! would be reading what the tester types into other applications, which this
//! tool promises never to do.
//!
//! # The measurement this stands on
//!
//! Measured 2026-09-30 with `tools/sonda-klawisze` (binary `ktory`) on the real
//! shortcuts window, the Polish programmer's layout, twelve synthetic chords
//! pressed one event at a time and one physical run: asked from inside the
//! window's key handler, this named the chord correctly every time -
//! `Alt+Shift+1` (text `!`), the left `Ctrl+Alt+A` and `AltGr+A` (text
//! `U+0105`, both read as `Ctrl+Alt+A`), `Ctrl+Alt+F5`, `F10`, `Shift+Q`.

/// The modifiers held, and which of the keys asked about are held too.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Held {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    /// Either Windows key.
    pub win: bool,
    /// The keys asked about that are down, in the order they were asked about.
    pub keys: Vec<u16>,
}

/// What is held down as this thread handles the key press in front of it,
/// among `keys` - virtual-key codes.
///
/// Meaningful only inside the handling of a key press on the thread that owns
/// the window. `None` where the system has no such question: on macOS and
/// Linux the shortcut vocabulary is not virtual-key codes, and the answer
/// there has yet to be measured.
#[must_use]
pub fn held(keys: &[u16]) -> Option<Held> {
    platform::held(keys)
}

/// Whether a `GetKeyState` answer says the key is down: the high-order bit,
/// which makes the value negative. The low-order bit is the toggle of a lock
/// key and says nothing about being held.
#[cfg_attr(not(windows), allow(dead_code))]
const fn is_down(state: i16) -> bool {
    state < 0
}

#[cfg(windows)]
mod platform {
    use super::Held;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetKeyState, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
    };

    fn down(vk: u16) -> bool {
        super::is_down(unsafe { GetKeyState(i32::from(vk)) })
    }

    pub fn held(keys: &[u16]) -> Option<Held> {
        Some(Held {
            ctrl: down(VK_CONTROL),
            alt: down(VK_MENU),
            shift: down(VK_SHIFT),
            win: down(VK_LWIN) || down(VK_RWIN),
            keys: keys.iter().copied().filter(|key| down(*key)).collect(),
        })
    }
}

#[cfg(not(windows))]
mod platform {
    use super::Held;

    pub fn held(_keys: &[u16]) -> Option<Held> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_high_bit_means_held() {
        // Held, held and toggled, not held, not held but toggled (Caps Lock on).
        assert!(is_down(i16::MIN));
        assert!(is_down(i16::MIN | 1));
        assert!(!is_down(0));
        assert!(!is_down(1));
    }

    #[test]
    fn a_system_without_the_question_says_so() {
        // Where the answer exists it has one entry per key held, never more
        // keys than were asked about.
        let answer = held(&[0x41, 0x42]);
        assert_eq!(answer.is_some(), cfg!(windows), "{answer:?}");
        if let Some(answer) = answer {
            assert!(answer.keys.len() <= 2);
            assert!(answer.keys.iter().all(|key| [0x41, 0x42].contains(key)));
        }
    }
}
