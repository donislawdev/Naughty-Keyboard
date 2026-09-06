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
#[allow(dead_code, reason = "the published set must be complete from version one")]
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
    }
}
