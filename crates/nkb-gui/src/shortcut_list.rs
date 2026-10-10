//! The shortcuts window's list: rows, the selected one, recording and what
//! the last press came to - with no window (K5.5).
//!
//! # Why this is not in the view and not in `app`
//!
//! The same answer as `picker`: what a key does here and which row is
//! selected are decisions, and a view holds layout and bindings only (GUI
//! rules 11 and 15). What a CHANGE comes to is already in `app` -
//! `KeptSettings::change_shortcut` on the worker (`D90`) - and this type only
//! asks for one and shows the answer.
//!
//! # Recording
//!
//! Enter or a click starts recording the selected row. The chord is read from
//! the system at the press that finishes it (`D91`): a press of a modifier
//! alone is the tester half way through, and says nothing. Esc stops
//! recording, and otherwise closes the window. While a change waits for the
//! worker's answer, nothing else is asked for, so two answers can never cross.

use nkb_adapters::ChordHeld;
use nkb_adapters::i18n::{self, ShortcutsLabel};
use nkb_app::ShortcutChange;
use nkb_app::ports::ShortcutRegistration;
use nkb_core::hotkeys::{Bindings, HotkeyAction, HotkeyChord};

use crate::live::ShortcutsNow;
use crate::picker::Badge;

/// What a key press is, to this window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Up,
    Down,
    Enter,
    /// Delete, or Backspace for a keyboard that has no Delete key.
    Restore,
    Escape,
    /// A modifier on its own - `Shift`, `Ctrl`, `Alt`, `AltGr`, `Win`.
    Modifier,
    Other,
}

/// What `text` - the text Slint gives a key press - is to this window.
///
/// Compared with the toolkit's own names for the keys rather than with
/// characters written here, so a key the toolkit renames does not go quiet.
#[must_use]
pub fn key_of(text: &str) -> Key {
    use slint::platform::Key as Named;
    let is = |key: Named| text == slint::SharedString::from(key).as_str();
    if is(Named::UpArrow) {
        Key::Up
    } else if is(Named::DownArrow) {
        Key::Down
    } else if is(Named::Return) {
        Key::Enter
    } else if is(Named::Escape) {
        Key::Escape
    } else if is(Named::Delete) || is(Named::Backspace) {
        Key::Restore
    } else if crate::packs::MODIFIERS.iter().any(|key| is(*key)) {
        Key::Modifier
    } else {
        Key::Other
    }
}

/// What the window does after a press or a click.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Act {
    /// The press was the window's - redraw what it changed.
    Handled,
    /// Not the window's - it travels on, unhandled.
    NotOurs,
    /// Ask the worker for this change (`live::Command::Shortcut`), then redraw.
    Ask {
        action: HotkeyAction,
        chord: Option<HotkeyChord>,
    },
    /// Close the window.
    Close,
}

/// One row as the window draws it: finished strings and facts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShortcutRow {
    /// The action's name.
    pub title: String,
    /// The chord it answers, written as the hint bar writes it.
    pub key: String,
    pub badge: Option<Badge>,
    /// Being recorded now.
    pub current: bool,
}

/// The list, the selected row, and what the last press came to.
#[derive(Debug, Clone)]
pub struct ShortcutList {
    defaults: Bindings,
    now: ShortcutsNow,
    /// Position in [`HotkeyAction::READING_ORDER`] - every row can be chosen, so there
    /// is always one selected.
    selected: usize,
    recording: bool,
    /// A change asked for and not answered yet: what was asked, for the
    /// sentence the answer gets.
    waiting: Option<(HotkeyAction, Option<HotkeyChord>)>,
    /// What the last press came to, finished.
    messages: Vec<String>,
}

impl ShortcutList {
    /// The list over the table the worker told the window when it paused
    /// (`live::Told::Paused`), the first row selected.
    #[must_use]
    pub fn new(defaults: Bindings, now: ShortcutsNow) -> Self {
        Self {
            defaults,
            now,
            selected: 0,
            recording: false,
            waiting: None,
            messages: Vec::new(),
        }
    }

    /// Applies one press. `held` asks the system which chord is down, and is
    /// asked only while recording - a press anywhere else needs no answer.
    pub fn press(&mut self, key: Key, held: &dyn Fn() -> ChordHeld) -> Act {
        if self.recording {
            return self.record(key, held);
        }
        match key {
            Key::Up => {
                self.selected = self.selected.saturating_sub(1);
                Act::Handled
            }
            Key::Down => {
                // Stays on the last row rather than wrapping, for the reason
                // `PackPicker::next` gives.
                self.selected = (self.selected + 1).min(HotkeyAction::READING_ORDER.len() - 1);
                Act::Handled
            }
            Key::Enter => self.start_recording(),
            Key::Restore => self.ask(None),
            Key::Escape => Act::Close,
            Key::Modifier | Key::Other => Act::NotOurs,
        }
    }

    /// A click on row `row`: selects it and starts recording it.
    pub fn click(&mut self, row: usize) -> Act {
        if row >= HotkeyAction::READING_ORDER.len() {
            return Act::Handled;
        }
        if self.waiting.is_none() {
            self.selected = row;
            self.recording = false;
        }
        self.start_recording()
    }

    /// The worker's answer to the change this list asked for. An answer about
    /// another action - which the list did not ask for - is shown all the same,
    /// because the change it reports happened.
    pub fn answer(
        &mut self,
        action: HotkeyAction,
        change: &ShortcutChange,
        not_saved: Option<String>,
    ) {
        let asked = match self.waiting.take() {
            Some((waited, asked)) if waited == action => asked,
            // Not what was asked: a restore reads as a restore only when it
            // was one, so the sentence is chosen by the answer instead.
            _ => match change {
                ShortcutChange::Changed { bindings, .. }
                    if bindings.chord(action) == self.defaults.chord(action) =>
                {
                    None
                }
                _ => Some(self.now.bindings.chord(action)),
            },
        };
        let answers = self.now.bindings.chord(action);
        if let ShortcutChange::Changed { bindings, .. } = change {
            self.now.bindings = *bindings;
        }
        self.messages = i18n::shortcut_answer(action, asked, change, answers);
        self.messages.extend(not_saved);
    }

    /// Whether a change waits for the worker's answer.
    #[must_use]
    pub const fn waiting(&self) -> bool {
        self.waiting.is_some()
    }

    #[must_use]
    pub const fn recording(&self) -> bool {
        self.recording
    }

    #[must_use]
    pub const fn selected(&self) -> usize {
        self.selected
    }

    /// What the last press came to, a line each.
    #[must_use]
    pub fn messages(&self) -> &[String] {
        &self.messages
    }

    /// How many shortcuts are not the default.
    #[must_use]
    pub fn summary(&self) -> String {
        let changed = HotkeyAction::READING_ORDER
            .iter()
            .filter(|action| self.now.bindings.chord(**action) != self.defaults.chord(**action))
            .count();
        i18n::shortcuts_summary(changed, HotkeyAction::READING_ORDER.len())
    }

    /// Every action, in the order a person reads them (`D120`).
    #[must_use]
    pub fn rows(&self) -> Vec<ShortcutRow> {
        HotkeyAction::READING_ORDER
            .iter()
            .enumerate()
            .map(|(at, action)| {
                let chord = self.now.bindings.chord(*action);
                ShortcutRow {
                    title: i18n::action_name(*action).to_owned(),
                    key: i18n::chord(chord),
                    badge: self.badge(*action, chord),
                    current: self.recording && at == self.selected,
                }
            })
            .collect()
    }

    /// What the system said about `chord` at the last registration, when it
    /// is still the chord registered - then whether the action does anything
    /// in this version, then whether the chord is the tester's own. A risk
    /// outranks the rest, and an action that does nothing outranks a change:
    /// a row that looks like a working shortcut while it is not was
    /// `UX-GUI-006`. Still recordable - the chord stays registered so nobody
    /// else takes it, and the tester may want it elsewhere.
    fn badge(&self, action: HotkeyAction, chord: HotkeyChord) -> Option<Badge> {
        let registered = self
            .now
            .registered
            .iter()
            .find(|(held, was, _)| *held == action && *was == chord)
            .map(|(_, _, outcome)| *outcome);
        let risk = match registered {
            Some(ShortcutRegistration::Taken) => Some(ShortcutsLabel::Taken),
            Some(ShortcutRegistration::Failed { .. }) => Some(ShortcutsLabel::NotRegistered),
            Some(ShortcutRegistration::Registered) | None => None,
        };
        if let Some(label) = risk {
            return Some(Badge {
                text: i18n::shortcuts_label(label).to_owned(),
                risky: true,
                current: false,
            });
        }
        if !crate::live::wired(action) {
            return Some(Badge {
                text: i18n::shortcuts_label(ShortcutsLabel::NotAvailable).to_owned(),
                risky: false,
                current: false,
            });
        }
        (chord != self.defaults.chord(action)).then(|| Badge {
            text: i18n::shortcuts_label(ShortcutsLabel::Changed).to_owned(),
            risky: false,
            current: false,
        })
    }

    fn action(&self) -> HotkeyAction {
        HotkeyAction::READING_ORDER[self.selected]
    }

    fn start_recording(&mut self) -> Act {
        if self.waiting.is_some() {
            return Act::Handled;
        }
        self.recording = true;
        self.messages = vec![i18n::shortcut_invite(self.action())];
        Act::Handled
    }

    fn record(&mut self, key: Key, held: &dyn Fn() -> ChordHeld) -> Act {
        match key {
            Key::Escape => {
                self.recording = false;
                self.messages.clear();
                return Act::Handled;
            }
            // Half way through a chord: nothing to say yet.
            Key::Modifier => return Act::Handled,
            Key::Up | Key::Down | Key::Enter | Key::Restore | Key::Other => {}
        }
        match held() {
            ChordHeld::Chord(chord) => {
                self.recording = false;
                self.ask(Some(chord))
            }
            ChordHeld::NoKey => self.say(ShortcutsLabel::NotAKey),
            ChordHeld::SeveralKeys => self.say(ShortcutsLabel::SeveralKeys),
            ChordHeld::Unknown => {
                self.recording = false;
                self.say(ShortcutsLabel::CannotTell)
            }
        }
    }

    fn ask(&mut self, chord: Option<HotkeyChord>) -> Act {
        if self.waiting.is_some() {
            return Act::Handled;
        }
        let action = self.action();
        self.waiting = Some((action, chord));
        Act::Ask { action, chord }
    }

    fn say(&mut self, label: ShortcutsLabel) -> Act {
        self.messages = vec![i18n::shortcuts_label(label).to_owned()];
        Act::Handled
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use nkb_app::ports::ShortcutsUnavailable;
    use nkb_core::hotkeys::{Refusal, Refused};

    use super::*;

    fn chord(text: &str) -> HotkeyChord {
        HotkeyChord::parse(text).unwrap_or_else(|error| panic!("{text}: {error:?}"))
    }

    fn defaults() -> Bindings {
        nkb_adapters::default_bindings()
    }

    fn a_list() -> ShortcutList {
        ShortcutList::new(
            defaults(),
            ShortcutsNow {
                bindings: defaults(),
                registered: defaults()
                    .as_slice()
                    .iter()
                    .map(|(action, held)| (*action, *held, ShortcutRegistration::Registered))
                    .collect(),
            },
        )
    }

    fn never() -> ChordHeld {
        panic!("the system was asked outside recording")
    }

    #[test]
    fn the_keys_the_window_answers_are_known_by_the_toolkits_own_names() {
        use slint::platform::Key as Named;
        let text = |key: Named| slint::SharedString::from(key).to_string();
        assert_eq!(key_of(&text(Named::UpArrow)), Key::Up);
        assert_eq!(key_of(&text(Named::DownArrow)), Key::Down);
        assert_eq!(key_of(&text(Named::Return)), Key::Enter);
        assert_eq!(key_of(&text(Named::Escape)), Key::Escape);
        assert_eq!(key_of(&text(Named::Delete)), Key::Restore);
        assert_eq!(key_of(&text(Named::Backspace)), Key::Restore);
        assert_eq!(key_of(&text(Named::Shift)), Key::Modifier);
        assert_eq!(key_of(&text(Named::AltGr)), Key::Modifier);
        assert_eq!(key_of("m"), Key::Other);
        assert_eq!(key_of("!"), Key::Other);
    }

    #[test]
    fn arrows_move_and_stay_at_the_ends_and_nothing_is_asked_of_the_system() {
        let mut list = a_list();
        assert_eq!(list.press(Key::Up, &never), Act::Handled);
        assert_eq!(list.selected(), 0);
        for _ in 0..20 {
            list.press(Key::Down, &never);
        }
        assert_eq!(list.selected(), HotkeyAction::READING_ORDER.len() - 1);
        assert_eq!(list.press(Key::Other, &never), Act::NotOurs);
        assert_eq!(list.press(Key::Escape, &never), Act::Close);
    }

    /// 🔴 The chord is read at the press that finishes it: modifiers alone are
    /// the tester half way through and say nothing, and the finished chord is
    /// asked of the worker for the selected action.
    #[test]
    fn enter_records_the_chord_the_system_says_is_held_and_asks_for_it() {
        let mut list = a_list();
        list.press(Key::Down, &never);
        assert_eq!(list.press(Key::Enter, &never), Act::Handled);
        assert!(list.recording());
        assert_eq!(
            list.messages(),
            [i18n::shortcut_invite(HotkeyAction::PreviousValue)]
        );
        assert!(
            list.rows()[1].current,
            "the row being recorded wears the pill"
        );

        assert_eq!(
            list.press(Key::Modifier, &|| ChordHeld::NoKey),
            Act::Handled
        );
        assert_eq!(
            list.messages(),
            [i18n::shortcut_invite(HotkeyAction::PreviousValue)],
            "a modifier alone said something"
        );

        let wanted = chord("Alt+Shift+M");
        assert_eq!(
            list.press(Key::Other, &|| ChordHeld::Chord(wanted)),
            Act::Ask {
                action: HotkeyAction::PreviousValue,
                chord: Some(wanted),
            }
        );
        assert!(!list.recording());
        assert!(list.waiting());
        assert_eq!(
            list.press(Key::Restore, &never),
            Act::Handled,
            "a second change was asked for while the first waits"
        );
    }

    #[test]
    fn a_key_that_ends_no_shortcut_or_two_keys_keep_recording_and_say_why() {
        let mut list = a_list();
        list.press(Key::Enter, &never);
        assert_eq!(list.press(Key::Enter, &|| ChordHeld::NoKey), Act::Handled);
        assert!(list.recording());
        assert_eq!(
            list.messages(),
            [i18n::shortcuts_label(ShortcutsLabel::NotAKey)]
        );
        list.press(Key::Other, &|| ChordHeld::SeveralKeys);
        assert!(list.recording());
        assert_eq!(
            list.messages(),
            [i18n::shortcuts_label(ShortcutsLabel::SeveralKeys)]
        );
        list.press(Key::Other, &|| ChordHeld::Unknown);
        assert!(
            !list.recording(),
            "a system that cannot tell stops recording"
        );
        assert_eq!(
            list.messages(),
            [i18n::shortcuts_label(ShortcutsLabel::CannotTell)]
        );
    }

    #[test]
    fn esc_stops_recording_first_and_closes_only_after() {
        let mut list = a_list();
        list.press(Key::Enter, &never);
        assert_eq!(list.press(Key::Escape, &never), Act::Handled);
        assert!(!list.recording());
        assert!(list.messages().is_empty());
        assert_eq!(list.press(Key::Escape, &never), Act::Close);
    }

    #[test]
    fn delete_asks_for_the_default_back_and_a_click_records_its_row() {
        let mut list = a_list();
        assert_eq!(
            list.press(Key::Restore, &never),
            Act::Ask {
                action: HotkeyAction::NextValue,
                chord: None,
            }
        );
        let mut clicked = a_list();
        assert_eq!(clicked.click(4), Act::Handled);
        assert_eq!(clicked.selected(), 4);
        assert!(clicked.recording());
        assert_eq!(clicked.click(99), Act::Handled);
        assert_eq!(
            clicked.selected(),
            4,
            "a click past the rows moved the selection"
        );
    }

    /// `UX-GUI-006`: exactly the actions that do nothing in this version wear
    /// "not available yet" - neutral, not a risk - and a risk still outranks
    /// it, because a shortcut another application holds is worse news.
    #[test]
    fn an_action_this_version_does_not_carry_out_says_so_in_its_row() {
        let not_available = Badge {
            text: String::from("not available yet"),
            risky: false,
            current: false,
        };
        let list = a_list();
        for (action, row) in HotkeyAction::READING_ORDER.iter().zip(list.rows()) {
            assert_eq!(
                row.badge.as_ref() == Some(&not_available),
                !crate::live::wired(*action),
                "{action:?}: {:?}",
                row.badge
            );
        }
        let repeat = defaults().chord(HotkeyAction::RepeatLast);
        let taken = ShortcutList::new(
            defaults(),
            ShortcutsNow {
                bindings: defaults(),
                registered: vec![(
                    HotkeyAction::RepeatLast,
                    repeat,
                    ShortcutRegistration::Taken,
                )],
            },
        );
        // Found by its action, not by a number: the rows follow the reading
        // order, which moves when an action is added (`D120`).
        let at = HotkeyAction::READING_ORDER
            .iter()
            .position(|action| *action == HotkeyAction::RepeatLast)
            .expect("every action has a row");
        assert_eq!(
            taken.rows()[at]
                .badge
                .as_ref()
                .map(|badge| badge.text.as_str()),
            Some("taken")
        );
    }

    /// The answer changes the rows, the count and the sentence - and a chord
    /// changed since the last registration loses the badge that registration
    /// earned, because nothing is known about the new one yet.
    #[test]
    fn a_change_answered_moves_the_row_counts_it_and_says_so() {
        let previous = defaults().chord(HotkeyAction::PreviousValue);
        let mut list = ShortcutList::new(
            defaults(),
            ShortcutsNow {
                bindings: defaults(),
                registered: vec![(
                    HotkeyAction::PreviousValue,
                    previous,
                    ShortcutRegistration::Taken,
                )],
            },
        );
        assert_eq!(
            list.rows()[1].badge,
            Some(Badge {
                text: String::from("taken"),
                risky: true,
                current: false,
            })
        );
        assert_eq!(list.summary(), "changed: 0 of 14");

        list.press(Key::Down, &never);
        list.press(Key::Enter, &never);
        let wanted = chord("Alt+Shift+M");
        list.press(Key::Other, &|| ChordHeld::Chord(wanted));
        let bindings = defaults()
            .with(&[(HotkeyAction::PreviousValue, wanted)], &|_| None)
            .0;
        let change = ShortcutChange::Changed {
            bindings,
            also: Vec::new(),
            unchecked: None,
        };
        list.answer(
            HotkeyAction::PreviousValue,
            &change,
            Some(String::from("not saved")),
        );

        assert!(!list.waiting());
        let row = &list.rows()[1];
        assert_eq!(row.key, i18n::chord(wanted));
        assert_eq!(
            row.badge,
            Some(Badge {
                text: String::from("changed"),
                risky: false,
                current: false,
            })
        );
        assert_eq!(list.summary(), "changed: 1 of 14");
        let mut expected =
            i18n::shortcut_answer(HotkeyAction::PreviousValue, Some(wanted), &change, previous);
        expected.push(String::from("not saved"));
        assert_eq!(list.messages(), expected.as_slice());
    }

    #[test]
    fn a_refusal_answered_leaves_the_table_and_names_the_reason() {
        let mut list = a_list();
        list.press(Key::Enter, &never);
        let wanted = chord("Alt+Shift+P");
        list.press(Key::Other, &|| ChordHeld::Chord(wanted));
        let change = ShortcutChange::Refused(Refused {
            action: HotkeyAction::NextValue,
            chord: wanted,
            why: Refusal::SameAs(HotkeyAction::PreviousValue),
        });
        list.answer(HotkeyAction::NextValue, &change, None);
        assert_eq!(
            list.rows()[0].key,
            i18n::chord(defaults().chord(HotkeyAction::NextValue))
        );
        assert!(list.messages()[0].contains(i18n::action_name(HotkeyAction::PreviousValue)));

        // A change the system could not check is kept and says so.
        let mut unchecked = a_list();
        unchecked.press(Key::Restore, &never);
        unchecked.answer(
            HotkeyAction::NextValue,
            &ShortcutChange::Changed {
                bindings: defaults(),
                also: Vec::new(),
                unchecked: Some(ShortcutsUnavailable::CouldNotStart),
            },
            None,
        );
        assert_eq!(unchecked.messages().len(), 2);
    }
}
