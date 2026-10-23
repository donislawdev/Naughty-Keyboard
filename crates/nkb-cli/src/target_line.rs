//! The line `nkb send` says before it types: where the value is about to go, or why it
//! cannot go anywhere. Out of `main.rs` under the ceiling of `D113`, with the sentence for a
//! session that cannot name the window in front (Wayland, `D145`).

use std::io::Write;

use nkb_app::TargetRef;
use nkb_core::report::{ControlKind, Target};

use crate::exit::ExitCode;

/// What the countdown saw of the target: the window in front before and after it.
pub(crate) struct Countdown {
    pub(crate) before: Option<TargetRef>,
    pub(crate) after: Option<TargetRef>,
    pub(crate) delay_seconds: u64,
}

/// Says where the value is about to go, or why it cannot go anywhere, and gives the
/// exit code to stop with in the second case. `describe` names the program and the
/// control of a target, and is not asked for one that has no name.
pub(crate) fn say_where_it_goes(
    err: &mut impl Write,
    seen: &Countdown,
    describe: impl FnOnce(TargetRef) -> String,
) -> Result<(), ExitCode> {
    match seen.after {
        // A session that does not say which window has the keyboard focus (Wayland,
        // `D145`): there is no window to name and no focus to compare, so the line says
        // that and does not offer the check it cannot make.
        Some(target) if target.is_unnamed() => {
            let _ = writeln!(
                err,
                "nkb send: this session does not say which window has the keyboard focus - the value goes to whatever has it"
            );
            Ok(())
        }
        Some(target) => {
            let _ = writeln!(
                err,
                "nkb send: target is window {:#x} - {}",
                target.0,
                describe(target)
            );
            if seen.delay_seconds > 0 && seen.before == seen.after {
                let _ = writeln!(
                    err,
                    "  the focus did not change during the countdown - if that is the terminal, stop now"
                );
            }
            Ok(())
        }
        None => {
            let _ = writeln!(err, "nkb send: nothing holds the keyboard focus.");
            let _ = writeln!(err, "Click into a field, then run this again.");
            Err(ExitCode::InsertFailed)
        }
    }
}

/// The program and the kind of control a value goes to, in the CLI's words -
/// `chrome.exe, a text field` (`D104`). The program's name comes from the
/// system and is escaped, so a control character in it cannot reach the
/// terminal as a command.
pub(crate) fn where_it_goes(target: &Target) -> String {
    let control = match target.field {
        ControlKind::TextField => "a text field",
        ControlKind::Terminal => "a terminal",
        ControlKind::Unconfirmed => "a control not confirmed as a field",
        ControlKind::NotTextField => "not a text field",
        ControlKind::ClosedToAccessibility => "a window closed to accessibility",
    };
    match &target.program {
        Some(program) => format!("{}, {control}", program.escape_debug()),
        None => format!("a program that did not give its name, {control}"),
    }
}

#[cfg(test)]
#[allow(
    clippy::panic,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;

    fn said_where(seen: &Countdown) -> (String, Result<(), ExitCode>) {
        let mut said = Vec::new();
        let result = say_where_it_goes(&mut said, seen, |target| format!("window {}", target.0));
        let said = String::from_utf8(said).unwrap_or_else(|e| panic!("not UTF-8: {e}"));
        (said, result)
    }

    #[test]
    fn a_session_that_cannot_name_the_window_says_so_and_offers_no_comparison() {
        // `D145`: on Wayland the window in front has no name, and two unnamed windows are
        // always equal, so "the focus did not change" would be a claim nobody checked.
        let (said, result) = said_where(&Countdown {
            before: Some(TargetRef::UNNAMED),
            after: Some(TargetRef::UNNAMED),
            delay_seconds: 3,
        });
        assert_eq!(result, Ok(()));
        assert!(said.contains("does not say which window"), "{said}");
        assert!(!said.contains("focus did not change"), "{said}");
        assert!(!said.contains("0xffff"), "{said}");
    }

    #[test]
    fn a_named_target_is_said_with_its_window_and_the_unchanged_focus_is_noted() {
        let seen = |before, after| Countdown {
            before: Some(TargetRef(before)),
            after: Some(TargetRef(after)),
            delay_seconds: 3,
        };
        let (same, result) = said_where(&seen(0x20, 0x20));
        assert_eq!(result, Ok(()));
        assert!(same.contains("target is window 0x20 - window 32"), "{same}");
        assert!(same.contains("focus did not change"), "{same}");
        let (moved, _) = said_where(&seen(0x20, 0x30));
        assert!(!moved.contains("focus did not change"), "{moved}");
        let (no_wait, _) = said_where(&Countdown {
            delay_seconds: 0,
            ..seen(0x20, 0x20)
        });
        assert!(!no_wait.contains("focus did not change"), "{no_wait}");
    }

    #[test]
    fn no_target_stops_the_send_with_a_sentence() {
        let (said, result) = said_where(&Countdown {
            before: None,
            after: None,
            delay_seconds: 3,
        });
        assert_eq!(result, Err(ExitCode::InsertFailed));
        assert!(said.contains("nothing holds the keyboard focus"), "{said}");
    }

    #[test]
    fn the_target_line_names_the_program_and_the_kind_of_control_as_the_palette_does() {
        let at = |program: Option<&str>, field| Target {
            program: program.map(ToOwned::to_owned),
            field,
        };
        assert_eq!(
            where_it_goes(&at(Some("WindowsTerminal.exe"), ControlKind::Terminal)),
            "WindowsTerminal.exe, a terminal"
        );
        assert_eq!(
            where_it_goes(&at(None, ControlKind::TextField)),
            "a program that did not give its name, a text field"
        );
        // A control character from the system reaches the terminal escaped.
        assert_eq!(
            where_it_goes(&at(Some("a\u{1b}[2Jb"), ControlKind::Unconfirmed)),
            "a\\u{1b}[2Jb, a control not confirmed as a field"
        );
        assert_eq!(
            where_it_goes(&at(Some("x.exe"), ControlKind::NotTextField)),
            "x.exe, not a text field"
        );
        assert_eq!(
            where_it_goes(&at(Some("firefox.exe"), ControlKind::ClosedToAccessibility)),
            "firefox.exe, a window closed to accessibility"
        );
    }
}
