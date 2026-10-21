//! Typing and pressing keys on macOS: one event at a time, each waited for.
//!
//! The order of an event is the one `confirm.rs` already keeps for Windows: look
//! at the window in front and at `Escape`, post the event, wait until the system
//! shows it, and only then the next. Here "shows it" is the HID key state, which
//! follows an event posted at the HID tap and nothing posted higher.
//!
//! Measured 2026-10-19 (`macos_rules.rs` has the numbers): without that wait the
//! system loses the events and `CGEventPost` says nothing - not one character of
//! a back-to-back send arrived. The wait is not a courtesy, it is the flow
//! control, and with the permission missing it is also what notices: the key
//! state never changes, nine of nine events unconfirmed.
//!
//! What the system showing an event does not say is whether the application took
//! it, and that is `pulse.rs`.

use core::time::Duration;

use super::ffi::{self, MutRef, Owned};
use super::front::front_pid;
use super::pulse::Pulse;
use super::trust;
use crate::cadence::{Cadence, PROGRESS_INTERVAL};
use crate::confirm::press_confirmed;
use crate::confirm::{Step, TAKEN_WAIT, Waits, Watch, Wire};
use crate::escape::{ReservedEscape, SystemEscape};
use crate::macos_rules::{
    MacEvent, PulseVerdict, TEXT_KEY, chord_acted, chord_steps, held_modifier, scalars, text_steps,
    units_after,
};
use crate::{Chord, MODIFIER_RELEASE_WAIT, SendError, SendOutcome, SendProgress, StopReason};

/// Whether this program is allowed to send keystrokes right now. Asked on every
/// send: the person can grant it, or take it back, while the program runs.
pub(crate) fn send_allowed() -> bool {
    trust::may_send()
}

/// The first of the keys that would change what the tool types, if one is down.
///
/// The modifiers, because a held `Command` turns the next letter into a
/// shortcut - and `A`, because every character is posted as that key and the
/// wait for it to show up would be answered by a key somebody is holding.
fn held_key() -> Option<&'static str> {
    let flags = unsafe { ffi::CGEventSourceFlagsState(ffi::STATE_HID) };
    if let Some(name) = held_modifier(flags) {
        return Some(name);
    }
    unsafe { ffi::CGEventSourceKeyState(ffi::STATE_HID, TEXT_KEY) }.then_some("A")
}

/// Waits for every such key to come up, for at most `MODIFIER_RELEASE_WAIT`.
fn wait_for_keys_released() -> Result<(), SendError> {
    let deadline = std::time::Instant::now() + MODIFIER_RELEASE_WAIT;
    loop {
        let Some(key) = held_key() else {
            return Ok(());
        };
        if std::time::Instant::now() >= deadline {
            return Err(SendError::ModifierHeld { key });
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// The system as `confirm` asks it, around the application that was in front
/// when the send began.
struct SystemWire {
    /// The source of every event of this send. Private, so nothing the person
    /// holds is added to an event that did not ask for it.
    source: Owned,
    front: i32,
    escape: ReservedEscape,
    pulse: Pulse,
}

impl SystemWire {
    /// One event to the system, which does not say whether it took it.
    fn post(&self, event: MacEvent) -> bool {
        let made = unsafe {
            ffi::CGEventCreateKeyboardEvent(self.source.0.cast_mut(), event.key, event.down)
        };
        if made.is_null() {
            return false;
        }
        let made: MutRef = made;
        unsafe {
            if event.unit_count > 0 {
                ffi::CGEventKeyboardSetUnicodeString(made, event.unit_count, event.units.as_ptr());
            }
            ffi::CGEventSetFlags(made, event.flags);
            ffi::CGEventPost(ffi::TAP_HID, made);
            ffi::CFRelease(made.cast_const());
        }
        true
    }
}

impl Wire<MacEvent> for SystemWire {
    fn send(&mut self, event: MacEvent) -> bool {
        self.post(event)
    }
    fn system_down(&mut self, key: u16) -> bool {
        unsafe { ffi::CGEventSourceKeyState(ffi::STATE_HID, key) }
    }
    fn target_down(&mut self, _key: u16) -> bool {
        // Never asked: the send is not paced by a key state of the application
        // (`Waits::target` is `None`) but by the question of `pulse.rs`.
        true
    }
    fn watch(&mut self) -> Watch {
        // First: a person who pressed Escape asked for exactly this stop.
        if self.escape.pressed() {
            Watch::Escape
        } else if front_pid() != Some(self.front) {
            Watch::Moved
        } else if self.pulse.tick() == PulseVerdict::Hung {
            Watch::Hung
        } else {
            Watch::Steady
        }
    }
    fn pause(&mut self, how_long: Duration) {
        std::thread::sleep(how_long);
    }
}

/// The application in front, `Escape` held for as long as the send lasts, and
/// the round trips to the application. One per send.
struct Session {
    wire: SystemWire,
}

impl Session {
    fn begin() -> Option<Self> {
        let front = front_pid()?;
        let source =
            Owned::new(unsafe { ffi::CGEventSourceCreate(ffi::STATE_PRIVATE) }.cast_const())?;
        Some(Self {
            wire: SystemWire {
                source,
                front,
                // Reserved on THIS thread, which is the one that asks it between
                // events (`D96`).
                escape: SystemEscape::reserve(),
                pulse: Pulse::start(front),
            },
        })
    }

    fn waits() -> Waits {
        Waits {
            system: TAKEN_WAIT,
            target: None,
        }
    }

    fn press(&mut self, steps: &[Step<MacEvent>]) -> crate::confirm::Pressed {
        press_confirmed(steps, &mut self.wire, Self::waits())
    }

    /// Lets go of whatever went down in `steps`, unconfirmed and clean of
    /// modifiers: a key the system did not take is not down, so its release is
    /// harmless, and one it took must not stay down - least of all `Command`.
    fn release(&mut self, steps: &[Step<MacEvent>]) {
        for step in steps.iter().filter(|step| step.down) {
            self.wire.post(MacEvent {
                down: false,
                flags: 0,
                ..step.event
            });
        }
    }
}

pub(crate) fn send_text(
    text: &str,
    progress: &mut dyn FnMut(SendProgress),
) -> Result<SendOutcome, SendError> {
    let characters = scalars(text);
    let expected: usize = characters.iter().map(|scalar| scalar.count).sum();
    let events = characters.len() * 2;
    if characters.is_empty() {
        // Nothing to send, so nothing to wait for and nothing to ask permission
        // for: an empty value is a real catalogue entry.
        return Ok(SendOutcome {
            units: 0,
            events: 0,
            paced: true,
        });
    }
    if !trust::may_send() {
        return Err(SendError::NeedsPermission);
    }
    wait_for_keys_released()?;
    let Some(mut session) = Session::begin() else {
        return Err(SendError::Truncated {
            units_sent: 0,
            units_expected: expected,
            reason: StopReason::FocusMoved,
        });
    };

    let mut cadence = Cadence::starting(std::time::Instant::now(), PROGRESS_INTERVAL);
    let mut arrived = 0usize;
    for scalar in characters {
        let steps = text_steps(scalar);
        let pressed = session.press(&steps);
        if let Some(reason) = pressed.stop {
            // Always, unconfirmed (`D96`): a stop between the two halves of a
            // character must not leave its key held.
            session.release(&steps);
            return Err(SendError::Truncated {
                units_sent: units_after(arrived, scalar.count, pressed.taken),
                units_expected: expected,
                reason,
            });
        }
        arrived += scalar.count;
        if cadence.due(std::time::Instant::now()) {
            progress(SendProgress {
                units_arrived: arrived,
                units_total: expected,
            });
        }
    }
    Ok(SendOutcome {
        units: expected,
        events,
        paced: session.wire.pulse.active(),
    })
}

pub(crate) fn send_chords(chords: &[Chord]) -> Result<usize, SendError> {
    if chords.is_empty() {
        return Ok(0);
    }
    if !trust::may_send() {
        return Err(SendError::NeedsPermission);
    }
    wait_for_keys_released()?;
    let Some(mut session) = Session::begin() else {
        return Err(SendError::ChordsTruncated {
            chords_sent: 0,
            chords_expected: chords.len(),
            reason: StopReason::FocusMoved,
        });
    };
    for (index, chord) in chords.iter().enumerate() {
        let (steps, leads) = chord_steps(*chord);
        let pressed = session.press(&steps);
        if let Some(reason) = pressed.stop {
            session.release(&steps);
            return Err(SendError::ChordsTruncated {
                chords_sent: index + usize::from(chord_acted(pressed.taken, leads)),
                chords_expected: chords.len(),
                reason,
            });
        }
    }
    Ok(chords.len())
}
