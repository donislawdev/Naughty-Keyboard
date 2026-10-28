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
//! What this route cannot say, and the person is told: whether the field took what it was
//! handed. The text is HANDED OVER to the input method, and what the application did with it
//! is for the person to look at (`Following::HandedOver`). A held modifier and Caps Lock do
//! not apply: a commit is not a key.
//!
//! What it can say, since the engine asks the daemon to name the input context that has the
//! focus (`D159`), is two things. The focus may be the daemon's own, which it is when the
//! desktop or a window that takes no typed text has it, and then nothing is sent. And the
//! field may lose the focus while the value goes in, when another application comes to the
//! front, and then the send stops before the next piece. A move between two fields of one
//! window is not heard (`linux_ibus_rules::Trail`). `Escape`, the caller's flag and the field being left
//! are looked for in every wait of the send, not only between pieces.
//!
//! The keys that clear a field are keys, and the same engine stands guard over them
//! (`keys`, `OBS-229`): they go through the remote-desktop portal, and the engine sees each
//! before the application, with the modifiers the hand holds.

mod daemon;
mod engine;
mod keys;

use core::time::Duration;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use zbus::blocking::Connection;
use zbus::zvariant::Value;

use self::daemon::Opened;
use self::engine::{
    BUS, BUS_PATH, ENGINE_INTERFACE, Event, Factory, Gate, component, expect, ibus_text,
    name_of_engine,
};
pub(crate) use self::keys::press_chords;
use crate::cadence::{Cadence, PROGRESS_INTERVAL};
use crate::linux_clearing_rules::{Expected, Judgement, keys_refusal};
use crate::linux_ibus_rules::{
    COMMIT_GAP, DEADLINE, ESCAPE_LOOK, FOCUS_SETTLE, FOCUS_WAIT, FORGET_LOOK, FORGET_WAIT,
    SETTLE_AFTER_COMMIT, Trail, deadline_for, engine_name, engine_to_restore, is_our_engine,
    pieces,
};
use crate::linux_rules::Refusal;
use crate::portal::Unavailable;
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

fn stopped_chords(chords_sent: usize, chords_expected: usize, reason: StopReason) -> SendError {
    SendError::ChordsTruncated {
        chords_sent,
        chords_expected,
        reason,
    }
}

/// The keys cannot be pressed on this session, and `why` is what the portal said.
fn unsupported_keys(why: Unavailable) -> SendError {
    SendError::Unsupported {
        system: keys_refusal(why).system(),
    }
}

/// Why a send may not begin to type or press keys.
#[derive(Debug)]
enum Unready {
    /// A call to the daemon failed.
    Failed,
    /// The daemon told the engine of no focus at all in `FOCUS_WAIT`.
    Unfocused,
    /// `Escape`, the caller's flag, or a focus that belongs to nobody.
    Stopped(StopReason),
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
    /// The key of the recipe the engine waits for, while the keys that clear a field are sent.
    gate: Gate,
    /// What the daemon said of the focus, and whether the field was left once the send began.
    trail: Trail,
}

impl Session {
    fn begin(socket: &str, deadline: Duration) -> Result<Self, Failed> {
        let name = engine_name(std::process::id());
        let (sender, events) = mpsc::channel();
        let escape = Arc::new(AtomicBool::new(false));
        let gate: Gate = Arc::new(Mutex::new(None));
        let factory = Factory::new(name.clone(), sender, Arc::clone(&escape), Arc::clone(&gate));
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
            gate,
            trail: Trail::default(),
        })
    }

    /// Tells the engine which key of the recipe to wait for next, or none.
    fn expect(&self, expected: Option<Expected>) {
        expect(&self.gate, expected);
    }

    /// Takes what the engine said: the path it was made at, what the daemon said of the
    /// focus, and what it found on a key of the recipe, which is returned.
    fn take(&mut self, event: Event) -> Option<Judgement> {
        match event {
            Event::Created(path) => self.path = Some(path),
            Event::Focus(heard) => self.trail.hear(heard),
            Event::Key(judged) => return Some(judged),
        }
        None
    }

    /// What the engine found on the next key of the recipe, if it said within `wait`. The
    /// answers that came before the key was sent are not this key's, so they are dropped
    /// first by [`Session::forget_echoes`].
    fn echo(&mut self, wait: Duration) -> Option<Judgement> {
        let end = Instant::now() + wait;
        while let Some(left) = end.checked_duration_since(Instant::now()) {
            let event = self.events.recv_timeout(left).ok()?;
            if let Some(judged) = self.take(event) {
                return Some(judged);
            }
        }
        None
    }

    /// Drops what the engine said of keys before now. What it said of the focus is kept.
    fn forget_echoes(&mut self) {
        while let Ok(event) = self.events.try_recv() {
            self.take(event);
        }
    }

    /// Why the send must stop now, if it must: the caller's flag (R03), `Escape` pressed
    /// since the engine was made the global one (`D96`), or the field left (`D159`).
    fn stopped(&mut self, halt: &AtomicBool) -> Option<StopReason> {
        self.forget_echoes();
        if halt.load(Ordering::Relaxed) {
            Some(StopReason::Halted)
        } else if self.escape.load(Ordering::SeqCst) {
            Some(StopReason::Escape)
        } else if self.trail.left() {
            Some(StopReason::FieldLeft)
        } else {
            None
        }
    }

    /// Waits for `time`, looking every `ESCAPE_LOOK` for a reason to stop, which it returns.
    fn pause(&mut self, time: Duration, halt: &AtomicBool) -> Option<StopReason> {
        let end = Instant::now() + time;
        while let Some(left) = end.checked_duration_since(Instant::now()) {
            if let Some(reason) = self.stopped(halt) {
                return Some(reason);
            }
            std::thread::sleep(left.min(ESCAPE_LOOK));
        }
        self.stopped(halt)
    }

    /// Waits until the daemon says an input context has the focus, gives the application a
    /// moment to switch to the engine, and begins following the focus. A focus that belongs
    /// to nobody is no field to send to (`D159`).
    fn ready(&mut self, halt: &AtomicBool) -> Result<(), Unready> {
        let deadline = Instant::now() + FOCUS_WAIT;
        loop {
            let Some(left) = deadline.checked_duration_since(Instant::now()) else {
                return Err(Unready::Unfocused);
            };
            match self.events.recv_timeout(left.min(ESCAPE_LOOK)) {
                Ok(event) => {
                    self.take(event);
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return Err(Unready::Failed),
            }
            if let Some(reason) = self.stopped(halt) {
                return Err(Unready::Stopped(reason));
            }
            // The engine is made before the daemon tells it anything, so a commit has a
            // path to go from by the time it has a place to go to.
            if self.trail.focused() && self.path.is_some() {
                break;
            }
        }
        if let Some(reason) = self.pause(FOCUS_SETTLE, halt) {
            return Err(Unready::Stopped(reason));
        }
        self.trail.begin().map_err(Unready::Stopped)
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
    halt: &AtomicBool,
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
    let result = hand_over(&mut session, text, expected, halt, progress);
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
    halt: &AtomicBool,
    progress: &mut dyn FnMut(SendProgress),
) -> Result<SendOutcome, SendError> {
    session.ready(halt).map_err(|unready| match unready {
        Unready::Failed => unsupported(),
        // The engine is made and nobody was told they have the focus: nothing was sent.
        Unready::Unfocused => truncated(0, expected, StopReason::Dropped),
        Unready::Stopped(reason) => truncated(0, expected, reason),
    })?;

    let mut cadence = Cadence::starting(Instant::now(), PROGRESS_INTERVAL);
    let mut handed = 0usize;
    let mut commits = 0usize;
    for (number, piece) in pieces(text).into_iter().enumerate() {
        // `Escape`, the caller's flag and the field being left stop a send before the next
        // piece, as on every route: what was handed over is counted, and nothing more goes
        // (`D96`, R03, `D159`).
        let stop = if number > 0 {
            session.pause(COMMIT_GAP, halt)
        } else {
            session.stopped(halt)
        };
        if let Some(reason) = stop {
            return Err(truncated(handed, expected, reason));
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
            send_text("a\0b", &AtomicBool::new(false), &mut |_| {}),
            Err(SendError::CannotCarry { character: '\0' })
        );
    }

    #[test]
    fn an_empty_text_needs_no_daemon_and_nothing_followed() {
        assert_eq!(
            send_text("", &AtomicBool::new(false), &mut |_| {}),
            Ok(SendOutcome {
                units: 0,
                events: 0,
                following: Following::Application,
            })
        );
    }
}
