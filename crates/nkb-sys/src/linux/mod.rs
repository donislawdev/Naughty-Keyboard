//! The Linux route: typing into an X11 session (`D145`, `OBS-202`).
//!
//! One file per question, so each can be read against the measurement that stands
//! behind it (`linux_rules.rs` has the numbers):
//!
//! - `connect` - whether there is an X server this program may talk to, and the
//!   connection to it, local socket only,
//! - `front` - which window holds the keyboard focus, and which window of it
//!   answers `_NET_WM_PING`,
//! - `escape` - `Escape` held for as long as one send lasts,
//! - `send` - typing: spare keys rewritten to the characters, batches, and the
//!   answer of the application after every batch.
//!
//! The decisions that need no server are in `linux_rules.rs` and tested on every
//! system.
//!
//! Wayland is not here, on purpose: through the portal and through XWayland only
//! the characters of the compositor's layout arrive (`OBS-203`), and a send that
//! types half a value and says it sent it is the one thing this program does not
//! do (untouchable rule 1). A Wayland session is refused by name instead.

mod connect;
mod escape;
mod front;
mod send;

pub(crate) use connect::route_refusal;
pub(crate) use front::{foreground_window, window_pid};
pub(crate) use send::{send_allowed, send_chords, send_text};

use x11rb::errors::{ConnectionError, ReplyError, ReplyOrIdError};

/// The connection to the server failed in the middle of a question. Nothing more
/// is known than that, and nothing more is needed: the caller reports what it had
/// already delivered.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Lost;

impl From<ConnectionError> for Lost {
    fn from(_: ConnectionError) -> Self {
        Self
    }
}

impl From<ReplyError> for Lost {
    fn from(_: ReplyError) -> Self {
        Self
    }
}

impl From<ReplyOrIdError> for Lost {
    fn from(_: ReplyOrIdError) -> Self {
        Self
    }
}
