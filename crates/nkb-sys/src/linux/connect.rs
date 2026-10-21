//! The connection to the X server, to a LOCAL one and nothing else (`D145` point 4).
//!
//! The library's own `connect` follows `DISPLAY` wherever it points, a host over
//! TCP included. This program promises to send nothing off the machine, and a
//! keystroke is data like any other, so the address is read here, by the pure
//! `parse_display`, and only the socket of this machine is opened.

use std::os::unix::net::UnixStream;

use x11rb::connection::Connection;
use x11rb::reexports::x11rb_protocol::xauth::get_auth;
use x11rb::rust_connection::{DefaultStream, RustConnection};

use crate::linux_rules::{DisplayAddress, Environment, Refusal, route, socket_path};

/// A connection, the root window of its screen, and the facts about the server
/// that decide whether it can be typed into.
pub(crate) struct Server {
    pub conn: RustConnection,
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

/// Connects to the local socket of `address`. The server is refused when it is
/// XWayland (`OBS-203`) or has no `XTEST`, the one way to inject key events.
pub(crate) fn connect(address: DisplayAddress) -> Result<Server, Refusal> {
    let stream = UnixStream::connect(socket_path(address)).map_err(|_| Refusal::NoLocalServer)?;
    let (stream, (family, peer)) =
        DefaultStream::from_unix_stream(stream).map_err(|_| Refusal::NoLocalServer)?;
    // No authority file, or none that names this display, is a server that lets
    // local clients in: try without, as every X client does.
    let (name, data) = u16::try_from(address.number)
        .ok()
        .and_then(|number| get_auth(family, &peer, number).ok().flatten())
        .unwrap_or_default();
    let conn = RustConnection::connect_to_stream_with_auth_info(stream, address.screen, name, data)
        .map_err(|_| Refusal::NoLocalServer)?;
    let root = conn
        .setup()
        .roots
        .get(address.screen)
        .map(|screen| screen.root)
        .ok_or(Refusal::NoLocalServer)?;
    // The server of a Wayland session. The environment need not say so: a shell
    // that was started from an SSH login has no session type, and `DISPLAY=:0`
    // still names XWayland.
    if has_extension(&conn, "XWAYLAND") {
        return Err(Refusal::Wayland);
    }
    if !has_extension(&conn, "XTEST") {
        return Err(Refusal::NoXtest);
    }
    Ok(Server { conn, root })
}

/// Whether the server has the extension. A server that cannot say has not.
fn has_extension(conn: &RustConnection, name: &'static str) -> bool {
    use x11rb::connection::RequestConnection;
    conn.extension_information(name)
        .is_ok_and(|information| information.is_some())
}
