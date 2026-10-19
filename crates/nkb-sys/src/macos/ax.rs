//! What holds the keyboard focus in the application in front, by accessibility.
//!
//! The same question `field.rs` answers on Windows with UI Automation, with the
//! same promise: a few properties of ONE element, none of them its contents.
//! Here those are four:
//!
//! - its role, a type name (`AXTextField`, `AXButton`), compared with a closed
//!   list in `macos_rules.rs` and dropped,
//! - whether its value can be set, a yes or no from `AXUIElementIsAttributeSettable`
//!   that reads no value,
//! - whether it has a selected-range attribute, from the LIST OF ATTRIBUTE NAMES -
//!   the range itself, which is where the caret is, is never asked for,
//! - which process it belongs to, to leave an element of another process alone.
//!
//! Plus, for a terminal, the file name of the program (`program.rs`). Never the
//! title, the description, the value or the text of anything.
//!
//! # Measured 2026-10-19 (macOS 27.0, a Slint window, `plain` field)
//!
//! - asked of the system-wide element the answer is always an error (`-25204`),
//!   so the question is asked of the APPLICATION's element, made from its pid.
//! - the first request after the application started answered `-25212` (no
//!   value) and the second succeeded: the tree wakes on the first question, as a
//!   browser's does on Windows (`D74`). So `-25212` is asked again, every
//!   [`LOOK_STEP_MS`] for at most [`SECOND_LOOK_MS`], and what comes last is
//!   classified.
//! - the field answered `AXTextArea`, subrole `AXUnknown`, value settable, a
//!   selected-range attribute present. A second request took 0.9 ms.
//!
//! Not measured, and so `Unknown` here: Chromium and Firefox (whose trees are
//! built only for a client that asks), Safari, and anything in a terminal beyond
//! Terminal itself.

use core::time::Duration;
use std::time::Instant;

use super::ffi::{self, CfRef, Owned};
use crate::WindowRef;
use crate::field::{FocusedInput, LOOK_STEP_MS, SECOND_LOOK_MS, TIMEOUT_MS};
use crate::macos_rules::{TERMINAL_PROGRAMS, classify};

/// `kAXErrorNoValue`: the element has no focused child yet.
const NO_VALUE: i32 = -25212;

/// What the system said about the focused element, once: the properties and
/// nothing they point at.
struct Reading {
    role: String,
    settable: bool,
    has_range: bool,
}

/// The attribute names of `element` contain `wanted`. Names of attributes, as in
/// "has an AXSelectedTextRange" - the attribute itself is not read.
fn has_attribute(element: CfRef, wanted: &str) -> bool {
    let mut names: CfRef = core::ptr::null();
    if unsafe { ffi::AXUIElementCopyAttributeNames(element, &mut names) } != 0 {
        return false;
    }
    let Some(names) = Owned::new(names) else {
        return false;
    };
    let count = unsafe { ffi::CFArrayGetCount(names.0) };
    (0..count).any(|index| {
        let name = unsafe { ffi::CFArrayGetValueAtIndex(names.0, index) };
        ffi::string_of(name).is_some_and(|name| name == wanted)
    })
}

/// Reads the four properties of the focused element of `app`, or the error code
/// that says why not.
fn read(app: CfRef, pid: i32) -> Result<Reading, i32> {
    let focus_name = ffi::cf_string(c"AXFocusedUIElement").ok_or(-1)?;
    let mut focus: CfRef = core::ptr::null();
    let error = unsafe { ffi::AXUIElementCopyAttributeValue(app, focus_name.0, &mut focus) };
    let focus = Owned::new(focus).ok_or(if error == 0 { -1 } else { error })?;

    // An element of another process is not in the window in front.
    let mut owner = 0i32;
    if unsafe { ffi::AXUIElementGetPid(focus.0, &mut owner) } != 0 || owner != pid {
        return Err(-1);
    }

    let role_name = ffi::cf_string(c"AXRole").ok_or(-1)?;
    let mut role: CfRef = core::ptr::null();
    let error = unsafe { ffi::AXUIElementCopyAttributeValue(focus.0, role_name.0, &mut role) };
    let role = Owned::new(role).ok_or(error)?;
    let role = ffi::string_of(role.0).ok_or(-1)?;

    let value_name = ffi::cf_string(c"AXValue").ok_or(-1)?;
    let mut settable = 0u8;
    let error =
        unsafe { ffi::AXUIElementIsAttributeSettable(focus.0, value_name.0, &mut settable) };
    Ok(Reading {
        role,
        settable: error == 0 && settable != 0,
        has_range: has_attribute(focus.0, "AXSelectedTextRange"),
    })
}

/// What holds the keyboard focus in the application behind `window`, asked fresh.
pub(crate) fn focused_input(window: WindowRef) -> FocusedInput {
    // A pid of zero or below is no process, and the accessibility call does not
    // answer it with an error: it kills the program with a trap (found by the
    // test that asks about a window that does not exist, 2026-10-19).
    let Some(pid) = i32::try_from(window.0).ok().filter(|pid| *pid > 0) else {
        return FocusedInput::Unknown;
    };
    let Some(app) = Owned::new(unsafe { ffi::AXUIElementCreateApplication(pid) }.cast_const())
    else {
        return FocusedInput::Unknown;
    };
    // A request to an application that does not answer holds for seconds.
    unsafe { ffi::AXUIElementSetMessagingTimeout(app.0, TIMEOUT_MS as f32 / 1000.0) };

    let deadline = Instant::now() + Duration::from_millis(SECOND_LOOK_MS);
    let reading = loop {
        match read(app.0, pid) {
            Ok(reading) => break reading,
            Err(NO_VALUE) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(LOOK_STEP_MS));
            }
            Err(_) => return FocusedInput::Unknown,
        }
    };
    let terminal = crate::program::program_name(window)
        .is_some_and(|name| TERMINAL_PROGRAMS.contains(&name.as_str()));
    classify(&reading.role, reading.settable, reading.has_range, terminal)
}
