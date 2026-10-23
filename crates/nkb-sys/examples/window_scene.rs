//! Asks what the program says of one window of the display, for a script that made the
//! window and knows what the answer has to be (`tools/petla-palety-wayland.py`).
//!
//! ```text
//! cargo run -p nkb-sys --example window_scene -- refuse 0xc00013
//! ```
//!
//! The first argument is what to ask, the second the id of the window in hexadecimal:
//!
//! ```text
//! refuse     make the window refuse the keyboard, and say whether it does
//! hand-back  say whether the window holds a keyboard it must give back
//! take       bring the window in front with the keyboard, and say whether it took it
//! ```
//!
//! It prints one line for each fact, so the script reads the lines and needs no knowledge
//! of the program:
//!
//! ```text
//! route <true|false>      whether this machine can keep a window from the keyboard
//! xwayland <true|false>   whether the windows stand on the X server of a Wayland session
//! result <Ok|the error>   what the call came to
//! ```

// A harness prints, and a `println!` that fails to flush is not worth handling.
#![allow(clippy::print_stdout, clippy::print_stderr)]

use nkb_sys::WindowRef;
use nkb_sys::window::{
    can_refuse_focus, hand_back_foreground, on_xwayland, refuse_activation, take_foreground,
};

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let [what, id] = arguments.as_slice() else {
        println!("usage: window_scene <refuse|hand-back|take> <window id in hexadecimal>");
        return;
    };
    let Ok(id) = u64::from_str_radix(id.trim_start_matches("0x"), 16) else {
        println!("the window id could not be read");
        return;
    };
    println!("route {}", can_refuse_focus());
    println!("xwayland {}", on_xwayland());
    let window = WindowRef(id);
    let result = match what.as_str() {
        "refuse" => refuse_activation(window),
        "hand-back" => hand_back_foreground(window, None),
        "take" => take_foreground(window),
        _ => {
            println!("the question is not one of refuse, hand-back, take");
            return;
        }
    };
    match result {
        Ok(()) => println!("result Ok"),
        Err(error) => println!("result {error:?}"),
    }
}
