//! Typing on X11: the keys of the layout for the characters it types, spare keys
//! rewritten to the rest, batches, and the answer of the application after every
//! batch (`D145`, `OBS-202`, `OBS-212`).
//!
//! The order of one send, and the measurement behind each step:
//!
//! 1. the characters become keysyms, and the send is refused by name if one has
//!    none (`U+0000`), before anything is touched,
//! 2. the connection is made to the LOCAL socket, or the send is refused (Wayland,
//!    no server, no `XTEST`). Then the display is locked for the whole send, and the
//!    spare keys a send before this one left rewritten are given back (`D157`,
//!    `lock`): a second send would read the keys of the first as keys of the layout,
//!    and the map is read only after this,
//! 3. a held modifier is waited for, and Caps Lock on is refused: GTK turns the
//!    letters to capitals under it whatever key typed them (point 8). The state
//!    then read decides whether the keys of the layout may type the characters
//!    (`LayoutKeys::typing`), and a send with a character they cannot type is
//!    refused if the server has no spare key. A modifier latched or locked that makes a
//!    key a command (`Ctrl`, `Alt`, `Super`) is then waited for or refused before any key
//!    is pressed: GTK typed nothing of a value under a locked `Alt` while the send said
//!    "sent" (`OBS-231`, `D160`),
//! 4. when a spare key is needed, one tap of an unmapped key makes the server
//!    announce its map. The first XTest key event of a connection does, and a
//!    remap made before it is gone (point 1). A value the layout types alone gets
//!    no tap and no remap,
//! 5. a batch is: rewrite the spare keys, look at the modifiers the person holds and at
//!    the layout group (`hands_off`: `Alt` and Caps Lock stop the send, `Ctrl` and `Super`
//!    are waited for, `Shift`, `AltGr` and a switched group count for a batch with a key of
//!    the layout, R27), press and release the keys (Shift held
//!    around those that need it), and ask the window at the keyboard focus to
//!    answer `_NET_WM_PING`. The answer comes from the application's own loop after
//!    the key events before it, so it means they were taken (point 3). The spare
//!    keys are rewritten again only after it. The palette closing ends the wait for
//!    the answer at once, and `Escape` once the batch has had the time a batch is
//!    cut to (`unanswered`, R20). A batch left without an answer that held a key of
//!    the layout makes the count a floor, because the application types those keys
//!    whenever it reads them (`InFlight`, R18, `D158`),
//! 6. the spare keys are given back to the layout, the ones that still carry what
//!    was written on them and no other, and Shift is let go, on every way out of the
//!    send.
//!
//! What the answer does not say is whether the application put the character in
//! the field, and that is measured with a victim that reports its field
//! (`tools/petla-ofiary-linux.py`).

use core::sync::atomic::{AtomicBool, Ordering};
use core::time::Duration;
use std::time::Instant;

use x11rb::connection::Connection;
use x11rb::protocol::Event;
use x11rb::protocol::xproto::{
    ChangeWindowAttributesAux, ClientMessageData, ClientMessageEvent, ConnectionExt as _,
    EventMask, KEY_PRESS_EVENT, KEY_RELEASE_EVENT,
};
use x11rb::protocol::xtest::ConnectionExt as _;

use super::Lost;
use super::connect::{Server, connect, environment_route};
use super::escape::{ReservedEscape, XEscape};
use super::front::{PingAtoms, focus, focused_toplevel, ping_atoms, ping_target};
use super::lock::{DisplayLock, NotTaken};
use super::stream::Conn;
use super::watch::{
    hands_off, modifier_in_the_way, stop_before_next, wait_for_keys_released, wait_for_modifiers,
};
use crate::cadence::{Cadence, PROGRESS_INTERVAL};
use crate::linux_clearing_rules::{NoKey, acted_after_silence, keys_for, modifier_stop};
use crate::linux_remap_rules::{Rewrite, ours, span};
use crate::linux_rules::{
    Batch, FIRST_BATCH_CHARS, KEYSYM_ESCAPE, Keymap, LOOK_EVERY, LayoutKey, LayoutKeys,
    PING_ANSWER_MASK, REMAP_SETTLE, Refusal, UNPACED_SETTLE, WARMUP_SETTLE, batch_of_value,
    give_back_wait, keysym_for, names_an_input_method, next_batch, next_limit, ping_token,
    slots_for_value, stopped_before_batch, stopped_on_the_wire, unanswered, utf16_units,
};
use crate::{
    Chord, Following, SendError, SendOutcome, SendProgress, StopReason, WindowRef, still_in_front,
};

/// Presses the keys that clear a field, each as its own key of the layout, with the server
/// saying before every chord whether a modifier is in the way (`D145` point 75).
///
/// The order of one clearing, and the reason for each step:
///
/// 1. the layout must have a key for every chord, or nothing is pressed: a recipe that
///    stops after `Shift+End` leaves the line selected,
/// 2. the application is asked once whether it is reading at all, because a key of the
///    layout stays with the application and is taken whenever it runs again: a program
///    found stopped after the keys went would be said to have taken nothing while it
///    holds them,
/// 3. a modifier that is down, latched or locked is waited for. Under `Ctrl`, `Home` is the
///    start of the document and `Shift+End` selects to its end, so the recipe would delete
///    the document. The server sees the keys of the hand and says so, which no portal on a
///    Wayland session does,
/// 4. before each chord `Escape` and the window in front are looked at as for typing, and the
///    server is asked again for a modifier: one that turned up since the last chord stops
///    the clearing where it is, because `Delete` would remove what is left of a line,
/// 5. each chord is answered by the application before the next goes, as a batch of
///    characters is.
///
/// Clearing needs a field that is KNOWN to be one (`D76`), and that is decided before this.
pub(crate) fn send_chords(
    chords: &[Chord],
    target: WindowRef,
    halt: &AtomicBool,
) -> Result<usize, SendError> {
    if chords.is_empty() {
        // Nothing to press, so nothing to connect to.
        return Ok(0);
    }
    let total = chords.len();
    let address = environment_route().map_err(unsupported)?;
    let server = connect(address).map_err(unsupported)?;
    let lost = |_: Lost| unsupported(Refusal::NoLocalServer);
    let keyboard = read_keyboard(&server.conn).map_err(lost)?;
    let keys = keys_for(chords, &keyboard.keymap, keyboard.shift_key)
        .map_err(|NoKey| unsupported(Refusal::NoClearingKeys))?;
    // The window the clearing was meant for (`D148`), and after the wait below every
    // chord looks again (`Recipe::press`), so a window that comes up meanwhile gets nothing.
    let Some(front) = focused_toplevel(&server.conn, server.root)
        .map_err(lost)?
        .filter(|front| still_in_front(target, Some(WindowRef(u64::from(*front)))))
    else {
        return Err(chords_stopped(0, total, StopReason::FocusMoved));
    };
    let pinged = pinged_window(&server).map_err(lost)?;
    // Declared before the keys, so `Escape` is given back when the clearing is over.
    let mut escape = XEscape::reserve(address, keyboard.keymap.find_keycode(KEYSYM_ESCAPE));
    if let Some(pinged) = pinged {
        alive(&server, pinged, &mut escape, halt)
            .map_err(|reason| chords_stopped(0, total, reason))?;
    }
    wait_for_modifiers(&server, &keyboard)?;
    let recipe = Recipe {
        server: &server,
        keyboard: &keyboard,
        front,
        pinged,
        total,
        halt,
    };
    for (number, key) in keys.iter().enumerate() {
        recipe.press(&mut escape, number, *key)?;
    }
    Ok(total)
}

/// What the chords of one clearing are pressed against.
struct Recipe<'a> {
    server: &'a Server,
    keyboard: &'a Keyboard,
    /// The window in front when the clearing began.
    front: u32,
    pinged: Option<Pinged>,
    total: usize,
    /// The caller's flag to stop the clearing (R03).
    halt: &'a AtomicBool,
}

impl Recipe<'_> {
    /// Chord number `number`, once nothing is found in the way, and its answer.
    fn press(
        &self,
        escape: &mut ReservedEscape,
        number: usize,
        key: LayoutKey,
    ) -> Result<(), SendError> {
        let stopped = |reason| chords_stopped(number, self.total, reason);
        if let Some(reason) = stop_before_next(self.server, self.front, escape, self.halt) {
            return Err(stopped(reason));
        }
        match modifier_in_the_way(self.server, self.keyboard) {
            Ok(Some(name)) => return Err(modifier_stop(name, number, self.total)),
            Ok(None) => {}
            Err(Lost) => return Err(stopped(StopReason::Dropped)),
        }
        let sent = tap_layout_key(self.server, self.keyboard.shift_key, key)
            .and_then(|()| self.server.conn.flush().map_err(Lost::from));
        if sent.is_err() {
            return Err(stopped(StopReason::Dropped));
        }
        // From here the keys are on the wire. An application that gives no answer holds
        // them and takes them when it runs again, so the chord counts as having acted.
        self.answered(escape, number)
            .map_err(|reason| chords_stopped(acted_after_silence(number), self.total, reason))
    }

    /// The application's answer to the keys just sent: a ping, or a pause for one that
    /// does not take part in the protocol.
    fn answered(&self, escape: &mut ReservedEscape, number: usize) -> Result<(), StopReason> {
        let Some(pinged) = self.pinged else {
            std::thread::sleep(UNPACED_SETTLE);
            return Ok(());
        };
        let counter = u32::try_from(number).unwrap_or(u32::MAX - 1);
        let token = ping_token(std::process::id(), counter);
        match ping(self.server, pinged, token, true, escape, self.halt) {
            Ok(Answer::Unanswered(reason)) => Err(reason),
            Ok(_) => Ok(()),
            Err(Lost) => Err(StopReason::Dropped),
        }
    }
}

fn chords_stopped(chords_sent: usize, chords_expected: usize, reason: StopReason) -> SendError {
    SendError::ChordsTruncated {
        chords_sent,
        chords_expected,
        reason,
    }
}

pub(super) fn unsupported(refusal: Refusal) -> SendError {
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
fn fake(conn: &Conn, root: u32, press: bool, keycode: u8) -> Result<(), Lost> {
    let kind = if press {
        KEY_PRESS_EVENT
    } else {
        KEY_RELEASE_EVENT
    };
    conn.xtest_fake_input(kind, keycode, 0, root, 0, 0, 0)?;
    Ok(())
}

/// Waits until the server has handled everything sent to it so far.
fn roundtrip(conn: &Conn) -> Result<(), Lost> {
    conn.get_input_focus()?.reply()?;
    Ok(())
}

/// The spare keys that were rewritten, given back to the layout when this is
/// dropped - on every way out of the send, a panic included.
struct Remapped<'a> {
    conn: &'a Conn,
    spare: &'a [u8],
    /// The keysym written on each spare key, zero for a key not rewritten.
    written: Vec<u32>,
    /// How long the keys must stay as they are, because keys were sent through
    /// them and the application may not have read them yet (`give_back_wait`).
    owed: Duration,
    /// The lock of the display, whose journal names every key rewritten (`D157`).
    journal: Option<&'a mut DisplayLock>,
}

impl Remapped<'_> {
    /// Makes the `slot`-th spare key type `keysym`, on both levels: a keysym alone
    /// on a key is changed to lower case by the server, and one copy makes the key
    /// a letter that Caps Lock acts on (`OBS-202` point 7). The journal is told
    /// first, so a send that stops right after leaves the key named.
    fn assign(&mut self, slot: usize, keysym: u32) -> Result<(), Lost> {
        self.pay();
        let keycode = self.spare[slot];
        if let Some(journal) = self.journal.as_deref_mut() {
            journal.note(Rewrite { keycode, keysym });
        }
        self.written[slot] = keysym;
        self.conn
            .change_keyboard_mapping(1, keycode, 2, &[keysym, keysym])?;
        Ok(())
    }

    /// The keys rewritten, each with what is written on it.
    fn rewrites(&self) -> Vec<Rewrite> {
        self.spare
            .iter()
            .zip(&self.written)
            .filter(|(_, keysym)| **keysym != 0)
            .map(|(keycode, keysym)| Rewrite {
                keycode: *keycode,
                keysym: *keysym,
            })
            .collect()
    }

    /// The server has carried out every rewrite asked of it, so the journal needs to
    /// name only what the keys carry now.
    fn settled(&mut self) {
        let rewrites = self.rewrites();
        if let Some(journal) = self.journal.as_deref_mut() {
            journal.settle(&rewrites);
        }
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
    /// Gives back the keys that still carry what this send wrote on them. A server that
    /// does not answer gets nothing zeroed blindly: the journal keeps the keys, and the
    /// next send gives them back.
    fn drop(&mut self) {
        self.pay();
        let rewrites = self.rewrites();
        if rewrites.is_empty() {
            return;
        }
        if give_back(self.conn, &rewrites).is_ok()
            && let Some(journal) = self.journal.as_deref_mut()
        {
            journal.clear();
        }
    }
}

/// Zeroes the keys among `rewrites` that still carry the keysym written on them, and no
/// other: a key another program wrote on in the meantime is that program's now. Returns
/// once the server has done it.
fn give_back(conn: &Conn, rewrites: &[Rewrite]) -> Result<(), Lost> {
    let setup = conn.setup();
    let Some((first, count)) = span(rewrites, setup.min_keycode, setup.max_keycode) else {
        return Ok(());
    };
    let mapping = conn.get_keyboard_mapping(first, count)?.reply()?;
    let keymap = Keymap::new(
        first,
        usize::from(mapping.keysyms_per_keycode),
        mapping.keysyms,
    );
    for keycode in ours(&keymap, rewrites) {
        conn.change_keyboard_mapping(1, keycode, 1, &[0])?;
    }
    roundtrip(conn)
}

/// The display locked for this send, and the keys a send before it left rewritten given
/// back (`D157`). `None` when there is no lock to ask about, and the send types as it did
/// before there was one.
fn hold_display(
    server: &Server,
    number: u32,
    halt: &AtomicBool,
    expected: usize,
) -> Result<Option<DisplayLock>, SendError> {
    let mut lock = match DisplayLock::take(number, halt) {
        Ok(lock) => lock,
        Err(NotTaken::Busy) => return Err(SendError::Busy),
        Err(NotTaken::Halted) => return Err(truncated(0, expected, StopReason::Halted)),
    };
    if let Some(lock) = lock.as_mut() {
        let left = lock.journal();
        if !left.is_empty() {
            give_back(&server.conn, &left).map_err(|_| unsupported(Refusal::NoLocalServer))?;
            lock.clear();
        }
    }
    Ok(lock)
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
    /// No answer, and the wait ended for the reason given (`unanswered`): the
    /// application took no answer in `PING_WAIT`, or the person stopped the send
    /// before it came. The spare keys of the batch were sent, but the layout is
    /// given back before the application runs again, and a key it reads then
    /// types nothing (`OBS-206`). The keys of the layout are not given back and
    /// are typed when the application reads them, so a batch that held one is
    /// counted as on its way, not as lost (`stopped_on_the_wire`, R18).
    Unanswered(StopReason),
}

/// Asks `pinged.window` whether it has got through everything the server queued for
/// it before this message, and waits for the answer. Events reach a client in the
/// order the server queued them, and a toolkit answers from its event loop.
///
/// A ping that is not answered is the application's silence only while the server
/// still answers: a stopped server holds the ping too, and that is the connection
/// lost, not an application that stopped taking keys. So the server is asked once
/// before the silence is put on the application.
///
/// `Escape` and `halt` are looked at every `LOOK_EVERY` meanwhile (R20), and
/// `keys_out` says whether keys went before this ping: with none, `Escape` has
/// nothing to wait for (`unanswered`).
fn ping(
    server: &Server,
    pinged: Pinged,
    token: u32,
    keys_out: bool,
    escape: &mut ReservedEscape,
    halt: &AtomicBool,
) -> Result<Answer, Lost> {
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
    let mut next_look = started;
    loop {
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
        let now = Instant::now();
        if now >= next_look {
            next_look = now + LOOK_EVERY;
            let halted = halt.load(Ordering::Relaxed);
            if let Some(reason) = unanswered(now - started, keys_out, escape.pressed(), halted) {
                if reason == StopReason::NotTaking {
                    roundtrip(&server.conn)?;
                }
                return Ok(Answer::Unanswered(reason));
            }
        }
        std::thread::sleep(Duration::from_millis(1));
    }
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

pub(super) fn read_keyboard(conn: &Conn) -> Result<Keyboard, Lost> {
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
    remapped.settled();
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

/// One batch on the wire: the keys pressed and released, the spare keys already
/// rewritten for it. Tells whether a spare key was pressed, because only then is there
/// a layout to keep for an application that has not read the keys yet.
fn type_batch(
    server: &Server,
    spare: &[u8],
    batch: &Batch,
    keysyms: &[u32],
    layout: &LayoutKeys,
) -> Result<bool, Lost> {
    let mut used_spare = false;
    for keysym in &keysyms[batch.first..batch.end] {
        if let Some(key) = layout.key_for(*keysym) {
            tap_layout_key(server, layout.shift_key(), key)?;
            continue;
        }
        let keycode = spare[batch.slot(*keysym).ok_or(Lost)?];
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
fn confirm_alive(
    server: &Server,
    pinged: Pinged,
    expected: usize,
    escape: &mut ReservedEscape,
    halt: &AtomicBool,
) -> Result<(), SendError> {
    alive(server, pinged, escape, halt).map_err(|reason| truncated(0, expected, reason))
}

/// Whether the application answers a ping at all, and if it does not, why the send stops.
/// No key went before it, so `Escape` ends the wait at once.
fn alive(
    server: &Server,
    pinged: Pinged,
    escape: &mut ReservedEscape,
    halt: &AtomicBool,
) -> Result<(), StopReason> {
    let token = ping_token(std::process::id(), u32::MAX);
    match ping(server, pinged, token, false, escape, halt) {
        Ok(Answer::Unanswered(reason)) => Err(reason),
        Ok(_) => Ok(()),
        Err(Lost) => Err(StopReason::Dropped),
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
        // layout for it would only keep the send waiting. Nor is a person who
        // stopped the send waited on.
        Answer::Unanswered(reason) => Err(reason),
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
    target: WindowRef,
    halt: &AtomicBool,
    progress: &mut dyn FnMut(SendProgress),
) -> Result<SendOutcome, SendError> {
    let characters: Vec<char> = text.chars().collect();
    if characters.is_empty() {
        // Nothing to send, so nothing to connect to: an empty value is a real
        // catalogue entry.
        return Ok(SendOutcome {
            units: 0,
            events: 0,
            following: Following::Application,
        });
    }
    let keysyms = keysyms_of(&characters)?;
    let address = environment_route().map_err(unsupported)?;
    let server = connect(address).map_err(unsupported)?;
    let lost = |_: Lost| unsupported(Refusal::NoLocalServer);
    let expected = utf16_units(&characters);
    // Before the map is read: the keys of another send are not keys of the layout.
    let mut lock = hold_display(&server, address.number, halt, expected)?;
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

    // Read after the wait for held keys, which can last two seconds: the window the value
    // was meant for, or nothing is typed (`D148`).
    let Some(front) = focused_toplevel(&server.conn, server.root)
        .map_err(lost)?
        .filter(|front| still_in_front(target, Some(WindowRef(u64::from(*front)))))
    else {
        return Err(truncated(0, expected, StopReason::FocusMoved));
    };
    let pinged = pinged_window(&server).map_err(lost)?;
    // Declared before the keys, so the keys are given back first: the wait for
    // `Escape` to come up must not keep the layout rewritten.
    let mut escape = XEscape::reserve(address, keyboard.keymap.find_keycode(KEYSYM_ESCAPE));
    // No key goes under a modifier that makes it a command, the tap of the warm-up included:
    // a lock somebody left on is found here, before anything is pressed (`OBS-231`).
    hands_off(&server, &keyboard, state, false, front, &mut escape, halt)
        .map_err(|reason| stopped_before_batch(reason, 0, expected, true))?;
    if needs_spare {
        warm_up(&server, spare[0]).map_err(lost)?;
    }
    let mut remapped = Remapped {
        conn: &server.conn,
        spare: &spare,
        written: vec![0; spare.len()],
        owed: Duration::ZERO,
        journal: lock.as_mut(),
    };
    // A value that fits the spare keys is assigned once, here, and the keys are not
    // touched again until the send is over. One that does not fit is rewritten
    // between batches, after the wait the last batch is owed.
    let input_method = names_an_input_method(std::env::var("XMODIFIERS").ok().as_deref());
    let plan = slots_for_value(&keysyms, spare.len(), &layout);
    if let Some(slots) = plan.as_ref().filter(|slots| !slots.is_empty()) {
        assign_all(&server, &mut remapped, slots).map_err(lost)?;
    }

    let uses_layout = layout.types_any(&keysyms);
    if let (true, Some(pinged)) = (uses_layout, pinged) {
        confirm_alive(&server, pinged, expected, &mut escape, halt)?;
    }

    let mut cadence = Cadence::starting(Instant::now(), PROGRESS_INTERVAL);
    let mut arrived = 0usize;
    let mut first = 0usize;
    let mut limit = FIRST_BATCH_CHARS;
    let mut number = 0u32;
    while first < keysyms.len() {
        if let Some(reason) = stop_before_next(&server, front, &mut escape, halt) {
            return Err(truncated(arrived, expected, reason));
        }
        let batch = match &plan {
            Some(slots) => batch_of_value(slots, first, keysyms.len(), limit),
            None => next_batch(&keysyms, first, spare.len(), limit, &layout),
        };
        let batch_units = utf16_units(&characters[batch.first..batch.end]);
        if plan.is_none() && !batch.keysyms.is_empty() {
            assign_all(&server, &mut remapped, &batch.keysyms)
                .map_err(|_| truncated(arrived, expected, StopReason::Dropped))?;
        }
        // From here the batch is on the wire, and it counts only once the
        // application has answered: the layout is given back as the send ends, and
        // an application that has not looked yet reads the spare keys of this batch
        // in the layout it finds then, where they type nothing (`OBS-206`). Its keys
        // of the layout it types whenever it reads them, so with one of those a stop
        // leaves a floor, not a count (R18).
        let layout_keys = layout.types_any(&keysyms[batch.first..batch.end]);
        // Looked at last, right before the keys: the server stamps each with the state it
        // meets, so what the person pressed since the last batch is seen here (R27).
        hands_off(
            &server,
            &keyboard,
            state,
            layout_keys,
            front,
            &mut escape,
            halt,
        )
        .map_err(|reason| stopped_before_batch(reason, arrived, expected, first == 0))?;
        let on_the_wire = move |reason| stopped_on_the_wire(arrived, expected, reason, layout_keys);
        let Ok(used_spare) = type_batch(&server, &spare, &batch, &keysyms, &layout) else {
            return Err(on_the_wire(StopReason::Dropped));
        };
        let answer = match pinged {
            Some(pinged) => {
                let token = ping_token(std::process::id(), number);
                ping(&server, pinged, token, true, &mut escape, halt)
                    .map_err(|_| on_the_wire(StopReason::Dropped))?
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
        limit = take_answer(answer, limit, &kept, &mut remapped).map_err(on_the_wire)?;
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
        following: if pinged.is_some() {
            Following::Application
        } else {
            Following::System
        },
    })
}
