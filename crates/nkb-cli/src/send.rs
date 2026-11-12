//! `nkb send` - one value into the field that has the keyboard focus.
//!
//! The value is one of a pack, or one of the values made for a limit
//! (`D178`): the set `nkb boundary` prints and the palette walks, typed by its
//! place in that order. A limit reads no pack and no folder, so the settings
//! file is not opened for it.

use std::io::Write;
use std::path::{Path, PathBuf};

use nkb_adapters::boundary_text::command;
use nkb_adapters::{DirectInjection, EnglishBoundary, PackFolders, SystemClock, TomlPackFormat};
use nkb_app::{
    Availability, ClearingOutcome, Clock, MadeSet, OnTheWay, PackSource, SendOutcome, SendRequest,
    SourceError, TargetInspector, TargetRef, ValueDelivery, ValueFacts, Wanted, send_value,
};
use nkb_core::boundary::Limit;

use crate::browse_report;
use crate::clearing::{not_cleared, say_clearing_before_failure};
use crate::exit::ExitCode;
use crate::folders;
use crate::limit_args::see_help;
use crate::on_the_way;
use crate::refusals::{NEEDS_PERMISSION, say_cut_short, say_not_cleared, say_not_delivered};
use crate::send_args::{COMMAND, SendArgs, Source, say_refused, send_args};
use crate::sent_notes::{say_pace, say_text_substitutions, sent_counts};
use crate::target_line::{Countdown, say_where_it_goes, where_it_goes};

/// `nkb send <pack> [--index N] [--delay S] [--clear] [--team-folder <folder>]`
/// or `nkb send --max-length N [--min-length N] [--unit U] --index N [--delay S]
/// [--clear]` - put one value into the focused field.
///
/// # `--clear` is opt-in, and that is the whole point of the switch
///
/// Without it, the only keys this command ever presses are the value's own
/// characters. With it, `Home`, `Shift+End`, `Delete` go out first - the
/// within-the-line recipe of `ux-spec.md` 4, which never selects beyond the
/// field. A command run from scripts should not press navigation keys in
/// somebody's window unless the script said so.
///
/// # Why there is a delay at all, and why it defaults to more than zero
///
/// The command is run from a terminal, so at the moment it starts, the terminal
/// is the focused window - and the focused window is exactly where this writes.
/// Without a pause, the only thing `nkb send` could ever demonstrate is typing
/// into the shell that launched it.
///
/// So the pause is not a convenience, it is what makes the command mean what its
/// name says. `--delay 0` is kept for a target that is already focused by
/// something else, which is how the automated check drives it.
///
/// # A limit needs `--index`
///
/// Its first value is the empty one, so a default would type nothing, and the
/// values of a limit are a set to choose from rather than a list read from the
/// top. Asked for now, a default can still come later without breaking a
/// script - the other way round it could not (`D178`).
///
/// Standard output stays EMPTY here, deliberately. `09-CLI-I-CI.md` gives stdout
/// to data, and this command's data does not come back to the caller - it goes
/// into somebody else's window. Everything a person reads is on standard error.
pub(crate) fn send(args: &[String]) -> ExitCode {
    let (rest, team) = match folders::take(args, COMMAND) {
        Ok(taken) => taken,
        Err(code) => return code,
    };
    let asked = match send_args(&rest) {
        Ok(asked) => asked,
        Err(code) => return code,
    };
    match asked.wanted {
        Source::Pack(name) => send_from_pack(&asked, name, &folders::of_run(team)),
        Source::Limit(_) if team.is_some() => say_refused(&[
            "a limit reads no pack, so --team-folder has no folder to read.".to_owned(),
            see_help(COMMAND),
        ]),
        Source::Limit(limit) => send_made(&asked, limit),
    }
}

/// One value of the pack `name`, read from the folders of this run.
fn send_from_pack(asked: &SendArgs<'_>, name: &str, folders: &PackFolders) -> ExitCode {
    let catalogue = folders.catalogue();
    let about = About::Pack {
        name,
        file: folders::pack_file(&catalogue, folders, name),
    };
    let code = deliver(asked, Wanted::Pack(name), &about, &catalogue);
    if code == ExitCode::NotFound {
        folders::say_not_read(&mut crate::output::err(), COMMAND, &catalogue, folders);
    }
    code
}

/// One of the values made for `limit`, by its place in the set.
fn send_made(asked: &SendArgs<'_>, limit: Limit) -> ExitCode {
    let set = MadeSet::new(limit, &EnglishBoundary);
    let position = usize::try_from(asked.position).unwrap_or(usize::MAX);
    let about = About::Made {
        set: &set,
        position,
        command: command(limit),
    };
    let available = set.pack().values.len();
    if position == 0 || position > available {
        // Known without the machine, so said before the countdown rather than
        // after three seconds of watching it.
        return report_send(
            &mut crate::output::err(),
            &about,
            SendOutcome::NoSuchIndex {
                asked: position,
                available,
            },
        );
    }
    deliver(asked, Wanted::Made(&set), &about, &NoPacks)
}

/// A source with no packs in it, for a send whose value was made for a
/// limit: the use case reads no pack for it, and this says so if it ever did.
struct NoPacks;

impl PackSource for NoPacks {
    fn read(&self, _id: &str) -> Result<String, SourceError> {
        Err(SourceError::NotFound)
    }
}

/// What the send is about, in the words its sentences need.
enum About<'a> {
    /// A pack, by the name the person typed, and the file it is read from
    /// when a folder holds it - what a sentence about a file that gave no
    /// text names (`OBS-247`).
    Pack {
        name: &'a str,
        file: Option<PathBuf>,
    },
    /// The values made for a limit: the value at `position`, and the command
    /// that prints the same values.
    Made {
        set: &'a MadeSet,
        position: usize,
        command: String,
    },
}

impl About<'_> {
    /// What a sentence about a pack calls it. A made set is never read, so a
    /// sentence of that kind never reaches it.
    fn named(&self) -> &str {
        match self {
            Self::Pack { name, .. } => name,
            Self::Made { set, .. } => &set.pack().id,
        }
    }

    /// The file the pack is read from when a folder holds it. None for a
    /// built-in pack, and for a made set, which no file holds.
    fn file(&self) -> Option<&Path> {
        match self {
            Self::Pack { file, .. } => file.as_deref(),
            Self::Made { .. } => None,
        }
    }

    /// The command that prints the values instead of typing one.
    fn instead(&self) -> String {
        match self {
            Self::Pack { name, .. } => format!("nkb emit {name}"),
            Self::Made { command, .. } => command.clone(),
        }
    }

    /// What a field that keeps to the limit does with the value sent: the
    /// sentence the report block quotes, and what the palette's mark says.
    fn expected(&self) -> Option<&str> {
        let Self::Made { set, position, .. } = self else {
            return None;
        };
        set.pack()
            .values
            .get(position.checked_sub(1)?)?
            .expect
            .as_deref()
    }
}

/// Counts down, says where the value goes, sends it and says what happened.
fn deliver(
    asked: &SendArgs<'_>,
    wanted: Wanted<'_>,
    about: &About<'_>,
    source: &dyn PackSource,
) -> ExitCode {
    let delivery = DirectInjection;
    let target = match wait_for_a_target(&delivery, &about.instead(), asked.delay_seconds) {
        Ok(target) => target,
        Err(code) => return code,
    };

    let request = SendRequest {
        wanted,
        position: usize::try_from(asked.position).unwrap_or(usize::MAX),
        clearing: asked.clearing,
        // Today in UTC, for a `next-date` value (`D133`).
        on: SystemClock.today(),
        // The window the line above named: the clearing and the value go there
        // or nowhere (`D148`).
        target,
    };

    // `DirectInjection` is both the delivery and the keystroke route: one
    // adapter, two ports, so the two cannot disagree about the target.
    // `OBS-160`: a send still running after a second says so once, on the way,
    // with how to stop it - the palette's send band, in this command's words.
    let started = std::time::Instant::now();
    let mut said = false;
    // `OBS-160`, `D145` point 76: what the send says on its way, in the words of this command.
    let mut on_the_way = |way: OnTheWay<'_>| {
        if let Some(line) = on_the_way::line_for(way, started.elapsed(), &mut said) {
            let _ = writeln!(crate::output::err(), "{line}");
        }
    };
    let outcome = send_value(
        source,
        &TomlPackFormat,
        &delivery,
        &delivery,
        &request,
        &mut on_the_way,
    );
    report_send(&mut crate::output::err(), about, outcome)
}

/// Asks whether this system can send at all, counts down and says where the
/// value is about to go - the target the send is aimed at - or gives the exit
/// code to stop with, having said why.
fn wait_for_a_target(
    delivery: &DirectInjection,
    instead: &str,
    delay_seconds: u64,
) -> Result<TargetRef, ExitCode> {
    // Asked before the countdown, so a machine with no route does not make
    // somebody watch three seconds tick away for nothing.
    match delivery.availability() {
        Availability::Ready => {}
        Availability::Unavailable { reason } => {
            let mut err = crate::output::err();
            let _ = writeln!(
                err,
                "nkb send: no way to deliver keystrokes on {reason} yet."
            );
            let _ = writeln!(
                err,
                "Nothing was sent. `{instead}` writes the values instead."
            );
            return Err(ExitCode::InsertFailed);
        }
        Availability::NeedsPermission => {
            let mut err = crate::output::err();
            let _ = writeln!(err, "{NEEDS_PERMISSION}");
            let _ = writeln!(
                err,
                "`{instead}` writes the values instead, for as long as it is off."
            );
            return Err(ExitCode::InsertFailed);
        }
    }

    // Measured 2026-09-08: run from a terminal with nothing else focused, this
    // command reported `sent` while the value went into the terminal. The system
    // had accepted the events, so the report was not false - it was just read as
    // something stronger than it said. Comparing the target across the wait is
    // the cheapest thing that turns that into a sentence somebody can act on.
    let before = delivery.target();
    count_down(delay_seconds);
    let after = delivery.target();

    // Reported BEFORE sending, and never as a refusal.
    //
    // ⚠️ A blocking version of this shipped for about ten minutes and was wrong:
    // it refused whenever the target had not changed during the countdown, which
    // is also what a correctly focused target looks like when nobody touches it.
    // Measured the same day, `GetConsoleWindow` was tried as a sharper test and
    // returns 0 for a process launched without a console of its own - so there is
    // no cheap way here to tell "the terminal I came from" from "the field you
    // meant". Since `D104` the line names the program and the kind of control,
    // read once here - "WindowsTerminal.exe, a terminal" is the sentence a tester
    // can act on, and the palette says the same after each value (parity,
    // untouchable rule 12).
    say_where_it_goes(
        &mut crate::output::err(),
        &Countdown {
            before,
            after,
            delay_seconds,
        },
        |target| where_it_goes(&delivery.inspect(target)),
    )
}

/// What `nkb send` says about one send, and the exit code it ends with.
///
/// Every word goes to `err` - standard error in the command, a buffer in the
/// tests. Standard output stays empty, for the reason given on [`send`].
fn report_send(err: &mut impl Write, about: &About<'_>, outcome: SendOutcome) -> ExitCode {
    let wanted = about.named();
    let in_flight = matches!(outcome, SendOutcome::InFlight { .. });
    match outcome {
        SendOutcome::Sent {
            facts:
                ValueFacts {
                    reference,
                    name,
                    graphemes,
                    code_points,
                    bytes,
                    warnings,
                    // The preview and the shape are for the PALETTE. The CLI
                    // prints the escaped form through `nkb emit`, which answers
                    // the same question without inventing a second format for
                    // it - and a marker glyph in a pipeline would be worse than
                    // the escape it replaced.
                    preview: _,
                    shape: _,
                },
            utf16_units,
            clearing,
            following,
            may_alter,
        } => {
            match clearing {
                ClearingOutcome::Done => {
                    let _ = writeln!(err, "nkb send: cleared the line (Home, Shift+End, Delete)");
                }
                ClearingOutcome::Skipped(reason) => {
                    let _ = writeln!(err, "{}", not_cleared(reason));
                }
                ClearingOutcome::NotAsked => {}
            }
            let _ = writeln!(err, "nkb send: sent {reference} - {name}");
            let _ = writeln!(
                err,
                "  {}",
                sent_counts(graphemes, code_points, bytes, utf16_units)
            );
            if let Some(expected) = about.expected() {
                let _ = writeln!(err, "  {expected}");
            }
            say_pace(err, following);
            say_text_substitutions(err, may_alter);
            if warnings > 0 {
                let _ = writeln!(
                    err,
                    "  the pack carries {warnings} warnings - run `nkb lint`"
                );
            }
            ExitCode::Ok
        }
        SendOutcome::NotFound => {
            let _ = writeln!(err, "nkb send: no pack called '{wanted}'.");
            let _ = writeln!(err, "Run 'nkb packs' to see what there is.");
            ExitCode::NotFound
        }
        SendOutcome::Unreadable { reason } => {
            let line = browse_report::unreadable("send", wanted, reason, about.file());
            let _ = writeln!(err, "{line}");
            ExitCode::IoFailed
        }
        SendOutcome::Refused { errors } => {
            let _ = writeln!(err, "{}", browse_report::not_loaded("send", wanted, errors));
            let _ = writeln!(err, "Nothing was sent. Run `nkb lint` on it.");
            ExitCode::ValidationFailed
        }
        SendOutcome::NoSuchIndex { asked, available } => {
            say_no_such_index(err, about, asked, available)
        }
        SendOutcome::ValueTooLarge { id, code } => {
            let _ = writeln!(
                err,
                "nkb send: value '{id}' describes more text than the format allows ({code})."
            );
            ExitCode::ValidationFailed
        }
        SendOutcome::RouteUnavailable { reason } => {
            let _ = writeln!(
                err,
                "nkb send: no way to deliver keystrokes on {reason} yet."
            );
            ExitCode::InsertFailed
        }
        SendOutcome::NotCleared { error } => {
            say_not_cleared(err, &error);
            ExitCode::InsertFailed
        }
        SendOutcome::Interrupted {
            facts,
            units_sent,
            units_expected,
            clearing,
            reason,
        }
        | SendOutcome::InFlight {
            facts,
            units_sent,
            units_expected,
            clearing,
            reason,
        } => {
            say_clearing_before_failure(err, clearing);
            say_cut_short(err, &facts, units_sent, units_expected, reason, in_flight);
            ExitCode::InsertFailed
        }
        SendOutcome::NotDelivered { error, clearing } => {
            say_clearing_before_failure(err, clearing);
            say_not_delivered(err, &error);
            ExitCode::InsertFailed
        }
    }
}

/// There is no value at `asked`: how many there are, and where to see them in
/// their order.
fn say_no_such_index(
    err: &mut impl Write,
    about: &About<'_>,
    asked: usize,
    available: usize,
) -> ExitCode {
    match about {
        About::Pack { name, .. } => {
            let _ = writeln!(
                err,
                "nkb send: '{name}' has {available} values, so there is no number {asked}."
            );
            let _ = writeln!(
                err,
                "Positions start at 1, like the counter in the palette."
            );
        }
        About::Made { command, .. } => {
            let _ = writeln!(
                err,
                "nkb send: the limit gives {available} values, so there is no number {asked}."
            );
            let _ = writeln!(
                err,
                "Positions start at 1, in the order `{command}` prints them."
            );
        }
    }
    ExitCode::NotFound
}

/// Counts down on standard error so the person can put the focus where they mean.
fn count_down(seconds: u64) {
    if seconds == 0 {
        return;
    }
    let mut err = crate::output::err();
    let _ = writeln!(
        err,
        "nkb send: click into the target field - sending in {seconds}s"
    );
    for left in (1..=seconds).rev() {
        let _ = write!(err, "  {left}... ");
        let _ = err.flush();
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
    let _ = writeln!(err);
}

#[cfg(test)]
#[allow(
    clippy::panic,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;
    use crate::run;
    use nkb_app::{Clearing, DeliveryError, Following, KeystrokeError, StopReason};
    use nkb_core::boundary::{Unit, values};
    use nkb_core::preview::{Recipe, ValuePreview};
    use nkb_core::substitutions::Substitutions;

    fn args(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_owned()).collect()
    }

    // ---- nkb send ---------------------------------------------------------
    //
    // ⚠️ These stop at argument parsing on purpose. Everything past that point
    // depends on the machine: macOS needs a permission, Linux needs an X11
    // session, and on a build server there may be no focused window - so a test
    // that reached the send would be green here and red on others of the three
    // systems this project is checked on. What a real send does is measured by running
    // it, and that measurement is written down rather than automated, because
    // automating it means typing into whatever window the build agent has.
    //
    // What the command SAYS about a send is another matter: `report_send` takes
    // the outcome and a writer, so every ending is checked below with no machine
    // and no window at all.

    #[test]
    fn send_without_a_pack_name_is_a_usage_error() {
        assert_eq!(run(&args(&["send"])), ExitCode::Usage);
    }

    #[test]
    fn send_rejects_an_unknown_switch_rather_than_ignoring_it() {
        assert_eq!(
            run(&args(&["send", "whitespace", "--frobnicate"])),
            ExitCode::Usage
        );
    }

    #[test]
    fn send_refuses_two_pack_names() {
        assert_eq!(
            run(&args(&["send", "whitespace", "unicode-text"])),
            ExitCode::Usage
        );
    }

    #[test]
    fn send_needs_a_number_after_index_and_delay() {
        // Missing entirely.
        assert_eq!(
            run(&args(&["send", "whitespace", "--index"])),
            ExitCode::Usage
        );
        assert_eq!(
            run(&args(&["send", "whitespace", "--delay"])),
            ExitCode::Usage
        );
        // Present but not a number - told apart from missing, and both refused.
        assert_eq!(
            run(&args(&["send", "whitespace", "--index", "first"])),
            ExitCode::Usage
        );
        assert_eq!(
            run(&args(&["send", "whitespace", "--delay", "soon"])),
            ExitCode::Usage
        );
    }

    #[test]
    fn asking_send_for_help_is_a_success_and_needs_no_machine() {
        // Untouchable rule: --help works everywhere, including where the command
        // itself could never run.
        assert_eq!(run(&args(&["send", "--help"])), ExitCode::Ok);
        assert_eq!(run(&args(&["send", "-h"])), ExitCode::Ok);
    }

    #[test]
    fn send_never_reports_success_when_it_could_not_deliver() {
        // Whatever this machine is, one thing must hold: `send` either delivers
        // or reports a failure. A zero exit code with nothing sent is the shape
        // of bug this whole command is most likely to grow, so it is asserted
        // directly rather than left to a reading of the code.
        //
        // The pack name does not exist, so even on a machine that CAN deliver,
        // nothing is typed anywhere by this test.
        let code = run(&args(&["send", "no-such-pack-at-all", "--delay", "0"]));
        assert_ne!(
            code,
            ExitCode::Ok,
            "a send that delivered nothing must not exit successfully"
        );
    }

    #[test]
    fn send_reads_its_switches_into_what_it_was_asked_to_do() {
        assert_eq!(
            send_args(&args(&["whitespace"])),
            Ok(SendArgs {
                wanted: Source::Pack("whitespace"),
                position: 1,
                delay_seconds: 3,
                clearing: Clearing::Keep,
            }),
            "the defaults are the first value, three seconds and no clearing"
        );
        assert_eq!(
            send_args(&args(&[
                "--clear",
                "--index",
                "7",
                "unicode-text",
                "--delay",
                "0"
            ])),
            Ok(SendArgs {
                wanted: Source::Pack("unicode-text"),
                position: 7,
                delay_seconds: 0,
                clearing: Clearing::Line,
            }),
            "switches are read wherever they stand, before or after the pack"
        );
    }

    fn at_most(max: u32, unit: Unit) -> Limit {
        Limit::new(unit, None, Some(max)).unwrap_or_else(|problem| panic!("{problem:?}"))
    }

    #[test]
    fn a_limit_is_read_as_nkb_boundary_reads_it_and_takes_its_value_by_index() {
        assert_eq!(
            send_args(&args(&["--max-length", "50", "--index", "7"])),
            Ok(SendArgs {
                wanted: Source::Limit(at_most(50, Unit::Graphemes)),
                position: 7,
                delay_seconds: 3,
                clearing: Clearing::Keep,
            })
        );
        assert_eq!(
            send_args(&args(&[
                "--unit",
                "bytes",
                "--clear",
                "--index",
                "2",
                "--min-length",
                "3",
                "--max-length",
                "9",
                "--delay",
                "0",
            ])),
            Ok(SendArgs {
                wanted: Source::Limit(
                    Limit::new(Unit::Bytes, Some(3), Some(9))
                        .unwrap_or_else(|problem| panic!("{problem:?}"))
                ),
                position: 2,
                delay_seconds: 0,
                clearing: Clearing::Line,
            })
        );
    }

    #[test]
    fn a_limit_without_an_index_with_a_pack_or_without_a_bound_is_a_usage_error() {
        for wrong in [
            &["--max-length", "50"][..],
            &["whitespace", "--max-length", "50", "--index", "2"],
            &["--unit", "bytes", "--index", "2"],
            &["--max-length", "+50", "--index", "2"],
            &["--max-length", "5", "--max-length", "6", "--index", "2"],
            &["--min-length", "6", "--max-length", "5", "--index", "2"],
            &["--index", "2", "--max-length"],
            &["--unit", "chars", "--max-length", "5", "--index", "1"],
        ] {
            assert_eq!(send_args(&args(wrong)), Err(ExitCode::Usage), "{wrong:?}");
        }
    }

    #[test]
    fn a_limit_refuses_a_team_folder_and_a_number_past_its_values_before_any_countdown() {
        // Neither touches the machine: the folder is refused as a wrong call,
        // and the set is counted without a window, so the countdown of three
        // seconds is never waited out.
        let started = std::time::Instant::now();
        assert_eq!(
            run(&args(&[
                "send",
                "--team-folder",
                "qa packs",
                "--max-length",
                "5",
                "--index",
                "1",
            ])),
            ExitCode::Usage
        );
        let count = values(at_most(5, Unit::Graphemes)).len();
        let past = (count + 1).to_string();
        assert_eq!(
            run(&args(&["send", "--max-length", "5", "--index", &past])),
            ExitCode::NotFound
        );
        assert_eq!(
            run(&args(&["send", "--max-length", "5", "--index", "0"])),
            ExitCode::NotFound
        );
        assert!(started.elapsed() < std::time::Duration::from_secs(2));
    }

    #[test]
    fn a_value_made_for_a_limit_is_named_with_what_to_expect_and_a_missing_one_with_its_command() {
        let limit = at_most(50, Unit::Bytes);
        let set = MadeSet::new(limit, &EnglishBoundary);
        let value = &set.pack().values[1];
        let about = About::Made {
            set: &set,
            position: 2,
            command: command(limit),
        };
        let mut said = Vec::new();
        let code = report_send(
            &mut said,
            &about,
            SendOutcome::Sent {
                facts: ValueFacts {
                    reference: format!("boundary/{}", value.id),
                    name: value.name.clone(),
                    ..some_facts()
                },
                utf16_units: 1,
                clearing: ClearingOutcome::NotAsked,
                following: Following::Application,
                may_alter: Substitutions::NONE,
            },
        );
        let said = String::from_utf8(said).unwrap_or_else(|e| panic!("not UTF-8: {e}"));
        assert_eq!(code, ExitCode::Ok);
        let expected = value
            .expect
            .as_deref()
            .unwrap_or_else(|| panic!("a made value says what to expect"));
        assert!(
            said.contains(&format!(
                "nkb send: sent boundary/{} - {}\n",
                value.id, value.name
            )),
            "{said}"
        );
        assert!(said.contains(&format!("\n  {expected}\n")), "{said}");

        let mut said = Vec::new();
        let code = report_send(
            &mut said,
            &about,
            SendOutcome::NoSuchIndex {
                asked: 40,
                available: 19,
            },
        );
        let said = String::from_utf8(said).unwrap_or_else(|e| panic!("not UTF-8: {e}"));
        assert_eq!(code, ExitCode::NotFound);
        assert_eq!(
            said,
            "nkb send: the limit gives 19 values, so there is no number 40.\n\
             Positions start at 1, in the order `nkb boundary --max-length 50 --unit bytes` prints them.\n"
        );
        let (pack, _) = reported(SendOutcome::NoSuchIndex {
            asked: 9,
            available: 3,
        });
        assert!(pack.contains("like the counter in the palette"), "{pack}");
    }

    /// A pack the built-in catalogue would hold: no file to name.
    fn some_pack() -> About<'static> {
        About::Pack {
            name: "some-pack",
            file: None,
        }
    }

    /// What `report_send` said, and the code it ended with.
    fn reported(outcome: SendOutcome) -> (String, ExitCode) {
        let mut said = Vec::new();
        let code = report_send(&mut said, &some_pack(), outcome);
        let said = String::from_utf8(said).unwrap_or_else(|e| panic!("not UTF-8: {e}"));
        (said, code)
    }

    fn some_facts() -> ValueFacts {
        ValueFacts {
            reference: "some-pack/some-value".to_owned(),
            name: "Some value".to_owned(),
            graphemes: 1,
            code_points: 1,
            bytes: 1,
            warnings: 0,
            preview: ValuePreview::Recipe(Recipe {
                unit: "a".to_owned(),
                count: 1,
            }),
            shape: Vec::new(),
        }
    }

    #[test]
    fn a_send_the_system_may_have_changed_says_which_substitutions_could_have_acted() {
        // `D139`: nothing is said when nothing could have acted, and when
        // something could, the note names the services in words the person finds
        // in the keyboard settings - on stderr, the exit code unchanged.
        let sent = |may_alter: Substitutions| SendOutcome::Sent {
            facts: some_facts(),
            utf16_units: 1,
            clearing: ClearingOutcome::NotAsked,
            following: Following::Application,
            may_alter,
        };
        let (quiet, code) = reported(sent(Substitutions::NONE));
        assert_eq!(code, ExitCode::Ok);
        assert!(!quiet.contains("text substitutions"), "{quiet}");

        let (said, code) = reported(sent(Substitutions {
            capitalization: true,
            period: true,
            ..Substitutions::NONE
        }));
        assert_eq!(code, ExitCode::Ok);
        assert!(
            said.contains("the system's text substitutions may have changed what went in: the first letter of a sentence made a capital and a period added after two spaces - "),
            "{said}"
        );
    }

    #[test]
    fn keys_still_on_their_way_name_the_value_and_ask_to_look_not_to_send_again() {
        // `D158`: none known to have arrived is a floor, not "nothing arrived".
        let mut said = Vec::new();
        let code = report_send(
            &mut said,
            &some_pack(),
            SendOutcome::InFlight {
                facts: some_facts(),
                units_sent: 0,
                units_expected: 2,
                clearing: ClearingOutcome::NotAsked,
                reason: StopReason::NotTaking,
            },
        );
        let said = String::from_utf8(said).unwrap_or_else(|e| panic!("not UTF-8: {e}"));
        assert_eq!(code, ExitCode::InsertFailed);
        assert!(
            said.contains("nkb send: interrupted some-pack/some-value - Some value"),
            "{said}"
        );
        assert!(
            said.contains("at least 0 of 2 UTF-16 units arrived - the application stopped taking keys, and more may still arrive."),
            "{said}"
        );
        assert!(said.contains("Check the field and clear it"), "{said}");
        assert!(!said.contains("PARTIAL"), "{said}");
    }

    #[test]
    fn every_ending_of_a_send_has_its_exit_code_and_only_a_delivery_says_sent() {
        // The exit codes are the table every pipeline reads (`D46`), and a zero
        // with nothing delivered is the bug this command is most likely to grow.
        let reason = StopReason::FocusMoved;
        let endings = [
            (
                SendOutcome::Sent {
                    facts: some_facts(),
                    utf16_units: 1,
                    clearing: ClearingOutcome::Done,
                    following: Following::Application,
                    may_alter: Substitutions::NONE,
                },
                ExitCode::Ok,
            ),
            (
                SendOutcome::Interrupted {
                    facts: some_facts(),
                    units_sent: 1,
                    units_expected: 2,
                    clearing: ClearingOutcome::Done,
                    reason,
                },
                ExitCode::InsertFailed,
            ),
            (
                SendOutcome::InFlight {
                    facts: some_facts(),
                    units_sent: 0,
                    units_expected: 2,
                    clearing: ClearingOutcome::Done,
                    reason,
                },
                ExitCode::InsertFailed,
            ),
            (SendOutcome::NotFound, ExitCode::NotFound),
            (
                SendOutcome::Unreadable {
                    reason: SourceError::NotUtf8,
                },
                ExitCode::IoFailed,
            ),
            (
                SendOutcome::Refused { errors: 2 },
                ExitCode::ValidationFailed,
            ),
            (
                SendOutcome::NoSuchIndex {
                    asked: 9,
                    available: 3,
                },
                ExitCode::NotFound,
            ),
            (
                SendOutcome::ValueTooLarge {
                    id: "huge".to_owned(),
                    code: "E026",
                },
                ExitCode::ValidationFailed,
            ),
            (
                SendOutcome::RouteUnavailable {
                    reason: "this system".to_owned(),
                },
                ExitCode::InsertFailed,
            ),
            (
                SendOutcome::NotCleared {
                    error: KeystrokeError::NoTarget,
                },
                ExitCode::InsertFailed,
            ),
            (
                SendOutcome::NotDelivered {
                    error: DeliveryError::NothingArrived { reason },
                    clearing: ClearingOutcome::Done,
                },
                ExitCode::InsertFailed,
            ),
        ];
        for (outcome, expected) in endings {
            let shown = format!("{outcome:?}");
            let (said, code) = reported(outcome);
            assert_eq!(code, expected, "{shown} ended with the wrong code:\n{said}");
            assert!(
                said.starts_with("nkb send: "),
                "{shown} did not open with the command's name:\n{said}"
            );
            assert_eq!(
                said.contains("nkb send: sent "),
                code == ExitCode::Ok,
                "{shown} - only a value that arrived may be called sent:\n{said}"
            );
        }
    }

    #[test]
    fn a_cleared_line_is_said_before_the_news_that_the_value_did_not_arrive() {
        // The field is empty-plus-nothing, not as the tester left it, and the
        // order of the two sentences is the order of what happened to it.
        let (said, _) = reported(SendOutcome::NotDelivered {
            error: DeliveryError::NoTarget,
            clearing: ClearingOutcome::Done,
        });
        let cleared = said.find("the line was cleared");
        let focus = said.find("nothing holds the keyboard focus");
        assert!(
            cleared.is_some() && focus.is_some() && cleared < focus,
            "the clearing and the failure are both said, in that order:\n{said}"
        );
    }

    #[test]
    fn every_failure_to_clear_or_to_deliver_has_a_sentence_of_its_own() {
        let reason = StopReason::Dropped;
        let not_cleared = [
            KeystrokeError::Unsupported {
                system: "this system".to_owned(),
            },
            KeystrokeError::NoTarget,
            KeystrokeError::ModifierHeld {
                which: "Shift".to_owned(),
            },
            KeystrokeError::Partial {
                chords_sent: 1,
                chords_expected: 3,
                reason,
            },
            KeystrokeError::NothingArrived { reason },
            KeystrokeError::HigherPrivileges,
            KeystrokeError::NoTextField,
            KeystrokeError::FieldUnconfirmed,
            KeystrokeError::InTerminal,
            KeystrokeError::NoClearingKeys,
        ];
        let mut seen = std::collections::BTreeSet::new();
        for error in &not_cleared {
            let mut said = Vec::new();
            say_not_cleared(&mut said, error);
            let said = String::from_utf8(said).unwrap_or_else(|e| panic!("not UTF-8: {e}"));
            assert!(said.starts_with("nkb send: "), "{error:?} said: {said}");
            assert!(
                seen.insert(said.clone()),
                "{error:?} says what another failure says: {said}"
            );
        }

        let not_delivered = [
            DeliveryError::Unsupported {
                system: "this system".to_owned(),
            },
            DeliveryError::NoTarget,
            DeliveryError::ModifierHeld {
                which: "Shift".to_owned(),
            },
            DeliveryError::Partial {
                units_sent: 1,
                units_expected: 3,
                reason,
            },
            DeliveryError::NothingArrived { reason },
            DeliveryError::Busy,
            DeliveryError::Refused {
                detail: "the route said no".to_owned(),
            },
            DeliveryError::CannotCarry { character: '\0' },
            DeliveryError::HigherPrivileges,
            DeliveryError::NoTextField,
            DeliveryError::CapsLockOn,
        ];
        let mut seen = std::collections::BTreeSet::new();
        for error in &not_delivered {
            let mut said = Vec::new();
            say_not_delivered(&mut said, error);
            let said = String::from_utf8(said).unwrap_or_else(|e| panic!("not UTF-8: {e}"));
            assert!(said.starts_with("nkb send: "), "{error:?} said: {said}");
            assert!(
                seen.insert(said.clone()),
                "{error:?} says what another failure says: {said}"
            );
        }
    }
}
