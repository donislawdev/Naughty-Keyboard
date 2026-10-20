//! Letting the windows of this program stand in the space of an application in
//! full screen (macOS).
//!
//! # The problem, as measured
//!
//! An application in full screen has a space of its own, and a window made
//! before that is left in the space it was made in. Measured 2026-10-20 on
//! macOS 27.0 with Safari: the list of windows on the screen held the palette
//! once before the full screen and not at all in it, while the shortcut was heard
//! and the value reached the field. The tester saw no value, no counter and no
//! message (`OBS-193`).
//!
//! # What this does, and why it is not a call on a window
//!
//! It puts the collection behaviour "every space, and with a full screen window"
//! on EVERY window this program has (`macos_rules::over_full_screen`). The windows
//! of the process are all ours, so no window is named, and there is nothing to be
//! handed that could be wrong: a pointer to a view passed across the layers would
//! have been the only way to name one, and the one thing here that can end the
//! process is a wrong pointer. The toolkit does not offer the setting (winit
//! 0.30.13).
//!
//! The palette asks once, when its window exists. The windows opened on request
//! do not: each opening makes a NEW system window (`OBS-80`), made in the space
//! that is active, and measured 2026-10-21 twice in a row with Safari in full
//! screen, such a window stood there with the keyboard and Safari stayed on the
//! screen.
//!
//! The verdict is the READ of the behaviour afterwards, as everywhere in this
//! crate: the call returning is not the window standing in the space.
//!
//! Main thread only, because AppKit's windows belong to it. Where the window
//! system has no such spaces nothing is done and nothing is claimed.

/// Why the windows could not be let into the space of a full screen application.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpacesError {
    /// Asked from a thread that does not own the windows.
    NotMainThread,
    /// The window toolkit's application object could not be found, so there are
    /// no windows to speak of.
    NoApplication,
    /// The program has no window yet - asked before the first one was made.
    NoWindow,
    /// The behaviour was written and the read-back does not show it, on
    /// `failed` of the `windows` the program has.
    NotKept { failed: usize, windows: usize },
}

impl core::fmt::Display for SpacesError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NotMainThread => write!(
                f,
                "it was asked from a thread that does not own the windows"
            ),
            Self::NoApplication => write!(
                f,
                "the application object of the window toolkit could not be found"
            ),
            Self::NoWindow => write!(f, "the program has no window yet"),
            Self::NotKept { failed, windows } => write!(
                f,
                "the system kept the old setting on {failed} of {windows} windows"
            ),
        }
    }
}

/// Lets every window of this program stand in the space of an application in
/// full screen, and answers how many it did that for.
///
/// Idempotent: a window that already stands there is written the same value and
/// read back. Call it on the main thread once the window to be let in is shown.
/// A window made later is not covered, so the caller asks again for it.
///
/// Where there are no such spaces (everything but macOS) nothing is done and the
/// answer is `Ok(0)`.
///
/// # Errors
///
/// [`SpacesError::NoWindow`] when the program has none yet,
/// [`SpacesError::NotKept`] when the read-back does not show the behaviour, and
/// the two reasons the windows could not be asked at all.
pub fn stand_over_full_screen() -> Result<usize, SpacesError> {
    #[cfg(target_os = "macos")]
    {
        crate::macos::spaces::stand_over_full_screen()
    }
    #[cfg(not(target_os = "macos"))]
    {
        Ok(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_reason_names_what_went_wrong_and_the_counts_are_in_the_sentence() {
        let text = SpacesError::NotKept {
            failed: 1,
            windows: 3,
        }
        .to_string();
        assert!(text.contains("1 of 3 windows"), "got: {text}");
        assert!(SpacesError::NoWindow.to_string().contains("no window"));
        assert!(
            SpacesError::NotMainThread
                .to_string()
                .contains("thread that does not own")
        );
        assert!(
            SpacesError::NoApplication
                .to_string()
                .contains("application")
        );
    }

    #[test]
    fn where_there_are_no_such_spaces_nothing_is_done_and_nothing_claimed() {
        if cfg!(not(target_os = "macos")) {
            assert_eq!(stand_over_full_screen(), Ok(0));
        }
    }
}
