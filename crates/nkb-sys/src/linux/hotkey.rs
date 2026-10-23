//! Global shortcuts on X11: a grab of each chord on the root window, held by a
//! connection of its own, and a thread that hears the presses.
//!
//! # Why a connection and a thread of its own
//!
//! A press of a grabbed chord is an event the server sends to the client that holds
//! the grab, and only when that client reads its connection. The palette's main
//! thread belongs to the window toolkit, so the shortcuts live on a thread that does
//! nothing else, the same arrangement as on Windows (`hotkey.rs`). The send of a value
//! has a connection of its own too (`send.rs`), so a long send never makes a press wait
//! and a press never makes a send wait. The grabs are gone with the connection even
//! if the program is killed in the middle, which `Escape` during a send also relies on
//! (`escape.rs`).
//!
//! # What the decisions rest on
//!
//! `linux_hotkeys.rs` has the measurements and the rules, which need no server. Here
//! are the calls, and three things that only a server can say: whether the grab is
//! taken (`BadAccess`, measured in `OBS-200` point 4), whether the keyboard changed
//! under a grab (`MappingNotify`), and whether the thread is to stop.
//!
//! # Stopping
//!
//! The thread waits for an event and a blocking wait cannot be interrupted, so it is
//! asked to stop with an event: a `ClientMessage` of ours to a window that only this
//! listener has. A message sent with no event mask goes to the client that made the
//! window, which is the thread's own connection.
//!
//! # A change of the keyboard under a grab
//!
//! A grab is on a KEY, and which key types `n` is the layout's to say. When the layout
//! changes the server sends `MappingNotify`, and the shortcuts that now belong on
//! another key are moved. A shortcut that has no key on the new layout is let go and
//! comes back when it has one. There is nowhere to say that to the person, since the
//! outcomes were given when the listener started (`hotkey.rs`).

use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::JoinHandle;

use x11rb::COPY_FROM_PARENT;
use x11rb::connection::Connection;
use x11rb::errors::ReplyError;
use x11rb::protocol::Event;
use x11rb::protocol::xproto::{
    AtomEnum, ClientMessageEvent, ConnectionExt as _, CreateWindowAux, EventMask, GrabMode,
    Mapping, ModMask, WindowClass,
};
use x11rb::rust_connection::RustConnection;

use super::Lost;
use super::connect::{Server, connect, environment_route};
use super::send::{Keyboard, read_keyboard};
use crate::hotkey::{Hotkey, HotkeyId, HotkeyRegistration, HotkeyUnavailable};
use crate::linux_hotkeys::{self as rules, Binding, ModifierBits};
use crate::linux_rules::Refusal;

/// What listening gives: one outcome per hotkey, the presses, and the listener.
type Started = (Vec<HotkeyRegistration>, Receiver<HotkeyId>, Listener);

/// How many four byte units of the `_XKB_RULES_NAMES` property are read. The five
/// strings are a few hundred bytes at the longest.
const OPTIONS_PROPERTY_UNITS: u32 = 256;

fn unsupported(refusal: Refusal) -> HotkeyUnavailable {
    HotkeyUnavailable::Unsupported {
        system: refusal.system(),
    }
}

/// The options of the keyboard layout the server was started with, such as
/// `grp:alt_shift_toggle`. `None` when it keeps none, which is the case of a server
/// that was never given a layout by `setxkbmap` or by a session.
fn layout_options(conn: &RustConnection, root: u32) -> Option<String> {
    let atom = conn
        .intern_atom(true, b"_XKB_RULES_NAMES")
        .ok()?
        .reply()
        .ok()?
        .atom;
    if atom == u32::from(AtomEnum::NONE) {
        return None;
    }
    let property = conn
        .get_property(
            false,
            root,
            atom,
            AtomEnum::STRING,
            0,
            OPTIONS_PROPERTY_UNITS,
        )
        .ok()?
        .reply()
        .ok()?;
    rules::xkb_options(&property.value)
}

/// What a shortcut needs from the server before the first grab.
struct Prepared {
    keyboard: Keyboard,
    options: Option<String>,
    /// A window nobody sees, which the stop message is sent to.
    window: u32,
    /// The type of that message.
    stop: u32,
}

fn prepare(conn: &RustConnection, root: u32) -> Result<Prepared, Lost> {
    let keyboard = read_keyboard(conn)?;
    let window = conn.generate_id()?;
    conn.create_window(
        0,
        window,
        root,
        0,
        0,
        1,
        1,
        0,
        WindowClass::INPUT_ONLY,
        COPY_FROM_PARENT,
        &CreateWindowAux::new(),
    )?
    .check()?;
    let stop = conn.intern_atom(false, b"NKB_HOTKEY_STOP")?.reply()?.atom;
    Ok(Prepared {
        keyboard,
        options: layout_options(conn, root),
        window,
        stop,
    })
}

/// Gives back what a grab took, in every combination it was taken in.
fn ungrab(conn: &RustConnection, root: u32, binding: &Binding, variants: &[u16]) {
    for locks in variants {
        let _ = conn.ungrab_key(
            binding.keycode,
            root,
            ModMask::from(binding.modifiers | locks),
        );
    }
    let _ = conn.flush();
}

/// Takes `binding` in every combination of the locks. The answer is `Taken` when
/// another client holds any of them, and then none of ours is kept, so half a
/// shortcut never stays behind.
fn grab(
    conn: &RustConnection,
    root: u32,
    binding: &Binding,
    variants: &[u16],
) -> Result<HotkeyRegistration, Lost> {
    for (done, locks) in variants.iter().enumerate() {
        let answer = conn
            .grab_key(
                false,
                root,
                ModMask::from(binding.modifiers | locks),
                binding.keycode,
                GrabMode::ASYNC,
                GrabMode::ASYNC,
            )?
            .check();
        match answer {
            Ok(()) => {}
            Err(ReplyError::X11Error(error)) => {
                ungrab(conn, root, binding, &variants[..done]);
                return Ok(rules::registration_of(error.error_code));
            }
            Err(ReplyError::ConnectionError(_)) => return Err(Lost),
        }
    }
    Ok(HotkeyRegistration::Registered)
}

/// A shortcut that is held, with the key it is held on right now.
struct Shortcut {
    hotkey: Hotkey,
    binding: Option<Binding>,
}

/// The shortcuts that were taken, and what they were taken with.
struct Held {
    shortcuts: Vec<Shortcut>,
    variants: Vec<u16>,
    locks: u16,
    /// The bindings of `shortcuts`, kept for the lookup of a press.
    bindings: Vec<Binding>,
}

impl Held {
    fn rebuild(&mut self) {
        self.bindings = self
            .shortcuts
            .iter()
            .filter_map(|shortcut| shortcut.binding)
            .collect();
    }

    /// Moves the shortcuts whose key is another one now, after the server said the
    /// keyboard changed. What did not move is not touched, so a press is never lost
    /// to a change that was not about it. The send of a value changes the map too, on
    /// keys that are no shortcut's, and costs one read of the map here and no grab.
    fn refresh(&mut self, conn: &RustConnection, root: u32) {
        let Ok(keyboard) = read_keyboard(conn) else {
            return;
        };
        let bits = ModifierBits::read(
            &keyboard.modifier_keys,
            keyboard.keys_per_modifier,
            &keyboard.keymap,
        );
        let variants = rules::lock_variants(&bits);
        let everything = variants != self.variants;
        let moved: Vec<(usize, Option<Binding>)> = self
            .shortcuts
            .iter()
            .enumerate()
            .map(|(index, shortcut)| {
                (
                    index,
                    rules::resolve(&shortcut.hotkey, &keyboard.keymap, &bits).ok(),
                )
            })
            .filter(|(index, new)| everything || *new != self.shortcuts[*index].binding)
            .collect();
        // Everything that moves is let go before anything is taken, so two shortcuts
        // that swap keys do not refuse each other.
        for (index, _) in &moved {
            if let Some(old) = self.shortcuts[*index].binding {
                ungrab(conn, root, &old, &self.variants);
            }
        }
        for (index, new) in moved {
            self.shortcuts[index].binding = new.filter(|binding| {
                matches!(
                    grab(conn, root, binding, &variants),
                    Ok(HotkeyRegistration::Registered)
                )
            });
        }
        self.variants = variants;
        self.locks = bits.locks();
        self.rebuild();
        let _ = conn.flush();
    }

    fn release(&self, conn: &RustConnection, root: u32) {
        for shortcut in &self.shortcuts {
            if let Some(binding) = &shortcut.binding {
                ungrab(conn, root, binding, &self.variants);
            }
        }
    }
}

/// What shortcuts are taken with: the connection, and the keyboard as it is now.
struct Taking<'a> {
    conn: &'a RustConnection,
    root: u32,
    prepared: &'a Prepared,
    bits: ModifierBits,
    variants: Vec<u16>,
}

impl Taking<'_> {
    /// The outcome of one hotkey, and its key when it was taken. `held` are the
    /// shortcuts this listener has taken so far.
    fn register(
        &self,
        hotkey: &Hotkey,
        held: &[Binding],
    ) -> Result<(HotkeyRegistration, Option<Binding>), Lost> {
        // A server that takes a modifier of the chord for a change of layout never
        // sends the chord to a grab, and the registration would be accepted all the
        // same.
        if let Some(options) = &self.prepared.options
            && rules::layout_toggle(options, hotkey).is_some()
        {
            return Ok((
                HotkeyRegistration::Failed {
                    code: rules::LAYOUT_TOGGLE,
                },
                None,
            ));
        }
        let binding = match rules::resolve(hotkey, &self.prepared.keyboard.keymap, &self.bits) {
            Ok(binding) => binding,
            Err(code) => return Ok((HotkeyRegistration::Failed { code }, None)),
        };
        if rules::held_already(held, &binding) {
            return Ok((HotkeyRegistration::Taken, None));
        }
        let outcome = grab(self.conn, self.root, &binding, &self.variants)?;
        Ok((
            outcome,
            (outcome == HotkeyRegistration::Registered).then_some(binding),
        ))
    }
}

fn register_all(
    conn: &RustConnection,
    root: u32,
    hotkeys: &[Hotkey],
    prepared: &Prepared,
) -> Result<(Vec<HotkeyRegistration>, Held), Lost> {
    let bits = ModifierBits::read(
        &prepared.keyboard.modifier_keys,
        prepared.keyboard.keys_per_modifier,
        &prepared.keyboard.keymap,
    );
    let taking = Taking {
        conn,
        root,
        prepared,
        bits,
        variants: rules::lock_variants(&bits),
    };
    let mut outcomes = Vec::with_capacity(hotkeys.len());
    let mut shortcuts: Vec<Shortcut> = Vec::new();
    for hotkey in hotkeys {
        let held: Vec<Binding> = shortcuts
            .iter()
            .filter_map(|shortcut| shortcut.binding)
            .collect();
        let (outcome, binding) = taking.register(hotkey, &held)?;
        outcomes.push(outcome);
        if binding.is_some() {
            shortcuts.push(Shortcut {
                hotkey: *hotkey,
                binding,
            });
        }
    }
    conn.flush()?;
    let mut held = Held {
        shortcuts,
        variants: taking.variants,
        locks: bits.locks(),
        bindings: Vec::new(),
    };
    held.rebuild();
    Ok((outcomes, held))
}

/// Hears the presses until the stop message, and reports each shortcut by its id.
fn pump(
    conn: &RustConnection,
    root: u32,
    prepared: &Prepared,
    mut held: Held,
    fired: &Sender<HotkeyId>,
) {
    let mut repeats = rules::Repeats::default();
    while let Ok(event) = conn.wait_for_event() {
        match event {
            Event::KeyPress(press) => {
                if repeats.is_repeat(press.detail, press.time) {
                    continue;
                }
                if let Some(id) = rules::matching(
                    &held.bindings,
                    held.locks,
                    press.detail,
                    u16::from(press.state),
                ) {
                    // A send that fails means the receiver was dropped while the
                    // listener is alive. The shortcuts stay held and the press is
                    // discarded, as on Windows.
                    let _ = fired.send(id);
                }
            }
            Event::KeyRelease(release) => repeats.released(release.detail, release.time),
            Event::MappingNotify(change)
                if matches!(change.request, Mapping::KEYBOARD | Mapping::MODIFIER) =>
            {
                held.refresh(conn, root);
            }
            Event::ClientMessage(message)
                if message.window == prepared.window && message.type_ == prepared.stop =>
            {
                break;
            }
            _ => {}
        }
    }
    held.release(conn, root);
}

/// The thread that holds the shortcuts, and what it takes to ask it to stop.
struct Running {
    conn: Arc<RustConnection>,
    window: u32,
    stop: u32,
    handle: JoinHandle<()>,
}

/// What keeps the shortcuts held. Dropping it stops the thread and lets them go.
pub struct Listener {
    running: Option<Running>,
}

impl Listener {
    pub fn stop(mut self) {
        self.shut_down();
    }

    fn shut_down(&mut self) {
        let Some(running) = self.running.take() else {
            return;
        };
        // A thread that has ended has nobody to hear the message, and one that has
        // not will hear it from its blocking wait.
        if !running.handle.is_finished() {
            let message = ClientMessageEvent::new(32, running.window, running.stop, [0u32; 5]);
            let _ = running
                .conn
                .send_event(false, running.window, EventMask::NO_EVENT, message);
            let _ = running.conn.flush();
        }
        let _ = running.handle.join();
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        self.shut_down();
    }
}

pub(crate) fn listen(hotkeys: &[Hotkey]) -> Result<Started, HotkeyUnavailable> {
    let address = environment_route().map_err(unsupported)?;
    let Server { conn, root } = connect(address).map_err(unsupported)?;
    let conn = Arc::new(conn);
    let could_not = |_: Lost| HotkeyUnavailable::CouldNotStart;
    let prepared = prepare(&conn, root).map_err(could_not)?;
    let (outcomes, held) = register_all(&conn, root, hotkeys, &prepared).map_err(could_not)?;
    let (fired_tx, fired_rx) = mpsc::channel::<HotkeyId>();
    if held.shortcuts.is_empty() {
        // Nothing was taken, so there is nothing to hear. The channel is closed, the
        // way a Windows listener that registered nothing leaves it.
        drop(fired_tx);
        return Ok((outcomes, fired_rx, Listener { running: None }));
    }

    let thread_conn = Arc::clone(&conn);
    let (window, stop) = (prepared.window, prepared.stop);
    // Builder rather than `thread::spawn`: a refusal of the system is reported, not
    // a panic of product code.
    let handle = std::thread::Builder::new()
        .name("nkb-hotkey".to_owned())
        .spawn(move || pump(&thread_conn, root, &prepared, held, &fired_tx))
        .map_err(|_| HotkeyUnavailable::CouldNotStart)?;
    Ok((
        outcomes,
        fired_rx,
        Listener {
            running: Some(Running {
                conn,
                window,
                stop,
                handle,
            }),
        },
    ))
}
