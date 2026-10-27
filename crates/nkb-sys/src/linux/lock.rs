//! One send at a time on a display, and the journal of the spare keys it rewrote (`D157`).
//! Why, and what the journal says, is in `linux_remap_rules.rs`.
//!
//! The lock is `flock` on a file in the runtime folder of the user (`File::try_lock`, as the
//! claim of the shortcuts, `D144`). The system lets go of it when the process ends, however
//! it ends, so a send that was killed leaves nothing locked. The file stays, and holds the
//! journal.
//!
//! A send that cannot ask - no runtime folder, a file that cannot be opened - types as every
//! send did before the lock: a lock that cannot be asked about must not stop the typing. A
//! line of the journal that cannot be written leaves its key as a send killed before the
//! journal existed left it, and the typing goes on.

use core::sync::atomic::{AtomicBool, Ordering};
use std::fs::{File, OpenOptions, TryLockError};
use std::io::{Read as _, Seek as _, SeekFrom, Write as _};
use std::os::unix::fs::{FileExt as _, OpenOptionsExt as _};
use std::path::Path;
use std::time::Instant;

use crate::linux_remap_rules::{
    JOURNAL_LIMIT, LOCK_WAIT, Rewrite, journal_entries, journal_line, journal_of, lock_file_name,
    runtime_folder,
};
use crate::linux_rules::LOOK_EVERY;

/// The lock of one display, held until it is dropped.
pub(super) struct DisplayLock {
    file: File,
}

/// Why the lock was not had.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NotTaken {
    /// Another send held it for all of `LOCK_WAIT`.
    Busy,
    /// The caller asked the send to stop while it waited (R03).
    Halted,
}

impl DisplayLock {
    /// The lock of display `number`, waited for for at most `LOCK_WAIT`, or `None` when
    /// there is no lock to ask about.
    pub(super) fn take(number: u32, halt: &AtomicBool) -> Result<Option<Self>, NotTaken> {
        let runtime = std::env::var("XDG_RUNTIME_DIR").ok();
        let Some(folder) = runtime_folder(runtime.as_deref()) else {
            return Ok(None);
        };
        Self::take_in(Path::new(folder), number, halt)
    }

    /// The lock of display `number` in `folder`, as [`DisplayLock::take`] asks for it.
    fn take_in(folder: &Path, number: u32, halt: &AtomicBool) -> Result<Option<Self>, NotTaken> {
        let opened = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(folder.join(lock_file_name(number)));
        let Ok(file) = opened else {
            return Ok(None);
        };
        let deadline = Instant::now() + LOCK_WAIT;
        loop {
            match file.try_lock() {
                Ok(()) => return Ok(Some(Self { file })),
                Err(TryLockError::WouldBlock) => {}
                Err(TryLockError::Error(_)) => return Ok(None),
            }
            if halt.load(Ordering::Relaxed) {
                return Err(NotTaken::Halted);
            }
            if Instant::now() >= deadline {
                return Err(NotTaken::Busy);
            }
            std::thread::sleep(LOOK_EVERY);
        }
    }

    /// The keys the journal names. Nothing, when it cannot be read.
    pub(super) fn journal(&mut self) -> Vec<Rewrite> {
        let mut bytes = Vec::new();
        let read = self
            .file
            .seek(SeekFrom::Start(0))
            .and_then(|_| (&self.file).take(JOURNAL_LIMIT).read_to_end(&mut bytes));
        if read.is_err() {
            return Vec::new();
        }
        journal_entries(&bytes)
    }

    /// Adds `rewrite` to the journal. Called before the key is rewritten, so a send that
    /// stops in between leaves a line for a key that carries nothing, which is passed over.
    pub(super) fn note(&mut self, rewrite: Rewrite) {
        let line = journal_line(rewrite);
        let _ = self
            .file
            .seek(SeekFrom::End(0))
            .and_then(|_| self.file.write_all(line.as_bytes()));
    }

    /// Makes the journal `rewrites` and nothing else, once the server has carried them all
    /// out: the lines of keys rewritten again are no longer needed. Written over from the
    /// start and then cut, so a stop in between leaves the new lines and some old ones, never
    /// fewer than the keys carry.
    pub(super) fn settle(&mut self, rewrites: &[Rewrite]) {
        let journal = journal_of(rewrites);
        if self.file.write_all_at(journal.as_bytes(), 0).is_ok() {
            let _ = self.file.set_len(journal.len() as u64);
        }
    }

    /// Empties the journal, once the server has given back every key it named.
    pub(super) fn clear(&mut self) {
        let _ = self.file.set_len(0);
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use core::time::Duration;
    use std::path::PathBuf;

    use super::*;

    /// A folder of its own for one test, standing in for the runtime folder.
    struct Folder(PathBuf);

    impl Folder {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!("nkb-lock-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("a folder for the test");
            Self(path)
        }

        fn take(&self, number: u32, halt: &AtomicBool) -> Result<Option<DisplayLock>, NotTaken> {
            DisplayLock::take_in(&self.0, number, halt)
        }
    }

    impl Drop for Folder {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn held(folder: &Folder, number: u32) -> DisplayLock {
        match folder.take(number, &AtomicBool::new(false)) {
            Ok(Some(lock)) => lock,
            other => panic!(
                "the lock was not had: {:?}",
                other.map(|lock| lock.is_some())
            ),
        }
    }

    #[test]
    fn a_second_send_on_the_display_waits_and_is_then_told_it_is_busy() {
        let folder = Folder::new("busy");
        let first = held(&folder, 97);
        let started = Instant::now();
        let second = folder.take(97, &AtomicBool::new(false));
        let waited = started.elapsed();
        assert!(matches!(second, Err(NotTaken::Busy)));
        assert!(waited >= LOCK_WAIT, "{waited:?}");
        assert!(waited < LOCK_WAIT + Duration::from_secs(1), "{waited:?}");
        // Another display is another map, and nobody holds it.
        let other = held(&folder, 98);
        drop((first, other));
        // Let go, so the next send has it at once.
        let started = Instant::now();
        let _again = held(&folder, 97);
        assert!(started.elapsed() < Duration::from_millis(500));
    }

    #[test]
    fn a_halt_ends_the_wait_for_the_lock_at_once() {
        let folder = Folder::new("halt");
        let _first = held(&folder, 97);
        let started = Instant::now();
        let second = folder.take(97, &AtomicBool::new(true));
        assert!(matches!(second, Err(NotTaken::Halted)));
        assert!(started.elapsed() < Duration::from_millis(500));
    }

    #[test]
    fn the_journal_outlives_a_send_that_did_not_clear_it() {
        let folder = Folder::new("journal");
        let one = Rewrite {
            keycode: 8,
            keysym: 0x0100_0105,
        };
        let two = Rewrite {
            keycode: 9,
            keysym: 0x0100_4E2D,
        };
        let mut lock = held(&folder, 97);
        assert_eq!(lock.journal(), Vec::new(), "a new file names nothing");
        lock.note(one);
        lock.note(two);
        lock.note(Rewrite {
            keycode: 8,
            keysym: 0x0100_00E9,
        });
        lock.settle(&[one, two]);
        assert_eq!(lock.journal(), vec![one, two]);
        // A send killed here leaves its lines, and the next one reads them.
        drop(lock);
        let mut next = held(&folder, 97);
        assert_eq!(next.journal(), vec![one, two]);
        next.clear();
        assert_eq!(next.journal(), Vec::new());
    }
}
