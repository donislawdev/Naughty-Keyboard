//! The kind of control that holds the keyboard focus, asked of the accessibility bus of the
//! session (`D145`, L4). `linux_atspi_rules.rs` has what is measured and what is decided,
//! and this file makes the calls.
//!
//! # The order of a look
//!
//! 1. The session bus (a file socket of this machine, as for the portal) is asked where the
//!    bus of accessibility is (`org.a11y.Bus.GetAddress`) and whether it is switched on
//!    (`org.a11y.Status.IsEnabled`).
//! 2. The address is read by the rule of the other buses of this program (`unix:path=`
//!    only), and the stream is opened HERE and handed to the library, which never follows
//!    an address.
//! 3. The applications are the children of the root of the registry, their windows the
//!    children of those, and the window in front the one whose states hold `ACTIVE`. The
//!    element with the focus is the first one in the windows in front whose states hold
//!    `FOCUSED`.
//!
//! # What this asks, and what it does not
//!
//! Three questions of an element: its children, its states, and, of the one element that
//! holds the focus, its role. Never its name, its description, its text or its attributes.
//! `tests/atspi_reads_only_kinds.rs` holds the list of every word this program says to the
//! bus, and goes red when one is added. The look is made at a press and never in between,
//! and it does not listen to what the person focuses.
//!
//! # A hung application
//!
//! Every question has its own limit, the whole look has another (`LOOK_WAIT`), and a watch
//! over both sockets shuts them when twice that has passed, so nothing in a look can hold a
//! press for good. An answer that did not come is `Unknown`.

use std::sync::Mutex;
use std::time::Instant;

use zbus::blocking::Connection;
use zbus::blocking::connection::Builder;
use zbus::zvariant::{ObjectPath, OwnedObjectPath, Value};
use zbus::{Error, Message};

use super::bus::{self, Handle};
use crate::field::FocusedInput;
use crate::linux_atspi_rules::{
    ACCESSIBLE_INTERFACE, CALL_WAIT, LAUNCHER_NAME, LAUNCHER_PATH, LOOK_WAIT, REGISTRY_NAME,
    REGISTRY_ROOT, Recent, STATUS_INTERFACE, Tree, find, settle,
};
use crate::linux_ibus_rules::socket_in_address;
use crate::linux_portal_rules::session_bus_socket;

/// The last look, for the questions a press asks three times in a few milliseconds.
static RECENT: Mutex<Recent> = Mutex::new(Recent::new());

/// What holds the keyboard focus, from the bus of accessibility. `Unknown` for anything
/// that cannot be asked or answered.
pub(crate) fn focused_input() -> FocusedInput {
    if let Some(kept) = recent().kept(Instant::now()) {
        return kept;
    }
    let kind = look().unwrap_or(FocusedInput::Unknown);
    recent().keep(Instant::now(), kind);
    kind
}

fn recent() -> std::sync::MutexGuard<'static, Recent> {
    // A look that panicked left nothing behind worth refusing: the answer is asked again.
    RECENT
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The bus could not be asked. The reason is not told apart: the answer is `Unknown`.
struct Failed;

impl From<Error> for Failed {
    fn from(_: Error) -> Self {
        Self
    }
}

impl From<std::io::Error> for Failed {
    fn from(_: std::io::Error) -> Self {
        Self
    }
}

impl From<zbus::zvariant::Error> for Failed {
    fn from(_: zbus::zvariant::Error) -> Self {
        Self
    }
}

/// A connection to a file socket of this machine, with a watch over it.
struct Opened {
    connection: Connection,
    _handle: Handle,
    _watch: bus::Watch,
}

fn open(socket: &str) -> Result<Opened, Failed> {
    let (stream, handle) = bus::open(socket)?;
    let watch = handle.watch(LOOK_WAIT * 2)?;
    let connection = Builder::async_io_unix_stream(stream)
        .method_timeout(CALL_WAIT)
        .build()?;
    Ok(Opened {
        connection,
        _handle: handle,
        _watch: watch,
    })
}

/// The address of the bus of accessibility and whether it is switched on, from the
/// session bus.
fn where_is_the_bus() -> Result<(String, bool), Failed> {
    let address = std::env::var("DBUS_SESSION_BUS_ADDRESS").ok();
    let runtime = std::env::var("XDG_RUNTIME_DIR").ok();
    let socket = session_bus_socket(address.as_deref(), runtime.as_deref()).map_err(|_| Failed)?;
    let session = open(&socket)?;
    let said = session.connection.call_method(
        Some(LAUNCHER_NAME),
        LAUNCHER_PATH,
        Some(LAUNCHER_NAME),
        "GetAddress",
        &(),
    )?;
    let address: String = said.body().deserialize()?;
    let switch = session.connection.call_method(
        Some(LAUNCHER_NAME),
        LAUNCHER_PATH,
        Some("org.freedesktop.DBus.Properties"),
        "Get",
        &(STATUS_INTERFACE, "IsEnabled"),
    )?;
    let body = switch.body();
    let value: Value<'_> = body.deserialize()?;
    Ok((address, is_true(&value)))
}

/// A boolean, through the variants a property is wrapped in.
fn is_true(value: &Value<'_>) -> bool {
    match value {
        Value::Bool(on) => *on,
        Value::Value(inner) => is_true(inner),
        _ => false,
    }
}

fn look() -> Result<FocusedInput, Failed> {
    let (address, switched_on) = where_is_the_bus()?;
    let socket = socket_in_address(&address).map_err(|_| Failed)?;
    let opened = open(&socket)?;
    let mut tree = Bus {
        connection: &opened.connection,
        until: Instant::now() + LOOK_WAIT,
    };
    Ok(settle(find(&mut tree), switched_on))
}

/// An element of the tree: the bus name of its application and its path.
type Element = (String, OwnedObjectPath);

/// The tree of the bus of accessibility, asked through one connection.
struct Bus<'c> {
    connection: &'c Connection,
    until: Instant,
}

impl Bus<'_> {
    /// Calls `member` of the accessible interface on `element`. `None` when the
    /// application does not answer, which includes one that has gone.
    fn ask(&self, element: &Element, member: &str) -> Option<Message> {
        self.connection
            .call_method(
                Some(element.0.as_str()),
                element.1.as_str(),
                Some(ACCESSIBLE_INTERFACE),
                member,
                &(),
            )
            .ok()
    }
}

impl Tree for Bus<'_> {
    type Node = Element;

    fn applications(&mut self) -> Option<Vec<Element>> {
        let root = ObjectPath::try_from(REGISTRY_ROOT).ok()?;
        self.children(&(REGISTRY_NAME.to_owned(), OwnedObjectPath::from(root)))
    }

    fn children(&mut self, node: &Element) -> Option<Vec<Element>> {
        self.ask(node, "GetChildren")?.body().deserialize().ok()
    }

    fn states(&mut self, node: &Element) -> Option<Vec<u32>> {
        self.ask(node, "GetState")?.body().deserialize().ok()
    }

    fn role(&mut self, node: &Element) -> Option<u32> {
        self.ask(node, "GetRole")?.body().deserialize().ok()
    }

    fn time_left(&self) -> bool {
        Instant::now() < self.until
    }
}
