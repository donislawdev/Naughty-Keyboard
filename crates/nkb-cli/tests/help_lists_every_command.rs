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

#[test]
fn the_general_help_names_every_command_the_dispatcher_accepts() {
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

    // Without this, a clean result is also what a wrong parse produces.
    assert!(
        commands.len() >= 6,
        "found {} commands in the dispatcher, expected at least 6 - the parse missed them, \
         so a clean result would mean nothing: {commands:?}",
        commands.len()
    );

    // The help body: `    println!("  packs             List the packs ...");`
    let help_start = body.find("fn print_help()").expect("print_help must exist");
    let help_body = &body[help_start..];
    let help_end = help_body.find("\n}").unwrap_or(help_body.len());
    let help_body = &help_body[..help_end];

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
