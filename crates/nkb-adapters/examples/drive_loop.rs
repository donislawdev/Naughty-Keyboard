//! The palette's core loop, live, without a palette: shortcuts drive a pack
//! into whatever field has the focus.
//!
//! The deterministic tests prove the port, the adapter and the loop apart.
//! What they cannot prove is the chain end to end through the production
//! path - a real press on `Ctrl+Alt+N` arriving as `HotkeyAction::NextValue`,
//! going through `AdvanceSequence`, and landing as the next value of a
//! built-in pack in a real field. This example is that proof, and it is the
//! runnable artefact for the rest of piece C4a-R of step 3 - the thing piece
//! C5 wraps in a window.
//!
//! Run it, click into any text field (a notepad will do), and press
//! `Ctrl+Alt+N` a few times, `Ctrl+Alt+P` for the previous value, or
//! `Ctrl+Alt+0` to start over. Each press prints one line naming what
//! happened. It stops on its own after a short window, releasing every
//! shortcut:
//!
//! ```text
//! cargo run -p nkb-adapters --example drive_loop
//! ```
//!
//! All ten defaults from `ux-spec.md` 3 are registered, so a taken combination
//! shows up as such at startup, and a shortcut that is not wired yet answers
//! "unhandled" rather than doing nothing.

// A harness prints, and a `println!` that fails to flush is not worth handling.
#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::time::{Duration, Instant};

use nkb_adapters::{BuiltInCatalogue, DirectInjection, GlobalShortcuts, TomlPackFormat, i18n};
use nkb_app::ports::HotkeyRegistrar;
use nkb_app::{AdvanceSequence, Outcome, drive_sequence};
use nkb_core::hotkeys::DEFAULT_BINDINGS;

const PACK: &str = "whitespace";
const RUN_FOR: Duration = Duration::from_secs(30);
const TICK: Duration = Duration::from_millis(100);

fn main() {
    let format = TomlPackFormat;
    let catalogue = BuiltInCatalogue::new();
    let mut sequence = AdvanceSequence::new();
    if let Err(error) = sequence.choose_pack(&catalogue, &format, PACK) {
        eprintln!("{}", i18n::choose_error(&error, PACK));
        std::process::exit(1);
    }
    let Some((_, total)) = sequence.counter() else {
        eprintln!("the pack was chosen but the counter is empty");
        std::process::exit(1);
    };

    let live = match GlobalShortcuts.register(&DEFAULT_BINDINGS) {
        Ok(live) => live,
        Err(error) => {
            eprintln!("{}", i18n::shortcuts_unavailable(&error));
            std::process::exit(4);
        }
    };
    // A registration that worked says nothing - `i18n::registration` returns
    // `None` for it, so the silence is the dictionary's decision rather than
    // this harness's.
    for (action, outcome) in live.outcomes() {
        if let Some(line) = i18n::registration(outcome, *action) {
            println!("{line}");
        }
    }
    println!(
        "pack {PACK} chosen, {total} values; click into a text field and press Ctrl+Alt+N (next), \
         Ctrl+Alt+P (previous), Ctrl+Alt+0 (restart) - stopping in {} s",
        RUN_FOR.as_secs()
    );

    let started = Instant::now();
    let mut keep_going = || started.elapsed() < RUN_FOR;
    let mut present = |outcome: Outcome| println!("{}", describe(&outcome));
    let ended = drive_sequence(
        live.as_ref(),
        &mut sequence,
        &DirectInjection,
        &DirectInjection,
        TICK,
        &mut keep_going,
        &mut present,
    );
    drop(live);
    if let Some(line) = i18n::ended(ended) {
        println!("{line}");
    }
    println!("every shortcut released");
}

/// One line per outcome.
///
/// The counter and the delivery facts are formatted here, because they are
/// diagnostics for whoever runs this harness. The SENTENCES are not: they come
/// from `i18n`, so this example and the palette cannot end up saying two
/// different things about the same event.
fn describe(outcome: &Outcome) -> String {
    let counter = outcome.sequence.counter().map_or_else(
        || "-/-".to_owned(),
        |(done, total)| format!("{done}/{total}"),
    );
    let mut line = format!("[{counter}]");
    if let Some(sent) = &outcome.sent {
        line.push_str(&format!(
            " sent {} ({} code points, cleared: {})",
            sent.reference, sent.code_points, sent.cleared
        ));
    }
    for message in &outcome.messages {
        line.push(' ');
        line.push_str(&i18n::message(message, PACK));
    }
    line
}
