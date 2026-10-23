//! The palette's own frame, where the system gives its window none (`D145` point 38).
//!
//! On a Wayland session the palette is a notification of XWayland, which is the one
//! way found to keep a click on it from taking the keyboard from the field under test
//! (`nkb-sys`, `linux_window_rules.rs`). Such a window has no title bar: nothing to
//! move it by and nothing to close it with. Both are the palette's own, in its header.
//!
//! # Moving
//!
//! The header reports where the pointer was when it pressed and where it is now, both in
//! the header's own coordinates. Those coordinates move with the window, so after a move
//! the pointer is again where it pressed, and the difference of the two is how far the
//! window has to go from where it stands. The window is moved by the toolkit, which on X
//! asks the server directly: the window manager is not asked, and mutter lets a
//! notification be placed this way (`OBS-218`, point 5).
//!
//! # Closing
//!
//! The same request as the close button of a frame would make, so every window of the
//! palette goes the way it goes on any other system, and the place is remembered as it is
//! there (`main.rs`).

use slint::platform::WindowEvent;
use slint::{ComponentHandle, PhysicalPosition};

use nkb_adapters::i18n::{self, PaletteLabel};

use crate::Palette;

/// Where the window goes, in physical pixels, for a pointer that pressed the header at
/// `pressed` and is at `now` (logical pixels, the header's coordinates), when the window
/// stands at `at` and the scale of the screen is `scale`.
pub(crate) fn dragged_to(
    at: (i32, i32),
    scale: f32,
    pressed: (f32, f32),
    now: (f32, f32),
) -> (i32, i32) {
    let step = |from: f32, to: f32| ((to - from) * scale).round() as i32;
    (
        at.0.saturating_add(step(pressed.0, now.0)),
        at.1.saturating_add(step(pressed.1, now.1)),
    )
}

/// Makes the palette draw a frame of its own: the header moves the window, and the
/// button at its end closes it. Called once, before the window is shown.
pub fn wire(palette: &Palette) {
    palette.set_own_frame(true);
    palette.set_close_label(i18n::label(PaletteLabel::ClosePalette).into());
    palette.on_drag_header({
        let palette = palette.as_weak();
        move |pressed_x, pressed_y, now_x, now_y| {
            let Some(palette) = palette.upgrade() else {
                return;
            };
            let window = palette.window();
            let at = window.position();
            let (x, y) = dragged_to(
                (at.x, at.y),
                window.scale_factor(),
                (pressed_x, pressed_y),
                (now_x, now_y),
            );
            if (x, y) != (at.x, at.y) {
                window.set_position(PhysicalPosition::new(x, y));
            }
        }
    });
    palette.on_close_palette({
        let palette = palette.as_weak();
        move || {
            if let Some(palette) = palette.upgrade() {
                // Through the window's own close request, so the handler that remembers the
                // place and closes the other windows runs, as it does for a frame's button.
                palette.window().dispatch_event(WindowEvent::CloseRequested);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pointer_that_has_not_moved_leaves_the_window_where_it_is() {
        assert_eq!(
            dragged_to((100, 50), 1.0, (30.0, 8.0), (30.0, 8.0)),
            (100, 50)
        );
    }

    #[test]
    fn the_window_goes_as_far_as_the_pointer_has_gone_from_where_it_pressed() {
        assert_eq!(
            dragged_to((100, 50), 1.0, (30.0, 8.0), (45.0, 2.0)),
            (115, 44)
        );
    }

    #[test]
    fn the_distance_is_scaled_to_physical_pixels() {
        // Two pixels of the screen for each of the toolkit's, and the half rounds away.
        assert_eq!(dragged_to((0, 0), 2.0, (0.0, 0.0), (3.5, -1.0)), (7, -2));
        assert_eq!(dragged_to((0, 0), 1.5, (0.0, 0.0), (1.0, 0.0)), (2, 0));
    }

    #[test]
    fn a_window_cannot_be_sent_past_the_end_of_the_numbers() {
        assert_eq!(
            dragged_to((i32::MAX - 1, i32::MIN + 1), 1.0, (0.0, 0.0), (10.0, -10.0)),
            (i32::MAX, i32::MIN)
        );
    }
}
