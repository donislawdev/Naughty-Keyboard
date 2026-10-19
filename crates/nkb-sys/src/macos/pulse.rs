//! Keeping the send at the pace of the application.
//!
//! On Windows the application's own queue says what it has taken (`D95`). macOS
//! gives nothing like it: the system shows an event taken as soon as it is queued
//! (the HID key state), and the application works through its queue later - 20 000
//! characters posted in 5 s took the field 23 s more to type (measured
//! 2026-10-19). Left alone, a send is finished long before the field is, and an
//! `Escape` pressed halfway stops nothing that is already queued.
//!
//! What does wait behind the queue is a question to the application. An
//! accessibility request is answered by the application's main thread, which also
//! types, so its answer comes when the keys ahead of it have been taken: 50 us
//! from an idle one and 1.4 s from one 500 characters behind. Asked every
//! [`PULSE_EVERY`] characters it holds the send to the application's pace -
//! 20 000 of 20 000 arrived, the field finished 0.25 to 0.8 s after the send did -
//! and, with a limit on the wait, tells an application that stopped from one that
//! is merely slow: a stopped process answers it with an error after exactly the
//! limit and keeps what was sent to it.
//!
//! What it asks is the ROLE of the focused element, a type name, and the answer
//! is released at once without being read. Never its title, its value or its text.
//!
//! An application that cannot be asked - it answers an error at once - is not
//! paced, and the send says so rather than going on as if it were
//! (`SendOutcome::paced`). A web browser answers from its own process while the
//! page that types lives in another, so for one the pace is the browser's and not
//! the page's: not measured, and named here so nobody counts on it.

use core::time::Duration;

use super::ffi::{self, CfRef, Owned};
use crate::confirm::TARGET_WAIT;
use crate::macos_rules::{PULSE_EVERY, PulseVerdict, pulse_verdict};

/// The round trips of one send, to the application that was in front when it
/// began.
pub(crate) struct Pulse {
    app: Option<Owned>,
    role: Option<Owned>,
    /// Events seen since the last question. Two per character.
    since: usize,
    /// Whether the application answers at all.
    active: bool,
    /// The application did not answer the first question: it was hung before the
    /// send began, and the send stops at its first look.
    hung_at_start: bool,
}

impl Pulse {
    /// Asks the application once, and keeps asking only if it answered.
    pub(crate) fn start(pid: i32) -> Self {
        let app = Owned::new(unsafe { ffi::AXUIElementCreateApplication(pid) }.cast_const());
        let role = ffi::cf_string(c"AXRole");
        if let Some(app) = &app {
            // The same bound the application is given to take a key on Windows:
            // what the system itself calls not responding.
            unsafe { ffi::AXUIElementSetMessagingTimeout(app.0, TARGET_WAIT.as_secs_f32()) };
        }
        let mut pulse = Self {
            app,
            role,
            since: 0,
            active: false,
            hung_at_start: false,
        };
        match pulse.ask() {
            PulseVerdict::Answered => pulse.active = true,
            PulseVerdict::Hung => pulse.hung_at_start = true,
            PulseVerdict::Unsupported => {}
        }
        pulse
    }

    /// Whether the send is held to the application's pace.
    pub(crate) fn active(&self) -> bool {
        self.active
    }

    /// Counts one event, and asks the application when enough have gone by.
    /// `Hung` is the only answer that stops the send.
    pub(crate) fn tick(&mut self) -> PulseVerdict {
        if self.hung_at_start {
            return PulseVerdict::Hung;
        }
        if !self.active {
            return PulseVerdict::Answered;
        }
        self.since += 1;
        if self.since < PULSE_EVERY * 2 {
            return PulseVerdict::Answered;
        }
        self.since = 0;
        match self.ask() {
            PulseVerdict::Unsupported => {
                // It answered before and refuses now: stop asking, keep going.
                self.active = false;
                PulseVerdict::Answered
            }
            verdict => verdict,
        }
    }

    /// One round trip, timed against the limit.
    fn ask(&self) -> PulseVerdict {
        let (Some(app), Some(role)) = (&self.app, &self.role) else {
            return PulseVerdict::Unsupported;
        };
        let start = std::time::Instant::now();
        let mut value: CfRef = core::ptr::null();
        let error = unsafe { ffi::AXUIElementCopyAttributeValue(app.0, role.0, &mut value) };
        // The answer is a role name and is not looked at: only that it came.
        drop(Owned::new(value));
        let waited: Duration = start.elapsed();
        pulse_verdict(error, waited, TARGET_WAIT)
    }
}
