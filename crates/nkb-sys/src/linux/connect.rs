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

/// Whether the server has the extension. A server that cannot say has not.
fn has_extension(conn: &Conn, name: &'static str) -> bool {
    use x11rb::connection::RequestConnection;
    conn.extension_information(name)
        .is_ok_and(|information| information.is_some())
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    //! The bounded wait of `stream`, tested here because this is the file where the
    //! product opens its socket, and so the one the guard of the network lets a test
    //! open a socket in.

    use core::time::Duration;
    use std::io::{self, Read as _, Write as _};
    use std::time::Instant;

    use x11rb::rust_connection::{PollMode, RustConnection, Stream as _};

    use super::*;

    const SHORT: Duration = Duration::from_millis(50);

    /// Two connected sockets with no address, made inside this process: ours, as the
    /// library holds it, and the side that stands in for a server, silent or answering.
    fn socket_pair() -> io::Result<(DefaultStream, UnixStream)> {
        let (ours, theirs) = UnixStream::pair()?;
        let (ours, _peer) = DefaultStream::from_unix_stream(ours)?;
        Ok((ours, theirs))
    }

    #[test]
    fn a_silent_server_is_given_up_after_the_limit_and_then_at_once() {
        let (inner, _server) = socket_pair().expect("a socket pair");
        let stream = LocalStream::new(inner, Wait::Bounded(SHORT));
        let started = Instant::now();
        let first = stream
            .poll(PollMode::Readable)
            .expect_err("nothing was written");
        let took = started.elapsed();
        assert_eq!(first.kind(), io::ErrorKind::TimedOut);
        assert!(took >= SHORT, "waited the limit: {took:?}");
        assert!(took < SHORT * 20, "and not much longer: {took:?}");

        let again = Instant::now();
        let second = stream.poll(PollMode::Readable).expect_err("given up");
        assert_eq!(second.kind(), io::ErrorKind::TimedOut);
        assert!(
            again.elapsed() < SHORT,
            "not waited twice: {:?}",
            again.elapsed()
        );
    }

    #[test]
    fn an_answer_ends_the_wait() {
        let (inner, mut server) = socket_pair().expect("a socket pair");
        let stream = LocalStream::new(inner, Wait::Bounded(Duration::from_secs(10)));
        server.write_all(b"x").expect("the answer");
        stream
            .poll(PollMode::Readable)
            .expect("a readable socket is not a silent server");
        let mut fds = Vec::new();
        let mut buf = [0u8; 4];
        assert_eq!(stream.read(&mut buf, &mut fds).expect("the byte"), 1);
    }

    #[test]
    fn writes_still_go_once_the_server_is_given_up() {
        // Giving up refuses waiting, not writing: the requests that give the spare
        // keys back are still written after the wait that ran out.
        let (inner, mut server) = socket_pair().expect("a socket pair");
        let stream = LocalStream::new(inner, Wait::Bounded(SHORT));
        let _ = stream.poll(PollMode::Readable);
        let mut fds = Vec::new();
        assert_eq!(stream.write(b"give back", &mut fds).expect("written"), 9);
        let mut buf = [0u8; 9];
        server.read_exact(&mut buf).expect("what was written");
        assert_eq!(&buf, b"give back");
    }

    #[test]
    fn a_connection_to_a_silent_server_ends_within_the_limit() {
        // The handshake waits for the server's setup like any answer: a socket that
        // accepts and never speaks is a refusal, not a hang.
        let (inner, _server) = socket_pair().expect("a socket pair");
        let stream = LocalStream::new(inner, Wait::Bounded(SHORT));
        let started = Instant::now();
        let refused = RustConnection::connect_to_stream(stream, 0);
        assert!(refused.is_err(), "no setup was ever sent");
        assert!(
            started.elapsed() < SHORT * 20,
            "ended within the limit: {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn the_listener_of_the_shortcuts_waits_as_long_as_it_takes() {
        // The thread of the shortcuts waits for the person, and an answer that comes
        // after a bounded wait would have run out still ends its wait.
        let (inner, mut server) = socket_pair().expect("a socket pair");
        let stream = LocalStream::new(inner, Wait::Forever);
        let writer = std::thread::spawn(move || {
            std::thread::sleep(SHORT * 2);
            server.write_all(b"x").expect("the answer");
            server
        });
        stream
            .poll(PollMode::Readable)
            .expect("waited until the answer");
        let _server = writer.join().expect("the writer");
    }
}
