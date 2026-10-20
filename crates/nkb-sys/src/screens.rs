//! The screens of this machine, in the unit windows are placed in (UX7,
//! `UX-GUI-012`, `D143`).
//!
//! # Why the system is asked and not the window library
//!
//! The palette is put where the tester left it on this layout of screens, and
//! that has to be known BEFORE its window exists. Slint 1.18 names no screen to
//! its application, and winit lists them only inside a running event loop.
//!
//! # The unit
//!
//! On Windows, physical pixels. `EnumDisplayMonitors` reports rectangles in the
//! coordinates of the calling thread's DPI awareness. A thread that is not
//! aware per monitor gets them scaled, and a place remembered in one unit and
//! compared in the other would land somewhere else. winit makes the process
//! aware per monitor when its event loop is created (winit 0.30.13,
//! `platform_impl/windows/event_loop.rs`), which in Slint happens with the first
//! window - so the palette asks after building it, and this module answers
//! `None` rather than scaled rectangles when the thread is not aware per
//! monitor.
//!
//! On macOS, points. The desktop there is one space of points, with the origin
//! at the top left corner of the main display (`CGDisplayBounds`), and a
//! display's scale is how many pixels one point has on THAT display. A layout
//! in physical pixels would need a different factor on every display and would
//! stop being one rectangle space. A window is placed in points too (winit
//! 0.30.13, `platform_impl/macos/window_delegate.rs`, `set_outer_position`), so
//! the place and the screens stay in one unit with no scale in between.
//!
//! Nothing is read from any window: a screen's rectangle is all that is asked.

/// One screen's rectangle on the virtual desktop, in the unit of the
/// module header: physical pixels on Windows, points on macOS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Monitor {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// The screens, in the order the system lists them - or `None` when they
/// could not be listed in that unit, or on a system where this module does not
/// ask (Linux, for now). `None` means "no layout to remember a place for",
/// never "no screen".
#[must_use]
pub fn monitors() -> Option<Vec<Monitor>> {
    platform::monitors()
}

/// More screens than any machine this tool has met. A longer list is not
/// trusted, the same rule as `layout::LAYOUT_LIMIT`.
#[cfg(any(windows, target_os = "macos"))]
const MONITOR_LIMIT: usize = 64;

/// A coordinate the system gave as a number with a fraction, as a whole one: the
/// nearest, or `None` for one that is no number or does not fit. Display bounds
/// on macOS are doubles, whole on every display measured, so rounding only has
/// to be the same on every run for a layout to keep one text form.
#[cfg(any(target_os = "macos", test))]
fn whole(value: f64) -> Option<i32> {
    let rounded = value.round();
    if rounded.is_nan() || rounded < f64::from(i32::MIN) || rounded > f64::from(i32::MAX) {
        return None;
    }
    // Checked just above: the value is a whole number inside the range of `i32`.
    #[allow(clippy::cast_possible_truncation)]
    Some(rounded as i32)
}

/// The screen a rectangle given by its corner and size is, as the doubles the
/// system gives - or `None` for one that is not a number, does not fit or has no
/// area. The edges are rounded, not the size, so two displays that touch keep
/// touching.
#[cfg(any(target_os = "macos", test))]
fn monitor_of_rect(x: f64, y: f64, width: f64, height: f64) -> Option<Monitor> {
    monitor_of(whole(x)?, whole(y)?, whole(x + width)?, whole(y + height)?)
}

/// The screen a rectangle given by its edges is - or `None` for one with no
/// area, which no window can stand on. Apart from the system call, so it is
/// checked on every system.
#[cfg(any(windows, target_os = "macos", test))]
fn monitor_of(left: i32, top: i32, right: i32, bottom: i32) -> Option<Monitor> {
    let width = u32::try_from(i64::from(right) - i64::from(left)).ok()?;
    let height = u32::try_from(i64::from(bottom) - i64::from(top)).ok()?;
    (width > 0 && height > 0).then_some(Monitor {
        x: left,
        y: top,
        width,
        height,
    })
}

#[cfg(windows)]
mod platform {
    use windows_sys::Win32::Foundation::{LPARAM, RECT};
    use windows_sys::Win32::Graphics::Gdi::{EnumDisplayMonitors, HDC, HMONITOR};
    use windows_sys::Win32::UI::HiDpi::{
        DPI_AWARENESS_PER_MONITOR_AWARE, GetAwarenessFromDpiAwarenessContext,
        GetThreadDpiAwarenessContext,
    };
    use windows_sys::core::BOOL;

    use super::{MONITOR_LIMIT, Monitor, monitor_of};

    /// What the enumeration collects. `broken` is set by a screen with no area
    /// or by one screen too many - either way the list is not a layout.
    struct Found {
        monitors: Vec<Monitor>,
        broken: bool,
    }

    pub fn monitors() -> Option<Vec<Monitor>> {
        let awareness =
            unsafe { GetAwarenessFromDpiAwarenessContext(GetThreadDpiAwarenessContext()) };
        if awareness != DPI_AWARENESS_PER_MONITOR_AWARE {
            return None;
        }
        let mut found = Found {
            monitors: Vec::new(),
            broken: false,
        };
        // With no device context and no clipping rectangle, the rectangle each
        // call receives IS the screen's rectangle (Microsoft Learn,
        // `MonitorEnumProc`) - no `GetMonitorInfoW` needed.
        let listed = unsafe {
            EnumDisplayMonitors(
                core::ptr::null_mut(),
                core::ptr::null(),
                Some(each),
                (&raw mut found) as LPARAM,
            )
        };
        (listed != 0 && !found.broken && !found.monitors.is_empty()).then_some(found.monitors)
    }

    unsafe extern "system" fn each(_: HMONITOR, _: HDC, rect: *mut RECT, data: LPARAM) -> BOOL {
        // `data` is the `Found` of the call above, alive for the whole
        // enumeration and touched by nothing else meanwhile.
        let found = unsafe { &mut *(data as *mut Found) };
        let monitor = unsafe { rect.as_ref() }
            .and_then(|rect| monitor_of(rect.left, rect.top, rect.right, rect.bottom));
        match monitor {
            Some(monitor) if found.monitors.len() < MONITOR_LIMIT => {
                found.monitors.push(monitor);
                1
            }
            _ => {
                found.broken = true;
                0
            }
        }
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::{MONITOR_LIMIT, Monitor, monitor_of_rect};
    use crate::macos::ffi::{CGDisplayBounds, CGGetActiveDisplayList};

    pub fn monitors() -> Option<Vec<Monitor>> {
        // One more than the limit, so a list that was cut short shows as longer
        // than it: `count` never exceeds `max`, and a machine with more displays
        // than asked for would otherwise look like one that has exactly that many.
        let mut ids = [0_u32; MONITOR_LIMIT + 1];
        let mut count = 0_u32;
        let asked = u32::try_from(ids.len()).ok()?;
        // `ids` holds `asked` entries, and `count` is written once.
        let status = unsafe { CGGetActiveDisplayList(asked, ids.as_mut_ptr(), &raw mut count) };
        let count = usize::try_from(count).ok()?;
        if status != 0 || count == 0 || count > MONITOR_LIMIT {
            return None;
        }
        ids[..count]
            .iter()
            .map(|id| {
                let rect = unsafe { CGDisplayBounds(*id) };
                monitor_of_rect(
                    rect.origin.x,
                    rect.origin.y,
                    rect.size.width,
                    rect.size.height,
                )
            })
            .collect()
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
mod platform {
    // Not asked here yet: the palette has no global shortcuts on this system
    // (step 6), and a layout read some other way would be a second unit to
    // keep apart. `None` is "nothing to remember a place for", said by the
    // caller as such.
    pub fn monitors() -> Option<Vec<super::Monitor>> {
        None
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;

    #[test]
    fn a_rectangle_by_its_edges_is_a_screen_with_its_corner_and_size() {
        assert_eq!(
            monitor_of(-1920, 180, 0, 1260),
            Some(Monitor {
                x: -1920,
                y: 180,
                width: 1920,
                height: 1080
            })
        );
    }

    #[test]
    fn a_rectangle_with_no_area_or_turned_inside_out_is_no_screen() {
        assert_eq!(monitor_of(0, 0, 0, 1080), None);
        assert_eq!(monitor_of(0, 0, 1920, 0), None);
        assert_eq!(monitor_of(100, 0, 0, 1080), None);
        // The widest the edges can be still fits.
        assert!(monitor_of(i32::MIN, 0, i32::MAX, 1).is_some());
    }

    /// The doubles macOS gives: the main display at the origin, one to its left
    /// and a little higher, so with negative coordinates.
    #[test]
    fn display_bounds_in_points_are_screens_and_the_edges_are_what_is_rounded() {
        assert_eq!(
            monitor_of_rect(0.0, 0.0, 1512.0, 982.0),
            Some(Monitor {
                x: 0,
                y: 0,
                width: 1512,
                height: 982
            })
        );
        assert_eq!(
            monitor_of_rect(-1920.0, -120.0, 1920.0, 1080.0),
            Some(Monitor {
                x: -1920,
                y: -120,
                width: 1920,
                height: 1080
            })
        );
        // A fraction on a corner moves the edge, not the neighbour: this one
        // ends where the next begins, and no sliver opens between them.
        let left = monitor_of_rect(0.0, 0.0, 1440.4, 900.0).expect("a screen");
        let right = monitor_of_rect(1440.4, 0.0, 1920.0, 1080.0).expect("a screen");
        assert_eq!(
            i64::from(left.x) + i64::from(left.width),
            i64::from(right.x)
        );
    }

    #[test]
    fn display_bounds_that_are_no_number_or_no_area_are_no_screen() {
        // A coordinate is the nearest whole one, half away from zero, and what is
        // no number or does not fit is none.
        assert_eq!(whole(2.4), Some(2));
        assert_eq!(whole(2.5), Some(3));
        assert_eq!(whole(-2.5), Some(-3));
        assert_eq!(whole(f64::NAN), None);
        assert_eq!(whole(f64::NEG_INFINITY), None);
        assert_eq!(whole(2.2e9), None);
        assert_eq!(monitor_of_rect(f64::NAN, 0.0, 100.0, 100.0), None);
        assert_eq!(monitor_of_rect(0.0, 0.0, f64::INFINITY, 100.0), None);
        assert_eq!(monitor_of_rect(0.0, 0.0, 0.0, 100.0), None);
        assert_eq!(monitor_of_rect(0.0, 0.0, 100.0, -5.0), None);
        // Past what a coordinate holds.
        assert_eq!(monitor_of_rect(3.0e9, 0.0, 100.0, 100.0), None);
        // A sliver under half a point rounds to nothing.
        assert_eq!(monitor_of_rect(0.0, 0.0, 0.4, 100.0), None);
    }

    /// Asks the real display list, on the system that has one. Not what the
    /// list IS - a machine has the screens it has - but that every screen is
    /// a rectangle with an area and that there are not more than the limit.
    #[cfg(target_os = "macos")]
    #[test]
    fn the_displays_of_this_mac_are_listed() {
        let monitors = monitors().expect("a Mac with a display lists it");
        assert!(!monitors.is_empty() && monitors.len() <= MONITOR_LIMIT);
        assert!(monitors.iter().all(|m| m.width > 0 && m.height > 0));
    }
}
