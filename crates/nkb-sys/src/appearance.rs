//! What the system asks of the way windows look.
//!
//! One question so far, on Windows: whether a high-contrast theme is on. The
//! palette's windows are dark whatever the system's colours (`D48`), but under
//! high contrast the title bar the system draws over them is left to the system,
//! because there its own colours are the point (`OBS-263`).

/// Whether a high-contrast theme is on: `SPI_GETHIGHCONTRAST` with
/// `HCF_HIGHCONTRASTON` (Microsoft Learn, `HIGHCONTRASTW`). A read the system
/// refuses counts as off.
#[cfg(windows)]
#[must_use]
pub fn high_contrast() -> bool {
    use windows_sys::Win32::UI::Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW};
    use windows_sys::Win32::UI::WindowsAndMessaging::{SPI_GETHIGHCONTRAST, SystemParametersInfoW};

    let Ok(size) = u32::try_from(size_of::<HIGHCONTRASTW>()) else {
        return false;
    };
    let mut answer = HIGHCONTRASTW {
        cbSize: size,
        ..HIGHCONTRASTW::default()
    };
    // `answer` is a `HIGHCONTRASTW` with its size written in it and in the
    // second argument, as the call requires. Nothing is written to the settings.
    let read =
        unsafe { SystemParametersInfoW(SPI_GETHIGHCONTRAST, size, (&raw mut answer).cast(), 0) };
    read != 0 && answer.dwFlags & HCF_HIGHCONTRASTON != 0
}
