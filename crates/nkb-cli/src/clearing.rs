//! What `nkb send` says about the clearing `--clear` asks for: why the line
//! was not cleared, after a value that arrived and before the news that one
//! did not. Out of `main.rs` since the third reason (`D125`), under the
//! ceiling of `D113`.

use std::io::Write;

use nkb_app::{ClearingOutcome, SkipReason};

/// What became of the clearing, said before the news that the value did not
/// arrive whole - the tester needs to know what the field held before it.
pub(crate) fn say_clearing_before_failure(err: &mut impl Write, clearing: ClearingOutcome) {
    match clearing {
        ClearingOutcome::Done => {
            let _ = writeln!(err, "nkb send: the line was cleared before this happened.");
        }
        ClearingOutcome::Skipped(reason) => {
            let _ = writeln!(err, "{}", not_cleared(reason));
        }
        ClearingOutcome::NotAsked => {}
    }
}

/// Why the line was not cleared, in the sentence of that reason - one place
/// for both moments it is said, after a value that arrived and before the news
/// that one did not.
pub(crate) fn not_cleared(reason: SkipReason) -> &'static str {
    match reason {
        SkipReason::Unconfirmed => CLEARING_SKIPPED,
        SkipReason::Terminal => CLEARING_SKIPPED_IN_TERMINAL,
        SkipReason::ClosedToAccessibility => CLEARING_SKIPPED_CLOSED_TO_ACCESSIBILITY,
    }
}

/// What `nkb send --clear` says when it sent the value without clearing.
///
/// The clearing keys go out only where the focus is confirmed as a text field
/// (`D76`): measured in a spreadsheet, which does not say what holds its focus,
/// they emptied a whole row. So the value went on top of whatever was there,
/// and the command says that rather than let `--clear` read as done.
const CLEARING_SKIPPED: &str = "nkb send: not cleared first - the system does not report this focus as a text field, and the clearing keys could reach beyond one. The value went in on top of what was there.";

/// What `nkb send --clear` says when the focus is a terminal (`OBS-141`).
///
/// A terminal answers the same with a prompt and with a full-screen program
/// running in it, and the clearing keys there go to that program - a file
/// manager may select to the last file and delete. The system does report a
/// text field here, so the sentence above would be false, and this one names
/// the terminal instead.
const CLEARING_SKIPPED_IN_TERMINAL: &str = "nkb send: not cleared first - this is a terminal, where the clearing keys go to the program running in it and could act beyond the line. The value went in on top of what was there.";

/// What `nkb send --clear` says when the window shows the system nothing
/// inside it (`D125`, `OBS-175`).
///
/// Firefox does so while it keeps accessibility services out: the focus is the
/// window itself, wherever it is inside. Nothing tells a field from the rest
/// there, so the keys stay out as for any focus nobody recognised - and this
/// cause is known and the tester can change it, so the sentence names the
/// setting in Firefox's own words.
const CLEARING_SKIPPED_CLOSED_TO_ACCESSIBILITY: &str = "nkb send: not cleared first - this window shows the system nothing inside it, so the clearing keys could reach beyond the field, and the value went in on top of what was there. Firefox does this while \"Prevent accessibility services from accessing your browser\" is on in its settings - turn it off and restart Firefox to have the line cleared.";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_reason_for_not_clearing_has_its_own_sentence() {
        // `D125`: the window closed to accessibility names Firefox's setting,
        // and the other two keep the sentences they had.
        assert_eq!(not_cleared(SkipReason::Unconfirmed), CLEARING_SKIPPED);
        assert_eq!(
            not_cleared(SkipReason::Terminal),
            CLEARING_SKIPPED_IN_TERMINAL
        );
        let closed = not_cleared(SkipReason::ClosedToAccessibility);
        assert!(
            closed.starts_with("nkb send: not cleared first - this window shows")
                && closed
                    .contains("\"Prevent accessibility services from accessing your browser\"")
                && closed.contains("restart Firefox"),
            "{closed}"
        );
    }
}
