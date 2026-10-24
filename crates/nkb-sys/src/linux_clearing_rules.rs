//! What clearing a field on a Wayland session decides without a bus, so that it is tested on
//! every system (`D145` point 74, `OBS-229`).
//!
//! # What was measured before this was written
//!
//! GNOME 50, 2026-10-24, the probes `tools/sondy/linux/portal_czyszczenie.py` and
//! `ibus_klawisze_sonda.py`, a keyboard of the kernel for the hand:
//!
//! - `Home`, `Shift+End`, `Delete` pressed through the remote-desktop portal
//!   (`NotifyKeyboardKeysym`) emptied the line of the field in GTK4, Qt, Firefox and Slint.
//! - The same keys with `Ctrl` held by the hand wiped the whole document (7 characters to 0
//!   where a clean line clear leaves 4). The shortcut of the palette on Linux is
//!   `Ctrl+Alt+<letter>`, so a modifier is usually still down when the keys are sent.
//! - A key-up for a modifier sent through the portal does not release a modifier the hand
//!   holds, and the X server of XWayland does not see one when a native window has the focus.
//!   There is no way to ask the compositor which modifier is down.
//! - The input method engine of this program gets every key BEFORE the application, with the
//!   modifiers the hand holds in its state, and a key it takes never reaches the application.
//!   With `Ctrl`, `Alt` or `Shift` held and the engine taking the keys that carry a modifier
//!   the recipe does not have, the count of characters after the whole recipe was the count
//!   before, in all four toolkits. With nothing held the recipe cleared the line. The key of
//!   a modifier itself is never taken, because a taken key-up of a held `Ctrl` would leave it
//!   down in the application.
//!
//! So the engine is the sensor and the filter at once, and the recipe is sent as a dialogue:
//! each key is announced to the engine first, and the engine says what it found on arrival.

use core::time::Duration;

use crate::linux_ibus_rules::IBUS_MODIFIER_MASK;
use crate::portal::Unavailable;
use crate::{Chord, NavKey, StopReason};

/// The keysyms of the keys of the recipe and of the `Shift` that goes with one of them.
pub(crate) const KEYSYM_SHIFT_L: u32 = 0xffe1;
pub(crate) const KEYSYM_HOME: u32 = 0xff50;
pub(crate) const KEYSYM_END: u32 = 0xff57;
pub(crate) const KEYSYM_DELETE: u32 = 0xffff;

/// The keysym of a key of the recipe.
pub(crate) const fn keysym_of(key: NavKey) -> u32 {
    match key {
        NavKey::Home => KEYSYM_HOME,
        NavKey::End => KEYSYM_END,
        NavKey::Delete => KEYSYM_DELETE,
    }
}

/// How long the engine is given to say it saw a key. The portal hands a key to the compositor
/// at once and the compositor hands it to the engine within a few milliseconds, so this is a
/// ceiling for a busy desktop, and a key that is not heard in it did not go through the
/// engine: the field is not one the input method serves, and nothing more is sent.
pub(crate) const ECHO_WAIT: Duration = Duration::from_millis(500);

/// The pause between two events of one chord. 40 ms is what the probe sent with, and the
/// engine saw every event in order.
pub(crate) const KEY_GAP: Duration = Duration::from_millis(40);

/// The pause between two chords of the recipe, so that the application has taken the last
/// before the next goes.
pub(crate) const CHORD_GAP: Duration = Duration::from_millis(30);

/// How long a modifier the hand holds is waited out before the first key of the recipe, and
/// how often that key is tried again meanwhile. The first key is `Home`, which the engine
/// takes whenever a modifier is down, so a try that finds one costs nothing in the field.
pub(crate) const HELD_RETRY: Duration = Duration::from_millis(80);

/// The `Shift` bit of the state of a key.
const SHIFT: u32 = 1 << 0;

/// The modifiers that change what a key means: those of the input method rules, and `Mod5`,
/// which is `AltGr` on the layouts that have one. The lock bits and Num Lock are not among them.
const MODIFIERS: u32 = IBUS_MODIFIER_MASK | (1 << 7);

/// The name of the first modifier in `mask`, in the words the refusal of a send uses.
pub(crate) const fn held_name(mask: u32) -> Option<&'static str> {
    if mask & (1 << 2) != 0 {
        Some("Ctrl")
    } else if mask & (1 << 3) != 0 {
        Some("Alt")
    } else if mask & (1 << 7) != 0 {
        Some("AltGr")
    } else if mask & ((1 << 6) | (1 << 26)) != 0 {
        Some("Super")
    } else if mask & SHIFT != 0 {
        Some("Shift")
    } else if mask & (1 << 27) != 0 {
        Some("Hyper")
    } else if mask & (1 << 28) != 0 {
        Some("Meta")
    } else {
        None
    }
}

/// The key the engine is told to expect next, and whether `Shift` is part of the chord.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Expected {
    pub keysym: u32,
    pub shifted: bool,
}

/// What the engine expects of `chord`.
pub(crate) const fn expected_for(chord: &Chord) -> Expected {
    Expected {
        keysym: keysym_of(chord.key),
        shifted: chord.shift,
    }
}

/// What the engine found on the arrival of a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Judgement {
    /// Not the key being waited for, so the application gets it as it came.
    Unrelated,
    /// The key being waited for, with the modifiers of the recipe and no others.
    Clean,
    /// The key being waited for, with a modifier the hand holds. Taken, so the application
    /// never sees it. Carries the name of the modifier.
    Held(&'static str),
    /// The key being waited for, which should carry `Shift` and did not. Taken: `Delete`
    /// after a `End` that selected nothing would remove one character, not the line.
    NoShift,
}

impl Judgement {
    /// Whether the engine takes the key.
    pub(crate) const fn taken(self) -> bool {
        matches!(self, Self::Held(_) | Self::NoShift)
    }
}

/// What to make of a key the engine was handed, while the dialogue of the recipe is on.
///
/// Only the key being waited for is judged: every other key, the modifiers included, goes
/// to the application as it came. A key of the recipe with a modifier it does not have is
/// the hand holding that modifier, and is taken whole, press and release alike, so the
/// application does not get half of it.
pub(crate) fn judge_key(keyval: u32, state: u32, expected: Option<Expected>) -> Judgement {
    let Some(expected) = expected else {
        return Judgement::Unrelated;
    };
    if keyval != expected.keysym {
        return Judgement::Unrelated;
    }
    // The bit that marks a release is not among the modifiers, so a key is judged as its press was.
    let held = state & MODIFIERS;
    let ours = if expected.shifted { SHIFT } else { 0 };
    match held_name(held & !ours) {
        Some(name) => Judgement::Held(name),
        None if held & ours != ours => Judgement::NoShift,
        None => Judgement::Clean,
    }
}

/// What the send does once the engine has said what it found on a key, or has said nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Next {
    /// The key is in: the recipe goes on.
    Go,
    /// A modifier is held on the first key, which the engine took, and the wait for it to
    /// come up is not over: the key is tried again after a pause.
    Wait,
    /// A modifier is still held when the wait is over, and nothing was pressed that acts.
    Refuse(&'static str),
    /// The recipe is not sent further, for this reason.
    Stop(StopReason),
}

/// The step after the key `judged` says what it found. Only the first key of the recipe is the
/// probe that waits out a modifier: a modifier that turns up after other keys acted stops the
/// recipe where it is, because `Delete` would then remove what is left of a line, and a key
/// the engine did not hear at all did not go through the engine, so none is sent after it.
pub(crate) const fn next_step(
    judged: Option<Judgement>,
    first_chord: bool,
    wait_left: bool,
) -> Next {
    match judged {
        Some(Judgement::Clean) => Next::Go,
        Some(Judgement::Held(name)) if first_chord => {
            if wait_left {
                Next::Wait
            } else {
                Next::Refuse(name)
            }
        }
        Some(Judgement::Held(_) | Judgement::NoShift | Judgement::Unrelated) => {
            Next::Stop(StopReason::Dropped)
        }
        None => Next::Stop(StopReason::NotTaking),
    }
}

/// Why no key could be pressed through the portal, in the words that follow `on`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum KeysRefusal {
    /// The desktop has no portal for remote control, or the bus to ask is not there.
    NoPortal,
    /// The person did not allow this program to press keys.
    NotAllowed,
    /// The portal took the request and did not answer in time.
    NoAnswer,
}

impl KeysRefusal {
    pub(crate) const fn system(self) -> &'static str {
        match self {
            Self::NoPortal => "Linux under Wayland without a portal to press keys through",
            Self::NotAllowed => {
                "Linux under Wayland where this program was not allowed to press keys"
            }
            Self::NoAnswer => {
                "Linux under Wayland where the portal did not answer the request to press keys"
            }
        }
    }
}

/// The refusal a portal that came to nothing stands for.
pub(crate) const fn keys_refusal(why: Unavailable) -> KeysRefusal {
    match why {
        Unavailable::Cancelled => KeysRefusal::NotAllowed,
        Unavailable::NoAnswer => KeysRefusal::NoAnswer,
        _ => KeysRefusal::NoPortal,
    }
}

// ---------------------------------------------------------------------------
// The token that lets the desktop remember the permission
// ---------------------------------------------------------------------------

/// The type of device asked for: a keyboard, and nothing else.
pub(crate) const DEVICE_KEYBOARD: u32 = 1;

/// The permission is kept by the desktop until the person takes it back (`persist_mode` 2).
pub(crate) const PERSIST_UNTIL_REVOKED: u32 = 2;

/// The interface of the portal that presses keys.
pub(crate) const REMOTE_DESKTOP_INTERFACE: &str = "org.freedesktop.portal.RemoteDesktop";

/// The folder under the state directory, lower case and without a space, as the folder of the
/// settings is under the configuration directory.
const STATE_FOLDER: &str = "naughty-keyboard";

/// The file the token is kept in.
const TOKEN_FILE: &str = "remote-desktop-token";

/// The longest token that is read back. A token of the portal is a short string, and one longer
/// than this is not one.
const TOKEN_LONGEST: usize = 256;

/// Where the environment says the state of the person is.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct StateDirs<'a> {
    pub state_home: Option<&'a str>,
    pub home: Option<&'a str>,
}

/// The file the restore token is kept in: `XDG_STATE_HOME` when it is a full path, and
/// `.local/state` in the home otherwise, as the base directory specification says. `None`
/// when neither is a full path, and nothing is then kept.
pub(crate) fn token_file(dirs: &StateDirs<'_>) -> Option<String> {
    let base = match dirs.state_home.filter(|dir| dir.starts_with('/')) {
        Some(dir) => dir.trim_end_matches('/').to_owned(),
        None => format!(
            "{}/.local/state",
            dirs.home
                .filter(|dir| dir.starts_with('/'))?
                .trim_end_matches('/')
        ),
    };
    Some(format!("{base}/{STATE_FOLDER}/{TOKEN_FILE}"))
}

/// The folder the token file is in, and the name of the file.
pub(crate) fn split_token_file(path: &str) -> Option<(&str, &str)> {
    path.rsplit_once('/')
        .filter(|(dir, name)| !dir.is_empty() && !name.is_empty())
}

/// The token a file holds, or `None` when it holds no token this program would hand back
/// to the portal: empty, too long, or with a character that is not graphic ASCII. A file
/// somebody edited, or half of a write, is no token, and the desktop is asked again.
pub(crate) fn usable_token(content: &str) -> Option<&str> {
    let token = content.trim();
    (!token.is_empty()
        && token.len() <= TOKEN_LONGEST
        && token.bytes().all(|byte| byte.is_ascii_graphic()))
    .then_some(token)
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;
    use crate::linux_ibus_rules::IBUS_RELEASE_MASK;

    const CTRL: u32 = 1 << 2;
    const ALT: u32 = 1 << 3;
    const LOCK: u32 = 1 << 1;
    const NUM_LOCK: u32 = 1 << 4;

    fn plain(key: NavKey) -> Option<Expected> {
        Some(expected_for(&Chord { key, shift: false }))
    }

    fn shifted(key: NavKey) -> Option<Expected> {
        Some(expected_for(&Chord { key, shift: true }))
    }

    #[test]
    fn the_keys_of_the_recipe_have_the_keysyms_of_the_x_protocol() {
        // Measured in the probe: these are the values the portal and the engine speak in.
        assert_eq!(keysym_of(NavKey::Home), 0xff50);
        assert_eq!(keysym_of(NavKey::End), 0xff57);
        assert_eq!(keysym_of(NavKey::Delete), 0xffff);
        assert_eq!(KEYSYM_SHIFT_L, 0xffe1);
    }

    #[test]
    fn a_key_with_no_modifier_is_clean() {
        assert_eq!(
            judge_key(KEYSYM_HOME, 0, plain(NavKey::Home)),
            Judgement::Clean
        );
        assert_eq!(
            judge_key(KEYSYM_DELETE, 0, plain(NavKey::Delete)),
            Judgement::Clean
        );
    }

    #[test]
    fn the_lock_bits_and_the_release_bit_change_nothing() {
        // Measured: Num Lock is on in a virtual machine more often than not.
        let state = LOCK | NUM_LOCK | IBUS_RELEASE_MASK;
        assert_eq!(
            judge_key(KEYSYM_HOME, state, plain(NavKey::Home)),
            Judgement::Clean
        );
    }

    #[test]
    fn a_modifier_the_hand_holds_is_named_and_taken() {
        // The measurement behind the whole route: with `Ctrl` held the recipe wiped the
        // document, and with it taken the count did not change.
        for (state, name) in [
            (CTRL, "Ctrl"),
            (ALT, "Alt"),
            (CTRL | ALT, "Ctrl"),
            (1 << 7, "AltGr"),
            (1 << 6, "Super"),
            (1 << 26, "Super"),
            (SHIFT, "Shift"),
            (1 << 27, "Hyper"),
            (1 << 28, "Meta"),
        ] {
            let judged = judge_key(KEYSYM_HOME, state, plain(NavKey::Home));
            assert_eq!(judged, Judgement::Held(name), "state {state:#x}");
            assert!(judged.taken());
        }
    }

    #[test]
    fn the_release_of_a_key_is_judged_as_its_press_was() {
        let held = CTRL | IBUS_RELEASE_MASK;
        assert_eq!(
            judge_key(KEYSYM_HOME, held, plain(NavKey::Home)),
            Judgement::Held("Ctrl"),
            "a press that was taken has its release taken, or the application gets half a key"
        );
    }

    #[test]
    fn the_shift_of_the_recipe_is_part_of_the_chord_and_nothing_else_is() {
        assert_eq!(
            judge_key(KEYSYM_END, SHIFT, shifted(NavKey::End)),
            Judgement::Clean
        );
        assert_eq!(
            judge_key(KEYSYM_END, SHIFT | CTRL, shifted(NavKey::End)),
            Judgement::Held("Ctrl")
        );
        assert_eq!(
            judge_key(KEYSYM_END, SHIFT | ALT, shifted(NavKey::End)),
            Judgement::Held("Alt")
        );
    }

    #[test]
    fn a_shift_that_did_not_arrive_with_its_key_is_taken() {
        // `Delete` after an `End` that selected nothing would remove one character.
        let judged = judge_key(KEYSYM_END, 0, shifted(NavKey::End));
        assert_eq!(judged, Judgement::NoShift);
        assert!(judged.taken());
    }

    #[test]
    fn only_the_key_waited_for_is_judged() {
        // The modifiers are never taken: a taken release of a `Ctrl` the hand holds would
        // leave it down in the application.
        for modifier in [KEYSYM_SHIFT_L, 0xffe3, 0xffe9, 0xffeb] {
            assert_eq!(
                judge_key(modifier, CTRL, plain(NavKey::Home)),
                Judgement::Unrelated,
                "{modifier:#x}"
            );
        }
        // Another key is the person's own, and is theirs to type.
        assert_eq!(
            judge_key(0x0061, CTRL, plain(NavKey::Home)),
            Judgement::Unrelated
        );
        assert_eq!(
            judge_key(KEYSYM_DELETE, CTRL, plain(NavKey::Home)),
            Judgement::Unrelated,
            "the key after this one is not judged before its turn"
        );
        // Nothing is waited for outside the dialogue.
        assert_eq!(
            judge_key(KEYSYM_HOME, CTRL, None),
            Judgement::Unrelated,
            "outside the recipe the engine takes nothing"
        );
        assert!(!Judgement::Unrelated.taken());
        assert!(!Judgement::Clean.taken());
    }

    #[test]
    fn a_clean_key_lets_the_recipe_go_on_whichever_chord_it_is() {
        for first in [true, false] {
            for wait_left in [true, false] {
                assert_eq!(
                    next_step(Some(Judgement::Clean), first, wait_left),
                    Next::Go
                );
            }
        }
    }

    #[test]
    fn a_modifier_on_the_first_key_is_waited_out_and_then_refused_by_name() {
        let held = Some(Judgement::Held("Ctrl"));
        assert_eq!(next_step(held, true, true), Next::Wait);
        assert_eq!(next_step(held, true, false), Next::Refuse("Ctrl"));
    }

    #[test]
    fn a_modifier_that_turns_up_after_a_key_acted_stops_the_recipe_and_is_not_waited_for() {
        // `Delete` after a clean `Home` and a taken `End` would remove one character.
        for wait_left in [true, false] {
            assert_eq!(
                next_step(Some(Judgement::Held("Alt")), false, wait_left),
                Next::Stop(StopReason::Dropped)
            );
            assert_eq!(
                next_step(Some(Judgement::NoShift), false, wait_left),
                Next::Stop(StopReason::Dropped)
            );
        }
    }

    #[test]
    fn a_shift_that_did_not_arrive_stops_the_recipe_on_the_first_key_too() {
        // The first key has no `Shift`, but the rule does not depend on which key it is.
        assert_eq!(
            next_step(Some(Judgement::NoShift), true, true),
            Next::Stop(StopReason::Dropped)
        );
        assert_eq!(
            next_step(Some(Judgement::Unrelated), true, true),
            Next::Stop(StopReason::Dropped)
        );
    }

    #[test]
    fn a_key_the_engine_did_not_hear_is_a_field_it_does_not_serve() {
        for first in [true, false] {
            assert_eq!(
                next_step(None, first, true),
                Next::Stop(StopReason::NotTaking)
            );
        }
    }

    #[test]
    fn a_modifier_is_named_in_a_fixed_order() {
        assert_eq!(held_name(0), None);
        assert_eq!(held_name(LOCK | NUM_LOCK), None);
        assert_eq!(held_name(CTRL | ALT | SHIFT), Some("Ctrl"));
        assert_eq!(held_name(ALT | SHIFT), Some("Alt"));
        assert_eq!(held_name((1 << 6) | SHIFT), Some("Super"));
    }

    #[test]
    fn the_chord_says_which_key_and_whether_shift_goes_with_it() {
        let end = expected_for(&Chord {
            key: NavKey::End,
            shift: true,
        });
        assert_eq!(end.keysym, KEYSYM_END);
        assert!(end.shifted);
        let home = expected_for(&Chord {
            key: NavKey::Home,
            shift: false,
        });
        assert!(!home.shifted);
    }

    #[test]
    fn the_refusals_have_words_of_their_own_and_a_cancelled_window_is_a_person_saying_no() {
        let words = [
            KeysRefusal::NoPortal.system(),
            KeysRefusal::NotAllowed.system(),
            KeysRefusal::NoAnswer.system(),
        ];
        for (index, one) in words.iter().enumerate() {
            assert!(one.starts_with("Linux under Wayland"));
            for other in &words[index + 1..] {
                assert_ne!(one, other);
            }
        }
        assert_eq!(
            keys_refusal(Unavailable::Cancelled),
            KeysRefusal::NotAllowed
        );
        assert_eq!(keys_refusal(Unavailable::NoAnswer), KeysRefusal::NoAnswer);
        for other in [
            Unavailable::NotHere,
            Unavailable::NoBus,
            Unavailable::Unreachable,
            Unavailable::NoPortal,
            Unavailable::NeedsEntry,
            Unavailable::Ended(2),
            Unavailable::Odd,
            Unavailable::BadWish,
        ] {
            assert_eq!(keys_refusal(other), KeysRefusal::NoPortal, "{other:?}");
        }
    }

    #[test]
    fn the_token_is_kept_under_the_state_directory_of_the_person() {
        let both = StateDirs {
            state_home: Some("/state/"),
            home: Some("/home/a"),
        };
        assert_eq!(
            token_file(&both).as_deref(),
            Some("/state/naughty-keyboard/remote-desktop-token")
        );
        let home = StateDirs {
            state_home: None,
            home: Some("/home/a/"),
        };
        assert_eq!(
            token_file(&home).as_deref(),
            Some("/home/a/.local/state/naughty-keyboard/remote-desktop-token")
        );
        for state_home in [Some(""), Some("relative"), Some("~/s")] {
            let ignored = StateDirs {
                state_home,
                home: Some("/home/a"),
            };
            assert_eq!(
                token_file(&ignored).as_deref(),
                Some("/home/a/.local/state/naughty-keyboard/remote-desktop-token"),
                "{state_home:?} is not a full path and is ignored"
            );
        }
        assert_eq!(token_file(&StateDirs::default()), None);
        assert_eq!(
            token_file(&StateDirs {
                state_home: Some("rel"),
                home: Some("rel"),
            }),
            None
        );
    }

    #[test]
    fn the_file_is_split_into_its_folder_and_its_name() {
        assert_eq!(
            split_token_file("/state/naughty-keyboard/remote-desktop-token"),
            Some(("/state/naughty-keyboard", "remote-desktop-token"))
        );
        for bad in ["", "/", "name", "/name", "dir/"] {
            assert_eq!(split_token_file(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn a_token_is_a_short_string_of_graphic_characters() {
        assert_eq!(usable_token("abc-123\n"), Some("abc-123"));
        assert_eq!(usable_token("  d4f1a2b0-77  "), Some("d4f1a2b0-77"));
        for bad in ["", "   ", "two words", "tab\there", "\u{17c}", "a\0b"] {
            assert_eq!(usable_token(bad), None, "{bad:?}");
        }
        assert!(usable_token(&"a".repeat(TOKEN_LONGEST)).is_some());
        assert_eq!(usable_token(&"a".repeat(TOKEN_LONGEST + 1)), None);
    }

    #[test]
    fn the_waits_are_ordered_so_that_a_busy_desktop_is_not_a_missing_one() {
        assert!(KEY_GAP < ECHO_WAIT);
        assert!(CHORD_GAP < ECHO_WAIT);
        assert!(HELD_RETRY < crate::MODIFIER_RELEASE_WAIT);
        assert_eq!(DEVICE_KEYBOARD, 1);
        assert_eq!(PERSIST_UNTIL_REVOKED, 2);
        assert_eq!(
            REMOTE_DESKTOP_INTERFACE,
            "org.freedesktop.portal.RemoteDesktop"
        );
    }
}
