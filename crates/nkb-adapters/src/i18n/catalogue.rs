//! What the palette and the value window say about where packs come from: a
//! pack that could not be chosen, a pack used instead of another of the same
//! name, a folder that was not read (`ux-spec.md` 6 D and K, `D173`).
//!
//! # Everything from a disk is shown as somebody else's text
//!
//! A folder's path comes from the settings file or the command line, and the
//! names of the files in it from whoever put them there - in a team folder, a
//! stranger (untouchable rule 24). Each goes into a sentence through
//! [`escaped_prose`], so a control character or a direction override in a
//! file name cannot reorder or forge the line it stands in, and only after the
//! pattern was chosen, so a brace in a name is a brace (`fill`).

use std::path::Path;

use nkb_app::advance_sequence::ChooseError;
use nkb_app::ports::{CatalogueCoverage, CatalogueSource, PackOrigin, SourceSkipped};
use nkb_core::text::escaped_prose;

use super::fill::fill;
use crate::catalogue::PackFolders;
use crate::pack_folder::{FOLDER_FILES_LIMIT, FOLDER_WAIT};

/// How many misnamed files a note names before it counts the rest - the rule
/// `UnknownKeys` follows for the settings file.
const MISNAMED_NAMED: usize = 6;

/// The words about the two folders, as keys (`ux-spec.md` 6 D and K).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FolderNote {
    /// A pack in use instead of the built-in pack of the same name.
    OverridesBuiltIn,
    /// A pack of the own folder in use instead of the team's of the same name.
    OverridesTeam,
    /// Added to a sentence about a pack that could not be used and that
    /// overrides another: the other is not used in its place.
    NotReplaced,
    /// The team folder was not read, with the reason.
    TeamNotRead,
    /// The own folder was not read, with the reason.
    OwnNotRead,
    /// The reasons inside the two above.
    ReasonMissing,
    ReasonNotAFolder,
    ReasonUnreadable,
    ReasonUnreachable,
    ReasonTooManyFiles,
    /// The team folder's setting is there and could not be used - the palette
    /// said why when it started.
    TeamSettingUnusable,
    /// There is no own folder, because there is no place for the settings.
    OwnNoPlace,
    /// One file in a folder that is not named as a pack.
    MisnamedOne,
    /// Several such files.
    MisnamedMany,
    /// Several, and more than are named.
    MisnamedMore,
    /// Where the tester's own packs go.
    OwnHere,
    /// Where a pack in the value window comes from, before its description.
    FromFolder,
    /// The same, for a pack used instead of a built-in one.
    FromFolderInstead,
    /// The same, for an own pack used instead of the team's.
    FromOwnInsteadOfTeam,
    /// The pill beside a pack from a folder in the value window.
    BadgeTeam,
    BadgeOwn,
    /// The folder words inside the sentences.
    TeamFolder,
    OwnFolder,
}

fn pattern_folder_note(note: FolderNote) -> &'static str {
    match note {
        FolderNote::OverridesBuiltIn => {
            "Pack \"{pack}\" comes from your {folder} and is used instead of the built-in pack of the same name."
        }
        FolderNote::OverridesTeam => {
            "Pack \"{pack}\" comes from your own folder and is used instead of the one in your team folder."
        }
        FolderNote::NotReplaced => {
            "It replaces a pack of the same name, which is not used in its place."
        }
        FolderNote::TeamNotRead => {
            "The team pack folder {path} {reason}, so its packs are not offered. Built-in and own packs work."
        }
        FolderNote::OwnNotRead => {
            "Your own pack folder {path} {reason}, so its packs are not offered."
        }
        FolderNote::ReasonMissing => "does not exist",
        FolderNote::ReasonNotAFolder => "is a file, not a folder",
        FolderNote::ReasonUnreadable => "could not be read",
        FolderNote::ReasonUnreachable => "did not answer within {seconds} seconds",
        FolderNote::ReasonTooManyFiles => "holds more than {limit} pack files",
        FolderNote::TeamSettingUnusable => {
            "The team folder named in the settings file could not be used, so it is not read."
        }
        FolderNote::OwnNoPlace => {
            "There is no own pack folder: the settings file has no place on this system."
        }
        FolderNote::MisnamedOne => {
            "In your {folder}, {names} is not read: a pack file is named after its pack, such as whitespace.toml."
        }
        FolderNote::MisnamedMany => {
            "In your {folder}, {names} are not read: a pack file is named after its pack, such as whitespace.toml."
        }
        FolderNote::MisnamedMore => {
            "In your {folder}, {names} and {more} more files are not read: a pack file is named after its pack, such as whitespace.toml."
        }
        FolderNote::OwnHere => "Put your own pack files in {path}.",
        FolderNote::FromFolder => "From your {folder}.",
        FolderNote::FromFolderInstead => {
            "From your {folder}, used instead of the built-in pack of the same name."
        }
        FolderNote::FromOwnInsteadOfTeam => {
            "From your own folder, used instead of the one in your team folder."
        }
        FolderNote::BadgeTeam => "team folder",
        FolderNote::BadgeOwn => "own folder",
        FolderNote::TeamFolder => "team folder",
        FolderNote::OwnFolder => "own folder",
    }
}

/// The pattern for one pack-choosing failure. `ux-spec.md` 6, section D.
///
/// ⚠️ `Refused` has TWO patterns per place, because English inflects the noun:
/// one problem, three problems. Languages with more than two plural forms -
/// Polish among them - need rules, and those arrive with the second language.
///
/// A pack from a folder is named with its file, which `nkb lint` reads (R04,
/// `OBS-235`), in ONE sentence: [`choose_error`] may add a second about the
/// pack it overrides, and the document allows a message two. A built-in pack has no file on the tester's disk - its sentence
/// names the command alone, and no built-in pack reaches it, because
/// `shipped_packs.rs` checks every one before a release.
fn pattern_choose_error(error: &ChooseError, in_folder: bool) -> &'static str {
    match (error, in_folder) {
        (ChooseError::NotFound, _) => "Pack \"{pack}\" was not found. Run: nkb packs",
        (ChooseError::Unreadable { .. }, false) => {
            "Pack \"{pack}\" could not be read. Check that the file is there and is valid UTF-8."
        }
        (ChooseError::Unreadable { .. }, true) => {
            "Pack \"{pack}\" in your {folder} could not be read - check that {file} is there and is valid UTF-8."
        }
        (ChooseError::TooLarge { .. }, false) => {
            "Pack \"{pack}\" was not read: its file is larger than 4 MiB, the most a pack file may hold."
        }
        (ChooseError::TooLarge { .. }, true) => {
            "Pack \"{pack}\" in your {folder} was not read: {file} is larger than 4 MiB, the most a pack file may hold."
        }
        (ChooseError::Refused { errors, .. }, false) if *errors == 1 => {
            "Pack \"{pack}\" has 1 problem. Run nkb lint on its file to see it."
        }
        (ChooseError::Refused { .. }, false) => {
            "Pack \"{pack}\" has {errors} problems. Run nkb lint on its file to see them."
        }
        (ChooseError::Refused { errors, .. }, true) if *errors == 1 => {
            "Pack \"{pack}\" in your {folder} has 1 problem, which nkb lint \"{file}\" shows."
        }
        (ChooseError::Refused { .. }, true) => {
            "Pack \"{pack}\" in your {folder} has {errors} problems, which nkb lint \"{file}\" shows."
        }
    }
}

/// What the palette says when a pack could not be chosen.
///
/// `pack` is the identifier that was asked for, and `folders` place the file of a
/// pack that came from one.
#[must_use]
pub fn choose_error(error: &ChooseError, pack: &str, folders: &PackFolders) -> String {
    let origin = match error {
        ChooseError::NotFound => None,
        ChooseError::Unreadable { origin }
        | ChooseError::TooLarge { origin }
        | ChooseError::Refused { origin, .. } => Some(origin),
    };
    let file = origin.and_then(|origin| {
        folders
            .path_of(origin.source)
            .map(|folder| shown_path(&folder.join(format!("{pack}.toml"))))
    });
    let folder = origin.map_or("", |origin| folder_word(origin.source));
    let errors = match error {
        ChooseError::Refused { errors, .. } => errors.to_string(),
        ChooseError::NotFound | ChooseError::Unreadable { .. } | ChooseError::TooLarge { .. } => {
            String::new()
        }
    };
    let mut sentence = fill(
        pattern_choose_error(error, file.is_some()),
        &[
            ("pack", &escaped_prose(pack)),
            ("folder", folder),
            ("file", file.as_deref().unwrap_or_default()),
            ("errors", &errors),
        ],
    );
    if origin.is_some_and(|origin| !origin.overrides.is_empty()) {
        sentence.push(' ');
        sentence.push_str(pattern_folder_note(FolderNote::NotReplaced));
    }
    sentence
}

/// What the palette says when the pack it opened is used instead of another of
/// the same name (`pack-format.md` 12: never a silent override). `None` for a
/// pack that overrides nothing.
#[must_use]
pub fn pack_overrides(pack: &str, origin: &PackOrigin) -> Option<String> {
    let note = match origin.overrides.first()? {
        CatalogueSource::Team => FolderNote::OverridesTeam,
        CatalogueSource::BuiltIn | CatalogueSource::Own => FolderNote::OverridesBuiltIn,
    };
    Some(fill(
        pattern_folder_note(note),
        &[
            ("pack", &escaped_prose(pack)),
            ("folder", folder_word(origin.source)),
        ],
    ))
}

/// What is said about a folder that was named and could not be read. `None`
/// for a reason that is not a fault to report: no folder named, an own folder
/// not made yet, a team folder that is the own one, and the two reasons the
/// palette has already said through the settings file.
#[must_use]
pub fn folder_not_read(
    source: CatalogueSource,
    why: SourceSkipped,
    folders: &PackFolders,
) -> Option<String> {
    let reason = match why {
        SourceSkipped::Missing if source == CatalogueSource::Own => return None,
        SourceSkipped::Missing => FolderNote::ReasonMissing,
        SourceSkipped::NotAFolder => FolderNote::ReasonNotAFolder,
        SourceSkipped::Unreadable => FolderNote::ReasonUnreadable,
        SourceSkipped::Unreachable => FolderNote::ReasonUnreachable,
        SourceSkipped::TooManyFiles => FolderNote::ReasonTooManyFiles,
        SourceSkipped::NotSet
        | SourceSkipped::SettingUnusable
        | SourceSkipped::NoPlace
        | SourceSkipped::SameAsOwn => return None,
    };
    let note = match source {
        CatalogueSource::Team => FolderNote::TeamNotRead,
        CatalogueSource::Own => FolderNote::OwnNotRead,
        // The packs the tool carries are never skipped.
        CatalogueSource::BuiltIn => return None,
    };
    let reason = fill(
        pattern_folder_note(reason),
        &[
            ("seconds", &FOLDER_WAIT.as_secs().to_string()),
            ("limit", &FOLDER_FILES_LIMIT.to_string()),
        ],
    );
    let path = folders.path_of(source).map(shown_path).unwrap_or_default();
    Some(fill(
        pattern_folder_note(note),
        &[("path", &path), ("reason", &reason)],
    ))
}

/// The notes under the list of the value window: what was not read and why,
/// files that are not named as packs, and where the tester's own packs go.
///
/// The last line is there on every opening on purpose - it is the one place
/// the window tells a tester that packs of their own can be added, and it
/// reads as an instruction, not as a fault (`UX-GUI-009`: a note that sounds
/// like a failure on every opening teaches the tester to skip the real one).
#[must_use]
pub fn catalogue_notes(coverage: &CatalogueCoverage, folders: &PackFolders) -> Vec<String> {
    let mut notes = Vec::new();
    for (source, why) in &coverage.skipped {
        match (source, why) {
            (CatalogueSource::Team, SourceSkipped::SettingUnusable) => {
                notes.push(pattern_folder_note(FolderNote::TeamSettingUnusable).to_owned());
            }
            (CatalogueSource::Own, SourceSkipped::NoPlace) => {
                notes.push(pattern_folder_note(FolderNote::OwnNoPlace).to_owned());
            }
            _ => notes.extend(folder_not_read(*source, *why, folders)),
        }
    }
    for source in [CatalogueSource::Team, CatalogueSource::Own] {
        let names: Vec<&str> = coverage
            .misnamed
            .iter()
            .filter(|(from, _)| *from == source)
            .map(|(_, name)| name.as_str())
            .collect();
        notes.extend(misnamed(source, &names));
    }
    if let Ok(own) = folders.own() {
        notes.push(fill(
            pattern_folder_note(FolderNote::OwnHere),
            &[("path", &shown_path(own))],
        ));
    }
    notes
}

/// Where a pack comes from, as the value window says it before the pack's
/// description - `None` for a built-in pack. The override is never silent
/// (`pack-format.md` 12).
#[must_use]
pub fn pack_source(origin: &PackOrigin) -> Option<String> {
    if origin.source == CatalogueSource::BuiltIn {
        return None;
    }
    let note = match origin.overrides.first() {
        None => FolderNote::FromFolder,
        Some(CatalogueSource::Team) => FolderNote::FromOwnInsteadOfTeam,
        Some(CatalogueSource::BuiltIn | CatalogueSource::Own) => FolderNote::FromFolderInstead,
    };
    Some(fill(
        pattern_folder_note(note),
        &[("folder", folder_word(origin.source))],
    ))
}

/// The pill beside a pack from a folder - `None` for a built-in pack.
#[must_use]
pub fn source_badge(origin: &PackOrigin) -> Option<&'static str> {
    match origin.source {
        CatalogueSource::BuiltIn => None,
        CatalogueSource::Team => Some(pattern_folder_note(FolderNote::BadgeTeam)),
        CatalogueSource::Own => Some(pattern_folder_note(FolderNote::BadgeOwn)),
    }
}

fn misnamed(source: CatalogueSource, names: &[&str]) -> Option<String> {
    let note = match names.len() {
        0 => return None,
        1 => FolderNote::MisnamedOne,
        count if count <= MISNAMED_NAMED + 1 => FolderNote::MisnamedMany,
        _ => FolderNote::MisnamedMore,
    };
    let named: Vec<String> = names
        .iter()
        .take(if note == FolderNote::MisnamedMore {
            MISNAMED_NAMED
        } else {
            names.len()
        })
        .map(|name| format!("\"{}\"", escaped_prose(name)))
        .collect();
    let more = names.len().saturating_sub(named.len()).to_string();
    Some(fill(
        pattern_folder_note(note),
        &[
            ("folder", folder_word(source)),
            ("names", &named.join(", ")),
            ("more", &more),
        ],
    ))
}

fn folder_word(source: CatalogueSource) -> &'static str {
    match source {
        CatalogueSource::Team => pattern_folder_note(FolderNote::TeamFolder),
        CatalogueSource::Own | CatalogueSource::BuiltIn => {
            pattern_folder_note(FolderNote::OwnFolder)
        }
    }
}

/// A path for a sentence: lossy where the system's name is not Unicode, and
/// escaped as somebody else's text.
fn shown_path(path: &Path) -> String {
    escaped_prose(&path.to_string_lossy())
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn team_folders() -> PackFolders {
        PackFolders::new(
            Ok(PathBuf::from("/work/qa packs")),
            Ok(PathBuf::from("/home/tester/.config/naughty-keyboard/packs")),
        )
    }

    fn team_over_built_in() -> PackOrigin {
        PackOrigin {
            source: CatalogueSource::Team,
            overrides: vec![CatalogueSource::BuiltIn],
        }
    }

    #[test]
    fn a_refused_folder_pack_names_its_file_and_that_the_built_in_one_is_not_used() {
        let folder = PathBuf::from("/work/qa packs").join("whitespace.toml");
        let said = choose_error(
            &ChooseError::Refused {
                errors: 3,
                origin: team_over_built_in(),
            },
            "whitespace",
            &team_folders(),
        );
        assert_eq!(
            said,
            format!(
                "Pack \"whitespace\" in your team folder has 3 problems, which nkb lint \"{}\" shows. It replaces a pack of the same name, which is not used in its place.",
                folder.to_string_lossy()
            )
        );
    }

    #[test]
    fn a_built_in_pack_that_cannot_be_chosen_keeps_the_sentence_without_a_file() {
        assert_eq!(
            choose_error(
                &ChooseError::Refused {
                    errors: 1,
                    origin: PackOrigin::built_in(),
                },
                "locale-cz",
                &team_folders(),
            ),
            "Pack \"locale-cz\" has 1 problem. Run nkb lint on its file to see it."
        );
        // The noun is inflected rather than parenthesised.
        assert_eq!(
            choose_error(
                &ChooseError::Refused {
                    errors: 3,
                    origin: PackOrigin::built_in(),
                },
                "locale-cz",
                &team_folders(),
            ),
            "Pack \"locale-cz\" has 3 problems. Run nkb lint on its file to see them."
        );
        assert_eq!(
            choose_error(&ChooseError::NotFound, "nope", &team_folders()),
            "Pack \"nope\" was not found. Run: nkb packs"
        );
    }

    #[test]
    fn the_size_in_the_sentence_is_the_limit_in_the_code() {
        // The sentences say "4 MiB" in words, and this keeps them true.
        assert_eq!(nkb_app::PACK_FILE_LIMIT, 4 * 1024 * 1024);
    }

    #[test]
    fn a_pack_used_instead_of_another_says_so_and_a_plain_one_says_nothing() {
        assert_eq!(
            pack_overrides("whitespace", &team_over_built_in()).as_deref(),
            Some(
                "Pack \"whitespace\" comes from your team folder and is used instead of the built-in pack of the same name."
            )
        );
        let own_over_team = PackOrigin {
            source: CatalogueSource::Own,
            overrides: vec![CatalogueSource::Team, CatalogueSource::BuiltIn],
        };
        assert_eq!(
            pack_overrides("whitespace", &own_over_team).as_deref(),
            Some(
                "Pack \"whitespace\" comes from your own folder and is used instead of the one in your team folder."
            )
        );
        assert_eq!(pack_overrides("whitespace", &PackOrigin::built_in()), None);
    }

    #[test]
    fn the_value_window_says_where_a_folder_pack_comes_from_and_nothing_for_a_built_in_one() {
        assert_eq!(pack_source(&PackOrigin::built_in()), None);
        assert_eq!(source_badge(&PackOrigin::built_in()), None);
        assert_eq!(
            pack_source(&team_over_built_in()).as_deref(),
            Some("From your team folder, used instead of the built-in pack of the same name.")
        );
        let own = PackOrigin {
            source: CatalogueSource::Own,
            overrides: Vec::new(),
        };
        assert_eq!(pack_source(&own).as_deref(), Some("From your own folder."));
        assert_eq!(source_badge(&own), Some("own folder"));
    }

    #[test]
    fn a_folder_fault_is_said_with_its_path_and_the_normal_states_are_not() {
        let folders = team_folders();
        let unreachable =
            folder_not_read(CatalogueSource::Team, SourceSkipped::Unreachable, &folders)
                .expect("a fault to say");
        assert!(unreachable.starts_with("The team pack folder "));
        assert!(unreachable.contains("did not answer within 2 seconds"));
        assert!(
            folder_not_read(CatalogueSource::Team, SourceSkipped::TooManyFiles, &folders)
                .expect("a fault")
                .contains("more than 256 pack files")
        );
        for quiet in [
            (CatalogueSource::Own, SourceSkipped::Missing),
            (CatalogueSource::Team, SourceSkipped::NotSet),
            (CatalogueSource::Team, SourceSkipped::SameAsOwn),
        ] {
            assert_eq!(
                folder_not_read(quiet.0, quiet.1, &folders),
                None,
                "{quiet:?}"
            );
        }
    }

    #[test]
    fn a_hostile_file_name_cannot_forge_or_reorder_the_note() {
        // A right-to-left override and a line break in a name, as a stranger
        // could put them in a team folder.
        let hostile = "evil\u{202E}lmot.\nNot read: everything.toml";
        let coverage = CatalogueCoverage {
            consulted: vec![CatalogueSource::BuiltIn, CatalogueSource::Team],
            skipped: Vec::new(),
            misnamed: vec![(CatalogueSource::Team, hostile.to_owned())],
        };
        let notes = catalogue_notes(&coverage, &team_folders());
        let note = notes.first().expect("the misnamed file is named");
        assert!(!note.contains('\u{202E}'), "{note}");
        assert!(!note.contains('\n'), "{note}");
    }

    #[test]
    fn many_misnamed_files_are_named_up_to_six_and_counted_after() {
        let names: Vec<String> = (0..9).map(|n| format!("Pack {n}.toml")).collect();
        let coverage = CatalogueCoverage {
            consulted: vec![CatalogueSource::BuiltIn, CatalogueSource::Own],
            skipped: Vec::new(),
            misnamed: names
                .iter()
                .map(|name| (CatalogueSource::Own, name.clone()))
                .collect(),
        };
        let notes = catalogue_notes(&coverage, &team_folders());
        assert!(
            notes[0].contains("\"Pack 5.toml\" and 3 more files are not read"),
            "{}",
            notes[0]
        );
        assert!(!notes[0].contains("Pack 6.toml"));
        assert!(
            notes
                .last()
                .expect("the own line")
                .starts_with("Put your own pack files in ")
        );
    }
}
