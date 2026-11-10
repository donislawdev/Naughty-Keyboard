//! The tester's pack folders as the system shows them (`D175`): a folder chosen
//! in the system's own folder window, and a folder opened in the system's file
//! manager.
//!
//! # What runs where, and why
//!
//! Choosing waits for a person, so the answer comes back through a closure, on a
//! thread of this module's choosing. The one exception is a system that refuses
//! the thread: the answer then says so at once, on the caller's thread, before
//! the call returns - a caller must not hold anything the answer needs:
//!
//! - **Windows:** the folder window (`IFileOpenDialog`) runs on a thread of its
//!   own, in a single-threaded apartment, owned by the caller's window - the way
//!   `rfd` 0.18.0 does it for its asynchronous dialogs. The modal loop of the
//!   folder window is then that thread's, and the window toolkit's loop on the
//!   main thread goes on drawing the palette.
//! - **macOS:** AppKit's panels belong to the main thread, and a panel run from a
//!   handler of the window toolkit is a modal loop inside that handler: winit
//!   0.30.13 then panics at the next event it is handed ("tried to handle event
//!   while another event is currently being handled", `event_handler.rs`). So the
//!   panel is run from the main queue (`dispatch_async_f`), which the main run
//!   loop drains between the toolkit's handlers.
//! - **Linux:** the folder window is the desktop portal's (`FileChooser`), a
//!   window of another process, waited for on a thread of this module.
//!
//! # What the folder window brings into the process, said plainly
//!
//! On Windows and macOS the system's folder window runs INSIDE this process. It
//! loads what other programs add to the system's file browser - cloud drives,
//! icon overlays - and its own path box copies and pastes when the tester
//! presses the keys for that. Neither is this program's doing, and the owner
//! chose the window with both said aloud (`D175`). The folder can also be
//! dropped on the window, which brings in neither.
//!
//! # Opening a folder in the file manager
//!
//! The one program this product starts, and only on the tester's click
//! (`D173` point 2c): Explorer from the Windows directory, Finder through
//! `NSWorkspace`, and on Linux the file manager of the desktop through its
//! interface on the session bus (`org.freedesktop.FileManager1`).
//! `nothing_reaches_the_network.rs` grants each in one file and nowhere else.
//!
//! All of it on a thread of its own, the check that the folder is one
//! included: a team folder on a share that stopped answering holds `stat` for as
//! long as the network waits, and on the thread of the windows that would freeze
//! the palette with it (`D173`, the same reason the folders are read with a limit).

use std::path::{Path, PathBuf};

#[cfg(windows)]
mod win32;

/// What the system's folder window came back with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Picked {
    /// The folder the person chose, as the system names it.
    Folder(PathBuf),
    /// The person closed the window without choosing.
    Cancelled,
}

/// Why no folder was chosen in the system's folder window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PickError {
    /// This desktop has no folder window this program can ask for - on Linux, a
    /// session without the desktop portal.
    NoWindow,
    /// The system would not show the window, or its answer could not be read.
    /// The words are this module's, naming what the system answered.
    Failed(String),
}

impl core::fmt::Display for PickError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NoWindow => write!(f, "this desktop has no folder window to ask for"),
            Self::Failed(why) => write!(f, "{why}"),
        }
    }
}

/// Why a folder could not be shown in the file manager.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShowError {
    /// The folder is not there, or is not a folder - a file manager handed a
    /// path that is not a folder opens somewhere else without a word.
    NotAFolder,
    /// This desktop has no file manager this program can ask for.
    NoFileManager,
    /// The file manager could not be asked. The words name what failed.
    Failed(String),
}

impl core::fmt::Display for ShowError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NotAFolder => write!(f, "it is not a folder"),
            Self::NoFileManager => write!(f, "this desktop has no file manager to ask for"),
            Self::Failed(why) => write!(f, "{why}"),
        }
    }
}

/// What the folder window is asked to be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickRequest {
    /// The window's title, finished words.
    pub title: String,
    /// The folder the window opens in. `None` leaves it to the system.
    pub start: Option<PathBuf>,
    /// The window the folder window belongs to and stays above, as the window
    /// toolkit names it: a window handle on Windows, an X11 window on Linux.
    /// Unused on macOS, where the panel is the application's.
    pub owner: Option<u64>,
}

/// Where the answer of the folder window goes. Called once.
pub type PickAnswer = Box<dyn FnOnce(Result<Picked, PickError>) + Send>;

/// Where the answer of the file manager goes. Called once.
pub type ShowAnswer = Box<dyn FnOnce(Result<(), ShowError>) + Send>;

/// Shows the system's folder window and hands the person's answer to `answer`,
/// on another thread (Windows, Linux) or on the main thread (macOS) - see the
/// module text. Any thread may ask.
///
/// One window at a time is the caller's to keep: a second call while the first
/// window stands shows a second window. So is the check that what came back is a
/// folder: a desktop portal older than its `directory` option shows a window that
/// chooses files, and a folder can be gone by the time the answer is read.
pub fn pick_folder(request: PickRequest, answer: PickAnswer) {
    platform::pick_folder(request, answer);
}

/// Opens `folder` in the system's file manager and hands what came of it to
/// `answer`, from a thread of its own.
///
/// A path that is not a folder is refused before anything is asked: a file
/// manager handed one opens somewhere else without a word, and on Windows the
/// check is also what keeps a path that cannot be a folder off Explorer's
/// command line.
pub fn show_folder(folder: &Path, answer: ShowAnswer) {
    let folder = folder.to_path_buf();
    on_a_thread(
        "nkb-file-manager",
        move || {
            if !folder.is_dir() {
                return Err(ShowError::NotAFolder);
            }
            platform::show_folder(&folder)
        },
        answer,
        |error| {
            Err(ShowError::Failed(format!(
                "the system gave the file manager no thread: {error}"
            )))
        },
        |_| Err(ShowError::Failed(String::from(FAULT))),
    );
}

/// What a fault of this module's own says, in place of the answer it lost.
pub(crate) const FAULT: &str = "this program stopped on a fault of its own while it asked";

/// Runs `work` on a thread of its own and hands its result to `answer` - once,
/// whatever happens:
///
/// - the system refuses the thread: `refused` makes the answer, at once, on the
///   caller's thread. The closure that would have carried `answer` into the
///   thread is dropped with the thread that never ran, so without this the
///   caller would wait for ever.
/// - `work` panics: `faulted` makes the answer, for the same reason.
pub(crate) fn on_a_thread<T: Send + 'static>(
    name: &str,
    work: impl FnOnce() -> T + Send + 'static,
    answer: Box<dyn FnOnce(T) + Send>,
    refused: impl FnOnce(std::io::Error) -> T,
    faulted: impl FnOnce(Box<dyn std::any::Any + Send>) -> T + Send + 'static,
) {
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::sync::{Arc, Mutex, PoisonError};
    let slot = Arc::new(Mutex::new(Some(answer)));
    let theirs = Arc::clone(&slot);
    let spawned = std::thread::Builder::new()
        .name(name.to_owned())
        .spawn(move || {
            // Unwind safe: nothing `work` touched is looked at again after a
            // panic - only its result, which a panic did not make.
            let result = catch_unwind(AssertUnwindSafe(work)).unwrap_or_else(faulted);
            let answer = theirs.lock().unwrap_or_else(PoisonError::into_inner).take();
            if let Some(answer) = answer {
                answer(result);
            }
        });
    if let Err(error) = spawned {
        let answer = slot.lock().unwrap_or_else(PoisonError::into_inner).take();
        if let Some(answer) = answer {
            answer(refused(error));
        }
    }
}

#[cfg(windows)]
mod platform {
    pub(super) use super::win32::{pick_folder, show_folder};
}

#[cfg(target_os = "macos")]
mod platform {
    pub(super) use crate::macos::folders::{pick_folder, show_folder};
}

#[cfg(target_os = "linux")]
mod platform {
    pub(super) use crate::linux::folders::{pick_folder, show_folder};
}

#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
mod platform {
    use super::{PickAnswer, PickError, PickRequest, ShowError};
    use std::path::Path;

    pub(super) fn pick_folder(_request: PickRequest, answer: PickAnswer) {
        answer(Err(PickError::NoWindow));
    }

    pub(super) fn show_folder(_folder: &Path) -> Result<(), ShowError> {
        Err(ShowError::NoFileManager)
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    /// Long enough for any thread to start and end, short enough that a lost
    /// answer fails the test rather than hanging the run.
    const HEARD_WITHIN: Duration = Duration::from_secs(10);

    fn asked(folder: &Path) -> Result<(), ShowError> {
        let (told, heard) = mpsc::channel();
        show_folder(
            folder,
            Box::new(move |answer| {
                let _ = told.send(answer);
            }),
        );
        heard.recv_timeout(HEARD_WITHIN).expect("an answer")
    }

    #[test]
    fn a_path_that_is_not_a_folder_is_refused_before_anything_is_asked() {
        let file = std::env::temp_dir().join(format!("nkb-folders-{}.toml", std::process::id()));
        std::fs::write(&file, b"").expect("a file to point at");
        let a_file = asked(&file);
        let _ = std::fs::remove_file(&file);
        assert_eq!(a_file, Err(ShowError::NotAFolder));
        assert_eq!(
            asked(&file.with_extension("gone")),
            Err(ShowError::NotAFolder)
        );
    }

    fn heard_from(work: impl FnOnce() -> u8 + Send + 'static) -> u8 {
        let (told, heard) = mpsc::channel();
        on_a_thread(
            "nkb-folders-test",
            work,
            Box::new(move |answer| {
                let _ = told.send(answer);
            }),
            |_| 2,
            |_| 3,
        );
        heard.recv_timeout(HEARD_WITHIN).expect("an answer")
    }

    #[test]
    fn the_work_of_a_thread_is_answered_once_it_is_done() {
        assert_eq!(heard_from(|| 1), 1);
    }

    #[test]
    #[allow(clippy::panic, reason = "the fault the answer must survive")]
    fn a_thread_that_stops_on_a_fault_still_answers() {
        assert_eq!(heard_from(|| panic!("a fault of our own")), 3);
    }
}
