//! Registering the palette's global shortcuts with the operating system.
//!
//! This is the `HotkeyRegistrar` adapter from architektura.md section 3, and
//! it is as thin as `DirectInjection` next door: one port becomes one call
//! into `nkb-sys`, and two vocabularies are mapped in one named place. The
//! thread that holds the shortcuts, the message pump and every `unsafe` line
//! live in `nkb_sys::hotkey`. Nothing here needs any of them.
//!
//! # What the handle owns, and why that is all it owns
//!
//! [`Live`] holds exactly three things: the outcomes the system gave, the table
//! from a fired id back to its action, and the `nkb-sys` listener whose drop
//! releases every shortcut. No cached "last press", no flag - architektura.md
//! 6a puts state in `app`, and the one piece an adapter may hold is its own
//! system handle.

use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::Duration;

use nkb_app::ports::{
    HotkeyRegistrar, LiveShortcuts, ShortcutRegistration, ShortcutsUnavailable, Wait,
};
use nkb_core::hotkeys::{Bindings, Convention, HotkeyAction, HotkeyChord, HotkeyKey};
use nkb_sys::hotkey::{self, Hotkey, HotkeyId, HotkeyListener, HotkeyRegistration};

/// Which default table this build follows. The one place that knows the system,
/// because `nkb_core::hotkeys` holds both tables and no `cfg` (`D82`).
pub const CONVENTION: Convention = if cfg!(target_os = "macos") {
    Convention::MacOs
} else {
    Convention::WindowsAndLinux
};

/// The default shortcuts on the system this build runs on - what a palette
/// whose settings name no shortcut registers, and what the tester's own go on
/// top of (`KeptSettings::bindings`).
#[must_use]
pub const fn default_bindings() -> Bindings {
    Bindings::defaults(CONVENTION)
}

/// The character a keyboard layout of this system types for `chord` as
/// `AltGr` - the answer `KeptSettings::bindings` hands to the core (`K4d`).
///
/// Asks `nkb-sys` about `Ctrl+Alt` on the chord's key, with `Shift` when the
/// chord has it. It does not look at the chord's own `Ctrl` and `Alt`:
/// whether a chord CAN be `AltGr` is the core's question
/// (`HotkeyChord::could_be_altgr`), asked before this one - so the two
/// vocabularies still meet only in [`virtual_key`].
#[must_use]
pub fn altgr_character(chord: HotkeyChord) -> Option<char> {
    nkb_sys::layout::altgr_character(virtual_key(chord.key), chord.shift)
}

/// Registers shortcuts through `nkb_sys::hotkey::listen`.
///
/// Stateless, like `DirectInjection`: the state is the handle it returns.
#[derive(Debug, Default, Clone, Copy)]
pub struct GlobalShortcuts;

/// The registered set: outcomes, the id-to-action table, and the listener.
///
/// Field order is drop order, and it matters: the listener stops first, so the
/// thread has unregistered and gone before the receiver it sends to is torn
/// down. The other order works too - `nkb-sys` keeps pumping on a failed send -
/// but this one leaves nothing to reason about.
struct Live {
    _listener: HotkeyListener,
    fired: Receiver<HotkeyId>,
    /// Indexed by [`HotkeyId`]: the id handed to the system for binding `n` is
    /// `n`, so a fired id is a position in this table and nothing else.
    actions: Vec<HotkeyAction>,
    outcomes: Vec<(HotkeyAction, ShortcutRegistration)>,
}

impl HotkeyRegistrar for GlobalShortcuts {
    fn register(
        &self,
        bindings: &[(HotkeyAction, HotkeyChord)],
    ) -> Result<Box<dyn LiveShortcuts + Send>, ShortcutsUnavailable> {
        let hotkeys: Vec<Hotkey> = bindings
            .iter()
            .enumerate()
            .map(|(index, (_, chord))| hotkey_for(index, chord))
            .collect();
        let listening = hotkey::listen(&hotkeys).map_err(unavailable)?;
        let actions: Vec<HotkeyAction> = bindings.iter().map(|(action, _)| *action).collect();
        let outcomes = actions
            .iter()
            .copied()
            .zip(listening.outcomes.into_iter().map(registration))
            .collect();
        Ok(Box::new(Live {
            _listener: listening.listener,
            fired: listening.fired,
            actions,
            outcomes,
        }))
    }
}

impl LiveShortcuts for Live {
    fn outcomes(&self) -> &[(HotkeyAction, ShortcutRegistration)] {
        &self.outcomes
    }

    fn next(&self, wait: Duration) -> Wait {
        // A zero wait must not block at all - it is the drain after a send -
        // and `recv_timeout(0)` honours that, but `try_recv` says so by name.
        let received = if wait.is_zero() {
            self.fired.try_recv().map_err(|error| match error {
                std::sync::mpsc::TryRecvError::Empty => RecvTimeoutError::Timeout,
                std::sync::mpsc::TryRecvError::Disconnected => RecvTimeoutError::Disconnected,
            })
        } else {
            self.fired.recv_timeout(wait)
        };
        match received {
            Ok(id) => pressed(&self.actions, id),
            Err(RecvTimeoutError::Timeout) => Wait::Nothing,
            Err(RecvTimeoutError::Disconnected) => Wait::Gone,
        }
    }
}

/// The action a fired id stands for: its position in the binding table.
///
/// The system echoes only ids this handle registered (documented at
/// [`HotkeyId`]), so a miss cannot happen. If it ever did, an id nobody bound
/// is reported as nothing pressed rather than as a made-up action.
fn pressed(actions: &[HotkeyAction], id: HotkeyId) -> Wait {
    usize::try_from(id.0)
        .ok()
        .and_then(|index| actions.get(index))
        .map_or(Wait::Nothing, |action| Wait::Pressed(*action))
}

/// One binding in the words `nkb-sys` speaks: a raw virtual-key code and the
/// modifier bits, tagged with its position.
///
/// The key codes are the Windows `VK_*` values, which for letters and digits
/// are their ASCII codes and for Space is `VK_SPACE`. They go through
/// `nkb-sys`, whose route today is Windows only. The day a Linux route exists
/// it will want X11 keysyms here, and this is the one function that changes.
///
/// 🔴 The chord is registered AS WRITTEN, on every system. Until `D82` the macOS
/// branch swapped `ctrl` for the Command bit here, because the core carried one
/// table in the Windows convention. That was right for a table whose every entry
/// held `ctrl`, and it would have rewritten any chord a tester chose. The macOS
/// defaults are now their own table (`nkb_core::hotkeys::MACOS_DEFAULT_BINDINGS`),
/// so nothing here needs to know which system it is on.
fn hotkey_for(index: usize, chord: &HotkeyChord) -> Hotkey {
    Hotkey {
        // The application id range is `0..=0xBFFF`. A binding table is ten
        // entries, so the narrowing cannot lose anything, and if a table ever
        // grew past the range the system would refuse the id, which the
        // outcome reports as `Failed` rather than hiding.
        id: HotkeyId(u32::try_from(index).unwrap_or(u32::MAX)),
        ctrl: chord.ctrl,
        alt: chord.alt,
        shift: chord.shift,
        win: chord.win,
        vk: virtual_key(chord.key),
    }
}

/// The Windows virtual-key code for one key of the shortcut vocabulary.
///
/// Letters and digits are their ASCII codes, `VK_F1` to `VK_F24` run from
/// `0x70` to `0x87`, and Space is `VK_SPACE` - Microsoft's virtual-key table.
/// Written out rather than computed, so the match stays exhaustive and a key
/// added to the core without a code here does not compile.
const fn virtual_key(key: HotkeyKey) -> u16 {
    match key {
        HotkeyKey::A => 0x41,
        HotkeyKey::B => 0x42,
        HotkeyKey::C => 0x43,
        HotkeyKey::D => 0x44,
        HotkeyKey::E => 0x45,
        HotkeyKey::F => 0x46,
        HotkeyKey::G => 0x47,
        HotkeyKey::H => 0x48,
        HotkeyKey::I => 0x49,
        HotkeyKey::J => 0x4A,
        HotkeyKey::K => 0x4B,
        HotkeyKey::L => 0x4C,
        HotkeyKey::M => 0x4D,
        HotkeyKey::N => 0x4E,
        HotkeyKey::O => 0x4F,
        HotkeyKey::P => 0x50,
        HotkeyKey::Q => 0x51,
        HotkeyKey::R => 0x52,
        HotkeyKey::S => 0x53,
        HotkeyKey::T => 0x54,
        HotkeyKey::U => 0x55,
        HotkeyKey::V => 0x56,
        HotkeyKey::W => 0x57,
        HotkeyKey::X => 0x58,
        HotkeyKey::Y => 0x59,
        HotkeyKey::Z => 0x5A,
        HotkeyKey::Digit0 => 0x30,
        HotkeyKey::Digit1 => 0x31,
        HotkeyKey::Digit2 => 0x32,
        HotkeyKey::Digit3 => 0x33,
        HotkeyKey::Digit4 => 0x34,
        HotkeyKey::Digit5 => 0x35,
        HotkeyKey::Digit6 => 0x36,
        HotkeyKey::Digit7 => 0x37,
        HotkeyKey::Digit8 => 0x38,
        HotkeyKey::Digit9 => 0x39,
        HotkeyKey::F1 => 0x70,
        HotkeyKey::F2 => 0x71,
        HotkeyKey::F3 => 0x72,
        HotkeyKey::F4 => 0x73,
        HotkeyKey::F5 => 0x74,
        HotkeyKey::F6 => 0x75,
        HotkeyKey::F7 => 0x76,
        HotkeyKey::F8 => 0x77,
        HotkeyKey::F9 => 0x78,
        HotkeyKey::F10 => 0x79,
        HotkeyKey::F11 => 0x7A,
        HotkeyKey::F12 => 0x7B,
        HotkeyKey::F13 => 0x7C,
        HotkeyKey::F14 => 0x7D,
        HotkeyKey::F15 => 0x7E,
        HotkeyKey::F16 => 0x7F,
        HotkeyKey::F17 => 0x80,
        HotkeyKey::F18 => 0x81,
        HotkeyKey::F19 => 0x82,
        HotkeyKey::F20 => 0x83,
        HotkeyKey::F21 => 0x84,
        HotkeyKey::F22 => 0x85,
        HotkeyKey::F23 => 0x86,
        HotkeyKey::F24 => 0x87,
        HotkeyKey::Space => 0x20,
    }
}

fn registration(outcome: HotkeyRegistration) -> ShortcutRegistration {
    match outcome {
        HotkeyRegistration::Registered => ShortcutRegistration::Registered,
        HotkeyRegistration::Taken => ShortcutRegistration::Taken,
        HotkeyRegistration::Failed { code } => ShortcutRegistration::Failed { code },
    }
}

fn unavailable(error: hotkey::HotkeyUnavailable) -> ShortcutsUnavailable {
    match error {
        hotkey::HotkeyUnavailable::Unsupported { system } => ShortcutsUnavailable::Unsupported {
            system: system.to_owned(),
        },
        hotkey::HotkeyUnavailable::CouldNotStart => ShortcutsUnavailable::CouldNotStart,
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
    use nkb_core::hotkeys::DEFAULT_BINDINGS;

    #[test]
    fn every_key_of_the_vocabulary_has_its_own_virtual_key() {
        // A collision would make two shortcuts one, and the second would
        // register as Taken against the first. The expected codes come from the
        // key's NAME, not from the table under test: a letter or digit is its
        // ASCII code, `Fn` is `0x6F + n`, Space is `VK_SPACE`.
        let mut seen = std::collections::HashSet::new();
        for key in HotkeyKey::ALL {
            let code = virtual_key(*key);
            assert!(seen.insert(code), "{key:?} shares a code");
            let name = key.name();
            let expected = if name == "Space" {
                0x20
            } else if let Some(n) = name.strip_prefix('F').filter(|n| !n.is_empty()) {
                0x6F + n.parse::<u16>().expect("a function key number")
            } else {
                assert_eq!(name.len(), 1, "{name}");
                u16::from(name.as_bytes()[0])
            };
            assert_eq!(code, expected, "{name}");
        }
        assert_eq!(virtual_key(HotkeyKey::F24), 0x87);
    }

    #[test]
    fn the_id_is_the_position_so_a_fired_id_finds_its_action() {
        for (index, (_, chord)) in DEFAULT_BINDINGS.iter().enumerate() {
            let hotkey = hotkey_for(index, chord);
            assert_eq!(hotkey.id, HotkeyId(u32::try_from(index).expect("ten fits")));
        }
    }

    #[test]
    fn every_chord_is_registered_as_written_on_every_system() {
        // D82: no modifier is swapped anywhere. Every combination of the four
        // bits goes through unchanged, so a chord a tester picks is the chord
        // the system is asked for.
        for bits in 0u8..16 {
            let chord = HotkeyChord {
                ctrl: bits & 1 != 0,
                alt: bits & 2 != 0,
                shift: bits & 4 != 0,
                win: bits & 8 != 0,
                key: HotkeyKey::N,
            };
            let hotkey = hotkey_for(0, &chord);
            assert_eq!(
                (hotkey.ctrl, hotkey.alt, hotkey.shift, hotkey.win),
                (chord.ctrl, chord.alt, chord.shift, chord.win),
                "{chord:?}"
            );
        }
    }

    #[test]
    fn this_build_uses_the_table_of_its_own_system() {
        let expected = if cfg!(target_os = "macos") {
            &nkb_core::hotkeys::MACOS_DEFAULT_BINDINGS
        } else {
            &DEFAULT_BINDINGS
        };
        assert_eq!(default_bindings().as_slice(), expected);
    }

    #[test]
    fn a_fired_id_is_its_position_in_the_table_and_a_stray_id_presses_nothing() {
        let actions: Vec<HotkeyAction> = DEFAULT_BINDINGS.iter().map(|(a, _)| *a).collect();
        for (index, action) in actions.iter().enumerate() {
            let id = HotkeyId(u32::try_from(index).expect("ten fits"));
            assert_eq!(pressed(&actions, id), Wait::Pressed(*action));
        }
        assert_eq!(pressed(&actions, HotkeyId(10)), Wait::Nothing);
        assert_eq!(pressed(&actions, HotkeyId(u32::MAX)), Wait::Nothing);
    }

    #[test]
    fn registration_outcomes_translate_one_to_one() {
        assert_eq!(
            registration(HotkeyRegistration::Registered),
            ShortcutRegistration::Registered
        );
        assert_eq!(
            registration(HotkeyRegistration::Taken),
            ShortcutRegistration::Taken
        );
        assert_eq!(
            registration(HotkeyRegistration::Failed { code: 1400 }),
            ShortcutRegistration::Failed { code: 1400 }
        );
    }

    /// A combination nothing on the machine is likely to hold, so the live
    /// test does not depend on what else is running. Three modifiers, so it
    /// never collides with a real default of either convention.
    fn obscure(key: HotkeyKey) -> (HotkeyAction, HotkeyChord) {
        (
            HotkeyAction::MarkSuspect,
            HotkeyChord {
                ctrl: true,
                alt: true,
                shift: true,
                win: false,
                key,
            },
        )
    }

    #[test]
    fn registering_for_real_reports_per_binding_and_releases_on_drop() {
        // On a system with a route: one registration succeeds, its outcome is
        // paired with its action, and dropping the handle frees the
        // combination so a second registration succeeds too. Elsewhere the
        // port names the system instead of pretending.
        let bindings = [obscure(HotkeyKey::Digit3)];
        match GlobalShortcuts.register(&bindings) {
            Ok(live) => {
                assert!(hotkey::can_register());
                assert_eq!(
                    live.outcomes(),
                    &[(HotkeyAction::MarkSuspect, ShortcutRegistration::Registered)]
                );
                assert_eq!(
                    live.next(Duration::ZERO),
                    Wait::Nothing,
                    "nothing was pressed, and a zero wait does not block"
                );
                drop(live);
                let again = GlobalShortcuts
                    .register(&bindings)
                    .expect("the route did not vanish");
                assert_eq!(
                    again.outcomes()[0].1,
                    ShortcutRegistration::Registered,
                    "the drop released the combination"
                );
            }
            Err(ShortcutsUnavailable::Unsupported { system }) => {
                assert!(!hotkey::can_register());
                assert!(!system.is_empty(), "the system is named");
            }
            Err(ShortcutsUnavailable::CouldNotStart) => {
                panic!("the listener thread could not start on a system with a route")
            }
        }
    }

    #[test]
    fn a_taken_combination_is_reported_beside_the_others_not_as_an_error() {
        if !hotkey::can_register() {
            return;
        }
        // The same combination twice in one set: the second is Taken by the
        // first. The set still registers, and the outcome says which.
        let bindings = [obscure(HotkeyKey::Digit2), obscure(HotkeyKey::Digit2)];
        let live = GlobalShortcuts
            .register(&bindings)
            .expect("a taken combination is an outcome, not an error");
        assert_eq!(live.outcomes()[0].1, ShortcutRegistration::Registered);
        assert_eq!(live.outcomes()[1].1, ShortcutRegistration::Taken);
    }
}
