//! The call behind `nkb_sys::screens::work_area` on Linux: `_NET_WORKAREA` of the
//! current desktop, which the window manager keeps on the root window of the X
//! server the windows of this program stand on - XWayland's on a Wayland session,
//! where the palette and every window opened from it are its clients
//! (`connect_windows`).
//!
//! One rectangle for the whole desktop, which is what the extended hints define:
//! the desktop less what the panels reserve. The screen a window stands on is
//! known to the window toolkit, not here, so the caller meets the two.

use x11rb::protocol::xproto::{AtomEnum, ConnectionExt};

use super::Lost;
use super::connect::connect_windows;
use crate::screens::Monitor;

/// The work area of the current desktop, in the pixels of the server - or `None`
/// when there is no server for the windows, no manager that writes the hint, or a
/// hint this does not read.
pub(crate) fn work_area() -> Option<Monitor> {
    let windows = connect_windows().ok()?;
    read(&windows.server.conn, windows.server.root)
        .ok()
        .flatten()
}

fn read(conn: &super::stream::Conn, root: u32) -> Result<Option<Monitor>, Lost> {
    let atom =
        |name: &[u8]| -> Result<u32, Lost> { Ok(conn.intern_atom(false, name)?.reply()?.atom) };
    let (current, area) = (atom(b"_NET_CURRENT_DESKTOP")?, atom(b"_NET_WORKAREA")?);
    // Not written is the first desktop, the one every manager has.
    let desktop = conn
        .get_property(false, root, current, AtomEnum::CARDINAL, 0, 1)?
        .reply()?
        .value32()
        .and_then(|mut values| values.next())
        .unwrap_or(0);
    let values: Vec<u32> = conn
        .get_property(false, root, area, AtomEnum::CARDINAL, 0, WORK_AREA_LONGEST)?
        .reply()?
        .value32()
        .map(Iterator::collect)
        .unwrap_or_default();
    Ok(crate::screens::work_area_of_desktop(&values, desktop))
}

/// How many numbers of the hint are read: four for each of far more desktops
/// than any manager keeps, so the current one is never cut off.
const WORK_AREA_LONGEST: u32 = 4 * 64;
