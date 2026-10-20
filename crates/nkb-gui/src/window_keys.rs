//! Which chord is held, read from the key events the shortcuts window itself is
//! given - the way macOS records a shortcut (`D142`).
//!
//! # Why the window follows its own events
//!
//! The shortcuts window records a global shortcut, and a global shortcut is a
//! KEY with modifiers, never a character. Slint hands a press over as text, and
//! the text is not the key: measured 2026-10-20 (macOS 27.0, Polish Pro layout,
//! Slint 1.18.1, winit 0.30.13), `Option+A` arrives as `U+0105` and
//! `Control+Option+1` as `1`, with Slint's own modifier flags swapped between
//! `Command` and `Control` on top (`slint.md` 2.29).
//!
//! On Windows the key is asked of the system, for the thread of the window
//! (`nkb_sys::held`). macOS has no such question that stays inside the window:
//! the key state of the whole session answers for every application at any
//! moment, which is reading what the tester types elsewhere, and this tool
//! promises never to do that. So the window follows what its own winit window is
//! given, and nothing else: the physical key of every press and release, and the
//! modifiers, from the filter on the window's events.
//!
//! # What winit gives, measured
//!
//! On the real window, with chords sent at the HID tap:
//!
//! - every press reaches the filter as a `KeyboardInput` event carrying the
//!   PHYSICAL key, before Slint hands the same press to the view. So when the
//!   view asks which chord is held, the press is already counted here. The
//!   physical key is the position of the key on the keyboard, the same notion
//!   the system registers a shortcut by, whatever the layout types on it.
//! - `ModifiersChanged` comes before the first key that has the modifier, and a
//!   key press refreshes the modifiers from its own event, so a modifier
//!   pressed before the window was active is known from the first key on.
//! - a key pressed with `Command` held also reports its release.
//! - the layout does not matter to a dead key: `Option+I` and `Option+U` on the
//!   Polish Pro layout, which arm an accent in a text field, reach the view like
//!   any other key.
//! - the application menu owns `Command+Q`, `Command+H` and `Option+Command+H`:
//!   they never reach the window (the first quits the palette, the third hides
//!   the other applications). They cannot be recorded here (`OBS-199`).
//!
//! # What it keeps
//!
//! The keys of the vocabulary that are down, and the modifiers. A key outside
//! the vocabulary is not kept: the answer for it is "no key", as on Windows. All
//! of it is forgotten when the window gains or loses the focus, because a
//! release that happens elsewhere is never seen here.

use nkb_adapters::ChordHeld;
use nkb_core::hotkeys::{HotkeyChord, HotkeyKey};
use slint::winit_030::winit::event::{ElementState, WindowEvent};
use slint::winit_030::winit::keyboard::{KeyCode, ModifiersState, PhysicalKey};

/// The physical key that stands for each key of the shortcut vocabulary.
///
/// Exhaustive on purpose: a key added to the vocabulary does not compile here
/// until it has a physical key. The other direction is searched in
/// [`hotkey_key`].
const fn key_code(key: HotkeyKey) -> KeyCode {
    match key {
        HotkeyKey::A => KeyCode::KeyA,
        HotkeyKey::B => KeyCode::KeyB,
        HotkeyKey::C => KeyCode::KeyC,
        HotkeyKey::D => KeyCode::KeyD,
        HotkeyKey::E => KeyCode::KeyE,
        HotkeyKey::F => KeyCode::KeyF,
        HotkeyKey::G => KeyCode::KeyG,
        HotkeyKey::H => KeyCode::KeyH,
        HotkeyKey::I => KeyCode::KeyI,
        HotkeyKey::J => KeyCode::KeyJ,
        HotkeyKey::K => KeyCode::KeyK,
        HotkeyKey::L => KeyCode::KeyL,
        HotkeyKey::M => KeyCode::KeyM,
        HotkeyKey::N => KeyCode::KeyN,
        HotkeyKey::O => KeyCode::KeyO,
        HotkeyKey::P => KeyCode::KeyP,
        HotkeyKey::Q => KeyCode::KeyQ,
        HotkeyKey::R => KeyCode::KeyR,
        HotkeyKey::S => KeyCode::KeyS,
        HotkeyKey::T => KeyCode::KeyT,
        HotkeyKey::U => KeyCode::KeyU,
        HotkeyKey::V => KeyCode::KeyV,
        HotkeyKey::W => KeyCode::KeyW,
        HotkeyKey::X => KeyCode::KeyX,
        HotkeyKey::Y => KeyCode::KeyY,
        HotkeyKey::Z => KeyCode::KeyZ,
        HotkeyKey::Digit0 => KeyCode::Digit0,
        HotkeyKey::Digit1 => KeyCode::Digit1,
        HotkeyKey::Digit2 => KeyCode::Digit2,
        HotkeyKey::Digit3 => KeyCode::Digit3,
        HotkeyKey::Digit4 => KeyCode::Digit4,
        HotkeyKey::Digit5 => KeyCode::Digit5,
        HotkeyKey::Digit6 => KeyCode::Digit6,
        HotkeyKey::Digit7 => KeyCode::Digit7,
        HotkeyKey::Digit8 => KeyCode::Digit8,
        HotkeyKey::Digit9 => KeyCode::Digit9,
        HotkeyKey::F1 => KeyCode::F1,
        HotkeyKey::F2 => KeyCode::F2,
        HotkeyKey::F3 => KeyCode::F3,
        HotkeyKey::F4 => KeyCode::F4,
        HotkeyKey::F5 => KeyCode::F5,
        HotkeyKey::F6 => KeyCode::F6,
        HotkeyKey::F7 => KeyCode::F7,
        HotkeyKey::F8 => KeyCode::F8,
        HotkeyKey::F9 => KeyCode::F9,
        HotkeyKey::F10 => KeyCode::F10,
        HotkeyKey::F11 => KeyCode::F11,
        HotkeyKey::F12 => KeyCode::F12,
        HotkeyKey::F13 => KeyCode::F13,
        HotkeyKey::F14 => KeyCode::F14,
        HotkeyKey::F15 => KeyCode::F15,
        HotkeyKey::F16 => KeyCode::F16,
        HotkeyKey::F17 => KeyCode::F17,
        HotkeyKey::F18 => KeyCode::F18,
        HotkeyKey::F19 => KeyCode::F19,
        HotkeyKey::F20 => KeyCode::F20,
        HotkeyKey::F21 => KeyCode::F21,
        HotkeyKey::F22 => KeyCode::F22,
        HotkeyKey::F23 => KeyCode::F23,
        HotkeyKey::F24 => KeyCode::F24,
        HotkeyKey::Space => KeyCode::Space,
    }
}

/// The key of the vocabulary a physical key is, if it is one. The number pad,
/// punctuation, the editing keys and the modifiers themselves are not.
fn hotkey_key(physical: PhysicalKey) -> Option<HotkeyKey> {
    let PhysicalKey::Code(code) = physical else {
        return None;
    };
    HotkeyKey::ALL
        .iter()
        .copied()
        .find(|key| key_code(*key) == code)
}

/// The keys of the vocabulary that are down, and the modifiers held with them.
#[derive(Debug, Default)]
pub struct KeyTracker {
    modifiers: ModifiersState,
    /// In the order they were pressed. A repeat of a key already down adds
    /// nothing.
    keys: Vec<HotkeyKey>,
}

impl KeyTracker {
    /// Takes one event of the window, and ignores the ones that say nothing
    /// about keys.
    pub fn observe(&mut self, event: &WindowEvent) {
        match event {
            WindowEvent::KeyboardInput { event, .. } => self.key(event.physical_key, event.state),
            WindowEvent::ModifiersChanged(modifiers) => self.modifiers(modifiers.state()),
            // A release that happens while another window is active is never
            // seen here, so what was down is forgotten.
            WindowEvent::Focused(_) => self.reset(),
            _ => {}
        }
    }

    /// A physical key went down or up.
    pub fn key(&mut self, physical: PhysicalKey, state: ElementState) {
        let Some(key) = hotkey_key(physical) else {
            return;
        };
        match state {
            ElementState::Pressed => {
                if !self.keys.contains(&key) {
                    self.keys.push(key);
                }
            }
            ElementState::Released => self.keys.retain(|down| *down != key),
        }
    }

    /// The modifiers are now `state`.
    pub fn modifiers(&mut self, state: ModifiersState) {
        self.modifiers = state;
    }

    /// Forgets every key and modifier.
    pub fn reset(&mut self) {
        self.keys.clear();
        self.modifiers = ModifiersState::empty();
    }

    /// The chord held now, in the shape the shortcuts window records. `win` is
    /// the Command key and `alt` the Option key, as everywhere on macOS.
    #[must_use]
    pub fn held(&self) -> ChordHeld {
        match self.keys.as_slice() {
            [] => ChordHeld::NoKey,
            [key] => ChordHeld::Chord(HotkeyChord {
                ctrl: self.modifiers.control_key(),
                alt: self.modifiers.alt_key(),
                shift: self.modifiers.shift_key(),
                win: self.modifiers.super_key(),
                key: *key,
            }),
            _ => ChordHeld::SeveralKeys,
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
    use slint::winit_030::winit::event::Modifiers;
    use slint::winit_030::winit::keyboard::NativeKeyCode;

    use super::*;

    fn code(code: KeyCode) -> PhysicalKey {
        PhysicalKey::Code(code)
    }

    fn chord(text: &str) -> ChordHeld {
        ChordHeld::Chord(HotkeyChord::parse(text).expect("a chord"))
    }

    /// A key with no modifier.
    fn bare(key: HotkeyKey) -> ChordHeld {
        ChordHeld::Chord(HotkeyChord {
            ctrl: false,
            alt: false,
            shift: false,
            win: false,
            key,
        })
    }

    fn press(tracker: &mut KeyTracker, key: KeyCode) {
        tracker.key(code(key), ElementState::Pressed);
    }

    #[test]
    fn every_key_of_the_vocabulary_has_its_own_physical_key_and_is_found_again() {
        let mut seen = Vec::new();
        for key in HotkeyKey::ALL {
            let physical = key_code(*key);
            assert!(!seen.contains(&physical), "{key:?} shares its physical key");
            seen.push(physical);
            assert_eq!(hotkey_key(code(physical)), Some(*key), "{key:?}");
        }
    }

    #[test]
    fn keys_the_vocabulary_does_not_hold_are_not_keys() {
        for physical in [
            code(KeyCode::Enter),
            code(KeyCode::Numpad5),
            code(KeyCode::Comma),
            code(KeyCode::ArrowUp),
            code(KeyCode::Escape),
            code(KeyCode::SuperLeft),
            code(KeyCode::AltRight),
            PhysicalKey::Unidentified(NativeKeyCode::Unidentified),
        ] {
            assert_eq!(hotkey_key(physical), None, "{physical:?}");
        }
    }

    /// The physical key is the position on the keyboard, the notion the system
    /// registers a shortcut by: these are the key codes the Carbon registration
    /// uses (`macos_hotkeys`), asked of winit's own table.
    #[cfg(target_os = "macos")]
    #[test]
    fn the_physical_keys_are_the_key_codes_a_shortcut_is_registered_by() {
        use slint::winit_030::winit::platform::scancode::PhysicalKeyExtScancode;
        for (key, scancode) in [
            (HotkeyKey::A, 0),
            (HotkeyKey::S, 1),
            (HotkeyKey::N, 45),
            (HotkeyKey::M, 46),
            (HotkeyKey::Digit1, 18),
            (HotkeyKey::Digit0, 29),
            (HotkeyKey::Space, 49),
            (HotkeyKey::F5, 96),
        ] {
            assert_eq!(code(key_code(key)).to_scancode(), Some(scancode), "{key:?}");
        }
    }

    #[test]
    fn nothing_held_and_modifiers_alone_are_no_key() {
        let mut tracker = KeyTracker::default();
        assert_eq!(tracker.held(), ChordHeld::NoKey);
        tracker.modifiers(ModifiersState::ALT | ModifiersState::SUPER);
        assert_eq!(tracker.held(), ChordHeld::NoKey);
        // A key outside the vocabulary says nothing either.
        press(&mut tracker, KeyCode::Enter);
        assert_eq!(tracker.held(), ChordHeld::NoKey);
    }

    #[test]
    fn one_key_with_the_modifiers_held_is_the_chord_in_the_macos_words() {
        let mut tracker = KeyTracker::default();
        tracker.modifiers(ModifiersState::ALT | ModifiersState::SUPER);
        press(&mut tracker, KeyCode::KeyM);
        assert_eq!(tracker.held(), chord("Alt+Win+M"));

        let mut tracker = KeyTracker::default();
        tracker.modifiers(ModifiersState::CONTROL | ModifiersState::ALT | ModifiersState::SHIFT);
        press(&mut tracker, KeyCode::Digit1);
        assert_eq!(tracker.held(), chord("Ctrl+Alt+Shift+1"));

        // No modifier at all is still a chord here: whether it may be used is
        // the core's question, asked after.
        let mut tracker = KeyTracker::default();
        press(&mut tracker, KeyCode::F13);
        assert_eq!(tracker.held(), bare(HotkeyKey::F13));
    }

    #[test]
    fn two_keys_are_ambiguous_until_one_is_let_go() {
        let mut tracker = KeyTracker::default();
        tracker.modifiers(ModifiersState::SUPER);
        press(&mut tracker, KeyCode::KeyA);
        press(&mut tracker, KeyCode::KeyB);
        assert_eq!(tracker.held(), ChordHeld::SeveralKeys);
        tracker.key(code(KeyCode::KeyA), ElementState::Released);
        assert_eq!(tracker.held(), chord("Win+B"));
        tracker.key(code(KeyCode::KeyB), ElementState::Released);
        assert_eq!(tracker.held(), ChordHeld::NoKey);
    }

    #[test]
    fn a_repeat_of_a_key_already_down_is_still_one_key() {
        let mut tracker = KeyTracker::default();
        tracker.modifiers(ModifiersState::ALT);
        for _ in 0..5 {
            press(&mut tracker, KeyCode::KeyN);
        }
        assert_eq!(tracker.held(), chord("Alt+N"));
        tracker.key(code(KeyCode::KeyN), ElementState::Released);
        assert_eq!(tracker.held(), ChordHeld::NoKey);
    }

    #[test]
    fn the_release_of_a_key_never_pressed_is_nothing() {
        let mut tracker = KeyTracker::default();
        press(&mut tracker, KeyCode::KeyA);
        tracker.key(code(KeyCode::KeyB), ElementState::Released);
        assert_eq!(tracker.held(), bare(HotkeyKey::A));
    }

    #[test]
    fn the_window_events_that_carry_modifiers_or_the_focus_are_followed() {
        let mut tracker = KeyTracker::default();
        tracker.observe(&WindowEvent::ModifiersChanged(Modifiers::from(
            ModifiersState::ALT | ModifiersState::SHIFT,
        )));
        press(&mut tracker, KeyCode::KeyK);
        assert_eq!(tracker.held(), chord("Alt+Shift+K"));

        // An event that says nothing about keys changes nothing.
        tracker.observe(&WindowEvent::CloseRequested);
        assert_eq!(tracker.held(), chord("Alt+Shift+K"));

        // A release elsewhere is never seen, so the focus forgets everything.
        tracker.observe(&WindowEvent::Focused(false));
        assert_eq!(tracker.held(), ChordHeld::NoKey);
        press(&mut tracker, KeyCode::KeyK);
        assert_eq!(tracker.held(), bare(HotkeyKey::K));
        tracker.observe(&WindowEvent::Focused(true));
        assert_eq!(tracker.held(), ChordHeld::NoKey);
    }
}
