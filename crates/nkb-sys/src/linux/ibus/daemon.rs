//! The IBus daemon: where its socket is, the connection to it, and the watch that keeps a
//! daemon which stopped answering from holding a send for ever (`D145` points 37 and 41).
//!
//! The socket is a FILE socket on this machine and nothing else. The daemon writes its
//! address to a file of the person's configuration directory (`IBUS_ADDRESS` in the
//! environment wins when set), the address is read by `linux_ibus_rules`, which accepts
//! only `unix:path=...`, and the stream is opened HERE and handed to the bus library. The
//! library therefore never reads an address and never follows one to a host, which is
//! what an `IBUS_ADDRESS` set to `tcp:` would otherwise make it do.

use core::time::Duration;
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread::JoinHandle;

use zbus::blocking::Connection;
use zbus::blocking::connection::Builder;

use super::Failed;
use super::engine::Factory;
use crate::linux_ibus_rules::{
    AddressRefusal, CALL_WAIT, Where, address_file, socket_in_address, socket_in_file,
};

/// The socket of the daemon for this session, or why there is none to be found.
pub(super) fn locate() -> Result<String, AddressRefusal> {
    if let Some(address) = std::env::var("IBUS_ADDRESS")
        .ok()
        .filter(|value| !value.trim().is_empty())
    {
        return socket_in_address(&address);
    }
    let machine = std::fs::read_to_string("/var/lib/dbus/machine-id")
        .or_else(|_| std::fs::read_to_string("/etc/machine-id"))
        .ok();
    let config_home = std::env::var("XDG_CONFIG_HOME").ok();
    let home = std::env::var("HOME").ok();
    let wayland_display = std::env::var("WAYLAND_DISPLAY").ok();
    let file = address_file(&Where {
        config_home: config_home.as_deref(),
        home: home.as_deref(),
        machine_id: machine.as_deref(),
        wayland_display: wayland_display.as_deref(),
    })
    .ok_or(AddressRefusal::Missing)?;
    let content = std::fs::read_to_string(file).map_err(|_| AddressRefusal::Missing)?;
    socket_in_file(&content)
}

/// Whether something accepts connections at `path`. A connection made and dropped: the
/// daemon sees a client that never speaks, which it takes to be one that went away.
pub(super) fn reachable(path: &str) -> bool {
    UnixStream::connect(path).is_ok()
}

/// A connection to the daemon that serves our factory, and the watch over it.
pub(super) struct Opened {
    pub connection: Connection,
    _watch: Watch,
}

/// Connects to the file socket `path`, makes the connection a client of the daemon's bus
/// that also serves `factory` at its path when there is one, and starts the watch.
pub(super) fn open(
    path: &str,
    factory: Option<Factory>,
    deadline: Duration,
) -> Result<Opened, Failed> {
    let stream = UnixStream::connect(path)?;
    let watch = Watch::start(&stream, deadline)?;
    let builder = Builder::async_io_unix_stream(stream).method_timeout(CALL_WAIT);
    let builder = match factory {
        Some(factory) => builder.serve_at(super::engine::FACTORY_PATH, factory)?,
        None => builder,
    };
    Ok(Opened {
        connection: builder.build()?,
        _watch: watch,
    })
}

/// Shuts the connection when `deadline` has passed, so that every call waiting on it
/// returns an error, and does nothing when it is dropped first.
struct Watch {
    done: Option<Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl Watch {
    fn start(stream: &UnixStream, deadline: Duration) -> Result<Self, Failed> {
        let shut = stream.try_clone()?;
        let (done, wait) = mpsc::channel::<()>();
        let thread = std::thread::Builder::new()
            .name("nkb-ibus-watch".to_owned())
            .spawn(move || {
                if wait.recv_timeout(deadline) == Err(RecvTimeoutError::Timeout) {
                    let _ = shut.shutdown(Shutdown::Both);
                }
            })?;
        Ok(Self {
            done: Some(done),
            thread: Some(thread),
        })
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        // The thread wakes when the sender goes, and ends without having shut anything.
        drop(self.done.take());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
