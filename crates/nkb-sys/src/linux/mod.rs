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
//! - `stream` - the socket under every connection, which waits for the server for a
//!   bounded time: the library alone would wait for ever.
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
//! - `bus` - the file socket of a bus of this machine, which `ibus`, `portal` and `atspi` all use,
//! - `ibus` - typing on a Wayland session, by committing text from an engine of ours,
//! - `portal` - global shortcuts on a Wayland session, asked of the portal,
//! - `atspi` - the kind of control that holds the keyboard focus, asked of the bus of
//!   accessibility on either kind of session.

mod atspi;
mod bus;
mod connect;
mod escape;
mod front;
pub(crate) mod hotkey;
mod ibus;
pub(crate) mod portal;
mod send;
mod stream;
pub(crate) mod window;

use core::sync::atomic::AtomicBool;

use crate::{SendError, SendOutcome, SendProgress, Waiting, WindowRef};

// `route_refusal` is the question of a real X server alone, which is what the grab of a
// shortcut asks: a Wayland session has the portal instead (`grab_refused_by_wayland` says
// when), whatever typing has. The windows are at home on XWayland too (`window_refusal`,
// `on_xwayland`).
pub(crate) use connect::{grab_refused_by_wayland, on_xwayland, route_refusal, window_refusal};
pub(crate) use front::window_pid;

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

/// What holds the keyboard focus, from the bus of accessibility. The window is not needed:
/// the bus names the window in front itself, which a Wayland session cannot do for us.
pub(crate) fn focused_input() -> crate::field::FocusedInput {
    atspi::focused_input()
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
///
/// `target` is the window the send was meant for (`D148`). A Wayland session cannot name
/// it, so the input method types into whatever has the focus, as it always did.
pub(crate) fn send_text(
    text: &str,
    target: WindowRef,
    halt: &AtomicBool,
    progress: &mut dyn FnMut(SendProgress),
) -> Result<SendOutcome, SendError> {
    if connect::wayland_session() {
        ibus::send_text(text, halt, progress)
    } else {
        send::send_text(text, target, halt, progress)
    }
}

/// Presses the keys that clear a field. On a Wayland session they go through the remote-desktop
/// portal with the engine of the input method standing guard over each (`ibus::press_chords`).
/// On an X11 session they are the keys of the layout, pressed through XTEST, with the server
/// saying whether a modifier is in the way (`send::send_chords`). Only the first has a person
/// to wait for, and says so through `waiting` (`D145` point 76).
pub(crate) fn send_chords(
    chords: &[crate::Chord],
    target: WindowRef,
    halt: &AtomicBool,
    waiting: &mut dyn FnMut(Waiting),
) -> Result<usize, SendError> {
    if connect::wayland_session() {
        ibus::press_chords(chords, halt, waiting)
    } else {
        send::send_chords(chords, target, halt)
    }
}

/// Whether this session has a route for the keys that clear a field: a Wayland session with an
/// input method to stand guard, or a local X server with XTEST. Whether the person allows the
/// portal to press them is asked when they are pressed, because the answer is a window of the
/// system, and whether the layout has the keys is found out when they are pressed.
pub(crate) fn can_press_chords() -> bool {
    if connect::wayland_session() {
        ibus::refusal().is_none()
    } else {
        connect::route_refusal().is_none()
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
