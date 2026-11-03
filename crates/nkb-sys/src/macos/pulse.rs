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
//! What it asks is the window in front of the application, a reference that is
//! released at once, kept only as its token: the same round trip that paces the
//! send says whether the window the send was meant for is still the one in
//! front (R02). Never its title, its value or its text. An application with no
//! window in front answers that, which paces the send just as well.
//!
//! Each question waits [`PULSE_LOOK`] at most and is asked again until
//! [`TARGET_WAIT`] has gone by in all, so a person who presses `Escape`, or a
//! palette that closes, is heard between two of them (R20). One question for the
//! whole limit held such a send for as long as five seconds.
//!
//! An application that cannot be asked - it answers an error at once - is not
//! paced, and the send says so rather than going on as if it were
//! (`SendOutcome::paced`). A web browser answers from its own process while the
//! page that types lives in another, so for one the pace is the browser's and not
//! the page's: not measured, and named here so nobody counts on it.

use super::ax;
use super::ffi::{self, Owned};
use crate::confirm::{TARGET_WAIT, Watch};
use crate::macos_rules::{
    PULSE_EVERY, PULSE_LOOK, PulseVerdict, pulse_after, pulse_verdict, window_still,
};

/// The round trips of one send, to the application that was in front when it
/// began.
pub(crate) struct Pulse {
    app: Option<Owned>,
    /// The window the send is meant for, or `None` when it was never told, and
    /// then only the application is compared.
    window: Option<u32>,
    /// Events seen since the last question. Two per character.
    since: usize,
    /// Whether the application answers at all.
    active: bool,
    /// What the first question found, said at the first look: the application
    /// was hung, or another of its windows was in front, before the send began.
    at_start: Watch,
}

impl Pulse {
    /// Asks the application once, and keeps asking only if it answered. The
    /// window is `meant` if the send's target told it, the one in front now if
    /// not. `stop` says whether the send is being stopped meanwhile.
    pub(crate) fn start(pid: i32, meant: Option<u32>, stop: &mut dyn FnMut() -> bool) -> Self {
        let app = Owned::new(unsafe { ffi::AXUIElementCreateApplication(pid) }.cast_const());
        if let Some(app) = &app {
            unsafe { ffi::AXUIElementSetMessagingTimeout(app.0, PULSE_LOOK.as_secs_f32()) };
        }
        let mut pulse = Self {
            app,
            window: meant,
            since: 0,
            active: false,
            at_start: Watch::Steady,
        };
        match pulse.ask(stop) {
            Some((PulseVerdict::Answered, now)) => {
                pulse.active = true;
                if !window_still(meant, now) {
                    pulse.at_start = Watch::Moved;
                }
                pulse.window = meant.or(now);
            }
            Some((PulseVerdict::Hung, _)) => pulse.at_start = Watch::Hung,
            // Stopped while asking: the first look says why, before this does.
            Some((PulseVerdict::Unsupported, _)) | None => {}
        }
        pulse
    }

    /// Whether the send is held to the application's pace.
    pub(crate) fn active(&self) -> bool {
        self.active
    }

    /// Counts one event, and asks the application when enough have gone by.
    /// `Steady` goes on, and so does a question cut short by `stop`: the caller
    /// looks at what stopped it.
    pub(crate) fn tick(&mut self, stop: &mut dyn FnMut() -> bool) -> Watch {
        if self.at_start != Watch::Steady {
            return self.at_start;
        }
        if !self.active {
            return Watch::Steady;
        }
        self.since += 1;
        if self.since < PULSE_EVERY * 2 {
            return Watch::Steady;
        }
        self.since = 0;
        match self.ask(stop) {
            Some((PulseVerdict::Answered, now)) if !window_still(self.window, now) => Watch::Moved,
            Some((PulseVerdict::Hung, _)) => Watch::Hung,
            Some((PulseVerdict::Unsupported, _)) => {
                // It answered before and refuses now: stop asking, keep going.
                self.active = false;
                Watch::Steady
            }
            Some((PulseVerdict::Answered, _)) | None => Watch::Steady,
        }
    }

    /// Round trips until one says something: the answer and the token of the
    /// window in front, or `None` when `stop` asked the send to stop between two.
    fn ask(&self, stop: &mut dyn FnMut() -> bool) -> Option<(PulseVerdict, Option<u32>)> {
        let Some(app) = &self.app else {
            return Some((PulseVerdict::Unsupported, None));
        };
        let first = std::time::Instant::now();
        loop {
            let asked = std::time::Instant::now();
            let answer = ax::focused_window(app.0);
            let one = pulse_verdict(
                answer.as_ref().err().copied().unwrap_or(0),
                asked.elapsed(),
                PULSE_LOOK,
            );
            if let Some(verdict) = pulse_after(one, first.elapsed(), TARGET_WAIT) {
                // The reference is released here: only its token is kept.
                return Some((verdict, answer.ok().map(|window| ax::token(&window))));
            }
            if stop() {
                return None;
            }
        }
    }
}
