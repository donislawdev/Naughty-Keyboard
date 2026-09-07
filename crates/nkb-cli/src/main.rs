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

mod exit;
mod json;
mod lint_json;
mod lint_report;

use exit::ExitCode;
use nkb_adapters::{DirectoryPackSink, DirectoryPackSource, SystemClock, TomlPackFormat};
use nkb_app::{LintOutcome, NewPackOutcome, lint_pack, new_pack};
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
        "lint" => lint(&args[1..]),
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
            // The published set of exit codes has no code for a read failure, so
            // this reports the nearest one and says plainly that the file is there
            // - the wording carries what the number cannot. Adding a code after
            // release is a breaking change, so it is the owner's call, and it is
            // recorded as an open observation rather than decided here.
            let mut err = std::io::stderr();
            let _ = writeln!(
                err,
                "nkb lint: '{path}' is there and could not be read. Check the file's permissions."
            );
            ExitCode::NotFound
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
            // The published set of exit codes has no code for a write failure, so
            // the wording carries what the number cannot - the same compromise
            // `nkb lint` makes for an unreadable file, and recorded the same way.
            let mut err = std::io::stderr();
            let _ = writeln!(
                err,
                "nkb new-pack: {file} could not be written here. Check that this folder exists and that you can write to it."
            );
            ExitCode::Usage
        }
    }
}

/// Help is written to standard output and never requires loading the catalogue.
/// Someone reaching for `--help` may well be doing it because the catalogue is
/// what is broken.
fn print_help() {
    println!("nkb {VERSION} - malicious test data, one shortcut away");
    println!();
    println!("Usage:");
    println!("  nkb <command> [options]");
    println!();
    println!("Commands:");
    println!("  lint <file>       Check a pack file against the format rules");
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
    fn version_succeeds() {
        assert_eq!(run(&args(&["--version"])), ExitCode::Ok);
    }

    #[test]
    fn an_unknown_command_is_a_usage_error_not_a_silent_success() {
        assert_eq!(run(&args(&["frobnicate"])), ExitCode::Usage);
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
