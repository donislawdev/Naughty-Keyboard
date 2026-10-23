//! Typing on X11: the keys of the layout for the characters it types, spare keys
//! rewritten to the rest, batches, and the answer of the application after every
//! batch (`D145`, `OBS-202`, `OBS-212`).
//!
//! The order of one send, and the measurement behind each step:
//!
//! 1. the characters become keysyms, and the send is refused by name if one has
//!    none (`U+0000`), before anything is touched,
//! 2. the connection is made to the LOCAL socket, or the send is refused (Wayland,
//!    no server, no `XTEST`),
//! 3. a held modifier is waited for, and Caps Lock on is refused: GTK turns the
//!    letters to capitals under it whatever key typed them (point 8). The state
//!    then read decides whether the keys of the layout may type the characters
//!    (`LayoutKeys::typing`), and a send with a character they cannot type is
//!    refused if the server has no spare key,
//! 4. when a spare key is needed, one tap of an unmapped key makes the server
//!    announce its map. The first XTest key event of a connection does, and a
//!    remap made before it is gone (point 1). A value the layout types alone gets
//!    no tap and no remap,
//! 5. a batch is: rewrite the spare keys, press and release the keys (Shift held
//!    around those that need it), and ask the window at the keyboard focus to
//!    answer `_NET_WM_PING`. The answer comes from the application's own loop after
//!    the key events before it, so it means they were taken (point 3). The spare
//!    keys are rewritten again only after it,
//! 6. the spare keys are given back to the layout, and Shift is let go, on every
//!    way out of the send.
//!
//! What the answer does not say is whether the application put the character in
//! the field, and that is measured with a victim that reports its field
//! (`tools/petla-ofiary-linux.py`).

use core::time::Duration;
use std::time::Instant;

use x11rb::connection::Connection;
use x11rb::protocol::Event;
use x11rb::protocol::xproto::{
    ChangeWindowAttributesAux, ClientMessageData, ClientMessageEvent, ConnectionExt as _,
    EventMask, KEY_PRESS_EVENT, KEY_RELEASE_EVENT,
};
use x11rb::protocol::xtest::ConnectionExt as _;
use x11rb::rust_connection::RustConnection;

use super::Lost;
use super::connect::{Server, connect, environment_route, route_refusal};
use super::escape::{ReservedEscape, XEscape};
use super::front::{PingAtoms, focus, focused_toplevel, ping_atoms, ping_target};
use crate::cadence::{Cadence, PROGRESS_INTERVAL};
use crate::linux_rules::{
    Batch, FIRST_BATCH_CHARS, KEYSYM_ESCAPE, Keymap, LOOK_EVERY, LayoutKey, LayoutKeys,
    PING_ANSWER_MASK, PING_WAIT, REMAP_SETTLE, Refusal, UNPACED_SETTLE, WARMUP_SETTLE,
    batch_of_value, caps_lock_on, give_back_wait, held_modifier, keysym_for, names_an_input_method,
    next_batch, next_limit, ping_token, slots_for_value, utf16_units,
};
use crate::{Chord, MODIFIER_RELEASE_WAIT, SendError, SendOutcome, SendProgress, StopReason};

/// What this system is called when a route cannot be named more closely.
const SYSTEM: &str = "Linux";

/// Whether this program may type here: a session that is not Wayland, a local
/// `DISPLAY`, and a server that has what is needed (`route_refusal`).
pub(crate) fn send_allowed() -> bool {
    route_refusal().is_none()
}

/// Clearing needs a field that is KNOWN to be one (`D76`), and the kind of field
/// is read through accessibility, which this route does not have yet.
pub(crate) fn send_chords(chords: &[Chord]) -> Result<usize, SendError> {
    if chords.is_empty() {
        // Nothing to press, so nothing to be unable to press.
        return Ok(0);
    }
    Err(SendError::Unsupported { system: SYSTEM })
}

fn unsupported(refusal: Refusal) -> SendError {
    SendError::Unsupported {
        system: refusal.system(),
    }
}

fn truncated(units_sent: usize, units_expected: usize, reason: StopReason) -> SendError {
    SendError::Truncated {
        units_sent,
        units_expected,
        reason,
    }
}

/// One key event to the server, which does not say whether anyone took it.
fn fake(conn: &RustConnection, root: u32, press: bool, keycode: u8) -> Result<(), Lost> {
    let kind = if press {
        KEY_PRESS_EVENT
    } else {
        KEY_RELEASE_EVENT
    };
    conn.xtest_fake_input(kind, keycode, 0, root, 0, 0, 0)?;
    Ok(())
}

/// Waits until the server has handled everything sent to it so far.
fn roundtrip(conn: &RustConnection) -> Result<(), Lost> {
    conn.get_input_focus()?.reply()?;
    Ok(())
}

/// The spare keys that were rewritten, given back to the layout when this is
/// dropped - on every way out of the send, a panic included.
struct Remapped<'a> {
    conn: &'a RustConnection,
    spare: &'a [u8],
    used: usize,
    /// How long the keys must stay as they are, because keys were sent through
    /// them and the application may not have read them yet (`give_back_wait`).
    owed: Duration,
}

impl Remapped<'_> {
    /// Makes the `slot`-th spare key type `keysym`, on both levels: a keysym alone
    /// on a key is changed to lower case by the server, and one copy makes the key
    /// a letter that Caps Lock acts on (`OBS-202` point 7).
    fn assign(&mut self, slot: usize, keysym: u32) -> Result<(), Lost> {
        self.pay();
        self.used = self.used.max(slot + 1);
        self.conn
            .change_keyboard_mapping(1, self.spare[slot], 2, &[keysym, keysym])?;
        Ok(())
    }
}

impl Remapped<'_> {
    /// Notes that keys were sent and answered, so the layout is kept for `wait`
    /// before the next rewrite or the give-back. Zero when nothing was sent that
    /// the application could still read.
    fn owe(&mut self, wait: Duration) {
        self.owed = wait;
    }

    /// Waits out what is owed, once.
    fn pay(&mut self) {
        if !self.owed.is_zero() {
            std::thread::sleep(self.owed);
            self.owed = Duration::ZERO;
        }
    }
}

impl Drop for Remapped<'_> {
    fn drop(&mut self) {
        self.pay();
        for keycode in &self.spare[..self.used] {
            let _ = self.conn.change_keyboard_mapping(1, *keycode, 1, &[0]);
        }
        if let Ok(cookie) = self.conn.get_input_focus() {
            let _ = cookie.reply();
        }
    }
}

/// The window that is asked whether it has taken the keys, and the atoms of the
/// question.
#[derive(Clone, Copy)]
struct Pinged {
    atoms: PingAtoms,
    window: u32,
}

/// What the application did with a ping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Answer {
    /// It answered, this long after it was asked.
    Taken(Duration),
    /// It does not take part in the protocol, so it was given time instead.
    Unpaced,
    /// No answer in time. The spare keys of the batch were sent, but the layout is
    /// given back before the application runs again, and a key it reads then types
    /// nothing (`OBS-206`): the batch is counted as not arrived, and what the
    /// application took before it stopped is not known. The keys of the layout
    /// are not given back and are typed when the application runs again, which is
    /// why a send begins with `confirm_alive`: a program found stopped after those
    /// keys went would be said to have taken nothing while it holds them.
    TimedOut,
}

/// Asks `pinged.window` whether it has got through everything the server queued for
/// it before this message, and waits for the answer. Events reach a client in the
/// order the server queued them, and a toolkit answers from its event loop.
fn ping(server: &Server, pinged: Pinged, token: u32) -> Result<Answer, Lost> {
    let message = ClientMessageEvent::new(
        32,
        pinged.window,
        pinged.atoms.protocols,
        ClientMessageData::from([pinged.atoms.ping, token, pinged.window, 0, 0]),
    );
    server
        .conn
        .send_event(false, pinged.window, EventMask::NO_EVENT, message)?;
    server.conn.flush()?;
    let started = Instant::now();
    while started.elapsed() < PING_WAIT {
        while let Some(event) = server.conn.poll_for_event()? {
            if let Event::ClientMessage(reply) = event {
                let data = reply.data.as_data32();
                if reply.type_ == pinged.atoms.protocols
                    && data[0] == pinged.atoms.ping
                    && data[1] == token
                    && data[2] == pinged.window
                {
                    return Ok(Answer::Taken(started.elapsed()));
                }
            }
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    Ok(Answer::TimedOut)
}

/// The window that answers pings, if the one at the focus takes part in the
/// protocol. The answers go to the root window, so the root is listened to, on the
/// masks that GTK, winit and Qt send their answers with (`PING_ANSWER_MASK`).
fn pinged_window(server: &Server) -> Result<Option<Pinged>, Lost> {
    let Some(focused) = focus(&server.conn, server.root)? else {
        return Ok(None);
    };
    let atoms = ping_atoms(&server.conn)?;
    let Some(window) = ping_target(&server.conn, server.root, focused, atoms)? else {
        return Ok(None);
    };
    let listening = server
        .conn
        .change_window_attributes(
            server.root,
            &ChangeWindowAttributesAux::new().event_mask(EventMask::from(PING_ANSWER_MASK)),
        )?
        .check();
    Ok(listening.is_ok().then_some(Pinged { atoms, window }))
}

/// What the server says about the keyboard before anything is sent. A shortcut asks
/// the same questions (`hotkey.rs`), so the fields it reads are not private.
pub(super) struct Keyboard {
    pub(super) keymap: Keymap,
    pub(super) modifier_keys: Vec<u8>,
    pub(super) keys_per_modifier: usize,
    /// The key the server counts as Shift: the first of the first modifier.
    shift_key: Option<u8>,
}

pub(super) fn read_keyboard(conn: &RustConnection) -> Result<Keyboard, Lost> {
    let setup = conn.setup();
    let (min, max) = (setup.min_keycode, setup.max_keycode);
    let mapping = conn.get_keyboard_mapping(min, max - min + 1)?.reply()?;
    let modifiers = conn.get_modifier_mapping()?.reply()?;
    let keys_per_modifier = usize::from(modifiers.keycodes_per_modifier());
    let shift_key = modifiers
        .keycodes
        .iter()
        .take(keys_per_modifier)
        .copied()
        .find(|keycode| *keycode != 0);
    Ok(Keyboard {
        keymap: Keymap::new(
            min,
            usize::from(mapping.keysyms_per_keycode),
            mapping.keysyms,
        ),
        modifier_keys: modifiers.keycodes,
        keys_per_modifier,
        shift_key,
    })
}

/// The modifier that is down right now, if one is.
fn modifier_down(server: &Server, keyboard: &Keyboard) -> Result<Option<&'static str>, Lost> {
    let keys = server.conn.query_keymap()?.reply()?.keys;
    Ok(held_modifier(
        &keys,
        &keyboard.modifier_keys,
        keyboard.keys_per_modifier,
    ))
}

/// Waits for every modifier to come up, for at most `MODIFIER_RELEASE_WAIT`, and
/// then refuses Caps Lock. The modifier is the ordinary case right after a hotkey.
/// Caps Lock is a state somebody left on, and waiting for it would be waiting for
/// them. Gives the modifier state the server reports, with the layout group in it.
fn wait_for_keys_released(server: &Server, keyboard: &Keyboard) -> Result<u16, SendError> {
    let lost = || unsupported(Refusal::NoLocalServer);
    let deadline = Instant::now() + MODIFIER_RELEASE_WAIT;
    while let Some(key) = modifier_down(server, keyboard).map_err(|_| lost())? {
        if Instant::now() >= deadline {
            return Err(SendError::ModifierHeld { key });
        }
        std::thread::sleep(LOOK_EVERY);
    }
    let state = u16::from(
        server
            .conn
            .query_pointer(server.root)
            .map_err(|_| lost())?
            .reply()
            .map_err(|_| lost())?
            .mask,
    );
    if caps_lock_on(state) {
        return Err(SendError::CapsLockOn);
    }
    Ok(state)
}

/// Why the send must stop before the next batch, if it must: `Escape` first, as on
/// every system, then the window in front.
fn stop_before_next(
    server: &Server,
    front: u32,
    escape: &mut ReservedEscape,
) -> Option<StopReason> {
    if escape.pressed() {
        return Some(StopReason::Escape);
    }
    match focused_toplevel(&server.conn, server.root) {
        Ok(Some(window)) if window == front => None,
        Ok(_) => Some(StopReason::FocusMoved),
        Err(Lost) => Some(StopReason::Dropped),
    }
}

/// The keysyms of `characters`, or the first that has none.
fn keysyms_of(characters: &[char]) -> Result<Vec<u32>, SendError> {
    characters
        .iter()
        .map(|character| {
            keysym_for(*character).map_err(|refused| SendError::CannotCarry {
                character: refused.0,
            })
        })
        .collect()
}

/// The spare keys rewritten to `keysyms`, and the application given time to read
/// the new map before a key is pressed.
fn assign_all(server: &Server, remapped: &mut Remapped<'_>, keysyms: &[u32]) -> Result<(), Lost> {
    for (slot, keysym) in keysyms.iter().enumerate() {
        remapped.assign(slot, *keysym)?;
    }
    roundtrip(&server.conn)?;
    std::thread::sleep(REMAP_SETTLE);
    Ok(())
}

/// One key of the layout pressed and released, with Shift held around it when the
/// character is on the second level. Shift is let go whether or not the key went: a
/// Shift left down would change everything the person types next.
fn tap_layout_key(server: &Server, shift_key: Option<u8>, key: LayoutKey) -> Result<(), Lost> {
    let shift = if key.shift {
        let shift = shift_key.ok_or(Lost)?;
        fake(&server.conn, server.root, true, shift)?;
        Some(shift)
    } else {
        None
    };
    let typed = fake(&server.conn, server.root, true, key.keycode)
        .and_then(|()| fake(&server.conn, server.root, false, key.keycode));
    let released = shift.map_or(Ok(()), |shift| {
        fake(&server.conn, server.root, false, shift)
    });
    typed.and(released)
}

/// One batch on the wire: the keys pressed and released, after the spare keys were
/// rewritten when `rewrite` says they were not assigned for the whole value. Tells
/// whether a spare key was pressed, because only then is there a layout to keep for
/// an application that has not read the keys yet.
fn type_batch(
    server: &Server,
    remapped: &mut Remapped<'_>,
    batch: &Batch,
    keysyms: &[u32],
    layout: &LayoutKeys,
    rewrite: bool,
) -> Result<bool, Lost> {
    if rewrite && !batch.keysyms.is_empty() {
        assign_all(server, remapped, &batch.keysyms)?;
    }
    let mut used_spare = false;
    for keysym in &keysyms[batch.first..batch.end] {
        if let Some(key) = layout.key_for(*keysym) {
            tap_layout_key(server, layout.shift_key(), key)?;
            continue;
        }
        let keycode = remapped.spare[batch.slot(*keysym).ok_or(Lost)?];
        fake(&server.conn, server.root, true, keycode)?;
        fake(&server.conn, server.root, false, keycode)?;
        used_spare = true;
    }
    server.conn.flush()?;
    Ok(used_spare)
}

/// The tap that makes the server announce its map, before any key is rewritten.
fn warm_up(server: &Server, keycode: u8) -> Result<(), Lost> {
    fake(&server.conn, server.root, true, keycode)?;
    fake(&server.conn, server.root, false, keycode)?;
    roundtrip(&server.conn)?;
    std::thread::sleep(WARMUP_SETTLE);
    Ok(())
}

/// Asks the window once, before any key goes, whether the application is reading at
/// all. A key of the layout stays with the application and is typed whenever it runs
/// again, so a stopped program found out only after the keys went would be reported
/// as having taken nothing while the field fills up later (measured with `SIGSTOP`
/// on the first batch, `OBS-212`). Found out here, nothing was sent and that is what
/// the person is told.
fn confirm_alive(server: &Server, pinged: Pinged, expected: usize) -> Result<(), SendError> {
    let token = ping_token(std::process::id(), u32::MAX);
    match ping(server, pinged, token) {
        Ok(Answer::TimedOut) => Err(truncated(0, expected, StopReason::NotTaking)),
        Ok(_) => Ok(()),
        Err(Lost) => Err(truncated(0, expected, StopReason::Dropped)),
    }
}

/// What the batch just answered tells the rest of the send.
struct Kept {
    /// Characters in the batch.
    sent: usize,
    /// Whether a spare key was pressed in it: only then is there a layout to keep,
    /// because the keys of the layout are never rewritten.
    used_spare: bool,
    input_method: bool,
}

/// What the answer of the application changes: the length the next batch may have,
/// or the end of the send.
fn take_answer(
    answer: Answer,
    limit: usize,
    kept: &Kept,
    remapped: &mut Remapped<'_>,
) -> Result<usize, StopReason> {
    match answer {
        // An application that does not answer is not reading, and holding the
        // layout for it would only keep the send waiting.
        Answer::TimedOut => Err(StopReason::NotTaking),
        // The next batch is as long as the pace just seen allows (`OBS-204`).
        // The answer says the keys were taken, not that they were read, so the
        // spare keys are kept a while longer (`OBS-210`).
        Answer::Taken(took) => {
            if kept.used_spare {
                remapped.owe(give_back_wait(kept.sent, kept.input_method));
            }
            Ok(next_limit(kept.sent, took))
        }
        // The pause that stands in for the answer was part of the wait already:
        // an application that takes no part in the ping reads its keys as late
        // as any other (xterm through IBus, `OBS-211`).
        Answer::Unpaced => {
            if kept.used_spare {
                remapped.owe(
                    give_back_wait(kept.sent, kept.input_method).saturating_sub(UNPACED_SETTLE),
                );
            }
            Ok(limit)
        }
    }
}

pub(crate) fn send_text(
    text: &str,
    progress: &mut dyn FnMut(SendProgress),
) -> Result<SendOutcome, SendError> {
    let characters: Vec<char> = text.chars().collect();
    if characters.is_empty() {
        // Nothing to send, so nothing to connect to: an empty value is a real
        // catalogue entry.
        return Ok(SendOutcome {
            units: 0,
            events: 0,
            paced: true,
        });
    }
    let keysyms = keysyms_of(&characters)?;
    let address = environment_route().map_err(unsupported)?;
    let server = connect(address).map_err(unsupported)?;
    let lost = |_: Lost| unsupported(Refusal::NoLocalServer);
    let keyboard = read_keyboard(&server.conn).map_err(lost)?;
    let state = wait_for_keys_released(&server, &keyboard)?;
    // What the keys of the layout type depends on the state just read, so the table of
    // them is made after the wait. A value they type alone needs no spare key.
    let layout = LayoutKeys::typing(&keyboard.keymap, keyboard.shift_key, state);
    let spare = keyboard.keymap.spare_keycodes();
    let needs_spare = keysyms
        .iter()
        .any(|keysym| layout.key_for(*keysym).is_none());
    if needs_spare && spare.is_empty() {
        return Err(unsupported(Refusal::NoSpareKeys));
    }

    let expected = utf16_units(&characters);
    let Some(front) = focused_toplevel(&server.conn, server.root).map_err(lost)? else {
        return Err(truncated(0, expected, StopReason::FocusMoved));
    };
    let pinged = pinged_window(&server).map_err(lost)?;
    // Declared before the keys, so the keys are given back first: the wait for
    // `Escape` to come up must not keep the layout rewritten.
    let mut escape = XEscape::reserve(address, keyboard.keymap.find_keycode(KEYSYM_ESCAPE));
    if needs_spare {
        warm_up(&server, spare[0]).map_err(lost)?;
    }
    let mut remapped = Remapped {
        conn: &server.conn,
        spare: &spare,
        used: 0,
        owed: Duration::ZERO,
    };
    // A value that fits the spare keys is assigned once, here, and the keys are not
    // touched again until the send is over. One that does not fit is rewritten
    // between batches, after the wait the last batch is owed.
    let input_method = names_an_input_method(std::env::var("XMODIFIERS").ok().as_deref());
    let plan = slots_for_value(&keysyms, spare.len(), &layout);
    if let Some(slots) = plan.as_ref().filter(|slots| !slots.is_empty()) {
        assign_all(&server, &mut remapped, slots).map_err(lost)?;
    }

    let uses_layout = keysyms
        .iter()
        .any(|keysym| layout.key_for(*keysym).is_some());
    if let (true, Some(pinged)) = (uses_layout, pinged) {
        confirm_alive(&server, pinged, expected)?;
    }

    let mut cadence = Cadence::starting(Instant::now(), PROGRESS_INTERVAL);
    let mut arrived = 0usize;
    let mut first = 0usize;
    let mut limit = FIRST_BATCH_CHARS;
    let mut number = 0u32;
    while first < keysyms.len() {
        if let Some(reason) = stop_before_next(&server, front, &mut escape) {
            return Err(truncated(arrived, expected, reason));
        }
        let batch = match &plan {
            Some(slots) => batch_of_value(slots, first, keysyms.len(), limit),
            None => next_batch(&keysyms, first, spare.len(), limit, &layout),
        };
        let batch_units = utf16_units(&characters[batch.first..batch.end]);
        let Ok(used_spare) = type_batch(
            &server,
            &mut remapped,
            &batch,
            &keysyms,
            &layout,
            plan.is_none(),
        ) else {
            return Err(truncated(arrived, expected, StopReason::Dropped));
        };
        // From here the batch is on the wire, and it counts only once the
        // application has answered: the layout is given back as the send ends, and
        // an application that has not looked yet reads the keys of this batch in
        // the layout it finds then, where they type nothing (`OBS-206`).
        let answer = match pinged {
            Some(pinged) => {
                let token = ping_token(std::process::id(), number);
                ping(&server, pinged, token)
                    .map_err(|_| truncated(arrived, expected, StopReason::Dropped))?
            }
            None => {
                std::thread::sleep(UNPACED_SETTLE);
                Answer::Unpaced
            }
        };
        let kept = Kept {
            sent: batch.end - batch.first,
            used_spare,
            input_method,
        };
        limit = take_answer(answer, limit, &kept, &mut remapped)
            .map_err(|reason| truncated(arrived, expected, reason))?;
        arrived += batch_units;
        first = batch.end;
        number = number.wrapping_add(1);
        if cadence.due(Instant::now()) {
            progress(SendProgress {
                units_arrived: arrived,
                units_total: expected,
            });
        }
    }
    Ok(SendOutcome {
        units: expected,
        events: characters.len() * 2,
        paced: pinged.is_some(),
    })
}
