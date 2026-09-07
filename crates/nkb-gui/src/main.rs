//! The nkb palette.
//!
//! A full product in its own right, not a windowless CLI with a window bolted
//! on - D25. It shares `core` and `app` with `nkb` and shares the same catalogue
//! on disk, and neither executable can start the other.
//!
//! Today it opens the gallery: every component in every state, which is what
//! document 13 section 3 asks for and what a session reads to find out what the
//! vocabulary already contains. The palette itself comes next.

use nkb_gui::Gallery;
use slint::ComponentHandle;

fn main() -> Result<(), slint::PlatformError> {
    Gallery::new()?.run()
}
