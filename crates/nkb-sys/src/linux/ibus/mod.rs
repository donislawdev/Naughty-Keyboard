//! Typing on Wayland: the text goes to the desktop's input method (`D145` points 37 and 41,
//! `OBS-217` to `OBS-219`).
//!
//! Through XTest, XWayland and the remote-desktop portal only the characters of the
//! compositor's layout arrive, and the rest is lost or turned into another character
//! without a word (`Ó` came out as `o`, `£` as `3`), at any pace. The desktop's own way
//! of writing a character the keyboard has no key for is the input method, and GNOME
//! runs IBus. So a send is:
//!
//! 1. the daemon's file socket is found (`daemon::locate`) and connected to, and an
//!    engine of ours is registered with it (`RegisterComponent`),
//! 2. the engine the person was using is read, and ours is made the global one
//!    (`SetGlobalEngine`). The daemon asks the factory for it and tells it when an input
//!    context has the focus,
//! 3. when it does, and after a short pause, the text is committed from the engine, in
//!    pieces that reach the application in order,
//! 4. one question is put to the daemon, which answers only after it has dealt with the
//!    commits before it, the connection of the engine is closed, and the engine the person
//!    was using is asked for again from a second connection. The daemon drops the global
//!    engine together with ours, whichever engine was made the global one in between
//!    (`put_back`), so asking for it before the first connection closes does not last.
//!
//! The engine exists only while this process does, so a send that is killed leaves the
//! desktop without an engine, as it was when the person had none, and a person who had
//! one switches input source once. Measured exact in GTK4, Slint, Qt and xterm (23 of 23).
//!
//! What this route cannot say, and the person is told: whether anything with a field had
//! the focus. The daemon announces a focused input context also when the focus is on the
//! desktop, so the text is HANDED OVER to the input method, and what the application did
//! with it is for the person to look at (`Following::HandedOver`). `Escape`, a held
//! modifier and Caps Lock do not apply: a commit is not a key.

mod daemon;
mod engine;

use core::time::Duration;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::Instant;

use zbus::blocking::Connection;
use zbus::zvariant::Value;

use self::daemon::Opened;
use self::engine::{
    BUS, BUS_PATH, ENGINE_INTERFACE, Event, Factory, component, ibus_text, name_of_engine,
};
use crate::cadence::{Cadence, PROGRESS_INTERVAL};
use crate::linux_ibus_rules::{
    COMMIT_GAP, DEADLINE, ESCAPE_LOOK, FOCUS_SETTLE, FOCUS_WAIT, FORGET_LOOK, FORGET_WAIT,
    SETTLE_AFTER_COMMIT, deadline_for, engine_name, engine_to_restore, is_our_engine, pieces,
};
use crate::linux_rules::Refusal;
use crate::{
    Following, SendError, SendOutcome, SendProgress, StopReason, UNNAMED_WINDOW, WindowRef,
};

/// A call to the daemon failed, or the daemon is not as this route expects. Nothing more
/// is known than that, and nothing more is needed: the caller reports what it had already
/// handed over.
#[derive(Debug, Clone, Copy)]
pub(super) struct Failed;

impl From<std::io::Error> for Failed {
    fn from(_: std::io::Error) -> Self {
        Self
    }
}

impl From<zbus::Error> for Failed {
    fn from(_: zbus::Error) -> Self {
        Self
    }
}

impl From<zbus::zvariant::Error> for Failed {
    fn from(_: zbus::zvariant::Error) -> Self {
        Self
    }
}

impl From<zbus::fdo::Error> for Failed {
    fn from(_: zbus::fdo::Error) -> Self {
        Self
    }
}

/// Why this program does not type on this Wayland session, in the words that follow `on`,
/// or `None` when there is a daemon to hand the text to. One look at the address file and
/// one connection, made and dropped here.
pub(crate) fn refusal() -> Option<&'static str> {
    let reachable = daemon::locate()
        .ok()
        .is_some_and(|socket| daemon::reachable(&socket));
    (!reachable).then(|| Refusal::NoInputMethod.system())
}

/// The window in front, as far as this session says: it cannot be named, and there is
/// one to type into as long as there is a daemon.
pub(crate) fn front() -> Option<WindowRef> {
    refusal().is_none().then_some(UNNAMED_WINDOW)
}

fn unsupported() -> SendError {
    SendError::Unsupported {
        system: Refusal::NoInputMethod.system(),
    }
}

fn truncated(units_sent: usize, units_expected: usize, reason: StopReason) -> SendError {
    SendError::Truncated {
        units_sent,
        units_expected,
        reason,
    }
}

/// One send's engine, registered and made the global one, and the way back.
struct Session {
    opened: Opened,
    events: Receiver<Event>,
    /// The engine of the person, put back when the send is over.
    previous: Option<String>,
    /// Where the engine was made, once the daemon asked for it.
    path: Option<String>,
    /// Set by the engine when the person presses `Escape`.
    escape: Arc<AtomicBool>,
}

impl Session {
    fn begin(socket: &str, deadline: Duration) -> Result<Self, Failed> {
        let name = engine_name(std::process::id());
        let (sender, events) = mpsc::channel();
        let escape = Arc::new(AtomicBool::new(false));
        let factory = Factory::new(name.clone(), sender, Arc::clone(&escape));
        let opened = daemon::open(socket, Some(factory), deadline)?;
        let connection = &opened.connection;
        connection.call_method(
            Some(BUS),
            BUS_PATH,
            Some(BUS),
            "RegisterComponent",
            &(component(&name)?,),
        )?;
        let previous = global_engine(connection);
        // Blocking is safe here: the factory is served by the executor thread of the
        // connection, not by the thread that waits.
        connection.call_method(
            Some(BUS),
            BUS_PATH,
            Some(BUS),
            "SetGlobalEngine",
            &(name.as_str(),),
        )?;
        Ok(Self {
            opened,
            events,
            previous,
            path: None,
            escape,
        })
    }

    /// Whether the person pressed `Escape` since the engine was made the global one.
    fn escape_pressed(&self) -> bool {
        self.escape.load(Ordering::SeqCst)
    }

    /// Waits for `time`, looking for `Escape` every `ESCAPE_LOOK`. `true` if it was pressed.
    fn pause(&self, time: Duration) -> bool {
        let end = Instant::now() + time;
        while let Some(left) = end.checked_duration_since(Instant::now()) {
            if self.escape_pressed() {
                return true;
            }
            std::thread::sleep(left.min(ESCAPE_LOOK));
        }
        self.escape_pressed()
    }

    /// Waits until the daemon says an input context has the focus. `Ok(false)` when it
    /// did not in `FOCUS_WAIT`, which means there is nobody to hand the text to.
    fn wait_for_focus(&mut self) -> Result<bool, Failed> {
        let deadline = Instant::now() + FOCUS_WAIT;
        let mut focused = false;
        while let Some(left) = deadline.checked_duration_since(Instant::now()) {
            match self.events.recv_timeout(left) {
                Ok(Event::Created(path)) => self.path = Some(path),
                Ok(Event::FocusIn) => focused = true,
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => return Err(Failed),
            }
            // The engine is made before the daemon tells it anything, so a commit has a
            // path to go from by the time it has a place to go to.
            if focused && self.path.is_some() {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// One piece of text, committed from the engine.
    fn commit(&self, piece: &str) -> Result<(), Failed> {
        let path = self.path.as_deref().ok_or(Failed)?;
        self.opened.connection.emit_signal(
            None::<&str>,
            path,
            ENGINE_INTERFACE,
            "CommitText",
            &(ibus_text(piece)?,),
        )?;
        Ok(())
    }

    /// Puts a question to the daemon, which it answers only after it has dealt with what
    /// was sent before it, and gives the application a moment to take what was committed.
    /// Best effort: the text was handed over before this, and that is all this route says.
    fn settle(&self) {
        let _ = global_engine(&self.opened.connection);
        std::thread::sleep(SETTLE_AFTER_COMMIT);
    }
}

impl Session {
    /// Closes the connection of the engine, which the daemon forgets, and gives back the
    /// engine the person was using.
    fn end(self) -> Option<String> {
        let Self {
            opened, previous, ..
        } = self;
        drop(opened);
        previous
    }
}

/// Asks for the engine of the person again, from a connection of its own.
///
/// The daemon drops the global engine when the engine of this send goes, whichever one was
/// made the global one in between: put back from the connection that is about to close, and
/// even by `ibus engine` from another process, the person's engine was gone once ours
/// closed (measured on GNOME 50). So the first connection is closed, the daemon is asked
/// until it no longer names an engine of ours, and only then is the person's engine asked
/// for, from a connection that stays open until the daemon has answered. Best effort: the
/// text was handed over before this, and a person without an engine of their own is never
/// here.
fn put_back(socket: &str, name: &str) {
    let Ok(opened) = daemon::open(socket, None, DEADLINE) else {
        return;
    };
    let connection = &opened.connection;
    let deadline = Instant::now() + FORGET_WAIT;
    while Instant::now() < deadline
        && global_engine(connection).is_some_and(|name| is_our_engine(&name))
    {
        std::thread::sleep(FORGET_LOOK);
    }
    let _ = connection.call_method(Some(BUS), BUS_PATH, Some(BUS), "SetGlobalEngine", &(name,));
}

/// The engine the daemon has set, if it has one.
fn global_engine(connection: &Connection) -> Option<String> {
    let reply = connection
        .call_method(
            Some(BUS),
            BUS_PATH,
            Some("org.freedesktop.DBus.Properties"),
            "Get",
            &(BUS, "GlobalEngine"),
        )
        .ok()?;
    let body = reply.body();
    let value: Value<'_> = body.deserialize().ok()?;
    name_of_engine(&value)
}

pub(crate) fn send_text(
    text: &str,
    progress: &mut dyn FnMut(SendProgress),
) -> Result<SendOutcome, SendError> {
    if text.is_empty() {
        // Nothing to send, so nothing to connect to: an empty value is a real catalogue
        // entry, and nothing needed following.
        return Ok(SendOutcome {
            units: 0,
            events: 0,
            following: Following::Application,
        });
    }
    // A D-Bus string cannot hold it, and refusing it by name is the alternative to
    // sending the text without it.
    if text.contains('\0') {
        return Err(SendError::CannotCarry { character: '\0' });
    }
    let expected = text.encode_utf16().count();
    let socket = daemon::locate().map_err(|_| unsupported())?;
    let mut session =
        Session::begin(&socket, deadline_for(pieces(text).len())).map_err(|_| unsupported())?;
    let result = hand_over(&mut session, text, expected, progress);
    // On every way out, a failed send included: the person's engine is theirs.
    let previous = session.end();
    if let Some(name) = engine_to_restore(previous.as_deref()) {
        put_back(&socket, name);
    }
    result
}

/// The text committed from the engine, in pieces, once an input context has the focus.
fn hand_over(
    session: &mut Session,
    text: &str,
    expected: usize,
    progress: &mut dyn FnMut(SendProgress),
) -> Result<SendOutcome, SendError> {
    if !session.wait_for_focus().map_err(|_| unsupported())? {
        // The engine is made and nobody was told they have the focus: nothing was sent.
        return Err(truncated(0, expected, StopReason::Dropped));
    }
    if session.pause(FOCUS_SETTLE) {
        return Err(truncated(0, expected, StopReason::Escape));
    }

    let mut cadence = Cadence::starting(Instant::now(), PROGRESS_INTERVAL);
    let mut handed = 0usize;
    let mut commits = 0usize;
    for (number, piece) in pieces(text).into_iter().enumerate() {
        // `Escape` stops a send before the next piece, as on every route: what was handed
        // over is counted, and nothing more goes (`D96`).
        if (number > 0 && session.pause(COMMIT_GAP)) || session.escape_pressed() {
            return Err(truncated(handed, expected, StopReason::Escape));
        }
        session
            .commit(piece)
            .map_err(|_| truncated(handed, expected, StopReason::Dropped))?;
        handed += piece.encode_utf16().count();
        commits += 1;
        if cadence.due(Instant::now()) {
            progress(SendProgress {
                units_arrived: handed,
                units_total: expected,
            });
        }
    }
    session.settle();
    Ok(SendOutcome {
        units: expected,
        events: commits,
        following: Following::HandedOver,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_text_with_a_nul_is_refused_by_name_before_any_connection() {
        // A D-Bus string cannot hold it, and the send must say so rather than drop it.
        assert_eq!(
            send_text("a\0b", &mut |_| {}),
            Err(SendError::CannotCarry { character: '\0' })
        );
    }

    #[test]
    fn an_empty_text_needs_no_daemon_and_nothing_followed() {
        assert_eq!(
            send_text("", &mut |_| {}),
            Ok(SendOutcome {
                units: 0,
                events: 0,
                following: Following::Application,
            })
        );
    }
}
