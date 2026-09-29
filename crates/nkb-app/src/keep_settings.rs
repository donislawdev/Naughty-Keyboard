//! Remembering the palette's settings between runs, and saying so when it
//! cannot.
//!
//! # Why this is a use case and not three lines in the palette
//!
//! `architektura.md` 6.4 puts the decision here: a machine where nothing can
//! be written is a STATE, the tool says so once and keeps working. Three
//! rules follow, and each is easy to lose in a window's event handler:
//!
//! - after a load that could not be used, nothing is saved - not "tried and
//!   failed", NOT ATTEMPTED - because the file on disk is the tester's and a
//!   save would replace it with a file holding only what this version
//!   understands (document `15` section 10).
//! - a failure is said once per kind, not once per press: a palette that
//!   repeats "could not save" on every collapse teaches the tester to stop
//!   reading its messages.
//! - a change that changes nothing is not written, so the file keeps the
//!   tester's own bytes until the tester actually changes something.
//!
//! # The pack the palette opens on
//!
//! In the order a tester would expect: the pack asked for now, then the one
//! remembered, then the default. Only a pack asked for is remembered (`D84`).
//! A remembered pack that cannot be opened today gives way to the default for
//! THIS run and stays remembered: a team folder that is offline for an hour
//! must not erase the choice the tester made last week.

use std::mem::{Discriminant, discriminant};

use nkb_core::hotkeys::{Bindings, Refused};

use crate::advance_sequence::ChooseError;
use crate::ports::{
    SaveError, SettingChange, Settings, SettingsLoad, SettingsNote, SettingsStore, SettingsUnusable,
};

/// Something about the settings worth telling the tester.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsMessage {
    /// There is no place for settings, so nothing will be remembered.
    Nowhere { missing: String },
    /// A file is there and cannot be used: the palette starts from the
    /// defaults and saves nothing over it.
    Unusable(SettingsUnusable),
    /// Something in a readable file was not used.
    Note(SettingsNote),
    /// A change was not saved. Said the first time each kind happens.
    NotSaved(SaveError),
    /// The remembered pack could not be opened, so the palette opened another
    /// one - and the remembered one stays remembered.
    RememberedPackUnavailable { remembered: String, opened: String },
    /// A shortcut the tester wrote reads, and still cannot be used - the
    /// action keeps its default. Why is in the refusal.
    ShortcutNotUsed(Refused),
}

/// What opening a pack had to say, in the order it happened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Opening {
    /// This pack was tried and could not be chosen.
    CouldNotChoose { pack: String, error: ChooseError },
    /// Something about the settings.
    Settings(SettingsMessage),
}

/// Which pack the palette opened, and what was said on the way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opened {
    /// The pack tried last. When nothing could be chosen it is still the name
    /// the tester has to hear, because it is the one the messages are about.
    pub pack: String,
    pub notes: Vec<Opening>,
}

/// Whether saves are attempted at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Keeping {
    Saving,
    /// The load already said why nothing is saved - no place, or a file that
    /// cannot be used - so no save is attempted and nothing more is said.
    NotSaving,
}

/// The settings for one run of the palette.
#[derive(Debug, Clone)]
pub struct KeptSettings {
    /// What is in effect now: what was read, with every change of this run on
    /// top, saved or not.
    settings: Settings,
    keeping: Keeping,
    /// The kinds of save failure already said, so each is said once. Emptied
    /// by a save that works, because a failure that comes back after a
    /// recovery is news again.
    said: Vec<Discriminant<SaveError>>,
}

impl KeptSettings {
    /// Reads the settings, and what the tester has to hear about them.
    #[must_use]
    pub fn open(store: &dyn SettingsStore) -> (Self, Vec<SettingsMessage>) {
        let (settings, keeping, messages) = match store.load() {
            SettingsLoad::Absent => (Settings::default(), Keeping::Saving, Vec::new()),
            SettingsLoad::Read { settings, notes } => (
                settings,
                Keeping::Saving,
                notes.into_iter().map(SettingsMessage::Note).collect(),
            ),
            SettingsLoad::Unusable(why) => (
                Settings::default(),
                Keeping::NotSaving,
                vec![SettingsMessage::Unusable(why)],
            ),
            SettingsLoad::Nowhere { missing } => (
                Settings::default(),
                Keeping::NotSaving,
                vec![SettingsMessage::Nowhere { missing }],
            ),
        };
        (
            Self {
                settings,
                keeping,
                said: Vec::new(),
            },
            messages,
        )
    }

    /// What is in effect now.
    #[must_use]
    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// The shortcuts for this run: `defaults` with the tester's own on top,
    /// and a message for each one that could not be used.
    ///
    /// Asked once, at start: the file is not read again while the palette
    /// runs (`settings-format.md` 8), so the answer holds for the whole run.
    #[must_use]
    pub fn bindings(&self, defaults: Bindings) -> (Bindings, Vec<SettingsMessage>) {
        let (bindings, refused) = defaults.with(&self.settings.shortcuts);
        (
            bindings,
            refused
                .into_iter()
                .map(SettingsMessage::ShortcutNotUsed)
                .collect(),
        )
    }

    /// Remembers one change - in effect at once, saved when it can be.
    ///
    /// Returns a message only for a failure of a kind not said before.
    pub fn keep(
        &mut self,
        store: &dyn SettingsStore,
        change: SettingChange,
    ) -> Option<SettingsMessage> {
        if self.holds(&change) {
            return None;
        }
        let saved = match self.keeping {
            Keeping::NotSaving => Ok(()),
            Keeping::Saving => store.save(&change),
        };
        // In effect for this run whether or not it reached the disk: the
        // tester collapsed the palette, and it stays collapsed.
        self.apply(change);
        match saved {
            Ok(()) => {
                self.said.clear();
                None
            }
            Err(error) => {
                let kind = discriminant(&error);
                if self.said.contains(&kind) {
                    None
                } else {
                    self.said.push(kind);
                    Some(SettingsMessage::NotSaved(error))
                }
            }
        }
    }

    /// Opens the pack the palette starts on, through `choose`.
    ///
    /// `asked` is the pack named on the command line, if any. It is remembered
    /// when it opens. The remembered pack is tried next, and the default last -
    /// and a default reached only because the remembered pack failed is NOT
    /// remembered (the module comment says why).
    pub fn open_pack(
        &mut self,
        store: &dyn SettingsStore,
        asked: Option<&str>,
        default: &str,
        choose: &mut dyn FnMut(&str) -> Result<(), ChooseError>,
    ) -> Opened {
        let mut notes = Vec::new();
        if let Some(pack) = asked {
            match choose(pack) {
                Ok(()) => {
                    if let Some(message) = self.keep(store, SettingChange::Pack(pack.to_owned())) {
                        notes.push(Opening::Settings(message));
                    }
                }
                Err(error) => notes.push(Opening::CouldNotChoose {
                    pack: pack.to_owned(),
                    error,
                }),
            }
            return Opened {
                pack: pack.to_owned(),
                notes,
            };
        }

        if let Some(remembered) = self.settings.pack.clone() {
            match choose(&remembered) {
                Ok(()) => {
                    return Opened {
                        pack: remembered,
                        notes,
                    };
                }
                Err(error) => notes.push(Opening::CouldNotChoose {
                    pack: remembered.clone(),
                    error,
                }),
            }
            if remembered == default {
                // The default IS what failed. Trying it again would say the
                // same thing twice and change nothing.
                return Opened {
                    pack: remembered,
                    notes,
                };
            }
            match choose(default) {
                Ok(()) => notes.push(Opening::Settings(
                    SettingsMessage::RememberedPackUnavailable {
                        remembered,
                        opened: default.to_owned(),
                    },
                )),
                Err(error) => notes.push(Opening::CouldNotChoose {
                    pack: default.to_owned(),
                    error,
                }),
            }
            return Opened {
                pack: default.to_owned(),
                notes,
            };
        }

        if let Err(error) = choose(default) {
            notes.push(Opening::CouldNotChoose {
                pack: default.to_owned(),
                error,
            });
        }
        Opened {
            pack: default.to_owned(),
            notes,
        }
    }

    /// Whether this change is already in effect, so saving it changes nothing.
    ///
    /// Compared with what is IN EFFECT, not with a default: a setting the file
    /// does not hold is not the same as one set to its default value, and
    /// only the caller knows the default.
    fn holds(&self, change: &SettingChange) -> bool {
        match change {
            SettingChange::Pack(pack) => self.settings.pack.as_deref() == Some(pack.as_str()),
            SettingChange::Compact(compact) => self.settings.compact == Some(*compact),
        }
    }

    fn apply(&mut self, change: SettingChange) {
        match change {
            SettingChange::Pack(pack) => self.settings.pack = Some(pack),
            SettingChange::Compact(compact) => self.settings.compact = Some(compact),
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
    use std::cell::RefCell;

    use super::*;

    /// A store that answers what a test dictates and keeps every save it was
    /// asked for.
    struct FakeStore {
        load: SettingsLoad,
        fail: RefCell<Option<SaveError>>,
        saved: RefCell<Vec<SettingChange>>,
    }

    impl FakeStore {
        fn loading(load: SettingsLoad) -> Self {
            Self {
                load,
                fail: RefCell::new(None),
                saved: RefCell::new(Vec::new()),
            }
        }
        fn failing_with(self, error: SaveError) -> Self {
            *self.fail.borrow_mut() = Some(error);
            self
        }
        fn saves(&self) -> Vec<SettingChange> {
            self.saved.borrow().clone()
        }
    }

    impl SettingsStore for FakeStore {
        fn load(&self) -> SettingsLoad {
            self.load.clone()
        }
        fn save(&self, change: &SettingChange) -> Result<(), SaveError> {
            self.saved.borrow_mut().push(change.clone());
            match self.fail.borrow().clone() {
                Some(error) => Err(error),
                None => Ok(()),
            }
        }
    }

    fn read(pack: Option<&str>, compact: Option<bool>) -> SettingsLoad {
        SettingsLoad::Read {
            settings: Settings {
                pack: pack.map(ToOwned::to_owned),
                compact,
                shortcuts: Vec::new(),
            },
            notes: Vec::new(),
        }
    }

    /// A `choose` that accepts only the packs it is given, and records every
    /// pack it was asked for.
    fn chooser<'a>(
        exists: &'a [&'a str],
        asked: &'a RefCell<Vec<String>>,
    ) -> impl FnMut(&str) -> Result<(), ChooseError> + 'a {
        move |pack| {
            asked.borrow_mut().push(pack.to_owned());
            if exists.contains(&pack) {
                Ok(())
            } else {
                Err(ChooseError::NotFound)
            }
        }
    }

    #[test]
    fn a_first_run_reads_nothing_says_nothing_and_saves_the_first_change() {
        let store = FakeStore::loading(SettingsLoad::Absent);
        let (mut kept, said) = KeptSettings::open(&store);
        assert!(said.is_empty(), "a first run is not news: {said:?}");
        assert_eq!(kept.settings(), &Settings::default());

        assert_eq!(kept.keep(&store, SettingChange::Compact(true)), None);
        assert_eq!(store.saves(), vec![SettingChange::Compact(true)]);
        assert_eq!(kept.settings().compact, Some(true));
    }

    #[test]
    fn a_file_that_cannot_be_used_is_said_once_and_never_saved_over() {
        // 🔴 The rule the whole module exists for: the file is the tester's
        // only copy, and a save would replace it with what this version
        // understands of it.
        let why = SettingsUnusable::NotToml {
            line: 3,
            detail: String::from("unclosed table"),
        };
        let store = FakeStore::loading(SettingsLoad::Unusable(why.clone()));
        let (mut kept, said) = KeptSettings::open(&store);
        assert_eq!(said, vec![SettingsMessage::Unusable(why)]);

        assert_eq!(kept.keep(&store, SettingChange::Compact(true)), None);
        assert_eq!(
            kept.keep(&store, SettingChange::Pack(String::from("x"))),
            None
        );
        assert!(
            store.saves().is_empty(),
            "a save was attempted over a file that could not be read: {:?}",
            store.saves()
        );
        assert_eq!(
            kept.settings().compact,
            Some(true),
            "the change is still in effect for this run"
        );
    }

    #[test]
    fn no_place_for_settings_is_said_once_and_nothing_is_attempted() {
        let store = FakeStore::loading(SettingsLoad::Nowhere {
            missing: String::from("APPDATA"),
        });
        let (mut kept, said) = KeptSettings::open(&store);
        assert_eq!(
            said,
            vec![SettingsMessage::Nowhere {
                missing: String::from("APPDATA")
            }]
        );
        assert_eq!(kept.keep(&store, SettingChange::Compact(true)), None);
        assert!(store.saves().is_empty());
    }

    #[test]
    fn notes_about_a_readable_file_reach_the_tester_and_saving_goes_on() {
        let note = SettingsNote::UnknownKeys {
            keys: vec![String::from("palette.compct")],
        };
        let store = FakeStore::loading(SettingsLoad::Read {
            settings: Settings::default(),
            notes: vec![note.clone()],
        });
        let (mut kept, said) = KeptSettings::open(&store);
        assert_eq!(said, vec![SettingsMessage::Note(note)]);
        let _ = kept.keep(&store, SettingChange::Compact(true));
        assert_eq!(store.saves(), vec![SettingChange::Compact(true)]);
    }

    #[test]
    fn a_failed_save_is_said_once_per_kind_and_again_after_a_recovery() {
        let store = FakeStore::loading(SettingsLoad::Absent).failing_with(SaveError::Unwritable);
        let (mut kept, _) = KeptSettings::open(&store);

        assert_eq!(
            kept.keep(&store, SettingChange::Compact(true)),
            Some(SettingsMessage::NotSaved(SaveError::Unwritable))
        );
        assert_eq!(
            kept.keep(&store, SettingChange::Compact(false)),
            None,
            "the same failure twice is said once"
        );
        assert_eq!(
            store.saves().len(),
            2,
            "every change is still tried - a failure may pass"
        );

        // A different kind is news.
        *store.fail.borrow_mut() = Some(SaveError::Unusable(SettingsUnusable::NotUtf8));
        assert_eq!(
            kept.keep(&store, SettingChange::Compact(true)),
            Some(SettingsMessage::NotSaved(SaveError::Unusable(
                SettingsUnusable::NotUtf8
            )))
        );

        // A save that works forgets what was said, so the next failure is
        // news again.
        *store.fail.borrow_mut() = None;
        assert_eq!(kept.keep(&store, SettingChange::Compact(false)), None);
        *store.fail.borrow_mut() = Some(SaveError::Unwritable);
        assert_eq!(
            kept.keep(&store, SettingChange::Compact(true)),
            Some(SettingsMessage::NotSaved(SaveError::Unwritable))
        );
    }

    #[test]
    fn a_change_that_changes_nothing_is_not_written() {
        let store = FakeStore::loading(read(Some("whitespace"), Some(true)));
        let (mut kept, _) = KeptSettings::open(&store);
        assert_eq!(
            kept.keep(&store, SettingChange::Pack(String::from("whitespace"))),
            None
        );
        assert_eq!(kept.keep(&store, SettingChange::Compact(true)), None);
        assert!(store.saves().is_empty(), "{:?}", store.saves());
    }

    #[test]
    fn a_setting_the_file_does_not_hold_is_written_even_as_its_default() {
        // `None` is "the file does not say", not "false". Only the caller
        // knows the default, so the use case cannot fold the two.
        let store = FakeStore::loading(SettingsLoad::Absent);
        let (mut kept, _) = KeptSettings::open(&store);
        let _ = kept.keep(&store, SettingChange::Compact(false));
        assert_eq!(store.saves(), vec![SettingChange::Compact(false)]);
    }

    #[test]
    fn a_pack_asked_for_opens_and_is_remembered() {
        let store = FakeStore::loading(read(Some("whitespace"), None));
        let (mut kept, _) = KeptSettings::open(&store);
        let asked = RefCell::new(Vec::new());
        let opened = kept.open_pack(
            &store,
            Some("unicode-text"),
            "whitespace",
            &mut chooser(&["unicode-text", "whitespace"], &asked),
        );
        assert_eq!(opened.pack, "unicode-text");
        assert!(opened.notes.is_empty());
        assert_eq!(
            store.saves(),
            vec![SettingChange::Pack(String::from("unicode-text"))]
        );
    }

    #[test]
    fn a_pack_asked_for_that_fails_is_neither_replaced_nor_remembered() {
        // An explicit request is answered as asked: the tester named it, and
        // opening something else in its place would hide the typo.
        let store = FakeStore::loading(read(Some("whitespace"), None));
        let (mut kept, _) = KeptSettings::open(&store);
        let asked = RefCell::new(Vec::new());
        let opened = kept.open_pack(
            &store,
            Some("unicode-txt"),
            "whitespace",
            &mut chooser(&["whitespace"], &asked),
        );
        assert_eq!(opened.pack, "unicode-txt");
        assert_eq!(
            opened.notes,
            vec![Opening::CouldNotChoose {
                pack: String::from("unicode-txt"),
                error: ChooseError::NotFound
            }]
        );
        assert_eq!(*asked.borrow(), vec![String::from("unicode-txt")]);
        assert!(store.saves().is_empty());
    }

    #[test]
    fn with_nothing_asked_the_remembered_pack_opens_and_nothing_is_written() {
        let store = FakeStore::loading(read(Some("unicode-text"), None));
        let (mut kept, _) = KeptSettings::open(&store);
        let asked = RefCell::new(Vec::new());
        let opened = kept.open_pack(
            &store,
            None,
            "whitespace",
            &mut chooser(&["unicode-text", "whitespace"], &asked),
        );
        assert_eq!(opened.pack, "unicode-text");
        assert!(opened.notes.is_empty());
        assert!(store.saves().is_empty());
    }

    #[test]
    fn a_remembered_pack_that_cannot_open_gives_way_to_the_default_and_stays_remembered() {
        // 🔴 A team folder offline for an hour must not erase last week's
        // choice: the default opens for this run, and nothing is written.
        let store = FakeStore::loading(read(Some("team-pack"), None));
        let (mut kept, _) = KeptSettings::open(&store);
        let asked = RefCell::new(Vec::new());
        let opened = kept.open_pack(
            &store,
            None,
            "whitespace",
            &mut chooser(&["whitespace"], &asked),
        );
        assert_eq!(opened.pack, "whitespace");
        assert_eq!(
            opened.notes,
            vec![
                Opening::CouldNotChoose {
                    pack: String::from("team-pack"),
                    error: ChooseError::NotFound
                },
                Opening::Settings(SettingsMessage::RememberedPackUnavailable {
                    remembered: String::from("team-pack"),
                    opened: String::from("whitespace"),
                }),
            ]
        );
        assert!(
            store.saves().is_empty(),
            "the stand-in was saved: {:?}",
            store.saves()
        );
        assert_eq!(kept.settings().pack.as_deref(), Some("team-pack"));
    }

    #[test]
    fn a_remembered_default_that_fails_is_tried_once() {
        let store = FakeStore::loading(read(Some("whitespace"), None));
        let (mut kept, _) = KeptSettings::open(&store);
        let asked = RefCell::new(Vec::new());
        let opened = kept.open_pack(&store, None, "whitespace", &mut chooser(&[], &asked));
        assert_eq!(opened.pack, "whitespace");
        assert_eq!(*asked.borrow(), vec![String::from("whitespace")]);
        assert_eq!(opened.notes.len(), 1);
    }

    #[test]
    fn with_nothing_asked_or_remembered_the_default_opens_and_is_not_written() {
        let store = FakeStore::loading(SettingsLoad::Absent);
        let (mut kept, _) = KeptSettings::open(&store);
        let asked = RefCell::new(Vec::new());
        let opened = kept.open_pack(
            &store,
            None,
            "whitespace",
            &mut chooser(&["whitespace"], &asked),
        );
        assert_eq!(opened.pack, "whitespace");
        assert!(opened.notes.is_empty());
        assert!(store.saves().is_empty());
    }

    #[test]
    fn a_pack_opened_but_not_saved_is_said_like_any_other_failed_save() {
        let store = FakeStore::loading(SettingsLoad::Absent).failing_with(SaveError::Unwritable);
        let (mut kept, _) = KeptSettings::open(&store);
        let asked = RefCell::new(Vec::new());
        let opened = kept.open_pack(
            &store,
            Some("whitespace"),
            "whitespace",
            &mut chooser(&["whitespace"], &asked),
        );
        assert_eq!(
            opened.notes,
            vec![Opening::Settings(SettingsMessage::NotSaved(
                SaveError::Unwritable
            ))]
        );
    }
    #[test]
    fn the_shortcuts_of_a_run_are_the_files_on_top_of_the_defaults_and_a_refusal_is_said() {
        use nkb_core::hotkeys::{Convention, HotkeyAction, HotkeyChord, Refusal};
        let chord = |text: &str| HotkeyChord::parse(text).unwrap_or_else(|e| panic!("{e:?}"));
        let store = FakeStore::loading(SettingsLoad::Read {
            settings: Settings {
                shortcuts: vec![
                    (HotkeyAction::NextValue, chord("Alt+Shift+M")),
                    (HotkeyAction::PreviousValue, chord("Alt+Shift+R")),
                ],
                ..Settings::default()
            },
            notes: Vec::new(),
        });
        let (kept, said) = KeptSettings::open(&store);
        assert!(said.is_empty(), "{said:?}");
        let defaults = Bindings::defaults(Convention::WindowsAndLinux);
        let (bindings, messages) = kept.bindings(defaults);
        assert_eq!(
            bindings.chord(HotkeyAction::NextValue),
            chord("Alt+Shift+M")
        );
        assert_eq!(
            bindings.chord(HotkeyAction::PreviousValue),
            defaults.chord(HotkeyAction::PreviousValue),
            "Alt+Shift+R is Repeat last value's"
        );
        assert_eq!(
            messages,
            vec![SettingsMessage::ShortcutNotUsed(Refused {
                action: HotkeyAction::PreviousValue,
                chord: chord("Alt+Shift+R"),
                why: Refusal::SameAs(HotkeyAction::RepeatLast),
            })]
        );
        assert!(store.saves().is_empty(), "reading shortcuts writes nothing");
    }
}
