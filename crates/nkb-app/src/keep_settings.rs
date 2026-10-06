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

use nkb_core::hotkeys::{Bindings, HotkeyAction, HotkeyChord, Refusal, Refused};

use crate::advance_sequence::ChooseError;
use crate::ports::{
    HotkeyRegistrar, SaveError, SettingChange, Settings, SettingsLoad, SettingsNote, SettingsStore,
    SettingsUnusable, ShortcutRegistration, ShortcutsUnavailable,
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
    /// `types` is the system's answer to which character a `Ctrl+Alt` chord
    /// types as `AltGr` - see [`Bindings::with`], which decides when to ask.
    #[must_use]
    pub fn bindings(
        &self,
        defaults: Bindings,
        types: &dyn Fn(HotkeyChord) -> Option<char>,
    ) -> (Bindings, Vec<SettingsMessage>) {
        let (bindings, refused) = defaults.with(&self.settings.shortcuts, types);
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

    /// What giving `action` the shortcut `chord` - or its default back, for
    /// `None` - would come to, before anything is saved (K5).
    ///
    /// The table the shortcuts window shows after the change is the table the
    /// next start computes from the file the change leaves behind - the same
    /// rule, [`Bindings::with`] over the file's wishes. What may be refused is
    /// decided BEFORE that, against the table in use NOW:
    ///
    /// - a chord that is no global shortcut, or that a keyboard layout types
    ///   as `AltGr` - the reasons the palette gives about the settings file.
    /// - a chord another action answers to now - recorded, or the default a
    ///   restore asks for - refused, naming that action, rather than taken from
    ///   it. The tester gives the other action something else first.
    ///
    /// 🔴 Not left to [`Bindings::with`], and measured why: it sends back the
    /// later of two wishes in the order of [`HotkeyAction::ALL`], not the
    /// order of the file, so a recording that clashed with a swap two actions
    /// had made unravelled the swap and named an action that no longer held
    /// the chord (the exhaustive test below found it).
    ///
    /// Once the chord is free the change can only GIVE: the chord the action
    /// leaves may let another action's wish in the file come true. That is
    /// the file's own wish and it happens, but it is named in `also`, because
    /// a row that changes without being touched looks like a fault.
    ///
    /// ⚠️ `AlreadySo` for a RESTORE does not prove the file holds no key for
    /// the action: a value that did not read as a shortcut is not in the
    /// settings at all. The caller may save the restore anyway - the store
    /// writes nothing when the key is not there.
    #[must_use]
    pub fn consider(
        &self,
        defaults: Bindings,
        types: &dyn Fn(HotkeyChord) -> Option<char>,
        action: HotkeyAction,
        chord: Option<HotkeyChord>,
    ) -> Considered {
        let refuse = |chord, why| Considered::Refused(Refused { action, chord, why });
        if let Some(chord) = chord {
            if let Some(problem) = chord.problem() {
                return refuse(chord, Refusal::Problem(problem));
            }
            if let Some(character) = chord.could_be_altgr().then(|| types(chord)).flatten() {
                return refuse(chord, Refusal::TypesCharacter(character));
            }
        }
        let (now, _) = defaults.with(&self.settings.shortcuts, types);
        let default = defaults.chord(action);
        let wanted = chord.unwrap_or(default);
        if let Some(holder) = HotkeyAction::ALL
            .iter()
            .copied()
            .find(|other| *other != action && now.chord(*other) == wanted)
        {
            return refuse(wanted, Refusal::SameAs(holder));
        }
        let mut after = self.settings.shortcuts.clone();
        wish(&mut after, action, chord);
        let (next, _) = defaults.with(&after, types);
        let in_file = self
            .settings
            .shortcuts
            .iter()
            .find_map(|(wished, held)| (*wished == action).then_some(*held));
        // A default the file does not name needs no key saying it.
        let file_says_it = in_file == chord || (in_file.is_none() && chord == Some(default));
        if next == now && file_says_it {
            return Considered::AlreadySo;
        }
        Considered::Gives {
            bindings: next,
            also: HotkeyAction::ALL
                .iter()
                .copied()
                .filter(|other| *other != action && next.chord(*other) != now.chord(*other))
                .collect(),
        }
    }

    /// Gives `action` the shortcut `chord` - or its default back, for `None` -
    /// when it can be had, and remembers it (K5).
    ///
    /// Three steps, in an order that is the point:
    ///
    /// 1. [`KeptSettings::consider`] - a chord refused there is refused here,
    ///    and nothing is asked of the system.
    /// 2. The system is asked whether the chord can be registered, by
    ///    registering it alone and releasing it at once. A chord another
    ///    program holds is refused BEFORE anything is saved, so the file never
    ///    names a shortcut that will not respond. The palette's own shortcuts
    ///    are given back while the shortcuts window is open, so the answer is
    ///    about other programs - and a chord the action already answers to is
    ///    not asked about at all, because our own registration would answer
    ///    `Taken` for it.
    /// 3. The change is kept: in effect at once, saved when it can be.
    ///
    /// 🔴 The trial registration is released before this returns. A handle
    /// kept past it would hold the chord against the palette's own
    /// registration when the window closes.
    ///
    /// A restore that changes nothing is still saved (`D89`): the key it is
    /// meant to clear may be one that did not read as a shortcut, which the
    /// settings cannot see and the store removes.
    ///
    /// The second value is a failure to save, of a kind not said before.
    pub fn change_shortcut(
        &mut self,
        store: &dyn SettingsStore,
        registrar: &dyn HotkeyRegistrar,
        defaults: Bindings,
        types: &dyn Fn(HotkeyChord) -> Option<char>,
        action: HotkeyAction,
        chord: Option<HotkeyChord>,
    ) -> (ShortcutChange, Option<SettingsMessage>) {
        let change = SettingChange::Shortcut { action, chord };
        let (bindings, also) = match self.consider(defaults, types, action, chord) {
            Considered::Refused(refused) => return (ShortcutChange::Refused(refused), None),
            Considered::AlreadySo => {
                let said = chord.is_none().then(|| self.keep(store, change)).flatten();
                return (ShortcutChange::AlreadySo, said);
            }
            Considered::Gives { bindings, also } => (bindings, also),
        };
        let wanted = bindings.chord(action);
        let (now, _) = defaults.with(&self.settings.shortcuts, types);
        let unchecked = if wanted == now.chord(action) {
            None
        } else {
            match trial(registrar, action, wanted) {
                Trial::Free => None,
                Trial::Taken => return (ShortcutChange::Taken { chord: wanted }, None),
                Trial::Failed { code } => {
                    return (
                        ShortcutChange::NotRegistered {
                            chord: wanted,
                            code,
                        },
                        None,
                    );
                }
                Trial::Unchecked(why) => Some(why),
            }
        };
        let said = self.keep(store, change);
        (
            ShortcutChange::Changed {
                bindings,
                also,
                unchecked,
            },
            said,
        )
    }

    /// Whether this change is already in effect, so saving it changes nothing.
    ///
    /// Compared with what is IN EFFECT, not with a default: a setting the file
    /// does not hold is not the same as one set to its default value, and
    /// only the caller knows the default.
    ///
    /// A shortcut is never "already in effect" here. The settings hold only
    /// the shortcuts that READ, so a key the file holds and nobody can read -
    /// the very key a restore is meant to clear - is invisible from here. The
    /// store answers instead: it writes nothing when the key is already so.
    fn holds(&self, change: &SettingChange) -> bool {
        match change {
            SettingChange::Pack(pack) => self.settings.pack.as_deref() == Some(pack.as_str()),
            SettingChange::Compact(compact) => self.settings.compact == Some(*compact),
            SettingChange::Clearing(clearing) => self.settings.clearing == Some(*clearing),
            SettingChange::Shortcut { .. } => false,
        }
    }

    fn apply(&mut self, change: SettingChange) {
        match change {
            SettingChange::Pack(pack) => self.settings.pack = Some(pack),
            SettingChange::Compact(compact) => self.settings.compact = Some(compact),
            SettingChange::Clearing(clearing) => self.settings.clearing = Some(clearing),
            SettingChange::Shortcut { action, chord } => {
                wish(&mut self.settings.shortcuts, action, chord);
            }
        }
    }
}

/// One action's wish put into a list of the file's wishes the way the store
/// puts it into the file: in the place its key already stands, at the end
/// when it has none, and gone for a restore. One function for the change in
/// effect and for the change considered, so the two cannot drift apart.
fn wish(
    shortcuts: &mut Vec<(HotkeyAction, HotkeyChord)>,
    action: HotkeyAction,
    chord: Option<HotkeyChord>,
) {
    match chord {
        Some(chord) => match shortcuts.iter_mut().find(|(held, _)| *held == action) {
            Some(entry) => entry.1 = chord,
            None => shortcuts.push((action, chord)),
        },
        None => shortcuts.retain(|(held, _)| *held != action),
    }
}

/// What the system said about one chord registered on trial.
enum Trial {
    Free,
    Taken,
    Failed {
        code: u32,
    },
    /// Nothing could be registered at all, so nobody knows.
    Unchecked(ShortcutsUnavailable),
}

/// Registers `chord` alone for `action` and releases it before answering.
///
/// A set with no outcome for its one binding is a registrar that broke its
/// promise (one outcome per binding). It answers `Unchecked` rather than
/// `Free`, so the change goes through saying it was not checked instead of
/// claiming a check that never happened.
fn trial(registrar: &dyn HotkeyRegistrar, action: HotkeyAction, chord: HotkeyChord) -> Trial {
    let live = match registrar.register(&[(action, chord)]) {
        Ok(live) => live,
        Err(why) => return Trial::Unchecked(why),
    };
    let outcome = live.outcomes().first().map(|(_, outcome)| *outcome);
    // Released HERE, before anything else happens - the doc comment of
    // `change_shortcut` says why.
    drop(live);
    match outcome {
        Some(ShortcutRegistration::Registered) => Trial::Free,
        Some(ShortcutRegistration::Taken) => Trial::Taken,
        Some(ShortcutRegistration::Failed { code }) => Trial::Failed { code },
        None => Trial::Unchecked(ShortcutsUnavailable::CouldNotStart),
    }
}

/// What came of changing one shortcut - [`KeptSettings::change_shortcut`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShortcutChange {
    /// Nothing to change. A restore was still handed to the store (`D89`).
    AlreadySo,
    /// Refused before the system was asked - the same refusals the palette
    /// says about the settings file. Nothing saved.
    Refused(Refused),
    /// Another program holds the chord. Nothing saved.
    Taken { chord: HotkeyChord },
    /// The system refused the chord for another reason, with its raw code.
    /// Nothing saved.
    NotRegistered { chord: HotkeyChord, code: u32 },
    /// In effect now, and saved unless the second value of
    /// [`KeptSettings::change_shortcut`] says otherwise. `bindings` is the
    /// table the palette registers when the shortcuts window closes, `also`
    /// every other action whose chord moves with it. `unchecked` names why the
    /// system could not be asked whether the chord is free - `None` when it
    /// was asked, or when the action already answered to the chord.
    Changed {
        bindings: Bindings,
        also: Vec<HotkeyAction>,
        unchecked: Option<ShortcutsUnavailable>,
    },
}

/// What changing one shortcut would come to - [`KeptSettings::consider`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Considered {
    /// The action already answers to it and the file already says so -
    /// nothing to register and nothing to save.
    AlreadySo,
    /// The table the change gives, and every OTHER action whose shortcut
    /// moves with it.
    Gives {
        bindings: Bindings,
        also: Vec<HotkeyAction>,
    },
    /// Not possible, and why - the same refusals the palette says about the
    /// settings file.
    Refused(Refused),
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
                clearing: None,
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

    /// `D101`: the way of clearing is kept like any other choice - in effect at
    /// once, saved once, and a second click on the same choice writes nothing.
    #[test]
    fn the_way_of_clearing_is_kept_and_saved_once_per_change() {
        use crate::ports::Clearing;
        let store = FakeStore::loading(SettingsLoad::Absent);
        let (mut kept, _) = KeptSettings::open(&store);
        assert_eq!(kept.settings().clearing, None, "the file says nothing yet");
        assert_eq!(
            kept.keep(&store, SettingChange::Clearing(Clearing::Keep)),
            None
        );
        assert_eq!(kept.settings().clearing, Some(Clearing::Keep));
        assert_eq!(
            kept.keep(&store, SettingChange::Clearing(Clearing::Keep)),
            None
        );
        assert_eq!(
            kept.keep(&store, SettingChange::Clearing(Clearing::Line)),
            None
        );
        assert_eq!(
            store.saves(),
            vec![
                SettingChange::Clearing(Clearing::Keep),
                SettingChange::Clearing(Clearing::Line)
            ],
            "the same choice twice went to disk twice"
        );
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
                    (HotkeyAction::RepeatLast, chord("Ctrl+Alt+A")),
                ],
                ..Settings::default()
            },
            notes: Vec::new(),
        });
        let (kept, said) = KeptSettings::open(&store);
        assert!(said.is_empty(), "{said:?}");
        let defaults = Bindings::defaults(Convention::WindowsAndLinux);
        // The system's answer goes through untouched: `AltGr+A` types `U+0105`.
        let polish = |wanted: HotkeyChord| (wanted == chord("Ctrl+Alt+A")).then_some('\u{105}');
        let (bindings, messages) = kept.bindings(defaults, &polish);
        assert_eq!(
            bindings.chord(HotkeyAction::NextValue),
            chord("Alt+Shift+M")
        );
        assert_eq!(
            bindings.chord(HotkeyAction::PreviousValue),
            defaults.chord(HotkeyAction::PreviousValue),
            "Alt+Shift+R is Repeat last value's"
        );
        // Repeat last keeps Alt+Shift+R, so the clash names the true holder.
        assert_eq!(
            messages,
            vec![
                SettingsMessage::ShortcutNotUsed(Refused {
                    action: HotkeyAction::RepeatLast,
                    chord: chord("Ctrl+Alt+A"),
                    why: Refusal::TypesCharacter('\u{105}'),
                }),
                SettingsMessage::ShortcutNotUsed(Refused {
                    action: HotkeyAction::PreviousValue,
                    chord: chord("Alt+Shift+R"),
                    why: Refusal::SameAs(HotkeyAction::RepeatLast),
                }),
            ]
        );
        assert!(store.saves().is_empty(), "reading shortcuts writes nothing");
    }

    // ---- changing one shortcut (K5) ------------------------------------------

    fn chord(text: &str) -> HotkeyChord {
        HotkeyChord::parse(text).unwrap_or_else(|error| panic!("{text}: {error:?}"))
    }

    const DEFAULTS: Bindings = Bindings::defaults(nkb_core::hotkeys::Convention::WindowsAndLinux);

    fn no_layout(_: HotkeyChord) -> Option<char> {
        None
    }

    fn wishing(shortcuts: Vec<(HotkeyAction, HotkeyChord)>) -> KeptSettings {
        KeptSettings::open(&FakeStore::loading(SettingsLoad::Read {
            settings: Settings {
                shortcuts,
                ..Settings::default()
            },
            notes: Vec::new(),
        }))
        .0
    }

    #[test]
    fn a_free_chord_is_given_saved_and_then_already_so() {
        let store = FakeStore::loading(SettingsLoad::Absent);
        let (mut kept, _) = KeptSettings::open(&store);
        let wanted = chord("Alt+Shift+M");
        let Considered::Gives { bindings, also } =
            kept.consider(DEFAULTS, &no_layout, HotkeyAction::NextValue, Some(wanted))
        else {
            panic!("a free chord was not given");
        };
        assert_eq!(bindings.chord(HotkeyAction::NextValue), wanted);
        assert!(also.is_empty(), "{also:?}");

        let change = SettingChange::Shortcut {
            action: HotkeyAction::NextValue,
            chord: Some(wanted),
        };
        assert_eq!(kept.keep(&store, change.clone()), None);
        assert_eq!(store.saves(), vec![change]);
        assert_eq!(kept.bindings(DEFAULTS, &no_layout).0, bindings);
        assert_eq!(
            kept.consider(DEFAULTS, &no_layout, HotkeyAction::NextValue, Some(wanted)),
            Considered::AlreadySo
        );
    }

    #[test]
    fn a_chord_another_action_holds_is_refused_and_the_holder_named() {
        // Held by default, and held by a wish in the file - the recording is
        // wished last, so it never takes the chord, wherever its own key
        // stands in the file.
        let kept = wishing(vec![
            (HotkeyAction::NextValue, chord("Ctrl+Alt+Win+N")),
            (HotkeyAction::PreviousValue, chord("Alt+Shift+M")),
        ]);
        for (wanted, holder) in [
            ("Alt+Shift+R", HotkeyAction::RepeatLast),
            ("Alt+Shift+M", HotkeyAction::PreviousValue),
        ] {
            assert_eq!(
                kept.consider(
                    DEFAULTS,
                    &no_layout,
                    HotkeyAction::NextValue,
                    Some(chord(wanted))
                ),
                Considered::Refused(Refused {
                    action: HotkeyAction::NextValue,
                    chord: chord(wanted),
                    why: Refusal::SameAs(holder),
                }),
                "{wanted}"
            );
        }
    }

    #[test]
    fn a_chord_that_is_no_global_shortcut_is_refused_for_its_own_reason() {
        let kept = wishing(Vec::new());
        let polish = |wanted: HotkeyChord| (wanted == chord("Ctrl+Alt+A")).then_some('\u{105}');
        assert!(matches!(
            kept.consider(
                DEFAULTS,
                &polish,
                HotkeyAction::MarkOk,
                Some(chord("Shift+Q"))
            ),
            Considered::Refused(Refused {
                why: Refusal::Problem(_),
                ..
            })
        ));
        assert_eq!(
            kept.consider(
                DEFAULTS,
                &polish,
                HotkeyAction::MarkOk,
                Some(chord("Ctrl+Alt+A"))
            ),
            Considered::Refused(Refused {
                action: HotkeyAction::MarkOk,
                chord: chord("Ctrl+Alt+A"),
                why: Refusal::TypesCharacter('\u{105}'),
            })
        );
    }

    #[test]
    fn a_default_another_action_now_holds_is_not_taken_back_without_a_word() {
        // Next value moved away, so Previous value could take its default.
        let kept = wishing(vec![
            (HotkeyAction::NextValue, chord("Ctrl+Alt+Win+N")),
            (HotkeyAction::PreviousValue, chord("Alt+Shift+N")),
        ]);
        assert_eq!(
            kept.consider(DEFAULTS, &no_layout, HotkeyAction::NextValue, None),
            Considered::Refused(Refused {
                action: HotkeyAction::NextValue,
                chord: chord("Alt+Shift+N"),
                why: Refusal::SameAs(HotkeyAction::PreviousValue),
            })
        );
    }

    #[test]
    fn a_wish_in_the_file_that_a_change_lets_come_true_is_named() {
        // Previous value wants Next value's default - refused at start. Next
        // value records another chord, and the file's wish comes true.
        let kept = wishing(vec![(HotkeyAction::PreviousValue, chord("Alt+Shift+N"))]);
        let Considered::Gives { bindings, also } = kept.consider(
            DEFAULTS,
            &no_layout,
            HotkeyAction::NextValue,
            Some(chord("Alt+Shift+M")),
        ) else {
            panic!("the change was not given");
        };
        assert_eq!(also, vec![HotkeyAction::PreviousValue]);
        assert_eq!(
            bindings.chord(HotkeyAction::PreviousValue),
            chord("Alt+Shift+N")
        );
    }

    #[test]
    fn what_the_file_already_says_is_already_so_and_a_stale_wish_is_not() {
        let kept = wishing(Vec::new());
        assert_eq!(
            kept.consider(DEFAULTS, &no_layout, HotkeyAction::RepeatLast, None),
            Considered::AlreadySo
        );
        assert_eq!(
            kept.consider(
                DEFAULTS,
                &no_layout,
                HotkeyAction::RepeatLast,
                Some(chord("Alt+Shift+R"))
            ),
            Considered::AlreadySo,
            "recording the default the file does not name changes nothing"
        );
        // A refused wish stands in the file: recording the default replaces
        // it, so the next start stops saying it cannot be used.
        let stale = wishing(vec![(HotkeyAction::RepeatLast, chord("Alt+Shift+N"))]);
        assert!(matches!(
            stale.consider(
                DEFAULTS,
                &no_layout,
                HotkeyAction::RepeatLast,
                Some(chord("Alt+Shift+R"))
            ),
            Considered::Gives { .. }
        ));
    }

    #[test]
    fn whatever_the_file_wishes_a_change_shows_what_the_next_start_will_use() {
        // Three actions, six chords - their three defaults, two free ones and
        // one that is no global shortcut - over every file and every change.
        // What the window shows must be what the file it saves gives the next
        // start, and a change may move another action only onto its own wish.
        let actions = [
            HotkeyAction::NextValue,
            HotkeyAction::PreviousValue,
            HotkeyAction::RepeatLast,
        ];
        let pool = [
            chord("Alt+Shift+N"),
            chord("Alt+Shift+P"),
            chord("Alt+Shift+R"),
            chord("Ctrl+Alt+Win+X"),
            chord("Ctrl+Alt+Win+Y"),
            chord("Shift+Q"),
        ];
        let choices: Vec<Option<HotkeyChord>> = std::iter::once(None)
            .chain(pool.iter().copied().map(Some))
            .collect();
        let mut cases = 0;
        for a in &choices {
            for b in &choices {
                for c in &choices {
                    let file: Vec<(HotkeyAction, HotkeyChord)> = actions
                        .iter()
                        .zip([a, b, c])
                        .filter_map(|(action, wish)| wish.map(|wish| (*action, wish)))
                        .collect();
                    let kept = wishing(file.clone());
                    let now = kept.bindings(DEFAULTS, &no_layout).0;
                    for action in actions {
                        for request in &choices {
                            cases += 1;
                            let considered = kept.consider(DEFAULTS, &no_layout, action, *request);
                            let mut after = kept.clone();
                            after.apply(SettingChange::Shortcut {
                                action,
                                chord: *request,
                            });
                            let next_start = after.bindings(DEFAULTS, &no_layout).0;
                            let wish_of = |other: HotkeyAction| {
                                file.iter()
                                    .find_map(|(held, wish)| (*held == other).then_some(*wish))
                            };
                            match considered {
                                Considered::AlreadySo => {
                                    assert_eq!(next_start, now, "{file:?} {action:?} {request:?}")
                                }
                                Considered::Gives { bindings, also } => {
                                    assert_eq!(
                                        bindings, next_start,
                                        "{file:?} {action:?} {request:?}"
                                    );
                                    let wanted = request.unwrap_or_else(|| DEFAULTS.chord(action));
                                    assert_eq!(
                                        bindings.chord(action),
                                        wanted,
                                        "{file:?} {action:?} {request:?}"
                                    );
                                    for other in HotkeyAction::ALL {
                                        if other == action {
                                            continue;
                                        }
                                        let moved = bindings.chord(other) != now.chord(other);
                                        assert_eq!(also.contains(&other), moved);
                                        assert!(
                                            !moved || wish_of(other) == Some(bindings.chord(other)),
                                            "{other:?} moved off its own wish: {file:?} \
                                             {action:?} {request:?}"
                                        );
                                    }
                                }
                                Considered::Refused(refused) => {
                                    assert_eq!(refused.action, action);
                                    if let Refusal::SameAs(holder) = refused.why {
                                        assert_ne!(holder, action);
                                        assert_eq!(
                                            now.chord(holder),
                                            refused.chord,
                                            "{file:?} {action:?} {request:?} {refused:?}"
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(cases, 7 * 7 * 7 * 3 * 7);
    }

    // ---- changing one shortcut for real: the trial registration (K5.3) ------

    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use crate::ports::{LiveShortcuts, Wait};

    /// A registrar that answers what a test dictates, keeps every set it was
    /// asked to register and counts the handles still alive - the trial must
    /// leave none behind.
    struct FakeRegistrar {
        answer: Result<ShortcutRegistration, ShortcutsUnavailable>,
        asked: RefCell<Vec<Vec<(HotkeyAction, HotkeyChord)>>>,
        alive: Arc<AtomicUsize>,
    }

    impl FakeRegistrar {
        fn answering(answer: Result<ShortcutRegistration, ShortcutsUnavailable>) -> Self {
            Self {
                answer,
                asked: RefCell::new(Vec::new()),
                alive: Arc::new(AtomicUsize::new(0)),
            }
        }
    }

    struct FakeLive {
        outcomes: Vec<(HotkeyAction, ShortcutRegistration)>,
        alive: Arc<AtomicUsize>,
    }

    impl Drop for FakeLive {
        fn drop(&mut self) {
            self.alive.fetch_sub(1, Ordering::SeqCst);
        }
    }

    impl LiveShortcuts for FakeLive {
        fn outcomes(&self) -> &[(HotkeyAction, ShortcutRegistration)] {
            &self.outcomes
        }
        fn next(&self, _: Duration) -> Wait {
            Wait::Nothing
        }
    }

    impl HotkeyRegistrar for FakeRegistrar {
        fn register(
            &self,
            bindings: &[(HotkeyAction, HotkeyChord)],
        ) -> Result<Box<dyn LiveShortcuts + Send>, ShortcutsUnavailable> {
            self.asked.borrow_mut().push(bindings.to_vec());
            let outcome = self.answer.clone()?;
            self.alive.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(FakeLive {
                outcomes: bindings
                    .iter()
                    .map(|(action, _)| (*action, outcome))
                    .collect(),
                alive: Arc::clone(&self.alive),
            }))
        }
    }

    /// A store that notes how many trial handles were alive at each save.
    struct Watching<'a> {
        inner: FakeStore,
        alive: &'a AtomicUsize,
        at_save: RefCell<Vec<usize>>,
    }

    impl SettingsStore for Watching<'_> {
        fn load(&self) -> SettingsLoad {
            self.inner.load()
        }
        fn save(&self, change: &SettingChange) -> Result<(), SaveError> {
            self.at_save
                .borrow_mut()
                .push(self.alive.load(Ordering::SeqCst));
            self.inner.save(change)
        }
    }

    #[test]
    fn a_free_chord_is_tried_alone_released_and_only_then_saved() {
        let registrar = FakeRegistrar::answering(Ok(ShortcutRegistration::Registered));
        let store = Watching {
            inner: FakeStore::loading(SettingsLoad::Absent),
            alive: &registrar.alive,
            at_save: RefCell::new(Vec::new()),
        };
        let (mut kept, _) = KeptSettings::open(&store);
        let wanted = chord("Alt+Shift+M");

        let (change, said) = kept.change_shortcut(
            &store,
            &registrar,
            DEFAULTS,
            &no_layout,
            HotkeyAction::NextValue,
            Some(wanted),
        );

        let ShortcutChange::Changed {
            bindings,
            also,
            unchecked,
        } = change
        else {
            panic!("a free chord was not given: {change:?}");
        };
        assert_eq!(bindings.chord(HotkeyAction::NextValue), wanted);
        assert!(also.is_empty());
        assert_eq!(unchecked, None, "the system was asked");
        assert_eq!(said, None);
        assert_eq!(
            *registrar.asked.borrow(),
            vec![vec![(HotkeyAction::NextValue, wanted)]],
            "the chord is tried alone, for its own action"
        );
        assert_eq!(
            *store.at_save.borrow(),
            vec![0],
            "the trial still held the chord when the change was saved"
        );
        assert_eq!(registrar.alive.load(Ordering::SeqCst), 0);
        assert_eq!(
            store.inner.saves(),
            vec![SettingChange::Shortcut {
                action: HotkeyAction::NextValue,
                chord: Some(wanted),
            }]
        );
        assert_eq!(kept.bindings(DEFAULTS, &no_layout).0, bindings);
    }

    #[test]
    fn a_chord_the_system_will_not_give_is_refused_and_nothing_is_saved() {
        for (answer, expected) in [
            (
                ShortcutRegistration::Taken,
                ShortcutChange::Taken {
                    chord: chord("Alt+Shift+M"),
                },
            ),
            (
                ShortcutRegistration::Failed { code: 1400 },
                ShortcutChange::NotRegistered {
                    chord: chord("Alt+Shift+M"),
                    code: 1400,
                },
            ),
        ] {
            let registrar = FakeRegistrar::answering(Ok(answer));
            let store = FakeStore::loading(SettingsLoad::Absent);
            let (mut kept, _) = KeptSettings::open(&store);
            let before = kept.bindings(DEFAULTS, &no_layout).0;

            let (change, said) = kept.change_shortcut(
                &store,
                &registrar,
                DEFAULTS,
                &no_layout,
                HotkeyAction::NextValue,
                Some(chord("Alt+Shift+M")),
            );

            assert_eq!(change, expected);
            assert_eq!(said, None);
            assert!(store.saves().is_empty(), "{answer:?} was saved");
            assert_eq!(
                kept.bindings(DEFAULTS, &no_layout).0,
                before,
                "{answer:?} changed the table in effect"
            );
            assert_eq!(registrar.alive.load(Ordering::SeqCst), 0);
        }
    }

    #[test]
    fn a_refusal_never_reaches_the_system() {
        let registrar = FakeRegistrar::answering(Ok(ShortcutRegistration::Registered));
        let store = FakeStore::loading(SettingsLoad::Absent);
        let (mut kept, _) = KeptSettings::open(&store);

        let (change, _) = kept.change_shortcut(
            &store,
            &registrar,
            DEFAULTS,
            &no_layout,
            HotkeyAction::NextValue,
            Some(chord("Alt+Shift+R")),
        );

        assert_eq!(
            change,
            ShortcutChange::Refused(Refused {
                action: HotkeyAction::NextValue,
                chord: chord("Alt+Shift+R"),
                why: Refusal::SameAs(HotkeyAction::RepeatLast),
            })
        );
        assert!(registrar.asked.borrow().is_empty());
        assert!(store.saves().is_empty());
    }

    #[test]
    fn a_restore_that_changes_nothing_is_still_saved_and_a_recording_is_not() {
        // `D89`: the key a restore is meant to clear may not have read as a
        // shortcut, so the settings cannot see it - the store removes it.
        let registrar = FakeRegistrar::answering(Ok(ShortcutRegistration::Registered));
        let store = FakeStore::loading(SettingsLoad::Absent);
        let (mut kept, _) = KeptSettings::open(&store);

        let restored = kept.change_shortcut(
            &store,
            &registrar,
            DEFAULTS,
            &no_layout,
            HotkeyAction::RepeatLast,
            None,
        );
        assert_eq!(restored, (ShortcutChange::AlreadySo, None));
        assert_eq!(
            store.saves(),
            vec![SettingChange::Shortcut {
                action: HotkeyAction::RepeatLast,
                chord: None,
            }]
        );

        let recorded = kept.change_shortcut(
            &store,
            &registrar,
            DEFAULTS,
            &no_layout,
            HotkeyAction::RepeatLast,
            Some(chord("Alt+Shift+R")),
        );
        assert_eq!(recorded, (ShortcutChange::AlreadySo, None));
        assert_eq!(store.saves().len(), 1, "recording what is so was saved");
        assert!(
            registrar.asked.borrow().is_empty(),
            "nothing to change, nothing to ask"
        );
    }

    #[test]
    fn a_chord_the_action_already_answers_to_is_not_tried() {
        // Repeat last wants Next value's default and is refused at start, so it
        // answers to its own default. Recording that default replaces the stale
        // wish - a change - but the chord is the one it answers to: while held,
        // our own registration would call it Taken.
        let registrar = FakeRegistrar::answering(Ok(ShortcutRegistration::Taken));
        let store = FakeStore::loading(SettingsLoad::Absent);
        let mut kept = wishing(vec![(HotkeyAction::RepeatLast, chord("Alt+Shift+N"))]);

        let (change, _) = kept.change_shortcut(
            &store,
            &registrar,
            DEFAULTS,
            &no_layout,
            HotkeyAction::RepeatLast,
            Some(chord("Alt+Shift+R")),
        );

        assert!(
            matches!(
                change,
                ShortcutChange::Changed {
                    unchecked: None,
                    ..
                }
            ),
            "{change:?}"
        );
        assert!(registrar.asked.borrow().is_empty());
        assert_eq!(store.saves().len(), 1);
    }

    #[test]
    fn with_no_way_to_ask_the_change_goes_through_and_says_it_was_not_checked() {
        let why = ShortcutsUnavailable::Unsupported {
            system: String::from("macOS"),
        };
        let registrar = FakeRegistrar::answering(Err(why.clone()));
        let store = FakeStore::loading(SettingsLoad::Absent).failing_with(SaveError::Unwritable);
        let (mut kept, _) = KeptSettings::open(&store);

        let (change, said) = kept.change_shortcut(
            &store,
            &registrar,
            DEFAULTS,
            &no_layout,
            HotkeyAction::NextValue,
            Some(chord("Alt+Shift+M")),
        );

        assert!(
            matches!(&change, ShortcutChange::Changed { unchecked: Some(named), .. } if *named == why),
            "{change:?}"
        );
        assert_eq!(
            said,
            Some(SettingsMessage::NotSaved(SaveError::Unwritable)),
            "a save that failed is said, beside the change in effect"
        );
        assert_eq!(
            kept.bindings(DEFAULTS, &no_layout)
                .0
                .chord(HotkeyAction::NextValue),
            chord("Alt+Shift+M")
        );
    }
}
