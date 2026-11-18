//! The theme of the title bar the system draws over the palette's windows.
//!
//! The windows are dark and only dark (`D48`), but the bar above them is the
//! system's, drawn from the theme the window is made with. Left without one,
//! winit 0.30 asks Windows whether applications should be dark through an
//! undocumented call that answers no to a process that has not asked for dark
//! itself (measured 2026-11-18 on Windows 11 build 26300 with dark applications
//! on, `slint.md` 2.95), and the bar came out white over a dark window
//! (`OBS-263`). So every window is made dark, whatever the system's choice.
//!
//! Under high contrast on Windows the bar is left to the system, which draws it
//! in the colours of the contrast theme - there the system's colours are the
//! point (`D48`). The theme is asked as each window is made, so a window made
//! after the setting changed follows it.
//!
//! On Linux nothing is given here: the frame is the window manager's, and Slint
//! hands winit the desktop's colour preference right after it makes a window and
//! again at every change of it, so a theme given at the making would not last. A
//! dark desktop gives a dark frame there (measured under GNOME 50, `slint.md`
//! 2.95).

#[cfg(any(windows, target_os = "macos"))]
use slint::winit_030::winit::window::Theme;

/// The theme a window is made with: dark, or on Windows under high contrast
/// none, which leaves the bar to the system.
#[cfg(windows)]
#[must_use]
pub fn theme() -> Option<Theme> {
    (!nkb_adapters::appearance::high_contrast()).then_some(Theme::Dark)
}

/// The theme a window is made with: dark. AppKit draws the dark appearance in
/// its own high-contrast form when Increase contrast is on.
#[cfg(target_os = "macos")]
#[must_use]
pub fn theme() -> Option<Theme> {
    Some(Theme::Dark)
}
