//! The support page, opened in the browser when the tester clicks Donate in
//! the palette's header (the owner's point 5, 2026-10-14).
//!
//! 🔴 The one place in the product that hands an address to anything. The tool
//! promises it opens no network connection, and it still does not: the browser
//! makes the connection, on a click, to the one address below. Nothing here
//! takes an address from a pack, a setting, a view or a sentence - the guard
//! `nothing_reaches_the_network.rs` grants `webbrowser::` to this file alone
//! and holds the address in it to [`SUPPORT_PAGE`].

/// The page the Donate link opens.
pub const SUPPORT_PAGE: &str = "https://donislawdev.com/support/";

use nkb_adapters::i18n::{self, PaletteLabel};
use slint::{ModelRc, SharedString, VecModel};

use crate::Palette;

/// Asks the system to open the support page in the default browser. Whether
/// it could: a failure is the caller's to say, never a silence (untouchable
/// rule 1).
#[must_use]
pub fn open_support_page() -> bool {
    webbrowser::open(SUPPORT_PAGE).is_ok()
}

/// A click on Donate, on the MAIN thread: the page opens, or the palette's
/// status line says it did not and names the page, so the tester can type it.
pub fn on_click(palette: &Palette) {
    if !open_support_page() {
        palette.set_messages(ModelRc::new(VecModel::from(vec![SharedString::from(
            i18n::label(PaletteLabel::DonateFailed),
        )])));
    }
}
