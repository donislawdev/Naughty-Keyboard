//! Keeping the keyboard where the tester put it, and lending it to a window, on X11.
//!
//! # What was measured before this was written
//!
//! Probed 2026-10-23 on a real Xorg with `openbox` and with `xfwm4` (`tools/sondy/linux/
//! xorg-fizyczny/okno-fokus.sh`), the verdict read from the focus of the server and from
//! `_NET_ACTIVE_WINDOW`, each step beside a control without the setting:
//!
//! - a click on a palette window takes the keyboard from the application under test on
//!   both managers (control, 2 of 2). With `WM_HINTS` saying `input = False` the click
//!   leaves the keyboard where it was (2 of 2 on each, a click twice). With `input` put
//!   back to `True` the click takes it again. The hint is read by the manager when it
//!   changes, on a window that is mapped already.
//! - the palette takes the focus once, as it is mapped, on `openbox`. Giving it back to
//!   the window that had it works through a `_NET_ACTIVE_WINDOW` request of either
//!   source (application or pager) and through `SetInputFocus`, on both managers.
//! - a `_NET_ACTIVE_WINDOW` request for a window with `input = False` is ignored by both
//!   managers, and `SetInputFocus` on it works. So a window that must take the
//!   keyboard is simply one that never had the hint set.
//! - the `always-on-top` of the toolkit does not reach the server: `_NET_WM_STATE` of the
//!   palette is empty on both managers. A `_NET_WM_STATE` request to the root window
//!   adding `_NET_WM_STATE_ABOVE` is honoured by both.
//!
//! So the two jobs of the Windows account (`window.rs`) are done by one property and one
//! request here, and as there the verdict is a READ and not the return of a call.
//!
//! # A window of ours and the window in front are not the same number
//!
//! The toolkit hands out the window of the application. With a reparenting manager the
//! window in front is the frame the manager put around it (`front.rs`). Both directions
//! are mapped: ours is walked up to its frame to be compared, and the window to give
//! the keyboard back to is walked down from its frame to the window that has `WM_STATE`,
//! the one the manager manages and the one a request names.

use std::time::{Duration, Instant};

use x11rb::connection::Connection;
use x11rb::properties::WmHints;
use x11rb::protocol::xproto::{
    AtomEnum, ClientMessageEvent, ConnectionExt as _, EventMask, InputFocus,
};
use x11rb::rust_connection::RustConnection;

use super::Lost;
use super::connect::{Server, connect, environment_route};
use super::front::{focused_toplevel, toplevel};
use crate::WindowRef;
use crate::linux_rules::Refusal;
use crate::window::{FocusError, KeyMenuError};

/// How long the manager is given to show a change of the focus. Measured 2026-10-23: both
/// managers had moved it by the first look after the request, a few milliseconds. This is
/// far above that, because a wait that is too short blames the manager for the impatience
/// of this code and one that is too long is only spent when the answer is no.
const SETTLE: Duration = Duration::from_millis(400);

/// How often the window in front is looked at while waiting.
const LOOK: Duration = Duration::from_millis(5);

/// How many levels below a frame a managed window is looked for. A reparenting manager
/// has the window one level down, and a manager with decorations built from several
/// windows has it a few more.
const CLIENT_DEPTH: u8 = 4;

/// The source of a `_NET_ACTIVE_WINDOW` request that managers honour without the
/// focus-stealing rules they hold for an application: the one a pager sends.
const SOURCE_PAGER: u32 = 2;

/// The source of a `_NET_WM_STATE` request made by an application.
const SOURCE_APPLICATION: u32 = 1;

/// `_NET_WM_STATE_ADD`.
const STATE_ADD: u32 = 1;

fn refused(refusal: Refusal) -> FocusError {
    FocusError::Unsupported {
        system: refusal.system(),
    }
}

fn server() -> Result<Server, FocusError> {
    connect(environment_route().map_err(refused)?).map_err(refused)
}

fn id_of(window: WindowRef) -> Result<u32, FocusError> {
    u32::try_from(window.0).map_err(|_| FocusError::HandleUnavailable)
}

fn atom(conn: &RustConnection, name: &[u8]) -> Result<u32, Lost> {
    Ok(conn.intern_atom(false, name)?.reply()?.atom)
}

/// Sends a request of the extended hints to the root window, which is where a manager
/// listens for them.
fn ask_manager(server: &Server, window: u32, kind: u32, data: [u32; 5]) -> Result<(), Lost> {
    let message = ClientMessageEvent::new(32, window, kind, data);
    server.conn.send_event(
        false,
        server.root,
        EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
        message,
    )?;
    server.conn.flush()?;
    Ok(())
}

/// The window the manager manages inside `window`, which has `WM_STATE`.
fn managed_below(
    conn: &RustConnection,
    window: u32,
    wm_state: u32,
    depth: u8,
) -> Result<Option<u32>, Lost> {
    let property = conn
        .get_property(false, window, wm_state, AtomEnum::ANY, 0, 0)?
        .reply()?;
    if property.type_ != u32::from(AtomEnum::NONE) {
        return Ok(Some(window));
    }
    if depth == 0 {
        return Ok(None);
    }
    for child in conn.query_tree(window)?.reply()?.children {
        if let Some(found) = managed_below(conn, child, wm_state, depth - 1)? {
            return Ok(Some(found));
        }
    }
    Ok(None)
}

/// Looks at the window in front until `done` says so, for at most `SETTLE`.
fn wait_front(server: &Server, mut done: impl FnMut(Option<u32>) -> bool) -> bool {
    let started = Instant::now();
    loop {
        if let Ok(front) = focused_toplevel(&server.conn, server.root)
            && done(front)
        {
            return true;
        }
        if started.elapsed() >= SETTLE {
            return false;
        }
        std::thread::sleep(LOOK);
    }
}

/// Asks for the keyboard for `window`: the request of a pager first, and `SetInputFocus`
/// when the manager did not move it.
fn bring_to_front(
    server: &Server,
    window: u32,
    front_is_ready: impl Fn(Option<u32>) -> bool,
) -> bool {
    let Ok(active) = atom(&server.conn, b"_NET_ACTIVE_WINDOW") else {
        return false;
    };
    if ask_manager(server, window, active, [SOURCE_PAGER, 0, 0, 0, 0]).is_err() {
        return false;
    }
    if wait_front(server, &front_is_ready) {
        return true;
    }
    let set = server
        .conn
        .set_input_focus(InputFocus::PARENT, window, x11rb::CURRENT_TIME)
        .is_ok_and(|cookie| cookie.check().is_ok());
    set && wait_front(server, &front_is_ready)
}

/// Makes the manager leave the palette's keyboard alone and keeps it above the other
/// windows. The first by `WM_HINTS` saying the window takes no input, which the manager
/// reads as "never give this window the focus", the second by a request to add
/// `_NET_WM_STATE_ABOVE`, because the toolkit's own never reached the server.
pub(crate) fn refuse_activation(window: WindowRef) -> Result<(), FocusError> {
    let server = server()?;
    match refuse_on(&server, id_of(window)?) {
        Ok(true) => Ok(()),
        Ok(false) | Err(Lost) => Err(FocusError::StyleRefused),
    }
}

/// Writes the hints and the request, and answers whether the server holds the hints.
fn refuse_on(server: &Server, id: u32) -> Result<bool, Lost> {
    let mut hints = WmHints::get(&server.conn, id)?.reply()?.unwrap_or_default();
    hints.input = Some(false);
    hints.set(&server.conn, id)?.check()?;
    // Above everything, whether the manager listens or not. Not read back: a manager that
    // ignores it leaves the palette under the application, which the tester sees.
    let state = atom(&server.conn, b"_NET_WM_STATE")?;
    let above = atom(&server.conn, b"_NET_WM_STATE_ABOVE")?;
    ask_manager(
        server,
        id,
        state,
        [STATE_ADD, above, 0, SOURCE_APPLICATION, 0],
    )?;
    // The verdict from a READ: the hints are what the server holds now.
    let read = WmHints::get(&server.conn, id)?.reply()?;
    Ok(read.is_some_and(|hints| hints.input == Some(false)))
}

/// Gives the keyboard back to `previous`, if `ours` holds it.
pub(crate) fn hand_back_foreground(
    ours: WindowRef,
    previous: Option<WindowRef>,
) -> Result<(), FocusError> {
    let server = server()?;
    let id = id_of(ours)?;
    let still = |_: Lost| FocusError::StillForeground;
    // A window the server does not know holds nothing, so there is nothing to give back.
    let Ok(frame) = toplevel(&server.conn, server.root, id) else {
        return Ok(());
    };
    if focused_toplevel(&server.conn, server.root).map_err(still)? != Some(frame) {
        // We never took it, or have given it back already.
        return Ok(());
    }
    let Some(previous) = previous else {
        return Ok(());
    };
    let wm_state = atom(&server.conn, b"WM_STATE").map_err(still)?;
    let wanted = u32::try_from(previous.0).map_err(|_| FocusError::StillForeground)?;
    let target = managed_below(&server.conn, wanted, wm_state, CLIENT_DEPTH)
        .map_err(still)?
        .ok_or(FocusError::StillForeground)?;
    if bring_to_front(&server, target, |front| front != Some(frame)) {
        Ok(())
    } else {
        Err(FocusError::StillForeground)
    }
}

/// Brings `window`, one of ours, to the front with the keyboard.
pub(crate) fn take_foreground(window: WindowRef) -> Result<(), FocusError> {
    let server = server()?;
    let id = id_of(window)?;
    let not_taken = |_: Lost| FocusError::NotTaken;
    let frame = toplevel(&server.conn, server.root, id).map_err(not_taken)?;
    if bring_to_front(&server, id, |front| front == Some(frame)) {
        Ok(())
    } else {
        Err(FocusError::NotTaken)
    }
}

/// X11 has no menu a window opens from the keyboard on its own: a manager that binds
/// `Alt+Space` to one grabs the chord before any window sees it, which no window can
/// prevent. So there is nothing to keep off. What is read is that the window is there,
/// because "nothing to do" said of a window that does not exist would be a claim about
/// nothing.
pub(crate) fn no_keyboard_menu(window: WindowRef) -> Result<(), KeyMenuError> {
    let refused = |refusal: Refusal| KeyMenuError::Unsupported {
        system: refusal.system(),
    };
    let id = u32::try_from(window.0).map_err(|_| KeyMenuError::HandleUnavailable)?;
    let address = environment_route().map_err(refused)?;
    let server = connect(address).map_err(refused)?;
    server
        .conn
        .get_window_attributes(id)
        .ok()
        .and_then(|cookie| cookie.reply().ok())
        .map(|_| ())
        .ok_or(KeyMenuError::Refused)
}
