//! The call behind `nkb_sys::screens::work_area` on macOS: each screen's frame and
//! visible frame - the screen less the menu bar and the Dock - as AppKit lists the
//! screens (`[NSScreen screens]`, `frame`, `visibleFrame`).
//!
//! AppKit's rectangles are in its own space: the origin at the BOTTOM left corner
//! of the first screen of that list, `y` growing up. `nkb_sys::screens` turns them
//! into the layout's space, whose origin is the top left corner of the same screen.
//! The first screen is the one with the menu bar and the origin, by AppKit's own
//! definition of the list.
//!
//! On the main thread only, where AppKit lives - the windows ask from there.

use core::ffi::CStr;

use super::ffi::{self, CgRect, MutRef};

/// `id message(id, SEL)`.
type ToObject = unsafe extern "C" fn(MutRef, MutRef) -> MutRef;
/// `NSUInteger message(id, SEL)`.
type ToCount = unsafe extern "C" fn(MutRef, MutRef) -> usize;
/// `id message(id, SEL, NSUInteger)`.
type AtIndex = unsafe extern "C" fn(MutRef, MutRef, usize) -> MutRef;
/// `NSRect message(id, SEL)`: four doubles, which arm64 returns in registers
/// through the plain entry point.
#[cfg(not(target_arch = "x86_64"))]
type ToRect = unsafe extern "C" fn(MutRef, MutRef) -> CgRect;
/// The same on x86-64, where a structure that large comes back through a pointer
/// the caller gives, by the `_stret` entry point.
#[cfg(target_arch = "x86_64")]
type ToRect = unsafe extern "C" fn(*mut CgRect, MutRef, MutRef);

/// More screens than any Mac drives. A longer list is not trusted.
const SCREEN_LIMIT: usize = 64;

/// One screen as AppKit gives it, in AppKit's space.
pub(crate) struct Frames {
    pub frame: CgRect,
    pub visible: CgRect,
}

fn selector(name: &CStr) -> MutRef {
    unsafe { ffi::sel_registerName(name.as_ptr()) }
}

/// Every screen, the one with the menu bar first - or `None` off the main thread,
/// with no screen, or with more than any Mac has.
pub(crate) fn screens() -> Option<Vec<Frames>> {
    if unsafe { ffi::pthread_main_np() } != 1 {
        return None;
    }
    let class = unsafe { ffi::objc_getClass(c"NSScreen".as_ptr()) };
    if class.is_null() {
        return None;
    }
    // The list and the screens come back autoreleased: their pool is drained
    // once the frames are copied out.
    super::pool::in_a_pool(|| listed(class))
}

fn listed(class: MutRef) -> Option<Vec<Frames>> {
    let entry: unsafe extern "C" fn() = ffi::objc_msgSend;
    // SAFETY: the same function, called with the arguments each type lists,
    // which is what the message to each selector below takes.
    let (to_object, to_count, at_index) = unsafe {
        (
            core::mem::transmute::<unsafe extern "C" fn(), ToObject>(entry),
            core::mem::transmute::<unsafe extern "C" fn(), ToCount>(entry),
            core::mem::transmute::<unsafe extern "C" fn(), AtIndex>(entry),
        )
    };
    let list = unsafe { to_object(class, selector(c"screens")) };
    if list.is_null() {
        return None;
    }
    let count = unsafe { to_count(list, selector(c"count")) };
    if count == 0 || count > SCREEN_LIMIT {
        return None;
    }
    let at = selector(c"objectAtIndex:");
    (0..count)
        .map(|index| {
            let screen = unsafe { at_index(list, at, index) };
            (!screen.is_null()).then(|| Frames {
                frame: rect(screen, c"frame"),
                visible: rect(screen, c"visibleFrame"),
            })
        })
        .collect()
}

/// A rectangle-valued property of an object.
fn rect(object: MutRef, name: &CStr) -> CgRect {
    #[cfg(not(target_arch = "x86_64"))]
    {
        let entry: unsafe extern "C" fn() = ffi::objc_msgSend;
        // SAFETY: the message takes the object and the selector and returns
        // an `NSRect`, which is a `CGRect`.
        let send = unsafe { core::mem::transmute::<unsafe extern "C" fn(), ToRect>(entry) };
        unsafe { send(object, selector(name)) }
    }
    #[cfg(target_arch = "x86_64")]
    {
        let entry: unsafe extern "C" fn() = ffi::objc_msgSend_stret;
        // SAFETY: as above, with the rectangle written where the first argument
        // points.
        let send = unsafe { core::mem::transmute::<unsafe extern "C" fn(), ToRect>(entry) };
        let mut out = CgRect {
            origin: ffi::CgPoint { x: 0.0, y: 0.0 },
            size: ffi::CgSize {
                width: 0.0,
                height: 0.0,
            },
        };
        unsafe { send(&raw mut out, object, selector(name)) };
        out
    }
}
