//! The nkb palette.
//!
//! A full product in its own right, not a windowless CLI with a window bolted
//! on - D25. It shares `core` and `app` with `nkb` and shares the same catalogue
//! on disk, and neither executable can start the other.
//!
//! Today it opens the gallery: every component in every state, which is what
//! document 13 section 3 asks for and what a session reads to find out what the
//! vocabulary already contains. The palette itself comes next.
//!
//! # No console window, and what had to exist first
//!
//! The attribute below stops Windows from giving this process a console. Without
//! it, a tester launching the palette from the desktop gets a black window
//! beside it with the full path to the binary in its title (`OBS-101`).
//!
//! 🔴 It is UNCONDITIONAL rather than release-only, and that is deliberate: a
//! release that behaves differently from what a session looks at is the failure
//! class of `OBS-83`, where the thing being judged is not the thing being
//! shipped.
//!
//! The attribute could not go in on its own. Slint has no runtime fallback from
//! its hardware renderer to the software one, so a machine without a graphics
//! context gets a `PlatformError` back out of `run()` and nothing else
//! (`OBS-103`). Before this change that error reached a console; after it, with
//! no channel of its own, it would reach NOBODY - which is exactly the silence
//! untouchable rule 1 forbids. So the console disappears in the same commit as
//! `report_window_failure`, which says the failure through standard error when
//! there is one and through a message box when there is not.
//!
//! ⚠️ What this still does NOT do: try the software renderer by itself. Which
//! route Slint 1.17 allows after a failed `run()` is unmeasured - the platform
//! is set once per process - and there is no machine here without OpenGL 2 to
//! measure it on. The gap is named in `OBS-103` and the sentence the tester sees
//! names the environment variable instead. Minimum of rule 1 is to SHOW the
//! failure, not to repair it quietly.
#![cfg_attr(windows, windows_subsystem = "windows")]

use std::process::ExitCode;

use nkb_adapters::report_window_failure;
use nkb_gui::Gallery;
use slint::ComponentHandle;

fn main() -> ExitCode {
    match start() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            // The channel is reported rather than ignored, but there is nothing
            // left to do with `Nowhere` except exit non-zero - which is why the
            // exit code below does not depend on it. Untouchable rule 1 is
            // satisfied by having tried every channel there is, in the order
            // that does not block an unattended run.
            let _channel = report_window_failure(&error.to_string());
            // The same code this binary returned before the attribute went in,
            // so nobody's script changes behaviour. `ux-spec.md` 10 governs the
            // CLI's table and is untouched: this is the other executable.
            ExitCode::FAILURE
        }
    }
}

/// Everything that can fail before there is a window to fail in.
fn start() -> Result<(), slint::PlatformError> {
    Gallery::new()?.run()
}
