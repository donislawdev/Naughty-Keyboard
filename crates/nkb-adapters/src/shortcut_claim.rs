//! One palette per user holds the global shortcuts - the claim a Mac needs
//! because the system does not ask for it (`D144`, `OBS-187`).
//!
//! # Why
//!
//! Windows refuses a shortcut that is held already: a second palette is told so
//! for every one of them, says that all of them are taken (`OBS-176`) and does
//! not wait on a set that cannot be pressed. The hot key calls of Carbon do not
//! refuse. Measured on macOS 27.0 (2026-10-20): a second process registering the
//! same chord is told it worked, and BOTH processes hear every press, so two
//! palettes type two different values into one field.
//!
//! So a Mac palette asks first. The answer is a lock, and the shortcuts are
//! registered only by the palette that holds it. A palette that does not hold it
//! reports every shortcut as taken, which is what the palette already knows how
//! to start with and to say (`RegistrationSummary::AllTaken`): no shortcuts, one
//! sentence naming the likely holder, and a resume (closing the shortcuts window)
//! asks again.
//!
//! # The lock
//!
//! A file beside the settings file, locked with `File::try_lock` (`flock` on
//! Unix). The system releases the lock when the process ends, however it ends,
//! so a palette killed or crashed leaves nothing to clean up. The file itself is
//! never removed and has no content. It stays locked for the life of the
//! process once taken: giving the shortcuts back while the shortcuts window is
//! open must not let a second palette in.
//!
//! A lock file deleted under a running palette lets a second one create and lock
//! a new one. Nothing here protects against that, and it is not a mistake a
//! tester makes by accident in a folder of the program's own.
//!
//! # When the answer cannot be had
//!
//! No home folder, a folder that cannot be made, a file that cannot be opened:
//! the palette then registers its shortcuts as it did before the claim existed.
//! A lock that cannot be asked about must not leave a tester with a palette
//! that never answers.

use std::fs::{File, OpenOptions, TryLockError};
use std::path::Path;
use std::sync::Mutex;

/// What asking for the claim came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Claim {
    /// This process holds it, now or already.
    Held,
    /// Another process holds it.
    Refused,
    /// The lock could not be asked about, so there is no answer either way.
    Unasked,
}

/// The claim of one process. A process has one, in the static of `shortcuts`.
pub(crate) struct Claims {
    held: Mutex<Option<File>>,
}

impl Claims {
    pub(crate) const fn new() -> Self {
        Self {
            held: Mutex::new(None),
        }
    }

    /// Asks for the claim on `place`, or says this process holds it already.
    pub(crate) fn take(&self, place: &Path) -> Claim {
        let Ok(mut held) = self.held.lock() else {
            return Claim::Unasked;
        };
        if held.is_some() {
            return Claim::Held;
        }
        match lock(place) {
            Ok(file) => {
                *held = Some(file);
                Claim::Held
            }
            Err(answer) => answer,
        }
    }
}

impl Claims {
    /// Whether another process holds the claim. Two inputs are given rather than
    /// read here, so that every answer is tested on every system: whether the
    /// system is one that needs the claim (`asked_here`) and whether the lock file
    /// has a place at all. Neither a system that does not ask nor a lock that
    /// cannot be asked about is a refusal.
    pub(crate) fn is_held_elsewhere(&self, asked_here: bool, place: Option<&Path>) -> bool {
        if !asked_here {
            return false;
        }
        let Some(place) = place else {
            return false;
        };
        self.take(place) == Claim::Refused
    }
}

/// The file at `place`, locked - or why it is not.
fn lock(place: &Path) -> Result<File, Claim> {
    if let Some(folder) = place.parent() {
        std::fs::create_dir_all(folder).map_err(|_| Claim::Unasked)?;
    }
    // Not truncated: the file has no content, and a second palette opening it
    // must not touch what the first holds.
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(place)
        .map_err(|_| Claim::Unasked)?;
    match file.try_lock() {
        Ok(()) => Ok(file),
        Err(TryLockError::WouldBlock) => Err(Claim::Refused),
        Err(TryLockError::Error(_)) => Err(Claim::Unasked),
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    /// A folder of its own in the temp folder, removed before and after.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("nkb-claim-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            Self(path)
        }

        fn lock_file(&self) -> PathBuf {
            self.0.join("nested").join("shortcuts.lock")
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn the_first_to_ask_holds_it_and_makes_the_folder() {
        let scratch = Scratch::new("first");
        let claims = Claims::new();
        assert_eq!(claims.take(&scratch.lock_file()), Claim::Held);
        assert!(scratch.lock_file().is_file(), "the file was not made");
    }

    #[test]
    fn a_process_that_holds_it_holds_it_again_without_opening_a_second_lock() {
        let scratch = Scratch::new("again");
        let claims = Claims::new();
        for _ in 0..3 {
            assert_eq!(claims.take(&scratch.lock_file()), Claim::Held);
        }
    }

    /// Another process is another holder of the lock: here a second `Claims`,
    /// which opens the file anew and so conflicts with the first as a second
    /// process would.
    #[test]
    fn a_second_holder_is_refused_until_the_first_is_gone() {
        let scratch = Scratch::new("second");
        let first = Claims::new();
        assert_eq!(first.take(&scratch.lock_file()), Claim::Held);

        let second = Claims::new();
        assert_eq!(second.take(&scratch.lock_file()), Claim::Refused);
        // Asking again changes nothing while the first holds it.
        assert_eq!(second.take(&scratch.lock_file()), Claim::Refused);

        // The first ends, as a process does: its lock goes with it.
        drop(first);
        assert_eq!(second.take(&scratch.lock_file()), Claim::Held);
    }

    #[test]
    fn two_places_are_two_claims() {
        let scratch = Scratch::new("places");
        let one = Claims::new();
        let other = Claims::new();
        assert_eq!(one.take(&scratch.0.join("a").join("l")), Claim::Held);
        assert_eq!(other.take(&scratch.0.join("b").join("l")), Claim::Held);
    }

    /// The lock cannot be asked about, and the answer is "no answer", never a
    /// refusal: the palette keeps its shortcuts.
    #[test]
    fn a_folder_that_cannot_be_made_is_no_answer() {
        let scratch = Scratch::new("blocked");
        std::fs::create_dir_all(&scratch.0).expect("a scratch folder");
        // A file where the folder should be.
        let blocker = scratch.0.join("nested");
        std::fs::write(&blocker, b"a file").expect("a scratch file");
        assert_eq!(Claims::new().take(&scratch.lock_file()), Claim::Unasked);
    }

    /// The same, one step later: the folder is there, the file cannot be opened.
    #[test]
    fn a_lock_file_that_cannot_be_opened_is_no_answer() {
        let scratch = Scratch::new("unopenable");
        // A folder where the file should be.
        std::fs::create_dir_all(scratch.lock_file()).expect("a scratch folder");
        assert_eq!(Claims::new().take(&scratch.lock_file()), Claim::Unasked);
    }

    /// The system that does not ask is never refused, and never takes the lock:
    /// a Windows palette has no use for it, and a lock it took would lock out a
    /// Mac palette on a shared folder.
    #[test]
    fn a_system_that_does_not_ask_is_not_refused_and_takes_nothing() {
        let scratch = Scratch::new("not-asked");
        let holder = Claims::new();
        assert_eq!(holder.take(&scratch.lock_file()), Claim::Held);

        let other = Claims::new();
        assert!(!other.is_held_elsewhere(false, Some(&scratch.lock_file())));
        drop(holder);
        assert_eq!(
            other.take(&scratch.lock_file()),
            Claim::Held,
            "nothing was taken by the question that was not asked"
        );
    }

    #[test]
    fn a_system_that_asks_is_refused_while_another_holds_it() {
        let scratch = Scratch::new("asked-refused");
        let holder = Claims::new();
        assert_eq!(holder.take(&scratch.lock_file()), Claim::Held);

        let other = Claims::new();
        assert!(other.is_held_elsewhere(true, Some(&scratch.lock_file())));
        // The one that holds it is not told that another does.
        assert!(!holder.is_held_elsewhere(true, Some(&scratch.lock_file())));
    }

    #[test]
    fn a_system_that_asks_and_finds_it_free_takes_it() {
        let scratch = Scratch::new("asked-free");
        let first = Claims::new();
        assert!(!first.is_held_elsewhere(true, Some(&scratch.lock_file())));
        assert_eq!(
            Claims::new().take(&scratch.lock_file()),
            Claim::Refused,
            "asking took it"
        );
    }

    #[test]
    fn a_lock_that_cannot_be_asked_about_is_no_refusal() {
        let scratch = Scratch::new("asked-nothing");
        assert!(
            !Claims::new().is_held_elsewhere(true, None),
            "no place for the file"
        );
        std::fs::create_dir_all(&scratch.0).expect("a scratch folder");
        std::fs::write(scratch.0.join("nested"), b"a file").expect("a scratch file");
        assert!(
            !Claims::new().is_held_elsewhere(true, Some(&scratch.lock_file())),
            "a folder that cannot be made"
        );
    }
}
