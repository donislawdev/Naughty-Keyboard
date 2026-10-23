//! Registers the shortcuts given on the command line and prints what happens, for a
//! script that presses the keys (`tools/sondy/linux/xorg-fizyczny/skrot-sceny.sh`).
//!
//! ```text
//! cargo run -p nkb-sys --example hotkey_scene -- 8000 1=Alt+Shift+N 2=Alt+Shift+Space
//! ```
//!
//! The first argument is how long to listen, in milliseconds. Each of the others is an
//! id and a chord. It prints one line for every fact, so a script reads the lines and
//! needs no knowledge of the program:
//!
//! ```text
//! route <true|false>            whether this machine has a route to a shortcut
//! outcome <id> <Registered|Taken|Failed 0x...>
//! ready                         the shortcuts are held, press now
//! fired <id> <ms since ready>
//! stopped                       the listener was stopped and the shortcuts let go
//! ```

// A harness prints, and a `println!` that fails to flush is not worth handling.
#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::time::{Duration, Instant};

use nkb_sys::hotkey::{self, Hotkey, HotkeyId, HotkeyRegistration};

/// The code a key of the vocabulary is written in: letters and digits are their ASCII
/// codes, `F1` to `F24` run from `0x70`, and Space is `0x20`.
fn vk_of(key: &str) -> Option<u16> {
    let upper = key.to_ascii_uppercase();
    if upper == "SPACE" {
        return Some(0x20);
    }
    if let Some(number) = upper.strip_prefix('F').and_then(|n| n.parse::<u16>().ok()) {
        return (1..=24).contains(&number).then(|| 0x70 + number - 1);
    }
    let mut chars = upper.chars();
    match (chars.next(), chars.next()) {
        (Some(letter), None) if letter.is_ascii_alphanumeric() => Some(letter as u16),
        _ => None,
    }
}

fn parse(spec: &str) -> Option<Hotkey> {
    let (id, chord) = spec.split_once('=')?;
    let mut hotkey = Hotkey {
        id: HotkeyId(id.parse().ok()?),
        ctrl: false,
        alt: false,
        shift: false,
        win: false,
        vk: 0,
    };
    for part in chord.split('+') {
        match part.to_ascii_lowercase().as_str() {
            "ctrl" => hotkey.ctrl = true,
            "alt" => hotkey.alt = true,
            "shift" => hotkey.shift = true,
            "win" | "super" => hotkey.win = true,
            _ => hotkey.vk = vk_of(part)?,
        }
    }
    (hotkey.vk != 0).then_some(hotkey)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(listen_ms) = args.first().and_then(|ms| ms.parse::<u64>().ok()) else {
        println!("usage: hotkey_scene <ms> <id=chord>...");
        return;
    };
    let Some(wanted) = args[1..]
        .iter()
        .map(|spec| parse(spec))
        .collect::<Option<Vec<Hotkey>>>()
    else {
        println!("a chord could not be read");
        return;
    };
    println!("route {}", hotkey::can_register());
    let listening = match hotkey::listen(&wanted) {
        Ok(listening) => listening,
        Err(unavailable) => {
            println!("unavailable {unavailable}");
            return;
        }
    };
    for (hotkey, outcome) in wanted.iter().zip(&listening.outcomes) {
        match outcome {
            HotkeyRegistration::Registered => println!("outcome {} Registered", hotkey.id.0),
            HotkeyRegistration::Taken => println!("outcome {} Taken", hotkey.id.0),
            HotkeyRegistration::Failed { code } => {
                println!("outcome {} Failed 0x{code:x}", hotkey.id.0);
            }
        }
    }
    println!("ready");
    let started = Instant::now();
    let deadline = started + Duration::from_millis(listen_ms);
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        match listening.fired.recv_timeout(left) {
            Ok(id) => println!("fired {} {}", id.0, started.elapsed().as_millis()),
            Err(_) => break,
        }
    }
    listening.listener.stop();
    println!("stopped");
}
