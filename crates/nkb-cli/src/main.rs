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

use exit::ExitCode;
use std::io::Write;

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
        unknown => {
            let mut err = std::io::stderr();
            let _ = writeln!(err, "nkb: unknown command '{unknown}'");
            let _ = writeln!(err, "Run 'nkb --help' to see what is available.");
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
    println!("  (none yet - the catalogue and validator land in the next step)");
    println!();
    println!("Options:");
    println!("  -h, --help     Show this help and exit with 0");
    println!("      --version  Print the version and exit with 0");
    println!();
    println!("The graphical interface is a separate executable: nkb-gui");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_owned()).collect()
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
}
