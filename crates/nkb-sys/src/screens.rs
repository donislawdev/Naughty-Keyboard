//! The screens of this machine, in physical pixels (UX7, `UX-GUI-012`).
//!
//! # Why the system is asked and not the window library
//!
//! The palette is put where the tester left it on this layout of screens, and
//! that has to be known BEFORE its window exists. Slint 1.18 names no screen to
//! its application, and winit lists them only inside a running event loop.
//!
//! # Physical pixels, or nothing
//!
//! `EnumDisplayMonitors` reports rectangles in the coordinates of the calling
//! thread's DPI awareness. A thread that is not aware per monitor gets them
//! scaled, and a place remembered in one unit and compared in the other would
//! land somewhere else. winit makes the process aware per monitor when its
//! event loop is created (winit 0.30.13, `platform_impl/windows/event_loop.rs`),
//! which in Slint happens with the first window - so the palette asks after
//! building it, and this module answers `None` rather than scaled rectangles
//! when the thread is not aware per monitor.
//!
//! Nothing is read from any window: a screen's rectangle is all that is asked.

/// One screen's rectangle on the virtual desktop, in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Monitor {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// The screens, in the order the system lists them - or `None` when they
/// could not be listed in physical pixels, or on a system where this module
/// does not ask (macOS and Linux, for now). `None` means "no layout to
/// remember a place for", never "no screen".
#[must_use]
pub fn monitors() -> Option<Vec<Monitor>> {
    platform::monitors()
}

/// More screens than any machine this tool has met. A longer list is not
/// trusted, the same rule as `layout::LAYOUT_LIMIT`.
const MONITOR_LIMIT: usize = 64;

/// The screen a rectangle given by its edges is - or `None` for one with no
/// area, which no window can stand on. Apart from the system call, so it is
/// checked on every system.
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

#[cfg(not(windows))]
mod platform {
    // Not asked here yet: the palette has no global shortcuts on these systems
    // (step 6), and a layout read some other way would be a second unit to
    // keep apart. `None` is "nothing to remember a place for", said by the
    // caller as such.
    pub fn monitors() -> Option<Vec<super::Monitor>> {
        None
    }
}

#[cfg(test)]
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
}
