//! The one file `nkb lint` and `nkb fmt` are named, and what they say when it
//! is not a pack file or could not be read.
//!
//! # Exactly the file named
//!
//! Both commands take a path a person typed and read the file at that path, no
//! other (R04). A name that does not end in `.toml` is a usage error: until it
//! was, `nkb fmt notes.txt` read and rewrote `notes.toml` beside it, a file the
//! person never named, and `nkb lint notes` judged it.
//!
//! # Shown as somebody else's text
//!
//! The path goes into every sentence through `escaped_prose`, like the keys and
//! identifiers of a problem (R25). A file name is chosen by whoever made the
//! file, and in a pull request that is a stranger.

use std::io::Write;
use std::path::Path;

use nkb_adapters::{DirectoryPackSource, NotAPackFile};
use nkb_app::LintOutcome;
use nkb_core::text::escaped_prose;

use crate::exit::ExitCode;
use crate::lint_report::pack_file_limit;

/// A pack file a person named: where it is read from, the identifier it is read
/// as, and its name as it may be printed.
pub struct NamedPack {
    pub source: DirectoryPackSource,
    pub id: String,
    pub shown: String,
}

/// The pack file at `path`, or nothing after saying why `path` does not name
/// one. Nothing means a usage error.
pub fn named_pack(command: &str, path: &str) -> Option<NamedPack> {
    let shown = escaped_prose(path);
    match DirectoryPackSource::split(Path::new(path)) {
        Ok((source, id)) => Some(NamedPack { source, id, shown }),
        Err(why) => {
            let mut err = std::io::stderr();
            let _ = match why {
                NotAPackFile::NoFileName => {
                    writeln!(err, "nkb {command}: '{shown}' does not name a file.")
                }
                NotAPackFile::NotToml => writeln!(
                    err,
                    "nkb {command}: '{shown}' is not a .toml file. A pack file's name ends in .toml, and only the file named is read."
                ),
            };
            None
        }
    }
}

/// Why a named file was not read at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotRead {
    NotFound,
    Unreadable,
    TooLarge,
}

impl NotRead {
    /// The exit code for it, from the table in `D46`: a file that is not there
    /// is 3, one that is there and was not read is 5.
    #[must_use]
    pub fn exit(self) -> ExitCode {
        match self {
            Self::NotFound => ExitCode::NotFound,
            Self::Unreadable | Self::TooLarge => ExitCode::IoFailed,
        }
    }
}

/// Says why the file named was not read, and returns the exit code for it.
pub fn say_not_read(command: &str, shown: &str, why: NotRead) -> ExitCode {
    let mut err = std::io::stderr();
    let _ = match why {
        NotRead::NotFound => writeln!(err, "nkb {command}: no pack file at '{shown}'."),
        NotRead::Unreadable => writeln!(
            err,
            "nkb {command}: '{shown}' is there and could not be read. Check the file's permissions."
        ),
        NotRead::TooLarge => writeln!(
            err,
            "nkb {command}: '{shown}' is larger than {}, the most a pack file may be, so it was not read.",
            pack_file_limit()
        ),
    };
    why.exit()
}

/// The exit code of `nkb lint` for one outcome, the same with and without
/// `--json`. Until `OBS-234` the JSON branch answered 3 for a file it could not
/// read, while the text branch answered 5 for the same file.
#[must_use]
pub fn lint_exit(outcome: &LintOutcome) -> ExitCode {
    match outcome {
        LintOutcome::Judged(report) if report.accepted() => ExitCode::Ok,
        LintOutcome::Judged(_) => ExitCode::ValidationFailed,
        LintOutcome::NotFound => NotRead::NotFound.exit(),
        LintOutcome::Unreadable => NotRead::Unreadable.exit(),
        LintOutcome::TooLarge => NotRead::TooLarge.exit(),
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;
    use nkb_core::lint::{LintProblem, LintReport, RuleCode};

    #[test]
    fn a_file_that_was_not_read_ends_lint_with_the_same_code_in_both_modes() {
        // OBS-234: one function decides for both, so the modes cannot drift.
        assert_eq!(lint_exit(&LintOutcome::NotFound), ExitCode::NotFound);
        assert_eq!(lint_exit(&LintOutcome::Unreadable), ExitCode::IoFailed);
        assert_eq!(lint_exit(&LintOutcome::TooLarge), ExitCode::IoFailed);
        assert_eq!(
            lint_exit(&LintOutcome::Judged(LintReport::default())),
            ExitCode::Ok
        );
        let mut refused = LintReport::default();
        refused.push(LintProblem::new(RuleCode::NotUtf8OrByteOrderMark));
        assert_eq!(
            lint_exit(&LintOutcome::Judged(refused)),
            ExitCode::ValidationFailed
        );
    }

    #[test]
    fn a_path_is_a_pack_file_only_when_its_name_ends_in_toml() {
        assert!(named_pack("lint", "packs/whitespace.toml").is_some());
        assert!(named_pack("lint", "packs/whitespace").is_none());
        assert!(named_pack("fmt", "notes.txt").is_none());
    }

    #[test]
    fn the_path_is_printed_with_what_would_act_on_a_terminal_escaped() {
        let named = named_pack("lint", "evil\u{1B}[2J.toml").expect("a .toml name");
        assert_eq!(named.shown, r"evil\u001B[2J.toml");
        assert_eq!(named.id, "evil\u{1B}[2J");
    }
}
