//! The portal for global shortcuts: the calls, the answers, and the thread that hears the
//! presses (`D145` points 39 and 53, `OBS-217`, `OBS-222`). `portal.rs` at the root of the
//! crate says what this route is and is not, and `linux_portal_rules.rs` has what it
//! decides without a bus, and the measurements.
//!
//! # The order of a start, and why it is this one
//!
//! 1. Connect to the file socket of the session bus (`bus`) and say who the program is
//!    (`Registry.Register`), which the portal accepts only for a program that has an entry
//!    in a directory of applications.
//! 2. `CreateSession`. Every call of the portal that needs the person is answered by a
//!    signal on a path of its own, which is known before the call is made. So the listening
//!    starts FIRST and the answer cannot be missed.
//! 3. The listening for presses starts before the bind, for the same reason: a press the
//!    moment the shortcuts are given must not fall between the answer and the thread.
//! 4. `BindShortcuts`. The first time it shows a window of the system, and the answer comes
//!    when the person has chosen.
//!
//! # Stopping
//!
//! The thread waits for a signal and a wait for a signal cannot be interrupted, so the
//! connection is shut and the wait returns (`bus::shut_down`), after the session has been
//! closed on the portal's side as well as can be done in a moment.

pub(crate) mod entry;

use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::JoinHandle;

use zbus::blocking::connection::Builder;
use zbus::blocking::{Connection, MessageIterator};
use zbus::message::Type as MessageType;
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value};
use zbus::{Error, MatchRule, Message, fdo};

use super::bus::{self, Handle};
use crate::hotkey::HotkeyId;
use crate::linux_portal_rules::{
    ANSWER_WAIT, APP_ID, Answer, BIND_WAIT, CALL_WAIT, PORTAL_NAME, PORTAL_PATH,
    REGISTRY_INTERFACE, REQUEST_INTERFACE, SESSION_INTERFACE, SHORTCUTS_INTERFACE, answer_of,
    bound_hotkey, hotkey_id_of, preferred_trigger, request_path, session_bus_socket, shortcut_id,
    token,
};
use crate::portal::{Bound, Unavailable, Wish};

/// The answer of a request: a dictionary of what the portal has to say.
type Results = HashMap<String, OwnedValue>;

/// Names an error of the bus by what it says about the portal. A method the portal does not
/// have, or no portal at all, is a desktop without this route. The words the portal uses for
/// a program it does not know are the other one worth a name. Everything else is a bus that
/// did not do its part.
fn classify(error: &Error) -> Unavailable {
    let said = error.to_string();
    if said.contains("App info not found") || said.contains("app id is required") {
        return Unavailable::NeedsEntry;
    }
    match error {
        Error::MethodError(name, ..)
            if matches!(
                name.as_str(),
                "org.freedesktop.DBus.Error.ServiceUnknown"
                    | "org.freedesktop.DBus.Error.UnknownMethod"
                    | "org.freedesktop.DBus.Error.UnknownInterface"
                    | "org.freedesktop.DBus.Error.UnknownObject"
            ) =>
        {
            Unavailable::NoPortal
        }
        Error::FDO(fdo)
            if matches!(
                **fdo,
                fdo::Error::ServiceUnknown(_)
                    | fdo::Error::UnknownMethod(_)
                    | fdo::Error::UnknownInterface(_)
                    | fdo::Error::UnknownObject(_)
            ) =>
        {
            Unavailable::NoPortal
        }
        Error::MethodError(..) | Error::FDO(_) | Error::Variant(_) | Error::InvalidReply => {
            Unavailable::Odd
        }
        _ => Unavailable::Unreachable,
    }
}

impl From<Error> for Unavailable {
    fn from(error: Error) -> Self {
        classify(&error)
    }
}

impl From<std::io::Error> for Unavailable {
    fn from(_: std::io::Error) -> Self {
        Self::Unreachable
    }
}

/// The text in a value, through the variants a dictionary of the portal wraps it in.
fn text_of<'v>(value: &'v Value<'_>) -> Option<&'v str> {
    match value {
        Value::Str(text) => Some(text.as_str()),
        Value::Value(inner) => text_of(inner),
        _ => None,
    }
}

/// The value inside the variants that wrap it.
fn unwrapped<'v, 'a>(value: &'v Value<'a>) -> &'v Value<'a> {
    match value {
        Value::Value(inner) => unwrapped(inner),
        other => other,
    }
}

/// One shortcut of the answer to a bind, or `None` when it is not one of ours or is not in
/// the shape the specification gives.
fn bound_one(item: &Value<'_>) -> Option<Bound> {
    let Value::Structure(entry) = unwrapped(item) else {
        return None;
    };
    let [id, info] = entry.fields() else {
        return None;
    };
    let id = hotkey_id_of(text_of(id)?)?;
    let Value::Dict(info) = unwrapped(info) else {
        return None;
    };
    let words = info
        .iter()
        .find(|(key, _)| text_of(key) == Some("trigger_description"))
        .and_then(|(_, value)| text_of(value))
        .unwrap_or("");
    Some(Bound {
        id,
        words: words.to_owned(),
        chord: bound_hotkey(id, words),
    })
}

/// The shortcuts in the answer to a bind. None when the person was given none.
fn bound_in(results: &Results) -> Vec<Bound> {
    let Some(list) = results.get("shortcuts") else {
        return Vec::new();
    };
    let list: &Value<'_> = list;
    let Value::Array(array) = unwrapped(list) else {
        return Vec::new();
    };
    array.inner().iter().filter_map(bound_one).collect()
}

/// What an answer other than agreement means for the caller.
const fn refusal(answer: Answer) -> Unavailable {
    match answer {
        Answer::Granted => Unavailable::Odd,
        Answer::Cancelled => Unavailable::Cancelled,
        Answer::Ended(code) => Unavailable::Ended(code),
    }
}

/// A connection to the session bus that has been introduced to the portal.
struct Portal {
    connection: Connection,
    /// A second handle on the socket, to shut it and to keep a watch on it.
    stream: Handle,
    unique: String,
    requests: u32,
}

impl Portal {
    fn open() -> Result<Self, Unavailable> {
        let address = std::env::var("DBUS_SESSION_BUS_ADDRESS").ok();
        let runtime = std::env::var("XDG_RUNTIME_DIR").ok();
        let socket = session_bus_socket(address.as_deref(), runtime.as_deref())
            .map_err(|_| Unavailable::NoBus)?;
        let (stream, kept) = bus::open(&socket)?;
        // The introduction to the bus is waited for like any answer, and has the same limit.
        let _watch = kept.watch(ANSWER_WAIT)?;
        let connection = Builder::async_io_unix_stream(stream)
            .method_timeout(CALL_WAIT)
            .build()?;
        let unique = connection
            .unique_name()
            .map(|name| name.as_str().to_owned())
            .ok_or(Unavailable::Odd)?;
        Ok(Self {
            connection,
            stream: kept,
            unique,
            requests: 0,
        })
    }

    /// Says who the program is. Only a program with an entry is accepted. A portal without
    /// the call, or a program that has an identifier by other means, goes on to the next
    /// call, which says whether it was enough.
    fn introduce(&self) -> Result<(), Unavailable> {
        let options: HashMap<&str, Value<'_>> = HashMap::new();
        let said = self.connection.call_method(
            Some(PORTAL_NAME),
            PORTAL_PATH,
            Some(REGISTRY_INTERFACE),
            "Register",
            &(APP_ID, options),
        );
        match said {
            Ok(_) => Ok(()),
            Err(error) if classify(&error) == Unavailable::NeedsEntry => {
                Err(Unavailable::NeedsEntry)
            }
            Err(_) => Ok(()),
        }
    }

    fn next_token(&mut self, prefix: &str) -> String {
        self.requests += 1;
        token(prefix, std::process::id(), self.requests)
    }

    /// Makes a call that the portal answers with a signal, and waits for the answer for at
    /// most `wait`. The listening starts before the call, on the path the answer will come
    /// from, so it cannot be missed.
    fn request<F>(
        &mut self,
        wait: core::time::Duration,
        call: F,
    ) -> Result<(Answer, Results), Unavailable>
    where
        F: FnOnce(&Connection, &str) -> zbus::Result<Message>,
    {
        let handle = self.next_token("nkb");
        let path = request_path(&self.unique, &handle).ok_or(Unavailable::Odd)?;
        let rule = MatchRule::builder()
            .msg_type(MessageType::Signal)
            .interface(REQUEST_INTERFACE)?
            .member("Response")?
            .path(path.as_str())?
            .build();
        let mut responses = MessageIterator::for_match_rule(rule, &self.connection, Some(2))?;
        let reply = call(&self.connection, &handle)?;
        let announced: OwnedObjectPath = reply.body().deserialize()?;
        if announced.as_str() != path {
            return Err(Unavailable::Odd);
        }
        // If the portal never answers, the watch shuts the connection and the wait returns.
        let _watch = self.stream.watch(wait)?;
        let message = responses.next().ok_or(Unavailable::Unreachable)??;
        let (code, results): (u32, Results) = message.body().deserialize()?;
        Ok((answer_of(code), results))
    }

    /// A session of shortcuts, and the path the portal knows it by.
    fn create_session(&mut self) -> Result<String, Unavailable> {
        let session_token = self.next_token("nkbs");
        let (answer, results) = self.request(ANSWER_WAIT, |connection, handle| {
            let mut options: HashMap<&str, Value<'_>> = HashMap::new();
            options.insert("handle_token", Value::from(handle));
            options.insert("session_handle_token", Value::from(session_token.as_str()));
            connection.call_method(
                Some(PORTAL_NAME),
                PORTAL_PATH,
                Some(SHORTCUTS_INTERFACE),
                "CreateSession",
                &(options,),
            )
        })?;
        if answer != Answer::Granted {
            return Err(refusal(answer));
        }
        let session = results
            .get("session_handle")
            .and_then(|value| text_of(value))
            .ok_or(Unavailable::Odd)?;
        ObjectPath::try_from(session).map_err(|_| Unavailable::Odd)?;
        Ok(session.to_owned())
    }

    /// Asks for the shortcuts and returns the ones the person was given.
    fn bind(&mut self, session: &str, wishes: &[Wish]) -> Result<Vec<Bound>, Unavailable> {
        let session_path = ObjectPath::try_from(session).map_err(|_| Unavailable::Odd)?;
        let (answer, results) = self.request(BIND_WAIT, |connection, handle| {
            let shortcuts: Vec<(String, HashMap<&str, Value<'_>>)> = wishes
                .iter()
                .map(|wish| {
                    let mut info: HashMap<&str, Value<'_>> = HashMap::new();
                    info.insert("description", Value::from(wish.description.as_str()));
                    if let Some(offered) = preferred_trigger(&wish.hotkey) {
                        info.insert("preferred_trigger", Value::from(offered));
                    }
                    (shortcut_id(wish.hotkey.id), info)
                })
                .collect();
            let mut options: HashMap<&str, Value<'_>> = HashMap::new();
            options.insert("handle_token", Value::from(handle));
            connection.call_method(
                Some(PORTAL_NAME),
                PORTAL_PATH,
                Some(SHORTCUTS_INTERFACE),
                "BindShortcuts",
                &(&session_path, shortcuts, "", options),
            )
        })?;
        if answer != Answer::Granted {
            return Err(refusal(answer));
        }
        Ok(bound_in(&results))
    }

    /// The presses of the shortcuts of `session`, as the portal announces them.
    fn presses(&self) -> Result<MessageIterator, Unavailable> {
        let rule = MatchRule::builder()
            .msg_type(MessageType::Signal)
            .interface(SHORTCUTS_INTERFACE)?
            .member("Activated")?
            .path(PORTAL_PATH)?
            .build();
        Ok(MessageIterator::for_match_rule(
            rule,
            &self.connection,
            Some(64),
        )?)
    }
}

/// Hears the presses until the connection is shut, and passes on the ones of our session.
fn hear(presses: MessageIterator, session: &str, fired: &Sender<HotkeyId>) {
    for message in presses {
        let Ok(message) = message else {
            break;
        };
        let Ok((from, shortcut, _time, _options)) =
            message
                .body()
                .deserialize::<(OwnedObjectPath, String, u64, Results)>()
        else {
            continue;
        };
        if from.as_str() != session {
            continue;
        }
        if let Some(id) = hotkey_id_of(&shortcut)
            && fired.send(id).is_err()
        {
            break;
        }
    }
}

/// The session of shortcuts and the thread that hears them.
pub struct Listener {
    connection: Connection,
    stream: Handle,
    session: String,
    thread: Option<JoinHandle<()>>,
}

impl Listener {
    pub fn stop(mut self) {
        self.shut_down();
    }

    fn shut_down(&mut self) {
        let Some(thread) = self.thread.take() else {
            return;
        };
        // Best effort and bounded: the portal closes a session whose program has gone from
        // the bus anyway, so a call that does not come back costs only its limit.
        let _ = self.connection.call_method(
            Some(PORTAL_NAME),
            self.session.as_str(),
            Some(SESSION_INTERFACE),
            "Close",
            &(),
        );
        self.stream.shut_down();
        let _ = thread.join();
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        self.shut_down();
    }
}

/// What a start gives: the shortcuts bound, the presses of them, and the listener.
pub(crate) type Begun = (Vec<Bound>, Receiver<HotkeyId>, Listener);

/// Asks the portal for `wishes`. See `portal::start`.
pub(crate) fn start(wishes: &[Wish]) -> Result<Begun, Unavailable> {
    let mut portal = Portal::open()?;
    portal.introduce()?;
    let session = portal.create_session()?;
    let presses = portal.presses()?;
    let bound = portal.bind(&session, wishes)?;
    let (sender, fired) = mpsc::channel();
    let thread = std::thread::Builder::new()
        .name("nkb-portal".to_owned())
        .spawn({
            let session = session.clone();
            move || hear(presses, &session, &sender)
        })?;
    let Portal {
        connection, stream, ..
    } = portal;
    let listener = Listener {
        connection,
        stream,
        session,
        thread: Some(thread),
    };
    Ok((bound, fired, listener))
}
