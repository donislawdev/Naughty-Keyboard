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

use nkb_adapters::{BuiltInCatalogue, DirectInjection, GlobalShortcuts, TomlPackFormat};
use nkb_app::ports::{HotkeyRegistrar, ShortcutRegistration};
use nkb_app::{AdvanceSequence, Message, Outcome, drive_sequence};
use nkb_core::hotkeys::DEFAULT_BINDINGS;

const PACK: &str = "whitespace";
const RUN_FOR: Duration = Duration::from_secs(30);
const TICK: Duration = Duration::from_millis(100);

fn main() {
    let format = TomlPackFormat;
    let catalogue = BuiltInCatalogue::new();
    let mut sequence = AdvanceSequence::new();
    if let Err(error) = sequence.choose_pack(&catalogue, &format, PACK) {
        eprintln!("could not choose the built-in pack {PACK}: {error:?}");
        std::process::exit(1);
    }
    let Some((_, total)) = sequence.counter() else {
        eprintln!("the pack was chosen but the counter is empty");
        std::process::exit(1);
    };

    let live = match GlobalShortcuts.register(&DEFAULT_BINDINGS) {
        Ok(live) => live,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(4);
        }
    };
    for (action, outcome) in live.outcomes() {
        match outcome {
            ShortcutRegistration::Registered => {}
            ShortcutRegistration::Taken => println!("{action:?}: TAKEN by another application"),
            ShortcutRegistration::Failed { code } => {
                println!("{action:?}: FAILED with code {code}")
            }
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
    println!("ended: {ended:?} - every shortcut released");
}

/// One line per outcome. Words live here, in the harness, not in `app`.
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
        line.push_str(&match message {
            Message::EndOfPack { total } => {
                format!("end of pack ({total}); press again to start over")
            }
            Message::CounterKept { done, total } => format!("counter kept at {done}/{total}"),
            Message::NoPack => "no pack chosen".to_owned(),
            Message::NoTarget => "nothing has the focus, nothing sent".to_owned(),
            Message::ModifierHeld { key } => format!("release {key} and try again"),
            Message::ClearingFailed => "the field could not be cleared".to_owned(),
            Message::Interrupted {
                units_sent,
                units_expected,
            } => format!("interrupted after {units_sent} of {units_expected} units"),
            Message::Degraded => "direct delivery failed; clipboard mode".to_owned(),
            Message::ValueTooLarge { id } => format!("value {id} is too large"),
            Message::Unhandled { action } => format!("{action:?} is not wired yet"),
            Message::PressedWhileBusy { action } => {
                format!("{action:?} pressed while the previous send was running - ignored")
            }
        });
    }
    line
}
