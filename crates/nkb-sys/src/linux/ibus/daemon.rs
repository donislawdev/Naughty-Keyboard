//! The IBus daemon: where its socket is and the connection to it (`D145` points 37 and 41).
//!
//! The socket is a FILE socket on this machine and nothing else. The daemon writes its
//! address to a file of the person's configuration directory (`IBUS_ADDRESS` in the
//! environment wins when set), the address is read by `linux_ibus_rules`, which accepts
//! only `unix:path=...`, and the stream is opened by `bus` and handed to the bus library.
//! The library therefore never reads an address and never follows one to a host, which is
//! what an `IBUS_ADDRESS` set to `tcp:` would otherwise make it do.

use core::time::Duration;

use zbus::blocking::Connection;
use zbus::blocking::connection::Builder;

use super::Failed;
use super::engine::Factory;
use crate::linux::bus::{self, Watch};
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

/// Whether something accepts connections at `path`.
pub(super) fn reachable(path: &str) -> bool {
    bus::reachable(path)
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
    let stream = bus::connect(path)?;
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
