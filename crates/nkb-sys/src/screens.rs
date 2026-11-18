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

/// The part of the screen holding the point `(x, y)` that windows may use: the
/// screen less the taskbar, the menu bar, the Dock or a panel (`D190`). In the
/// unit of [`Monitor`], and on Linux in the pixels of the X server the windows
/// of this program stand on.
///
/// On Linux the answer is the work area of the whole desktop, which is all the
/// window manager says, and the caller meets it with the screen its window
/// stands on. `None` where the system does not say, or where the point is on no
/// screen it lists.
#[must_use]
pub fn work_area(x: i32, y: i32) -> Option<Monitor> {
    platform::work_area(x, y)
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

/// One screen as AppKit lists it, in AppKit's space - the origin at the BOTTOM
/// left corner of the first screen, `y` growing up: its frame and its visible
/// frame, each as x, y, width and height.
#[cfg(any(target_os = "macos", test))]
#[derive(Debug, Clone, Copy)]
struct AppKitScreen {
    frame: [f64; 4],
    visible: [f64; 4],
}

/// The visible frame of the screen whose frame holds the point, in the layout's
/// space - the origin at the top left corner of the first screen, which is the
/// one with the menu bar, `y` growing down. A rectangle turns over against that
/// screen's height. `None` when the point is on no screen.
#[cfg(any(target_os = "macos", test))]
fn visible_frame_at(screens: &[AppKitScreen], x: i32, y: i32) -> Option<Monitor> {
    let first = screens.first()?.frame[3];
    let turned = |[left, bottom, width, height]: [f64; 4]| {
        monitor_of_rect(left, first - (bottom + height), width, height)
    };
    screens.iter().find_map(|screen| {
        let frame = turned(screen.frame)?;
        let (x, y) = (i64::from(x), i64::from(y));
        let holds = x >= i64::from(frame.x)
            && y >= i64::from(frame.y)
            && x < i64::from(frame.x) + i64::from(frame.width)
            && y < i64::from(frame.y) + i64::from(frame.height);
        if holds { turned(screen.visible) } else { None }
    })
}

/// The work area of `desktop` from the numbers of `_NET_WORKAREA`: four for each
/// desktop - x, y, width and height. `None` when the hint holds fewer than that
/// desktop's four, or a rectangle that does not fit or has no area.
#[cfg(any(target_os = "linux", test))]
pub(crate) fn work_area_of_desktop(values: &[u32], desktop: u32) -> Option<Monitor> {
    let start = usize::try_from(desktop).ok()?.checked_mul(4)?;
    let four = values.get(start..start.checked_add(4)?)?;
    let [x, y, width, height] = <[u32; 4]>::try_from(four).ok()?;
    let (left, top) = (i32::try_from(x).ok()?, i32::try_from(y).ok()?);
    monitor_of(
        left,
        top,
        left.checked_add(i32::try_from(width).ok()?)?,
        top.checked_add(i32::try_from(height).ok()?)?,
    )
}

/// The screen a rectangle given by its edges is - or `None` for one with no
/// area, which no window can stand on. Apart from the system call, so it is
/// checked on every system.
#[cfg(any(windows, target_os = "macos", target_os = "linux", test))]
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
    use windows_sys::Win32::Foundation::{LPARAM, POINT, RECT};
    use windows_sys::Win32::Graphics::Gdi::{
        EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITOR_DEFAULTTONEAREST, MONITORINFO,
        MonitorFromPoint,
    };
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

    /// Whether this thread is told rectangles in physical pixels - the module
    /// header says why nothing is answered otherwise.
    fn per_monitor_aware() -> bool {
        let awareness =
            unsafe { GetAwarenessFromDpiAwarenessContext(GetThreadDpiAwarenessContext()) };
        awareness == DPI_AWARENESS_PER_MONITOR_AWARE
    }

    /// The work area of the screen nearest the point: `rcWork`, the screen less
    /// the taskbar and the bars docked to its edges (Microsoft Learn,
    /// `MONITORINFO`).
    pub fn work_area(x: i32, y: i32) -> Option<Monitor> {
        if !per_monitor_aware() {
            return None;
        }
        let monitor = unsafe { MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST) };
        if monitor.is_null() {
            return None;
        }
        let empty = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        let mut info = MONITORINFO {
            cbSize: u32::try_from(size_of::<MONITORINFO>()).ok()?,
            rcMonitor: empty,
            rcWork: empty,
            dwFlags: 0,
        };
        // `info` is a `MONITORINFO` with its size written, as the call requires.
        if unsafe { GetMonitorInfoW(monitor, &raw mut info) } == 0 {
            return None;
        }
        let work = info.rcWork;
        monitor_of(work.left, work.top, work.right, work.bottom)
    }

    pub fn monitors() -> Option<Vec<Monitor>> {
        if !per_monitor_aware() {
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
    use super::{AppKitScreen, MONITOR_LIMIT, Monitor, monitor_of_rect, visible_frame_at};
    use crate::macos::ffi::{CGDisplayBounds, CGGetActiveDisplayList, CgRect};

    /// The visible frame of the screen the point is on, as AppKit lists the
    /// screens (`macos/work_area.rs`), in the layout's space of points.
    pub fn work_area(x: i32, y: i32) -> Option<Monitor> {
        let four = |rect: CgRect| {
            [
                rect.origin.x,
                rect.origin.y,
                rect.size.width,
                rect.size.height,
            ]
        };
        let screens: Vec<AppKitScreen> = crate::macos::work_area::screens()?
            .iter()
            .map(|screen| AppKitScreen {
                frame: four(screen.frame),
                visible: four(screen.visible),
            })
            .collect();
        visible_frame_at(&screens, x, y)
    }

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

    /// The work area of the current desktop, from the X server the windows
    /// stand on (`linux/work_area.rs`) - the point is the caller's to meet with
    /// its screen.
    #[cfg(target_os = "linux")]
    pub fn work_area(_x: i32, _y: i32) -> Option<super::Monitor> {
        crate::linux::work_area::work_area()
    }

    #[cfg(not(target_os = "linux"))]
    pub fn work_area(_x: i32, _y: i32) -> Option<super::Monitor> {
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

    /// A MacBook's screen with the menu bar (37 points) and the Dock (70) - the
    /// first, at AppKit's origin - and a screen above it and to its left, with
    /// its own menu bar (25).
    const MAIN: AppKitScreen = AppKitScreen {
        frame: [0.0, 0.0, 1512.0, 982.0],
        visible: [0.0, 70.0, 1512.0, 875.0],
    };
    const ABOVE: AppKitScreen = AppKitScreen {
        frame: [-1920.0, 982.0, 1920.0, 1080.0],
        visible: [-1920.0, 982.0, 1920.0, 1055.0],
    };

    #[test]
    fn appkit_frames_turn_over_into_the_layouts_space_and_the_point_picks_the_screen() {
        let screens = [MAIN, ABOVE];
        // On the main screen: below the menu bar, above the Dock.
        assert_eq!(
            visible_frame_at(&screens, 700, 500),
            Some(Monitor {
                x: 0,
                y: 37,
                width: 1512,
                height: 875
            })
        );
        // On the screen above: negative in the layout, as `CGDisplayBounds`
        // has it, and below its own menu bar.
        assert_eq!(
            visible_frame_at(&screens, -100, -500),
            Some(Monitor {
                x: -1920,
                y: -1055,
                width: 1920,
                height: 1055
            })
        );
        // The frame decides, not the visible part: a point on the menu bar is
        // on its screen.
        assert_eq!(
            visible_frame_at(&screens, 700, 10).map(|area| area.y),
            Some(37)
        );
    }

    #[test]
    fn a_point_on_no_screen_or_no_screen_at_all_has_no_work_area() {
        assert_eq!(visible_frame_at(&[MAIN, ABOVE], 5000, 5000), None);
        // Right and bottom edges are outside, as everywhere.
        assert_eq!(visible_frame_at(&[MAIN], 1512, 500), None);
        assert_eq!(visible_frame_at(&[], 0, 0), None);
    }

    #[test]
    fn the_work_area_hint_gives_four_numbers_for_each_desktop() {
        // GNOME on a 1280 x 800 screen, a top bar of 32, two desktops.
        let hint = [0, 32, 1280, 768, 0, 0, 1280, 800];
        assert_eq!(
            work_area_of_desktop(&hint, 0),
            Some(Monitor {
                x: 0,
                y: 32,
                width: 1280,
                height: 768
            })
        );
        assert_eq!(
            work_area_of_desktop(&hint, 1).map(|area| area.height),
            Some(800)
        );
    }

    #[test]
    fn a_hint_too_short_or_with_no_area_or_out_of_range_says_nothing() {
        let hint = [0, 32, 1280, 768];
        assert_eq!(work_area_of_desktop(&hint, 1), None);
        assert_eq!(work_area_of_desktop(&hint[..3], 0), None);
        assert_eq!(work_area_of_desktop(&[0, 0, 0, 768], 0), None);
        assert_eq!(work_area_of_desktop(&[u32::MAX, 0, 10, 10], 0), None);
        assert_eq!(work_area_of_desktop(&[0, 0, u32::MAX, 10], 0), None);
        assert_eq!(work_area_of_desktop(&hint, u32::MAX), None);
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
