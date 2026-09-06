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
mod lint_report;

use exit::ExitCode;
use nkb_adapters::{DirectoryPackSource, TomlPackFormat};
use nkb_app::{LintOutcome, lint_pack};
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

    for arg in args {
        match arg.as_str() {
            "--explain" => explain = true,
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

    if explain {
        for line in lint_report::explanation() {
            println!("{line}");
        }
        // Explaining the rules is a question about the tool, not about a file, so
        // it answers on its own and does not require one.
        if path.is_none() {
            return ExitCode::Ok;
        }
        println!();
    }

    let Some(path) = path else {
        let mut err = std::io::stderr();
        let _ = writeln!(err, "nkb lint: name a pack file to check.");
        let _ = writeln!(err, "Usage: nkb lint <file.toml> [--explain]");
        return ExitCode::Usage;
    };

    let Some((source, id)) = DirectoryPackSource::split(Path::new(path)) else {
        let mut err = std::io::stderr();
        let _ = writeln!(err, "nkb lint: '{path}' does not name a file.");
        return ExitCode::Usage;
    };

    match lint_pack(&source, &TomlPackFormat, &id) {
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
    println!("  lint <file>    Check a pack file against the format rules");
    println!();
    println!("Options:");
    println!("  -h, --help     Show this help and exit with 0");
    println!("      --version  Print the version and exit with 0");
    println!();
    println!("The graphical interface is a separate executable: nkb-gui");
}

fn print_lint_help() {
    println!("nkb lint - check a pack file against the format rules");
    println!();
    println!("Usage:");
    println!("  nkb lint <file.toml> [--explain]");
    println!();
    println!("Options:");
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
}
