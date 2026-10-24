//! The palette's shortcuts through the portal of the desktop, live, without a palette.
//!
//! The tests of `PortalShortcuts` dictate the answers of a desktop. What they cannot show is
//! the chain through the production path: the default shortcuts asked of a real portal, what
//! the desktop says it gave, and a physical press arriving as the action it stands for. This
//! example is that proof, for a Wayland session (`D145` points 59 to 62).
//!
//! ```text
//! cargo run -p nkb-adapters --example portal_loop -- [make-entry] [first N] [seconds S]
//! ```
//!
//! `make-entry` writes the entry that gives the program an identifier (`nkb_sys::portal`), which
//! the desktop needs and which nothing writes by itself. `first N` asks for the first N
//! shortcuts of the defaults only, which keeps the window of the system short. The run ends by
//! itself after S seconds (30 by default) and lets every shortcut go.
//!
//! Output, one fact per line: `entry present|absent`, `made <path>`, `refused <words>`,
//! `outcome <action> registered|not-given`, `given <action> | <words> | <chord or ->`, `ready`,
//! `pressed <action> <ms>` and `stopped`.

// A harness prints, and a `println!` that fails to flush is not worth handling.
#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::time::{Duration, Instant};

use nkb_adapters::{PortalShortcuts, default_bindings, i18n};
use nkb_app::ports::{HotkeyRegistrar, ShortcutRegistration, Wait};
use nkb_sys::portal::{Written, entry_present, write_entry};

const TICK: Duration = Duration::from_millis(100);

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let after = |name: &str| {
        args.iter()
            .position(|arg| arg == name)
            .and_then(|at| args.get(at + 1))
            .and_then(|value| value.parse::<usize>().ok())
    };
    let seconds = after("seconds").unwrap_or(30);

    println!(
        "entry {}",
        if entry_present() { "present" } else { "absent" }
    );
    if args.iter().any(|arg| arg == "make-entry") {
        match write_entry() {
            Ok(Written::Made(path)) => println!("made {path}"),
            Ok(Written::AlreadyThere) => println!("already-there"),
            Err(error) => {
                println!("failed {error}");
                std::process::exit(1);
            }
        }
    }

    let table = default_bindings();
    let asked: Vec<_> = table
        .as_slice()
        .iter()
        .take(after("first").unwrap_or(usize::MAX))
        .copied()
        .collect();
    let live = match PortalShortcuts.register(&asked) {
        Ok(live) => live,
        Err(error) => {
            println!("refused {error}");
            println!("{}", i18n::shortcuts_unavailable(&error));
            std::process::exit(4);
        }
    };
    for (action, outcome) in live.outcomes() {
        let word = match outcome {
            ShortcutRegistration::Registered => "registered",
            ShortcutRegistration::NotGiven => "not-given",
            ShortcutRegistration::Taken | ShortcutRegistration::Failed { .. } => "odd",
        };
        println!("outcome {} {word}", i18n::action_name(*action));
    }
    for (action, given) in live.given() {
        let chord = given.chord.map_or_else(|| String::from("-"), i18n::chord);
        println!(
            "given {} | {} | {chord}",
            i18n::action_name(*action),
            given.words
        );
    }
    println!("ready");

    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(u64::try_from(seconds).unwrap_or(30)) {
        match live.next(TICK) {
            Wait::Pressed(action) => println!(
                "pressed {} {}",
                i18n::action_name(action),
                started.elapsed().as_millis()
            ),
            Wait::Nothing => {}
            Wait::Gone => {
                println!("gone");
                break;
            }
        }
    }
    drop(live);
    println!("stopped");
}
