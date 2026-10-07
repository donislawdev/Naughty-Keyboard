//! The layout of screens the palette stands on now (UX7, `UX-GUI-012`).
//!
//! The system's answer comes from `nkb-sys`, the rule of what makes a layout
//! from `nkb-core` - this is where the two meet, so the window layer needs
//! neither the system nor the rule. No port: there is one way to ask and no
//! second one to swap in, the same reasoning as `altgr_character` (`D88`).

use nkb_core::screens::{Layout, Screen};

/// The layout of the screens now - or `None` when the system gives none in
/// physical pixels, or on a system where it is not asked yet. `None` is
/// "nothing to remember a place for": the palette then opens where the system
/// puts it and remembers nothing on closing.
///
/// ⚠️ Ask it AFTER the first window is built: before that the process is not
/// aware of each screen's scale, and the answer would be `None`
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
