//! What the system asks of the way windows look, for the window layer.
//!
//! The answer comes from `nkb-sys`. No port: there is one way to ask and no
//! second one to swap in, the same reasoning as `screens` (`D88`).

/// Whether a high-contrast theme is on (`nkb_sys::appearance`), so that the
/// title bar of the palette's windows is left to the system (`D48`, `OBS-263`).
#[cfg(windows)]
#[must_use]
pub fn high_contrast() -> bool {
    nkb_sys::appearance::high_contrast()
}
