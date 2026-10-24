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
//! `Live` holds exactly three things: the outcomes the system gave, the table
//! from a fired id back to its action, and the `nkb-sys` listener whose drop
//! releases every shortcut. No cached "last press", no flag - architektura.md
//! 6a puts state in `app`, and the one piece an adapter may hold is its own
//! system handle.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::Duration;

use nkb_app::ports::{
    HotkeyRegistrar, LiveShortcuts, ShortcutRegistration, ShortcutsUnavailable, Wait,
};
use nkb_core::hotkeys::{Bindings, Convention, HotkeyAction, HotkeyChord, HotkeyKey};
use nkb_sys::hotkey::{self, Hotkey, HotkeyId, HotkeyListener, HotkeyRegistration};

use crate::settings_file::SettingsFile;
use crate::shortcut_claim::Claims;

/// Which default table this build follows. The one place that knows the system,
/// because `nkb_core::hotkeys` holds both tables and no `cfg` (`D82`).
pub const CONVENTION: Convention = if cfg!(target_os = "macos") {
    Convention::MacOs
} else if cfg!(target_os = "linux") {
    Convention::Linux
} else {
    Convention::Windows
};

/// The default shortcuts on the system this build runs on - what a palette
/// whose settings name no shortcut registers, and what the tester's own go on
/// top of (`KeptSettings::bindings`).
#[must_use]
pub const fn default_bindings() -> Bindings {
    Bindings::defaults(CONVENTION)
}

/// The character a keyboard layout of this system types for `chord`, as the key
/// that makes it type: `AltGr` on Windows, `Option` on macOS, none on Linux - the
/// answer `KeptSettings::bindings` hands to the core (`K4d`, `D141`).
///
/// Asks `nkb-sys` about that key on the chord's key, with `Shift` when the chord
/// has it. It does not look at the chord's own modifiers: whether a chord CAN
/// type is the core's question ([`Convention::can_type`]), asked before this
/// one - so the two vocabularies still meet only in `virtual_key`.
#[must_use]
pub fn layout_character(chord: HotkeyChord) -> Option<char> {
    let key = virtual_key(chord.key);
    match CONVENTION {
        Convention::Windows => nkb_sys::layout::altgr_character(key, chord.shift),
        // The core never asks on Linux (`can_type` is false there), and a layout
        // has no key that turns `Ctrl+Alt` into a character.
        Convention::Linux => None,
        Convention::MacOs => nkb_sys::layout::option_character(key, chord.shift),
    }
}

/// Reads the keyboard layouts again on the thread that calls it, which must be
/// the main one on macOS (`nkb_sys::layout::refresh_layouts`). Called where the
/// tester is about to choose a shortcut. [`layout_character`] answers from what
/// was read last, on any thread.
pub fn refresh_layouts() {
    nkb_sys::layout::refresh_layouts();
}

/// What is held down as a window of ours handles a key press - the chord the
/// shortcuts window records (K5.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChordHeld {
    /// No key of the shortcut vocabulary is down - only modifiers, or a key
    /// no shortcut can use. The chord is not finished.
    NoKey,
    /// Exactly one key of the vocabulary, with the modifiers held with it.
    Chord(HotkeyChord),
    /// More than one key of the vocabulary: which one was meant cannot be told,
    /// so none is taken.
    SeveralKeys,
    /// This system cannot tell - no route yet (`nkb_sys::held`).
    Unknown,
}

/// The chord held down right now, asked of the system - never read from the
/// text or the modifier flags a key event carries (`slint.md` 2.37).
///
/// 🔴 Meaningful only while the thread that owns the window handles a key
/// press: it reads that thread's own key state, which follows the keyboard
/// messages the thread has read and nothing typed anywhere else.
#[must_use]
pub fn chord_held() -> ChordHeld {
    let keys: Vec<u16> = HotkeyKey::ALL.iter().map(|key| virtual_key(*key)).collect();
    chord_of(nkb_sys::held::held(&keys))
}

/// The chord in what the system said is held. The two vocabularies meet in
/// [`virtual_key`], as everywhere in this file.
fn chord_of(held: Option<nkb_sys::held::Held>) -> ChordHeld {
    let Some(held) = held else {
        return ChordHeld::Unknown;
    };
    let mut keys = HotkeyKey::ALL
        .iter()
        .copied()
        .filter(|key| held.keys.contains(&virtual_key(*key)));
    match (keys.next(), keys.next()) {
        (None, _) => ChordHeld::NoKey,
        (Some(key), None) => ChordHeld::Chord(HotkeyChord {
            ctrl: held.ctrl,
            alt: held.alt,
            shift: held.shift,
            win: held.win,
            key,
        }),
        (Some(_), Some(_)) => ChordHeld::SeveralKeys,
    }
}

/// The claim on the shortcuts of this user, held by this process once taken.
static CLAIMS: Claims = Claims::new();

/// Whether another palette of this user holds the shortcuts, on a system that
/// does not say so itself (`shortcut_claim`). Windows refuses a second
/// registration of a chord, so it is not asked there, and where the lock cannot
/// be asked about the shortcuts are registered as always.
fn claimed_elsewhere() -> bool {
    CLAIMS.is_held_elsewhere(CLAIM_ASKED, claim_place().as_deref())
}

/// Whether this build asks for the claim: only where the system does not refuse
/// a chord that is held already, which is macOS (`OBS-187`).
const CLAIM_ASKED: bool = cfg!(target_os = "macos");

/// The lock file: beside the settings file, in the folder of the program's own.
fn claim_place() -> Option<PathBuf> {
    claim_place_beside(&SettingsFile::for_this_user())
}

fn claim_place_beside(settings: &SettingsFile) -> Option<PathBuf> {
    Some(settings.path()?.parent()?.join(CLAIM_FILE))
}

/// The name of the lock file, in the folder of the settings.
const CLAIM_FILE: &str = "shortcuts.lock";

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
    /// `None` for a set that was refused before the system was asked: another
    /// palette of this user holds the shortcuts (`D144`).
    _listener: Option<HotkeyListener>,
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
        Self::register_unless(bindings, claimed_elsewhere())
    }
}

impl GlobalShortcuts {
    /// [`HotkeyRegistrar::register`] with the claim answered by the caller: the
    /// tests that register for real do not ask the lock of the user running them,
    /// which a palette of that user may hold.
    fn register_unless(
        bindings: &[(HotkeyAction, HotkeyChord)],
        refused: bool,
    ) -> Result<Box<dyn LiveShortcuts + Send>, ShortcutsUnavailable> {
        if refused {
            return Ok(Box::new(Live::refused(bindings)));
        }
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
            _listener: Some(listening.listener),
            fired: listening.fired,
            actions,
            outcomes,
        }))
    }
}

impl Live {
    /// The handle of a set in which every shortcut is taken and the system was not
    /// asked. Nothing stands behind it: its presses are a channel nobody sends on,
    /// and the caller does not wait on a set that [`LiveShortcuts::holds_any`]
    /// says holds nothing.
    fn refused(bindings: &[(HotkeyAction, HotkeyChord)]) -> Self {
        let (_, fired) = mpsc::channel::<HotkeyId>();
        let actions: Vec<HotkeyAction> = bindings.iter().map(|(action, _)| *action).collect();
        let outcomes = actions
            .iter()
            .map(|action| (*action, ShortcutRegistration::Taken))
            .collect();
        Self {
            _listener: None,
            fired,
            actions,
            outcomes,
        }
    }
}

impl LiveShortcuts for Live {
    fn outcomes(&self) -> &[(HotkeyAction, ShortcutRegistration)] {
        &self.outcomes
    }

    fn next(&self, wait: Duration) -> Wait {
        wait_for(&self.fired, &self.actions, wait)
    }
}

/// Waits up to `wait` for the next fired id and names the action it stands for.
/// The one wait of every route: a set held by a grab and a set given by the
/// desktop's portal answer a caller in the same way.
pub(crate) fn wait_for(
    fired: &Receiver<HotkeyId>,
    actions: &[HotkeyAction],
    wait: Duration,
) -> Wait {
    // A zero wait must not block at all - it is the drain after a send -
    // and `recv_timeout(0)` honours that, but `try_recv` says so by name.
    let received = if wait.is_zero() {
        fired.try_recv().map_err(|error| match error {
            std::sync::mpsc::TryRecvError::Empty => RecvTimeoutError::Timeout,
            std::sync::mpsc::TryRecvError::Disconnected => RecvTimeoutError::Disconnected,
        })
    } else {
        fired.recv_timeout(wait)
    };
    match received {
        Ok(id) => pressed(actions, id),
        Err(RecvTimeoutError::Timeout) => Wait::Nothing,
        Err(RecvTimeoutError::Disconnected) => Wait::Gone,
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
/// `nkb-sys`, which takes them as they are on Windows and translates them on the
/// other two systems: to a Carbon key code on macOS (`macos_hotkeys::key_code`) and
/// to an X11 keysym on Linux (`linux_hotkeys::keysym_for_vk`). So this is still the
/// one function that names keys, and no system but its own needs to know them.
///
/// 🔴 The chord is registered AS WRITTEN, on every system. Until `D82` the macOS
/// branch swapped `ctrl` for the Command bit here, because the core carried one
/// table in the Windows convention. That was right for a table whose every entry
/// held `ctrl`, and it would have rewritten any chord a tester chose. The macOS
/// defaults are now their own table (`nkb_core::hotkeys::MACOS_DEFAULT_BINDINGS`),
/// so nothing here needs to know which system it is on.
pub(crate) fn hotkey_for(index: usize, chord: &HotkeyChord) -> Hotkey {
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
pub(crate) const fn virtual_key(key: HotkeyKey) -> u16 {
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

    fn holding(ctrl: bool, alt: bool, shift: bool, win: bool, keys: &[u16]) -> ChordHeld {
        chord_of(Some(nkb_sys::held::Held {
            ctrl,
            alt,
            shift,
            win,
            keys: keys.to_vec(),
        }))
    }

    #[test]
    fn every_key_of_the_vocabulary_held_alone_is_read_back_as_itself() {
        // The reverse of `virtual_key`, for all sixty-one keys and every
        // combination of the four modifiers - the chord recorded is the chord
        // the registration will ask the system for.
        for key in HotkeyKey::ALL {
            for bits in 0u8..16 {
                let chord = HotkeyChord {
                    ctrl: bits & 1 != 0,
                    alt: bits & 2 != 0,
                    shift: bits & 4 != 0,
                    win: bits & 8 != 0,
                    key: *key,
                };
                assert_eq!(
                    holding(
                        chord.ctrl,
                        chord.alt,
                        chord.shift,
                        chord.win,
                        &[virtual_key(*key)]
                    ),
                    ChordHeld::Chord(chord)
                );
            }
        }
    }

    #[test]
    fn modifiers_alone_two_keys_and_no_route_are_not_a_chord() {
        // `Alt+Shift` pressed on the way to a chord: not finished yet.
        assert_eq!(holding(false, true, true, false, &[]), ChordHeld::NoKey);
        // A key outside the vocabulary (Tab) is no key of a shortcut.
        assert_eq!(holding(true, true, false, false, &[0x09]), ChordHeld::NoKey);
        // Two keys of the vocabulary: which one was meant cannot be told.
        assert_eq!(
            holding(false, true, true, false, &[0x4E, 0x50]),
            ChordHeld::SeveralKeys
        );
        assert_eq!(chord_of(None), ChordHeld::Unknown);
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
        } else if cfg!(target_os = "linux") {
            &nkb_core::hotkeys::LINUX_DEFAULT_BINDINGS
        } else {
            &DEFAULT_BINDINGS
        };
        assert_eq!(default_bindings().as_slice(), expected);
    }

    #[test]
    fn a_fired_id_is_its_position_in_the_table_and_a_stray_id_presses_nothing() {
        let actions: Vec<HotkeyAction> = DEFAULT_BINDINGS.iter().map(|(a, _)| *a).collect();
        for (index, action) in actions.iter().enumerate() {
            let id = HotkeyId(u32::try_from(index).expect("an index of the table fits"));
            assert_eq!(pressed(&actions, id), Wait::Pressed(*action));
        }
        // One past the table, whatever its size: a fixed number here went
        // stale the day the table grew (`D120`).
        let past = u32::try_from(actions.len()).expect("the table's length fits");
        assert_eq!(pressed(&actions, HotkeyId(past)), Wait::Nothing);
        assert_eq!(pressed(&actions, HotkeyId(u32::MAX)), Wait::Nothing);
    }

    /// `D144`: a palette that does not hold the claim gets a set in which every
    /// shortcut is taken and nothing stands behind it, so the palette says so, does
    /// not wait on it and tries again at a resume.
    #[test]
    fn a_set_refused_before_the_system_was_asked_is_taken_to_the_last_and_holds_nothing() {
        let bindings = default_bindings();
        let live = Live::refused(bindings.as_slice());
        assert_eq!(live.outcomes().len(), bindings.as_slice().len());
        assert!(
            live.outcomes()
                .iter()
                .zip(bindings.as_slice())
                .all(|((action, outcome), (asked, _))| action == asked
                    && *outcome == ShortcutRegistration::Taken)
        );
        assert!(!live.holds_any(), "a set where nothing took");
        // Nobody sends on it: a wait ends with the channel, not with a press.
        assert_eq!(live.next(Duration::ZERO), Wait::Gone);
    }

    /// A registration that the claim refused is the refused set, and the system
    /// is not asked: on a system with a route the same bindings would register.
    #[test]
    fn a_registration_that_the_claim_refused_is_the_refused_set() {
        let bindings = default_bindings();
        let live = GlobalShortcuts::register_unless(bindings.as_slice(), true)
            .expect("a refused set is still a set");
        assert!(!live.holds_any());
        assert!(
            live.outcomes()
                .iter()
                .all(|(_, outcome)| *outcome == ShortcutRegistration::Taken)
        );
        assert_eq!(live.outcomes().len(), bindings.as_slice().len());
    }

    /// Windows refuses a second registration of a chord by itself, so it never
    /// asks the lock and a Windows palette never holds one for a Mac palette:
    /// on Windows this pins the constant to false.
    #[test]
    fn only_the_system_that_does_not_refuse_a_held_chord_asks_for_the_claim() {
        assert_eq!(CLAIM_ASKED, cfg!(target_os = "macos"));
    }

    /// The lock lies beside the settings file, in the folder of the program's
    /// own, and nowhere when the settings have no place.
    #[test]
    fn the_lock_file_lies_beside_the_settings_file() {
        let beside = claim_place_beside(&SettingsFile::at(
            PathBuf::from("home")
                .join("Naughty Keyboard")
                .join("settings.toml"),
        ));
        assert_eq!(
            beside,
            Some(
                PathBuf::from("home")
                    .join("Naughty Keyboard")
                    .join("shortcuts.lock")
            )
        );
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
        match GlobalShortcuts::register_unless(&bindings, false) {
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
                let again = GlobalShortcuts::register_unless(&bindings, false)
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
            Err(ShortcutsUnavailable::Desktop(refusal)) => {
                panic!("a grab asks no desktop, and one answered: {refusal}")
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
        let live = GlobalShortcuts::register_unless(&bindings, false)
            .expect("a taken combination is an outcome, not an error");
        assert_eq!(live.outcomes()[0].1, ShortcutRegistration::Registered);
        assert_eq!(live.outcomes()[1].1, ShortcutRegistration::Taken);
    }
}
