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

use core::sync::atomic::{AtomicBool, Ordering};
use core::time::Duration;

use super::ffi::{self, MutRef, Owned};
use super::front::front_pid;
use super::pulse::Pulse;
use super::trust;
use crate::cadence::{Cadence, PROGRESS_INTERVAL};
use crate::confirm::press_confirmed;
use crate::confirm::{Hand, Step, TAKEN_WAIT, Waits, Watch, Wire};
use crate::escape::{ReservedEscape, SystemEscape};
use crate::macos_rules::{
    MacEvent, TEXT_KEY, chord_acted, chord_steps, hand, held_modifier, pid_of, scalars, text_steps,
    units_after, window_of,
};
use crate::{
    Chord, Following, MODIFIER_RELEASE_WAIT, SendError, SendOutcome, SendProgress, StopReason,
    WindowRef,
};

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

/// Waits for every such key to come up, for at most `MODIFIER_RELEASE_WAIT`,
/// and no longer than the caller lets the send go on. The error names the key
/// still down, or is `None` when the caller asked the send to stop (R03).
fn wait_for_keys_released(halt: &AtomicBool) -> Result<(), Option<&'static str>> {
    let deadline = std::time::Instant::now() + MODIFIER_RELEASE_WAIT;
    loop {
        let Some(key) = held_key() else {
            return Ok(());
        };
        if halt.load(Ordering::Relaxed) {
            return Err(None);
        }
        if std::time::Instant::now() >= deadline {
            return Err(Some(key));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// The system as `confirm` asks it, around the application that was in front
/// when the send began.
struct SystemWire<'a> {
    /// The source of every event of this send. Private, so nothing the person
    /// holds is added to an event that did not ask for it.
    source: Owned,
    front: i32,
    escape: ReservedEscape,
    pulse: Pulse,
    /// The caller's flag to stop the send (R03).
    halt: &'a AtomicBool,
    /// The modifiers the events of this send hold down: the flags of the last
    /// one posted. The system's state shows them as it shows the person's.
    ours: u64,
}

impl SystemWire<'_> {
    /// `Escape` pressed or the caller's stop, if the send is being stopped.
    fn stopping(&mut self) -> Option<Watch> {
        if self.escape.pressed() {
            Some(Watch::Escape)
        } else if self.halt.load(Ordering::Relaxed) {
            Some(Watch::Halted)
        } else {
            None
        }
    }

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

impl Wire<MacEvent> for SystemWire<'_> {
    fn send(&mut self, event: MacEvent) -> bool {
        let posted = self.post(event);
        if posted {
            self.ours = event.flags;
        }
        posted
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
        if let Some(stop) = self.stopping() {
            return stop;
        }
        if front_pid() != Some(self.front) {
            return Watch::Moved;
        }
        let (escape, halt) = (&mut self.escape, self.halt);
        let pulse = self
            .pulse
            .tick(&mut || escape.pressed() || halt.load(Ordering::Relaxed));
        // Asked again: the question can wait for seconds, and the stop asked
        // for meanwhile comes before what the application said.
        self.stopping().unwrap_or(pulse)
    }
    fn hand(&mut self, shift_counts: bool) -> Hand {
        let held = unsafe { ffi::CGEventSourceFlagsState(ffi::STATE_HID) };
        hand(held, self.ours, shift_counts)
    }
    fn pause(&mut self, how_long: Duration) {
        std::thread::sleep(how_long);
    }
}

/// The application in front, `Escape` held for as long as the send lasts, and
/// the round trips to the application. One per send.
struct Session<'a> {
    wire: SystemWire<'a>,
}

impl<'a> Session<'a> {
    /// The session around the application `target` names, or nothing when
    /// another application - or none - is in front by now (`D148`). Its window,
    /// when the target told it, is the pulse's to compare, with the same round
    /// trip that paces the send (R02): another window of the program in front by
    /// now stops the send at its first look, before any key.
    fn begin(target: WindowRef, halt: &'a AtomicBool) -> Option<Self> {
        let front = front_pid()?;
        if front.unsigned_abs() != pid_of(target.0) {
            return None;
        }
        let source =
            Owned::new(unsafe { ffi::CGEventSourceCreate(ffi::STATE_PRIVATE) }.cast_const())?;
        // Reserved on THIS thread, which is the one that asks it between events
        // (`D96`), and before the first question, which it may cut short.
        let mut escape = SystemEscape::reserve();
        let pulse = Pulse::start(front, window_of(target.0), &mut || {
            escape.pressed() || halt.load(Ordering::Relaxed)
        });
        Some(Self {
            wire: SystemWire {
                source,
                front,
                escape,
                pulse,
                halt,
                ours: 0,
            },
        })
    }

    fn waits() -> Waits {
        Waits {
            system: TAKEN_WAIT,
            target: None,
            release: MODIFIER_RELEASE_WAIT,
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
    target: WindowRef,
    halt: &AtomicBool,
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
            following: Following::Application,
        });
    }
    if !trust::may_send() {
        return Err(SendError::NeedsPermission);
    }
    let nothing_sent = |reason| SendError::Truncated {
        units_sent: 0,
        units_expected: expected,
        reason,
    };
    wait_for_keys_released(halt).map_err(|still| match still {
        Some(key) => SendError::ModifierHeld { key },
        None => nothing_sent(StopReason::Halted),
    })?;
    let Some(mut session) = Session::begin(target, halt) else {
        return Err(nothing_sent(StopReason::FocusMoved));
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
        following: if session.wire.pulse.active() {
            Following::Application
        } else {
            Following::System
        },
    })
}

pub(crate) fn send_chords(
    chords: &[Chord],
    target: WindowRef,
    halt: &AtomicBool,
) -> Result<usize, SendError> {
    if chords.is_empty() {
        return Ok(0);
    }
    if !trust::may_send() {
        return Err(SendError::NeedsPermission);
    }
    let nothing_pressed = |reason| SendError::ChordsTruncated {
        chords_sent: 0,
        chords_expected: chords.len(),
        reason,
    };
    wait_for_keys_released(halt).map_err(|still| match still {
        Some(key) => SendError::ModifierHeld { key },
        None => nothing_pressed(StopReason::Halted),
    })?;
    let Some(mut session) = Session::begin(target, halt) else {
        return Err(nothing_pressed(StopReason::FocusMoved));
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
