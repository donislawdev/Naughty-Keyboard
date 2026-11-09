//! The pack folders as the palette meets them (`D173`): every choice of a
//! pack reads them afresh, and the palette says what came of it - a pack used
//! instead of another of the same name each time it is opened, and a folder
//! that could not be read once, until what is wrong with it changes.
//!
//! Out of `live.rs`, which stands at the ceiling of `D113`.

use std::cell::RefCell;

use nkb_adapters::{FolderCatalogue, PackFolders, TomlPackFormat, i18n};
use nkb_app::PackCatalogue;
use nkb_app::advance_sequence::{AdvanceSequence, ChooseError};
use nkb_app::ports::SettingChange;

use super::Memory;

/// The folders of this run and what the palette last said about them.
pub(super) struct Folders {
    folders: PackFolders,
    /// The faults said last. A team folder on a share that stopped answering
    /// would otherwise say so at every change of pack - a sentence said every
    /// time teaches the tester to skip it.
    said: RefCell<Vec<String>>,
}

impl Folders {
    pub(super) fn new(folders: PackFolders) -> Self {
        Self {
            folders,
            said: RefCell::new(Vec::new()),
        }
    }

    /// A fresh look at the three sources.
    pub(super) fn catalogue(&self) -> FolderCatalogue {
        self.folders.catalogue()
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
        i18n::choose_error(error, pack, &self.folders)
    }

    /// The faults of the folders in `catalogue`, when they are not the ones
    /// said last.
    fn news(&self, catalogue: &FolderCatalogue) -> Vec<String> {
        let faults: Vec<String> = catalogue
            .coverage()
            .skipped
            .into_iter()
            .filter_map(|(source, why)| i18n::folder_not_read(source, why, &self.folders))
            .collect();
        let mut said = self.said.borrow_mut();
        if *said == faults {
            return Vec::new();
        }
        said.clone_from(&faults);
        faults
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
    if sequence.pack_id() == Some(asked) {
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
    use nkb_adapters::SystemClock;

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
