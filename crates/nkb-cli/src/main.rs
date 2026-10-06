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
mod emit_output;
mod english;
mod exit;
mod json;
mod lint_json;
mod lint_report;

use emit_output::{EmitFormat, Form};
use exit::ExitCode;
use nkb_adapters::{
    BuiltInCatalogue, DirectInjection, DirectoryPackSink, DirectoryPackSource, SystemClock,
    TomlPackFormat,
};
use nkb_app::{
    Availability, Clearing, ClearingOutcome, DeliveryError, EmitOutcome, FormatOutcome,
    KeystrokeError, LintOutcome, NewPackOutcome, Progress, SendOutcome, SendRequest, Sending,
    ShowOutcome, SkipReason, StopReason, ValueDelivery, ValueFacts, emit_values, format_pack,
    lint_pack, list_packs, new_pack, send_value, show_pack,
};
use std::io::Write;
use std::path::Path;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = run(&args);
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
            println!("nkb {VERSION}");
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
            print!("{}", lint_json::document(&[], VERSION).render());
        } else {
            for line in lint_report::explanation() {
                println!("{line}");
            }
        }
        return ExitCode::Ok;
    }

    // In machine readable mode standard output carries the document and nothing
    // else. One stray line of prose beside it and the consumer's parse fails.
    if explain && !json {
        for line in lint_report::explanation() {
            println!("{line}");
        }
        println!();
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
        print!(
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
                println!("{line}");
            }
            for line in lint_report::summary(&report) {
                println!("{line}");
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
            println!("{path} is already in shape. Nothing was written.");
            ExitCode::Ok
        }
        FormatOutcome::Formatted => {
            println!("Wrote {path} in canonical shape.");
            ExitCode::Ok
        }
        FormatOutcome::WouldFormat => {
            // A dry run reports on standard output because the report is what it
            // was asked for. It is the product of this run, not a complaint.
            println!("{path} is not in shape. Run without --dry-run to rewrite it.");
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
            println!("Wrote {file}.");
            println!("Edit it, then run `nkb lint {file}` - it reports everything in one pass.");
            println!(
                "The pack format is not frozen yet: it freezes with the first public release that ships packs."
            );
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
                println!("{line}");
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
                println!("{line}");
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
    println!("nkb packs - list the packs this build can offer");
    println!();
    println!("Usage:");
    println!("  nkb packs");
    println!();
    println!("Every run also states which pack sources were read and which were not,");
    println!("because a list drawn from one source looks exactly like a complete one.");
    println!();
    println!("Options:");
    println!("  -h, --help     Show this help and exit with 0");
}

/// Which switch is still waiting for its number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SendSwitch {
    Index,
    Delay,
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
    let mut wanted: Option<&str> = None;
    let mut position: u64 = 1;
    let mut delay_seconds: u64 = 3;
    let mut clearing = Clearing::Keep;
    let mut expecting: Option<SendSwitch> = None;

    for arg in args {
        if let Some(which) = expecting {
            let name = match which {
                SendSwitch::Index => "--index",
                SendSwitch::Delay => "--delay",
            };
            let Ok(number) = arg.parse::<u64>() else {
                let mut err = std::io::stderr();
                let _ = writeln!(err, "nkb send: {name} needs a whole number, got '{arg}'.");
                return ExitCode::Usage;
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
                return ExitCode::Ok;
            }
            "--index" => expecting = Some(SendSwitch::Index),
            "--delay" => expecting = Some(SendSwitch::Delay),
            "--clear" => clearing = Clearing::Line,
            other if other.starts_with('-') => {
                let mut err = std::io::stderr();
                let _ = writeln!(err, "nkb send: unknown switch '{other}'");
                let _ = writeln!(err, "Run 'nkb send --help' to see what is available.");
                return ExitCode::Usage;
            }
            name => {
                if wanted.is_some() {
                    let mut err = std::io::stderr();
                    let _ = writeln!(err, "nkb send: name one pack, not two.");
                    return ExitCode::Usage;
                }
                wanted = Some(name);
            }
        }
    }

    if let Some(which) = expecting {
        let name = match which {
            SendSwitch::Index => "--index",
            SendSwitch::Delay => "--delay",
        };
        let mut err = std::io::stderr();
        let _ = writeln!(err, "nkb send: {name} needs a number.");
        return ExitCode::Usage;
    }

    let Some(wanted) = wanted else {
        let mut err = std::io::stderr();
        let _ = writeln!(err, "nkb send: name a pack.");
        let _ = writeln!(err, "Run 'nkb packs' to see what there is.");
        return ExitCode::Usage;
    };

    // Asked before the countdown, so a machine with no route does not make
    // somebody watch three seconds tick away for nothing.
    let delivery = DirectInjection;
    if let Availability::Unavailable { reason } = delivery.availability() {
        let mut err = std::io::stderr();
        let _ = writeln!(
            err,
            "nkb send: no way to deliver keystrokes on {reason} yet."
        );
        let _ = writeln!(
            err,
            "Nothing was sent. `nkb emit {wanted}` writes the values instead."
        );
        return ExitCode::InsertFailed;
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
    // meant". Telling the truth about that is `TargetInspector`, and it is a
    // separate piece of work.
    {
        let mut err = std::io::stderr();
        match after {
            Some(target) => {
                let _ = writeln!(err, "nkb send: target is window {:#x}", target.0);
                if delay_seconds > 0 && before == after {
                    let _ = writeln!(
                        err,
                        "  the focus did not change during the countdown - if that is the terminal, stop now"
                    );
                }
            }
            None => {
                let _ = writeln!(err, "nkb send: nothing holds the keyboard focus.");
                let _ = writeln!(err, "Click into a field, then run this again.");
                return ExitCode::InsertFailed;
            }
        }
    }

    let catalogue = BuiltInCatalogue::new();
    let position = usize::try_from(position).unwrap_or(usize::MAX);
    let request = SendRequest {
        pack_id: wanted,
        position,
        clearing,
    };

    let mut err = std::io::stderr();
    // `DirectInjection` is both the delivery and the keystroke route: one
    // adapter, two ports, so the two cannot disagree about the target.
    // `OBS-160`: a send still running after a second says so once, on the way,
    // with how to stop it - the palette's send band, in this command's words.
    let started = std::time::Instant::now();
    let mut said = false;
    let mut on_the_way = |sending: Sending<'_>| {
        if let Some(line) = still_typing(started.elapsed(), &mut said, sending.progress) {
            let _ = writeln!(std::io::stderr(), "{line}");
        }
    };
    match send_value(
        &catalogue,
        &TomlPackFormat,
        &delivery,
        &delivery,
        &request,
        &mut on_the_way,
    ) {
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
            paced,
        } => {
            match clearing {
                ClearingOutcome::Done => {
                    let _ = writeln!(err, "nkb send: cleared the line (Home, Shift+End, Delete)");
                }
                ClearingOutcome::Skipped(SkipReason::Unconfirmed) => {
                    let _ = writeln!(err, "{CLEARING_SKIPPED}");
                }
                ClearingOutcome::Skipped(SkipReason::Terminal) => {
                    let _ = writeln!(err, "{CLEARING_SKIPPED_IN_TERMINAL}");
                }
                ClearingOutcome::NotAsked => {}
            }
            let _ = writeln!(err, "nkb send: sent {reference} - {name}");
            let _ = writeln!(
                err,
                "  {}",
                sent_counts(graphemes, code_points, bytes, utf16_units)
            );
            // Said plainly, because the shorter sentence reads as a stronger
            // claim than the tool can make. Paced (`D95`), the application's
            // queue took every key - which is still not what the field KEPT.
            // Unpaced, only the system took them, and a busy application may
            // have lost some (`OBS-158`).
            let _ = if paced {
                writeln!(
                    err,
                    "  the application took every keystroke - check the field to see what it kept"
                )
            } else {
                writeln!(
                    err,
                    "  this application's pace could not be followed, so characters may be missing if it was busy - check the field"
                )
            };
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
            let _ = match &error {
                KeystrokeError::ModifierHeld { which } => writeln!(
                    err,
                    "nkb send: {which} is still held on the keyboard, so nothing was sent - release it and run again."
                ),
                KeystrokeError::NoTarget => {
                    writeln!(
                        err,
                        "nkb send: nothing holds the keyboard focus, so there is nothing to clear."
                    )
                }
                KeystrokeError::Unsupported { system } => {
                    writeln!(err, "nkb send: clearing is not supported on {system} yet.")
                }
                KeystrokeError::Partial {
                    chords_sent,
                    chords_expected,
                    reason,
                } => writeln!(
                    err,
                    "nkb send: only {chords_sent} of {chords_expected} clearing key presses arrived - {}. The field may be half-cleared. Nothing else was sent.",
                    stop_reason(*reason)
                ),
                // `OBS-157`: zero presses acted, so the field is as it was -
                // "half-cleared" would send the tester looking for damage.
                KeystrokeError::NothingArrived { reason } => writeln!(
                    err,
                    "nkb send: none of the clearing key presses reached the field - {}. The field is as it was, and nothing was sent.",
                    stop_reason(*reason)
                ),
                KeystrokeError::HigherPrivileges => writeln!(err, "{HIGHER_PRIVILEGES}"),
                KeystrokeError::NoTextField => writeln!(err, "{NO_TEXT_FIELD}"),
                // `send_value` treats this one as a skip and sends the value, so
                // it does not arrive here. If that ever stops, nothing was
                // pressed and nothing was sent, and that is what is said.
                KeystrokeError::FieldUnconfirmed => writeln!(
                    err,
                    "nkb send: the focus could not be confirmed as a text field, so it was not cleared and nothing was sent."
                ),
                // The same skip for a terminal, and the same fallback if it ever
                // arrives here: nothing pressed, nothing sent.
                KeystrokeError::InTerminal => writeln!(
                    err,
                    "nkb send: the focus is a terminal, so it was not cleared and nothing was sent."
                ),
            };
            ExitCode::InsertFailed
        }
        SendOutcome::Interrupted {
            facts,
            units_sent,
            units_expected,
            clearing,
            reason,
        } => {
            say_clearing_before_failure(&mut err, clearing);
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
            say_clearing_before_failure(&mut err, clearing);
            match &error {
                DeliveryError::NoTarget => {
                    let _ = writeln!(err, "nkb send: nothing holds the keyboard focus.");
                    let _ = writeln!(err, "Click into a field, then run this again.");
                }
                DeliveryError::Unsupported { system } => {
                    let _ = writeln!(err, "nkb send: not supported on {system} yet.");
                }
                DeliveryError::ModifierHeld { which } => {
                    let _ = writeln!(
                        err,
                        "nkb send: {which} is still held on the keyboard, so the value was not sent - release it and run again."
                    );
                }
                // `send_value` reports a fragment as `Interrupted`, above, with
                // the value it belongs to. Reached only if a route ever reports
                // one another way - still said, without the name it does not have.
                DeliveryError::Partial {
                    units_sent,
                    units_expected,
                    reason,
                } => {
                    let _ = writeln!(
                        err,
                        "nkb send: only {units_sent} of {units_expected} UTF-16 units arrived - {}.",
                        stop_reason(*reason)
                    );
                    let _ = writeln!(
                        err,
                        "The field holds a PARTIAL value. Clear it before testing."
                    );
                }
                // The three below belong to the clipboard route of the palette,
                // which this command does not have and will not have (D71,
                // OBS-131). Typing reports none of them. They are answered in
                // plain words rather than folded into a guess, should a route
                // ever start to.
                DeliveryError::Busy => {
                    let _ = writeln!(
                        err,
                        "nkb send: the route was held by another application, so nothing was sent - run again in a moment."
                    );
                }
                DeliveryError::Refused { detail } => {
                    let _ = writeln!(err, "nkb send: the route refused the value: {detail}.");
                }
                DeliveryError::CannotCarry { character } => {
                    let _ = writeln!(
                        err,
                        "nkb send: this route cannot carry U+{:04X}, so nothing was sent.",
                        u32::from(*character)
                    );
                }
                DeliveryError::HigherPrivileges => {
                    let _ = writeln!(err, "{HIGHER_PRIVILEGES}");
                }
                DeliveryError::NoTextField => {
                    let _ = writeln!(err, "{NO_TEXT_FIELD}");
                }
                // `OBS-157`: the keys stopped before any reached the field. No
                // fragment - "PARTIAL" here sent testers to clear a field that
                // held only its own content. A clearing that went first was
                // said just above.
                DeliveryError::NothingArrived { reason } => {
                    let _ = writeln!(
                        err,
                        "nkb send: nothing of the value reached the field - {}.",
                        stop_reason(*reason)
                    );
                }
            }
            ExitCode::InsertFailed
        }
    }
}

/// How long a send runs before `nkb send` says, once, that it is still typing.
///
/// A second, and not the palette's tenth: the palette redraws a band in place,
/// while a line printed here stays in the terminal and in a CI log for good, so
/// it is kept for the sends a person would wonder about.
const STILL_TYPING_AFTER: std::time::Duration = std::time::Duration::from_secs(1);

/// The one line a long send says on its way, or nothing - before the first
/// second, and after the line was said once.
fn still_typing(
    elapsed: std::time::Duration,
    said: &mut bool,
    progress: Progress,
) -> Option<String> {
    if *said || elapsed < STILL_TYPING_AFTER {
        return None;
    }
    *said = true;
    Some(format!(
        "nkb send: still typing - {} of {} UTF-16 units so far. Press Escape to stop.",
        progress.units_arrived, progress.units_total
    ))
}

/// Why a send stopped, in the words `nkb send` uses after a dash. The same
/// reasons the palette names (`D95`, `D96`), said in the command's own sentences.
fn stop_reason(reason: StopReason) -> &'static str {
    match reason {
        StopReason::Dropped => {
            "the system did not deliver the keys, another program may be blocking input"
        }
        StopReason::FocusMoved => "another window came to the front",
        StopReason::NotTaking => "the application stopped taking keys",
        StopReason::Escape => "Escape was pressed",
    }
}

/// What became of the clearing, said before the news that the value did not
/// arrive whole - the tester needs to know what the field held before it.
fn say_clearing_before_failure(err: &mut impl Write, clearing: ClearingOutcome) {
    match clearing {
        ClearingOutcome::Done => {
            let _ = writeln!(err, "nkb send: the line was cleared before this happened.");
        }
        ClearingOutcome::Skipped(SkipReason::Unconfirmed) => {
            let _ = writeln!(err, "{CLEARING_SKIPPED}");
        }
        ClearingOutcome::Skipped(SkipReason::Terminal) => {
            let _ = writeln!(err, "{CLEARING_SKIPPED_IN_TERMINAL}");
        }
        ClearingOutcome::NotAsked => {}
    }
}

/// What `nkb send` says when the window in front runs with higher privileges.
///
/// One sentence for both places it can come from - the clearing, which goes
/// first, and the send without `--clear` - because the fact is the same and no
/// key was pressed in either. Until `D72` this case printed "sent" and exited
/// with 0 while nothing reached the field (`OBS-128`). Code 4 is the existing
/// "the value could not be inserted", not a new code (`D46`).
const HIGHER_PRIVILEGES: &str = "nkb send: the window in front runs with higher privileges than nkb, so the system would drop the keystrokes without a word. Nothing was sent - run nkb with the same privileges to type into it.";

/// What `nkb send` says when the keyboard focus is surely not in a text field.
///
/// One sentence for both doors, like the one above: the clearing refuses first
/// when `--clear` is given, the send refuses otherwise, and no key was pressed
/// in either (`D73`). The cause is one the command READ - the control type of
/// the focused element - so it is named, and a focus it could not read sends.
///
/// "The system reports", because the command knows only what UI Automation
/// answered: a Firefox window asked for the first time reports its page for a
/// while, although the focus is in a field, until the focus moves (`OBS-140`).
/// Clicking into the field again is what ends that, so it is what is asked for.
const NO_TEXT_FIELD: &str = "nkb send: the system reports the keyboard focus outside a text field, so nothing was sent - keys there could press buttons or act on list items. Click into the field and run this again.";

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
    println!("nkb send - type one value into the focused field");
    println!();
    println!("Usage:");
    println!("  nkb send <pack> [--index N] [--delay S] [--clear]");
    println!();
    println!("Put one value of a pack into whatever field has the keyboard focus.");
    println!();
    println!("Options:");
    println!("  --index N      which value, counting from 1 in the pack's own order");
    println!("                 (default: 1). The order of values is the order of testing.");
    println!("  --delay S      seconds to wait first, so you can focus the target");
    println!("                 (default: 3). Use 0 when something else focuses it.");
    println!("  --clear        press Home, Shift+End, Delete first, clearing the current");
    println!("                 line of the field. Never selects beyond the line, so a");
    println!("                 multi-line field keeps its other lines. Without this switch");
    println!("                 the only keys sent are the value's own characters.");
    println!("  -h, --help     Show this help and exit with 0");
    println!();
    println!("The value goes to the focused window, not to standard output.");
    println!("Everything you read here is on standard error.");
    println!("Nothing is sent while Ctrl, Alt, Shift or Win is held on the keyboard:");
    println!("the command waits up to two seconds for them to come up, then refuses.");
    println!("Nothing is sent to a window running with higher privileges than nkb,");
    println!("such as an application started as administrator: the system would drop");
    println!("the keystrokes without a word, so the command refuses and exits with 4.");
    println!("Nothing is sent when the keyboard focus is on a button, a link, a list item");
    println!("or a page that is not editable: keys there act on that control, so the");
    println!("command refuses and exits with 4. Where it cannot tell, it sends.");
    println!("Press Escape to stop a send in progress: nothing more is typed, the command");
    println!("says how much arrived and exits with 4. Escape belongs to the command only");
    println!("while it types - before and after, the key is the application's.");
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

    match emit_values(&catalogue, &TomlPackFormat, wanted) {
        EmitOutcome::Emitted(emission) => {
            let rendered = emit_output::render(&emission, format, form, base64);
            let mut err = std::io::stderr();
            for note in &rendered.notes {
                let _ = writeln!(err, "nkb emit: {note}");
            }
            // Written through `write!` rather than `print!` so that a closed
            // pipe - `nkb emit pack | head -1` - ends the run quietly instead of
            // panicking in the middle of somebody's shell.
            let mut out = std::io::stdout();
            match out.write_all(rendered.data.as_bytes()) {
                Ok(()) => ExitCode::Ok,
                Err(_) => ExitCode::Ok,
            }
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
    println!("nkb emit - print a pack's values for a script or a file");
    println!();
    println!("Usage:");
    println!("  nkb emit <pack> [--format json|csv|lines] [--escaped|--raw] [--base64]");
    println!();
    println!("Values go to standard output. The account of what came out, and any");
    println!("note about what a format could not carry, go to standard error - so a");
    println!("redirected file holds values and nothing else.");
    println!();
    println!("Formats:");
    println!("  json     Both forms of every value, and every field. The default,");
    println!("           because it is the only one that carries the whole catalogue");
    println!("  csv      One row per value. Escaped by default, because this");
    println!("           catalogue contains values that destroy CSV files");
    println!("  lines    One value per line. Refuses a value containing a line");
    println!("           break by name, rather than splitting it silently");
    println!();
    println!("Options:");
    println!("      --escaped  Print values as a pack file stores them");
    println!("      --raw      Print values literally");
    println!("      --base64   Encode values, for channels that damage text");
    println!("  -h, --help     Show this help and exit with 0");
}

fn print_show_help() {
    println!("nkb show - print one pack in full");
    println!();
    println!("Usage:");
    println!("  nkb show <pack>");
    println!();
    println!("Values are printed ESCAPED, which is the only readable form for a value");
    println!("made of characters nobody can see. A generated value is printed as its");
    println!("recipe and is never expanded here.");
    println!();
    println!("Options:");
    println!("  -h, --help     Show this help and exit with 0");
}

fn print_help() {
    println!("nkb {VERSION} - malicious test data, one shortcut away");
    println!();
    println!("Usage:");
    println!("  nkb <command> [options]");
    println!();
    println!("Commands:");
    println!("  packs             List the packs this build can offer");
    println!("  show <pack>       Print one pack in full");
    println!("  emit <pack>       Print a pack's values for a script or a file");
    println!("  send <pack>       Type one value into the focused field");
    println!("  lint <file>       Check a pack file against the format rules");
    println!("  fmt <file>        Rewrite a pack file in canonical shape");
    println!("  new-pack <name>   Write the skeleton for a new pack");
    println!();
    println!("Options:");
    println!("  -h, --help        Show this help and exit with 0");
    println!("      --version     Print the version and exit with 0");
    println!();
    println!("The graphical interface is a separate executable: nkb-gui");
}

fn print_new_pack_help() {
    println!("nkb new-pack - write the skeleton for a new pack");
    println!();
    println!("Usage:");
    println!("  nkb new-pack <name>");
    println!();
    println!("The name becomes both the file name and the pack identifier, so it has");
    println!("to be lower case letters, digits and hyphens, starting with a letter.");
    println!("The file is written in the current folder and never over an existing one.");
    println!();
    println!("Options:");
    println!("  -h, --help     Show this help and exit with 0");
}

fn print_lint_help() {
    println!("nkb lint - check a pack file against the format rules");
    println!();
    println!("Usage:");
    println!("  nkb lint <file.toml> [--json] [--explain]");
    println!();
    println!("Options:");
    println!("      --json     Write the verdict as JSON, and nothing else");
    println!("      --explain  List every rule and whether this build checks it");
    println!("  -h, --help     Show this help and exit with 0");
    println!();
    println!("Exit codes:");
    println!("  0  the pack passed every rule this build checks");
    println!("  1  the pack broke at least one blocking rule");
    println!("  2  the command was called wrongly");
    println!("  3  there is no pack file to read at that path");
    println!("  5  the file is there and could not be read");
}

fn print_fmt_help() {
    println!("nkb fmt - rewrite a pack file in canonical shape");
    println!();
    println!("Usage:");
    println!("  nkb fmt <file.toml> [--dry-run]");
    println!();
    println!("Puts the fields of each table in the order the format defines and lines");
    println!("up the equals signs. Comments, blank lines and the order of the values");
    println!("themselves are left exactly as they are - the order of the values is the");
    println!("order a tester meets them.");
    println!();
    println!("Options:");
    println!("      --dry-run  Say what would change and write nothing");
    println!("  -h, --help     Show this help and exit with 0");
    println!();
    println!("Exit codes:");
    println!("  0  the file is in shape, or was put in shape");
    println!("  1  the file is not TOML, so there was nothing to put in order");
    println!("  2  the command was called wrongly");
    println!("  3  there is no pack file to read at that path");
    println!("  4  formatting would have changed a value, so nothing was written");
    println!("  5  the file could not be read or written");
}

#[cfg(test)]
#[allow(
    clippy::panic,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;

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
    // depends on the machine: on macOS and Linux there is no delivery route at
    // all, and on a build server there may be no focused window - so a test that
    // reached the send would be green here and red on two of the three systems
    // this project is checked on. What a real send does is measured by running
    // it, and that measurement is written down rather than automated, because
    // automating it means typing into whatever window the build agent has.

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

    #[test]
    fn a_long_send_says_once_that_it_is_still_typing_and_how_to_stop_it() {
        // `OBS-160`: nothing before a second, one line after it, never two - a
        // line printed here stays in a CI log for good.
        let step = Progress {
            units_arrived: 1784,
            units_total: 65535,
        };
        let mut said = false;
        assert_eq!(
            still_typing(std::time::Duration::from_millis(900), &mut said, step),
            None
        );
        let Some(line) = still_typing(STILL_TYPING_AFTER, &mut said, step) else {
            panic!("a send past a second says so");
        };
        assert!(
            line.contains("1784 of 65535") && line.contains("Escape"),
            "the line says how far and how to stop: {line}"
        );
        assert_eq!(
            still_typing(std::time::Duration::from_secs(9), &mut said, step),
            None,
            "said once, not once a report"
        );
    }
}
