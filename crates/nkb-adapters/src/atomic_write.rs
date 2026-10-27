//! Replacing a file so that it holds either its old text or its new text, and
//! never half of either.
//!
//! Two writers share it: a save of the settings file (`settings_file.rs`) and
//! `nkb fmt` rewriting a pack (`DirectoryPackSink::replace`). Until R05 each had
//! its own copy, and the pack's copy used one fixed temporary name and removed
//! whatever stood under it when the write failed, its own file or not.
//!
//! # The steps
//!
//! 1. A link at the path is followed by [`write_target`], so the file it points
//!    to is the one replaced. A rename over the link would put a plain file in
//!    its place and quietly cut the real one out of the loop.
//! 2. A read-only file is refused. 🔴 Asked here rather than left to the rename,
//!    and measured on three systems: on Windows a rename over a read-only file
//!    fails, on Linux and macOS it SUCCEEDS, because the folder's permission
//!    decides there, not the file's. A read-only file is somebody's lock.
//! 3. The text goes under a neighbouring name nobody holds (`create_new`, the
//!    process number and a serial), is flushed, and takes the real name by a
//!    rename, which the system does in one step. The temporary file sits in the
//!    same folder, because a rename across folders is a copy, and a copy is not
//!    atomic.
//! 4. When a step fails, the temporary file this write made is removed, and
//!    nothing else is. A name that was taken was never opened (see
//!    [`create_temporary`]).
//!
//! # Permissions
//!
//! On Linux and macOS the temporary file is made readable and writable by its
//! owner only before a byte is in it, and takes the permission bits of the file
//! it replaces before the text goes in (R26). A file that was not there stays
//! private, which is what a new settings file is meant to be. Owner, group,
//! access control lists and extended attributes are NOT carried over: the new
//! file belongs to whoever ran the program, as with any editor that saves by a
//! rename.
//!
//! On Windows the replaced file's own access control entries are not carried
//! over either, and neither is the hidden attribute. Measured 2026-10-27 on
//! Windows 11 with `std::fs::rename`: an entry added to the file was gone
//! afterwards, and a file with inheritance turned off came out with the
//! folder's entries, wider than it had. A file whose entries all come from its
//! folder, which is every file this program makes and the usual case, comes
//! out as it was. `ReplaceFileW` would carry them, but it is not one step:
//! with no backup name, its failure `ERROR_UNABLE_TO_MOVE_REPLACEMENT` leaves
//! the replaced file gone.

use std::ffi::OsStr;
use std::fs::{File, Metadata, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// The file a write has to replace: the path itself, or what a link at the
/// path points to.
///
/// # Errors
///
/// The file a link points to could not be found.
pub(crate) fn write_target(path: &Path) -> std::io::Result<PathBuf> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => std::fs::canonicalize(path),
        _ => Ok(path.to_path_buf()),
    }
}

/// How many temporary names one write tries before it gives up.
const TEMPORARY_ATTEMPTS: u64 = 16;

/// The next serial number for a temporary name, shared by every write in this
/// process. Two writes running at once start from different names.
static NEXT_TEMPORARY: AtomicU64 = AtomicU64::new(0);

/// The neighbouring name a write uses first: the real name, the process number
/// and a serial.
fn temporary_path(folder: &Path, name: &OsStr, serial: u64) -> PathBuf {
    let mut temporary_name = name.to_os_string();
    temporary_name.push(format!(".nkb-new-{}-{serial}", std::process::id()));
    folder.join(temporary_name)
}

/// Creates the file a write uses first, under a name nobody holds.
///
/// `create_new` asks the operating system for a file that is NOT there yet, in
/// one step. A name that is taken is skipped and never opened: the leftover of
/// a write that was cut short, or a link somebody put there, is neither written
/// through nor removed, because it is not ours. The next serial is tried
/// instead, and after `TEMPORARY_ATTEMPTS` names the write fails and says so.
fn create_temporary(folder: &Path, name: &OsStr, first: u64) -> std::io::Result<(File, PathBuf)> {
    for step in 0..TEMPORARY_ATTEMPTS {
        let temporary = temporary_path(folder, name, first.wrapping_add(step));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        private(&mut options);
        match options.open(&temporary) {
            Ok(file) => return Ok((file, temporary)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
    Err(std::io::ErrorKind::AlreadyExists.into())
}

/// Readable and writable by the owner only, where the system has those bits.
/// The process's umask can take bits away and never adds any.
#[cfg(unix)]
fn private(options: &mut OpenOptions) {
    use std::os::unix::fs::OpenOptionsExt;
    options.mode(0o600);
}

#[cfg(not(unix))]
fn private(_: &mut OpenOptions) {}

/// Writes `text` to `target` so that either the old content or the new one is
/// there afterwards, and nothing in between.
///
/// `target` is the file itself: a path that may be a link goes through
/// [`write_target`] first. Its folder has to be there already, because which
/// folders may be made is the caller's to decide.
///
/// # Errors
///
/// `PermissionDenied` for a read-only file, and whatever the system answered
/// for a step that failed. Either way the file at `target` is as it was.
pub(crate) fn write_atomically(target: &Path, text: &str) -> std::io::Result<()> {
    let folder = target.parent().ok_or(std::io::ErrorKind::InvalidInput)?;
    let name = target.file_name().ok_or(std::io::ErrorKind::InvalidInput)?;
    let old = std::fs::metadata(target).ok();
    if old
        .as_ref()
        .is_some_and(|meta| meta.permissions().readonly())
    {
        return Err(std::io::ErrorKind::PermissionDenied.into());
    }
    let serial = NEXT_TEMPORARY.fetch_add(1, Ordering::Relaxed);
    // Before the closure, so that only a file this write created is ever
    // removed below. A write that could not create one has nothing to clean.
    let (mut file, temporary) = create_temporary(folder, name, serial)?;

    let written = (|| -> std::io::Result<()> {
        if let Some(old) = &old {
            keep_permissions(&file, old)?;
        }
        std::io::Write::write_all(&mut file, text.as_bytes())?;
        // Before the rename: a rename that publishes content the system has
        // not committed is a rename that publishes an empty file on the next
        // power cut.
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temporary, target)
    })();
    if written.is_err() {
        // Nothing half written is left under a name a person might open.
        let _ = std::fs::remove_file(&temporary);
    }
    written
}

/// Gives the new file the permissions of the one it replaces, while it is
/// still empty. On Windows the only bit is read-only, and a read-only file was
/// refused before this, so there it changes nothing.
fn keep_permissions(file: &File, old: &Metadata) -> std::io::Result<()> {
    file.set_permissions(old.permissions())
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;

    /// A folder of its own for one test, removed before and after.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir()
                .join(format!("nkb-atomic-write-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("a scratch folder");
            Self(path)
        }
        fn names(&self) -> Vec<String> {
            let mut names: Vec<String> = std::fs::read_dir(&self.0)
                .expect("the folder")
                .filter_map(Result::ok)
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect();
            names.sort();
            names
        }
    }

    impl Drop for Scratch {
        #[allow(
            clippy::permissions_set_readonly_false,
            reason = "a scratch file made read-only by a test has to go"
        )]
        fn drop(&mut self) {
            if let Ok(entries) = std::fs::read_dir(&self.0) {
                for entry in entries.filter_map(Result::ok) {
                    if let Ok(meta) = entry.metadata() {
                        let mut permissions = meta.permissions();
                        permissions.set_readonly(false);
                        let _ = std::fs::set_permissions(entry.path(), permissions);
                    }
                }
            }
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    const NAME: &str = "settings.toml";

    #[test]
    fn the_new_text_takes_the_place_of_the_old_and_nothing_else_is_left() {
        let scratch = Scratch::new("replace");
        let target = scratch.0.join(NAME);
        std::fs::write(&target, "old").expect("the old file");

        write_atomically(&target, "new").expect("a writable file");

        assert_eq!(std::fs::read_to_string(&target).expect("there"), "new");
        assert_eq!(scratch.names(), vec![NAME.to_owned()]);
    }

    #[test]
    fn a_read_only_file_is_refused_and_left_as_it_was() {
        // The tester's lock holds on every system, not only where the rename
        // happens to refuse.
        let scratch = Scratch::new("read-only");
        let target = scratch.0.join(NAME);
        std::fs::write(&target, "old").expect("the old file");
        let mut permissions = std::fs::metadata(&target).expect("there").permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&target, permissions).expect("read-only");

        let error = write_atomically(&target, "new").expect_err("a read-only file");

        assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
        assert_eq!(std::fs::read_to_string(&target).expect("there"), "old");
        assert_eq!(scratch.names(), vec![NAME.to_owned()]);
    }

    #[test]
    fn a_failed_write_removes_its_own_temporary_file_and_nothing_else() {
        // A folder where the file should be: the rename fails after the
        // temporary file was written. A file somebody left under a temporary
        // name of another process stays.
        let scratch = Scratch::new("failed");
        let target = scratch.0.join(NAME);
        std::fs::create_dir_all(&target).expect("a folder in the way");
        let foreign = scratch.0.join(format!("{NAME}.nkb-new-1-0"));
        std::fs::write(&foreign, "not ours").expect("a foreign file");

        write_atomically(&target, "new").expect_err("a folder cannot be replaced");

        assert_eq!(
            scratch.names(),
            vec![NAME.to_owned(), format!("{NAME}.nkb-new-1-0")]
        );
        assert_eq!(
            std::fs::read_to_string(&foreign).expect("there"),
            "not ours"
        );
    }

    #[test]
    fn a_temporary_name_that_is_taken_is_skipped_and_left_as_it_was() {
        let scratch = Scratch::new("taken-name");
        let name = OsStr::new(NAME);
        let planted = temporary_path(&scratch.0, name, 7);
        std::fs::write(&planted, "someone else's file").expect("a planted file");

        let (file, made) = create_temporary(&scratch.0, name, 7).expect("a free name is found");
        drop(file);

        assert_eq!(made, temporary_path(&scratch.0, name, 8));
        assert_eq!(
            std::fs::read_to_string(&planted).expect("still there"),
            "someone else's file",
            "a file that was not ours was opened"
        );
    }

    #[test]
    fn when_every_temporary_name_is_taken_nothing_is_written_and_nothing_is_removed() {
        let scratch = Scratch::new("all-taken");
        let name = OsStr::new(NAME);
        for serial in 0..TEMPORARY_ATTEMPTS {
            std::fs::write(temporary_path(&scratch.0, name, serial), "planted").expect("a plant");
        }

        let error = create_temporary(&scratch.0, name, 0).expect_err("no name is free");

        assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
        assert_eq!(
            scratch.names().len(),
            usize::try_from(TEMPORARY_ATTEMPTS).unwrap_or(0)
        );
        for serial in 0..TEMPORARY_ATTEMPTS {
            assert_eq!(
                std::fs::read_to_string(temporary_path(&scratch.0, name, serial)).expect("a plant"),
                "planted"
            );
        }
    }

    #[test]
    fn two_writes_in_one_process_start_from_different_temporary_names() {
        let first = NEXT_TEMPORARY.fetch_add(1, Ordering::Relaxed);
        let second = NEXT_TEMPORARY.fetch_add(1, Ordering::Relaxed);
        assert_ne!(first, second);
        let folder = Path::new("folder");
        let name = OsStr::new(NAME);
        assert_ne!(
            temporary_path(folder, name, first),
            temporary_path(folder, name, second)
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_link_under_the_temporary_name_is_not_written_through() {
        let scratch = Scratch::new("planted-link");
        let name = OsStr::new(NAME);
        let victim = scratch.0.join("victim.txt");
        std::fs::write(&victim, "keep me").expect("the victim");
        let planted = temporary_path(&scratch.0, name, 3);
        std::os::unix::fs::symlink(&victim, &planted).expect("a planted link");

        let (mut file, made) = create_temporary(&scratch.0, name, 3).expect("a free name is found");
        std::io::Write::write_all(&mut file, b"new content").expect("our own file");
        drop(file);

        assert_eq!(made, temporary_path(&scratch.0, name, 4));
        assert_eq!(
            std::fs::read_to_string(&victim).expect("the victim"),
            "keep me",
            "the write went through a link it did not make"
        );
        assert!(
            std::fs::symlink_metadata(&planted)
                .expect("still there")
                .file_type()
                .is_symlink(),
            "the planted link was removed"
        );
    }

    #[cfg(unix)]
    fn mode(path: &Path) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path).expect("there").permissions().mode() & 0o777
    }

    #[cfg(unix)]
    #[test]
    fn the_new_file_has_exactly_the_permission_bits_of_the_one_it_replaces() {
        // 0o664 is the case a umask would spoil: the group's write bit is set
        // by changing the open file, which the umask does not touch.
        use std::os::unix::fs::PermissionsExt;
        for bits in [0o600, 0o640, 0o644, 0o664] {
            let scratch = Scratch::new(&format!("mode-{bits:o}"));
            let target = scratch.0.join(NAME);
            std::fs::write(&target, "old").expect("the old file");
            std::fs::set_permissions(&target, std::fs::Permissions::from_mode(bits))
                .expect("the bits");

            write_atomically(&target, "new").expect("a writable file");

            assert_eq!(mode(&target), bits, "{bits:o}");
            assert_eq!(std::fs::read_to_string(&target).expect("there"), "new");
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_file_that_was_not_there_is_made_readable_by_its_owner_only() {
        let scratch = Scratch::new("new-file");
        let target = scratch.0.join(NAME);

        write_atomically(&target, "new").expect("a writable folder");

        assert_eq!(mode(&target), 0o600);
    }
}
