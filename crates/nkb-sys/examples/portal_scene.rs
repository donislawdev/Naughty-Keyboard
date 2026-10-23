//! Asks the portal of the desktop for shortcuts and prints what happens, for a script that
//! presses the keys and answers the window of the system (`tools/petla-skrotow-wayland.py`).
//!
//! ```text
//! cargo run -p nkb-sys --example portal_scene -- entry
//! cargo run -p nkb-sys --example portal_scene -- bind 20000 1=Ctrl+Alt+N 2=Ctrl+Alt+P
//! ```
//!
//! `entry` says whether the entry that gives the program an identifier is there. `entry
//! --make` makes it if it is not and `entry --remove` takes away the one this program
//! made. `bind` takes how long to listen after the bind, in
//! milliseconds, and the shortcuts to ask for, each an id and a chord. It prints one line for
//! every fact, so a script reads the lines and needs no knowledge of the program:
//!
//! ```text
//! entry <present|absent>
//! entry made <path> | entry already-there | entry failed <reason>
//! unavailable <reason>            the portal gave nothing, and why
//! bound <id> <words> | <chord>    a shortcut the portal holds, and the chord in its words
//! ready                           the shortcuts are held, press now
//! fired <id> <ms since ready>
//! stopped                         the listener was stopped and the session closed
//! ```

// A harness prints, and a `println!` that fails to flush is not worth handling.
#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::time::{Duration, Instant};

use nkb_sys::hotkey::{Hotkey, HotkeyId};
use nkb_sys::portal::{self, Removed, Wish, Written};

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

fn chord_text(hotkey: &Hotkey) -> String {
    let mut parts = Vec::new();
    for (held, name) in [
        (hotkey.ctrl, "Ctrl"),
        (hotkey.alt, "Alt"),
        (hotkey.shift, "Shift"),
        (hotkey.win, "Super"),
    ] {
        if held {
            parts.push(name.to_owned());
        }
    }
    parts.push(format!("{:#x}", hotkey.vk));
    parts.join("+")
}

fn entry(args: &[String]) {
    if args.iter().any(|arg| arg == "--remove") {
        match portal::remove_entry() {
            Ok(Removed::Removed) => println!("entry removed"),
            Ok(Removed::NotThere) => println!("entry not-there"),
            Ok(Removed::NotOurs) => println!("entry not-ours"),
            Err(error) => println!("entry failed {error}"),
        }
        return;
    }
    if args.iter().any(|arg| arg == "--make") {
        match portal::write_entry() {
            Ok(Written::Made(path)) => println!("entry made {path}"),
            Ok(Written::AlreadyThere) => println!("entry already-there"),
            Err(error) => println!("entry failed {error}"),
        }
        return;
    }
    println!(
        "entry {}",
        if portal::entry_present() {
            "present"
        } else {
            "absent"
        }
    );
}

fn bind(args: &[String]) {
    let Some(listen_ms) = args.first().and_then(|ms| ms.parse::<u64>().ok()) else {
        println!("usage: portal_scene bind <ms> <id=chord>...");
        return;
    };
    let mut wishes = Vec::new();
    for spec in &args[1..] {
        let Some(hotkey) = parse(spec) else {
            println!("usage: the shortcut {spec:?} is not an id and a chord");
            return;
        };
        wishes.push(Wish {
            name: format!("probe-{}", hotkey.id.0),
            description: format!("Probe shortcut {}", hotkey.id.0),
            hotkey,
        });
    }
    let started = match portal::start(&wishes) {
        Ok(started) => started,
        Err(reason) => {
            println!("unavailable {reason}");
            return;
        }
    };
    for bound in &started.bound {
        let chord = bound
            .chord
            .as_ref()
            .map_or_else(|| String::from("-"), chord_text);
        println!("bound {} {} | {chord}", bound.id.0, bound.words);
    }
    println!("ready");
    let begun = Instant::now();
    let end = begun + Duration::from_millis(listen_ms);
    while let Some(left) = end.checked_duration_since(Instant::now()) {
        if let Ok(id) = started
            .fired
            .recv_timeout(left.min(Duration::from_millis(200)))
        {
            println!("fired {} {}", id.0, begun.elapsed().as_millis());
        }
    }
    started.listener.stop();
    println!("stopped");
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("entry") => entry(&args[1..]),
        Some("bind") => bind(&args[1..]),
        _ => println!("usage: portal_scene <entry [--make] | bind <ms> <id=chord>...>"),
    }
}
