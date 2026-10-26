//! The keys that clear a field on a Wayland session, sent as a dialogue with the engine
//! (`D145` point 74, `OBS-229`). `linux_clearing_rules.rs` has the measurements.
//!
//! A Wayland session lets no client press a key but through the remote-desktop portal, and
//! the keys of the recipe (`Home`, `Shift+End`, `Delete`) are dangerous in the one situation
//! that is usual: the shortcut of the palette is `Ctrl+Alt+<letter>`, the hand is still on
//! it, and the compositor adds the modifiers the hand holds to every key the portal presses.
//! `Ctrl+Home` and `Ctrl+Shift+End` select the document, not the line (the document was
//! wiped, 7 characters to 0), and `Ctrl+Alt+Delete` is the logout dialog of the desktop. The
//! portal cannot be asked which modifier is down, and a key-up it sends does not release one.
//!
//! The engine of this program can: it is handed every key before the application, with the
//! state the hand holds, and may take it. So the order of a clearing is:
//!
//! 1. the person's agreement to the portal is had (`Keys::open`), the first time with a window
//!    of the system and after that at once,
//! 2. the engine is made the global one, and the clearing waits for the daemon to say a field
//!    has the focus: a field that does not go through the engine cannot be guarded, and no key
//!    is sent to it,
//! 3. for each chord the engine is told which key to wait for, the key is pressed through the
//!    portal, and the engine says what it found on arrival. A key with a modifier the recipe
//!    does not have is taken by the engine, so the application never sees it. `Home` is the
//!    first key and the probe: a modifier found on it is waited out for as long as a held
//!    modifier is waited out on every system (`MODIFIER_RELEASE_WAIT`), and the key is tried
//!    again meanwhile, which costs nothing in the field because the engine took it,
//! 4. a key the engine did not hear in time did not go through it, and the clearing stops.
//!
//! What the dialogue does not say is whether the field is empty after it. The keys arrived
//! as the recipe has them, and a multi-line field is cleared one line at a time, as on every
//! system (`D54`).

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use super::{Session, daemon, put_back, stopped_chords, unsupported_keys};
use crate::linux::portal::remote::Keys;
use crate::linux_clearing_rules::{
    CHORD_GAP, ECHO_WAIT, HELD_RETRY, Judgement, KEY_GAP, KEYSYM_SHIFT_L, Next, expected_for,
    keysym_of, next_step,
};
use crate::linux_ibus_rules::{DEADLINE, FOCUS_SETTLE, engine_to_restore};
use crate::portal::Unavailable;
use crate::{Chord, MODIFIER_RELEASE_WAIT, SendError, StopReason, Waiting};

/// Presses `chords` in order, each as a complete press and release, on the field that has the
/// focus, with the engine standing guard over every key. Returns how many were pressed.
/// `waiting` hears that the window of the system stands, if it does (`D145` point 76).
pub(crate) fn press_chords(
    chords: &[Chord],
    halt: &AtomicBool,
    waiting: &mut dyn FnMut(Waiting),
) -> Result<usize, SendError> {
    if chords.is_empty() {
        // Nothing to press, so nothing to ask of anybody.
        return Ok(0);
    }
    let socket = daemon::locate().map_err(|_| unsupported_keys(Unavailable::NoBus))?;
    // The agreement first: a window of the system that waits for the person must not have
    // the engine of this program standing in for their keyboard meanwhile.
    let keys = Keys::open(waiting).map_err(unsupported_keys)?;
    let mut session =
        Session::begin(&socket, DEADLINE).map_err(|_| unsupported_keys(Unavailable::NoBus))?;
    let result = run(&mut session, &keys, chords, halt);
    // On every way out, a failed clearing included: the person's engine is theirs.
    let previous = session.end();
    if let Some(name) = engine_to_restore(previous.as_deref()) {
        put_back(&socket, name);
    }
    drop(keys);
    result
}

fn run(
    session: &mut Session,
    keys: &Keys,
    chords: &[Chord],
    halt: &AtomicBool,
) -> Result<usize, SendError> {
    let total = chords.len();
    let focused = session
        .wait_for_focus()
        .map_err(|_| unsupported_keys(Unavailable::NoBus))?;
    if !focused {
        // Nobody who is served by the engine has the focus: nothing can be guarded.
        return Err(stopped_chords(0, total, StopReason::NotTaking));
    }
    if session.pause(FOCUS_SETTLE) {
        return Err(stopped_chords(0, total, StopReason::Escape));
    }
    for (number, chord) in chords.iter().enumerate() {
        // `Escape` stops the clearing before the next chord, as it stops every send (`D96`).
        if (number > 0 && session.pause(CHORD_GAP)) || session.escape_pressed() {
            return Err(stopped_chords(number, total, StopReason::Escape));
        }
        // And the caller's flag, the same way (R03). The window of the portal that asks
        // the person, before this, does not look at it.
        if halt.load(Ordering::Relaxed) {
            return Err(stopped_chords(number, total, StopReason::Halted));
        }
        press_guarded(session, keys, chord, number, total)?;
    }
    // The last key was handed back to the daemon, which hands it to the application after
    // this: the daemon is asked a question it answers only once it has dealt with it.
    session.settle();
    Ok(total)
}

/// One chord, pressed once the engine has found nothing wrong with it. The first chord is
/// tried again while a modifier the hand holds is waited out.
fn press_guarded(
    session: &Session,
    keys: &Keys,
    chord: &Chord,
    number: usize,
    total: usize,
) -> Result<(), SendError> {
    let deadline = Instant::now() + MODIFIER_RELEASE_WAIT;
    loop {
        let judged = press_and_listen(session, keys, chord)
            .map_err(|_| stopped_chords(number, total, StopReason::Dropped))?;
        match next_step(judged, number == 0, Instant::now() < deadline) {
            Next::Go => return Ok(()),
            Next::Wait => {
                if session.pause(HELD_RETRY) {
                    return Err(stopped_chords(0, total, StopReason::Escape));
                }
            }
            Next::Refuse(key) => return Err(SendError::ModifierHeld { key }),
            Next::Stop(reason) => return Err(stopped_chords(number, total, reason)),
        }
    }
}

/// The chord on the wire: `Shift` down when the chord has it, the key down, what the engine
/// says about the key, the key up, `Shift` up. `Shift` and the key are let go whatever the
/// engine said, and whether or not they went: a `Shift` left down changes everything the
/// person types next.
fn press_and_listen(
    session: &Session,
    keys: &Keys,
    chord: &Chord,
) -> Result<Option<Judgement>, Unavailable> {
    let keysym = keysym_of(chord.key);
    session.forget_echoes();
    session.expect(Some(expected_for(chord)));
    let pressed = press_down(keys, keysym, chord.shift);
    let judged = if pressed.is_ok() {
        session.echo(ECHO_WAIT)
    } else {
        None
    };
    let released = release(keys, keysym, chord.shift);
    pressed.and(released)?;
    Ok(judged)
}

fn press_down(keys: &Keys, keysym: u32, shift: bool) -> Result<(), Unavailable> {
    if shift {
        keys.key(KEYSYM_SHIFT_L, true)?;
        std::thread::sleep(KEY_GAP);
    }
    keys.key(keysym, true)
}

fn release(keys: &Keys, keysym: u32, shift: bool) -> Result<(), Unavailable> {
    std::thread::sleep(KEY_GAP);
    let key_up = keys.key(keysym, false);
    let shift_up = if shift {
        std::thread::sleep(KEY_GAP);
        keys.key(KEYSYM_SHIFT_L, false)
    } else {
        Ok(())
    };
    key_up.and(shift_up)
}
