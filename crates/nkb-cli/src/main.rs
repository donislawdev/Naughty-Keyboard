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

mod boundary;
mod browse_report;
mod clearing;
mod emit_output;
mod english;
mod exit;
mod folders;
mod help;
mod json;
mod limit_args;
mod lint_json;
mod lint_report;
mod on_the_way;
mod output;
mod pack_file;
mod refusals;
mod send;
mod sent_notes;
mod target_line;

use emit_output::{EmitFormat, Form};
use exit::ExitCode;
use help::{
    print_emit_help, print_fmt_help, print_help, print_lint_help, print_new_pack_help,
    print_packs_help, print_show_help,
};
use nkb_adapters::{DirectoryPackSink, PackFolders, SystemClock, TomlPackFormat};
use nkb_app::{
    EmitOutcome, FormatOutcome, LintOutcome, NewPackOutcome, ShowOutcome, emit_values, format_pack,
    lint_pack, list_packs, new_pack, show_pack,
};
use output::{out, outln};
use pack_file::{NamedPack, NotRead, lint_exit, named_pack, say_not_read};
use std::io::Write;

pub(crate) const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() -> std::process::ExitCode {
    // Not `std::env::args`, which ends the process with a panic on a word that
    // is not Unicode - a file name on Linux may be one. Refused by name.
    let Ok(args) = std::env::args_os()
        .skip(1)
        .map(std::ffi::OsString::into_string)
        .collect::<Result<Vec<String>, _>>()
    else {
        let _ = writeln!(
            std::io::stderr(),
            "nkb: an argument is not valid Unicode text, and nkb reads only text."
        );
        return std::process::ExitCode::from(2);
    };
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
        "packs" => folders::then(&args[1..], "packs", packs),
        "show" => folders::then(&args[1..], "show", show),
        "emit" => folders::then(&args[1..], "emit", emit),
        "send" => send::send(&args[1..]),
        "boundary" => boundary::boundary(&args[1..]),
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

    let Some(NamedPack { source, id, shown }) = named_pack("lint", path) else {
        return ExitCode::Usage;
    };

    let outcome = lint_pack(&source, &TomlPackFormat, &id);

    if json {
        out!(
            "{}",
            lint_json::document(&[(path.to_owned(), outcome.clone())], VERSION).render()
        );
        return lint_exit(&outcome);
    }

    match &outcome {
        LintOutcome::Judged(report) => {
            for line in lint_report::lines(report, path) {
                outln!("{line}");
            }
            for line in lint_report::summary(report) {
                outln!("{line}");
            }
            lint_exit(&outcome)
        }
        LintOutcome::NotFound => say_not_read("lint", &shown, NotRead::NotFound),
        LintOutcome::Unreadable => say_not_read("lint", &shown, NotRead::Unreadable),
        LintOutcome::TooLarge => say_not_read("lint", &shown, NotRead::TooLarge),
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

    let Some(NamedPack { source, id, shown }) = named_pack("fmt", path) else {
        return ExitCode::Usage;
    };
    let sink = DirectoryPackSink::new(source.folder());

    match format_pack(&source, &sink, &TomlPackFormat, &id, dry_run) {
        FormatOutcome::AlreadyCanonical => {
            outln!("{shown} is already in shape. Nothing was written.");
            ExitCode::Ok
        }
        FormatOutcome::Formatted => {
            outln!("Wrote {shown} in canonical shape.");
            ExitCode::Ok
        }
        FormatOutcome::WouldFormat => {
            // A dry run reports on standard output because the report is what it
            // was asked for. It is the product of this run, not a complaint.
            outln!("{shown} is not in shape. Run without --dry-run to rewrite it.");
            ExitCode::Ok
        }
        FormatOutcome::DidNotParse => {
            let mut err = std::io::stderr();
            let _ = writeln!(
                err,
                "nkb fmt: '{shown}' is not a pack file this tool can read, so there is nothing to put in order."
            );
            let _ = writeln!(err, "Run `nkb lint {shown}` to see what is wrong with it.");
            ExitCode::ValidationFailed
        }
        FormatOutcome::NotFound => say_not_read("fmt", &shown, NotRead::NotFound),
        FormatOutcome::Unreadable => say_not_read("fmt", &shown, NotRead::Unreadable),
        FormatOutcome::TooLarge => say_not_read("fmt", &shown, NotRead::TooLarge),
        FormatOutcome::Unwritable => {
            let mut err = std::io::stderr();
            let _ = writeln!(
                err,
                "nkb fmt: '{shown}' could not be written. Check that you can write to this folder - the file has not been changed."
            );
            ExitCode::IoFailed
        }
        FormatOutcome::WouldChangeValues => {
            // 🔴 A fault in this tool, not in the file, and the loudest thing the
            // command can say. Nothing was written.
            let mut err = std::io::stderr();
            let _ = writeln!(
                err,
                "nkb fmt: formatting '{shown}' would change what one of its values inserts, so nothing was written."
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
fn packs(args: &[String], folders: &PackFolders) -> ExitCode {
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

    let catalogue = folders.catalogue();
    let format = TomlPackFormat;

    match list_packs(&catalogue, &format) {
        Ok(found) => {
            for line in browse_report::listing(&found, folders) {
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
fn show(args: &[String], folders: &PackFolders) -> ExitCode {
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

    let catalogue = folders.catalogue();
    let format = TomlPackFormat;

    match show_pack(&catalogue, &format, wanted) {
        ShowOutcome::Shown {
            pack,
            warnings,
            origin,
        } => {
            for line in browse_report::origin_line(&origin, folders) {
                outln!("{line}");
            }
            for line in browse_report::pack(&pack, warnings) {
                outln!("{line}");
            }
            ExitCode::Ok
        }
        ShowOutcome::Refused { errors, origin } => {
            let mut err = std::io::stderr();
            let _ = writeln!(err, "{}", browse_report::not_loaded("show", wanted, errors));
            let lint = browse_report::lint_hint(wanted, &origin, folders);
            let _ = writeln!(
                err,
                "A pack is whole or absent - {lint} to see what is wrong."
            );
            ExitCode::ValidationFailed
        }
        ShowOutcome::NotFound => {
            let mut err = std::io::stderr();
            let _ = writeln!(err, "nkb show: no pack called '{wanted}'.");
            let _ = writeln!(err, "Run 'nkb packs' to see what there is.");
            folders::say_not_read(&mut err, "show", &catalogue, folders);
            ExitCode::NotFound
        }
        ShowOutcome::Unreadable { .. } => {
            let mut err = std::io::stderr();
            let _ = writeln!(err, "nkb show: '{wanted}' could not be read.");
            ExitCode::IoFailed
        }
    }
}

/// `nkb emit <pack> [--format json|csv|lines] [--escaped|--raw] [--base64]`
///
/// The command that takes the catalogue out of the tool. Everything it prints on
/// standard output is values. The account of what came out, and every note about
/// what a format could not carry, goes to the error stream - `ux-spec.md` 10.
fn emit(args: &[String], folders: &PackFolders) -> ExitCode {
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

    let catalogue = folders.catalogue();

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
            let _ = writeln!(err, "{}", browse_report::not_loaded("emit", wanted, errors));
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
            folders::say_not_read(&mut err, "emit", &catalogue, folders);
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

#[cfg(test)]
#[allow(
    clippy::panic,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;
    use nkb_adapters::DirectoryPackSource;

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
                let Ok((source, id)) = DirectoryPackSource::split(&path) else {
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
