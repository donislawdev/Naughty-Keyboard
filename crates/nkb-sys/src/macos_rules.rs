//! The decisions of the macOS route that need no system to make.
//!
//! Everything here is a function of its arguments, so it is tested on every
//! system, while only macOS feeds it. The calls that touch the system live in
//! `macos/`, and they ask this module what to do with the answers.
//!
//! # What was measured before this was written
//!
//! Probed 2026-10-19 on macOS 27.0 (arm64) with `tools/sondy/wpisywanie-macos.m`,
//! its own signed bundle, a victim window (`tools/nkb-target`) and the
//! permission granted to that bundle (`D138`):
//!
//! - a key event carrying a string types that string, whatever the layout - Polish
//!   letters, CJK, `U+200B`, `U+00A0`, `U+FEFF`, a right-to-left override and
//!   combining marks all arrived byte for byte (`OBS-70`).
//! - a character above the basic plane arrives only when both halves of its
//!   surrogate pair are in ONE event. Split across two events it is dropped, not
//!   garbled: `a<emoji>b` typed as `ab`. So the unit of work is a scalar value,
//!   one or two UTF-16 units, where Windows works in single units.
//! - the key code must be that of an ordinary character key. With a function
//!   key (`F13`, `F19`) the application receives THAT key and the string is
//!   ignored: the field stayed empty. Code `0` is the letter `A`.
//! - events posted back to back, with no wait, are lost: not one character
//!   arrived from `a`, and 100 microseconds between characters let four of nine
//!   through. The system takes an event asynchronously and `CGEventPost` says
//!   nothing. What does follow is the HID key state, 18 of 18 events, so every
//!   event is waited for until the state shows it (`D94` on Windows).
//! - the application is a separate matter: 20 000 characters were posted in 5 s
//!   and the field took 23 s more. A round trip to the application's main thread
//!   (an accessibility query for the role of the focused element) waits behind
//!   that queue, which paces the send to the application: one query every 25 to
//!   500 characters left the application 0.25 to 0.8 s behind instead of 23 s, and
//!   all 20 000 arrived. A stopped process answers it with an error after exactly
//!   the timeout, 10.0 s of 10 s, and loses nothing it was sent.
//!
//! Where the numbers below are not those measurements, they are written as
//! estimates.

use core::time::Duration;

use crate::confirm::Step;
use crate::field::FocusedInput;
use crate::{Chord, NavKey};

/// The key code of every character event: the letter `A`. Any ordinary
/// character key would do, a function key does not (see the module text).
pub(crate) const TEXT_KEY: u16 = 0x00;

const KEY_COMMAND: u16 = 0x37;
const KEY_SHIFT: u16 = 0x38;
const KEY_BACKSPACE: u16 = 0x33;
const KEY_LEFT: u16 = 0x7B;
const KEY_RIGHT: u16 = 0x7C;

/// Modifier bits of an event, as `CGEventFlags` defines them.
pub(crate) mod flag {
    pub const SHIFT: u64 = 0x0002_0000;
    pub const CONTROL: u64 = 0x0004_0000;
    pub const OPTION: u64 = 0x0008_0000;
    pub const COMMAND: u64 = 0x0010_0000;
}

/// What the person holds on the keyboard that makes the tool's own keys mean
/// something else, as the system's flags and the name to say it by.
const HELD_MODIFIERS: [(u64, &str); 4] = [
    (flag::SHIFT, "Shift"),
    (flag::CONTROL, "Control"),
    (flag::OPTION, "Option"),
    (flag::COMMAND, "Command"),
];

/// The first modifier among `flags`, by name - or `None` when none is held.
pub(crate) fn held_modifier(flags: u64) -> Option<&'static str> {
    HELD_MODIFIERS
        .iter()
        .find(|(mask, _)| flags & mask != 0)
        .map(|(_, name)| *name)
}

/// One event the route posts: a key going down or up, with the modifiers it
/// carries and, for a character, the string it types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MacEvent {
    pub key: u16,
    pub down: bool,
    pub flags: u64,
    /// The UTF-16 units of the character, of which only `unit_count` count.
    pub units: [u16; 2],
    /// Zero for a key that types nothing of its own (a modifier, an arrow).
    pub unit_count: usize,
}

/// One character of a text as the route sends it: one or two UTF-16 units, in
/// one event, because a pair split across events is dropped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Scalar {
    pub units: [u16; 2],
    pub count: usize,
}

/// The characters of `text`, each with its UTF-16 units.
pub(crate) fn scalars(text: &str) -> Vec<Scalar> {
    text.chars()
        .map(|character| {
            let mut units = [0u16; 2];
            let count = character.encode_utf16(&mut units).len();
            Scalar { units, count }
        })
        .collect()
}

/// The two steps of one character: the key down and up, each with the state the
/// system shows once it has taken it.
pub(crate) fn text_steps(scalar: Scalar) -> [Step<MacEvent>; 2] {
    let event = |down| MacEvent {
        key: TEXT_KEY,
        down,
        flags: 0,
        units: scalar.units,
        unit_count: scalar.count,
    };
    [
        Step {
            event: event(true),
            key: TEXT_KEY,
            down: true,
        },
        Step {
            event: event(false),
            key: TEXT_KEY,
            down: false,
        },
    ]
}

/// How many UTF-16 units of a text arrived, when the character after the first
/// `before` units stopped with `taken` of its two events shown. A character goes
/// in on its key-down, so one whose down was shown arrived even if its up was
/// not - the same count `confirm::units_arrived` makes, in units of two.
pub(crate) const fn units_after(before: usize, scalar_units: usize, taken: usize) -> usize {
    before + if taken >= 1 { scalar_units } else { 0 }
}

/// A modifier of a chord: the key that carries it and the bit it sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Lead {
    key: u16,
    mask: u64,
}

const SHIFT: Lead = Lead {
    key: KEY_SHIFT,
    mask: flag::SHIFT,
};
const COMMAND: Lead = Lead {
    key: KEY_COMMAND,
    mask: flag::COMMAND,
};

/// The key and the modifiers that stand for a navigation key on this system.
///
/// The tool's own vocabulary names the MEANING - start of the line, end of the
/// line, remove the selection - and this is where macOS says it: `Home` and `End`
/// are not those there, they scroll the document, and the line is `Command` with
/// an arrow. `Command` stays in this table and out of the vocabulary of the core,
/// which still cannot say "select all".
fn navigation(key: NavKey) -> (u16, &'static [Lead]) {
    match key {
        NavKey::Home => (KEY_LEFT, &[COMMAND]),
        NavKey::End => (KEY_RIGHT, &[COMMAND]),
        NavKey::Delete => (KEY_BACKSPACE, &[]),
    }
}

fn key_event(key: u16, down: bool, flags: u64) -> Step<MacEvent> {
    Step {
        event: MacEvent {
            key,
            down,
            flags,
            units: [0; 2],
            unit_count: 0,
        },
        key,
        down,
    }
}

/// The steps of one chord - the modifiers down, the key down and up, the
/// modifiers up - and how many lead the key, so a stopped chord can say whether
/// its main key went down. Each step carries the flags of the moment: a
/// modifier's own release no longer carries itself.
pub(crate) fn chord_steps(chord: Chord) -> (Vec<Step<MacEvent>>, usize) {
    let (key, system_leads) = navigation(chord.key);
    let mut leads: Vec<Lead> = Vec::with_capacity(2);
    if chord.shift {
        leads.push(SHIFT);
    }
    leads.extend_from_slice(system_leads);

    let mut steps = Vec::with_capacity(2 + 2 * leads.len());
    let mut flags = 0u64;
    for lead in &leads {
        flags |= lead.mask;
        steps.push(key_event(lead.key, true, flags));
    }
    steps.push(key_event(key, true, flags));
    steps.push(key_event(key, false, flags));
    for lead in leads.iter().rev() {
        flags &= !lead.mask;
        steps.push(key_event(lead.key, false, flags));
    }
    (steps, leads.len())
}

/// Whether a chord ACTED, when it stopped with `taken` of its steps shown: its
/// main key went down, which is the step after the modifiers that lead it.
pub(crate) const fn chord_acted(taken: usize, leads: usize) -> bool {
    taken > leads
}

/// What a round trip to the application said about its queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PulseVerdict {
    /// It answered: the queue ahead of the question has been taken.
    Answered,
    /// It did not, for as long as the system waits: the application stopped
    /// taking input - a stopped process answers so, after exactly the timeout.
    Hung,
    /// It refused at once: this application does not answer accessibility
    /// queries, so its queue cannot be followed and the send goes unpaced.
    Unsupported,
}

/// What a round trip's result means, given how long it took against the limit
/// it was allowed. An error that comes after (nearly) the whole limit is the
/// application not answering, and one that comes at once is an application that
/// cannot be asked.
pub(crate) fn pulse_verdict(error: i32, waited: Duration, limit: Duration) -> PulseVerdict {
    if error == 0 {
        PulseVerdict::Answered
    } else if waited >= limit.mul_f32(0.9) {
        PulseVerdict::Hung
    } else {
        PulseVerdict::Unsupported
    }
}

/// How many characters go between two round trips to the application.
///
/// ⚠️ Chosen from the measurement, not derived: 25 to 500 left the application
/// 0.25 to 0.8 s behind at about 10 % more time for the smaller number, and the
/// window in front is looked at before every character anyway. A hundred leaves
/// at most that many characters in its queue when the person presses `Escape`.
pub(crate) const PULSE_EVERY: usize = 64;

/// The role names of the focused element that take no text by definition: keys
/// there act on the control. A closed list, each a type name - never a title or a
/// value - and a role missing from it is `Unknown`, which presses as before.
const NOT_TEXT_ROLES: &[&str] = &[
    "AXButton",
    "AXCheckBox",
    "AXRadioButton",
    "AXPopUpButton",
    "AXMenuButton",
    "AXMenuItem",
    "AXLink",
    "AXImage",
    "AXRow",
    "AXOutline",
    "AXList",
    "AXTable",
    "AXScrollBar",
    "AXSlider",
    "AXToolbar",
    "AXTabGroup",
    "AXDisclosureTriangle",
    "AXStaticText",
    "AXWebArea",
];

/// The role names of a focus that takes typed text.
const TEXT_ROLES: &[&str] = &["AXTextField", "AXTextArea", "AXSearchField", "AXComboBox"];

/// Programs whose keys belong to whatever runs inside them, by the file name of
/// the executable, each measured as a terminal (`D77` is the Windows list).
///
/// ⚠️ Terminal.app only, and not yet measured - an empty answer from a program
/// missing here is a recipe pressed where it should not be, which is why a new
/// entry needs a measurement, not a guess.
pub(crate) const TERMINAL_PROGRAMS: &[&str] = &["Terminal"];

/// What an application that shows no focused element for the whole second look
/// is: a window with nothing inside it for the system to see - Chrome and the
/// Electron applications until accessibility is asked of them - or no window.
pub(crate) const fn silent_answer(has_window: bool) -> FocusedInput {
    if has_window {
        FocusedInput::ClosedToAccessibility
    } else {
        FocusedInput::Unknown
    }
}

/// What the role of the focused element, and the two facts read beside it, make
/// of it. `settable` is whether its value can be set at all and `has_range`
/// whether it has a selected text range - names of attributes, never their
/// contents.
pub(crate) fn classify(
    role: &str,
    settable: bool,
    has_range: bool,
    terminal: bool,
) -> FocusedInput {
    if TEXT_ROLES.contains(&role) {
        if terminal {
            return FocusedInput::Terminal;
        }
        // A field that takes text has a value that can be set and a caret. One
        // that has neither is read-only or disabled: nothing typed there lands.
        return if settable && has_range {
            FocusedInput::TextField
        } else {
            FocusedInput::NotTextField
        };
    }
    if NOT_TEXT_ROLES.contains(&role) {
        FocusedInput::NotTextField
    } else {
        FocusedInput::Unknown
    }
}

/// The bits of `NSWindowCollectionBehavior` the route below puts on a window or
/// takes off it, read from `NSWindow.h` of the SDK (macOS 27.0, 2026-10-21).
pub(crate) mod collection {
    /// Shown in every space.
    pub const CAN_JOIN_ALL_SPACES: u64 = 1 << 0;
    /// Moved to the space that is active when it is ordered front. Exclusive
    /// with the one above.
    pub const MOVE_TO_ACTIVE_SPACE: u64 = 1 << 1;
    /// The window that is the full screen one. At most one of this, the next
    /// and `FULL_SCREEN_NONE`.
    pub const FULL_SCREEN_PRIMARY: u64 = 1 << 7;
    /// Shown with a full screen window, in its space.
    pub const FULL_SCREEN_AUXILIARY: u64 = 1 << 8;
    /// Cannot be made full screen.
    pub const FULL_SCREEN_NONE: u64 = 1 << 9;
}

/// The collection behaviour that lets a window stand in the space of an
/// application in full screen, from the behaviour it has now.
///
/// Two bits are put on - every space, and with a full screen window - and the
/// three that cannot stand beside them are taken off. That is not tidiness: the
/// header says the full screen bits are at most one, and an Objective-C exception
/// raised for a combination that breaks it ends the process, since nothing here
/// can catch one. Every other bit the toolkit set stays as it was.
pub(crate) const fn over_full_screen(behavior: u64) -> u64 {
    use collection::{
        CAN_JOIN_ALL_SPACES, FULL_SCREEN_AUXILIARY, FULL_SCREEN_NONE, FULL_SCREEN_PRIMARY,
        MOVE_TO_ACTIVE_SPACE,
    };
    (behavior & !(MOVE_TO_ACTIVE_SPACE | FULL_SCREEN_PRIMARY | FULL_SCREEN_NONE))
        | CAN_JOIN_ALL_SPACES
        | FULL_SCREEN_AUXILIARY
}

/// Whether a window with this behaviour stands in the space of an application
/// in full screen - the read-back, which is the whole verdict.
pub(crate) const fn stands_over_full_screen(behavior: u64) -> bool {
    use collection::{CAN_JOIN_ALL_SPACES, FULL_SCREEN_AUXILIARY};
    behavior & (CAN_JOIN_ALL_SPACES | FULL_SCREEN_AUXILIARY)
        == CAN_JOIN_ALL_SPACES | FULL_SCREEN_AUXILIARY
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
    fn a_window_with_no_behaviour_gets_the_two_bits() {
        let wanted = collection::CAN_JOIN_ALL_SPACES | collection::FULL_SCREEN_AUXILIARY;
        assert_eq!(over_full_screen(0), wanted);
        assert!(stands_over_full_screen(wanted));
        assert!(!stands_over_full_screen(0));
    }

    #[test]
    fn the_bits_that_cannot_stand_beside_them_are_taken_off() {
        // Left on, FullScreenPrimary and FullScreenAuxiliary together raise an
        // exception in AppKit, and so do the two space bits.
        for old in [
            collection::FULL_SCREEN_PRIMARY,
            collection::FULL_SCREEN_NONE,
            collection::MOVE_TO_ACTIVE_SPACE,
            collection::FULL_SCREEN_PRIMARY | collection::MOVE_TO_ACTIVE_SPACE,
        ] {
            let new = over_full_screen(old);
            assert_eq!(new & collection::FULL_SCREEN_PRIMARY, 0, "{old:#x}");
            assert_eq!(new & collection::FULL_SCREEN_NONE, 0, "{old:#x}");
            assert_eq!(new & collection::MOVE_TO_ACTIVE_SPACE, 0, "{old:#x}");
            assert!(stands_over_full_screen(new), "{old:#x}");
        }
    }

    #[test]
    fn every_other_bit_the_toolkit_set_stays() {
        // Bits 2 to 6 and 11 to 18 are other groups of the header.
        let others: u64 = (0b1_1111 << 2) | (0xFF << 11);
        let new = over_full_screen(others | collection::FULL_SCREEN_PRIMARY);
        assert_eq!(new & others, others);
    }

    #[test]
    fn the_rule_is_idempotent() {
        let once = over_full_screen(collection::FULL_SCREEN_PRIMARY | 0b1_0100);
        assert_eq!(over_full_screen(once), once);
    }

    #[test]
    fn one_of_the_two_bits_is_not_standing_over_full_screen() {
        assert!(!stands_over_full_screen(collection::CAN_JOIN_ALL_SPACES));
        assert!(!stands_over_full_screen(collection::FULL_SCREEN_AUXILIARY));
    }

    fn keys(steps: &[Step<MacEvent>]) -> Vec<(u16, bool, u64)> {
        steps
            .iter()
            .map(|step| (step.event.key, step.event.down, step.event.flags))
            .collect()
    }

    #[test]
    fn a_character_above_the_basic_plane_travels_whole_in_one_event() {
        // Split across events the system drops it (measured), so the unit is the
        // scalar and never the UTF-16 unit.
        let text = scalars("a\u{1F600}b");
        assert_eq!(text.iter().map(|s| s.count).collect::<Vec<_>>(), [1, 2, 1]);
        assert_eq!(text[1].units, [0xD83D, 0xDE00]);
        let [down, up] = text_steps(text[1]);
        assert_eq!(down.event.unit_count, 2);
        assert_eq!(up.event.units, [0xD83D, 0xDE00]);
        assert!(down.down && !up.down);
    }

    #[test]
    fn every_character_is_the_same_ordinary_key_with_no_modifier() {
        // The key code decides what an application sees (a function key makes it
        // ignore the string), and a character carries no modifier of its own.
        for scalar in scalars("Za\u{17C}\u{F3}\u{142}\u{107}\u{4E2D}") {
            for step in text_steps(scalar) {
                assert_eq!(step.event.key, TEXT_KEY);
                assert_eq!(step.key, TEXT_KEY);
                assert_eq!(step.event.flags, 0);
            }
        }
    }

    #[test]
    fn an_empty_text_has_no_characters() {
        assert!(scalars("").is_empty());
    }

    #[test]
    fn a_character_counts_as_arrived_once_its_key_went_down() {
        assert_eq!(units_after(10, 2, 0), 10);
        assert_eq!(units_after(10, 2, 1), 12);
        assert_eq!(units_after(10, 1, 2), 11);
    }

    #[test]
    fn home_is_the_start_of_the_line_with_command_and_nothing_wider() {
        let (steps, leads) = chord_steps(Chord {
            key: NavKey::Home,
            shift: false,
        });
        assert_eq!(leads, 1);
        assert_eq!(
            keys(&steps),
            [
                (KEY_COMMAND, true, flag::COMMAND),
                (KEY_LEFT, true, flag::COMMAND),
                (KEY_LEFT, false, flag::COMMAND),
                (KEY_COMMAND, false, 0),
            ]
        );
    }

    #[test]
    fn shift_end_selects_to_the_end_of_the_line_and_releases_in_reverse() {
        let both = flag::SHIFT | flag::COMMAND;
        let (steps, leads) = chord_steps(Chord {
            key: NavKey::End,
            shift: true,
        });
        assert_eq!(leads, 2);
        assert_eq!(
            keys(&steps),
            [
                (KEY_SHIFT, true, flag::SHIFT),
                (KEY_COMMAND, true, both),
                (KEY_RIGHT, true, both),
                (KEY_RIGHT, false, both),
                (KEY_COMMAND, false, flag::SHIFT),
                (KEY_SHIFT, false, 0),
            ]
        );
    }

    #[test]
    fn delete_is_one_key_with_no_modifier() {
        let (steps, leads) = chord_steps(Chord {
            key: NavKey::Delete,
            shift: false,
        });
        assert_eq!(leads, 0);
        assert_eq!(
            keys(&steps),
            [(KEY_BACKSPACE, true, 0), (KEY_BACKSPACE, false, 0)]
        );
    }

    #[test]
    fn no_chord_of_the_vocabulary_can_select_everything() {
        // The core cannot say "select all" and this table must not say it for
        // the core: no chord presses `A` with `Command`.
        for key in [NavKey::Home, NavKey::End, NavKey::Delete] {
            for shift in [false, true] {
                let (steps, _) = chord_steps(Chord { key, shift });
                assert!(
                    steps.iter().all(|step| step.event.key != TEXT_KEY),
                    "{key:?} shift={shift}"
                );
            }
        }
    }

    #[test]
    fn a_chord_has_acted_once_its_main_key_went_down() {
        // Command alone, then Shift and Command: the key is the step after them.
        assert!(!chord_acted(0, 1));
        assert!(!chord_acted(1, 1));
        assert!(chord_acted(2, 1));
        assert!(!chord_acted(2, 2));
        assert!(chord_acted(3, 2));
        assert!(chord_acted(1, 0));
    }

    #[test]
    fn every_held_modifier_has_a_name_and_none_means_none() {
        assert_eq!(held_modifier(0), None);
        assert_eq!(held_modifier(0x2000_0000), None, "the device bit is no key");
        assert_eq!(held_modifier(flag::SHIFT), Some("Shift"));
        assert_eq!(held_modifier(flag::CONTROL), Some("Control"));
        assert_eq!(held_modifier(flag::OPTION), Some("Option"));
        assert_eq!(held_modifier(flag::COMMAND | 0x2000_0000), Some("Command"));
    }

    #[test]
    fn an_answer_is_answered_a_silence_is_hung_and_a_refusal_is_unsupported() {
        let limit = Duration::from_secs(5);
        assert_eq!(
            pulse_verdict(0, Duration::from_millis(900), limit),
            PulseVerdict::Answered
        );
        // A stopped process: the error comes after exactly the timeout.
        assert_eq!(pulse_verdict(-25204, limit, limit), PulseVerdict::Hung);
        assert_eq!(
            pulse_verdict(-25204, Duration::from_millis(4600), limit),
            PulseVerdict::Hung
        );
        // An application that cannot be asked says so at once.
        assert_eq!(
            pulse_verdict(-25204, Duration::from_millis(3), limit),
            PulseVerdict::Unsupported
        );
        assert_eq!(
            pulse_verdict(-25212, Duration::from_millis(1), limit),
            PulseVerdict::Unsupported
        );
    }

    #[test]
    fn the_verdict_changes_at_nine_tenths_of_the_limit_and_not_before() {
        // The boundary is part of the contract: an error after nine tenths of the
        // wait is the application not answering, one a moment earlier is not.
        let limit = Duration::from_secs(10);
        assert_eq!(
            pulse_verdict(-25204, Duration::from_secs(9), limit),
            PulseVerdict::Hung
        );
        assert_eq!(
            pulse_verdict(-25204, Duration::from_millis(8999), limit),
            PulseVerdict::Unsupported
        );
    }

    #[test]
    fn a_family_of_joined_emoji_is_five_characters_and_eight_units() {
        // Measured in the field: each scalar travels in its own event, the pairs
        // whole, and the field joined them into one cluster (18 bytes).
        let family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}";
        let characters = scalars(family);
        assert_eq!(characters.len(), 5);
        assert_eq!(characters.iter().map(|s| s.count).sum::<usize>(), 8);
        assert_eq!(
            characters.iter().map(|s| s.count).collect::<Vec<_>>(),
            [2, 1, 2, 1, 2]
        );
    }

    #[test]
    fn a_text_role_with_a_settable_value_and_a_caret_is_a_field() {
        for role in TEXT_ROLES {
            assert_eq!(classify(role, true, true, false), FocusedInput::TextField);
        }
    }

    #[test]
    fn a_text_role_that_cannot_take_text_is_not_a_field() {
        // Read-only and disabled fields: a value typed there lands nowhere.
        assert_eq!(
            classify("AXTextField", false, true, false),
            FocusedInput::NotTextField
        );
        assert_eq!(
            classify("AXTextArea", true, false, false),
            FocusedInput::NotTextField
        );
    }

    #[test]
    fn a_terminal_is_its_own_answer_for_a_text_role() {
        assert_eq!(
            classify("AXTextArea", true, true, true),
            FocusedInput::Terminal
        );
    }

    #[test]
    fn terminal_app_reads_as_a_terminal_though_its_value_cannot_be_set() {
        // Measured 2026-10-20: Terminal.app answers `AXTextArea` with a value that
        // cannot be set and a selected range present. Without the program name it
        // would be a read-only area, which is `NotTextField`.
        assert_eq!(
            classify("AXTextArea", false, true, true),
            FocusedInput::Terminal
        );
        assert_eq!(
            classify("AXTextArea", false, true, false),
            FocusedInput::NotTextField
        );
    }

    #[test]
    fn the_kinds_of_page_field_safari_and_chrome_report_are_all_fields() {
        // Measured 2026-10-20 in Safari and in Chrome once its tree was awake: the
        // roles and subroles are the ones the page fields (text, search, email,
        // password, textarea, contenteditable) answered, and the subrole is never
        // asked, so a password field is a field like any other.
        for role in ["AXTextField", "AXTextArea"] {
            assert_eq!(classify(role, true, true, false), FocusedInput::TextField);
        }
    }

    #[test]
    fn an_application_that_shows_no_focus_is_closed_only_with_a_window_in_front() {
        // Chrome until it is asked, measured: every request over 2.4 s answered
        // "no value" while the application had its window in front.
        assert_eq!(silent_answer(true), FocusedInput::ClosedToAccessibility);
        assert_eq!(silent_answer(false), FocusedInput::Unknown);
    }

    #[test]
    fn a_control_that_takes_no_text_is_not_a_field() {
        for role in NOT_TEXT_ROLES {
            assert_eq!(
                classify(role, false, false, false),
                FocusedInput::NotTextField,
                "{role}"
            );
            // Settable or not, a terminal program or not: the role decides.
            assert_eq!(
                classify(role, true, true, true),
                FocusedInput::NotTextField,
                "{role}"
            );
        }
    }

    #[test]
    fn a_role_nobody_listed_is_unknown_never_a_guess() {
        for role in [
            "AXGroup",
            "AXUnknown",
            "AXScrollArea",
            "AXCell",
            "",
            "axtextfield",
        ] {
            assert_eq!(
                classify(role, true, true, false),
                FocusedInput::Unknown,
                "{role}"
            );
        }
    }

    #[test]
    fn a_question_to_the_application_comes_often_enough_to_bound_what_is_queued() {
        // Two events per character, so the question comes every 128 of them. A
        // number of 0 would ask before every event, a very large one never.
        assert!((1..=500).contains(&PULSE_EVERY));
    }

    #[test]
    fn a_terminal_is_named_by_the_file_of_its_program_whole_and_with_case() {
        assert!(TERMINAL_PROGRAMS.contains(&"Terminal"));
        assert!(!TERMINAL_PROGRAMS.contains(&"terminal"));
        assert!(TERMINAL_PROGRAMS.iter().all(|name| !name.contains('/')));
    }

    #[test]
    fn the_two_role_lists_do_not_overlap() {
        for role in TEXT_ROLES {
            assert!(!NOT_TEXT_ROLES.contains(role), "{role} is in both lists");
        }
    }
}
