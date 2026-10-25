//! The remote-desktop portal, asked for one thing: to press the keys that clear a field
//! (`D145` point 74, `OBS-229`). `linux_clearing_rules.rs` has what is decided without a bus,
//! and the measurements.
//!
//! # The order of an opening, and why it is this one
//!
//! 1. `CreateSession` on the remote-desktop interface. Every call that needs the person is
//!    answered by a signal on a path of its own (`Portal::request`).
//! 2. `SelectDevices` with the keyboard and nothing else, asking the desktop to keep the
//!    permission (`persist_mode` 2), and with the token the desktop gave last time.
//! 3. `Start`. The first time it shows a window of the system, in which the person agrees to
//!    the program pressing keys, and the answer comes when they have chosen. With the token
//!    of an earlier agreement it is answered at once (0.0 s measured on GNOME 50) and shows
//!    nothing. The answer carries the token for the next time, and the token is good once.
//!
//! Then `NotifyKeyboardKeysym` presses a key by its keysym, which is how the keys of the
//! recipe are named whatever the layout is. The session is closed when [`Keys`] is dropped,
//! so no session of remote control outlives the clearing it was opened for.
//!
//! The token is kept in a file of the state directory of the person, written whole to a file
//! of its own and put in place with a rename (untouchable rule 31). A token that cannot be
//! kept costs the person the window of the system once more, and nothing else.

use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write as _;
use std::os::unix::fs::OpenOptionsExt as _;

use zbus::zvariant::{ObjectPath, Value};

use super::{Portal, refusal, text_of, unwrapped};
use crate::Waiting;
use crate::linux_clearing_rules::{
    DEVICE_KEYBOARD, PERSIST_UNTIL_REVOKED, REMOTE_DESKTOP_INTERFACE, StateDirs, split_token_file,
    token_file, usable_token,
};
use crate::linux_portal_rules::{
    ANSWER_WAIT, Answer, BIND_WAIT, PORTAL_NAME, PORTAL_PATH, SESSION_INTERFACE, Told,
};
use crate::portal::Unavailable;

/// A session of remote control that may press keys, open until it is dropped.
pub(crate) struct Keys {
    portal: Portal,
    session: String,
}

impl Keys {
    /// Opens a session that may press keys. Blocks while the person reads the window of the
    /// system, the first time, for at most five minutes, and `waiting` hears of it
    /// (`D145` points 76 and 77): first that the desktop is about to be asked, when no
    /// answer of the person's is kept, BEFORE any call is made, then that the question has
    /// stood for a moment, and at the end, whatever it came to, that it is answered.
    pub(crate) fn open(waiting: &mut dyn FnMut(Waiting)) -> Result<Self, Unavailable> {
        let remembered = recall();
        let mut told = Told::new(waiting);
        if remembered.is_none() {
            told.say(Waiting::Asking);
        }
        let opened = Self::open_session(remembered.as_deref(), &mut told);
        told.end();
        opened
    }

    fn open_session(remembered: Option<&str>, told: &mut Told<'_>) -> Result<Self, Unavailable> {
        let mut portal = Portal::open()?;
        let session = portal.create_session_on(REMOTE_DESKTOP_INTERFACE)?;
        // From here the session is closed on every way out, an error included.
        let mut keys = Self { portal, session };
        keys.select_devices(remembered)?;
        keys.start(told)?;
        Ok(keys)
    }

    /// Presses (`down`) or lets go of the key with `keysym`.
    pub(crate) fn key(&self, keysym: u32, down: bool) -> Result<(), Unavailable> {
        let session = ObjectPath::try_from(self.session.as_str()).map_err(|_| Unavailable::Odd)?;
        let keysym = i32::try_from(keysym).map_err(|_| Unavailable::Odd)?;
        let options: HashMap<&str, Value<'_>> = HashMap::new();
        self.portal.connection.call_method(
            Some(PORTAL_NAME),
            PORTAL_PATH,
            Some(REMOTE_DESKTOP_INTERFACE),
            "NotifyKeyboardKeysym",
            &(&session, options, keysym, u32::from(down)),
        )?;
        Ok(())
    }

    fn select_devices(&mut self, restore: Option<&str>) -> Result<(), Unavailable> {
        let session = ObjectPath::try_from(self.session.as_str()).map_err(|_| Unavailable::Odd)?;
        let (answer, _) = self.portal.request(ANSWER_WAIT, |connection, handle| {
            let mut options: HashMap<&str, Value<'_>> = HashMap::new();
            options.insert("handle_token", Value::from(handle));
            options.insert("types", Value::from(DEVICE_KEYBOARD));
            options.insert("persist_mode", Value::from(PERSIST_UNTIL_REVOKED));
            if let Some(token) = restore {
                options.insert("restore_token", Value::from(token));
            }
            connection.call_method(
                Some(PORTAL_NAME),
                PORTAL_PATH,
                Some(REMOTE_DESKTOP_INTERFACE),
                "SelectDevices",
                &(&session, options),
            )
        })?;
        if answer == Answer::Granted {
            Ok(())
        } else {
            Err(refusal(answer))
        }
    }

    fn start(&mut self, told: &mut Told<'_>) -> Result<(), Unavailable> {
        let session = ObjectPath::try_from(self.session.as_str()).map_err(|_| Unavailable::Odd)?;
        // The one call that may show the window, so the one the person is told about.
        let (answer, results) = self.portal.request_telling(
            BIND_WAIT,
            &mut |what| told.say(what),
            |connection, handle| {
                let mut options: HashMap<&str, Value<'_>> = HashMap::new();
                options.insert("handle_token", Value::from(handle));
                connection.call_method(
                    Some(PORTAL_NAME),
                    PORTAL_PATH,
                    Some(REMOTE_DESKTOP_INTERFACE),
                    "Start",
                    &(&session, "", options),
                )
            },
        )?;
        if answer != Answer::Granted {
            return Err(refusal(answer));
        }
        if let Some(token) = results
            .get("restore_token")
            .and_then(|value| text_of(value))
        {
            keep(token);
        }
        // The person may agree and take the keyboard off the list in the window.
        let devices = match results.get("devices").map(|value| unwrapped(value)) {
            Some(Value::U32(devices)) => *devices,
            _ => 0,
        };
        if devices & DEVICE_KEYBOARD == 0 {
            return Err(Unavailable::Cancelled);
        }
        Ok(())
    }
}

impl Drop for Keys {
    fn drop(&mut self) {
        // Best effort and bounded: the portal closes a session whose program has gone from
        // the bus anyway, so a call that does not come back costs only its limit.
        let _ = self.portal.connection.call_method(
            Some(PORTAL_NAME),
            self.session.as_str(),
            Some(SESSION_INTERFACE),
            "Close",
            &(),
        );
        self.portal.stream.shut_down();
    }
}

/// Where the token is kept, as the environment says.
fn state_file() -> Option<String> {
    let state_home = std::env::var("XDG_STATE_HOME").ok();
    let home = std::env::var("HOME").ok();
    token_file(&StateDirs {
        state_home: state_home.as_deref(),
        home: home.as_deref(),
    })
}

/// The token the desktop gave last time, if there is a file with one in it.
fn recall() -> Option<String> {
    let content = fs::read_to_string(state_file()?).ok()?;
    usable_token(&content).map(str::to_owned)
}

/// Keeps the token the desktop just gave, for the next time. Best effort.
fn keep(token: &str) {
    if let Some(path) = state_file() {
        let _ = keep_in(&path, token);
    }
}

/// Puts `token` in the file at `path`, whole or not at all: written to a file of its own in the
/// same folder, made new and readable by the person alone, and then renamed over the old one.
pub(crate) fn keep_in(path: &str, token: &str) -> std::io::Result<()> {
    let token = usable_token(token).ok_or(std::io::ErrorKind::InvalidInput)?;
    let (folder, name) = split_token_file(path).ok_or(std::io::ErrorKind::InvalidInput)?;
    fs::create_dir_all(folder)?;
    let own = format!("{folder}/.{name}.{}.part", std::process::id());
    let outcome = write_whole(&own, token).and_then(|()| fs::rename(&own, path));
    if outcome.is_err() {
        let _ = fs::remove_file(&own);
    }
    outcome
}

fn write_whole(path: &str, token: &str) -> std::io::Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(token.as_bytes())?;
    file.write_all(b"\n")?;
    file.sync_all()
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use std::os::unix::fs::PermissionsExt as _;
    use std::path::PathBuf;

    use super::*;

    /// A folder of its own for one test, removed when the test is over.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(label: &str) -> Self {
            let folder = std::env::temp_dir().join(format!(
                "nkb-remote-{label}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = fs::remove_dir_all(&folder);
            Self(folder)
        }

        fn file(&self) -> String {
            format!("{}/naughty-keyboard/remote-desktop-token", self.0.display())
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_token_that_is_kept_is_read_back_and_nothing_else_is_left_in_the_folder() {
        let scratch = Scratch::new("kept");
        let path = scratch.file();
        keep_in(&path, "d4f1a2b0-77").expect("kept");
        let read = fs::read_to_string(&path).expect("a file");
        assert_eq!(usable_token(&read), Some("d4f1a2b0-77"));
        let names: Vec<String> = fs::read_dir(format!("{}/naughty-keyboard", scratch.0.display()))
            .expect("a folder")
            .map(|entry| {
                entry
                    .expect("an entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        assert_eq!(
            names,
            ["remote-desktop-token"],
            "no file of its own is left"
        );
    }

    #[test]
    fn a_newer_token_replaces_the_older_whole() {
        // The token is good once, so the next start writes over the last.
        let scratch = Scratch::new("replaced");
        let path = scratch.file();
        keep_in(&path, "first-token-with-a-longer-body").expect("kept");
        keep_in(&path, "second").expect("kept");
        let read = fs::read_to_string(&path).expect("a file");
        assert_eq!(usable_token(&read), Some("second"));
    }

    #[test]
    fn the_file_is_the_persons_alone() {
        let scratch = Scratch::new("private");
        let path = scratch.file();
        keep_in(&path, "token").expect("kept");
        let mode = fs::metadata(&path).expect("a file").permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn what_is_not_a_token_is_not_kept() {
        let scratch = Scratch::new("refused");
        let path = scratch.file();
        for bad in ["", "two words", "\u{17c}"] {
            assert!(keep_in(&path, bad).is_err(), "{bad:?}");
        }
        assert!(
            !std::path::Path::new(&path).exists(),
            "nothing was written for a text that is no token"
        );
        assert!(
            keep_in("no-folder", "token").is_err(),
            "a path with no folder"
        );
    }

    #[test]
    fn a_stale_file_of_its_own_does_not_stop_the_next_keep_for_ever() {
        // A crash between the write and the rename leaves the file of its own behind, and the
        // next process has another number, so the token is kept.
        let scratch = Scratch::new("stale");
        let path = scratch.file();
        keep_in(&path, "token").expect("kept");
        let folder = format!("{}/naughty-keyboard", scratch.0.display());
        fs::write(format!("{folder}/.remote-desktop-token.1.part"), "junk").expect("a stale file");
        keep_in(&path, "token-2").expect("kept again");
        let read = fs::read_to_string(&path).expect("a file");
        assert_eq!(usable_token(&read), Some("token-2"));
    }
}
