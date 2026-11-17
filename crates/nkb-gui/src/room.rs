//! The room a screen has for a window as tall as its content (`D190`,
//! `frame.slint`): the palette and the shortcuts window are never taller than
//! the part of their screen windows may use - the screen less the taskbar, the
//! menu bar, the Dock or a panel.
//!
//! # One unit: the physical pixels the toolkit reports
//!
//! The toolkit reports where a window stands and the screen it stands on in
//! physical pixels, and `on_request::place_beside_palette` places a window in
//! them. The system's work area comes in the unit of the layout of screens
//! (`nkb_adapters::screens`): physical pixels on Windows and on Linux, points on
//! macOS - so there it is multiplied by the window's scale on the way in, and
//! everything is worked out in one unit. The room handed to a view is in its
//! logical pixels: divided by that scale on the way out.
//!
//! # Met with the screen
//!
//! Linux gives one work area for the whole desktop (`_NET_WORKAREA`), and a
//! window's screen is the toolkit's to know. So the work area is met with the
//! screen the window stands on - on every system, where it changes nothing.

use nkb_adapters::screens::{IN_POINTS, work_area};
use nkb_core::screens::{Point, Rect, room_below};
use slint::ComponentHandle;
use slint::winit_030::WinitWindowAccessor;

use crate::place;

/// Tells the palette the room it has: from the top of its content to the
/// bottom of its screen's work area, asked at start and whenever its content
/// wants another height (`content-resized`). Zero - what the view reads as "not
/// known" - where the system does not say, and the palette is then as tall as
/// its content, as it always was.
pub fn fit_palette(palette: &crate::Palette) {
    palette.set_room(below(palette.window()).unwrap_or(0.0));
}

/// The room from the top of the window's content to the bottom of the work
/// area of the screen it stands on, in the window's logical pixels - at least
/// one, because zero is what a view reads as not known. `None` when the system
/// does not say where the window or the work area is.
#[must_use]
pub fn below(window: &slint::Window) -> Option<f32> {
    let top = window
        .with_winit_window(|window| window.inner_position().ok())
        .flatten()?;
    let area = work_area_of(window)?;
    let room = room_below(top.y, area);
    Some(logical(room, window.scale_factor()).max(1.0))
}

/// The height a window opened beside `anchor` may have whole, in its logical
/// pixels: the height of the work area of the anchor's screen, less the
/// window's own title bar and frame. At least one pixel, for the reason
/// [`below`] gives.
#[must_use]
pub fn beside(anchor: &slint::Window, window: &slint::Window) -> Option<f32> {
    let area = work_area_of(anchor)?;
    let frame = window
        .with_winit_window(|window| {
            window
                .outer_size()
                .height
                .saturating_sub(window.inner_size().height)
        })
        .unwrap_or(0);
    let room = area.height.saturating_sub(frame);
    Some(logical(room, anchor.scale_factor()).max(1.0))
}

/// The work area of the screen `window` stands on, in physical pixels - the
/// system's answer for a point inside the window's content, met with that
/// screen as the toolkit reports it.
#[must_use]
pub fn work_area_of(window: &slint::Window) -> Option<Rect> {
    let scale = window.scale_factor();
    let inner = window
        .with_winit_window(|window| window.inner_position().ok())
        .flatten()?;
    let at = place::kept(slint::PhysicalPosition::new(inner.x, inner.y), scale);
    let area = physical(work_area(Point { x: at.x, y: at.y })?, scale, IN_POINTS);
    let screen = window
        .with_winit_window(|window| window.current_monitor())
        .flatten()
        .map(|monitor| {
            let (corner, size) = (monitor.position(), monitor.size());
            Rect {
                x: corner.x,
                y: corner.y,
                width: size.width,
                height: size.height,
            }
        });
    Some(screen.and_then(|screen| area.meet(screen)).unwrap_or(area))
}

/// A rectangle of the layout's unit in physical pixels: as it is where that
/// unit is the pixel, multiplied by the scale where it is the point.
fn physical(area: Rect, scale: f32, points: bool) -> Rect {
    if !points {
        return area;
    }
    let scale = if scale.is_finite() && scale > 0.0 {
        f64::from(scale)
    } else {
        1.0
    };
    let coordinate = |value: i32| (f64::from(value) * scale).round() as i32;
    let length = |value: u32| (f64::from(value) * scale).round() as u32;
    Rect {
        x: coordinate(area.x),
        y: coordinate(area.y),
        width: length(area.width),
        height: length(area.height),
    }
}

/// Physical pixels as a view's logical ones.
fn logical(physical: u32, scale: f32) -> f32 {
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    // A room is a screen's height at most, far inside what `f32` holds exactly.
    #[allow(clippy::cast_precision_loss)]
    let physical = physical as f32;
    physical / scale
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_work_area_in_points_is_multiplied_by_the_scale_and_one_in_pixels_is_not() {
        let area = Rect {
            x: -1920,
            y: 37,
            width: 1512,
            height: 875,
        };
        assert_eq!(
            physical(area, 2.0, true),
            Rect {
                x: -3840,
                y: 74,
                width: 3024,
                height: 1750
            }
        );
        assert_eq!(physical(area, 2.0, false), area);
        // A scale that is no positive number is taken as one.
        assert_eq!(physical(area, f32::NAN, true), area);
        assert_eq!(physical(area, 0.0, true), area);
    }

    #[test]
    fn a_room_in_pixels_is_a_room_in_the_views_logical_pixels() {
        assert!((logical(1080, 1.5) - 720.0).abs() < f32::EPSILON);
        assert!((logical(720, 1.0) - 720.0).abs() < f32::EPSILON);
        assert!((logical(720, f32::INFINITY) - 720.0).abs() < f32::EPSILON);
    }
}
