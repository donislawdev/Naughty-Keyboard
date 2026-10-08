//! What `nkb` writes on standard output, and how a run ends when that cannot
//! be written.
//!
//! # Why this exists
//!
//! `println!` and `print!` panic when a write fails, and a pipe whose reader
//! has gone fails every write. Measured 2026-10-08, before this module: `nkb
//! packs`, `nkb --help`, `nkb show` and `nkb lint` with a reader that had
//! already closed the pipe each ended in a panic and exit code 101, a number
//! the table in `exit.rs` does not have. `nkb emit` wrote through `write_all`
//! and survived - but ended with 0 on EVERY failed write, so a full disk under
//! `nkb emit pack > file` read as success and left half a file behind.
//!
//! # The rule
//!
//! - **The reader left** (a closed pipe, as in `nkb packs | head -1`): nothing
//!   more is written, nothing is said, and the run ends with the code the
//!   command found. A reader that stops reading made a choice, and a verdict
//!   that failed still fails.
//! - **Anything else** (a full disk, a file that refuses writes): one sentence
//!   on standard error, nothing more on standard output, and the run ends with
//!   5 whatever the command found. Its answer did not reach the caller whole,
//!   and a script reading 0 or 1 would act on half of it.
//!
//! Standard error is written with `let _ = writeln!(err, ...)` everywhere, and
//! its failures are dropped on purpose: a sentence nobody can read is no reason
//! to stop the work it describes.
//!
//! `tests/closed_output.rs` measures both cases on the built binary and keeps
//! the panicking macros out of every package of the workspace.

use crate::exit::ExitCode;
use std::fmt;
use std::io::{self, Write};
use std::sync::atomic::{AtomicU8, Ordering};

/// What became of standard output during this run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum Stdout {
    /// Every write so far went through.
    Open = 0,
    /// The reader closed the pipe.
    Closed = 1,
    /// A write failed for any other reason.
    Failed = 2,
}

impl Stdout {
    const fn from_u8(raw: u8) -> Self {
        match raw {
            1 => Self::Closed,
            2 => Self::Failed,
            _ => Self::Open,
        }
    }

    /// What a failed write leaves standard output as.
    ///
    /// A closed pipe is `BrokenPipe` on every system this builds for: `EPIPE`
    /// on Linux and macOS, where Rust ignores `SIGPIPE` before `main` runs, and
    /// `ERROR_NO_DATA` (232) or `ERROR_BROKEN_PIPE` (109) on Windows. The
    /// first of those is what the panic before this module quoted.
    fn after(error: &io::Error) -> Self {
        if error.kind() == io::ErrorKind::BrokenPipe {
            Self::Closed
        } else {
            Self::Failed
        }
    }
}

/// The first failure of standard output, kept for the rest of the run. Nothing
/// is written there after it.
static STDOUT: AtomicU8 = AtomicU8::new(Stdout::Open as u8);

/// The exit code a run ends with, given what became of standard output.
const fn settled(found: ExitCode, stdout: Stdout) -> ExitCode {
    match stdout {
        Stdout::Failed => ExitCode::IoFailed,
        Stdout::Open | Stdout::Closed => found,
    }
}

/// The exit code this run ends with: the one the command found, or 5 when its
/// answer could not be written. Called once, in `main`.
pub(crate) fn settle(found: ExitCode) -> ExitCode {
    settled(found, Stdout::from_u8(STDOUT.load(Ordering::Relaxed)))
}

/// Text on standard output, or nothing once standard output has failed. Use
/// it through `out!` and `outln!`.
///
/// Under `cargo test` the text goes where the test harness collects it, as it
/// did while this was `println!`, instead of into the middle of the test
/// report. The harness collects only what goes through `print!`. The real
/// path is measured on the built binary by `tests/closed_output.rs`.
pub(crate) fn to_stdout(args: fmt::Arguments<'_>, newline: bool) {
    #[cfg(test)]
    tests::collect(args, newline);
    #[cfg(not(test))]
    write_to_stdout(args, newline);
}

#[cfg_attr(
    test,
    allow(dead_code, reason = "a test build sends the text to the harness")
)]
fn write_to_stdout(args: fmt::Arguments<'_>, newline: bool) {
    if STDOUT.load(Ordering::Relaxed) != Stdout::Open as u8 {
        return;
    }
    let Err(error) = write_through(&mut io::stdout().lock(), args, newline) else {
        return;
    };
    let now = Stdout::after(&error);
    let first = STDOUT
        .compare_exchange(
            Stdout::Open as u8,
            now as u8,
            Ordering::Relaxed,
            Ordering::Relaxed,
        )
        .is_ok();
    if first && now == Stdout::Failed {
        let _ = writeln!(io::stderr(), "{}", failure_sentence(&error));
    }
}

/// What standard error says when standard output failed for a reason other
/// than a reader that left.
fn failure_sentence(error: &io::Error) -> String {
    format!("nkb: standard output could not be written ({error}), so what it holds is incomplete.")
}

/// Flushed on every call, because a buffer holding the end of the answer would
/// otherwise fail at exit, where nothing is left to say so or to change the
/// exit code.
fn write_through(sink: &mut impl Write, args: fmt::Arguments<'_>, newline: bool) -> io::Result<()> {
    sink.write_fmt(args)?;
    if newline {
        sink.write_all(b"\n")?;
    }
    sink.flush()
}

/// `println!` that cannot panic.
macro_rules! outln {
    () => {
        $crate::output::to_stdout(format_args!(""), true)
    };
    ($($arg:tt)*) => {
        $crate::output::to_stdout(format_args!($($arg)*), true)
    };
}

/// `print!` that cannot panic.
macro_rules! out {
    ($($arg:tt)*) => {
        $crate::output::to_stdout(format_args!($($arg)*), false)
    };
}

pub(crate) use {out, outln};

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;

    /// Where `to_stdout` sends its text in a test build: through `print!`,
    /// which the test harness collects per test.
    pub(super) fn collect(args: fmt::Arguments<'_>, newline: bool) {
        if newline {
            println!("{args}");
        } else {
            print!("{args}");
        }
    }

    /// A sink that refuses every write with one kind of error.
    struct Refuses(io::ErrorKind);

    impl Write for Refuses {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::from(self.0))
        }
        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::from(self.0))
        }
    }

    #[test]
    fn a_failed_write_is_returned_rather_than_raised() {
        let failed = write_through(
            &mut Refuses(io::ErrorKind::BrokenPipe),
            format_args!("{}", "verdict"),
            true,
        );
        assert_eq!(failed.map_err(|e| e.kind()), Err(io::ErrorKind::BrokenPipe));
    }

    #[test]
    fn a_line_is_the_text_and_one_newline() {
        let mut sink = Vec::new();
        write_through(&mut sink, format_args!("{} {}", "a", 1), true)
            .expect("a vector takes every write");
        write_through(&mut sink, format_args!("b"), false).expect("a vector takes every write");
        assert_eq!(sink, b"a 1\nb");
    }

    #[test]
    fn only_a_closed_pipe_counts_as_a_reader_that_left() {
        let closed = io::Error::from(io::ErrorKind::BrokenPipe);
        assert_eq!(Stdout::after(&closed), Stdout::Closed);
        for other in [
            io::ErrorKind::PermissionDenied,
            io::ErrorKind::StorageFull,
            io::ErrorKind::WriteZero,
            io::ErrorKind::Other,
        ] {
            assert_eq!(
                Stdout::after(&io::Error::from(other)),
                Stdout::Failed,
                "{other:?}"
            );
        }
    }

    #[test]
    fn a_closed_pipe_keeps_the_code_the_command_found_and_a_failure_ends_with_5() {
        for found in [
            ExitCode::Ok,
            ExitCode::ValidationFailed,
            ExitCode::NotFound,
            ExitCode::IoFailed,
        ] {
            assert_eq!(settled(found, Stdout::Open), found);
            assert_eq!(settled(found, Stdout::Closed), found);
            assert_eq!(settled(found, Stdout::Failed), ExitCode::IoFailed);
        }
    }

    #[test]
    fn the_state_survives_the_trip_through_the_atomic() {
        for state in [Stdout::Open, Stdout::Closed, Stdout::Failed] {
            assert_eq!(Stdout::from_u8(state as u8), state);
        }
    }

    #[test]
    fn the_failure_sentence_names_the_error_and_says_the_output_is_incomplete() {
        let sentence = failure_sentence(&io::Error::from(io::ErrorKind::StorageFull));
        assert!(sentence.starts_with("nkb: standard output could not be written ("));
        assert!(sentence.ends_with("so what it holds is incomplete."));
    }
}
