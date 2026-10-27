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
//! # Stepping aside from a window of the system
//!
//! The palette on XWayland is a notification, and mutter keeps that type in the top layer
//! whatever the state says, so taking `_NET_WM_STATE_ABOVE` off it changes nothing. The
//! type goes with the state, and both go BEFORE the window of the system opens, because a
//! window that is moved to the normal layer is put on top of the layer, above a window that
//! is in it (`step_aside`, and `linux_window_rules.rs` for the table of what was measured).
//!
//! # XWayland: the same property and request, and no frame besides
//!
//! On a Wayland session the palette is a client of XWayland (`D145` point 38). Probed
//! 2026-10-23 on GNOME 50 with a native application under test and keys and clicks through
//! the portal (`linux_window_rules.rs` has the numbers): mutter ignores the hint on a window
//! with a frame of its own and honours it on one without, whatever the type of the window.
//! The frame is given up when the window is made (`nkb-gui`, `focus.rs`), and here it is
//! READ, because a window the manager drew a frame round is one that a click gives the
//! keyboard to. The keyboard cannot be given back to an application of Wayland from here,
//! so what is checked is that the palette never holds it, and a window that takes it on
//! purpose is trusted to lose it by being hidden.
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
    AtomEnum, ClientMessageEvent, ConnectionExt as _, EventMask, InputFocus, PropMode,
};
use x11rb::wrapper::ConnectionExt as _;

use super::Lost;
use super::connect::{Display, Server, Windows, connect_windows};
use super::front::{focused_toplevel, toplevel};
use super::stream::Conn;
use crate::WindowRef;
use crate::linux_rules::Refusal;
use crate::linux_window_rules::{
    StillHeld, Stood, hand_back, holds_keyboard, is_aside, is_back, is_frameless,
};
use crate::window::{FocusError, KeyMenuError, StackingError};

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

/// How many atoms of a property are read: far more than any window lists, so a list
/// is never cut short.
const ATOM_LIST_LONGEST: u32 = 64;

/// `_NET_WM_STATE_ADD`.
const STATE_ADD: u32 = 1;

/// `_NET_WM_STATE_REMOVE`.
const STATE_REMOVE: u32 = 0;

fn refused(refusal: Refusal) -> FocusError {
    FocusError::Unsupported {
        system: refusal.system(),
    }
}

fn windows() -> Result<Windows, FocusError> {
    connect_windows().map_err(refused)
}

fn id_of(window: WindowRef) -> Result<u32, FocusError> {
    u32::try_from(window.0).map_err(|_| FocusError::HandleUnavailable)
}

fn atom(conn: &Conn, name: &[u8]) -> Result<u32, Lost> {
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
fn managed_below(conn: &Conn, window: u32, wm_state: u32, depth: u8) -> Result<Option<u32>, Lost> {
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
///
/// On XWayland the hint is not enough: mutter ignores it on a window it has framed
/// (`OBS-218`, `OBS-221`). The palette is made without a frame. That is not done later,
/// because a frame taken off a window that has drawn none of its own would leave it with
/// nothing to move it by. So here it is READ, and a window that has a frame is told not
/// to refuse the keyboard.
pub(crate) fn refuse_activation(window: WindowRef) -> Result<(), FocusError> {
    let windows = windows()?;
    let id = id_of(window)?;
    let refused = refuse_on(&windows.server, id).and_then(|held| match windows.display {
        Display::X11 => Ok(held),
        Display::XWayland => Ok(held && has_no_frame(&windows.server, id)?),
    });
    match refused {
        Ok(true) => Ok(()),
        Ok(false) | Err(Lost) => Err(FocusError::StyleRefused),
    }
}

/// The atoms a property of `window` holds, for a property of that type.
fn atoms_of(conn: &Conn, window: u32, property: &[u8]) -> Result<Vec<u32>, Lost> {
    let property = atom(conn, property)?;
    let reply = conn
        .get_property(
            false,
            window,
            property,
            AtomEnum::ATOM,
            0,
            ATOM_LIST_LONGEST,
        )?
        .reply()?;
    Ok(reply.value32().map(Iterator::collect).unwrap_or_default())
}

/// Whether the manager has put no frame round the window: `_NET_FRAME_EXTENTS` is written
/// for a window it has framed and not for one it has not.
fn has_no_frame(server: &Server, window: u32) -> Result<bool, Lost> {
    let property = atom(&server.conn, b"_NET_FRAME_EXTENTS")?;
    let reply = server
        .conn
        .get_property(false, window, property, AtomEnum::CARDINAL, 0, 4)?
        .reply()?;
    // Not written at all is no frame. Written in a shape that is not four edges is not
    // something this reads as no frame.
    Ok(match reply.value32().map(Iterator::collect::<Vec<u32>>) {
        None => is_frameless(None),
        Some(edges) => <[u32; 4]>::try_from(edges).is_ok_and(|edges| is_frameless(Some(edges))),
    })
}

/// Whether the window holds the keyboard, as the manager and the server say it.
fn holds_the_keyboard(server: &Server, window: u32) -> Result<bool, Lost> {
    let focused = atom(&server.conn, b"_NET_WM_STATE_FOCUSED")?;
    let state = atoms_of(&server.conn, window, b"_NET_WM_STATE")?;
    let input = focused_toplevel(&server.conn, server.root)?;
    Ok(holds_keyboard(&state, focused, input, window))
}

/// Giving the keyboard back on XWayland. A client of the X server cannot name the
/// application of Wayland that had it, and cannot give it to that application either, so
/// there is no call to make. What is read is whether a window that must never hold the
/// keyboard does (`linux_window_rules.rs`).
fn hand_back_on_xwayland(server: &Server, window: u32) -> Result<(), FocusError> {
    let still = |_: Lost| FocusError::StillForeground;
    let holds = holds_the_keyboard(server, window).map_err(still)?;
    let frameless = has_no_frame(server, window).map_err(still)?;
    hand_back(holds, frameless).map_err(|StillHeld| FocusError::StillForeground)
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
    let Windows { server, display } = windows()?;
    let id = id_of(ours)?;
    let still = |_: Lost| FocusError::StillForeground;
    // A window the server does not know holds nothing, so there is nothing to give back.
    let Ok(frame) = toplevel(&server.conn, server.root, id) else {
        return Ok(());
    };
    if display == Display::XWayland {
        return hand_back_on_xwayland(&server, id);
    }
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
    let Windows { server, display } = windows()?;
    let id = id_of(window)?;
    let not_taken = |_: Lost| FocusError::NotTaken;
    let frame = toplevel(&server.conn, server.root, id).map_err(not_taken)?;
    // On XWayland the window is put in a frame of the compositor's AFTER it is shown, so
    // the top-level window found now is not the one the focus will be reported in: the
    // verdict is asked of the window each time (`OBS-221`, a window that held the keyboard
    // was said not to).
    let ready = |front: Option<u32>| match display {
        Display::X11 => front == Some(frame),
        Display::XWayland => holds_the_keyboard(&server, id).unwrap_or(false),
    };
    if bring_to_front(&server, id, ready) {
        Ok(())
    } else {
        Err(FocusError::NotTaken)
    }
}

/// The atoms the stacking of a window is read and told in.
struct StackAtoms {
    kind: u32,
    normal: u32,
    state: u32,
    above: u32,
    skip_taskbar: u32,
    skip_pager: u32,
}

fn stack_atoms(conn: &Conn) -> Result<StackAtoms, Lost> {
    Ok(StackAtoms {
        kind: atom(conn, b"_NET_WM_WINDOW_TYPE")?,
        normal: atom(conn, b"_NET_WM_WINDOW_TYPE_NORMAL")?,
        state: atom(conn, b"_NET_WM_STATE")?,
        above: atom(conn, b"_NET_WM_STATE_ABOVE")?,
        skip_taskbar: atom(conn, b"_NET_WM_STATE_SKIP_TASKBAR")?,
        skip_pager: atom(conn, b"_NET_WM_STATE_SKIP_PAGER")?,
    })
}

/// Looks at the type and the state of the window until `done` says so, for at most `SETTLE`.
fn wait_stack(
    server: &Server,
    window: u32,
    done: impl Fn(&[u32], &[u32]) -> bool,
) -> Result<bool, Lost> {
    let started = Instant::now();
    loop {
        let types = atoms_of(&server.conn, window, b"_NET_WM_WINDOW_TYPE")?;
        let state = atoms_of(&server.conn, window, b"_NET_WM_STATE")?;
        if done(&types, &state) {
            return Ok(true);
        }
        if started.elapsed() >= SETTLE {
            return Ok(false);
        }
        std::thread::sleep(LOOK);
    }
}

/// Writes the type of the window and tells the manager to take it off the top, and answers
/// whether the manager shows it so. The type goes first: the layer of the window is worked
/// out from the type, and the state that is asked for after it is worked out with it.
/// Out of the taskbar and the switcher it is kept, which the notification type did and the
/// normal type does not.
fn put_aside(server: &Server, window: u32, atoms: &StackAtoms) -> Result<bool, Lost> {
    set_types(server, window, atoms.kind, &[atoms.normal])?;
    let keep_out = [
        STATE_ADD,
        atoms.skip_taskbar,
        atoms.skip_pager,
        SOURCE_APPLICATION,
        0,
    ];
    ask_manager(server, window, atoms.state, keep_out)?;
    let leave = [STATE_REMOVE, atoms.above, 0, SOURCE_APPLICATION, 0];
    ask_manager(server, window, atoms.state, leave)?;
    wait_stack(server, window, |types, state| {
        is_aside(types, state, atoms.normal, atoms.above)
    })
}

/// Puts the type back, asks for the top again if the window had it, and answers whether the
/// manager shows the window as it was.
fn put_back(server: &Server, window: u32, atoms: &StackAtoms, stood: &Stood) -> Result<bool, Lost> {
    set_types(server, window, atoms.kind, &stood.types)?;
    if stood.above {
        let again = [STATE_ADD, atoms.above, 0, SOURCE_APPLICATION, 0];
        ask_manager(server, window, atoms.state, again)?;
    }
    wait_stack(server, window, |types, state| {
        is_back(types, state, stood, atoms.above)
    })
}

fn set_types(server: &Server, window: u32, kind: u32, types: &[u32]) -> Result<(), Lost> {
    server
        .conn
        .change_property32(PropMode::REPLACE, window, kind, AtomEnum::ATOM, types)?
        .check()?;
    Ok(())
}

/// Takes the window off the top of the others, for the time of a question of the system to
/// the person (`D145` point 77, `linux_window_rules.rs` has the measurement). Only on
/// XWayland, where a window of the system opens over ours: `None` is the answer on a server
/// of X11 of our own, and means nothing was done. What the window stood as comes back, to be
/// given to [`step_back`].
///
/// 🔴 Before the question is put and not after, because a window that leaves the top after
/// the window of the system has opened is put on top of the layer it goes to.
pub(crate) fn step_aside(window: WindowRef) -> Result<Option<Stood>, StackingError> {
    let id = u32::try_from(window.0).map_err(|_| StackingError::HandleUnavailable)?;
    let lost = |Lost| StackingError::NotAside;
    let Windows { server, display } = connect_windows().map_err(|_| StackingError::NotAside)?;
    if display == Display::X11 {
        return Ok(None);
    }
    let atoms = stack_atoms(&server.conn).map_err(lost)?;
    let stood = Stood {
        types: atoms_of(&server.conn, id, b"_NET_WM_WINDOW_TYPE").map_err(lost)?,
        above: atoms_of(&server.conn, id, b"_NET_WM_STATE")
            .map_err(lost)?
            .contains(&atoms.above),
    };
    if put_aside(&server, id, &atoms).map_err(lost)? {
        return Ok(Some(stood));
    }
    // A manager that did not follow leaves the window as it was found, not half changed.
    let _ = put_back(&server, id, &atoms, &stood);
    Err(StackingError::NotAside)
}

/// Puts the window back on top, as it stood before [`step_aside`]. Nothing is done for a
/// window that was never put aside.
pub(crate) fn step_back(window: WindowRef, stood: Option<&Stood>) -> Result<(), StackingError> {
    let Some(stood) = stood else {
        return Ok(());
    };
    let id = u32::try_from(window.0).map_err(|_| StackingError::HandleUnavailable)?;
    let lost = |Lost| StackingError::NotBack;
    let Windows { server, .. } = connect_windows().map_err(|_| StackingError::NotBack)?;
    let atoms = stack_atoms(&server.conn).map_err(lost)?;
    if put_back(&server, id, &atoms, stood).map_err(lost)? {
        Ok(())
    } else {
        Err(StackingError::NotBack)
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
    let Windows { server, .. } = connect_windows().map_err(refused)?;
    server
        .conn
        .get_window_attributes(id)
        .ok()
        .and_then(|cookie| cookie.reply().ok())
        .map(|_| ())
        .ok_or(KeyMenuError::Refused)
}
