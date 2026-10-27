//! The connection to the X server, to a LOCAL one and nothing else (`D145` point 4).
//!
//! The library's own `connect` follows `DISPLAY` wherever it points, a host over
//! TCP included. This program promises to send nothing off the machine, and a
//! keystroke is data like any other, so the address is read here, by the pure
//! `parse_display`, and only the socket of this machine is opened.
//!
//! Every connection waits for the server for at most `SERVER_WAIT` (`stream`), the
//! one of the shortcuts' listener excepted, whose work is to wait.

use std::os::unix::net::UnixStream;

use x11rb::connection::Connection;
use x11rb::reexports::x11rb_protocol::xauth::get_auth;
use x11rb::rust_connection::DefaultStream;

use super::stream::{Conn, LocalStream, Wait};
use crate::linux_rules::{
    DisplayAddress, Environment, Refusal, SERVER_WAIT, route, session_is_wayland, socket_path,
    window_display,
};

/// A connection, the root window of its screen, and the facts about the server
/// that decide whether it can be typed into.
pub(crate) struct Server {
    pub conn: Conn,
    pub root: u32,
}

/// Where the environment says to type, or why not. No connection is made.
pub(crate) fn environment_route() -> Result<DisplayAddress, Refusal> {
    let display = std::env::var("DISPLAY").ok();
    let session_type = std::env::var("XDG_SESSION_TYPE").ok();
    let wayland_display = std::env::var("WAYLAND_DISPLAY").ok();
    route(&Environment {
        display: display.as_deref(),
        session_type: session_type.as_deref(),
        wayland_display: wayland_display.as_deref(),
    })
}

/// Whether the environment says this is a Wayland session. Typing there does not go
/// through the X server (`linux/ibus`).
pub(crate) fn wayland_session() -> bool {
    let session_type = std::env::var("XDG_SESSION_TYPE").ok();
    let wayland_display = std::env::var("WAYLAND_DISPLAY").ok();
    session_is_wayland(session_type.as_deref(), wayland_display.as_deref())
}

/// Why this program does not type here - in the words that follow `on` - or `None`
/// when it might. The environment is asked first, and then the server itself: a
/// shell started over SSH says nothing of Wayland while `DISPLAY=:0` still names
/// XWayland, and a server without `XTEST` cannot be typed through. One connection,
/// made and dropped here.
pub(crate) fn route_refusal() -> Option<&'static str> {
    match environment_route() {
        Err(refusal) => Some(refusal.system()),
        Ok(address) => connect(address).err().map(Refusal::system),
    }
}

/// Whether the shortcuts are out of reach because this is a Wayland session, which keeps the
/// keys for the desktop to give (`portal`). The environment is asked first and then the
/// server, as for [`route_refusal`], so a shell started over SSH is told by its XWayland.
pub(crate) fn grab_refused_by_wayland() -> bool {
    match environment_route() {
        Err(refusal) => refusal == Refusal::Wayland,
        Ok(address) => connect(address).err() == Some(Refusal::Wayland),
    }
}

/// Connects to the local socket of `address`. The server is refused when it is
/// XWayland (`OBS-203`) or has no `XTEST`, the one way to inject key events.
pub(crate) fn connect(address: DisplayAddress) -> Result<Server, Refusal> {
    checked(open(address, Wait::Bounded(SERVER_WAIT))?)
}

/// Connects as [`connect`] does, for a thread that waits for the server's events for as
/// long as the program runs: the listener of the shortcuts.
pub(crate) fn connect_listening(address: DisplayAddress) -> Result<Server, Refusal> {
    checked(open(address, Wait::Forever)?)
}

/// `server`, or why it cannot be typed through.
fn checked(server: Server) -> Result<Server, Refusal> {
    // The server of a Wayland session. The environment need not say so: a shell
    // that was started from an SSH login has no session type, and `DISPLAY=:0`
    // still names XWayland.
    if has_extension(&server.conn, "XWAYLAND") {
        return Err(Refusal::Wayland);
    }
    if !has_extension(&server.conn, "XTEST") {
        return Err(Refusal::NoXtest);
    }
    Ok(server)
}

/// What the windows of this program stand on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Display {
    /// A server of its own, with a window manager that obeys the hints.
    X11,
    /// The X server inside a Wayland compositor, which ignores the hint that keeps a
    /// window from taking the keyboard (`OBS-218`).
    XWayland,
}

/// The connection of the windows and what it stands on.
pub(crate) struct Windows {
    pub server: Server,
    pub display: Display,
}

/// Connects for the windows of this program. Unlike typing, they are at home on
/// XWayland: the palette is a client of it on a Wayland session, so that a click on
/// it can be kept from taking the keyboard (`D145` point 38). A Wayland session
/// without a local X server says Wayland, which is what it is.
pub(crate) fn connect_windows() -> Result<Windows, Refusal> {
    let display = std::env::var("DISPLAY").ok();
    let server = window_display(display.as_deref())
        .and_then(|address| open(address, Wait::Bounded(SERVER_WAIT)))
        .map_err(|refusal| {
            if wayland_session() {
                Refusal::Wayland
            } else {
                refusal
            }
        })?;
    if has_extension(&server.conn, "XWAYLAND") {
        return Ok(Windows {
            server,
            display: Display::XWayland,
        });
    }
    if !has_extension(&server.conn, "XTEST") {
        return Err(Refusal::NoXtest);
    }
    Ok(Windows {
        server,
        display: Display::X11,
    })
}

/// Why the windows cannot stand here, in the words that follow `on`, or `None`.
pub(crate) fn window_refusal() -> Option<&'static str> {
    connect_windows().err().map(Refusal::system)
}

/// Whether the windows stand on XWayland: the palette then draws its own frame
/// (`D145` point 38).
pub(crate) fn on_xwayland() -> bool {
    connect_windows().is_ok_and(|windows| windows.display == Display::XWayland)
}

/// Opens the local socket of `address` and says nothing of what answers. A server that
/// accepts the socket and never sends its setup is waited for as `wait` says.
fn open(address: DisplayAddress, wait: Wait) -> Result<Server, Refusal> {
    let stream = UnixStream::connect(socket_path(address)).map_err(|_| Refusal::NoLocalServer)?;
    let (stream, (family, peer)) =
        DefaultStream::from_unix_stream(stream).map_err(|_| Refusal::NoLocalServer)?;
    let stream = LocalStream::new(stream, wait);
    // No authority file, or none that names this display, is a server that lets
    // local clients in: try without, as every X client does.
    let (name, data) = u16::try_from(address.number)
        .ok()
        .and_then(|number| get_auth(family, &peer, number).ok().flatten())
        .unwrap_or_default();
    let conn = Conn::connect_to_stream_with_auth_info(stream, address.screen, name, data)
        .map_err(|_| Refusal::NoLocalServer)?;
    let root = conn
        .setup()
        .roots
        .get(address.screen)
        .map(|screen| screen.root)
        .ok_or(Refusal::NoLocalServer)?;
    Ok(Server { conn, root })
}

/// Two connected sockets with no address, made inside this process: the side of the tests of
/// `stream` that stands in for a server, silent or answering. Made here, where the product
/// opens its one socket to the X server, so the guard of the network sees it in that file.
#[cfg(test)]
pub(super) fn socket_pair() -> std::io::Result<(DefaultStream, UnixStream)> {
    let (ours, theirs) = UnixStream::pair()?;
    let (ours, _peer) = DefaultStream::from_unix_stream(ours)?;
    Ok((ours, theirs))
}

/// Whether the server has the extension. A server that cannot say has not.
fn has_extension(conn: &Conn, name: &'static str) -> bool {
    use x11rb::connection::RequestConnection;
    conn.extension_information(name)
        .is_ok_and(|information| information.is_some())
}
