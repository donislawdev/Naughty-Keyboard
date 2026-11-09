//! The folder panel and Finder on macOS (`D175`), through the Objective-C runtime
//! as `spaces.rs` does it: classes found by name, `objc_msgSend` read as the
//! signature of each message.
//!
//! # The panel
//!
//! `NSOpenPanel` told to choose one directory and no file. AppKit's panels belong
//! to the main thread, and `runModal` runs a loop of its own until the person
//! answers. Run from a handler of the window toolkit that loop would hand winit
//! events while its handler is still in use, and winit 0.30.13 panics then. So
//! the panel runs from the main queue (`dispatch_async_f`), which the main run
//! loop drains between the toolkit's handlers, and the answer is handed on from
//! there, on the main thread.
//!
//! A package - a directory Finder shows as one file, an application above all -
//! is not offered: the panel treats packages as files, and files cannot be chosen.
//!
//! # Finder
//!
//! `[[NSWorkspace sharedWorkspace] openURL:]` with the folder's file address
//! opens it in the person's file browser. A package handed to `openURL:` would be
//! OPENED - an application started - so a package is shown selected in its
//! folder instead (`activateFileViewerSelectingURLs:`), and nothing in it runs.
//! The check is the system's own (`isFilePackageAtPath:`). Asked on the main
//! queue too, from the thread `folders.rs` gives the request, which waits for
//! the answer a bounded time.

use core::ffi::{CStr, c_char};
use std::ffi::{CString, OsStr};
use std::os::unix::ffi::OsStrExt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::time::Duration;

use super::ffi::{self, MutRef};
use crate::folders::{FAULT, PickAnswer, PickError, PickRequest, Picked, ShowError};

/// `NSModalResponseOK`.
const RESPONSE_OK: isize = 1;

/// How long the thread that asked Finder waits for the main queue to take the
/// request. The main thread is free, or in the panel's loop, which drains the
/// main queue too, so this is a bound for a main thread that hangs.
const MAIN_WAIT: Duration = Duration::from_secs(5);

/// `id message(id, SEL)`.
type ToObject = unsafe extern "C" fn(MutRef, MutRef) -> MutRef;
/// `id message(id, SEL, const char *)`.
type WithText = unsafe extern "C" fn(MutRef, MutRef, *const c_char) -> MutRef;
/// `id message(id, SEL, id)`.
type WithObject = unsafe extern "C" fn(MutRef, MutRef, MutRef) -> MutRef;
/// `void message(id, SEL, BOOL)`.
type SetFlag = unsafe extern "C" fn(MutRef, MutRef, bool);
/// `void message(id, SEL, id)`.
type SetObject = unsafe extern "C" fn(MutRef, MutRef, MutRef);
/// `NSInteger message(id, SEL)`.
type ToInteger = unsafe extern "C" fn(MutRef, MutRef) -> isize;
/// `const char *message(id, SEL)`.
type ToText = unsafe extern "C" fn(MutRef, MutRef) -> *const c_char;
/// `BOOL message(id, SEL, id)`.
type ObjectToFlag = unsafe extern "C" fn(MutRef, MutRef, MutRef) -> bool;
/// `+[NSURL fileURLWithFileSystemRepresentation:isDirectory:relativeToURL:]`.
type UrlOfPath = unsafe extern "C" fn(MutRef, MutRef, *const c_char, bool, MutRef) -> MutRef;

fn selector(name: &CStr) -> MutRef {
    unsafe { ffi::sel_registerName(name.as_ptr()) }
}

fn class(name: &CStr) -> MutRef {
    unsafe { ffi::objc_getClass(name.as_ptr()) }
}

/// The message functions, each `objc_msgSend` read as the signature of the
/// message it is used for. On arm64 and x86-64 alike every one of these goes
/// through the plain entry point: none returns a structure or a float.
struct Messages {
    to_object: ToObject,
    with_text: WithText,
    with_object: WithObject,
    set_flag: SetFlag,
    set_object: SetObject,
    to_integer: ToInteger,
    to_text: ToText,
    object_to_flag: ObjectToFlag,
    url_of_path: UrlOfPath,
}

impl Messages {
    fn new() -> Self {
        let entry: unsafe extern "C" fn() = ffi::objc_msgSend;
        // SAFETY: the same function, called with the arguments each of these
        // types lists, which is what the message each is used for takes.
        unsafe {
            Self {
                to_object: core::mem::transmute::<unsafe extern "C" fn(), ToObject>(entry),
                with_text: core::mem::transmute::<unsafe extern "C" fn(), WithText>(entry),
                with_object: core::mem::transmute::<unsafe extern "C" fn(), WithObject>(entry),
                set_flag: core::mem::transmute::<unsafe extern "C" fn(), SetFlag>(entry),
                set_object: core::mem::transmute::<unsafe extern "C" fn(), SetObject>(entry),
                to_integer: core::mem::transmute::<unsafe extern "C" fn(), ToInteger>(entry),
                to_text: core::mem::transmute::<unsafe extern "C" fn(), ToText>(entry),
                object_to_flag: core::mem::transmute::<unsafe extern "C" fn(), ObjectToFlag>(entry),
                url_of_path: core::mem::transmute::<unsafe extern "C" fn(), UrlOfPath>(entry),
            }
        }
    }

    /// The file address of `path` as AppKit takes it, or null.
    fn url(&self, path: &CStr, directory: bool) -> MutRef {
        let url = class(c"NSURL");
        if url.is_null() {
            return url;
        }
        unsafe {
            (self.url_of_path)(
                url,
                selector(c"fileURLWithFileSystemRepresentation:isDirectory:relativeToURL:"),
                path.as_ptr(),
                directory,
                core::ptr::null_mut(),
            )
        }
    }
}

/// Runs `work` inside an autorelease pool of its own, so what AppKit hands back
/// for a moment is released when it is done. What `work` returns must not point
/// into the pool: it is copied out first.
fn in_a_pool<T>(work: impl FnOnce() -> T) -> T {
    let pool = unsafe { ffi::objc_autoreleasePoolPush() };
    let result = work();
    unsafe { ffi::objc_autoreleasePoolPop(pool) };
    result
}

/// `path` as the C string the system takes, or `None` for a path with a zero in
/// it, which no folder has.
fn representation(path: &Path) -> Option<CString> {
    CString::new(path.as_os_str().as_bytes()).ok()
}

/// What runs on the main queue: the request, and where its answer goes.
struct Pending {
    request: PickRequest,
    answer: PickAnswer,
}

pub(crate) fn pick_folder(request: PickRequest, answer: PickAnswer) {
    let pending = Box::into_raw(Box::new(Pending { request, answer }));
    unsafe {
        ffi::dispatch_async_f(&raw const ffi::_dispatch_main_q, pending.cast(), run_panel);
    }
}

/// On the main queue. A panic must not leave this function - it would unwind
/// into libdispatch - so the work runs under `catch_unwind`, and a fault is an
/// answer like any other.
extern "C" fn run_panel(context: MutRef) {
    let pending = unsafe { Box::from_raw(context.cast::<Pending>()) };
    let Pending { request, answer } = *pending;
    let result = catch_unwind(AssertUnwindSafe(|| in_a_pool(|| panel(&request))))
        .unwrap_or_else(|_| Err(PickError::Failed(String::from(FAULT))));
    let _ = catch_unwind(AssertUnwindSafe(|| answer(result)));
}

fn panel(request: &PickRequest) -> Result<Picked, PickError> {
    let send = Messages::new();
    let open_panel = class(c"NSOpenPanel");
    if open_panel.is_null() {
        return Err(PickError::Failed(String::from(
            "the folder panel is not in this program",
        )));
    }
    let panel = unsafe { (send.to_object)(open_panel, selector(c"openPanel")) };
    if panel.is_null() {
        return Err(PickError::Failed(String::from(
            "the folder panel could not be made",
        )));
    }
    unsafe {
        (send.set_flag)(panel, selector(c"setCanChooseDirectories:"), true);
        (send.set_flag)(panel, selector(c"setCanChooseFiles:"), false);
        (send.set_flag)(panel, selector(c"setAllowsMultipleSelection:"), false);
        (send.set_flag)(panel, selector(c"setCanCreateDirectories:"), true);
    }
    if let Ok(title) = CString::new(request.title.as_str()) {
        let string = class(c"NSString");
        if !string.is_null() {
            let message = unsafe {
                (send.with_text)(string, selector(c"stringWithUTF8String:"), title.as_ptr())
            };
            if !message.is_null() {
                unsafe { (send.set_object)(panel, selector(c"setMessage:"), message) };
            }
        }
    }
    if let Some(start) = request.start.as_deref().and_then(representation) {
        let url = send.url(&start, true);
        if !url.is_null() {
            unsafe { (send.set_object)(panel, selector(c"setDirectoryURL:"), url) };
        }
    }
    let answered = unsafe { (send.to_integer)(panel, selector(c"runModal")) };
    if answered != RESPONSE_OK {
        return Ok(Picked::Cancelled);
    }
    let url = unsafe { (send.to_object)(panel, selector(c"URL")) };
    if url.is_null() {
        return Err(PickError::Failed(String::from(
            "the folder panel gave no folder",
        )));
    }
    let path = unsafe { (send.to_text)(url, selector(c"fileSystemRepresentation")) };
    if path.is_null() {
        return Err(PickError::Failed(String::from(
            "the folder chosen has no path",
        )));
    }
    // Copied out of the pool before it is drained.
    let path = unsafe { CStr::from_ptr(path) };
    Ok(Picked::Folder(PathBuf::from(OsStr::from_bytes(
        path.to_bytes(),
    ))))
}

/// What runs on the main queue to open a folder: the folder, and where the
/// answer goes.
struct Showing {
    folder: CString,
    told: Sender<Result<(), ShowError>>,
}

/// Opens `folder` in Finder, which the caller has found to be a folder. Asked
/// on the main queue, and waited for here.
///
/// On the main thread itself it is asked at once: waiting there for the main
/// queue would hold the queue's own thread, the wait would run out, and Finder
/// would open after the answer had said it did not.
pub(crate) fn show_folder(folder: &Path) -> Result<(), ShowError> {
    let folder = representation(folder).ok_or(ShowError::NotAFolder)?;
    if unsafe { ffi::pthread_main_np() } == 1 {
        return in_a_pool(|| finder(&folder));
    }
    let (told, heard) = mpsc::channel();
    let showing = Box::into_raw(Box::new(Showing { folder, told }));
    unsafe {
        ffi::dispatch_async_f(&raw const ffi::_dispatch_main_q, showing.cast(), run_finder);
    }
    heard.recv_timeout(MAIN_WAIT).unwrap_or_else(|_| {
        Err(ShowError::Failed(String::from(
            "the main thread did not take the request in time",
        )))
    })
}

extern "C" fn run_finder(context: MutRef) {
    let showing = unsafe { Box::from_raw(context.cast::<Showing>()) };
    let Showing { folder, told } = *showing;
    let result = catch_unwind(AssertUnwindSafe(|| in_a_pool(|| finder(&folder))))
        .unwrap_or_else(|_| Err(ShowError::Failed(String::from(FAULT))));
    // A thread that stopped waiting has gone: the answer has nobody to reach.
    let _ = told.send(result);
}

fn finder(folder: &CStr) -> Result<(), ShowError> {
    let send = Messages::new();
    let workspace = class(c"NSWorkspace");
    if workspace.is_null() {
        return Err(ShowError::NoFileManager);
    }
    let workspace = unsafe { (send.to_object)(workspace, selector(c"sharedWorkspace")) };
    if workspace.is_null() {
        return Err(ShowError::NoFileManager);
    }
    let url = send.url(folder, true);
    if url.is_null() {
        return Err(ShowError::Failed(String::from(
            "the folder has no file address",
        )));
    }
    let path = unsafe { (send.to_object)(url, selector(c"path")) };
    let package = !path.is_null()
        && unsafe { (send.object_to_flag)(workspace, selector(c"isFilePackageAtPath:"), path) };
    if package {
        let array = class(c"NSArray");
        let list = if array.is_null() {
            core::ptr::null_mut()
        } else {
            unsafe { (send.with_object)(array, selector(c"arrayWithObject:"), url) }
        };
        if list.is_null() {
            return Err(ShowError::Failed(String::from(
                "the folder could not be shown in its folder",
            )));
        }
        unsafe {
            (send.set_object)(
                workspace,
                selector(c"activateFileViewerSelectingURLs:"),
                list,
            );
        }
        return Ok(());
    }
    if unsafe { (send.object_to_flag)(workspace, selector(c"openURL:"), url) } {
        Ok(())
    } else {
        Err(ShowError::Failed(String::from("Finder did not open it")))
    }
}
