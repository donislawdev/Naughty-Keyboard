//! Where the palette stood, in the unit the layout of screens is in (`D143`).
//!
//! The layout and the places remembered on it are in physical pixels on Windows
//! and in points on macOS (`nkb_sys::screens`). The toolkit speaks physical
//! pixels when it is asked for a window's position, and takes either kind when
//! it is told one. These two functions are the whole of the difference:
//!
//! - [`kept`] turns the position the toolkit reports into the unit of the layout:
//!   as it is on Windows, divided by the scale of the window on macOS, where the
//!   toolkit's physical position is the position in points times that scale
//!   (winit 0.30.13, `platform_impl/macos/window_delegate.rs`, `outer_position`).
//! - [`restore`] turns a place of the layout into what the toolkit is told to
//!   place a window at: physical on Windows, LOGICAL on macOS, which winit takes
//!   as points with no scale applied. A physical position given before the window
//!   exists would be divided by the scale of the main display, whichever display
//!   the place is on.
//!
//! # Which corner of the window
//!
//! A position given to a window that does not exist yet is not the position the
//! window reports once it does, on macOS. winit creates the window from a CONTENT
//! rectangle, so the corner it is told is the top left of the content, with the
//! title bar above it (`window_delegate.rs`, `new_window`), while `position()` is
//! the corner of the whole frame. Measured 2026-10-20 (macOS 27.0): a palette
//! moved to y = 307 and closed was restored at y = 275, 32 points higher, which
//! is the title bar. So the place kept on macOS is the corner of the content
//! ([`standing`]), the same corner [`restore`] hands back at creation. On Windows
//! both are the corner of the outer frame.
//!
//! The conversions need no window, so both directions are checked without a
//! screen.

use nkb_adapters::screens::IN_POINTS;
use nkb_core::screens::Point;
use slint::winit_030::WinitWindowAccessor;
use slint::{LogicalPosition, PhysicalPosition, Window, WindowPosition};

/// Where a window stands now, as a place of the layout: the corner of the outer
/// frame on Windows, the corner of the content on macOS (the header says why),
/// in the unit of the layout.
///
/// Needs a window that exists, so it is asked while the palette is open. A window
/// the toolkit does not back with a winit one, or one that cannot say, is taken
/// at the corner of its frame.
#[must_use]
pub fn standing(window: &Window) -> Point {
    let inner = IN_POINTS
        .then(|| window.with_winit_window(|window| window.inner_position().ok()))
        .flatten()
        .flatten()
        .map(|inner| PhysicalPosition::new(inner.x, inner.y));
    kept(
        inner.unwrap_or_else(|| window.position()),
        window.scale_factor(),
    )
}

/// The place a window stands at, in the unit of the layout, from the position the
/// toolkit reports for it and the scale the window has now.
#[must_use]
pub fn kept(position: PhysicalPosition, scale: f32) -> Point {
    kept_in(position, scale, IN_POINTS)
}

/// What the toolkit is told to place the window at, for a place of the layout.
#[must_use]
pub fn restore(at: Point) -> WindowPosition {
    restore_in(at, IN_POINTS)
}

fn kept_in(position: PhysicalPosition, scale: f32, points: bool) -> Point {
    if !points {
        return Point {
            x: position.x,
            y: position.y,
        };
    }
    // A scale that is no positive number is not one to divide by: the window is
    // then taken at one point per pixel, which is what a display with no scaling
    // is.
    let scale = if scale.is_finite() && scale > 0.0 {
        f64::from(scale)
    } else {
        1.0
    };
    Point {
        x: in_points(position.x, scale),
        y: in_points(position.y, scale),
    }
}

/// A physical coordinate in points: the nearest whole one.
fn in_points(physical: i32, scale: f64) -> i32 {
    // A division of a value that fits `i32` by a scale of at least one point per
    // pixel stays in range, and `as` saturates for the rest.
    (f64::from(physical) / scale).round() as i32
}

fn restore_in(at: Point, points: bool) -> WindowPosition {
    if points {
        // A whole coordinate of a screen is far inside what `f32` holds exactly.
        WindowPosition::Logical(LogicalPosition::new(at.x as f32, at.y as f32))
    } else {
        WindowPosition::Physical(PhysicalPosition::new(at.x, at.y))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn on_windows_the_place_is_the_position_the_toolkit_reports() {
        assert_eq!(
            kept_in(PhysicalPosition::new(-1920, 180), 1.5, false),
            Point { x: -1920, y: 180 }
        );
    }

    #[test]
    fn on_macos_the_place_is_the_position_in_points() {
        // A display with two pixels to a point: the toolkit says 2400, 1600.
        assert_eq!(
            kept_in(PhysicalPosition::new(2400, 1600), 2.0, true),
            Point { x: 1200, y: 800 }
        );
        // One pixel to a point: nothing to convert.
        assert_eq!(
            kept_in(PhysicalPosition::new(401, 303), 1.0, true),
            Point { x: 401, y: 303 }
        );
        // A screen left of the main one and a window pushed above its top.
        assert_eq!(
            kept_in(PhysicalPosition::new(-3840, -50), 2.0, true),
            Point { x: -1920, y: -25 }
        );
        // An odd physical coordinate on a display with two pixels to a point is
        // the nearest whole point.
        assert_eq!(
            kept_in(PhysicalPosition::new(2401, 1599), 2.0, true),
            Point { x: 1201, y: 800 }
        );
    }

    #[test]
    fn a_scale_that_is_not_a_positive_number_is_one_pixel_to_a_point() {
        for scale in [0.0, -2.0, f32::NAN, f32::INFINITY] {
            assert_eq!(
                kept_in(PhysicalPosition::new(300, 200), scale, true),
                Point { x: 300, y: 200 },
                "{scale}"
            );
        }
    }

    #[test]
    fn the_place_goes_back_to_the_toolkit_in_the_unit_it_takes_for_the_system() {
        let at = Point { x: -1920, y: 180 };
        assert!(matches!(
            restore_in(at, false),
            WindowPosition::Physical(position) if position.x == -1920 && position.y == 180
        ));
        assert!(matches!(
            restore_in(at, true),
            WindowPosition::Logical(position)
                if (position.x + 1920.0).abs() < f32::EPSILON && (position.y - 180.0).abs() < f32::EPSILON
        ));
    }

    /// The two calls the palette makes follow the system it runs on: points on
    /// macOS, physical pixels elsewhere.
    #[test]
    fn the_unit_follows_the_system() {
        let at = Point { x: 10, y: 20 };
        assert_eq!(
            matches!(restore(at), WindowPosition::Logical(_)),
            cfg!(target_os = "macos")
        );
        // With one pixel to a point the two units are the same number.
        assert_eq!(kept(PhysicalPosition::new(10, 20), 1.0), at);
    }

    /// The round trip the palette makes: reported at closing, told at the next
    /// start. On macOS the toolkit's physical position is the point times the
    /// scale, so what comes back is the place that was kept.
    #[test]
    fn a_place_kept_and_restored_on_macos_is_the_same_place() {
        for scale in [1.0_f32, 2.0, 3.0] {
            for at in [
                Point { x: 0, y: 25 },
                Point { x: 401, y: 303 },
                Point { x: -1920, y: -120 },
            ] {
                let physical = PhysicalPosition::new(
                    (f64::from(at.x) * f64::from(scale)).round() as i32,
                    (f64::from(at.y) * f64::from(scale)).round() as i32,
                );
                assert_eq!(kept_in(physical, scale, true), at, "scale {scale}");
            }
        }
    }
}
