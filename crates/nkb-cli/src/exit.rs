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
}
