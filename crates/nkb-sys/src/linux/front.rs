//! Which window holds the keyboard focus, and which of its windows takes part in
//! `_NET_WM_PING` (`OBS-202` points 3 and 10).
//!
//! The window in front is the top-level window the focus is in: the focus may rest
//! on a child, and moving it between two children of one application must not
//! read as another window coming to the front. A top-level window is the one whose
//! parent is the root. With a reparenting window manager that is the manager's
//! frame, which lives exactly as long as the window inside it.

use x11rb::protocol::xproto::{AtomEnum, ConnectionExt as _};

use super::Lost;
use super::connect::{connect, environment_route};
use super::stream::Conn;
use crate::WindowRef;
use crate::linux_rules::focus_window;

/// The window the keyboard focus is in, if it is in one.
pub(super) fn focus(conn: &Conn, root: u32) -> Result<Option<u32>, Lost> {
    let raw = conn.get_input_focus()?.reply()?.focus;
    Ok(focus_window(raw, root))
}

/// The top-level window `window` belongs to, found by walking up.
pub(super) fn toplevel(conn: &Conn, root: u32, window: u32) -> Result<u32, Lost> {
    let mut current = window;
    // No window tree is this deep. The bound is for a server that answers in a
    // loop, which would otherwise be waited for for ever.
    for _ in 0..64 {
        let parent = conn.query_tree(current)?.reply()?.parent;
        if parent == root || parent == 0 {
            return Ok(current);
        }
        current = parent;
    }
    Err(Lost)
}

/// The top-level window the keyboard focus is in.
pub(super) fn focused_toplevel(conn: &Conn, root: u32) -> Result<Option<u32>, Lost> {
    match focus(conn, root)? {
        Some(window) => toplevel(conn, root, window).map(Some),
        None => Ok(None),
    }
}

/// The window in front right now, if there is one. A connection of its own, made
/// and dropped here: the callers ask a few times per send, a few milliseconds
/// apart, and a connection held between two sends would be one more thing to
/// keep alive across the life of a palette.
pub(crate) fn foreground_window() -> Option<WindowRef> {
    let address = environment_route().ok()?;
    let server = connect(address).ok()?;
    let window = focused_toplevel(&server.conn, server.root).ok()??;
    Some(WindowRef(u64::from(window)))
}

/// The process behind `window`, from `_NET_WM_PID`. The property sits on the
/// application's own window, and with a reparenting window manager the window in
/// front is the frame that holds it, so the windows below are asked too - a few
/// levels, never the whole tree. Nothing else of the window is read: not its
/// title, not its class (`program_reads_only_its_name.rs`).
pub(crate) fn window_pid(window: WindowRef) -> Option<u32> {
    let address = environment_route().ok()?;
    let server = connect(address).ok()?;
    let atom = server
        .conn
        .intern_atom(false, b"_NET_WM_PID")
        .ok()?
        .reply()
        .ok()?
        .atom;
    pid_below(&server.conn, u32::try_from(window.0).ok()?, atom, 3)
}

fn pid_below(conn: &Conn, window: u32, atom: u32, depth: u8) -> Option<u32> {
    let property = conn
        .get_property(false, window, atom, AtomEnum::CARDINAL, 0, 1)
        .ok()?
        .reply()
        .ok()?;
    if let Some(pid) = property.value32().and_then(|mut values| values.next()) {
        return Some(pid);
    }
    if depth == 0 {
        return None;
    }
    let children = conn.query_tree(window).ok()?.reply().ok()?.children;
    children
        .into_iter()
        .find_map(|child| pid_below(conn, child, atom, depth - 1))
}

/// The atoms the ping is made of.
#[derive(Clone, Copy)]
pub(super) struct PingAtoms {
    pub protocols: u32,
    pub ping: u32,
}

pub(super) fn ping_atoms(conn: &Conn) -> Result<PingAtoms, Lost> {
    let protocols = conn.intern_atom(false, b"WM_PROTOCOLS")?;
    let ping = conn.intern_atom(false, b"_NET_WM_PING")?;
    Ok(PingAtoms {
        protocols: protocols.reply()?.atom,
        ping: ping.reply()?.atom,
    })
}

/// The window from `focus` upwards that lists `_NET_WM_PING` among its protocols,
/// if any does. The focus window can be a child of the top-level window that
/// speaks the protocol, so this walks up to the first that does - and stops below
/// the root, which is the window manager's.
pub(super) fn ping_target(
    conn: &Conn,
    root: u32,
    focus: u32,
    atoms: PingAtoms,
) -> Result<Option<u32>, Lost> {
    let mut current = focus;
    for _ in 0..64 {
        let protocols = conn
            .get_property(false, current, atoms.protocols, AtomEnum::ATOM, 0, 64)?
            .reply()?;
        if protocols
            .value32()
            .is_some_and(|mut listed| listed.any(|atom| atom == atoms.ping))
        {
            return Ok(Some(current));
        }
        let parent = conn.query_tree(current)?.reply()?.parent;
        if parent == root || parent == 0 {
            return Ok(None);
        }
        current = parent;
    }
    Ok(None)
}
