//! Every command the dispatcher accepts is named in `nkb --help`.
//!
//! # Why this is worth a test
//!
//! `nkb send` shipped without a line in the general help, and nothing noticed.
//! The command worked, its own `--help` worked, `cargo test` was green and
//! clippy was clean - the only symptom was that a person running `nkb --help`
//! could not find out the command existed.
//!
//! That is the shape this project keeps meeting: no error, no failing test, and
//! a gap that only shows up when somebody uses the thing. 08-TEKSTY-DLA-UZYTKOWNIKA.md
//! asks for help text that lists what there is, and until now that was a habit
//! rather than a rule.
//!
//! # How it checks, and what it cannot see
//!
//! It reads `main.rs` as text: the dispatcher's arms look like `"name" => `, and
//! the help body prints lines starting with two spaces and the command name. So:
//!
//! - it does NOT run the program, and does not see the rendered output. A help
//!   line that is unreachable at runtime would still satisfy it;
//! - it says nothing about whether the DESCRIPTION is right, only that the name
//!   appears;
//! - it knows nothing about `nkb-gui`, which has its own surfaces.

#![allow(clippy::panic, clippy::expect_used)]

use std::path::Path;

/// The commands the dispatcher accepts, read out of its own source.
///
/// One list for both tests below. Two lists would mean a new command could
/// satisfy one of them and fall through the gap between them.
fn commands_in_the_dispatcher() -> Vec<String> {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/main.rs");
    let body = std::fs::read_to_string(&source)
        .unwrap_or_else(|e| panic!("{} must be readable: {e}", source.display()));

    // The dispatcher: `        "packs" => packs(&args[1..]),`
    let mut commands = Vec::new();
    for line in body.lines() {
        let trimmed = line.trim();
        let Some(rest) = trimmed.strip_prefix('"') else {
            continue;
        };
        let Some((name, tail)) = rest.split_once('"') else {
            continue;
        };
        if !tail.trim_start().starts_with("=>") {
            continue;
        }
        // Switches are handled in the same match; they are not commands.
        if name.starts_with('-') || name.is_empty() {
            continue;
        }
        commands.push(name.to_owned());
    }
    commands
}

/// The help body, as `print_help` writes it.
fn general_help() -> String {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/main.rs");
    let body = std::fs::read_to_string(&source)
        .unwrap_or_else(|e| panic!("{} must be readable: {e}", source.display()));
    let start = body.find("fn print_help()").expect("print_help must exist");
    let rest = &body[start..];
    let end = rest.find("\n}").unwrap_or(rest.len());
    rest[..end].to_owned()
}

#[test]
fn the_general_help_names_every_command_the_dispatcher_accepts() {
    let commands = commands_in_the_dispatcher();

    // Without this, a clean result is also what a wrong parse produces.
    assert!(
        commands.len() >= 6,
        "found {} commands in the dispatcher, expected at least 6 - the parse missed them, \
         so a clean result would mean nothing: {commands:?}",
        commands.len()
    );

    // The help body: `    println!("  packs             List the packs ...");`
    let help_body = general_help();

    let missing: Vec<&String> = commands
        .iter()
        .filter(|name| !help_body.contains(&format!("  {name} ")))
        .collect();

    assert!(
        missing.is_empty(),
        "\nthese commands are accepted but never mentioned in `nkb --help`: {missing:?}\n\n\
         A command nobody can discover is a command that does not exist for most people. \
         Add a line to print_help - and if it is deliberately hidden, this test is the place \
         to say so.\n"
    );
}

/// Every command answers `--help` the same way, and this one RUNS them.
///
/// # Why a second test, and why it starts the program
///
/// The test above reads source. It can see that a command is mentioned in the
/// general help and nothing about what happens when somebody asks that command
/// for help. Measured 2026-09-08: all seven answered, and `nkb send` answered in
/// a different shape - no first line naming the command, no `Usage:`, no
/// `Options:` entry for `--help` itself. Nothing was broken; the surface was
/// simply not uniform, which is the kind of drift that arrives one command at a
/// time and is never worth fixing on its own day.
///
/// `ux-spec.md` 10 requires `nkb <command> --help` to work always. `09-CLI-I-CI.md`
/// section 8 settles that asking for help is a success rather than a usage
/// error, so the exit code is 0 and the text goes to standard output.
///
/// # What it cannot see
///
/// Whether the words are right. It checks the frame - the name, the sections, a
/// non-empty body - because a frame is what drifts silently, while wrong wording
/// is caught by anybody who reads it once.
#[test]
fn every_command_answers_its_own_help_in_the_same_shape() {
    let commands = commands_in_the_dispatcher();

    for name in &commands {
        let run = std::process::Command::new(env!("CARGO_BIN_EXE_nkb"))
            .args([name.as_str(), "--help"])
            .output()
            .unwrap_or_else(|e| panic!("`nkb {name} --help` must be runnable: {e}"));

        assert!(
            run.status.success(),
            "`nkb {name} --help` exited with {:?}. Asking for help is a success, not a usage \
             error - 09-CLI-I-CI.md section 8.",
            run.status.code()
        );

        let text = String::from_utf8_lossy(&run.stdout);
        assert!(
            !text.trim().is_empty(),
            "`nkb {name} --help` printed nothing on standard output. A command whose help is \
             empty, or goes to the error stream, is a command nobody can learn from a pipe."
        );

        let first = text.lines().next().unwrap_or("");
        let expected = format!("nkb {name} - ");
        assert!(
            first.starts_with(&expected),
            "`nkb {name} --help` opens with {first:?}, and every other command opens with \
             \"{expected}...\". One surface in a different shape is what a person notices \
             before they notice anything else."
        );

        for section in ["Usage:", "-h, --help"] {
            assert!(
                text.contains(section),
                "`nkb {name} --help` has no {section:?}. The sections are the same everywhere \
                 so that reading one help teaches the reader how to read the rest."
            );
        }
    }

    // Without this, a dispatcher that parsed as empty would satisfy the loop.
    assert!(
        commands.len() >= 7,
        "only {} commands were examined: {commands:?}",
        commands.len()
    );
}
