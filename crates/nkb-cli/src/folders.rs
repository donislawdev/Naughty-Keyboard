//! Which pack folders a command reads (`D173`): the team folder from
//! `--team-folder`, or else from the settings file the palette writes, and the
//! tester's own folder beside that file.
//!
//! # The settings file is read, never written
//!
//! `nkb` has no palette and nothing to remember, so the one key it takes from
//! the settings file is `team-folder` - read the way the palette reads it
//! (`KeptSettings::open`), so the two programs cannot disagree about which
//! folder the settings name. A run with `--team-folder` does not read the file
//! at all: a pipeline names its folder and needs nothing from a tester's
//! settings.
//!
//! # The option is somebody's text
//!
//! The folder named is read as a folder and nothing else: what is in it goes
//! through the same limits as any pack (`pack_folder.rs`), and its path is
//! shown escaped wherever it is shown.

use std::io::Write;
use std::path::PathBuf;

use nkb_adapters::{FolderCatalogue, PackFolders, SettingsFile, i18n};
use nkb_app::PackCatalogue;
use nkb_app::keep_settings::KeptSettings;

use crate::exit::ExitCode;

/// The option, as typed.
const OPTION: &str = "--team-folder";

/// Runs `command` with its arguments less `--team-folder <folder>`, and the
/// folders of this run.
pub(crate) fn then(
    args: &[String],
    command: &str,
    run: fn(&[String], &PackFolders) -> ExitCode,
) -> ExitCode {
    match take(args, command) {
        Ok((rest, team)) => run(&rest, &of_run(team)),
        Err(code) => code,
    }
}

/// Takes `--team-folder <folder>` out of `args`, once at most. Apart from
/// [`of_run`] for a command that reads the folders on one of its paths only:
/// `nkb send` with a limit reads no pack (`D178`).
pub(crate) fn take(
    args: &[String],
    command: &str,
) -> Result<(Vec<String>, Option<PathBuf>), ExitCode> {
    let mut rest = Vec::with_capacity(args.len());
    let mut team = None;
    let mut words = args.iter();
    while let Some(word) = words.next() {
        if word != OPTION {
            rest.push(word.clone());
            continue;
        }
        let mut err = crate::output::err();
        match words.next() {
            Some(folder) if !folder.is_empty() && team.is_none() => {
                team = Some(PathBuf::from(folder));
            }
            Some(folder) if !folder.is_empty() => {
                let _ = writeln!(err, "nkb {command}: name one team folder, not two.");
                return Err(ExitCode::Usage);
            }
            _ => {
                let _ = writeln!(err, "nkb {command}: {OPTION} needs a folder.");
                let _ = writeln!(err, "Run 'nkb {command} --help' to see what is available.");
                return Err(ExitCode::Usage);
            }
        }
    }
    Ok((rest, team))
}

/// The folders of this run: the option when given, else the settings.
pub(crate) fn of_run(team: Option<PathBuf>) -> PackFolders {
    let file = SettingsFile::for_this_user();
    if team.is_some() {
        return PackFolders::for_run(&file, &nkb_app::Settings::default(), &[], team);
    }
    let (kept, said) = KeptSettings::open(&file);
    PackFolders::for_run(&file, kept.settings(), &said, None)
}

/// Says, after "no such pack", which folder was not read and why - the pack
/// may well be in it. Nothing when every folder named was read.
pub(crate) fn say_not_read(
    err: &mut impl Write,
    command: &str,
    catalogue: &FolderCatalogue,
    folders: &PackFolders,
) {
    for (source, why) in catalogue.coverage().skipped {
        if let Some(sentence) = i18n::folder_not_read(source, why, folders) {
            let _ = writeln!(err, "nkb {command}: {sentence}");
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

    fn words(text: &[&str]) -> Vec<String> {
        text.iter().map(|word| (*word).to_owned()).collect()
    }

    #[test]
    fn the_option_is_taken_out_wherever_it_stands_and_the_rest_keeps_its_order() {
        let (rest, team) = take(
            &words(&["whitespace", "--team-folder", "qa packs", "--format", "csv"]),
            "emit",
        )
        .expect("reads");
        assert_eq!(rest, words(&["whitespace", "--format", "csv"]));
        assert_eq!(team, Some(PathBuf::from("qa packs")));
        let (rest, team) = take(&words(&["whitespace"]), "show").expect("reads");
        assert_eq!(rest, words(&["whitespace"]));
        assert_eq!(team, None);
    }

    #[test]
    fn the_option_needs_one_folder_and_takes_only_one() {
        for wrong in [
            words(&["--team-folder"]),
            words(&["--team-folder", ""]),
            words(&["--team-folder", "a", "--team-folder", "b"]),
        ] {
            assert_eq!(
                take(&wrong, "packs").err(),
                Some(ExitCode::Usage),
                "{wrong:?}"
            );
        }
    }
}
