//! A live check of the global-hotkey listener, for a person at the keyboard.
//!
//! The deterministic tests in `hotkey.rs` prove registration, teardown, release
//! and the "taken" path without anyone pressing a key. What they cannot prove is
//! the last link: that a real press actually arrives on the channel through the
//! production path. This example is that proof, and it is the runnable artefact
//! for piece C3 of step 3 - `product-spec.md` 11.
//!
//! Run it, then press Ctrl+Alt+N. Each press prints a line. After a short window
//! it stops on its own, which also demonstrates a clean teardown live rather
//! than only in a test:
//!
//! ```text
//! cargo run -p nkb-sys --example hotkey_listen
//! ```
//!
//! It registers two of the default shortcuts from `ux-spec.md` 3 - "next value"
//! and "show and hide the palette" - so a taken combination shows up as such
//! instead of looking like a dead key.

// A harness prints, and a `println!` that fails to flush is not worth handling.
#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::time::{Duration, Instant};

use nkb_sys::hotkey::{self, Hotkey, HotkeyId, HotkeyRegistration};

// Virtual-key codes for the two letters, so this example needs no platform
// crate of its own. On Windows a letter's VK is its uppercase ASCII value.
const VK_N: u16 = 0x4E;
const VK_H: u16 = 0x48;

const NEXT_VALUE: HotkeyId = HotkeyId(1);
const SHOW_HIDE: HotkeyId = HotkeyId(2);

const LISTEN_FOR: Duration = Duration::from_secs(20);

fn ctrl_alt(id: HotkeyId, vk: u16) -> Hotkey {
    Hotkey {
        id,
        ctrl: true,
        alt: true,
        shift: false,
        win: false,
        vk,
    }
}

fn name(id: HotkeyId) -> &'static str {
    match id {
        NEXT_VALUE => "next value (Ctrl+Alt+N)",
        SHOW_HIDE => "show/hide  (Ctrl+Alt+H)",
        _ => "unknown",
    }
}

fn main() {
    if !hotkey::can_register() {
        println!("This build has no global-shortcut route; nothing to listen for here.");
        return;
    }

    let wanted = [ctrl_alt(NEXT_VALUE, VK_N), ctrl_alt(SHOW_HIDE, VK_H)];
    let listening = match hotkey::listen(&wanted) {
        Ok(listening) => listening,
        Err(unavailable) => {
            println!("{unavailable}");
            return;
        }
    };

    for (hotkey, outcome) in wanted.iter().zip(&listening.outcomes) {
        match outcome {
            HotkeyRegistration::Registered => println!("registered: {}", name(hotkey.id)),
            HotkeyRegistration::Taken => {
                println!("TAKEN by another app: {} - pick another", name(hotkey.id));
            }
            HotkeyRegistration::Failed { code } => {
                println!("failed ({code}): {}", name(hotkey.id));
            }
        }
    }

    println!("\nPress Ctrl+Alt+N or Ctrl+Alt+H. Listening for {LISTEN_FOR:?}...\n");

    let deadline = Instant::now() + LISTEN_FOR;
    let mut count = 0u32;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        match listening.fired.recv_timeout(left) {
            Ok(id) => {
                count += 1;
                println!("  fired #{count}: {}", name(id));
            }
            Err(_) => break, // timed out, or the listener thread ended
        }
    }

    // Stops the thread and releases both shortcuts, now rather than at exit.
    listening.listener.stop();
    println!("\nStopped and released. {count} press(es) seen.");
}
