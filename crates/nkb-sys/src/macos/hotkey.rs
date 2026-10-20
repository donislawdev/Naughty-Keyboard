//! Global shortcuts through the hot key calls of Carbon.
//!
//! # No thread of its own
//!
//! Windows needs one: a hot key there is a message in the queue of the thread
//! that registered it, and only that thread can drain it (`architektura.md`
//! 6.5). Here the press arrives as an event of the application, and the loop of
//! the application delivers it - on the thread that runs the loop, not on the
//! one that registered (`macos_hotkeys.rs` has the measurement). So registering
//! is a call from the caller's own thread, and the presses are heard only while
//! the process runs an application loop, which the palette does on its main
//! thread. In a process with no loop - a test, an example - a press is queued
//! and nobody takes it, and that is said in `hotkey::listen`.
//!
//! # One handler for the process, and a sink for each listener
//!
//! Every hot key press reaches every handler on the application's target, so a
//! handler per listener would be asked about presses that are not its own, and
//! one that is removed while the loop is inside it would be a use of freed
//! memory. Instead ONE handler is installed, the first time anybody listens, and
//! stays for the life of the process. A press names its listener in the
//! signature of the hot key (`macos_hotkeys::signature`), and the handler looks
//! the sink up. A listener that is gone has no sink, and a late press for it is
//! dropped.

use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Mutex, OnceLock};

use super::ffi::{self, EventHotKeyId, EventTypeSpec, MutRef};
use crate::hotkey::{Hotkey, HotkeyId, HotkeyRegistration, HotkeyUnavailable};
use crate::macos_hotkeys as rules;

/// Who hears the presses of which listener: its serial, and where they go.
static SINKS: Mutex<Vec<(u16, Sender<HotkeyId>)>> = Mutex::new(Vec::new());

/// The serial of the next listener. A `u16` is the room the signature leaves,
/// and a listener 65 536 starts older than the one being made is not alive.
static NEXT_SERIAL: AtomicU16 = AtomicU16::new(1);

/// The system's answer to installing the one handler, kept so every `listen`
/// after a failed one says so again rather than listening to nothing.
static INSTALLED: OnceLock<i32> = OnceLock::new();

/// Hears every hot key press of the process and passes on those of listeners.
///
/// Runs on the thread that runs the loop of the application. It does no more
/// than look the sink up and send, and returns `eventNotHandledErr` for a press
/// that is not ours so the `Escape` reservation (`escape.rs`) hears its own.
extern "C" fn on_hot_key(_next: MutRef, event: MutRef, _user: MutRef) -> i32 {
    let Some(id) = ffi::hot_key_id(event) else {
        return ffi::EVENT_NOT_HANDLED;
    };
    let Some(serial) = rules::serial_of(id.signature) else {
        return ffi::EVENT_NOT_HANDLED;
    };
    // A poisoned lock means another thread panicked holding it. The press is
    // dropped, which is better than a panic across a call from the system.
    if let Ok(sinks) = SINKS.lock()
        && let Some((_, sink)) = sinks.iter().find(|(known, _)| *known == serial)
    {
        let _ = sink.send(HotkeyId(id.id));
    }
    ffi::NO_ERR
}

fn install_handler() -> i32 {
    *INSTALLED.get_or_init(|| {
        let spec = EventTypeSpec {
            class: ffi::CLASS_KEYBOARD,
            kind: ffi::HOT_KEY_PRESSED,
        };
        let mut handler: MutRef = core::ptr::null_mut();
        unsafe {
            ffi::InstallEventHandler(
                ffi::GetApplicationEventTarget(),
                on_hot_key,
                1,
                &spec,
                core::ptr::null_mut(),
                &mut handler,
            )
        }
    })
}

/// A registered hot key, to give back.
struct HotKey(MutRef);

// The reference is an opaque token that is handed back to the system to
// unregister, and nothing is read through it. `a_shortcut_registered_on_one_thread_is_released_on_another`
// (`hotkey.rs`) measures that the system takes it back from another thread.
unsafe impl Send for HotKey {}

/// What keeps the shortcuts registered. Dropping it releases them all.
pub struct Listener {
    serial: u16,
    keys: Vec<HotKey>,
}

impl Listener {
    pub fn stop(mut self) {
        self.release();
    }

    fn release(&mut self) {
        for key in self.keys.drain(..) {
            unsafe { ffi::UnregisterEventHotKey(key.0) };
        }
        if let Ok(mut sinks) = SINKS.lock() {
            sinks.retain(|(known, _)| *known != self.serial);
        }
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        self.release();
    }
}

fn register_one(serial: u16, hotkey: &Hotkey) -> (HotkeyRegistration, Option<HotKey>) {
    let Some(code) = rules::key_code(hotkey.vk) else {
        return (
            HotkeyRegistration::Failed {
                code: rules::NO_SUCH_KEY,
            },
            None,
        );
    };
    let id = EventHotKeyId {
        signature: rules::signature(serial),
        id: hotkey.id.0,
    };
    let mut registered: MutRef = core::ptr::null_mut();
    let status = unsafe {
        ffi::RegisterEventHotKey(
            code,
            rules::modifiers(hotkey.ctrl, hotkey.alt, hotkey.shift, hotkey.win),
            id,
            ffi::GetApplicationEventTarget(),
            0,
            &mut registered,
        )
    };
    let outcome = rules::registration(status);
    // `HotKey` has no `Drop`, so building it before the condition is read costs
    // nothing (unlike `Owned` in `ffi.rs`, whose release of null is a trap).
    let key = (outcome == HotkeyRegistration::Registered && !registered.is_null())
        .then_some(HotKey(registered));
    (outcome, key)
}

/// What listening gives: one outcome per hotkey, the presses, and the listener.
type Started = (Vec<HotkeyRegistration>, Receiver<HotkeyId>, Listener);

pub fn listen(hotkeys: &[Hotkey]) -> Result<Started, HotkeyUnavailable> {
    if install_handler() != ffi::NO_ERR {
        return Err(HotkeyUnavailable::CouldNotStart);
    }
    let serial = NEXT_SERIAL.fetch_add(1, Ordering::Relaxed);
    let (sender, receiver) = mpsc::channel::<HotkeyId>();
    // The sink first, so a press the moment a hot key is registered finds it.
    SINKS
        .lock()
        .map_err(|_| HotkeyUnavailable::CouldNotStart)?
        .push((serial, sender));
    let mut listener = Listener {
        serial,
        keys: Vec::new(),
    };
    let mut outcomes = Vec::with_capacity(hotkeys.len());
    for hotkey in hotkeys {
        let (outcome, key) = register_one(serial, hotkey);
        outcomes.push(outcome);
        listener.keys.extend(key);
    }
    Ok((outcomes, receiver, listener))
}
