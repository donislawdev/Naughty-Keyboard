//! Saying that the palette could not start, in words, through the system.
//!
//! # Why this thin file exists rather than a direct call
//!
//! `nkb-gui` depends on `core`, `app` and this package - not on `nkb-sys`. That
//! is not an accident of what anyone needed so far: `architektura.md` 2.3 makes
//! `gui` an ADAPTER, and an adapter that calls the operating system directly has
//! stopped being one. The alternative was to add `nkb-sys` to the graphical
//! package and widen the explicit allow-list in `tools/sprawdz-warstwy.ps1`,
//! which would have bought nothing and opened the door for every later call.
//!
//! So this file does what `keyboard.rs` does one layer over: it turns a system
//! call into something a layer above can use without knowing there is a system.
//! Here it also joins the two halves that must not drift apart - the SENTENCE,
//! which comes from the dictionary, and the CHANNEL, which comes from
//! `nkb-sys` - so that no caller can pick one and invent the other.

use nkb_sys::startup::{StartupChannel, report_startup_failure};

use crate::i18n::{self, Startup};

/// The product's own name, as it appears on a window.
///
/// Not a translation key. A name is a name in every language, and untouchable
/// rule 9 is about text that says something, not about what the thing is called.
const PRODUCT_NAME: &str = "Naughty Keyboard";

/// Says that the window could not be created, and reports how it was said.
///
/// `reason` is the graphics library's own error text, passed through unchanged:
/// it is what a tester will paste into a bug report, and shortening it here
/// would take away the only specific thing in the sentence.
#[must_use]
pub fn report_window_failure(reason: &str) -> StartupChannel {
    let text = i18n::startup_failure(Startup::WindowFailed, reason);
    report_startup_failure(PRODUCT_NAME, &text)
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;

    #[test]
    fn the_sentence_carries_the_reason_it_was_given() {
        // The join this file exists for: the text must be the dictionary's, with
        // the library's own words inside it. A caller building its own sentence
        // is exactly what this prevents, so the check is that the two halves are
        // both present.
        let text = i18n::startup_failure(Startup::WindowFailed, "Cannot create OpenGL context");
        assert!(
            text.contains("Cannot create OpenGL context"),
            "the library's reason must survive into the sentence: {text}"
        );
        assert!(
            text.contains("SLINT_BACKEND=winit-software"),
            "the one thing a tester can actually do must be in the sentence: {text}"
        );
    }

    #[test]
    fn reporting_picks_a_channel_under_test() {
        // A test process has inherited handles, so this is the terminal case.
        // The desktop case is measured by tools/sonda-konsola against a real
        // binary. It cannot be reached from inside a process that has handles,
        // and saying so here stops this test from looking like it covers both.
        assert_eq!(
            report_window_failure("startup probe, not a failure"),
            StartupChannel::StandardError
        );
    }
}
