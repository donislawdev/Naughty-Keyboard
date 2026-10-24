//! The entry that gives the program an identifier: whether it is there, and making it
//! (`D145` point 53, `OBS-222`).
//!
//! The text and the places are decided in `linux_portal_rules.rs`. Here are the files.
//!
//! # Nothing is ever replaced, and nothing is left half written
//!
//! The entry is written to a file of its own in the same directory, made new (so a stale one
//! of an earlier crash is not extended), and then put in place under its name with a hard
//! link. A hard link fails when the name is taken, where a rename would replace what is
//! there, so a file somebody else made, or one that appeared between the look and the link,
//! is never overwritten (untouchable rule 2, and the data of the person is sacred, rule 31).
//! The file of its own is removed on every path.

use std::fs::{self, OpenOptions};
use std::io::Write as _;
use std::path::Path;

use crate::linux_portal_rules::{
    DataDirs, ENTRY_FILE, entry_candidates, entry_text, is_our_entry, user_applications,
};
use crate::portal::{EntryError, Removed, Written};

/// The directories the environment names.
struct Env {
    data_home: Option<String>,
    home: Option<String>,
    data_dirs: Option<String>,
}

impl Env {
    fn read() -> Self {
        Self {
            data_home: std::env::var("XDG_DATA_HOME").ok(),
            home: std::env::var("HOME").ok(),
            data_dirs: std::env::var("XDG_DATA_DIRS").ok(),
        }
    }

    fn dirs(&self) -> DataDirs<'_> {
        DataDirs {
            data_home: self.data_home.as_deref(),
            home: self.home.as_deref(),
            data_dirs: self.data_dirs.as_deref(),
        }
    }
}

/// Whether an entry of this program is in a directory the desktop looks in.
pub(crate) fn present() -> bool {
    entry_candidates(&Env::read().dirs())
        .iter()
        .any(|path| Path::new(path).is_file())
}

/// Whether the entry in the person's own directory of applications is one this program
/// made, which is the only one [`remove`] would take away.
pub(crate) fn ours() -> bool {
    user_applications(&Env::read().dirs()).is_some_and(|directory| ours_in(Path::new(&directory)))
}

/// [`ours`] in `directory`.
pub(crate) fn ours_in(directory: &Path) -> bool {
    fs::read_to_string(directory.join(ENTRY_FILE)).is_ok_and(|content| is_our_entry(&content))
}

/// Makes the entry in the person's own directory of applications, unless one is there.
pub(crate) fn write() -> Result<Written, EntryError> {
    let env = Env::read();
    let dirs = env.dirs();
    if entry_candidates(&dirs)
        .iter()
        .any(|path| Path::new(path).is_file())
    {
        return Ok(Written::AlreadyThere);
    }
    let directory = user_applications(&dirs).ok_or(EntryError::NoPlace)?;
    let program = std::env::current_exe().map_err(|_| EntryError::NoProgram)?;
    let program = program.to_str().ok_or(EntryError::NoProgram)?;
    let text = entry_text(program).map_err(|_| EntryError::NoProgram)?;
    write_in(Path::new(&directory), &text)
}

/// Takes away the entry this program made, and only that one: a file of the same name that
/// somebody else made, or one in a directory of the system, is left where it is.
pub(crate) fn remove() -> Result<Removed, EntryError> {
    let env = Env::read();
    let directory = user_applications(&env.dirs()).ok_or(EntryError::NoPlace)?;
    remove_in(Path::new(&directory))
}

/// [`remove`] in `directory`.
pub(crate) fn remove_in(directory: &Path) -> Result<Removed, EntryError> {
    let target = directory.join(ENTRY_FILE);
    let content = match fs::read_to_string(&target) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Removed::NotThere);
        }
        Err(_) => return Err(EntryError::Io),
    };
    if !is_our_entry(&content) {
        return Ok(Removed::NotOurs);
    }
    fs::remove_file(&target).map_err(|_| EntryError::Io)?;
    Ok(Removed::Removed)
}

/// Puts `text` under the name of the entry in `directory`, made if it is not there.
pub(crate) fn write_in(directory: &Path, text: &str) -> Result<Written, EntryError> {
    fs::create_dir_all(directory).map_err(|_| EntryError::Io)?;
    let target = directory.join(ENTRY_FILE);
    let own = directory.join(format!(".{ENTRY_FILE}.{}.part", std::process::id()));
    let outcome = put(&own, &target, text);
    let _ = fs::remove_file(&own);
    outcome.map(|made| {
        if made {
            Written::Made(target.to_string_lossy().into_owned())
        } else {
            Written::AlreadyThere
        }
    })
}

/// Writes `own` whole and links it to `target`. `Ok(false)` when `target` is there already.
fn put(own: &Path, target: &Path, text: &str) -> Result<bool, EntryError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(own)
        .map_err(|_| EntryError::Io)?;
    file.write_all(text.as_bytes())
        .map_err(|_| EntryError::Io)?;
    file.sync_all().map_err(|_| EntryError::Io)?;
    drop(file);
    match fs::hard_link(own, target) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
        Err(_) => Err(EntryError::Io),
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
    use crate::linux_portal_rules::{ENTRY_MARK, is_our_entry};

    /// A directory of its own for one test, removed when the test is over.
    struct Scratch(std::path::PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("nkb-entry-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&path);
            Self(path)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn the_entry_is_made_whole_in_a_directory_that_is_not_there_yet() {
        let scratch = Scratch::new("made");
        let directory = scratch.0.join("a").join("applications");
        let text = entry_text("/opt/nkb/nkb-gui").expect("an entry");
        let written = write_in(&directory, &text).expect("written");
        let target = directory.join(ENTRY_FILE);
        assert_eq!(
            written,
            Written::Made(target.to_string_lossy().into_owned())
        );
        let content = fs::read_to_string(&target).expect("readable");
        assert_eq!(content, text);
        assert!(is_our_entry(&content));
        let left: Vec<_> = fs::read_dir(&directory)
            .expect("listed")
            .map(|entry| entry.expect("entry").file_name())
            .collect();
        assert_eq!(
            left,
            vec![std::ffi::OsString::from(ENTRY_FILE)],
            "no file of its own left"
        );
    }

    #[test]
    fn a_file_that_is_there_is_never_replaced() {
        let scratch = Scratch::new("kept");
        fs::create_dir_all(&scratch.0).expect("directory");
        let target = scratch.0.join(ENTRY_FILE);
        fs::write(&target, "[Desktop Entry]\nName=Somebody else's\n").expect("written");
        let text = entry_text("/opt/nkb/nkb-gui").expect("an entry");
        assert_eq!(write_in(&scratch.0, &text), Ok(Written::AlreadyThere));
        let content = fs::read_to_string(&target).expect("readable");
        assert!(content.contains("Somebody else's"), "{content:?}");
        assert!(!content.contains(ENTRY_MARK));
        let left = fs::read_dir(&scratch.0).expect("listed").count();
        assert_eq!(left, 1, "no file of its own left");
    }

    #[test]
    fn only_the_entry_this_program_made_is_taken_away() {
        let scratch = Scratch::new("removed");
        fs::create_dir_all(&scratch.0).expect("directory");
        assert_eq!(remove_in(&scratch.0), Ok(Removed::NotThere));

        let text = entry_text("/opt/nkb/nkb-gui").expect("an entry");
        write_in(&scratch.0, &text).expect("written");
        assert_eq!(remove_in(&scratch.0), Ok(Removed::Removed));
        assert!(!scratch.0.join(ENTRY_FILE).exists());

        let theirs = scratch.0.join(ENTRY_FILE);
        fs::write(&theirs, "[Desktop Entry]\nName=Somebody else's\n").expect("written");
        assert_eq!(remove_in(&scratch.0), Ok(Removed::NotOurs));
        assert!(theirs.exists(), "a file that is not ours is never removed");
    }

    #[test]
    fn an_entry_is_ours_only_when_this_program_made_it() {
        let scratch = Scratch::new("ours");
        fs::create_dir_all(&scratch.0).expect("directory");
        assert!(!ours_in(&scratch.0), "no file, no entry of ours");

        let text = entry_text("/opt/nkb/nkb-gui").expect("an entry");
        write_in(&scratch.0, &text).expect("written");
        assert!(ours_in(&scratch.0));

        fs::write(
            scratch.0.join(ENTRY_FILE),
            "[Desktop Entry]\nName=Somebody else's\n",
        )
        .expect("written");
        assert!(!ours_in(&scratch.0), "a file of the same name is not ours");
        assert!(
            !ours_in(&scratch.0.join("absent")),
            "no directory, no entry"
        );
    }

    #[test]
    fn a_directory_that_cannot_be_made_is_an_error_and_not_a_panic() {
        let scratch = Scratch::new("blocked");
        fs::create_dir_all(&scratch.0).expect("directory");
        let blocker = scratch.0.join("file");
        fs::write(&blocker, "x").expect("written");
        let inside = blocker.join("applications");
        assert_eq!(write_in(&inside, "x"), Err(EntryError::Io));
    }
}
