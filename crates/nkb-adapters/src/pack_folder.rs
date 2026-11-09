//! One folder of pack files - the team's or the tester's own (`pack-format.md`
//! 12, `D173`) - and the two questions asked of it: which packs it holds, and
//! the text of one of them.
//!
//! # Every question has a deadline
//!
//! A team folder may sit on a network share, and a share that stops answering
//! can hold `read_dir` or a read for tens of seconds. The value window asks on
//! the interface thread and the palette asks on the thread that carries the
//! shortcuts, so a folder that hangs would freeze the window or swallow the
//! presses. Each question therefore runs on a helper thread of its own, and the
//! caller waits at most [`FOLDER_WAIT`]. A question that did not answer in time
//! is abandoned, not cancelled - the system call cannot be - and while one is
//! still hanging the folder answers every new question at once with "did not
//! answer", so a dead share costs one wait, not one thread per question.
//!
//! # What is a pack file here
//!
//! Only the folder itself is read, no folder below it. A file is a pack file
//! when its name ends in `.toml`, lower case, the way `nkb lint` reads one
//! (R04). Of those:
//!
//! - a name that is a pack identifier plus `.toml` is a pack (`E010` asks the
//!   pack to declare that same identifier).
//! - a translation, `<id>.<language>.toml` (`pack-format.md` 10), belongs to a
//!   pack and is passed over in silence, as is a hidden file starting with a
//!   dot - neither is a pack somebody expects to see.
//! - any other `.toml` - `My Pack.toml`, `Cargo.toml` - is named back as
//!   misnamed, because a pack that vanishes from every list with no word is
//!   the silence untouchable rule 1 forbids.
//!
//! More than [`FOLDER_FILES_LIMIT`] such files and the folder is not read at
//! all: a folder like that is far likelier a repository's root or a whole drive
//! named by mistake than a catalogue, and reading and checking every file in it
//! on each opening of the value window would be the cost of the mistake.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use nkb_app::{PackSource, SourceError, SourceSkipped};
use nkb_core::identity::is_pack_id;

use crate::fs::DirectoryPackSource;

/// How long one question to a pack folder may take before the folder counts as
/// not answering - the wait `SERVER_WAIT` gives a display server on X11. A
/// local disk answers in milliseconds, so only a share that stopped
/// responding ever reaches it.
pub const FOLDER_WAIT: Duration = Duration::from_secs(2);

/// The most pack files one folder may hold (`D173`). About eleven times the
/// catalogue the tool carries. A folder with more is read as the wrong folder.
pub const FOLDER_FILES_LIMIT: usize = 256;

/// One folder of pack files and the questions still hanging on it.
///
/// Cheap to clone, and clones share the count of hanging questions - the
/// palette asks from two threads, and both must learn that the folder hangs.
#[derive(Debug, Clone)]
pub struct PackFolder {
    path: PathBuf,
    hanging: Arc<AtomicUsize>,
    wait: Duration,
}

/// What a folder holds, as one listing of it found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderListing {
    /// The folder as the system resolves it, links and all - how two names of
    /// one folder are told to be the same folder.
    pub resolved: PathBuf,
    /// Pack identifiers, in alphabetical order.
    pub packs: Vec<String>,
    /// Names of `.toml` files that are not packs, as the folder gave them, in
    /// alphabetical order.
    pub misnamed: Vec<String>,
}

/// How the questions of one folder are answered: by the disk on a helper
/// thread, or - in a test - by a stand-in that can hang on purpose.
type Question<T> = Box<dyn FnOnce(&Path) -> T + Send>;

impl PackFolder {
    /// A folder at `path`, asked with the deadline [`FOLDER_WAIT`].
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self::with_wait(path, FOLDER_WAIT)
    }

    /// A folder asked with another deadline - for the tests, which cannot
    /// spend two seconds on every hanging question.
    #[must_use]
    pub fn with_wait(path: impl Into<PathBuf>, wait: Duration) -> Self {
        Self {
            path: path.into(),
            hanging: Arc::new(AtomicUsize::new(0)),
            wait,
        }
    }

    /// The folder as it was named, for a sentence that tells the tester where
    /// to look.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Which packs the folder holds.
    ///
    /// # Errors
    ///
    /// The reason the folder was not read, in the words of the catalogue's
    /// account: missing, not a folder, unreadable, not answering in time, or
    /// holding too many pack files.
    pub fn list(&self) -> Result<FolderListing, SourceSkipped> {
        self.list_with(Box::new(list_folder))
    }

    fn list_with(
        &self,
        question: Question<Result<FolderListing, SourceSkipped>>,
    ) -> Result<FolderListing, SourceSkipped> {
        self.ask(question)
            .unwrap_or(Err(SourceSkipped::Unreachable))
    }

    /// The text of the pack `id` in this folder.
    ///
    /// # Errors
    ///
    /// The errors of a pack file read from a disk ([`DirectoryPackSource`]),
    /// and [`SourceError::Unreadable`] for a folder that did not answer in
    /// time: the pack is there, the listing said so, and it cannot be had now.
    pub fn read(&self, id: &str) -> Result<String, SourceError> {
        // 🔴 Only an identifier becomes part of a path: the alphabet has no
        // separator and no dot, so a name cannot leave the folder.
        if !is_pack_id(id) {
            return Err(SourceError::NotFound);
        }
        let id = id.to_owned();
        self.read_with(Box::new(move |folder| {
            let source = DirectoryPackSource::new(folder);
            // Something that is there and is not a plain file - a named pipe
            // put in place of a pack after the listing - is refused before it
            // is opened: opening a pipe waits for a writer that may never come.
            match std::fs::metadata(source.path_of(&id)) {
                Ok(meta) if !meta.is_file() => Err(SourceError::Unreadable),
                _ => source.read(&id),
            }
        }))
    }

    fn read_with(
        &self,
        question: Question<Result<String, SourceError>>,
    ) -> Result<String, SourceError> {
        self.ask(question).unwrap_or(Err(SourceError::Unreadable))
    }

    /// Runs `question` on a helper thread and waits for it at most `wait`.
    ///
    /// `None` when the folder did not answer: this question ran out of time,
    /// or an earlier one is still hanging. The state of each question is
    /// settled by one atomic swap on each side, so a question that answers in
    /// the very moment its caller gives up is counted once or not at all.
    fn ask<T: Send + 'static>(&self, question: Question<T>) -> Option<T> {
        if self.hanging.load(Ordering::SeqCst) > 0 {
            return None;
        }
        let (answer, answered) = mpsc::channel();
        let state = Arc::new(AtomicU8::new(RUNNING));
        let thread_state = Arc::clone(&state);
        let hanging = Arc::clone(&self.hanging);
        let path = self.path.clone();
        let spawned = std::thread::Builder::new()
            .name(String::from("nkb-pack-folder"))
            .spawn(move || {
                let _ = answer.send(question(&path));
                if thread_state.swap(DONE, Ordering::SeqCst) == ABANDONED {
                    hanging.fetch_sub(1, Ordering::SeqCst);
                }
            });
        if spawned.is_err() {
            // No thread to be had: the folder cannot be asked safely, and it
            // is said the way a folder that did not answer is said.
            return None;
        }
        match answered.recv_timeout(self.wait) {
            Ok(value) => Some(value),
            Err(_) => {
                if state
                    .compare_exchange(RUNNING, ABANDONED, Ordering::SeqCst, Ordering::SeqCst)
                    .is_ok()
                {
                    self.hanging.fetch_add(1, Ordering::SeqCst);
                    None
                } else {
                    // Finished between the timeout and this line.
                    answered.try_recv().ok()
                }
            }
        }
    }
}

const RUNNING: u8 = 0;
const ABANDONED: u8 = 1;
const DONE: u8 = 2;

/// The listing itself, on the helper thread.
fn list_folder(folder: &Path) -> Result<FolderListing, SourceSkipped> {
    let metadata = std::fs::metadata(folder).map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => SourceSkipped::Missing,
        _ => SourceSkipped::Unreadable,
    })?;
    if !metadata.is_dir() {
        return Err(SourceSkipped::NotAFolder);
    }
    let resolved = std::fs::canonicalize(folder).map_err(|_| SourceSkipped::Unreadable)?;
    let entries = std::fs::read_dir(folder).map_err(|_| SourceSkipped::Unreadable)?;

    let mut packs = Vec::new();
    let mut misnamed = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|_| SourceSkipped::Unreadable)?;
        let name = entry.file_name();
        let Some(kind) = kind_of(&name) else {
            continue;
        };
        // A folder named `x.toml`, or a link to nothing, is not a pack file.
        // Followed through a link on purpose: a pack kept elsewhere and linked
        // in is a pack of this folder.
        if !std::fs::metadata(entry.path()).is_ok_and(|meta| meta.is_file()) {
            continue;
        }
        match kind {
            Kind::Pack(id) => packs.push(id),
            Kind::Misnamed(shown) => misnamed.push(shown),
        }
        if packs.len() + misnamed.len() > FOLDER_FILES_LIMIT {
            return Err(SourceSkipped::TooManyFiles);
        }
    }
    packs.sort();
    misnamed.sort();
    Ok(FolderListing {
        resolved,
        packs,
        misnamed,
    })
}

/// What a file name in a pack folder is.
enum Kind {
    Pack(String),
    Misnamed(String),
}

/// `None` for a name that is not a pack file at all, or one passed over in
/// silence: not `.toml`, hidden, or a translation.
fn kind_of(name: &OsStr) -> Option<Kind> {
    let shown = name.to_string_lossy();
    let stem = shown.strip_suffix(".toml")?;
    if stem.starts_with('.') {
        return None;
    }
    if is_pack_id(stem) {
        return Some(Kind::Pack(stem.to_owned()));
    }
    // A name that is not valid Unicode cannot be a pack identifier, and is
    // named back with its lost characters marked.
    let translation = stem
        .rsplit_once('.')
        .is_some_and(|(pack, language)| is_pack_id(pack) && is_language(language));
    if translation && name.to_str().is_some() {
        return None;
    }
    Some(Kind::Misnamed(shown.into_owned()))
}

/// A language tag as translation files carry it (`pack-format.md` 10): two or
/// three lower-case letters, optionally a region - `pl`, `pt-br`.
fn is_language(text: &str) -> bool {
    let mut parts = text.splitn(2, '-');
    let primary = parts.next().unwrap_or_default();
    let primary_ok =
        (2..=3).contains(&primary.len()) && primary.bytes().all(|byte| byte.is_ascii_lowercase());
    let region_ok = parts.next().is_none_or(|region| {
        (2..=8).contains(&region.len()) && region.bytes().all(|b| b.is_ascii_alphanumeric())
    });
    primary_ok && region_ok
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;
    use std::sync::Barrier;

    /// A folder of its own under the target directory, removed when dropped.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir()
                .join("nkb-pack-folder-tests")
                .join(format!("{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("a scratch folder");
            Self(path)
        }
        fn write(&self, name: &str, text: &str) {
            std::fs::write(self.0.join(name), text).expect("a scratch file");
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_folder_lists_its_packs_and_names_the_misnamed_ones() {
        let scratch = Scratch::new("lists");
        scratch.write("zeta.toml", "z");
        scratch.write("alpha.toml", "a");
        scratch.write("My Pack.toml", "m");
        scratch.write("Cargo.toml", "c");
        scratch.write("alpha.pl.toml", "translation, silent");
        scratch.write(".hidden.toml", "hidden, silent");
        scratch.write("README.md", "not a pack file at all");
        scratch.write("alpha.TOML", "upper case is not read, as in nkb lint");
        std::fs::create_dir_all(scratch.0.join("folder.toml")).expect("a folder named like a file");

        let listing = PackFolder::new(&scratch.0)
            .list()
            .expect("the folder lists");
        assert_eq!(listing.packs, vec!["alpha", "zeta"]);
        assert_eq!(listing.misnamed, vec!["Cargo.toml", "My Pack.toml"]);
    }

    #[test]
    fn a_folder_that_is_not_there_or_is_a_file_says_so() {
        let scratch = Scratch::new("absent");
        assert_eq!(
            PackFolder::new(scratch.0.join("nothing-here")).list(),
            Err(SourceSkipped::Missing)
        );
        scratch.write("file.toml", "x");
        assert_eq!(
            PackFolder::new(scratch.0.join("file.toml")).list(),
            Err(SourceSkipped::NotAFolder)
        );
    }

    #[test]
    fn a_folder_with_more_pack_files_than_the_limit_is_not_read() {
        let scratch = Scratch::new("crowded");
        for n in 0..FOLDER_FILES_LIMIT {
            scratch.write(&format!("p{n}.toml"), "x");
        }
        assert_eq!(
            PackFolder::new(&scratch.0)
                .list()
                .expect("exactly the limit is read")
                .packs
                .len(),
            FOLDER_FILES_LIMIT
        );
        scratch.write("one-more.toml", "x");
        assert_eq!(
            PackFolder::new(&scratch.0).list(),
            Err(SourceSkipped::TooManyFiles)
        );
    }

    #[test]
    fn a_pack_is_read_by_identifier_and_never_by_a_path() {
        // A file that IS there, one folder up: without the gate, `../outside`
        // would read it.
        let scratch = Scratch::new("reads");
        scratch.write("outside.toml", "a file beside the pack folder");
        let inner = scratch.0.join("inner");
        std::fs::create_dir_all(&inner).expect("the pack folder");
        std::fs::write(inner.join("alpha.toml"), "text of alpha").expect("a pack");
        let folder = PackFolder::new(&inner);
        assert_eq!(folder.read("alpha"), Ok("text of alpha".to_owned()));
        assert_eq!(folder.read("beta"), Err(SourceError::NotFound));
        for escape in ["../outside", "..", "a/b", "Alpha"] {
            assert_eq!(
                folder.read(escape),
                Err(SourceError::NotFound),
                "{escape} is not an identifier and must not become a path"
            );
        }
    }

    #[test]
    fn a_folder_that_does_not_answer_in_time_is_unreachable_and_asked_no_more_until_it_does() {
        let folder = PackFolder::with_wait("anywhere", Duration::from_millis(50));
        let release = Arc::new(Barrier::new(2));
        let held = Arc::clone(&release);
        let hung = folder.list_with(Box::new(move |_| {
            held.wait();
            Err(SourceSkipped::Missing)
        }));
        assert_eq!(hung, Err(SourceSkipped::Unreachable));

        // While the first question hangs, a second is not even started: the
        // stand-in below would answer at once and must not be asked.
        let asked = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&asked);
        let second = folder.read_with(Box::new(move |_| {
            counted.fetch_add(1, Ordering::SeqCst);
            Ok(String::from("answer"))
        }));
        assert_eq!(second, Err(SourceError::Unreadable));
        assert_eq!(asked.load(Ordering::SeqCst), 0);

        // The hanging question ends, and the folder is asked again.
        release.wait();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while folder.hanging.load(Ordering::SeqCst) > 0 {
            assert!(
                std::time::Instant::now() < deadline,
                "the finished question never cleared its mark"
            );
            std::thread::yield_now();
        }
        let third = folder.read_with(Box::new(|_| Ok(String::from("answer"))));
        assert_eq!(third, Ok(String::from("answer")));
    }

    #[test]
    fn translation_names_are_told_from_misnamed_ones() {
        assert!(kind_of(OsStr::new("alpha.pl.toml")).is_none());
        assert!(kind_of(OsStr::new("alpha.pt-br.toml")).is_none());
        assert!(matches!(
            kind_of(OsStr::new("alpha.Polish.toml")),
            Some(Kind::Misnamed(_))
        ));
        assert!(matches!(
            kind_of(OsStr::new("Alpha.pl.toml")),
            Some(Kind::Misnamed(_))
        ));
        assert!(kind_of(OsStr::new("alpha.json")).is_none());
    }
}
