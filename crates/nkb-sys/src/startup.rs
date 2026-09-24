//! Saying that the program failed before it had a window to say it in.
//!
//! # The problem this exists for
//!
//! `nkb-gui` carries `windows_subsystem = "windows"` so that a tester launching
//! the palette from the desktop does not get a black console window beside it
//! with the full path to the binary in its title (`OBS-101`). The attribute is
//! unconditional rather than release-only, because a release that behaves
//! differently from what a session looks at is the failure class of `OBS-83`.
//!
//! But Slint has no fallback from its hardware renderer to the software one when
//! the graphics context cannot be created: the error comes back out of `run()`
//! and, with no console, lands nowhere (`OBS-103`). A tester would see NOTHING,
//! which is the shape untouchable rule 1 forbids. So the console may only
//! disappear together with a visible channel, and this module is that channel.
//!
//! # What was measured before this was written, and what it corrected
//!
//! Probed 2026-09-22 with `tools/sonda-konsola`, Windows 11, a binary carrying
//! the same attribute, in two runs: one whose parent passed standard handles and
//! one whose parent passed none.
//!
//! | question | what `OBS-101` assumed | measured |
//! |---|---|---|
//! | does the attribute take the output away from a terminal launch | yes | **no** - inherited handles work, the text is seen |
//! | does Rust hold the stderr handle it took at process start | yes | **no** - `0x0` became `0xf0` right after `AttachConsole` |
//! | does `eprintln!` report having nowhere to write | - | **no** - it returns `Ok` with a null handle and the write is gone |
//! | is `WriteConsoleW` a universal route | - | **no** - zero bytes against a pipe |
//! | does `MessageBoxW` work with no graphics library in play | "it should" | **yes** - the window came up in a background process |
//!
//! Two of those change the design. Because a write to a null handle succeeds in
//! silence, the CODE cannot ask "did that get through" - the only honest question
//! is whether a handle exists at all, asked before writing. And because the
//! attribute does not take away an inherited handle, a terminal launch still
//! shows the error the ordinary way.
//!
//! # Why the box is not always shown
//!
//! 🔴 `MessageBoxW` BLOCKS until someone clicks it. Always showing it would hang
//! every unattended run - a CI job, a script, a session driving the binary - on a
//! window nobody is there to close. So the box is the fallback for having no text
//! channel at all, which is exactly the desktop launch it exists for.

/// Where the startup failure was actually said.
///
/// Returned rather than swallowed because untouchable rule 1 applies to this
/// code too: a run that did less than it promised has to say so. [`Self::Nowhere`]
/// is the case where nothing worked, and a caller that gets it knows the exit
/// code is the only thing left carrying the failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupChannel {
    /// Written to standard error - inherited from the parent, or attached from it.
    StandardError,
    /// Shown in a message box, because there was no text channel.
    MessageBox,
    /// Neither worked. The exit code is all that is left.
    Nowhere,
}

/// Says `text` by whatever channel exists, and reports which one that was.
///
/// `title` names the window when it comes to that. It is not used otherwise.
///
/// The order is fixed and each step earns its place: attach a parent console if
/// this process has none, then ask whether a standard error handle exists at
/// all, then write or show. Asking first is not caution - it is the only way to
/// know, because the write itself succeeds silently into nothing.
pub fn report_startup_failure(title: &str, text: &str) -> StartupChannel {
    platform::report(title, text)
}

#[cfg(windows)]
mod platform {
    use super::StartupChannel;
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::System::Console::{
        ATTACH_PARENT_PROCESS, AttachConsole, GetStdHandle, STD_ERROR_HANDLE,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};

    /// Whether standard error leads anywhere.
    ///
    /// A null handle means the parent passed none. `INVALID_HANDLE_VALUE` means
    /// the request itself failed. Both mean "do not write", and they are checked
    /// separately from each other only in the probe - here the answer is one bit.
    fn has_standard_error() -> bool {
        let handle = unsafe { GetStdHandle(STD_ERROR_HANDLE) };
        !handle.is_null() && handle != INVALID_HANDLE_VALUE
    }

    /// Borrows the parent's console, if this process has none and the parent has one.
    ///
    /// The result is deliberately ignored. Failure here is the ordinary case in
    /// two different situations - this process already has a console, or the
    /// parent has none - and neither is something to report. What matters is the
    /// handle check afterwards, which answers the real question either way.
    fn attach_parent_console() {
        unsafe {
            AttachConsole(ATTACH_PARENT_PROCESS);
        }
    }

    fn show_box(title: &str, text: &str) -> bool {
        let title: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
        let text: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        // A null owner window is right here: there is no window of ours to own
        // it, which is the whole situation.
        let answer = unsafe {
            MessageBoxW(
                std::ptr::null_mut(),
                text.as_ptr(),
                title.as_ptr(),
                MB_OK | MB_ICONERROR,
            )
        };
        answer != 0
    }

    pub fn report(title: &str, text: &str) -> StartupChannel {
        attach_parent_console();
        if has_standard_error() {
            // Not `eprintln!`: a panic inside the error path would replace one
            // silent failure with a louder one. `write` returns a result and the
            // result is looked at.
            use std::io::Write;
            let mut stream = std::io::stderr();
            if writeln!(stream, "{text}").is_ok() {
                return StartupChannel::StandardError;
            }
        }
        if show_box(title, text) {
            StartupChannel::MessageBox
        } else {
            StartupChannel::Nowhere
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use super::StartupChannel;

    /// macOS and Linux do not take the output away: there is no
    /// `windows_subsystem` equivalent, a GUI binary launched from a terminal
    /// keeps its streams, and one launched from a desktop has them pointed at
    /// the system log. So standard error is the whole answer here, and a
    /// message box would be a second window library to carry for no gain.
    ///
    /// This will need revisiting the day the palette runs on macOS - `OBS-70`
    /// blocks that today - because a launch from Finder puts the text somewhere
    /// a tester will not look.
    pub fn report(_title: &str, text: &str) -> StartupChannel {
        use std::io::Write;
        let mut stream = std::io::stderr();
        if writeln!(stream, "{text}").is_ok() {
            StartupChannel::StandardError
        } else {
            StartupChannel::Nowhere
        }
    }
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
    fn reporting_says_which_channel_carried_it() {
        // Under `cargo test` the parent passes standard handles, so this is the
        // terminal case and the answer must be the text one. The desktop case -
        // no handles at all - cannot be reached from inside a test process that
        // has them. It is measured by `tools/sonda-konsola`, which runs a real
        // binary both ways, and that is said here so the gap is not mistaken for
        // coverage.
        let channel = report_startup_failure("Naughty Keyboard", "startup probe, not a failure");
        assert_eq!(
            channel,
            StartupChannel::StandardError,
            "a test process has inherited handles, so the text channel must be the one used"
        );
    }

    #[test]
    fn an_empty_message_still_picks_a_channel() {
        // Degenerate but reachable: a platform error whose display is empty.
        // The channel must still be chosen and reported rather than the call
        // falling through to `Nowhere` on a technicality.
        let channel = report_startup_failure("", "");
        assert_ne!(
            channel,
            StartupChannel::Nowhere,
            "an empty text is still a report, and the channel exists either way"
        );
    }
}
