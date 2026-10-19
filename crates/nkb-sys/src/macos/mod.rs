//! The macOS route: what `nkb send` and the palette need from the system.
//!
//! One file per question, so each can be read against the measurement that
//! stands behind it (`D138`, `OBS-70`, and the numbers in `macos_rules.rs`):
//!
//! - `trust` - whether the system lets this program type at all,
//! - `front` - which application is in front, and what its program is called,
//! - `send` - typing and pressing, one event at a time, each waited for,
//! - `pulse` - holding the send to the pace of the application,
//! - `ax` - what holds the keyboard focus, by accessibility,
//! - `ffi` - the calls themselves.
//!
//! The decisions that need no system are in `macos_rules.rs` and tested on every
//! system. The `Escape` key during a send is `escape.rs`, which Windows shares.

mod ax;
pub(crate) mod ffi;
mod front;
mod pulse;
mod send;
mod trust;

pub(crate) use ax::focused_input;
pub(crate) use front::{foreground_window, image_path};
pub(crate) use send::{send_allowed, send_chords, send_text};
