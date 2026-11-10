//! The team folder chosen in the pack folders window (`D175`): whether a folder
//! can be it, and as what text the settings file keeps it.
//!
//! The folder comes from the system's folder window or from a folder dropped on
//! the window - the system's word for it, which is no promise: a desktop portal
//! older than its `directory` option lets a file be chosen, a drop carries files
//! as readily as folders, and a folder can be gone by the time it is read. So it
//! is looked at here, once, with the time limit every folder is read with
//! (`FOLDER_WAIT`), and refused with a reason for each way it cannot be the team
//! folder. A folder that is there and does not answer in time is taken - it was
//! chosen, and the window says it does not answer, as for any team folder.

use std::path::Path;

use nkb_app::{CatalogueSource, SourceSkipped};

use crate::catalogue::{FolderHolds, PackFolders};

/// Why a folder cannot become the team folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeamFolderRefusal {
    /// Its name cannot be written in the settings file, which holds text: a
    /// name that is not Unicode - any bytes on Linux, an unpaired surrogate on
    /// Windows.
    NotText,
    /// Not a full path - which neither the folder window nor a drop gives, and
    /// which the settings would not take back (`D173`).
    NotFullPath,
    /// Not there, or a file rather than a folder.
    NotAFolder,
    /// The tester's own folder, whose packs are read already, as their own.
    OwnFolder,
}

/// The team folder `chosen` would be: the text the settings file keeps, and
/// the folders of the run with it - or why it cannot be.
///
/// # Errors
///
/// Each way `chosen` cannot be the team folder, as [`TeamFolderRefusal`].
pub fn take_team_folder(
    folders: &PackFolders,
    chosen: &Path,
) -> Result<(String, PackFolders), TeamFolderRefusal> {
    let text = chosen.to_str().ok_or(TeamFolderRefusal::NotText)?;
    if !chosen.is_absolute() {
        return Err(TeamFolderRefusal::NotFullPath);
    }
    let taken = folders.with_team(Some(chosen.to_path_buf()));
    match taken.catalogue().holds_of(CatalogueSource::Team) {
        Some(FolderHolds::Skipped(SourceSkipped::Missing | SourceSkipped::NotAFolder)) => {
            Err(TeamFolderRefusal::NotAFolder)
        }
        Some(FolderHolds::Skipped(SourceSkipped::SameAsOwn)) => Err(TeamFolderRefusal::OwnFolder),
        _ => Ok((text.to_owned(), taken)),
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
    use std::path::PathBuf;

    /// A folder of its own in the system's temporary place, removed when
    /// dropped.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir()
                .join("nkb-team-folder-tests")
                .join(format!("{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("a scratch folder");
            Self(path)
        }
        fn sub(&self, name: &str) -> PathBuf {
            let path = self.0.join(name);
            std::fs::create_dir_all(&path).expect("a folder");
            path
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn folders(own: &Path) -> PackFolders {
        PackFolders::new(Err(SourceSkipped::NotSet), Ok(own.to_path_buf()))
    }

    #[test]
    fn a_folder_becomes_the_team_folder_as_the_text_it_is_named_by() {
        let scratch = Scratch::new("taken");
        let team = scratch.sub("team packs");
        let own = scratch.sub("own");
        let (text, taken) = take_team_folder(&folders(&own), &team).expect("a folder");
        assert_eq!(Path::new(&text), team);
        assert_eq!(taken.team(), Ok(team.as_path()));
        assert_eq!(taken.own(), Ok(own.as_path()), "the own folder stays");
    }

    #[test]
    fn what_is_not_a_folder_or_is_the_own_folder_is_refused() {
        let scratch = Scratch::new("refused");
        let own = scratch.sub("own");
        let file = scratch.0.join("a-pack.toml");
        std::fs::write(&file, "").expect("a file");
        let folders = folders(&own);
        for (chosen, why) in [
            (file.clone(), TeamFolderRefusal::NotAFolder),
            (scratch.0.join("gone"), TeamFolderRefusal::NotAFolder),
            (own.clone(), TeamFolderRefusal::OwnFolder),
            (
                PathBuf::from("relative-packs"),
                TeamFolderRefusal::NotFullPath,
            ),
        ] {
            assert_eq!(
                take_team_folder(&folders, &chosen).map(|(text, _)| text),
                Err(why),
                "{}",
                chosen.display()
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_name_that_is_not_text_cannot_be_kept_in_the_settings() {
        use std::os::unix::ffi::OsStrExt;
        let scratch = Scratch::new("not-text");
        let own = scratch.sub("own");
        // Never made on the disk: APFS refuses such a name (EILSEQ, the macOS
        // runner), and the refusal comes before anything is read anyway.
        let odd = scratch.0.join(std::ffi::OsStr::from_bytes(b"packs-\xff"));
        assert_eq!(
            take_team_folder(&folders(&own), &odd).map(|(text, _)| text),
            Err(TeamFolderRefusal::NotText)
        );
    }

    #[cfg(windows)]
    #[test]
    fn a_name_that_is_not_text_cannot_be_kept_in_the_settings() {
        use std::os::windows::ffi::OsStringExt;
        let scratch = Scratch::new("not-text");
        let own = scratch.sub("own");
        let mut name: Vec<u16> = "packs-".encode_utf16().collect();
        name.push(0xD800);
        let odd = scratch.0.join(std::ffi::OsString::from_wide(&name));
        std::fs::create_dir_all(&odd).expect("a folder with a name that is not text");
        assert_eq!(
            take_team_folder(&folders(&own), &odd).map(|(text, _)| text),
            Err(TeamFolderRefusal::NotText)
        );
    }
}
