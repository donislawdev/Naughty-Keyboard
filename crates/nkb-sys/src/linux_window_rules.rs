//! What the windows of this program decide about the keyboard on a display that is
//! XWayland, without asking a server, so that it is tested on every system (`D145`
//! point 38, `OBS-218`, `OBS-221`).
//!
//! # What was measured before this was written
//!
//! GNOME 50, mutter 50.1, the palette as a client of XWayland, a native Wayland
//! application under test, keys and clicks through the portal (`tools/petla-palety-
//! wayland.py`, `OBS-221`). Whether a click on the palette takes the keyboard from the
//! application, and whether the keys typed after it arrive:
//!
//! | the palette's window | click takes the keyboard | keys after the click |
//! |---|---|---|
//! | frame of the system, hint `input = False` (`OBS-218`) | yes | lost |
//! | no frame, notification type, no hint | yes | lost 3 of 3 |
//! | no frame, normal type, hint turned off | yes, and at the start | lost |
//! | no frame, normal type, hint `input = False` | no | all arrive |
//! | no frame, notification type, hint `input = False` | no | all arrive (9 of 9, and the loop) |
//!
//! So mutter honours the hint when the window has no frame of the system's and ignores it
//! when there is one, and the type of the window does not matter to that. The type is the
//! palette's anyway, because it keeps the window out of the dock and the switcher, which a
//! window that never takes the keyboard has no business in.
//!
//! A window with a frame takes the keyboard when it is shown, and when it is hidden the
//! compositor gives the keyboard back to the application that had it. The client cannot
//! name that application and cannot give the keyboard to it.
//!
//! So a window with no frame must never hold the keyboard, which is read, and a window with
//! one holds it for as long as the tester uses it and loses it by being hidden, which
//! there is nothing to read before.

/// Whether a window holds the keyboard: the window manager says it is focused, or the
/// server's own input focus is in it. Either one is enough, because the two are told
/// at different moments (`OBS-218`: the state came before the focus, and the other way
/// round).
pub(crate) fn holds_keyboard(
    state: &[u32],
    focused_atom: u32,
    input_focus: Option<u32>,
    window: u32,
) -> bool {
    state.contains(&focused_atom) || input_focus == Some(window)
}

/// Whether a window has no frame of the system's, from `_NET_FRAME_EXTENTS` - left,
/// right, top and bottom, which the window manager writes for a window it has framed and
/// does not write for one it has not. A frame of nothing is no frame.
pub(crate) fn is_frameless(extents: Option<[u32; 4]>) -> bool {
    extents.is_none_or(|extents| extents.iter().all(|edge| *edge == 0))
}

/// The keyboard is held by a window that must not hold it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StillHeld;

/// What giving the keyboard back comes to on XWayland.
///
/// A window that does not hold it has nothing to give. One that holds it and has no frame
/// is the palette, which was made to never hold it, so that is the failure to report. One
/// that holds it and has a frame took it to be used, and it is the compositor that
/// returns the keyboard when the window is hidden (`OBS-221`).
pub(crate) fn hand_back(holds: bool, frameless: bool) -> Result<(), StillHeld> {
    if holds && frameless {
        Err(StillHeld)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FOCUSED: u32 = 7;
    const WINDOW: u32 = 0x00c0_0013;

    #[test]
    fn a_window_holds_the_keyboard_when_either_side_says_so() {
        assert!(holds_keyboard(&[3, FOCUSED], FOCUSED, None, WINDOW));
        assert!(holds_keyboard(&[3], FOCUSED, Some(WINDOW), WINDOW));
        assert!(holds_keyboard(&[FOCUSED], FOCUSED, Some(WINDOW), WINDOW));
    }

    #[test]
    fn a_window_does_not_hold_it_when_neither_side_says_so() {
        assert!(!holds_keyboard(&[], FOCUSED, None, WINDOW));
        assert!(!holds_keyboard(&[3, 4], FOCUSED, Some(WINDOW + 1), WINDOW));
        // The compositor's own window, which is where the focus rests while an application
        // of Wayland has the keyboard, is not ours.
        assert!(!holds_keyboard(&[], FOCUSED, Some(0x0060_0003), WINDOW));
    }

    #[test]
    fn a_window_the_manager_wrote_no_frame_for_has_none() {
        assert!(is_frameless(None));
        assert!(is_frameless(Some([0, 0, 0, 0])));
    }

    #[test]
    fn a_frame_on_any_one_side_is_a_frame() {
        assert!(!is_frameless(Some([1, 0, 0, 0])));
        assert!(!is_frameless(Some([0, 1, 0, 0])));
        assert!(!is_frameless(Some([0, 0, 37, 0])));
        assert!(!is_frameless(Some([0, 0, 0, 1])));
    }

    #[test]
    fn a_window_with_no_frame_that_holds_the_keyboard_is_the_failure() {
        assert_eq!(hand_back(true, true), Err(StillHeld));
    }

    #[test]
    fn nothing_else_is_a_failure_to_give_back() {
        // Holding nothing, whatever the frame, and holding it as a window that is used.
        assert_eq!(hand_back(false, true), Ok(()));
        assert_eq!(hand_back(false, false), Ok(()));
        assert_eq!(hand_back(true, false), Ok(()));
    }
}
