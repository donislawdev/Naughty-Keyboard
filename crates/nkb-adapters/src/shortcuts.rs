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
use nkb_core::hotkeys::{HotkeyAction, HotkeyChord, HotkeyKey};
use nkb_sys::hotkey::{self, Hotkey, HotkeyId, HotkeyListener, HotkeyRegistration};

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
/// On macOS the primary modifier is Command, not Control - `ux-spec.md` 3
/// writes the defaults as `Cmd+Alt+<key>` there. The core table carries the
/// Windows and Linux convention (`nkb_core::hotkeys` says why), and the swap
/// happens here, where the system is known: `ctrl` becomes the `win` bit,
/// which is Command on that platform. Unmeasured on macOS, where no route
/// exists yet (`OBS-70`). The shape is fixed so the swap is not forgotten.
fn hotkey_for(index: usize, chord: &HotkeyChord) -> Hotkey {
    let command_is_primary = cfg!(target_os = "macos");
    let (ctrl, win) = if command_is_primary {
        (chord.win, chord.ctrl)
    } else {
        (chord.ctrl, chord.win)
    };
    Hotkey {
        // The application id range is `0..=0xBFFF`. A binding table is ten
        // entries, so the narrowing cannot lose anything, and if a table ever
        // grew past the range the system would refuse the id, which the
        // outcome reports as `Failed` rather than hiding.
        id: HotkeyId(u32::try_from(index).unwrap_or(u32::MAX)),
        ctrl,
        alt: chord.alt,
        shift: chord.shift,
        win,
        vk: virtual_key(chord.key),
    }
}

/// The Windows virtual-key code for one key of the shortcut vocabulary.
const fn virtual_key(key: HotkeyKey) -> u16 {
    match key {
        HotkeyKey::N => 0x4E,
        HotkeyKey::P => 0x50,
        HotkeyKey::R => 0x52,
        HotkeyKey::B => 0x42,
        HotkeyKey::H => 0x48,
        HotkeyKey::Space => 0x20,
        HotkeyKey::Digit0 => 0x30,
        HotkeyKey::Digit1 => 0x31,
        HotkeyKey::Digit2 => 0x32,
        HotkeyKey::Digit3 => 0x33,
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
        // Letters and digits are their ASCII codes, Space is VK_SPACE. A
        // collision would make two shortcuts one, and the second would register
        // as Taken against the first.
        let keys = [
            HotkeyKey::N,
            HotkeyKey::P,
            HotkeyKey::R,
            HotkeyKey::B,
            HotkeyKey::H,
            HotkeyKey::Space,
            HotkeyKey::Digit0,
            HotkeyKey::Digit1,
            HotkeyKey::Digit2,
            HotkeyKey::Digit3,
        ];
        let mut seen = std::collections::HashSet::new();
        for key in keys {
            assert!(seen.insert(virtual_key(key)), "{key:?} shares a code");
        }
        assert_eq!(virtual_key(HotkeyKey::N), u16::from(b'N'));
        assert_eq!(virtual_key(HotkeyKey::Digit0), u16::from(b'0'));
        assert_eq!(virtual_key(HotkeyKey::Space), 0x20);
    }

    #[test]
    fn the_id_is_the_position_so_a_fired_id_finds_its_action() {
        for (index, (_, chord)) in DEFAULT_BINDINGS.iter().enumerate() {
            let hotkey = hotkey_for(index, chord);
            assert_eq!(hotkey.id, HotkeyId(u32::try_from(index).expect("ten fits")));
            assert!(
                hotkey.alt && !hotkey.shift,
                "defaults keep Alt and no Shift"
            );
        }
    }

    #[test]
    fn the_primary_modifier_is_control_except_where_command_is() {
        let chord = HotkeyChord {
            ctrl: true,
            alt: true,
            shift: false,
            win: false,
            key: HotkeyKey::N,
        };
        let hotkey = hotkey_for(0, &chord);
        if cfg!(target_os = "macos") {
            assert!(
                hotkey.win && !hotkey.ctrl,
                "Command carries the shortcut on macOS"
            );
        } else {
            assert!(hotkey.ctrl && !hotkey.win, "Control carries it elsewhere");
        }
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
    /// test does not depend on what else is running. Shift is added on top of
    /// the family so this never collides with a real default.
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
