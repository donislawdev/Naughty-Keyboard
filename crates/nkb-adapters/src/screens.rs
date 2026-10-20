//! The layout of screens the palette stands on now (UX7, `UX-GUI-012`).
//!
//! The system's answer comes from `nkb-sys`, the rule of what makes a layout
//! from `nkb-core` - this is where the two meet, so the window layer needs
//! neither the system nor the rule. No port: there is one way to ask and no
//! second one to swap in, the same reasoning as `layout_character` (`D88`).

use nkb_core::screens::{Layout, Screen};

/// Whether the layout and the places remembered on it are in points rather than
/// physical pixels - the unit this system places a window in (`D143`).
///
/// macOS has one desktop of points whatever the scale of each display, and a
/// window is placed in them. Windows reports physical pixels. The one place
/// that knows the system, like [`crate::shortcuts::CONVENTION`], so the window
/// layer converts what its toolkit says to the unit of the layout in one call.
pub const IN_POINTS: bool = cfg!(target_os = "macos");

/// The layout of the screens now - or `None` when the system gives none in the
/// unit of the layout, or on a system where it is not asked yet. `None` is
/// "nothing to remember a place for": the palette then opens where the system
/// puts it and remembers nothing on closing.
///
/// ⚠️ Ask it AFTER the first window is built: before that the process is not
/// aware of each screen's scale on Windows, and the answer would be `None`
/// (`nkb_sys::screens`).
#[must_use]
pub fn layout() -> Option<Layout> {
    let monitors = nkb_sys::screens::monitors()?;
    Layout::new(monitors.into_iter().map(|monitor| Screen {
        x: monitor.x,
        y: monitor.y,
        width: monitor.width,
        height: monitor.height,
    }))
}
