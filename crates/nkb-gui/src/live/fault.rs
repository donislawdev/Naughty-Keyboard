//! The worker that stopped on a fault, said in the palette (R08).
//!
//! # Why the worker is guarded
//!
//! The worker holds the shortcuts and runs the sequence ([`super::drive`]).
//! Product code denies `unwrap`, `expect` and `panic`, so a panic there is a
//! bug - an index out of range, a slice cut inside a character - never a path.
//! Until 2026-10-27 such a bug ended the thread and nothing else: the palette
//! stayed on the screen looking ready, no shortcut did anything, and the run
//! ended with exit code 0. Now the fault unwinds as far as [`guarded`], the
//! shortcuts go with the unwind as they did before, the palette says that it
//! stopped and why, and the run ends with a failure ([`WorkerEnd::Failed`]).
//!
//! # What this does not catch
//!
//! A fault on another thread - the Linux key grab's own, the one that asks a
//! Wayland desktop for the shortcuts - does not unwind the worker. A panic
//! while unwinding aborts the process, and then nothing is left to say
//! anything. Both stay as they were.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

use nkb_adapters::i18n;
use slint::Weak;

use crate::Palette;
use crate::focus::{self, Standing};

/// How the worker ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerEnd {
    /// It saw the palette close and let go of everything it held.
    Closed,
    /// It stopped on a fault, and the palette said so.
    Failed,
}

/// The most of a fault's first line the palette shows. The palette is as tall
/// as what it says (`D110`), and a fault from a library can carry a whole dump.
const REASON_LIMIT: usize = 200;

/// Runs `work`, the worker's whole life, and says in the palette when a fault
/// ends it.
///
/// `AssertUnwindSafe` is sound here because nothing `work` held is used after a
/// fault: only the palette's weak handle, and the standing sentence, whose lock
/// answers a poisoned state without a panic (`focus::standing_line`).
pub fn guarded(palette: &Weak<Palette>, standing: &Standing, work: impl FnOnce()) -> WorkerEnd {
    let Some(line) = fault_line(&catch_unwind(AssertUnwindSafe(work))) else {
        return WorkerEnd::Closed;
    };
    let standing = Arc::clone(standing);
    // Through the event loop, because the palette belongs to the main thread.
    // A loop that has already ended has no palette left to say it in.
    let _ = palette.upgrade_in_event_loop(move |palette| focus::say(&palette, &standing, &line));
    WorkerEnd::Failed
}

/// The palette's sentence for how the worker ended - `None` when it ended as
/// it should.
fn fault_line(ended: &std::thread::Result<()>) -> Option<String> {
    let payload = ended.as_ref().err()?;
    let words = payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str));
    Some(i18n::worker_stopped(words.map(reason).as_deref()))
}

/// The first line of a fault's words, cut at [`REASON_LIMIT`] characters with
/// the marker the standard library puts on a value it cuts.
fn reason(words: &str) -> String {
    let first = words.lines().next().unwrap_or_default();
    let mut kept: String = first.chars().take(REASON_LIMIT).collect();
    if kept.len() < first.len() {
        kept.push_str("[...]");
    }
    kept
}

#[cfg(test)]
#[allow(
    clippy::panic,
    reason = "a panic is what this guard is about, and the tests make one on purpose"
)]
mod tests {
    use super::*;

    fn failed(payload: impl std::any::Any + Send) -> std::thread::Result<()> {
        Err(Box::new(payload))
    }

    #[test]
    fn a_worker_that_ended_as_it_should_says_nothing() {
        assert_eq!(fault_line(&Ok(())), None);
    }

    #[test]
    fn a_fault_with_words_says_them() {
        for ended in [
            failed("index out of bounds: the len is 3 but the index is 5"),
            failed(String::from(
                "index out of bounds: the len is 3 but the index is 5",
            )),
        ] {
            let line = fault_line(&ended).unwrap_or_default();
            assert!(line.contains("the len is 3 but the index is 5"), "{line}");
            assert!(line.contains("stopped answering its shortcuts"), "{line}");
        }
    }

    #[test]
    fn a_fault_without_words_still_says_that_the_palette_stopped() {
        let line = fault_line(&failed(7_u8)).unwrap_or_default();
        assert_eq!(line, i18n::worker_stopped(None));
    }

    #[test]
    fn only_the_first_line_is_shown_and_a_long_one_is_cut_with_a_mark() {
        let long = "x".repeat(REASON_LIMIT + 1);
        let words = format!("{long}\nsecond line\nthird line");
        let shown = reason(&words);
        assert_eq!(shown, format!("{}[...]", "x".repeat(REASON_LIMIT)));
        assert_eq!(reason("short\nsecond"), "short");
        // Exactly at the limit nothing is cut, and characters count, not bytes.
        let at_limit = "\u{0105}".repeat(REASON_LIMIT);
        assert_eq!(reason(&at_limit), at_limit);
        assert_eq!(reason(""), "");
    }

    #[test]
    fn a_panic_in_the_work_is_caught_and_counted_as_a_failure() {
        // No palette and no event loop: the sentence has nowhere to go, and the
        // guard must still answer instead of letting the fault through.
        let ended = guarded(&Weak::default(), &focus::standing(), || {
            panic!("a fault made on purpose by the test");
        });
        assert_eq!(ended, WorkerEnd::Failed);
        assert_eq!(
            guarded(&Weak::default(), &focus::standing(), || {}),
            WorkerEnd::Closed
        );
    }
}
