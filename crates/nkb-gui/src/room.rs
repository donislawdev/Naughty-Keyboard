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
use nkb_core::screens::{Point, Rect, frame, room_below};
use slint::ComponentHandle;
use slint::winit_030::WinitWindowAccessor;

use crate::place;

/// Tells the palette the room it has: from the top of its content to the
/// bottom of its screen's work area, asked at start, whenever its content
/// wants another height (`content-resized`) and, on Linux, at every move
/// ([`refit_on_move`]). Zero - what the view reads as "not
/// known" - where the system does not say, and the palette is then as tall as
/// its content, as it always was.
pub fn fit_palette(palette: &crate::Palette) {
    palette.set_room(below(palette.window()).unwrap_or(0.0));
}

/// Fits the palette again whenever winit reports it moved - on Linux, where the
/// window manager decides where a window stands only once it is shown, and may
/// move it again at any time (the reason, measured: `palette.slint`).
///
/// The palette's one filter of winit events on Linux: a second one would
/// silently replace it (`slint.md` 2.81). On macOS the palette's filter is
/// `focus::hand_back_on_press`.
#[cfg(target_os = "linux")]
pub fn refit_on_move(palette: &crate::Palette) {
    use slint::winit_030::EventResult;
    use slint::winit_030::winit::event::WindowEvent;
    let palette_weak = palette.as_weak();
    palette.window().on_winit_window_event(move |_, event| {
        if let WindowEvent::Moved(_) = event
            && let Some(palette) = palette_weak.upgrade()
        {
            fit_palette(&palette);
        }
        EventResult::Propagate
    });
}

/// The room from the top of the window's content to the bottom of the work
/// area of the screen it stands on, less the window manager's frame under the
/// content (`frame_below`), in the window's logical pixels - at least one,
/// because zero is what a view reads as not known. `None` when the system does
/// not say where the window or the work area is.
#[must_use]
pub fn below(window: &slint::Window) -> Option<f32> {
    let top = window
        .with_winit_window(|window| window.inner_position().ok())
        .flatten()?;
    let area = work_area_of(window)?;
    let room = room_below(top.y, area).saturating_sub(frame_below(window));
    Some(logical(room, window.scale_factor()).max(1.0))
}

/// The window manager's frame under the window's content, in physical pixels -
/// on Linux, where a window manager keeps the whole frame of a window inside
/// the work area and moves a window whose frame reaches past it. Measured
/// 2026-11-17 under openbox (Xvfb at 1280 x 800, `_NET_FRAME_EXTENTS` 1, 1, 22,
/// 1): with its content reaching the bottom of the work area, the palette had
/// its 1 px border below it, openbox moved it up a pixel as it grew, the move
/// fitted it a pixel taller, and it crept up the screen some 15 px a second
/// until it was as tall as its content. Zero before the window manager has
/// framed the window, and zero elsewhere: on Windows the outer size counts 7 px
/// of invisible border that may lie over the taskbar (`slint.md` 2.89), and on
/// macOS nothing frames the content from below.
fn frame_below(window: &slint::Window) -> u32 {
    if !cfg!(target_os = "linux") {
        return 0;
    }
    let Some((_, (_, height))) = content_and_frame(window) else {
        return 0;
    };
    window
        .with_winit_window(|window| {
            let (Ok(outer), Ok(inner)) = (window.outer_position(), window.inner_position()) else {
                return 0;
            };
            // What of the frame's height is not above the content is below it.
            u32::try_from(i64::from(inner.y) - i64::from(outer.y))
                .map_or(0, |above| height.saturating_sub(above))
        })
        .unwrap_or(0)
}

/// The height a window opened beside `anchor` may have whole, in its logical
/// pixels: the height of the work area of the anchor's screen, less the
/// window's own title bar and frame. At least one pixel, for the reason
/// [`below`] gives.
#[must_use]
pub fn beside(anchor: &slint::Window, window: &slint::Window) -> Option<f32> {
    let area = work_area_of(anchor)?;
    let frame = content_and_frame(window).map_or(0, |(_, (_, height))| height);
    let room = area.height.saturating_sub(frame);
    Some(logical(room, anchor.scale_factor()).max(1.0))
}

/// The size of `window`'s content and what the window manager's frame adds to
/// it on each axis, in physical pixels, as winit reports them - `None` where
/// winit does not back the window. Every frame of a window that the palette's
/// code reads comes from here: a frame winit's numbers cannot be counts as
/// none yet, as before the window manager has framed the window
/// (`nkb_core::screens::frame` says when winit gives such numbers - right after
/// a window is shown on X11, which is when the windows opened beside the
/// palette are placed and told their room).
pub(crate) fn content_and_frame(window: &slint::Window) -> Option<((u32, u32), (u32, u32))> {
    window.with_winit_window(|window| {
        let (outer, inner) = (window.outer_size(), window.inner_size());
        let content = (inner.width, inner.height);
        (content, frame((outer.width, outer.height), content))
    })
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
