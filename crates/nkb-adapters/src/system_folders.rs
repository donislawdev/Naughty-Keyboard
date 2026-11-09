//! The system's folder window and file manager, for the pack folders window
//! (`D175`). The calls are `nkb-sys`'s - the one crate where `unsafe` compiles -
//! and the window reaches them through this adapter, as it reaches every call of
//! the system: `nkb-gui` depends on no system crate of its own.

use std::path::Path;

pub use nkb_sys::folders::{PickAnswer, PickError, PickRequest, Picked, ShowAnswer, ShowError};

/// Shows the system's folder window, and hands the answer on - see
/// `nkb_sys::folders::pick_folder` for the thread it arrives on.
pub fn pick_folder(request: PickRequest, answer: PickAnswer) {
    nkb_sys::folders::pick_folder(request, answer);
}

/// Opens `folder` in the system's file manager, and hands what came of it on,
/// from a thread of its own.
pub fn show_folder(folder: &Path, answer: ShowAnswer) {
    nkb_sys::folders::show_folder(folder, answer);
}
