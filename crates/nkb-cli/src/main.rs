//! `nkb` - the command line tool.
//!
//! This binary contains no graphical library at all. Not lazily loaded, not
//! conditionally linked: absent. That is what makes it usable inside a
//! container with no screen, and it is a property of the artifact rather than a
//! discipline someone has to maintain.
//!
//! The graphical interface lives in a separate executable and is a full product
//! of its own - neither one is a reduced version of the other.

#![forbid(unsafe_code)]

mod browse_report;
mod clearing;
mod emit_output;
mod english;
mod exit;
mod json;
mod lint_json;
mod lint_report;
mod on_the_way;
mod output;
mod refusals;
mod sent_notes;
mod target_line;

use clearing::{not_cleared, say_clearing_before_failure};
use emit_output::{EmitFormat, Form};
use exit::ExitCode;
use nkb_adapters::{
    BuiltInCatalogue, DirectInjection, DirectoryPackSink, DirectoryPackSource, SystemClock,
    TomlPackFormat,
};
use nkb_app::{
    Availability, Clearing, ClearingOutcome, Clock, EmitOutcome, FormatOutcome, LintOutcome,
    NewPackOutcome, OnTheWay, SendOutcome, SendRequest, ShowOutcome, TargetInspector,
    ValueDelivery, ValueFacts, emit_values, format_pack, lint_pack, list_packs, new_pack,
    send_value, show_pack,
};
use output::{out, outln};
use refusals::{NEEDS_PERMISSION, say_not_cleared, say_not_delivered, stop_reason};
use sent_notes::{say_pace, say_text_substitutions};
use std::io::Write;
use std::path::Path;
use target_line::{Countdown, say_where_it_goes, where_it_goes};

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // What the command found, unless its answer could not be written (`output`).
    let code = output::settle(run(&args));
    std::process::ExitCode::from(u8::try_from(code.as_i32()).unwrap_or(2))
}

fn run(args: &[String]) -> ExitCode {
    // Asking for help is a success, not a usage error. A bare invocation is a
    // question - "what does this do" - and it is the first thing anyone types.
    if args.is_empty() {
        print_help();
        return ExitCode::Ok;
    }

    match args[0].as_str() {
        "-h" | "--help" | "help" => {
            print_help();
            ExitCode::Ok
        }
        "--version" => {
            // Data goes to standard output, everything else to standard error,
            // so a pipeline gets the value and nothing but the value.
            outln!("nkb {VERSION}");
            ExitCode::Ok
        }
        "packs" => packs(&args[1..]),
        "show" => show(&args[1..]),
        "emit" => emit(&args[1..]),
        "send" => send(&args[1..]),
        "lint" => lint(&args[1..]),
        "fmt" => fmt(&args[1..]),
        "new-pack" => new_pack_command(&args[1..]),
        unknown => {
            let mut err = std::io::stderr();
            let _ = writeln!(err, "nkb: unknown command '{unknown}'");
            let _ = writeln!(err, "Run 'nkb --help' to see what is available.");
            ExitCode::Usage
        }
    }
}

/// `nkb lint <file>` - validate one pack file.
///
/// The verdict goes to standard output even when the pack fails, because a
/// verdict is a description rather than a product: it has something to say
/// precisely when the answer is no. The rule about a failed run writing nothing
/// to standard output protects an artifact that would otherwise look complete,
/// and this is not one.
fn lint(args: &[String]) -> ExitCode {
    let mut path: Option<&str> = None;
    let mut explain = false;
    let mut json = false;

    for arg in args {
        match arg.as_str() {
            "--explain" => explain = true,
            "--json" => json = true,
            "-h" | "--help" => {
                print_lint_help();
                return ExitCode::Ok;
            }
            other if other.starts_with('-') => {
                let mut err = std::io::stderr();
                let _ = writeln!(err, "nkb lint: unknown option '{other}'");
                let _ = writeln!(err, "Run 'nkb lint --help' to see what is available.");
                return ExitCode::Usage;
            }
            other => path = Some(other),
        }
    }

    // Explaining the rules is a question about the tool rather than about a file,
    // so it answers on its own. Asked for machine readable output it answers in
    // the same document as everything else, with no files in it - a second shape
    // for this one case would double the work of every consumer.
    if explain && path.is_none() {
        if json {
            out!("{}", lint_json::document(&[], VERSION).render());
        } else {
            for line in lint_report::explanation() {
                outln!("{line}");
            }
        }
        return ExitCode::Ok;
    }

    // In machine readable mode standard output carries the document and nothing
    // else. One stray line of prose beside it and the consumer's parse fails.
    if explain && !json {
        for line in lint_report::explanation() {
            outln!("{line}");
        }
        outln!();
    }

    let Some(path) = path else {
        let mut err = std::io::stderr();
        let _ = writeln!(err, "nkb lint: name a pack file to check.");
        let _ = writeln!(err, "Usage: nkb lint <file.toml> [--json] [--explain]");
        return ExitCode::Usage;
    };

    let Some((source, id)) = DirectoryPackSource::split(Path::new(path)) else {
        let mut err = std::io::stderr();
        let _ = writeln!(err, "nkb lint: '{path}' does not name a file.");
        return ExitCode::Usage;
    };

    let outcome = lint_pack(&source, &TomlPackFormat, &id);

    if json {
        out!(
            "{}",
            lint_json::document(&[(path.to_owned(), outcome.clone())], VERSION).render()
        );
        return match &outcome {
            LintOutcome::Judged(report) if report.accepted() => ExitCode::Ok,
            LintOutcome::Judged(_) => ExitCode::ValidationFailed,
            _ => ExitCode::NotFound,
        };
    }

    match outcome {
        LintOutcome::Judged(report) => {
            for line in lint_report::lines(&report, path) {
                outln!("{line}");
            }
            for line in lint_report::summary(&report) {
                outln!("{line}");
            }
            if report.accepted() {
                ExitCode::Ok
            } else {
                ExitCode::ValidationFailed
            }
        }
        LintOutcome::NotFound => {
            let mut err = std::io::stderr();
            let _ = writeln!(err, "nkb lint: no pack file at '{path}'.");
            ExitCode::NotFound
        }
        LintOutcome::Unreadable => {
            let mut err = std::io::stderr();
            let _ = writeln!(
                err,
                "nkb lint: '{path}' is there and could not be read. Check the file's permissions."
            );
            ExitCode::IoFailed
        }
    }
}

/// `nkb fmt <file>` - write one pack file in canonical shape.
///
/// # Why this exists instead of a style guide
///
/// A pack file is edited by people and rewritten by this tool, and a format in
/// that position needs one canonical shape or every machine write produces a
/// whole-file difference. With it, a difference in a pull request is always a
/// difference of content, and review is about the value and the sentence beside
/// it rather than about the column an equals sign sits in.
fn fmt(args: &[String]) -> ExitCode {
    let mut path: Option<&str> = None;
    let mut dry_run = false;

    for arg in args {
        match arg.as_str() {
            "--dry-run" => dry_run = true,
            "-h" | "--help" => {
                print_fmt_help();
                return ExitCode::Ok;
            }
            other if other.starts_with('-') => {
                let mut err = std::io::stderr();
                let _ = writeln!(err, "nkb fmt: unknown option '{other}'");
                let _ = writeln!(err, "Run 'nkb fmt --help' to see what is available.");
                return ExitCode::Usage;
            }
            other => path = Some(other),
        }
    }

    let Some(path) = path else {
        let mut err = std::io::stderr();
        let _ = writeln!(err, "nkb fmt: name a pack file to format.");
        let _ = writeln!(err, "Usage: nkb fmt <file.toml> [--dry-run]");
        return ExitCode::Usage;
    };

    let Some((source, id)) = DirectoryPackSource::split(Path::new(path)) else {
        let mut err = std::io::stderr();
        let _ = writeln!(err, "nkb fmt: '{path}' does not name a file.");
        return ExitCode::Usage;
    };
    let sink = DirectoryPackSink::new(source.folder());

    match format_pack(&source, &sink, &TomlPackFormat, &id, dry_run) {
        FormatOutcome::AlreadyCanonical => {
            outln!("{path} is already in shape. Nothing was written.");
            ExitCode::Ok
        }
        FormatOutcome::Formatted => {
            outln!("Wrote {path} in canonical shape.");
            ExitCode::Ok
        }
        FormatOutcome::WouldFormat => {
            // A dry run reports on standard output because the report is what it
            // was asked for. It is the product of this run, not a complaint.
            outln!("{path} is not in shape. Run without --dry-run to rewrite it.");
            ExitCode::Ok
        }
        FormatOutcome::DidNotParse => {
            let mut err = std::io::stderr();
            let _ = writeln!(
                err,
                "nkb fmt: '{path}' is not a pack file this tool can read, so there is nothing to put in order."
            );
            let _ = writeln!(err, "Run `nkb lint {path}` to see what is wrong with it.");
            ExitCode::ValidationFailed
        }
        FormatOutcome::NotFound => {
            let mut err = std::io::stderr();
            let _ = writeln!(err, "nkb fmt: no pack file at '{path}'.");
            ExitCode::NotFound
        }
        FormatOutcome::Unreadable => {
            let mut err = std::io::stderr();
            let _ = writeln!(
                err,
                "nkb fmt: '{path}' is there and could not be read. Check the file's permissions."
            );
            ExitCode::IoFailed
        }
        FormatOutcome::Unwritable => {
            let mut err = std::io::stderr();
            let _ = writeln!(
                err,
                "nkb fmt: '{path}' could not be written. Check that you can write to this folder - the file has not been changed."
            );
            ExitCode::IoFailed
        }
        FormatOutcome::WouldChangeValues => {
            // 🔴 A fault in this tool, not in the file, and the loudest thing the
            // command can say. Nothing was written.
            let mut err = std::io::stderr();
            let _ = writeln!(
                err,
                "nkb fmt: formatting '{path}' would change what one of its values inserts, so nothing was written."
            );
            let _ = writeln!(
                err,
                "This is a fault in Naughty Keyboard rather than in your pack. Please report it with the file attached."
            );
            ExitCode::InsertFailed
        }
    }
}

/// `nkb new-pack <name>` - write the skeleton for a new pack.
///
/// # Why it takes a name and not a path
///
/// The contributor path in the published guide runs `nkb new-pack locale-cz` and
/// then `nkb lint locale-cz.toml`, in the folder they are standing in. A path
/// would let the first command write somewhere the second one does not look, and
/// buy nothing: a pack file has to sit beside the pack it may translate anyway.
fn new_pack_command(args: &[String]) -> ExitCode {
    let mut name: Option<&str> = None;

    for arg in args {
        match arg.as_str() {
            "-h" | "--help" => {
                print_new_pack_help();
                return ExitCode::Ok;
            }
            other if other.starts_with('-') => {
                let mut err = std::io::stderr();
                let _ = writeln!(err, "nkb new-pack: unknown option '{other}'");
                let _ = writeln!(err, "Run 'nkb new-pack --help' to see what is available.");
                return ExitCode::Usage;
            }
            other => name = Some(other),
        }
    }

    let Some(name) = name else {
        let mut err = std::io::stderr();
        let _ = writeln!(err, "nkb new-pack: name the pack you want to start.");
        let _ = writeln!(err, "Usage: nkb new-pack <name>");
        return ExitCode::Usage;
    };

    let sink = DirectoryPackSink::new(".");
    match new_pack(&sink, &TomlPackFormat, &SystemClock, name) {
        NewPackOutcome::Created { file } => {
            // The command's answer, on standard output the way the linter's
            // verdict is. Nothing else is written there, so nothing is polluted.
            outln!("Wrote {file}.");
            outln!("Edit it, then run `nkb lint {file}` - it reports everything in one pass.");
            outln!("The pack format is not frozen yet: a later release may still change it.");
            ExitCode::Ok
        }
        NewPackOutcome::NameNotAnIdentifier => {
            let mut err = std::io::stderr();
            let _ = writeln!(
                err,
                "nkb new-pack: '{name}' is not the shape a pack name has: lower case letters, digits and hyphens, starting with a letter, two to forty characters."
            );
            let _ = writeln!(err, "Nothing was written.");
            ExitCode::Usage
        }
        NewPackOutcome::AlreadyExists { file } => {
            // 🔴 Never an overwrite, and the message says the file is untouched
            // rather than leaving the reader to wonder.
            let mut err = std::io::stderr();
            let _ = writeln!(
                err,
                "nkb new-pack: {file} is already here and has not been touched. Pick another name, or move that file first."
            );
            ExitCode::Usage
        }
        NewPackOutcome::Unwritable { file } => {
            let mut err = std::io::stderr();
            let _ = writeln!(
                err,
                "nkb new-pack: {file} could not be written here. Check that this folder exists and that you can write to it."
            );
            ExitCode::IoFailed
        }
    }
}

/// Help is written to standard output and never requires loading the catalogue.
/// Someone reaching for `--help` may well be doing it because the catalogue is
/// what is broken.
/// `nkb packs` - what the tool can offer.
///
/// The listing is data and goes to standard output. The account of which
/// sources were consulted goes there too rather than to standard error, and
/// that is deliberate: it is part of the answer, not a diagnostic about it. A
/// list of one source out of three read without that line is a false statement,
/// and standard error is exactly where a pipeline drops things.
fn packs(args: &[String]) -> ExitCode {
    // `first` rather than a loop: every branch below returns, so a loop would be
    // one that never loops, and the compiler is right to say so.
    match args.first().map(String::as_str) {
        None => {}
        Some("-h" | "--help") => {
            print_packs_help();
            return ExitCode::Ok;
        }
        Some(other) => {
            let mut err = std::io::stderr();
            let _ = writeln!(err, "nkb packs: unexpected argument '{other}'");
            let _ = writeln!(err, "Run 'nkb packs --help' to see what is available.");
            return ExitCode::Usage;
        }
    }

    let catalogue = BuiltInCatalogue::new();
    let format = TomlPackFormat;

    match list_packs(&catalogue, &format) {
        Ok(found) => {
            for line in browse_report::listing(&found) {
                outln!("{line}");
            }
            // A catalogue holding a pack nobody can load is not a failed run:
            // the run succeeded and the answer includes the bad news. `nkb lint`
            // is the command whose exit code is a verdict on a pack.
            ExitCode::Ok
        }
        Err(reason) => {
            let mut err = std::io::stderr();
            let _ = writeln!(
                err,
                "nkb packs: the catalogue could not be examined ({reason})"
            );
            ExitCode::IoFailed
        }
    }
}

/// `nkb show <pack>` - one pack in full.
fn show(args: &[String]) -> ExitCode {
    let mut wanted: Option<&str> = None;

    for arg in args {
        match arg.as_str() {
            "-h" | "--help" => {
                print_show_help();
                return ExitCode::Ok;
            }
            other if other.starts_with('-') => {
                let mut err = std::io::stderr();
                let _ = writeln!(err, "nkb show: unknown option '{other}'");
                let _ = writeln!(err, "Run 'nkb show --help' to see what is available.");
                return ExitCode::Usage;
            }
            other => wanted = Some(other),
        }
    }

    let Some(wanted) = wanted else {
        let mut err = std::io::stderr();
        let _ = writeln!(err, "nkb show: name a pack.");
        let _ = writeln!(err, "Run 'nkb packs' to see what there is.");
        return ExitCode::Usage;
    };

    let catalogue = BuiltInCatalogue::new();
    let format = TomlPackFormat;

    match show_pack(&catalogue, &format, wanted) {
        ShowOutcome::Shown { pack, warnings } => {
            for line in browse_report::pack(&pack, warnings) {
                outln!("{line}");
            }
            ExitCode::Ok
        }
        ShowOutcome::Refused { errors } => {
            let mut err = std::io::stderr();
            let _ = writeln!(
                err,
                "nkb show: '{wanted}' has {errors} errors and is not loaded at all."
            );
            let _ = writeln!(
                err,
                "A pack is whole or absent - run `nkb lint` on it to see what is wrong."
            );
            ExitCode::ValidationFailed
        }
        ShowOutcome::NotFound => {
            let mut err = std::io::stderr();
            let _ = writeln!(err, "nkb show: no pack called '{wanted}'.");
            let _ = writeln!(err, "Run 'nkb packs' to see what there is.");
            ExitCode::NotFound
        }
        ShowOutcome::Unreadable => {
            let mut err = std::io::stderr();
            let _ = writeln!(err, "nkb show: '{wanted}' could not be read.");
            ExitCode::IoFailed
        }
    }
}

fn print_packs_help() {
    outln!("nkb packs - list the packs this build can offer");
    outln!();
    outln!("Usage:");
    outln!("  nkb packs");
    outln!();
    outln!("Every run also states which pack sources were read and which were not,");
    outln!("because a list drawn from one source looks exactly like a complete one.");
    outln!();
    outln!("Options:");
    outln!("  -h, --help     Show this help and exit with 0");
}

/// Which switch is still waiting for its number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SendSwitch {
    Index,
    Delay,
}

impl SendSwitch {
    /// The switch as the person typed it, for the sentence that says what is
    /// missing after it.
    const fn name(self) -> &'static str {
        match self {
            Self::Index => "--index",
            Self::Delay => "--delay",
        }
    }
}

/// What `nkb send` was asked to do, read from its arguments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SendArgs<'a> {
    wanted: &'a str,
    position: u64,
    delay_seconds: u64,
    clearing: Clearing,
}

/// `nkb send <pack> [--index N] [--delay S] [--clear]` - put one value into the
/// focused field.
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
/// Standard output stays EMPTY here, deliberately. `09-CLI-I-CI.md` gives stdout
/// to data, and this command's data does not come back to the caller - it goes
/// into somebody else's window. Everything a person reads is on standard error.
fn send(args: &[String]) -> ExitCode {
    let asked = match send_args(args) {
        Ok(asked) => asked,
        Err(code) => return code,
    };
    let wanted = asked.wanted;

    let delivery = DirectInjection;
    if let Err(code) = wait_for_a_target(&delivery, wanted, asked.delay_seconds) {
        return code;
    }

    let catalogue = BuiltInCatalogue::new();
    let request = SendRequest {
        pack_id: wanted,
        position: usize::try_from(asked.position).unwrap_or(usize::MAX),
        clearing: asked.clearing,
        // Today in UTC, for a `next-date` value (`D133`).
        on: SystemClock.today(),
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
            let _ = writeln!(std::io::stderr(), "{line}");
        }
    };
    let outcome = send_value(
        &catalogue,
        &TomlPackFormat,
        &delivery,
        &delivery,
        &request,
        &mut on_the_way,
    );
    report_send(&mut std::io::stderr(), wanted, outcome)
}

/// The arguments of `nkb send`, or the exit code to stop with.
///
/// `Err` means the command is over and what there was to say has been said: a
/// usage error on standard error, or the help on standard output - which is why
/// `--help` comes back as `Err(ExitCode::Ok)`.
fn send_args(args: &[String]) -> Result<SendArgs<'_>, ExitCode> {
    let mut wanted: Option<&str> = None;
    let mut position: u64 = 1;
    let mut delay_seconds: u64 = 3;
    let mut clearing = Clearing::Keep;
    let mut expecting: Option<SendSwitch> = None;

    for arg in args {
        if let Some(which) = expecting {
            let name = which.name();
            let Ok(number) = arg.parse::<u64>() else {
                let mut err = std::io::stderr();
                let _ = writeln!(err, "nkb send: {name} needs a whole number, got '{arg}'.");
                return Err(ExitCode::Usage);
            };
            match which {
                SendSwitch::Index => position = number,
                SendSwitch::Delay => delay_seconds = number,
            }
            expecting = None;
            continue;
        }
        match arg.as_str() {
            "-h" | "--help" => {
                print_send_help();
                return Err(ExitCode::Ok);
            }
            "--index" => expecting = Some(SendSwitch::Index),
            "--delay" => expecting = Some(SendSwitch::Delay),
            "--clear" => clearing = Clearing::Line,
            other if other.starts_with('-') => {
                let mut err = std::io::stderr();
                let _ = writeln!(err, "nkb send: unknown switch '{other}'");
                let _ = writeln!(err, "Run 'nkb send --help' to see what is available.");
                return Err(ExitCode::Usage);
            }
            name => {
                if wanted.is_some() {
                    let mut err = std::io::stderr();
                    let _ = writeln!(err, "nkb send: name one pack, not two.");
                    return Err(ExitCode::Usage);
                }
                wanted = Some(name);
            }
        }
    }

    if let Some(which) = expecting {
        let name = which.name();
        let mut err = std::io::stderr();
        let _ = writeln!(err, "nkb send: {name} needs a number.");
        return Err(ExitCode::Usage);
    }

    let Some(wanted) = wanted else {
        let mut err = std::io::stderr();
        let _ = writeln!(err, "nkb send: name a pack.");
        let _ = writeln!(err, "Run 'nkb packs' to see what there is.");
        return Err(ExitCode::Usage);
    };

    Ok(SendArgs {
        wanted,
        position,
        delay_seconds,
        clearing,
    })
}

/// Asks whether this system can send at all, counts down and says where the
/// value is about to go - or gives the exit code to stop with, having said why.
fn wait_for_a_target(
    delivery: &DirectInjection,
    wanted: &str,
    delay_seconds: u64,
) -> Result<(), ExitCode> {
    // Asked before the countdown, so a machine with no route does not make
    // somebody watch three seconds tick away for nothing.
    match delivery.availability() {
        Availability::Ready => {}
        Availability::Unavailable { reason } => {
            let mut err = std::io::stderr();
            let _ = writeln!(
                err,
                "nkb send: no way to deliver keystrokes on {reason} yet."
            );
            let _ = writeln!(
                err,
                "Nothing was sent. `nkb emit {wanted}` writes the values instead."
            );
            return Err(ExitCode::InsertFailed);
        }
        Availability::NeedsPermission => {
            let mut err = std::io::stderr();
            let _ = writeln!(err, "{NEEDS_PERMISSION}");
            let _ = writeln!(
                err,
                "`nkb emit {wanted}` writes the values instead, for as long as it is off."
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
        &mut std::io::stderr(),
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
fn report_send(err: &mut impl Write, wanted: &str, outcome: SendOutcome) -> ExitCode {
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
        SendOutcome::Unreadable => {
            let _ = writeln!(err, "nkb send: '{wanted}' could not be read.");
            ExitCode::IoFailed
        }
        SendOutcome::Refused { errors } => {
            let _ = writeln!(
                err,
                "nkb send: '{wanted}' has {errors} errors and is not loaded at all."
            );
            let _ = writeln!(err, "Nothing was sent. Run `nkb lint` on it.");
            ExitCode::ValidationFailed
        }
        SendOutcome::NoSuchIndex { asked, available } => {
            let _ = writeln!(
                err,
                "nkb send: '{wanted}' has {available} values, so there is no number {asked}."
            );
            let _ = writeln!(
                err,
                "Positions start at 1, like the counter in the palette."
            );
            ExitCode::NotFound
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
        } => {
            say_clearing_before_failure(err, clearing);
            // The loudest message in this command on purpose: the field now
            // holds a fragment, and a person who does not know that will report
            // the fragment as the application's doing. It names the value, as a
            // value that arrived whole is named - until `OBS-126` a fragment was
            // the one outcome that did not say whose it was.
            let _ = writeln!(
                err,
                "nkb send: interrupted {} - {}",
                facts.reference, facts.name
            );
            let _ = writeln!(
                err,
                "  only {units_sent} of {units_expected} UTF-16 units arrived - {}.",
                stop_reason(reason)
            );
            let _ = writeln!(
                err,
                "The field holds a PARTIAL value. Clear it before testing."
            );
            ExitCode::InsertFailed
        }
        SendOutcome::NotDelivered { error, clearing } => {
            say_clearing_before_failure(err, clearing);
            say_not_delivered(err, &error);
            ExitCode::InsertFailed
        }
    }
}

/// Counts down on standard error so the person can put the focus where they mean.
/// How much text `nkb send` sent, in the four units that differ.
///
/// Four, because the difference is the point: a family emoji is one cluster,
/// five code points and eighteen bytes, and characters above the basic plane
/// cross as two units each. In the singular where the count is one - "1
/// graphemes" stood here until 2026-09-24, next to the shortest values in the
/// catalogue (`OBS-120`).
fn sent_counts(graphemes: usize, code_points: usize, bytes: usize, utf16_units: usize) -> String {
    format!(
        "{}, {}, {}, {}",
        english::plural(graphemes, "grapheme"),
        english::plural(code_points, "code point"),
        english::plural(bytes, "byte"),
        english::plural(utf16_units, "UTF-16 unit")
    )
}

fn count_down(seconds: u64) {
    if seconds == 0 {
        return;
    }
    let mut err = std::io::stderr();
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

fn print_send_help() {
    outln!("nkb send - type one value into the focused field");
    outln!();
    outln!("Usage:");
    outln!("  nkb send <pack> [--index N] [--delay S] [--clear]");
    outln!();
    outln!("Put one value of a pack into whatever field has the keyboard focus.");
    outln!();
    outln!("Options:");
    outln!("  --index N      which value, counting from 1 in the pack's own order");
    outln!("                 (default: 1). The order of values is the order of testing.");
    outln!("  --delay S      seconds to wait first, so you can focus the target");
    outln!("                 (default: 3). Use 0 when something else focuses it.");
    outln!("  --clear        press Home, Shift+End, Delete first, clearing the current");
    outln!("                 line of the field. Never selects beyond the line, so a");
    outln!("                 multi-line field keeps its other lines. Without this switch");
    outln!("                 the only keys sent are the value's own characters.");
    outln!("  -h, --help     Show this help and exit with 0");
    outln!();
    outln!("The value goes to the focused window, not to standard output.");
    outln!("Everything you read here is on standard error.");
    outln!("Nothing is sent while Ctrl, Alt, Shift or Win is held on the keyboard:");
    outln!("the command waits up to two seconds for them to come up, then refuses.");
    outln!("Nothing is sent to a window running with higher privileges than nkb,");
    outln!("such as an application started as administrator: the system would drop");
    outln!("the keystrokes without a word, so the command refuses and exits with 4.");
    outln!("Nothing is sent when the keyboard focus is on a button, a link, a list item");
    outln!("or a page that is not editable: keys there act on that control, so the");
    outln!("command refuses and exits with 4. Where it cannot tell, it sends.");
    outln!("Press Escape to stop a send in progress: nothing more is typed, the command");
    outln!("says how much arrived and exits with 4. Escape belongs to the command only");
    outln!("while it types - before and after, the key is the application's.");
}

/// `nkb emit <pack> [--format json|csv|lines] [--escaped|--raw] [--base64]`
///
/// The command that takes the catalogue out of the tool. Everything it prints on
/// standard output is values. The account of what came out, and every note about
/// what a format could not carry, goes to the error stream - `ux-spec.md` 10.
fn emit(args: &[String]) -> ExitCode {
    let mut wanted: Option<&str> = None;
    let mut format = EmitFormat::Json;
    let mut form: Option<Form> = None;
    let mut base64 = false;
    let mut expecting_format = false;

    for arg in args {
        if expecting_format {
            let Some(chosen) = EmitFormat::parse(arg) else {
                let mut err = std::io::stderr();
                let _ = writeln!(err, "nkb emit: unknown format '{arg}'");
                let _ = writeln!(err, "Available: json, csv, lines.");
                return ExitCode::Usage;
            };
            format = chosen;
            expecting_format = false;
            continue;
        }
        match arg.as_str() {
            "-h" | "--help" => {
                print_emit_help();
                return ExitCode::Ok;
            }
            "--format" => expecting_format = true,
            "--escaped" | "--raw" => {
                // The two point in opposite directions on one axis, so asking
                // for both is a contradiction rather than a preference to
                // resolve quietly.
                let asked = if arg == "--escaped" {
                    Form::Escaped
                } else {
                    Form::Literal
                };
                if form.is_some_and(|already| already != asked) {
                    let mut err = std::io::stderr();
                    let _ = writeln!(
                        err,
                        "nkb emit: --escaped and --raw ask for opposite things."
                    );
                    let _ = writeln!(err, "Pick one, or neither to take the format's default.");
                    return ExitCode::Usage;
                }
                form = Some(asked);
            }
            "--base64" => base64 = true,
            other if other.starts_with('-') => {
                let mut err = std::io::stderr();
                let _ = writeln!(err, "nkb emit: unknown option '{other}'");
                let _ = writeln!(err, "Run 'nkb emit --help' to see what is available.");
                return ExitCode::Usage;
            }
            other => wanted = Some(other),
        }
    }

    if expecting_format {
        let mut err = std::io::stderr();
        let _ = writeln!(err, "nkb emit: --format needs a name.");
        let _ = writeln!(err, "Available: json, csv, lines.");
        return ExitCode::Usage;
    }

    let Some(wanted) = wanted else {
        let mut err = std::io::stderr();
        let _ = writeln!(err, "nkb emit: name a pack.");
        let _ = writeln!(err, "Run 'nkb packs' to see what there is.");
        return ExitCode::Usage;
    };

    let catalogue = BuiltInCatalogue::new();

    match emit_values(&catalogue, &TomlPackFormat, &SystemClock, wanted) {
        EmitOutcome::Emitted(emission) => {
            let rendered = emit_output::render(&emission, format, form, base64);
            let mut err = std::io::stderr();
            for note in &rendered.notes {
                let _ = writeln!(err, "nkb emit: {note}");
            }
            // A closed pipe - `nkb emit pack | head -1` - ends the run quietly
            // with 0, a full disk under `> file` ends it with 5 (`output`).
            out!("{}", rendered.data);
            ExitCode::Ok
        }
        EmitOutcome::Refused { errors } => {
            let mut err = std::io::stderr();
            let _ = writeln!(
                err,
                "nkb emit: '{wanted}' has {errors} errors and is not loaded at all."
            );
            let _ = writeln!(
                err,
                "Nothing was printed - a pack is whole or absent. Run `nkb lint` on it."
            );
            ExitCode::ValidationFailed
        }
        EmitOutcome::NotFound => {
            let mut err = std::io::stderr();
            let _ = writeln!(err, "nkb emit: no pack called '{wanted}'.");
            let _ = writeln!(err, "Run 'nkb packs' to see what there is.");
            ExitCode::NotFound
        }
        EmitOutcome::Unreadable => {
            let mut err = std::io::stderr();
            let _ = writeln!(err, "nkb emit: '{wanted}' could not be read.");
            ExitCode::IoFailed
        }
        EmitOutcome::ValueTooLarge { id, code } => {
            let mut err = std::io::stderr();
            let _ = writeln!(
                err,
                "nkb emit: value '{id}' passed validation and could not be built ({code})."
            );
            let _ = writeln!(
                err,
                "This is a fault in the tool, not in the pack. Nothing was printed."
            );
            ExitCode::ValidationFailed
        }
    }
}

fn print_emit_help() {
    outln!("nkb emit - print a pack's values for a script or a file");
    outln!();
    outln!("Usage:");
    outln!("  nkb emit <pack> [--format json|csv|lines] [--escaped|--raw] [--base64]");
    outln!();
    outln!("Values go to standard output. The account of what came out, and any");
    outln!("note about what a format could not carry, go to standard error - so a");
    outln!("redirected file holds values and nothing else.");
    outln!();
    outln!("Formats:");
    outln!("  json     Both forms of every value, and every field. The default,");
    outln!("           because it is the only one that carries the whole catalogue");
    outln!("  csv      One row per value. Escaped by default, because this");
    outln!("           catalogue contains values that destroy CSV files");
    outln!("  lines    One value per line. Refuses a value containing a line");
    outln!("           break by name, rather than splitting it silently");
    outln!();
    outln!("Options:");
    outln!("      --escaped  Print values as a pack file stores them");
    outln!("      --raw      Print values literally");
    outln!("      --base64   Encode values, for channels that damage text");
    outln!("  -h, --help     Show this help and exit with 0");
}

fn print_show_help() {
    outln!("nkb show - print one pack in full");
    outln!();
    outln!("Usage:");
    outln!("  nkb show <pack>");
    outln!();
    outln!("Values are printed ESCAPED, which is the only readable form for a value");
    outln!("made of characters nobody can see. A generated value is printed as its");
    outln!("recipe and is never expanded here.");
    outln!();
    outln!("Options:");
    outln!("  -h, --help     Show this help and exit with 0");
}

fn print_help() {
    outln!("nkb {VERSION} - malicious test data, one shortcut away");
    outln!();
    outln!("Usage:");
    outln!("  nkb <command> [options]");
    outln!();
    outln!("Commands:");
    outln!("  packs             List the packs this build can offer");
    outln!("  show <pack>       Print one pack in full");
    outln!("  emit <pack>       Print a pack's values for a script or a file");
    outln!("  send <pack>       Type one value into the focused field");
    outln!("  lint <file>       Check a pack file against the format rules");
    outln!("  fmt <file>        Rewrite a pack file in canonical shape");
    outln!("  new-pack <name>   Write the skeleton for a new pack");
    outln!();
    outln!("Options:");
    outln!("  -h, --help        Show this help and exit with 0");
    outln!("      --version     Print the version and exit with 0");
    outln!();
    outln!("The graphical interface is a separate executable: nkb-gui");
}

fn print_new_pack_help() {
    outln!("nkb new-pack - write the skeleton for a new pack");
    outln!();
    outln!("Usage:");
    outln!("  nkb new-pack <name>");
    outln!();
    outln!("The name becomes both the file name and the pack identifier, so it has");
    outln!("to be lower case letters, digits and hyphens, starting with a letter.");
    outln!("The file is written in the current folder and never over an existing one.");
    outln!();
    outln!("Options:");
    outln!("  -h, --help     Show this help and exit with 0");
}

fn print_lint_help() {
    outln!("nkb lint - check a pack file against the format rules");
    outln!();
    outln!("Usage:");
    outln!("  nkb lint <file.toml> [--json] [--explain]");
    outln!();
    outln!("Options:");
    outln!("      --json     Write the verdict as JSON, and nothing else");
    outln!("      --explain  List every rule and whether this build checks it");
    outln!("  -h, --help     Show this help and exit with 0");
    outln!();
    outln!("Exit codes:");
    outln!("  0  the pack passed every rule this build checks");
    outln!("  1  the pack broke at least one blocking rule");
    outln!("  2  the command was called wrongly");
    outln!("  3  there is no pack file to read at that path");
    outln!("  5  the file is there and could not be read, or the verdict");
    outln!("     could not be written to standard output");
}

fn print_fmt_help() {
    outln!("nkb fmt - rewrite a pack file in canonical shape");
    outln!();
    outln!("Usage:");
    outln!("  nkb fmt <file.toml> [--dry-run]");
    outln!();
    outln!("Puts the fields of each table in the order the format defines and lines");
    outln!("up the equals signs. Comments, blank lines and the order of the values");
    outln!("themselves are left exactly as they are - the order of the values is the");
    outln!("order a tester meets them.");
    outln!();
    outln!("Options:");
    outln!("      --dry-run  Say what would change and write nothing");
    outln!("  -h, --help     Show this help and exit with 0");
    outln!();
    outln!("Exit codes:");
    outln!("  0  the file is in shape, or was put in shape");
    outln!("  1  the file is not TOML, so there was nothing to put in order");
    outln!("  2  the command was called wrongly");
    outln!("  3  there is no pack file to read at that path");
    outln!("  4  formatting would have changed a value, so nothing was written");
    outln!("  5  the file could not be read or written, or standard output");
    outln!("     could not be written");
}

#[cfg(test)]
#[allow(
    clippy::panic,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;
    use nkb_app::{DeliveryError, Following, KeystrokeError, StopReason};
    use nkb_core::preview::{Recipe, ValuePreview};
    use nkb_core::substitutions::Substitutions;

    fn args(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_owned()).collect()
    }

    /// A file from the format test suite, addressed from this crate.
    fn pack(kind: &str, name: &str) -> String {
        format!(
            "{}/../../tests/packs/{kind}/{name}.toml",
            env!("CARGO_MANIFEST_DIR")
        )
    }

    #[test]
    fn what_send_sent_is_counted_in_the_number_each_count_is() {
        // The shortest values in the catalogue are the ones this reads out
        // most: `magic-values/dash-only` is one of everything.
        assert_eq!(
            sent_counts(1, 1, 1, 1),
            "1 grapheme, 1 code point, 1 byte, 1 UTF-16 unit"
        );
        assert_eq!(
            sent_counts(1, 5, 18, 8),
            "1 grapheme, 5 code points, 18 bytes, 8 UTF-16 units"
        );
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
                wanted: "whitespace",
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
                wanted: "unicode-text",
                position: 7,
                delay_seconds: 0,
                clearing: Clearing::Line,
            }),
            "switches are read wherever they stand, before or after the pack"
        );
    }

    /// What `report_send` said, and the code it ended with.
    fn reported(outcome: SendOutcome) -> (String, ExitCode) {
        let mut said = Vec::new();
        let code = report_send(&mut said, "some-pack", outcome);
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
            (SendOutcome::NotFound, ExitCode::NotFound),
            (SendOutcome::Unreadable, ExitCode::IoFailed),
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

    #[test]
    fn a_bare_invocation_succeeds_instead_of_reporting_a_usage_error() {
        // It also must not try to start a window: the graphical interface is a
        // different executable, and this one runs where there is no screen.
        assert_eq!(run(&args(&[])), ExitCode::Ok);
    }

    #[test]
    fn asking_for_help_is_a_success() {
        assert_eq!(run(&args(&["--help"])), ExitCode::Ok);
        assert_eq!(run(&args(&["-h"])), ExitCode::Ok);
    }

    #[test]
    fn emitting_a_shipped_pack_succeeds_in_every_format() {
        for format in ["json", "csv", "lines"] {
            assert_eq!(
                run(&args(&["emit", "whitespace", "--format", format])),
                ExitCode::Ok,
                "--format {format}"
            );
        }
        // No format named at all takes the default, which is json.
        assert_eq!(run(&args(&["emit", "whitespace"])), ExitCode::Ok);
    }

    #[test]
    fn emitting_an_unknown_pack_is_told_apart_from_a_bad_invocation() {
        // The distinction a pipeline branches on: 3 means the name is wrong,
        // 2 means the command is.
        assert_eq!(run(&args(&["emit", "no-such-pack"])), ExitCode::NotFound);
        assert_eq!(run(&args(&["emit"])), ExitCode::Usage);
    }

    #[test]
    fn an_unknown_format_is_refused_rather_than_guessed_at() {
        assert_eq!(
            run(&args(&["emit", "whitespace", "--format", "yaml"])),
            ExitCode::Usage
        );
        // --format with nothing after it is the same mistake seen earlier.
        assert_eq!(
            run(&args(&["emit", "whitespace", "--format"])),
            ExitCode::Usage
        );
    }

    #[test]
    fn asking_for_both_forms_at_once_is_a_contradiction_not_a_preference() {
        assert_eq!(
            run(&args(&["emit", "whitespace", "--escaped", "--raw"])),
            ExitCode::Usage
        );
        // Repeating the same one is not a contradiction and stays allowed.
        assert_eq!(
            run(&args(&["emit", "whitespace", "--escaped", "--escaped"])),
            ExitCode::Ok
        );
    }

    #[test]
    fn emit_takes_the_switches_the_format_document_defines() {
        for switch in ["--escaped", "--raw", "--base64"] {
            assert_eq!(
                run(&args(&["emit", "whitespace", switch])),
                ExitCode::Ok,
                "{switch}"
            );
        }
        assert_eq!(
            run(&args(&["emit", "whitespace", "--frobnicate"])),
            ExitCode::Usage
        );
    }

    #[test]
    fn emit_help_is_a_success_and_needs_no_pack() {
        assert_eq!(run(&args(&["emit", "--help"])), ExitCode::Ok);
    }

    #[test]
    fn version_succeeds() {
        assert_eq!(run(&args(&["--version"])), ExitCode::Ok);
    }

    #[test]
    fn an_unknown_command_is_a_usage_error_not_a_silent_success() {
        assert_eq!(run(&args(&["frobnicate"])), ExitCode::Usage);
    }

    #[test]
    fn listing_the_packs_succeeds_and_does_not_need_a_file() {
        assert_eq!(run(&args(&["packs"])), ExitCode::Ok);
    }

    #[test]
    fn packs_takes_no_argument_and_says_so_rather_than_ignoring_one() {
        // Ignoring it would let `nkb packs whitespace` print the whole catalogue
        // and look like it had answered the question that was asked.
        assert_eq!(run(&args(&["packs", "whitespace"])), ExitCode::Usage);
    }

    #[test]
    fn showing_a_carried_pack_succeeds() {
        assert_eq!(run(&args(&["show", "whitespace"])), ExitCode::Ok);
    }

    #[test]
    fn showing_a_pack_that_does_not_exist_is_three_and_not_one() {
        // The same distinction lint makes: "no such pack" is not "this pack is
        // wrong", and a script branching on the code has to be able to tell.
        assert_eq!(run(&args(&["show", "no-such-pack"])), ExitCode::NotFound);
    }

    #[test]
    fn show_without_a_name_is_a_usage_error_rather_than_a_quiet_success() {
        assert_eq!(run(&args(&["show"])), ExitCode::Usage);
    }

    #[test]
    fn both_new_commands_answer_help_without_touching_the_catalogue() {
        assert_eq!(run(&args(&["packs", "--help"])), ExitCode::Ok);
        assert_eq!(run(&args(&["show", "--help"])), ExitCode::Ok);
    }

    #[test]
    fn a_pack_that_passes_exits_zero() {
        assert_eq!(
            run(&args(&["lint", &pack("accepted", "magic-values")])),
            ExitCode::Ok
        );
    }

    #[test]
    fn a_pack_that_fails_exits_one_because_that_is_what_contribution_checks_read() {
        assert_eq!(
            run(&args(&["lint", &pack("rejected", "no-format")])),
            ExitCode::ValidationFailed
        );
    }

    #[test]
    fn a_pack_that_is_not_there_is_told_apart_from_a_pack_that_is_wrong() {
        // Three, not one. A contribution check has to distinguish "the pack is
        // bad" from "the path is wrong", or a typo reads as a rejected pack.
        assert_eq!(
            run(&args(&["lint", "no-such-file-anywhere.toml"])),
            ExitCode::NotFound
        );
    }

    #[test]
    fn lint_without_a_file_is_a_usage_error_rather_than_a_quiet_success() {
        assert_eq!(run(&args(&["lint"])), ExitCode::Usage);
    }

    #[test]
    fn an_unknown_option_on_lint_is_a_usage_error() {
        assert_eq!(
            run(&args(&["lint", "whatever.toml", "--frobnicate"])),
            ExitCode::Usage
        );
    }

    #[test]
    fn explaining_the_rules_needs_no_file_and_succeeds() {
        assert_eq!(run(&args(&["lint", "--explain"])), ExitCode::Ok);
    }

    #[test]
    fn asking_lint_for_help_is_a_success() {
        assert_eq!(run(&args(&["lint", "--help"])), ExitCode::Ok);
    }
    #[test]
    fn machine_readable_output_keeps_the_same_exit_codes() {
        // The codes are the contract continuous integration reads. Asking for a
        // different output format must not change the verdict it carries.
        assert_eq!(
            run(&args(&[
                "lint",
                &pack("accepted", "magic-values"),
                "--json"
            ])),
            ExitCode::Ok
        );
        assert_eq!(
            run(&args(&["lint", &pack("rejected", "no-format"), "--json"])),
            ExitCode::ValidationFailed
        );
        assert_eq!(
            run(&args(&["lint", "no-such-file-anywhere.toml", "--json"])),
            ExitCode::NotFound
        );
    }

    #[test]
    fn a_warning_only_pack_exits_zero_in_both_output_formats() {
        // A warning is reported and passed through. If the two formats disagreed
        // about that, one of them would be lying about what the pack is.
        let path = pack("rejected", "long-literal-value");
        assert_eq!(run(&args(&["lint", &path])), ExitCode::Ok);
        assert_eq!(run(&args(&["lint", &path, "--json"])), ExitCode::Ok);
    }

    #[test]
    fn explaining_the_rules_as_json_needs_no_file() {
        assert_eq!(run(&args(&["lint", "--explain", "--json"])), ExitCode::Ok);
    }
    /// Reports raw control characters found **inside** a JSON string.
    ///
    /// A writer put together by hand is judged on exactly this. JSON forbids
    /// unescaped control characters, and every one of them is a value in this
    /// catalogue - so the output is the place they are most likely to appear and
    /// least likely to be noticed.
    fn control_characters_inside_strings(json: &str) -> Vec<u32> {
        let mut inside = false;
        let mut escaped = false;
        let mut found = Vec::new();

        for character in json.chars() {
            if escaped {
                escaped = false;
                continue;
            }
            match character {
                '\\' if inside => escaped = true,
                '\"' => inside = !inside,
                other if inside && (other as u32) < 0x20 => found.push(other as u32),
                _ => {}
            }
        }
        found
    }

    #[test]
    fn the_verifier_in_the_next_test_can_actually_fail() {
        // A check nobody has seen fail is indistinguishable from a broken one,
        // and this one is written here rather than taken from a library.
        assert!(control_characters_inside_strings("{\"a\": \"plain\"}").is_empty());

        // A raw control character inside a string is the fault being looked for.
        let raw = format!("{{\"a\": \"x{}y\"}}", '\u{1}');
        assert_eq!(control_characters_inside_strings(&raw), vec![1]);

        // The same character written as an escape is correct output, and calling
        // that broken would make the guard worse than useless.
        assert!(control_characters_inside_strings("{\"a\": \"x\\u0001y\"}").is_empty());

        // A newline between members is structure rather than content.
        assert!(control_characters_inside_strings("{\n  \"a\": 1\n}").is_empty());
    }

    #[test]
    fn every_pack_in_the_test_suite_survives_the_json_writer() {
        // End to end over the whole suite: a real file, a real parse, a real
        // document. The suite deliberately holds a pack whose key names carry
        // every character that breaks a writer put together by hand.
        let root = format!("{}/../../tests/packs", env!("CARGO_MANIFEST_DIR"));
        let mut checked = 0;

        for kind in ["accepted", "rejected"] {
            let dir = std::fs::read_dir(format!("{root}/{kind}"))
                .unwrap_or_else(|e| panic!("the {kind} set must exist: {e}"));

            for entry in dir.filter_map(Result::ok) {
                let path = entry.path();
                if path.extension().is_none_or(|ext| ext != "toml") {
                    continue;
                }
                let Some((source, id)) = DirectoryPackSource::split(&path) else {
                    continue;
                };

                let outcome = lint_pack(&source, &TomlPackFormat, &id);
                let text =
                    lint_json::document(&[(path.display().to_string(), outcome)], VERSION).render();

                let raw = control_characters_inside_strings(&text);
                assert!(
                    raw.is_empty(),
                    "{} produced raw control characters inside a JSON string: {raw:?}",
                    path.display()
                );
                assert!(
                    text.ends_with("}\n"),
                    "{} produced a document that does not close",
                    path.display()
                );
                checked += 1;
            }
        }

        assert!(
            checked >= 19,
            "the suite should not shrink without somebody noticing: {checked}"
        );
    }
}
