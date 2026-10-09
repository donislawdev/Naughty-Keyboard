//! Exit codes.
//!
//! These are a public contract. They end up in other people's pipelines, so
//! adding one after a release is a breaking change: a run that exits zero today
//! would exit non-zero tomorrow and turn somebody's CI red without a single
//! change on their side.
//!
//! An exit code is a property of the failure, carried from where it happened.
//! It is never guessed back from a message at the point of exit.

/// How a run ended.
///
/// Some variants are not constructed yet and that is deliberate, not an
/// oversight: the full set has to exist from the first release. A code added
/// later would change the meaning of an existing run, which is the definition
/// of a breaking change here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
#[allow(
    dead_code,
    reason = "the published set must be complete from version one"
)]
pub enum ExitCode {
    /// The run did what it promised.
    Ok = 0,
    /// Validation did not pass. Used by contribution checks: it says something
    /// about the pack, not about the tool.
    ValidationFailed = 1,
    /// Wrong invocation: unknown command, unknown switch.
    Usage = 2,
    /// The named pack or value does not exist.
    NotFound = 3,
    /// The value could not be inserted.
    InsertFailed = 4,
    /// A file could not be read or written: permissions, a device, a full disk.
    ///
    /// # Why reading and writing share one number
    ///
    /// The rule set says to split codes by the **repair** they force rather than
    /// by the resemblance of their causes, and both of these force the same one:
    /// read the message, then fix something about the file or the folder. What
    /// distinguishes them is wording, and wording is what the report is for.
    ///
    /// Added before the first release deliberately. Three commands were already
    /// reporting a read or write failure under a number that meant something
    /// else, with the message carrying what the number could not - and after a
    /// release, adding a code turns somebody's green pipeline red with no change
    /// on their side. Owner's decision, 2026-09-07.
    IoFailed = 5,
}

impl ExitCode {
    #[must_use]
    pub fn as_i32(self) -> i32 {
        self as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_published_numbers_are_the_ones_documented() {
        // Pinned deliberately. If this test has to change, the change is
        // breaking and belongs in a release note, not in a refactor.
        assert_eq!(ExitCode::Ok.as_i32(), 0);
        assert_eq!(ExitCode::ValidationFailed.as_i32(), 1);
        assert_eq!(ExitCode::Usage.as_i32(), 2);
        assert_eq!(ExitCode::NotFound.as_i32(), 3);
        assert_eq!(ExitCode::InsertFailed.as_i32(), 4);
        assert_eq!(ExitCode::IoFailed.as_i32(), 5);
    }

    #[test]
    fn no_two_ways_of_ending_share_a_number() {
        // The property that makes these readable by a script at all. Two names
        // on one number would make a pipeline branch on a coin toss.
        let all = [
            ExitCode::Ok,
            ExitCode::ValidationFailed,
            ExitCode::Usage,
            ExitCode::NotFound,
            ExitCode::InsertFailed,
            ExitCode::IoFailed,
        ];
        for (index, code) in all.iter().enumerate() {
            for other in &all[index + 1..] {
                assert_ne!(code.as_i32(), other.as_i32(), "{code:?} and {other:?}");
            }
        }
    }

    /// The name the project website gives each code. A `match` and not a list,
    /// so a code added here does not compile until it has a name there too.
    fn site_name(code: ExitCode) -> &'static str {
        match code {
            ExitCode::Ok => "ok",
            ExitCode::ValidationFailed => "validation-failed",
            ExitCode::Usage => "usage",
            ExitCode::NotFound => "not-found",
            ExitCode::InsertFailed => "insert-failed",
            ExitCode::IoFailed => "io-failed",
        }
    }

    /// The website lists every exit code, read from here.
    ///
    /// `web/data/facts/exit-codes.json` is what the site's page of exit codes
    /// is built from. With `NKB_WRITE_SITE=1` this writes it, and otherwise it
    /// fails when the file and this enum disagree. The words about each code
    /// are the site's own, in every language it is built in, and the site build
    /// refuses a code that has none.
    #[test]
    #[allow(
        clippy::expect_used,
        reason = "a failed expectation in a test is a failed test"
    )]
    fn the_site_lists_every_exit_code() {
        let all = [
            ExitCode::Ok,
            ExitCode::ValidationFailed,
            ExitCode::Usage,
            ExitCode::NotFound,
            ExitCode::InsertFailed,
            ExitCode::IoFailed,
        ];
        let rows: Vec<String> = all
            .iter()
            .map(|code| {
                format!(
                    "  {{\n    \"code\": {},\n    \"name\": \"{}\"\n  }}",
                    code.as_i32(),
                    site_name(*code)
                )
            })
            .collect();
        let now = format!("[\n{}\n]\n", rows.join(",\n"));
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../web/data/facts/exit-codes.json");
        if std::env::var_os("NKB_WRITE_SITE").is_some_and(|v| v == "1") {
            std::fs::write(&path, &now).expect("the facts can be written");
            return;
        }
        let kept = std::fs::read_to_string(&path).expect(
            "web/data/facts/exit-codes.json is readable. \
             Write it: NKB_WRITE_SITE=1 cargo test -p nkb-cli the_site_lists_every_exit_code",
        );
        assert_eq!(
            kept, now,
            "the website lists other exit codes than the program has. If the program is \
             right: NKB_WRITE_SITE=1 cargo test -p nkb-cli the_site_lists_every_exit_code"
        );
    }
}
