//! The folder window and the file manager on Linux (`D175`), both over the session
//! bus that the shortcuts already talk to (`portal`, `bus`): the stream opened by
//! this program from a file socket, the bus library never handed an address.
//!
//! # The folder window
//!
//! The desktop portal's `FileChooser.OpenFile` with `directory` set: a window of
//! the portal's own process, made modal to the palette's window. The answer comes
//! as a signal on a path known before the call (`Portal::request`), and is waited
//! for on the thread `folders.rs` gives it, for as long as a person may take
//! (`PICK_WAIT`).
//!
//! # The file manager
//!
//! `org.freedesktop.FileManager1.ShowFolders`, the interface the file managers of
//! the desktops answer on the session bus (GNOME Files, Dolphin, Nemo, Caja,
//! Thunar), which the bus starts when none is running. It opens the folder itself,
//! where the portal's `OpenDirectory` would show it selected in its parent, and it
//! starts no program of ours: the bus does that, by the desktop's own entry.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use zbus::zvariant::{OwnedValue, Value};

use super::portal::Portal;
use crate::folders::{FAULT, PickAnswer, PickError, PickRequest, Picked, ShowError};
use crate::linux_folder_rules::{
    current_folder, file_uri, parent_window, pick_refusal, picked, show_refusal,
};
use crate::linux_portal_rules::{ANSWER_WAIT, PORTAL_NAME, PORTAL_PATH};

const FILE_CHOOSER_INTERFACE: &str = "org.freedesktop.portal.FileChooser";
const FILE_MANAGER_NAME: &str = "org.freedesktop.FileManager1";
const FILE_MANAGER_PATH: &str = "/org/freedesktop/FileManager1";
const FILE_MANAGER_INTERFACE: &str = "org.freedesktop.FileManager1";

/// How long the folder window may stand before the wait for it is given up. It
/// waits for a person, so the bound is for a portal that never answers, not for
/// the person: a day.
const PICK_WAIT: Duration = Duration::from_secs(24 * 60 * 60);

pub(crate) fn pick_folder(request: PickRequest, answer: PickAnswer) {
    crate::folders::on_a_thread(
        "nkb-folder-window",
        move || pick(&request),
        answer,
        |error| {
            Err(PickError::Failed(format!(
                "the system gave the folder window no thread: {error}"
            )))
        },
        |_| Err(PickError::Failed(String::from(FAULT))),
    );
}

fn pick(request: &PickRequest) -> Result<Picked, PickError> {
    let mut portal = Portal::open().map_err(pick_refusal)?;
    // A program with an entry - the palette makes one when the tester asks for
    // the desktop's shortcuts - is named in the window. Without one it is asked as
    // a program with no name, which the folder window does not refuse.
    let _ = portal.introduce();
    let parent = parent_window(request.owner);
    let start = request
        .start
        .as_deref()
        .and_then(|start| current_folder(start.as_os_str().as_bytes()));
    let (answer, results) = portal
        .request(PICK_WAIT, |connection, handle| {
            let mut options: HashMap<&str, Value<'_>> = HashMap::new();
            options.insert("handle_token", Value::from(handle));
            options.insert("modal", Value::from(true));
            options.insert("directory", Value::from(true));
            if let Some(start) = &start {
                options.insert("current_folder", Value::from(start.clone()));
            }
            connection.call_method(
                Some(PORTAL_NAME),
                PORTAL_PATH,
                Some(FILE_CHOOSER_INTERFACE),
                "OpenFile",
                &(parent.as_str(), request.title.as_str(), options),
            )
        })
        .map_err(pick_refusal)?;
    Ok(match picked(answer, &uris_in(&results))? {
        Some(path) => Picked::Folder(PathBuf::from(OsStr::from_bytes(&path))),
        None => Picked::Cancelled,
    })
}

/// The addresses in the answer of the folder window, through the variants a
/// dictionary of the portal may wrap them in. None when there are none.
fn uris_in(results: &HashMap<String, OwnedValue>) -> Vec<String> {
    let Some(value) = results.get("uris") else {
        return Vec::new();
    };
    let mut value: &Value<'_> = value;
    while let Value::Value(inner) = value {
        value = inner;
    }
    let Value::Array(array) = value else {
        return Vec::new();
    };
    array
        .inner()
        .iter()
        .filter_map(|item| match item {
            Value::Str(text) => Some(text.as_str().to_owned()),
            _ => None,
        })
        .collect()
}

/// Opens `folder` in the desktop's file manager, which the caller has found to be
/// a folder.
///
/// The call is given the time of an answer, not of a call: a file manager that
/// is not running is started by the bus first, and a cold start on a slow machine
/// takes longer than a call is given. Cut short, the window would say the file
/// manager did not answer, and the folder would open a moment later.
pub(crate) fn show_folder(folder: &Path) -> Result<(), ShowError> {
    let uri = file_uri(folder.as_os_str().as_bytes()).ok_or(ShowError::NotAFolder)?;
    let portal = Portal::open_waiting(ANSWER_WAIT).map_err(show_refusal)?;
    portal
        .connection()
        .call_method(
            Some(FILE_MANAGER_NAME),
            FILE_MANAGER_PATH,
            Some(FILE_MANAGER_INTERFACE),
            "ShowFolders",
            &(vec![uri.as_str()], ""),
        )
        .map(drop)
        .map_err(|error| show_refusal(error.into()))
}
