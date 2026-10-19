//! How a window opened on request stands on the screen - the pack window, the
//! shortcuts window and About alike - and how it goes away.
//!
//! # Not minimized, ever
//!
//! 🔴 Measured, not read (`OBS-183`, `tools/sondy/minimalizacja-okna-paczek.ps1`,
//! 2026-10-19, Slint 1.18.1, winit 0.30.13, Windows 11). Such a window closes
//! when it stops being the active one (`OBS-147`), and minimizing it makes it
//! stop. Three things went wrong from there, each answered here:
//!
//! 1. The hide asked for in that moment was lost when the minimizing came from
//!    the window's own button, its menu or `Win+Down`: the report that the
//!    window went inactive arrives while the system is still minimizing it,
//!    and the window stayed in the taskbar while its owner took it for closed.
//!    Restored from there it answered no key, and neither Esc nor its close
//!    button closed it. So a window that was left hides once that event is
//!    over: [`hide_once_left`].
//! 2. Opened again, it came up minimized, in each of the three ways tried -
//!    `show()` shows a window in the state it was hidden in - and it took the
//!    keyboard there. The tester saw nothing, and the letters of the search
//!    went into a window nobody could see. So every opening brings it back:
//!    [`bring_up`].
//! 3. A button that closes the window under another name is one too many (the
//!    owner's choice, `D137`): its title bar keeps Close alone.
//!
//! What stays possible is named: without the button the window can still be
//! minimized another way - by a program of somebody else's at least - and that
//! is leaving like any other, which 1 and 2 make safe.

use std::rc::Rc;
use std::time::Duration;

use slint::winit_030::WinitWindowAccessor;
use slint::winit_030::winit::window::WindowButtons;

/// Shows `window` - or, already shown, brings it back - as a window opened on
/// request stands: on the screen rather than in the taskbar, with Close as its
/// only button.
///
/// # Errors
///
/// The toolkit's own, when the window cannot be shown.
pub(crate) fn bring_up(window: &slint::Window) -> Result<(), slint::PlatformError> {
    window.show()?;
    // After `show()`, never before: bringing a hidden window back from the
    // taskbar shows it, and a window that is not minimized is left as it is.
    window.set_minimized(false);
    // Last: winit does not write the buttons of a minimized window (its
    // `apply_diff`), and asked again for the same buttons it has no change
    // to write.
    keep_only_close(window);
    Ok(())
}

/// Leaves Close as the title bar's only button - minimize and maximize go.
///
/// Both, because on Windows a minimize button leaves the title bar only
/// together with maximize: alone it stays there, greyed. The window still
/// resizes by its edges.
///
/// Through winit rather than the window's style in `nkb-sys`: winit writes the
/// style from its own flags whenever any of them changes - every show and every
/// hide - and would put the buttons back. A window that winit does not back,
/// the tests' own, has no title bar to change.
fn keep_only_close(window: &slint::Window) {
    let _ = window.with_winit_window(|window| window.set_enabled_buttons(WindowButtons::CLOSE));
}

/// Hides the window of `owner` once the event that made it inactive is over -
/// if `closed` still says so then, because a window opened again in between
/// stays where it is.
///
/// One turn of the event loop, not a delay: a timer of zero runs once the
/// system has finished with the message it was handling, the minimizing
/// included. Asked for inside it, the hide was lost (point 1 of the header).
pub(crate) fn hide_once_left<T: 'static>(
    owner: &Rc<T>,
    closed: fn(&T) -> bool,
    window: fn(&T) -> &slint::Window,
) {
    let owner = Rc::downgrade(owner);
    slint::Timer::single_shot(Duration::ZERO, move || {
        if let Some(owner) = owner.upgrade()
            && closed(&owner)
        {
            let _ = window(&owner).hide();
        }
    });
}
