//! The system calls the macOS route makes, declared and nothing else.
//!
//! No crate stands between this package and the frameworks: the two that wrap
//! them (`core-graphics`, `objc2`) would put an Objective-C bridge into the
//! tree of the `nkb` command-line tool for some twenty C functions. These are
//! those functions, with the types they take, from the headers of the SDK
//! (`CGEvent.h`, `AXUIElement.h`, `CarbonEventsCore.h`, `Processes.h`).
//!
//! `Boolean` in Core Foundation is an unsigned char and `bool` in Core Graphics
//! is the C `_Bool`, and the declarations keep them apart on purpose.

use core::ffi::{c_char, c_void};

pub(crate) type CfRef = *const c_void;
pub(crate) type MutRef = *mut c_void;

/// `kCGEventSourceStatePrivate`.
pub(crate) const STATE_PRIVATE: i32 = -1;
/// `kCGEventSourceStateHIDSystemState` - the state of the keyboard as the
/// hardware and the events posted at the HID tap leave it.
pub(crate) const STATE_HID: i32 = 1;
/// `kCGHIDEventTap`: the only tap at which the HID key state follows an event.
/// Events posted higher are typed all the same and never shown in that state,
/// which is what every event is waited for by (measured, `D138`).
pub(crate) const TAP_HID: u32 = 0;
/// `kCFStringEncodingUTF8`.
pub(crate) const UTF8: u32 = 0x0800_0100;

/// `noErr`.
pub(crate) const NO_ERR: i32 = 0;
/// `kEventClassKeyboard` and `kEventHotKeyPressed`.
pub(crate) const CLASS_KEYBOARD: u32 = u32::from_be_bytes(*b"keyb");
pub(crate) const HOT_KEY_PRESSED: u32 = 5;
/// `kEventParamDirectObject` and `typeEventHotKeyID`: where a hot key event
/// keeps the identifier it was registered with.
pub(crate) const PARAM_DIRECT_OBJECT: u32 = u32::from_be_bytes(*b"----");
pub(crate) const TYPE_HOT_KEY_ID: u32 = u32::from_be_bytes(*b"hkid");
/// `eventNotHandledErr`: the handler leaves the event to the next one.
pub(crate) const EVENT_NOT_HANDLED: i32 = -9874;

/// `ProcessSerialNumber`.
#[repr(C)]
#[derive(Default)]
pub(crate) struct ProcessSerialNumber {
    pub high: u32,
    pub low: u32,
}

/// `EventTypeSpec`.
#[repr(C)]
pub(crate) struct EventTypeSpec {
    pub class: u32,
    pub kind: u32,
}

/// `EventHotKeyID`.
#[repr(C)]
pub(crate) struct EventHotKeyId {
    pub signature: u32,
    pub id: u32,
}

/// `EventHandlerUPP`.
pub(crate) type EventHandler = extern "C" fn(MutRef, MutRef, MutRef) -> i32;

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    pub(crate) fn CFRelease(object: CfRef);
    pub(crate) fn CFArrayGetCount(array: CfRef) -> isize;
    pub(crate) fn CFArrayGetValueAtIndex(array: CfRef, index: isize) -> CfRef;
    pub(crate) fn CFGetTypeID(object: CfRef) -> usize;
    pub(crate) fn CFStringGetTypeID() -> usize;
    pub(crate) fn CFStringCreateWithCString(
        allocator: CfRef,
        text: *const c_char,
        encoding: u32,
    ) -> CfRef;
    pub(crate) fn CFStringGetCString(
        string: CfRef,
        buffer: *mut c_char,
        size: isize,
        encoding: u32,
    ) -> u8;
    pub(crate) fn CFDataGetBytePtr(data: CfRef) -> *const u8;
    pub(crate) fn CFDataGetLength(data: CfRef) -> isize;
    pub(crate) fn CFBooleanGetTypeID() -> usize;
    pub(crate) fn CFBooleanGetValue(boolean: CfRef) -> u8;
    pub(crate) fn CFPreferencesCopyAppValue(key: CfRef, application: CfRef) -> CfRef;
    /// `kCFPreferencesAnyApplication`: the global domain.
    #[allow(non_upper_case_globals)]
    pub(crate) static kCFPreferencesAnyApplication: CfRef;
}

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    pub(crate) fn CGEventSourceCreate(state: i32) -> MutRef;
    pub(crate) fn CGEventCreateKeyboardEvent(source: MutRef, key: u16, down: bool) -> MutRef;
    pub(crate) fn CGEventKeyboardSetUnicodeString(event: MutRef, length: usize, units: *const u16);
    pub(crate) fn CGEventSetFlags(event: MutRef, flags: u64);
    pub(crate) fn CGEventPost(tap: u32, event: MutRef);
    pub(crate) fn CGEventSourceKeyState(state: i32, key: u16) -> bool;
    pub(crate) fn CGEventSourceFlagsState(state: i32) -> u64;
    pub(crate) fn CGPreflightPostEventAccess() -> bool;

    pub(crate) fn AXIsProcessTrusted() -> u8;
    pub(crate) fn AXUIElementCreateApplication(pid: i32) -> MutRef;
    pub(crate) fn AXUIElementSetMessagingTimeout(element: CfRef, seconds: f32) -> i32;
    pub(crate) fn AXUIElementCopyAttributeValue(
        element: CfRef,
        attribute: CfRef,
        value: *mut CfRef,
    ) -> i32;
    pub(crate) fn AXUIElementCopyAttributeNames(element: CfRef, names: *mut CfRef) -> i32;
    pub(crate) fn AXUIElementIsAttributeSettable(
        element: CfRef,
        attribute: CfRef,
        settable: *mut u8,
    ) -> i32;
    pub(crate) fn AXUIElementGetPid(element: CfRef, pid: *mut i32) -> i32;
}

#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    pub(crate) fn GetFrontProcess(psn: *mut ProcessSerialNumber) -> i16;
    pub(crate) fn GetProcessPID(psn: *const ProcessSerialNumber, pid: *mut i32) -> i16;
    pub(crate) fn GetProcessForPID(pid: i32, psn: *mut ProcessSerialNumber) -> i16;
    pub(crate) fn SetFrontProcess(psn: *const ProcessSerialNumber) -> i16;

    pub(crate) fn GetApplicationEventTarget() -> MutRef;
    pub(crate) fn GetEventDispatcherTarget() -> MutRef;
    pub(crate) fn InstallEventHandler(
        target: MutRef,
        handler: EventHandler,
        count: u32,
        types: *const EventTypeSpec,
        user_data: MutRef,
        out: *mut MutRef,
    ) -> i32;
    pub(crate) fn RemoveEventHandler(handler: MutRef) -> i32;
    pub(crate) fn RegisterEventHotKey(
        key: u32,
        modifiers: u32,
        id: EventHotKeyId,
        target: MutRef,
        options: u32,
        out: *mut MutRef,
    ) -> i32;
    pub(crate) fn UnregisterEventHotKey(hot_key: MutRef) -> i32;
    pub(crate) fn ReceiveNextEvent(
        count: u32,
        types: *const EventTypeSpec,
        timeout: f64,
        pull: u8,
        out: *mut MutRef,
    ) -> i32;
    pub(crate) fn SendEventToEventTarget(event: MutRef, target: MutRef) -> i32;
    pub(crate) fn ReleaseEvent(event: MutRef);

    // Text Input Sources and the Unicode utilities: the keyboard layouts and what
    // they type (`layout.rs`). The first two belong to the main thread.
    pub(crate) fn TISCreateInputSourceList(properties: CfRef, include_all_installed: u8) -> CfRef;
    pub(crate) fn TISGetInputSourceProperty(source: CfRef, key: CfRef) -> CfRef;
    /// `kTISPropertyUnicodeKeyLayoutData`: a `CFData` holding the `uchr` table.
    #[allow(non_upper_case_globals)]
    pub(crate) static kTISPropertyUnicodeKeyLayoutData: CfRef;
    pub(crate) fn UCKeyTranslate(
        layout: *const c_void,
        key_code: u16,
        action: u16,
        modifier_state: u32,
        keyboard_type: u32,
        options: u32,
        dead_key_state: *mut u32,
        max_length: usize,
        actual_length: *mut usize,
        text: *mut u16,
    ) -> i32;
    pub(crate) fn LMGetKbdType() -> u8;
    pub(crate) fn GetEventParameter(
        event: MutRef,
        name: u32,
        wanted: u32,
        actual: *mut u32,
        size: u32,
        actual_size: *mut u32,
        data: MutRef,
    ) -> i32;
}

// libSystem is linked into every program, so this block names no library.
unsafe extern "C" {
    pub(crate) fn proc_pidpath(pid: i32, buffer: *mut c_void, size: u32) -> i32;
    /// 1 on the main thread, 0 on any other.
    pub(crate) fn pthread_main_np() -> i32;
}

// The Objective-C runtime: three functions, to send messages to the windows of
// this very process (`spaces.rs`). Not AppKit - the classes are found by name at
// run time, so a program that never opens a window never asks for them.
#[link(name = "objc")]
unsafe extern "C" {
    pub(crate) fn objc_getClass(name: *const c_char) -> MutRef;
    pub(crate) fn sel_registerName(name: *const c_char) -> MutRef;
    // `objc_msgSend` is declared without its arguments and cast to the signature
    // of each message at the call, the only way messages of several shapes can be
    // sent without a declaration per shape.
    pub(crate) fn objc_msgSend();
}

/// The identifier a hot key event was registered with, if it carries one.
///
/// Every handler on the application's event target hears every hot key press,
/// whoever registered it, so each one asks first whether the press is its own.
pub(crate) fn hot_key_id(event: MutRef) -> Option<EventHotKeyId> {
    let mut id = EventHotKeyId {
        signature: 0,
        id: 0,
    };
    let read = unsafe {
        GetEventParameter(
            event,
            PARAM_DIRECT_OBJECT,
            TYPE_HOT_KEY_ID,
            core::ptr::null_mut(),
            core::mem::size_of::<EventHotKeyId>() as u32,
            core::ptr::null_mut(),
            core::ptr::from_mut(&mut id).cast(),
        )
    };
    (read == NO_ERR).then_some(id)
}

/// An owned Core Foundation object, released when it goes out of scope.
pub(crate) struct Owned(pub(crate) CfRef);

impl Owned {
    /// Takes ownership of `object`, or `None` for a null result.
    ///
    /// Lazy on purpose (`then`, not `then_some`): the value is BUILT before the
    /// condition is looked at, a null `Owned` that is built is dropped, and
    /// releasing null kills the program with a trap (found 2026-10-19, in the
    /// first test that asked about an application with no accessibility).
    pub(crate) fn new(object: CfRef) -> Option<Self> {
        (!object.is_null()).then(|| Self(object))
    }
}

impl Drop for Owned {
    fn drop(&mut self) {
        // Never null through `new`, and checked anyway: a release of null is not
        // an error the system reports, it is the end of the process.
        if !self.0.is_null() {
            unsafe { CFRelease(self.0) };
        }
    }
}

/// A Core Foundation string made from a NUL-ended name, for the attribute names
/// the accessibility API takes by string.
pub(crate) fn cf_string(name: &core::ffi::CStr) -> Option<Owned> {
    Owned::new(unsafe { CFStringCreateWithCString(core::ptr::null(), name.as_ptr(), UTF8) })
}

/// The text of a Core Foundation string, if `object` is one and it is valid.
/// Used for the names of roles and attributes - kinds, never contents.
pub(crate) fn string_of(object: CfRef) -> Option<String> {
    if object.is_null() || unsafe { CFGetTypeID(object) != CFStringGetTypeID() } {
        return None;
    }
    let mut buffer = [0 as c_char; 128];
    let ok = unsafe { CFStringGetCString(object, buffer.as_mut_ptr(), 128, UTF8) };
    if ok == 0 {
        return None;
    }
    let text = unsafe { core::ffi::CStr::from_ptr(buffer.as_ptr()) };
    text.to_str().ok().map(str::to_owned)
}
