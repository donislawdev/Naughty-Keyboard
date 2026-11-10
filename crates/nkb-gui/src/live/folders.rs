//! The pack folders as the palette meets them (`D173`): every choice of a
//! pack reads them afresh, and the palette says what came of it - a pack used
//! instead of another of the same name each time it is opened, and a folder
//! that could not be read once, until what is wrong with it changes.
//!
//! Out of `live.rs`, which stands at the ceiling of `D113`.

use std::cell::RefCell;
use std::path::{Path, PathBuf};

use nkb_adapters::{FolderCatalogue, PackFolders, TomlPackFormat, i18n, take_team_folder};
use nkb_app::PackCatalogue;
use nkb_app::advance_sequence::{AdvanceSequence, ChooseError};
use nkb_app::ports::SettingChange;

use super::{Carried, Memory, Told, Worker};

/// The folders of this run and what the palette last said about them.
pub(super) struct Folders {
    /// In a cell, because the pack folders window changes the team folder while
    /// the palette runs (`D175`).
    folders: RefCell<PackFolders>,
    /// The faults said last. A team folder on a share that stopped answering
    /// would otherwise say so at every change of pack - a sentence said every
    /// time teaches the tester to skip it.
    said: RefCell<Vec<String>>,
}

impl Folders {
    pub(super) fn new(folders: PackFolders) -> Self {
        Self {
            folders: RefCell::new(folders),
            said: RefCell::new(Vec::new()),
        }
    }

    /// A fresh look at the three sources.
    pub(super) fn catalogue(&self) -> FolderCatalogue {
        self.folders.borrow().catalogue()
    }

    /// The folders as they are now.
    fn now(&self) -> PackFolders {
        self.folders.borrow().clone()
    }

    /// Puts other folders in place - a team folder chosen or stopped. What was
    /// said about the old ones is forgotten, so a fault of the new team folder
    /// is said at the next choice of a pack even when it reads like the last.
    fn replace(&self, folders: PackFolders) {
        *self.folders.borrow_mut() = folders;
        self.said.borrow_mut().clear();
    }

    /// Opens `pack` from a fresh look at the folders. What the palette says
    /// comes back beside the answer: the faults of the folders not said
    /// before, and - when it opened - that the pack is used instead of
    /// another. Why it did not open is [`Self::why_not`], for the caller who
    /// knows whether it is said.
    pub(super) fn choose(
        &self,
        sequence: &mut AdvanceSequence,
        pack: &str,
    ) -> (Result<(), ChooseError>, Vec<String>) {
        let catalogue = self.catalogue();
        let chosen = sequence.choose_pack(&catalogue, &TomlPackFormat, pack);
        let mut said = self.news(&catalogue);
        if chosen.is_ok() {
            said.extend(
                sequence
                    .pack_origin()
                    .and_then(|origin| i18n::pack_overrides(pack, origin)),
            );
        }
        (chosen, said)
    }

    /// Why `pack` could not be chosen, naming its file when it is in a folder.
    pub(super) fn why_not(&self, error: &ChooseError, pack: &str) -> String {
        i18n::choose_error(error, pack, &self.folders.borrow())
    }

    /// The faults of the folders in `catalogue`, when they are not the ones
    /// said last.
    fn news(&self, catalogue: &FolderCatalogue) -> Vec<String> {
        let faults: Vec<String> = catalogue
            .coverage()
            .skipped
            .into_iter()
            .filter_map(|(source, why)| i18n::folder_not_read(source, why, &self.folders.borrow()))
            .collect();
        let mut said = self.said.borrow_mut();
        if *said == faults {
            return Vec::new();
        }
        said.clone_from(&faults);
        faults
    }
}

/// What came of one choice in the pack folders window (`D175`): the team folder
/// the run uses now - the one before, when the choice was refused - and the line
/// the window says. A path rather than the folders themselves, which the main
/// thread builds from it: the folders hold what a folder that does not answer
/// is still asked, and each thread keeps its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeamFolderNews {
    pub team: Option<PathBuf>,
    pub said: String,
}

impl Worker<'_> {
    /// Makes `chosen` the team folder, or stops using one: looked at first, in
    /// effect at once, saved when it can be - and said in the window every time.
    pub(super) fn team_folder(&mut self, chosen: Option<PathBuf>) -> Carried {
        let now = self.memory.folders.now();
        let (kept, folders) = match &chosen {
            Some(path) => match take_team_folder(&now, path) {
                Ok((text, taken)) => (Some(text), taken),
                Err(why) => {
                    let said = i18n::team_folder_refused(why, path);
                    return told(now.team().ok().map(Path::to_path_buf), said);
                }
            },
            None => (None, now.with_team(None)),
        };
        let saved = self
            .memory
            .kept
            .borrow_mut()
            .change_team_folder(self.memory.store, kept);
        let team = folders.team().ok().map(Path::to_path_buf);
        self.memory.folders.replace(folders);
        let file = (!self.memory.file.is_empty()).then(|| Path::new(&self.memory.file));
        told(team, i18n::team_folder_kept(chosen.is_none(), &saved, file))
    }
}

/// The answer to the pack folders window, and nothing for the palette.
fn told(team: Option<PathBuf>, said: String) -> Carried {
    Carried {
        view: None,
        told: Some(Told::TeamFolder(TeamFolderNews { team, said })),
        toggled: None,
    }
}

/// What opening a pack came to.
pub(super) enum PackOpening {
    /// It is the pack in use - the window works from a snapshot, and choosing
    /// it again must not send `7 / 34` back to the start.
    Same,
    /// Opened, with what saving it as the remembered pack had to say.
    New(Vec<String>),
    /// It could not be opened, and nothing changed - `choose_pack` leaves the
    /// pack in use, its place and the last value as they were. The last line
    /// says why, naming the pack that was asked for.
    Failed(Vec<String>),
}

/// Opens `asked` in place of the pack in use. One path for a pack row and a
/// value row of the window. `pack` follows the pack that opened.
pub(super) fn open_pack(
    sequence: &mut AdvanceSequence,
    memory: &Memory,
    pack: &mut String,
    asked: &str,
) -> PackOpening {
    // A set made for a limit is no pack, whatever it is called (`D178`): a
    // pack of that name opens in its place.
    if sequence.pack_id() == Some(asked) && sequence.made_for().is_none() {
        return PackOpening::Same;
    }
    let (chosen, mut said) = memory.folders.choose(sequence, asked);
    match chosen {
        Ok(()) => {
            asked.clone_into(pack);
            said.extend(memory.keep(SettingChange::Pack(asked.to_owned())));
            PackOpening::New(said)
        }
        Err(error) => {
            said.push(memory.folders.why_not(&error, asked));
            PackOpening::Failed(said)
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
    use super::*;
    use crate::live::Command;
    use crate::live::tests::{Registrar, a_store, with_worker};
    use nkb_adapters::SystemClock;

    /// What the window was told, from what the worker carried out.
    fn news(carried: Carried) -> TeamFolderNews {
        assert!(carried.view.is_none(), "the palette was redrawn");
        match carried.told {
            Some(Told::TeamFolder(news)) => news,
            other => panic!("the window was told {other:?}"),
        }
    }

    /// `D175`: a folder is looked at before anything is kept, kept and saved
    /// when it is one, and stopped - each told to the window, and the folders
    /// the palette reads changed with it.
    #[test]
    fn a_team_folder_is_looked_at_kept_saved_and_stopped() {
        let root = std::env::temp_dir()
            .join("nkb-live-folders-tests")
            .join(format!("team-{}", std::process::id()));
        let team = root.join("team packs");
        std::fs::create_dir_all(&team).expect("a team folder");
        let file = root.join("a-pack.toml");
        std::fs::write(&file, "").expect("a file");
        let store = a_store(false);
        with_worker(&store, &Registrar::default(), false, |worker, memory| {
            let refused = news(worker.carry_out(Command::TeamFolder(Some(file.clone()))));
            assert_eq!(
                refused.team, None,
                "a refused folder changed the team folder"
            );
            assert!(
                refused.said.starts_with("This is not a folder"),
                "{}",
                refused.said
            );
            assert!(
                store.saved.borrow().is_empty(),
                "a refused folder was saved"
            );

            let kept = news(worker.carry_out(Command::TeamFolder(Some(team.clone()))));
            assert_eq!(kept.team, Some(team.clone()));
            assert!(
                kept.said.starts_with("The team folder is saved"),
                "{}",
                kept.said
            );
            assert_eq!(memory.folders.now().team(), Ok(team.as_path()));
            let text = team.to_str().expect("a scratch path is text").to_owned();
            assert_eq!(
                *store.saved.borrow(),
                vec![SettingChange::TeamFolder(Some(text))]
            );

            let stopped = news(worker.carry_out(Command::TeamFolder(None)));
            assert_eq!(stopped.team, None);
            assert!(stopped.said.contains("no longer used"), "{}", stopped.said);
            assert!(memory.folders.now().team().is_err());
        });
        let failing = a_store(true);
        with_worker(&failing, &Registrar::default(), false, |worker, memory| {
            let kept = news(worker.carry_out(Command::TeamFolder(Some(team.clone()))));
            assert!(
                kept.said
                    .starts_with("The team folder is used until the palette closes"),
                "{}",
                kept.said
            );
            assert_eq!(
                memory.folders.now().team(),
                Ok(team.as_path()),
                "not used for the run"
            );
        });
        let _ = std::fs::remove_dir_all(&root);
    }

    fn missing_team() -> Folders {
        let nowhere = std::env::temp_dir()
            .join("nkb-live-folders-tests")
            .join(format!("missing-{}", std::process::id()));
        Folders::new(PackFolders::new(
            Ok(nowhere.join("team")),
            Ok(nowhere.join("own")),
        ))
    }

    #[test]
    fn a_folder_fault_is_said_once_and_not_at_every_pack() {
        let folders = missing_team();
        let mut sequence = AdvanceSequence::new(Box::new(SystemClock));
        let (chosen, said) = folders.choose(&mut sequence, "whitespace");
        assert_eq!(chosen, Ok(()));
        assert_eq!(said.len(), 1, "{said:?}");
        assert!(said[0].starts_with("The team pack folder "), "{said:?}");
        let (chosen, said) = folders.choose(&mut sequence, "unicode-text");
        assert_eq!(chosen, Ok(()));
        assert!(said.is_empty(), "said again: {said:?}");
    }

    #[test]
    fn a_pack_that_is_not_there_is_told_apart_from_a_folder_fault() {
        let folders = missing_team();
        let mut sequence = AdvanceSequence::new(Box::new(SystemClock));
        let (chosen, _) = folders.choose(&mut sequence, "no-such-pack");
        let error = chosen.expect_err("there is no such pack");
        assert_eq!(
            folders.why_not(&error, "no-such-pack"),
            "Pack \"no-such-pack\" was not found. Run: nkb packs"
        );
    }
}
