//! Reading pack files off a disk.
//!
//! Everything this module knows about is paths and bytes. It knows nothing about
//! what a pack means, which is why the layer above can be exercised with no file
//! system at all.
//!
//! # Untrusted input
//!
//! A pack file is a file from a stranger. It is read as bytes and turned into
//! text, and no part of that turns it into an instruction: the format has no
//! includes, no addresses and nothing executable, so reading a pack can do
//! nothing but add values to a list.

use nkb_app::{Clock, PACK_FILE_LIMIT, PackSink, PackSource, SinkError, SourceError};
use nkb_core::calendar::Day;

use crate::atomic_write::{write_atomically, write_target};
use std::ffi::OsStr;
use std::io::Read;
use std::path::{Path, PathBuf};

/// Why a path a person typed does not name a pack file this tool reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotAPackFile {
    /// No file name at all: a bare directory, or a path ending in a parent
    /// marker.
    NoFileName,
    /// A file name that does not end in `.toml`. Until R04 such a path was read
    /// as `<stem>.toml`, which is ANOTHER file: `nkb fmt notes.txt` rewrote
    /// `notes.toml` beside it, and `nkb lint notes` judged a file nobody named.
    NotToml,
}

/// Reads packs from one directory, addressing them by their pack identifier.
///
/// The identifier is the file name without its extension, which is the same
/// thing the format requires the pack to declare - so the mapping needs no index
/// and no configuration.
#[derive(Debug, Clone)]
pub struct DirectoryPackSource {
    directory: PathBuf,
}

impl DirectoryPackSource {
    #[must_use]
    pub fn new(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
        }
    }

    /// Splits a path a person typed into the directory to read from and the
    /// identifier to read, so that `nkb lint packs/whitespace.toml` works without
    /// the layer above ever learning what a path is.
    ///
    /// 🔴 Only a name ending in `.toml`, lower case on every system, so that the
    /// file [`Self::path_of`] reads back is the very file named (R04). A pack's
    /// file name is its identifier plus `.toml` (`pack-format.md` 7), and a
    /// `.TOML` that Windows would open is refused there too, so one command
    /// means one thing on all three systems.
    ///
    /// # Errors
    ///
    /// [`NotAPackFile`] when the path has no file name or the name does not end
    /// in `.toml`.
    pub fn split(path: &Path) -> Result<(Self, String), NotAPackFile> {
        if path.file_name().is_none() {
            return Err(NotAPackFile::NoFileName);
        }
        if path.extension() != Some(OsStr::new("toml")) {
            return Err(NotAPackFile::NotToml);
        }
        let stem = path
            .file_stem()
            .and_then(OsStr::to_str)
            .ok_or(NotAPackFile::NoFileName)?
            .to_owned();
        let directory = path
            .parent()
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
        let directory = if directory.as_os_str().is_empty() {
            PathBuf::from(".")
        } else {
            directory
        };
        Ok((Self::new(directory), stem))
    }

    #[must_use]
    pub fn path_of(&self, id: &str) -> PathBuf {
        self.directory.join(format!("{id}.toml"))
    }

    /// The folder this reads from.
    ///
    /// Exists so a command that both reads and writes one file can put its sink
    /// in the same place, without splitting the path a second time and risking
    /// two answers to one question.
    #[must_use]
    pub fn folder(&self) -> &Path {
        &self.directory
    }
}

impl PackSource for DirectoryPackSource {
    fn read(&self, id: &str) -> Result<String, SourceError> {
        let path = self.path_of(id);
        let file = std::fs::File::open(&path).map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => SourceError::NotFound,
            _ => SourceError::Unreadable,
        })?;
        // At most one byte past the limit (R06): enough to tell "too large" from
        // "exactly at the limit" without holding more of a stranger's file than
        // the limit allows.
        let mut bytes = Vec::new();
        file.take(PACK_FILE_LIMIT + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| SourceError::Unreadable)?;
        let within = u64::try_from(bytes.len()).is_ok_and(|read| read <= PACK_FILE_LIMIT);
        if !within {
            return Err(SourceError::TooLarge);
        }

        // Read as bytes and converted here rather than read as text, so that a
        // file which is not valid UTF-8 comes back as its own answer instead of
        // as a generic read failure. The format has a rule about exactly that,
        // and it cannot fire on an error that lost the distinction.
        String::from_utf8(bytes).map_err(|_| SourceError::NotUtf8)
    }
}

/// Writes new pack files into one directory.
///
/// Separate from [`DirectoryPackSource`] rather than a second method on it,
/// because the two carry opposite risks and it should take a deliberate act to
/// hand a component the ability to write.
#[derive(Debug, Clone)]
pub struct DirectoryPackSink {
    directory: PathBuf,
}

impl DirectoryPackSink {
    #[must_use]
    pub fn new(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
        }
    }
}

impl PackSink for DirectoryPackSink {
    /// # Why this is one call and not a check followed by a write
    ///
    /// 🔴 `create_new` asks the operating system to make the file **only if it is
    /// not there**, and to say so otherwise - one indivisible step. Asking
    /// whether the file exists and then writing it is two steps with a gap, and
    /// something can arrive in that gap: the shape of race the architecture notes
    /// call TOCTOU. Here what would arrive in the gap is somebody's work, and
    /// what would happen to it is deletion.
    fn create(&self, id: &str, text: &str) -> Result<(), SinkError> {
        let path = self.directory.join(format!("{id}.toml"));
        let mut file = match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                return Err(SinkError::AlreadyExists);
            }
            Err(_) => return Err(SinkError::Unwritable),
        };
        std::io::Write::write_all(&mut file, text.as_bytes()).map_err(|_| SinkError::Unwritable)
    }

    /// # Why the content goes somewhere else first
    ///
    /// 🔴 A plain write truncates the file and then fills it. Interrupted in
    /// between - a full disk, a killed process, a machine losing power - it
    /// leaves a pack that is half a pack and still looks like one, and the file
    /// it destroyed was the contributor's only copy.
    ///
    /// So the text goes through `atomic_write`, the one writer the settings
    /// file uses too: either the old content is there or the new content is,
    /// a read-only file is left alone on every system, the permission bits of
    /// the old file are kept on Linux and macOS, and a pack file that is a link
    /// is written through it. Until R05 this had its own copy with one fixed
    /// temporary name, and a write that found that name taken removed the file
    /// under it, which belonged to somebody else.
    fn replace(&self, id: &str, text: &str) -> Result<(), SinkError> {
        let path = self.directory.join(format!("{id}.toml"));
        let target = write_target(&path).map_err(|_| SinkError::Unwritable)?;
        write_atomically(&target, text).map_err(|_| SinkError::Unwritable)
    }
}

/// Today's date in UTC, from the machine this is running on.
#[derive(Debug, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    /// Falls back to 1970-01-01 if the machine reports a time before it. That is
    /// a clock nobody can trust, and a skeleton with a wrong date is a smaller
    /// problem than one that refuses to be written - the person editing it will
    /// change the date anyway. A `next-date` value worked out from it is still a
    /// real date with a four digit year (`nkb_core::next_date`).
    fn today(&self) -> Day {
        let seconds = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_secs());
        // Whole days, which is all a date needs. Leap seconds do not exist in
        // Unix time, so this division is exact rather than approximate. The
        // calendar arithmetic lives in the core, with its tests.
        Day::from_days_since_1970(i64::try_from(seconds / 86_400).unwrap_or(0))
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;

    #[test]
    fn a_new_pack_is_written_once_and_never_over_something_already_there() {
        // 🔴 The refusal that keeps somebody's hour of work. Written as a real
        // file rather than against a double, because what is being trusted here is
        // the operating system's behaviour and not ours.
        let dir = std::env::temp_dir().join(format!("nkb-sink-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a temporary directory");
        let sink = DirectoryPackSink::new(&dir);

        assert_eq!(sink.create("locale-cz", "first"), Ok(()));
        let written = |dir: &std::path::Path| {
            std::fs::read_to_string(dir.join("locale-cz.toml")).expect("the file is there")
        };
        assert_eq!(written(&dir), "first");

        assert_eq!(
            sink.create("locale-cz", "second"),
            Err(SinkError::AlreadyExists)
        );
        assert_eq!(
            written(&dir),
            "first",
            "the refused write must leave the file exactly as it was"
        );

        std::fs::remove_file(dir.join("locale-cz.toml")).ok();
        std::fs::remove_dir(&dir).ok();
    }

    #[test]
    fn a_folder_that_is_not_there_is_a_place_that_cannot_be_written_to() {
        // Told apart from a name already taken, because the repairs differ: make
        // the folder, or choose another name.
        let sink = DirectoryPackSink::new("no-such-directory-here");
        assert_eq!(sink.create("locale-cz", "x"), Err(SinkError::Unwritable));
    }

    #[test]
    fn a_path_splits_into_a_directory_and_an_identifier() {
        let (source, id) = DirectoryPackSource::split(Path::new("packs/whitespace.toml"))
            .expect("a path with a file name splits");
        assert_eq!(id, "whitespace");
        assert_eq!(
            source.path_of(&id),
            PathBuf::from("packs").join("whitespace.toml")
        );
    }

    #[test]
    fn a_bare_file_name_reads_from_the_current_directory() {
        // The shape a contributor actually types, standing in the pack folder.
        let (source, id) =
            DirectoryPackSource::split(Path::new("whitespace.toml")).expect("splits");
        assert_eq!(id, "whitespace");
        assert_eq!(
            source.path_of(&id),
            PathBuf::from(".").join("whitespace.toml")
        );
    }

    #[test]
    fn a_translation_file_keeps_its_language_in_the_identifier() {
        // `unicode-text.pl.toml` is one file whose stem is `unicode-text.pl`.
        // Losing the language half here would make the tool read the source pack
        // when asked for its translation.
        let (_, id) =
            DirectoryPackSource::split(Path::new("unicode-text.pl.toml")).expect("splits");
        assert_eq!(id, "unicode-text.pl");
    }

    #[test]
    fn a_path_without_a_file_name_does_not_pretend_to_be_a_pack() {
        assert_eq!(
            DirectoryPackSource::split(Path::new("..")).err(),
            Some(NotAPackFile::NoFileName)
        );
    }

    #[test]
    fn only_a_name_ending_in_toml_is_a_pack_file() {
        // R04: each of these was read as `<stem>.toml`, a file nobody named, and
        // `nkb fmt` wrote that file. A pack is never in any of them.
        for named in [
            "notes.txt",
            "packs/whitespace",
            "whitespace.TOML",
            "whitespace.toml.bak",
            ".toml",
        ] {
            assert_eq!(
                DirectoryPackSource::split(Path::new(named)).err(),
                Some(NotAPackFile::NotToml),
                "{named}"
            );
        }
    }

    /// A file of `size` bytes of `a` in a folder of its own, read through the
    /// source. The folder goes when the test ends.
    fn read_back(name: &str, size: u64) -> Result<String, SourceError> {
        let dir = std::env::temp_dir().join(format!("nkb-limit-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a temporary directory");
        let path = dir.join(format!("{name}.toml"));
        let file = std::fs::File::create(&path).expect("a file");
        std::io::copy(&mut std::io::repeat(b'a').take(size), &mut &file).expect("written");
        drop(file);
        let read = DirectoryPackSource::new(&dir).read(name);
        std::fs::remove_dir_all(&dir).ok();
        read
    }

    #[test]
    fn a_pack_file_larger_than_the_limit_is_refused_and_one_at_the_limit_is_read() {
        // R06: one byte decides, read as bytes on a real disk.
        let at = read_back("at-limit", PACK_FILE_LIMIT).expect("read whole");
        assert_eq!(at.len(), usize::try_from(PACK_FILE_LIMIT).expect("fits"));
        assert_eq!(
            read_back("past-limit", PACK_FILE_LIMIT + 1),
            Err(SourceError::TooLarge)
        );
    }

    #[test]
    fn a_missing_file_is_reported_as_missing_rather_than_as_unreadable() {
        // The two mean different things to whoever runs this: one is a typo in a
        // name, the other is a permission or a device. Collapsing them would send
        // the reader to fix the wrong thing.
        let source = DirectoryPackSource::new("no-such-directory-here");
        assert_eq!(source.read("absent"), Err(SourceError::NotFound));
    }

    /// A folder of its own holding `locale-cz.toml` with `old`, removed before
    /// and after.
    struct PackFolder(PathBuf);

    impl PackFolder {
        fn new(name: &str) -> Self {
            let dir =
                std::env::temp_dir().join(format!("nkb-replace-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("a temporary directory");
            std::fs::write(dir.join("locale-cz.toml"), "old").expect("the pack file");
            Self(dir)
        }
        fn pack(&self) -> PathBuf {
            self.0.join("locale-cz.toml")
        }
        fn text(&self) -> String {
            std::fs::read_to_string(self.pack()).expect("the pack file")
        }
    }

    impl Drop for PackFolder {
        #[allow(
            clippy::permissions_set_readonly_false,
            reason = "a scratch file made read-only by a test has to go"
        )]
        fn drop(&mut self) {
            if let Ok(meta) = std::fs::metadata(self.pack()) {
                let mut permissions = meta.permissions();
                permissions.set_readonly(false);
                let _ = std::fs::set_permissions(self.pack(), permissions);
            }
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_replace_leaves_a_file_under_the_old_temporary_name_alone() {
        // R05: the name the old writer always used. It found the name taken,
        // failed, and removed the file under it, which was not its own.
        let folder = PackFolder::new("old-temporary");
        let foreign = folder.0.join("locale-cz.toml.nkb-new");
        std::fs::write(&foreign, "not ours").expect("a foreign file");

        assert_eq!(
            DirectoryPackSink::new(&folder.0).replace("locale-cz", "new"),
            Ok(())
        );

        assert_eq!(folder.text(), "new");
        assert_eq!(
            std::fs::read_to_string(&foreign).expect("still there"),
            "not ours"
        );
    }

    #[test]
    fn a_read_only_pack_file_is_not_rewritten_on_any_system() {
        // On Linux and macOS a rename over a read-only file goes through, so
        // until R26 `nkb fmt` rewrote it there and refused it only on Windows.
        let folder = PackFolder::new("read-only");
        let mut permissions = std::fs::metadata(folder.pack())
            .expect("the pack file")
            .permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(folder.pack(), permissions).expect("read-only");

        assert_eq!(
            DirectoryPackSink::new(&folder.0).replace("locale-cz", "new"),
            Err(SinkError::Unwritable)
        );
        assert_eq!(folder.text(), "old");
    }

    #[test]
    fn a_replace_in_a_folder_that_is_not_there_makes_no_folder() {
        let folder = PackFolder::new("no-folder");
        let missing = folder.0.join("missing");

        assert_eq!(
            DirectoryPackSink::new(&missing).replace("locale-cz", "new"),
            Err(SinkError::Unwritable)
        );
        assert!(!missing.exists());
    }

    #[cfg(unix)]
    #[test]
    fn a_pack_file_that_is_a_link_is_written_through_it_and_keeps_its_bits() {
        // The link stays a link and the file it points to gets the new text,
        // as a formatter writing in place would leave it. Before R26 the link
        // was replaced by a plain file and the real one was never formatted.
        use std::os::unix::fs::PermissionsExt;
        let folder = PackFolder::new("link");
        let real = folder.0.join("real.toml");
        std::fs::write(&real, "old").expect("the real file");
        std::fs::set_permissions(&real, std::fs::Permissions::from_mode(0o640)).expect("bits");
        std::fs::remove_file(folder.pack()).expect("room for the link");
        std::os::unix::fs::symlink(&real, folder.pack()).expect("a link");

        assert_eq!(
            DirectoryPackSink::new(&folder.0).replace("locale-cz", "new"),
            Ok(())
        );

        assert!(
            std::fs::symlink_metadata(folder.pack())
                .expect("there")
                .file_type()
                .is_symlink(),
            "the link was replaced by a plain file"
        );
        assert_eq!(
            std::fs::read_to_string(&real).expect("the real file"),
            "new"
        );
        assert_eq!(
            std::fs::metadata(&real)
                .expect("there")
                .permissions()
                .mode()
                & 0o777,
            0o640
        );
    }
}
