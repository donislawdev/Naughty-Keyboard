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

// ---------------------------------------------------------------------------
// Standing aside from a window of the system (`D145` point 77, `OBS-232`)
// ---------------------------------------------------------------------------
//
// Measured 2026-10-25 on GNOME 50, mutter 50.1: the palette over the window of the portal
// that asks whether keys may be pressed, the verdict a screenshot of the machine and the
// state of the window read from the server (`tools/sondy/linux/pomiar-zejscia-palety.py`):
//
// | what was done to the palette | the window of the portal, opened before it |
// |---|---|
// | `_NET_WM_STATE_ABOVE` taken off | covered (the notification type keeps the layer) |
// | type `NORMAL`, then ABOVE off, 1.2 s after the window opened | covered |
// | the same, and the window lowered | covered |
// | state `BELOW` | uncovered, and the palette hidden behind the application too |
// | type `NORMAL` and ABOVE off BEFORE the window opened | uncovered, the palette above the application |
//
// So a window that is moved to the normal layer is put on top of it, above a window that
// is there already, and one that is in it before the window opens is below that window.
// The type of the window is what holds the palette in the top layer, not the state, and
// the type has to be changed along with it.

/// What a window stood as before it stepped aside, so that stepping back puts that back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Stood {
    /// `_NET_WM_WINDOW_TYPE` as it was, the atoms in the order they were in.
    pub types: Vec<u32>,
    /// Whether `_NET_WM_STATE_ABOVE` was in `_NET_WM_STATE`.
    pub above: bool,
}

/// What the window stands as now, if it stands aside: only the normal type, and not above.
pub(crate) fn is_aside(types: &[u32], state: &[u32], normal: u32, above: u32) -> bool {
    types == [normal] && !state.contains(&above)
}

/// Whether the window stands as it stood before it stepped aside.
pub(crate) fn is_back(types: &[u32], state: &[u32], stood: &Stood, above: u32) -> bool {
    types == stood.types.as_slice() && state.contains(&above) == stood.above
}

#[cfg(test)]
mod tests {
    use super::*;

    const FOCUSED: u32 = 7;
    const WINDOW: u32 = 0x00c0_0013;
    const NORMAL: u32 = 31;
    const NOTIFICATION: u32 = 32;
    const ABOVE: u32 = 41;
    const SKIP: u32 = 42;

    fn stood() -> Stood {
        Stood {
            types: vec![NOTIFICATION],
            above: true,
        }
    }

    #[test]
    fn a_window_is_aside_when_it_is_normal_and_not_above() {
        assert!(is_aside(&[NORMAL], &[], NORMAL, ABOVE));
        assert!(is_aside(&[NORMAL], &[SKIP, 5], NORMAL, ABOVE));
    }

    #[test]
    fn a_window_that_keeps_either_the_type_or_the_state_is_not_aside() {
        // Each of the two alone: the measurement showed neither is enough.
        assert!(!is_aside(&[NORMAL], &[ABOVE], NORMAL, ABOVE));
        assert!(!is_aside(&[NOTIFICATION], &[], NORMAL, ABOVE));
        assert!(!is_aside(&[NOTIFICATION], &[ABOVE], NORMAL, ABOVE));
        assert!(!is_aside(&[], &[], NORMAL, ABOVE));
        assert!(
            !is_aside(&[NORMAL, NOTIFICATION], &[], NORMAL, ABOVE),
            "a list of types that still names the old one is not the normal type"
        );
    }

    #[test]
    fn a_window_is_back_when_it_has_its_types_and_its_state_again() {
        assert!(is_back(&[NOTIFICATION], &[ABOVE, SKIP], &stood(), ABOVE));
        let never_above = Stood {
            types: vec![NOTIFICATION],
            above: false,
        };
        assert!(is_back(&[NOTIFICATION], &[SKIP], &never_above, ABOVE));
    }

    #[test]
    fn a_window_that_has_only_part_of_what_it_had_is_not_back() {
        assert!(!is_back(&[NORMAL], &[ABOVE], &stood(), ABOVE));
        assert!(!is_back(&[NOTIFICATION], &[], &stood(), ABOVE));
        assert!(!is_back(&[NOTIFICATION, NORMAL], &[ABOVE], &stood(), ABOVE));
        let never_above = Stood {
            types: vec![NOTIFICATION],
            above: false,
        };
        assert!(
            !is_back(&[NOTIFICATION], &[ABOVE], &never_above, ABOVE),
            "above is not put on a window that did not have it"
        );
    }

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
