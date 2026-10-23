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
//! - `hotkey` - the global shortcuts of the palette, grabs held by a thread,
//! - `window` - the keyboard focus of the palette and of the windows that take it,
//! - `send` - typing: spare keys rewritten to the characters, batches, and the
//!   answer of the application after every batch.
//!
//! The decisions that need no server are in `linux_rules.rs` and tested on every
//! system.
//!
//! A Wayland session is not typed into through the X server: through the portal and
//! through XWayland only the characters of the compositor's layout arrive (`OBS-203`),
//! and a send that types half a value and says it sent it is the one thing this program
//! does not do (untouchable rule 1). The text goes to the desktop's input method instead
//! (`ibus`). The palette's window stands on XWayland, and its shortcuts are asked of the
//! portal of the desktop (`portal`) instead of grabbed, because the X server of a Wayland
//! session never sees the keys of a window that is not an X client.
//!
//! - `bus` - the file socket of a bus of this machine, which `ibus` and `portal` both use,
//! - `ibus` - typing on a Wayland session, by committing text from an engine of ours,
//! - `portal` - global shortcuts on a Wayland session, asked of the portal.

mod bus;
mod connect;
mod escape;
mod front;
pub(crate) mod hotkey;
mod ibus;
pub(crate) mod portal;
mod send;
pub(crate) mod window;

use crate::{SendError, SendOutcome, SendProgress, WindowRef};

// `route_refusal` is the question of a real X server alone, which is what the shortcuts
// ask: they have no Wayland route yet, whatever typing has. The windows are at home on
// XWayland too (`window_refusal`, `on_xwayland`).
pub(crate) use connect::{on_xwayland, route_refusal, window_refusal};
pub(crate) use front::window_pid;
pub(crate) use send::send_chords;

use x11rb::errors::{ConnectionError, ReplyError, ReplyOrIdError};

/// Why this program does not type here, in the words that follow `on`, or `None`.
/// A Wayland session is asked of the input method, every other of the X server.
pub(crate) fn typing_refusal() -> Option<&'static str> {
    if connect::wayland_session() {
        ibus::refusal()
    } else {
        connect::route_refusal()
    }
}

/// Whether this program may type here right now.
pub(crate) fn send_allowed() -> bool {
    typing_refusal().is_none()
}

/// The window in front. A Wayland session cannot name it (`ibus::front`).
pub(crate) fn foreground_window() -> Option<WindowRef> {
    if connect::wayland_session() {
        ibus::front()
    } else {
        front::foreground_window()
    }
}

/// Types `text`, through the input method on a Wayland session and through the keys of
/// the X server on every other.
pub(crate) fn send_text(
    text: &str,
    progress: &mut dyn FnMut(SendProgress),
) -> Result<SendOutcome, SendError> {
    if connect::wayland_session() {
        ibus::send_text(text, progress)
    } else {
        send::send_text(text, progress)
    }
}

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
