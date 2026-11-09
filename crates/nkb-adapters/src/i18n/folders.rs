//! The words of the pack folders window (`ux-spec.md` 6 M, `D175`): what the
//! window is, what each folder holds, and what came of each thing the tester
//! did there.
//!
//! # A count stands after its label
//!
//! `packs: 3`, never "3 packs": the window has no mechanism for plurals, and a
//! language with three forms would need one (`D80`).
//!
//! # A path is somebody else's text
//!
//! A folder's path comes from the system's folder window, a drop or the
//! settings file, and it goes into the window through `escaped_prose`, like the
//! names of the files in it (`catalogue.rs` here says why).

use std::path::Path;

use nkb_app::TeamFolderSaved;
use nkb_app::ports::{CatalogueSource, SaveError, SourceSkipped};
use nkb_core::text::escaped_prose;
use nkb_sys::folders::{PickError, ShowError};

use super::fill::fill;
use crate::catalogue::FolderHolds;
use crate::pack_folder::{FOLDER_FILES_LIMIT, FOLDER_WAIT};
use crate::team_folder::TeamFolderRefusal;

/// The words of the window that carry nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoldersLabel {
    /// The window's title, which the system shows in the task switcher.
    Title,
    /// The heading in the window's header.
    Heading,
    /// Where packs come from, and which wins.
    Summary,
    TeamHeading,
    OwnHeading,
    /// The team folder's path line when none is set.
    NoTeamFolder,
    /// That a folder dropped on the window becomes the team folder.
    DropHint,
    /// The team folder's state line while a folder is dragged over the window.
    DropHere,
    Choose,
    ChooseAction,
    /// The title of the system's folder window.
    PickerTitle,
    /// The status line while the system's folder window is open.
    Picking,
    Open,
    OpenTeamAction,
    OpenOwnAction,
    StopUsing,
    StopUsingAction,
    /// The own folder's button while the folder is not there yet.
    CreateAndOpen,
    Close,
    CloseAction,
    /// The link in the value window's header that opens this window.
    Link,
    LinkAction,
    /// More than one thing was dropped, and only the first was taken.
    OnlyFirstDropped,
}

fn pattern_folders_label(label: FoldersLabel) -> &'static str {
    match label {
        FoldersLabel::Title => "Naughty Keyboard - pack folders",
        FoldersLabel::Heading => "Pack folders",
        FoldersLabel::Summary => {
            "Packs come from the program, the team folder and your own folder. A pack in your own folder is used instead of one with the same name in the team folder, and a team pack instead of the program's."
        }
        FoldersLabel::TeamHeading => "Team folder",
        FoldersLabel::OwnHeading => "Your own folder",
        FoldersLabel::NoTeamFolder => "No team folder is set.",
        FoldersLabel::DropHint => {
            "You can also drop a folder on this window to use it as the team folder."
        }
        FoldersLabel::DropHere => "Drop it here to use it as the team folder.",
        FoldersLabel::Choose => "Choose folder",
        FoldersLabel::ChooseAction => "Choose the team folder",
        FoldersLabel::PickerTitle => "Choose the team folder",
        FoldersLabel::Picking => "Choose the folder in the window the system opened.",
        FoldersLabel::Open => "Open",
        FoldersLabel::OpenTeamAction => "Open the team folder",
        FoldersLabel::OpenOwnAction => "Open your own folder",
        FoldersLabel::StopUsing => "Stop using",
        FoldersLabel::StopUsingAction => "Stop using the team folder",
        FoldersLabel::CreateAndOpen => "Create and open",
        FoldersLabel::Close => "Close",
        FoldersLabel::CloseAction => "Close the pack folders window",
        FoldersLabel::Link => "Pack folders",
        FoldersLabel::LinkAction => "Open the pack folders window",
        FoldersLabel::OnlyFirstDropped => {
            "More than one thing was dropped. Only the first is used."
        }
    }
}

/// A label of the window, ready to show.
#[must_use]
pub fn folders_label(label: FoldersLabel) -> &'static str {
    pattern_folders_label(label)
}

/// What a folder holds, and what a folder that was not read says instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FolderState {
    Packs,
    Instead,
    Misnamed,
    TeamNotSet,
    TeamSettingUnusable,
    TeamMissing,
    OwnMissing,
    OwnNoPlace,
    NotAFolder,
    Unreadable,
    Unreachable,
    TooManyFiles,
    SameAsOwn,
}

fn pattern_folder_state(state: FolderState) -> &'static str {
    match state {
        FolderState::Packs => "packs: {count}",
        FolderState::Instead => "used instead of another pack: {count}",
        FolderState::Misnamed => "files whose name is not a pack name: {count}",
        FolderState::TeamNotSet => "Choose the folder your team keeps its packs in.",
        FolderState::TeamSettingUnusable => {
            "The team-folder setting in your settings file could not be used. Choose the folder again."
        }
        FolderState::TeamMissing => "Not found, so its packs are not offered.",
        FolderState::OwnMissing => "Not created yet. It is created when you open it.",
        FolderState::OwnNoPlace => {
            "This computer gives the program no place for settings, so there is no own folder."
        }
        FolderState::NotAFolder => "This is a file, not a folder.",
        FolderState::Unreadable => "Could not be read, so its packs are not offered.",
        FolderState::Unreachable => {
            "Did not answer within {seconds} seconds, so its packs are not offered."
        }
        FolderState::TooManyFiles => {
            "Holds more than {limit} pack files, so it is not read. Is it the right folder?"
        }
        FolderState::SameAsOwn => {
            "This is your own folder, so its packs are read once, as your own."
        }
    }
}

/// The state line of the folder of `source`, and whether it is a fault the
/// tester has to act on.
#[must_use]
pub fn folder_state(source: CatalogueSource, holds: FolderHolds) -> (String, bool) {
    let why = match holds {
        FolderHolds::Packs {
            count,
            instead,
            misnamed,
        } => {
            let mut parts = vec![counted(FolderState::Packs, count)];
            if instead > 0 {
                parts.push(counted(FolderState::Instead, instead));
            }
            if misnamed > 0 {
                parts.push(counted(FolderState::Misnamed, misnamed));
            }
            return (parts.join(" \u{b7} "), false);
        }
        FolderHolds::Skipped(why) => why,
    };
    let (state, fault) = match (source, why) {
        (CatalogueSource::Own, SourceSkipped::Missing) => (FolderState::OwnMissing, false),
        (_, SourceSkipped::NotSet) => (FolderState::TeamNotSet, false),
        (_, SourceSkipped::SameAsOwn) => (FolderState::SameAsOwn, false),
        (_, SourceSkipped::SettingUnusable) => (FolderState::TeamSettingUnusable, true),
        (_, SourceSkipped::NoPlace) => (FolderState::OwnNoPlace, true),
        (_, SourceSkipped::Missing) => (FolderState::TeamMissing, true),
        (_, SourceSkipped::NotAFolder) => (FolderState::NotAFolder, true),
        (_, SourceSkipped::Unreadable) => (FolderState::Unreadable, true),
        (_, SourceSkipped::Unreachable) => (FolderState::Unreachable, true),
        (_, SourceSkipped::TooManyFiles) => (FolderState::TooManyFiles, true),
    };
    let text = fill(
        pattern_folder_state(state),
        &[
            ("seconds", &FOLDER_WAIT.as_secs().to_string()),
            ("limit", &FOLDER_FILES_LIMIT.to_string()),
        ],
    );
    (text, fault)
}

fn counted(state: FolderState, count: usize) -> String {
    fill(
        pattern_folder_state(state),
        &[("count", &count.to_string())],
    )
}

/// A path for the window: lossy where the system's name is not Unicode, and
/// escaped as somebody else's text.
#[must_use]
pub fn shown_folder(path: &Path) -> String {
    escaped_prose(&path.to_string_lossy())
}

/// What came of something the tester did in the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FolderNews {
    TeamSaved,
    TeamStopped,
    TeamUsedNotSaved,
    TeamStoppedNotSaved,
    NotSavedAtStart,
    NotSavedUnreadable,
    NotSavedUnwritable,
    RefusedNotText,
    RefusedNotFullPath,
    RefusedNotAFolder,
    RefusedOwnFolder,
    NoPicker,
    NoPickerNoFile,
    PickerFailed,
    FolderGone,
    NoFileManager,
    FileManagerFailed,
    OwnNotMade,
}

fn pattern_folder_news(news: FolderNews) -> &'static str {
    match news {
        FolderNews::TeamSaved => {
            "The team folder is saved. The palette and the value window offer its packs now."
        }
        FolderNews::TeamStopped => {
            "The team folder is no longer used. Its packs are not offered any more."
        }
        FolderNews::TeamUsedNotSaved => {
            "The team folder is used until the palette closes, and it was not saved: {reason}"
        }
        FolderNews::TeamStoppedNotSaved => {
            "The team folder is not used until the palette closes, and that was not saved: {reason}"
        }
        FolderNews::NotSavedAtStart => "the settings could not be used when the palette started.",
        FolderNews::NotSavedUnreadable => "the settings file {file} can no longer be read.",
        FolderNews::NotSavedUnwritable => "the settings file {file} could not be written to.",
        FolderNews::RefusedNotText => {
            "This folder's name cannot be written in the settings file, so it cannot be the team folder: {path}"
        }
        FolderNews::RefusedNotFullPath => "The team folder must be a full path: {path}",
        FolderNews::RefusedNotAFolder => {
            "This is not a folder, so it cannot be the team folder: {path}"
        }
        FolderNews::RefusedOwnFolder => {
            "This is your own folder. Its packs are read already, as your own."
        }
        FolderNews::NoPicker => {
            "This desktop has no window for choosing a folder. Drop the folder on this window, or set team-folder in {file}."
        }
        FolderNews::NoPickerNoFile => {
            "This desktop has no window for choosing a folder. Drop the folder on this window."
        }
        FolderNews::PickerFailed => "The window for choosing a folder did not open: {reason}.",
        FolderNews::FolderGone => "The folder is not there any more: {path}",
        FolderNews::NoFileManager => {
            "This desktop has no file manager to open the folder in. The folder is {path}"
        }
        FolderNews::FileManagerFailed => "The folder did not open: {reason}. The folder is {path}",
        FolderNews::OwnNotMade => {
            "Your own folder could not be created: {reason}. It goes here: {path}"
        }
    }
}

/// What came of choosing a team folder, or of stopping it - `stopped` - for the
/// window's status line. `file` is the settings file, named when a save failed.
#[must_use]
pub fn team_folder_kept(stopped: bool, saved: &TeamFolderSaved, file: Option<&Path>) -> String {
    let lead = match (stopped, saved) {
        (false, TeamFolderSaved::Saved) => {
            return pattern_folder_news(FolderNews::TeamSaved).to_owned();
        }
        (true, TeamFolderSaved::Saved) => {
            return pattern_folder_news(FolderNews::TeamStopped).to_owned();
        }
        (false, TeamFolderSaved::NotSaved(_)) => FolderNews::TeamUsedNotSaved,
        (true, TeamFolderSaved::NotSaved(_)) => FolderNews::TeamStoppedNotSaved,
    };
    let reason = match saved {
        TeamFolderSaved::NotSaved(Some(SaveError::Unusable(_))) => FolderNews::NotSavedUnreadable,
        TeamFolderSaved::NotSaved(Some(SaveError::Unwritable)) => FolderNews::NotSavedUnwritable,
        TeamFolderSaved::NotSaved(Some(SaveError::Nowhere) | None) | TeamFolderSaved::Saved => {
            FolderNews::NotSavedAtStart
        }
    };
    let file = file.map(shown_folder).unwrap_or_default();
    let reason = fill(pattern_folder_news(reason), &[("file", &file)]);
    fill(pattern_folder_news(lead), &[("reason", &reason)])
}

/// Why `chosen` did not become the team folder.
#[must_use]
pub fn team_folder_refused(why: TeamFolderRefusal, chosen: &Path) -> String {
    let news = match why {
        TeamFolderRefusal::NotText => FolderNews::RefusedNotText,
        TeamFolderRefusal::NotFullPath => FolderNews::RefusedNotFullPath,
        TeamFolderRefusal::NotAFolder => FolderNews::RefusedNotAFolder,
        TeamFolderRefusal::OwnFolder => FolderNews::RefusedOwnFolder,
    };
    fill(
        pattern_folder_news(news),
        &[("path", &shown_folder(chosen))],
    )
}

/// Why the system's folder window gave no folder - and, where the desktop has
/// none, the two other ways to set it.
#[must_use]
pub fn folder_window_failed(error: &PickError, file: Option<&Path>) -> String {
    match (error, file) {
        (PickError::NoWindow, Some(file)) => fill(
            pattern_folder_news(FolderNews::NoPicker),
            &[("file", &shown_folder(file))],
        ),
        (PickError::NoWindow, None) => pattern_folder_news(FolderNews::NoPickerNoFile).to_owned(),
        (PickError::Failed(why), _) => fill(
            pattern_folder_news(FolderNews::PickerFailed),
            &[("reason", why)],
        ),
    }
}

/// Why `folder` did not open in the file manager - with the folder named, so
/// the tester can go there another way.
#[must_use]
pub fn file_manager_failed(error: &ShowError, folder: &Path) -> String {
    let path = shown_folder(folder);
    match error {
        ShowError::NotAFolder => fill(
            pattern_folder_news(FolderNews::FolderGone),
            &[("path", &path)],
        ),
        ShowError::NoFileManager => fill(
            pattern_folder_news(FolderNews::NoFileManager),
            &[("path", &path)],
        ),
        ShowError::Failed(why) => fill(
            pattern_folder_news(FolderNews::FileManagerFailed),
            &[("reason", why), ("path", &path)],
        ),
    }
}

/// Why the own folder could not be made, and where it goes.
#[must_use]
pub fn own_folder_not_made(error: &std::io::Error, folder: &Path) -> String {
    fill(
        pattern_folder_news(FolderNews::OwnNotMade),
        &[
            ("reason", &error.to_string()),
            ("path", &shown_folder(folder)),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_that_was_read_says_its_counts_after_their_labels() {
        let holds = FolderHolds::Packs {
            count: 4,
            instead: 1,
            misnamed: 2,
        };
        assert_eq!(
            folder_state(CatalogueSource::Team, holds),
            (
                String::from(
                    "packs: 4 \u{b7} used instead of another pack: 1 \u{b7} files whose name is not a pack name: 2"
                ),
                false
            )
        );
        let plain = FolderHolds::Packs {
            count: 0,
            instead: 0,
            misnamed: 0,
        };
        assert_eq!(
            folder_state(CatalogueSource::Own, plain),
            (String::from("packs: 0"), false)
        );
    }

    #[test]
    fn a_folder_not_made_yet_is_no_fault_and_one_that_does_not_answer_is() {
        let own = folder_state(
            CatalogueSource::Own,
            FolderHolds::Skipped(SourceSkipped::Missing),
        );
        assert_eq!(
            own,
            (
                String::from("Not created yet. It is created when you open it."),
                false
            )
        );
        let (text, fault) = folder_state(
            CatalogueSource::Team,
            FolderHolds::Skipped(SourceSkipped::Unreachable),
        );
        assert!(fault);
        assert!(!text.contains('{'), "{text}");
        assert!(text.contains(&FOLDER_WAIT.as_secs().to_string()), "{text}");
    }

    #[test]
    fn every_sentence_fills_all_its_places() {
        let path = Path::new("/srv/packs");
        let sentences = [
            team_folder_kept(false, &TeamFolderSaved::Saved, None),
            team_folder_kept(true, &TeamFolderSaved::NotSaved(None), None),
            team_folder_kept(
                false,
                &TeamFolderSaved::NotSaved(Some(SaveError::Unwritable)),
                Some(path),
            ),
            team_folder_refused(TeamFolderRefusal::NotAFolder, path),
            folder_window_failed(&PickError::NoWindow, Some(path)),
            folder_window_failed(&PickError::NoWindow, None),
            folder_window_failed(&PickError::Failed(String::from("no thread")), None),
            file_manager_failed(&ShowError::NoFileManager, path),
            file_manager_failed(&ShowError::Failed(String::from("refused")), path),
            own_folder_not_made(
                &std::io::Error::from(std::io::ErrorKind::PermissionDenied),
                path,
            ),
        ];
        for sentence in sentences {
            assert!(!sentence.contains(['{', '}']), "{sentence}");
        }
        assert_eq!(
            team_folder_kept(
                false,
                &TeamFolderSaved::NotSaved(Some(SaveError::Unwritable)),
                Some(path)
            ),
            "The team folder is used until the palette closes, and it was not saved: the settings file /srv/packs could not be written to."
        );
    }

    #[test]
    fn a_path_from_a_disk_is_escaped_before_it_is_said() {
        let hostile = Path::new("/srv/team\u{202e}txt.exe\npacks");
        let said = team_folder_refused(TeamFolderRefusal::NotAFolder, hostile);
        assert!(
            !said.contains('\u{202e}') && !said.contains('\n'),
            "{said:?}"
        );
    }
}
