//! What a send on X11 looks at before it presses a key: the modifiers the person holds,
//! `Escape`, the caller's flag and the window in front (`D145`, `D148`, R03, R27).
//!
//! The server is the sensor. It sees a key of the hand before any client does, and it
//! stamps every key event with the state it meets, so a look right before the keys
//! go sees what they will meet. Typing and clearing ask it different questions,
//! because the keys they press answer to different modifiers: a key of the layout to
//! `Shift` and to the group, a spare key to neither, and every key to `Ctrl` and `Alt`.

use core::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use x11rb::protocol::xproto::ConnectionExt as _;

use super::Lost;
use super::connect::Server;
use super::escape::ReservedEscape;
use super::front::focused_toplevel;
use super::send::{Keyboard, unsupported};
use crate::linux_clearing_rules::state_holds;
use crate::linux_rules::{
    InTheWay, LOOK_EVERY, Refusal, caps_lock_on, held_modifier, in_the_way, modifiers_down,
};
use crate::{MODIFIER_RELEASE_WAIT, SendError, StopReason};

/// The modifier that is down right now, if one is.
fn modifier_down(server: &Server, keyboard: &Keyboard) -> Result<Option<&'static str>, Lost> {
    let keys = server.conn.query_keymap()?.reply()?.keys;
    Ok(held_modifier(
        &keys,
        &keyboard.modifier_keys,
        keyboard.keys_per_modifier,
    ))
}

/// The modifier that would change what a key of the recipe does, if one is in the way: a
/// key of it that is down, or one the state of the server says is latched or locked.
pub(super) fn modifier_in_the_way(
    server: &Server,
    keyboard: &Keyboard,
) -> Result<Option<&'static str>, Lost> {
    if let Some(name) = modifier_down(server, keyboard)? {
        return Ok(Some(name));
    }
    let state = server.conn.query_pointer(server.root)?.reply()?.mask;
    Ok(state_holds(u16::from(state)))
}

/// Waits for every modifier to come up, for at most `MODIFIER_RELEASE_WAIT`. Caps Lock is
/// not looked at: the keys of the recipe are not letters, and it does not change them.
pub(super) fn wait_for_modifiers(server: &Server, keyboard: &Keyboard) -> Result<(), SendError> {
    let deadline = Instant::now() + MODIFIER_RELEASE_WAIT;
    while let Some(key) =
        modifier_in_the_way(server, keyboard).map_err(|_| unsupported(Refusal::NoLocalServer))?
    {
        if Instant::now() >= deadline {
            return Err(SendError::ModifierHeld { key });
        }
        std::thread::sleep(LOOK_EVERY);
    }
    Ok(())
}

/// Waits for every modifier to come up, for at most `MODIFIER_RELEASE_WAIT`, and
/// then refuses Caps Lock. The modifier is the ordinary case right after a hotkey.
/// Caps Lock is a state somebody left on, and waiting for it would be waiting for
/// them. Gives the modifier state the server reports, with the layout group in it.
pub(super) fn wait_for_keys_released(
    server: &Server,
    keyboard: &Keyboard,
) -> Result<u16, SendError> {
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

/// Returns once nothing the keyboard holds would change what the next batch types, or says
/// why the send stops before it (R27, `OBS-231`, `D160`). `start` is the state the keys of
/// the layout were chosen for, and `layout_keys` says whether the batch holds one of them
/// (`in_the_way`). Nothing is pressed meanwhile, and `Escape`, the caller's flag and the
/// window in front are looked at between two looks, as between batches. A modifier still
/// in the way after `MODIFIER_RELEASE_WAIT` is held for something else, and the send stops
/// naming it.
pub(super) fn hands_off(
    server: &Server,
    keyboard: &Keyboard,
    start: u16,
    layout_keys: bool,
    front: u32,
    escape: &mut ReservedEscape,
    halt: &AtomicBool,
) -> Result<(), StopReason> {
    let waited = Instant::now();
    loop {
        let (down, state) = held_now(server, keyboard).map_err(|Lost| StopReason::Dropped)?;
        match in_the_way(down, state, start, layout_keys) {
            InTheWay::Nothing => return Ok(()),
            InTheWay::Stop(reason) => return Err(reason),
            InTheWay::WaitFor(key) if waited.elapsed() >= MODIFIER_RELEASE_WAIT => {
                return Err(StopReason::KeyPressed { key });
            }
            InTheWay::WaitFor(_) => {}
        }
        std::thread::sleep(LOOK_EVERY);
        if let Some(reason) = stop_before_next(server, front, escape, halt) {
            return Err(reason);
        }
    }
}

/// The modifiers with a key down, as the bits of a state, and the state the server reports
/// (latched and locked modifiers and the layout group in it), in one look.
fn held_now(server: &Server, keyboard: &Keyboard) -> Result<(u16, u16), Lost> {
    let keys = server.conn.query_keymap()?.reply()?.keys;
    let down = modifiers_down(&keys, &keyboard.modifier_keys, keyboard.keys_per_modifier);
    let state = server.conn.query_pointer(server.root)?.reply()?.mask;
    Ok((down, u16::from(state)))
}

/// Why the send must stop before the next batch, if it must: `Escape` first, as on
/// every system, then the caller's flag (R03), then the window in front.
pub(super) fn stop_before_next(
    server: &Server,
    front: u32,
    escape: &mut ReservedEscape,
    halt: &AtomicBool,
) -> Option<StopReason> {
    if escape.pressed() {
        return Some(StopReason::Escape);
    }
    if halt.load(Ordering::Relaxed) {
        return Some(StopReason::Halted);
    }
    match focused_toplevel(&server.conn, server.root) {
        Ok(Some(window)) if window == front => None,
        Ok(_) => Some(StopReason::FocusMoved),
        Err(Lost) => Some(StopReason::Dropped),
    }
}
