//! The call behind `nkb_sys::spaces`: the collection behaviour of the windows of
//! this process, set and read back through the Objective-C runtime.
//!
//! The windows are found as AppKit lists them - `[[NSApplication sharedApplication]
//! windows]` - and each is asked for its `collectionBehavior` and told
//! `setCollectionBehavior:` with what `macos_rules::over_full_screen` makes of it.
//! Three runtime functions and four message shapes are declared in `ffi.rs`, and
//! no crate stands between this package and AppKit.
//!
//! `[NSApplication sharedApplication]` creates the application object when there
//! is none. It is asked only from a program that already has windows, and that
//! program has one, so the call answers what exists. The class is looked up first
//! and the count of windows is the check that the call was not early.

use core::ffi::CStr;

use super::ffi::{self, MutRef};
use crate::macos_rules::{over_full_screen, stands_over_full_screen};
use crate::spaces::SpacesError;

/// `id message(id, SEL)`.
type ToObject = unsafe extern "C" fn(MutRef, MutRef) -> MutRef;
/// `NSUInteger message(id, SEL)`.
type ToCount = unsafe extern "C" fn(MutRef, MutRef) -> usize;
/// `id message(id, SEL, NSUInteger)`.
type AtIndex = unsafe extern "C" fn(MutRef, MutRef, usize) -> MutRef;
/// `void message(id, SEL, NSUInteger)`.
type SetCount = unsafe extern "C" fn(MutRef, MutRef, usize);

fn selector(name: &CStr) -> MutRef {
    unsafe { ffi::sel_registerName(name.as_ptr()) }
}

/// The message functions, each `objc_msgSend` read as the signature of the
/// message it is used for. On arm64 and x86-64 alike a message that returns an
/// object, a count or nothing goes through the plain entry point.
struct Messages {
    to_object: ToObject,
    to_count: ToCount,
    at_index: AtIndex,
    set_count: SetCount,
}

impl Messages {
    fn new() -> Self {
        let entry: unsafe extern "C" fn() = ffi::objc_msgSend;
        // SAFETY: the same function, called with the arguments each of these
        // types lists, which is what the message to each selector below takes.
        unsafe {
            Self {
                to_object: core::mem::transmute::<unsafe extern "C" fn(), ToObject>(entry),
                to_count: core::mem::transmute::<unsafe extern "C" fn(), ToCount>(entry),
                at_index: core::mem::transmute::<unsafe extern "C" fn(), AtIndex>(entry),
                set_count: core::mem::transmute::<unsafe extern "C" fn(), SetCount>(entry),
            }
        }
    }
}

/// Puts the behaviour on every window of the process and reads it back.
pub(crate) fn stand_over_full_screen() -> Result<usize, SpacesError> {
    if unsafe { ffi::pthread_main_np() } != 1 {
        return Err(SpacesError::NotMainThread);
    }
    let class = unsafe { ffi::objc_getClass(c"NSApplication".as_ptr()) };
    if class.is_null() {
        return Err(SpacesError::NoApplication);
    }
    let send = Messages::new();
    let application = unsafe { (send.to_object)(class, selector(c"sharedApplication")) };
    if application.is_null() {
        return Err(SpacesError::NoApplication);
    }
    let windows = unsafe { (send.to_object)(application, selector(c"windows")) };
    if windows.is_null() {
        return Err(SpacesError::NoWindow);
    }
    let count = unsafe { (send.to_count)(windows, selector(c"count")) };
    if count == 0 {
        return Err(SpacesError::NoWindow);
    }
    let (get, set, at) = (
        selector(c"collectionBehavior"),
        selector(c"setCollectionBehavior:"),
        selector(c"objectAtIndex:"),
    );
    let mut failed = 0;
    for index in 0..count {
        let window = unsafe { (send.at_index)(windows, at, index) };
        if window.is_null() {
            failed += 1;
            continue;
        }
        let before = unsafe { (send.to_count)(window, get) } as u64;
        let wanted = over_full_screen(before);
        if wanted != before {
            unsafe { (send.set_count)(window, set, wanted as usize) };
        }
        // The verdict from a READ, as everywhere in this crate.
        let after = unsafe { (send.to_count)(window, get) } as u64;
        if !stands_over_full_screen(after) {
            failed += 1;
        }
    }
    if failed == 0 {
        Ok(count)
    } else {
        Err(SpacesError::NotKept {
            failed,
            windows: count,
        })
    }
}
