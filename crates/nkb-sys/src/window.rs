//! Making a window refuse the keyboard focus.
//!
//! # The requirement, and why it needs two calls rather than one
//!
//! `ux-spec.md` 2 calls it the sharpest technical requirement in the product:
//! the palette may never take the keyboard focus, because the cursor would then
//! leave the field under test at every insertion. `OBS-119` recorded the symptom
//! and named the route as `WS_EX_NOACTIVATE`, measured and ready.
//!
//! 🔴 The measurement behind that name covered the STYLE, not the EFFECT, and
//! the effect is different. Probed 2026-09-22 with `tools/sonda-ognisko`,
//! Windows 11, ten runs per variant, the verdict read from
//! `GetForegroundWindow` rather than from any call's return value:
//!
//! | what the window did | palette holds the foreground | a click activates it |
//! |---|---|---|
//! | nothing | **10 of 10** | - |
//! | `WS_EX_NOACTIVATE` alone | **9 of 10** | - |
//! | the flag and handing the foreground back | **0 of 10** | **0 of 5** |
//! | handing it back, no flag | 0 of 10 | **5 of 5** |
//!
//! Two things follow, and neither could be read anywhere:
//!
//! - **the flag alone does not fix the start.** Slint hands out the window
//!   handle only after the window manager has created the window, measured here
//!   at 258 to 489 ms after the first ask, so the flag necessarily arrives after
//!   the system had its chance to activate. Worse, it is NON-DETERMINISTIC at
//!   nine out of ten, which is the least useful kind of repair.
//! - **the flag and the hand-back do different jobs and both are needed.** The
//!   hand-back settles the start. The flag settles the click: without it a click
//!   on the palette takes the focus away from the field every time, with it
//!   never, and the click still reaches the window either way.
//!
//! So the two functions below are never useful apart, and the adapter above
//! joins them into one call so that no caller can take half.

use crate::WindowRef;

/// Why the window could not be made to refuse the focus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FocusError {
    /// No route on this system yet. Never silence - untouchable rule 1 - so the
    /// caller is told which system it is standing on.
    Unsupported { system: &'static str },
    /// The style was written and the read-back did not show it.
    ///
    /// Checked by reading rather than by trusting the write, because
    /// `slint.md` 2.3 says so in as many words: a style that did not take looks
    /// exactly like one that did from the writing side.
    StyleRefused,
    /// The palette holds the foreground and would not give it up.
    ///
    /// Reported rather than swallowed: it means the next shortcut will deliver
    /// into our own window, which is precisely the symptom of `OBS-119`.
    StillForeground,
    /// A window of ours asked for the foreground and the system kept another
    /// one in front - read back, not taken from the call's return value.
    ///
    /// It matters because of what happens next: the tester sees our window and
    /// types, and the letters go to the window that stayed in front, which is
    /// the application under test.
    NotTaken,
    /// The window manager has not created the window, so there is no handle to
    /// put the style on.
    ///
    /// A real outcome rather than a corner: a window handle exists only after
    /// the window manager has made the window, measured at 258 to 489 ms after
    /// the first ask, and a machine slower than that would otherwise leave the
    /// caller with a silence to interpret.
    HandleUnavailable,
}

impl core::fmt::Display for FocusError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Unsupported { system } => write!(
                f,
                "the tool cannot move the keyboard focus between windows on {system} yet"
            ),
            Self::StyleRefused => write!(
                f,
                "the window style was set and read back without the flag, so clicking \
                 the palette will take the focus"
            ),
            Self::StillForeground => write!(
                f,
                "a window of this tool is still the foreground window, so the next \
                 shortcut would type into it"
            ),
            Self::NotTaken => write!(
                f,
                "the system kept another window in front, so what is typed now goes \
                 to that window"
            ),
            Self::HandleUnavailable => write!(
                f,
                "the window manager did not hand out a window handle in time, so the \
                 style could not be put on the palette"
            ),
        }
    }
}

/// Tells `window` not to become active when someone clicks it.
///
/// Idempotent: setting the flag on a window that already carries it is a write
/// of the same value and a read-back that agrees.
///
/// # Errors
///
/// [`FocusError::StyleRefused`] when the read-back does not show the flag,
/// [`FocusError::Unsupported`] where there is no route.
pub fn refuse_activation(window: WindowRef) -> Result<(), FocusError> {
    platform::refuse_activation(window)
}

/// Gives the foreground back to `previous`, but only if we took it.
///
/// Asking first is not caution. A window that never became the foreground has
/// nothing to hand back, and calling anyway would report a failure that is not
/// one: measured 2026-09-22, the palette takes the foreground on ten starts out
/// of ten, and an earlier run of the same probe showed it not taking it once, so
/// both cases are real on the same machine.
///
/// `previous` is `None` when nothing held the foreground before the palette
/// opened. There is then nobody to hand it to, and that is a success rather than
/// a failure: what the requirement asks for is that we do not KEEP it.
///
/// # Errors
///
/// [`FocusError::StillForeground`] when we hold the foreground and the system
/// refused to move it, [`FocusError::Unsupported`] where there is no route.
pub fn hand_back_foreground(
    ours: WindowRef,
    previous: Option<WindowRef>,
) -> Result<(), FocusError> {
    platform::hand_back_foreground(ours, previous)
}

/// Brings `window` - one of ours - to the foreground, with the keyboard.
///
/// The opposite of everything above, and needed by exactly one window: the pack
/// window, the one moment the tool holds the keyboard (`ux-spec.md` 5.2). It
/// opens on a global shortcut while the application under test is in front.
///
/// 🔴 Showing the window is NOT enough, measured 2026-09-29 with
/// `tools/sonda-okno-paczek` (Windows 11, the shortcut sent from a third
/// process, three runs per variant): shown alone, the window never got the
/// foreground (0 of 3) and the letters meant for its search went into the
/// field under test (3 of 3). With this call on the main thread after the
/// hotkey, 3 of 3 - the process that receives the hotkey may take the
/// foreground, which is the condition "received the last input event".
///
/// # Errors
///
/// [`FocusError::NotTaken`] when the read-back shows another window in front,
/// [`FocusError::Unsupported`] where there is no route.
pub fn take_foreground(window: WindowRef) -> Result<(), FocusError> {
    platform::take_foreground(window)
}

/// Why a window's menu could not be kept from opening from the keyboard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyMenuError {
    /// No route on this system yet, named - untouchable rule 1.
    Unsupported { system: &'static str },
    /// The system would not put our handler in front of the window's own -
    /// for one, when asked from a thread that does not own the window.
    Refused,
    /// The window manager has not created the window yet, so there is nothing
    /// to put the handler on - the same outcome as
    /// [`FocusError::HandleUnavailable`].
    HandleUnavailable,
}

impl core::fmt::Display for KeyMenuError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Unsupported { system } => write!(
                f,
                "the tool cannot keep a window menu from opening on {system} yet"
            ),
            Self::HandleUnavailable => write!(
                f,
                "the window manager did not hand out a window handle in time, so \
                 Alt+Space may open the window menu"
            ),
            Self::Refused => write!(
                f,
                "the system refused the handler, so Alt+Space opens the window menu, \
                 which takes the next key"
            ),
        }
    }
}

/// Keeps `window`'s own menu from opening from the keyboard - `Alt+Space`,
/// with or without `Shift` (K5.4).
///
/// # Why
///
/// The shortcuts window records chords, and `Alt+Shift+Space` is a default.
/// Measured 2026-09-30 with `tools/sonda-klawisze` (`ktory`), synthetic and
/// physical: the chord reaches the window's key handler, and then the window
/// menu opens and takes the next key - the system's own `Alt+Space`
/// (`slint.md` 2.37). Reading winit 0.30.13 predicted the opposite, which is
/// why this stands on the measurement and not on the reading.
///
/// # How, and what is left
///
/// A subclass of the window that answers `WM_SYSCOMMAND` with `SC_KEYMENU` -
/// the menu asked for by a key - itself, and passes every other message on.
/// The menu from the mouse (`SC_MOUSEMENU`), the close button and `Alt+F4`
/// are untouched. Only a window of ours is subclassed, on the thread that owns
/// it: nothing of any other application is touched.
///
/// ⚠️ A Slint window hidden and shown again is a NEW system window
/// (`OBS-80`), so this is called on every opening. Calling it twice on one
/// window replaces the subclass with itself.
///
/// # Errors
///
/// [`KeyMenuError::Refused`] when the system would not install it,
/// [`KeyMenuError::Unsupported`] where there is no route.
pub fn no_keyboard_menu(window: WindowRef) -> Result<(), KeyMenuError> {
    platform::no_keyboard_menu(window)
}

#[cfg(windows)]
mod platform {
    use super::{FocusError, KeyMenuError, WindowRef};
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GetWindowLongPtrW, SC_KEYMENU, SetForegroundWindow, SetWindowLongPtrW,
        WM_SYSCOMMAND, WS_EX_NOACTIVATE,
    };

    /// Which subclass this is, among any others on the window: "NKM1".
    const KEY_MENU_SUBCLASS: usize = 0x4E4B_4D31;

    /// The low four bits of a `WM_SYSCOMMAND` command are the system's own, so
    /// the command is compared with them masked off - as documented.
    const COMMAND_MASK: usize = 0xFFF0;

    /// Answers the keyboard's menu request itself, and hands everything else
    /// on. Runs on the window's thread, inside its message handling - it must
    /// not panic, and does nothing that could.
    unsafe extern "system" fn without_key_menu(
        window: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        _subclass: usize,
        _data: usize,
    ) -> LRESULT {
        if message == WM_SYSCOMMAND && wparam & COMMAND_MASK == SC_KEYMENU as usize {
            return 0;
        }
        unsafe { DefSubclassProc(window, message, wparam, lparam) }
    }

    pub fn no_keyboard_menu(window: WindowRef) -> Result<(), KeyMenuError> {
        let installed = unsafe {
            SetWindowSubclass(handle(window), Some(without_key_menu), KEY_MENU_SUBCLASS, 0)
        };
        if installed == 0 {
            return Err(KeyMenuError::Refused);
        }
        Ok(())
    }

    /// The handle as the API wants it. The `WindowRef` was made from one of
    /// these on the way in, so the round trip is the identity.
    fn handle(window: WindowRef) -> HWND {
        window.0 as usize as HWND
    }

    pub fn refuse_activation(window: WindowRef) -> Result<(), FocusError> {
        let hwnd = handle(window);
        let before = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) };
        let wanted = before | WS_EX_NOACTIVATE as isize;
        unsafe { SetWindowLongPtrW(hwnd, GWL_EXSTYLE, wanted) };
        // The read-back is the whole check. `SetWindowLongPtrW` returns the
        // PREVIOUS value, and zero for it is ambiguous - it also means "the
        // previous value was zero" - so the return value cannot answer this.
        let after = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) };
        if after & WS_EX_NOACTIVATE as isize == 0 {
            return Err(FocusError::StyleRefused);
        }
        Ok(())
    }

    pub fn hand_back_foreground(
        ours: WindowRef,
        previous: Option<WindowRef>,
    ) -> Result<(), FocusError> {
        if super::super::foreground_window() != Some(ours) {
            // We never took it. Nothing to give back.
            return Ok(());
        }
        let Some(previous) = previous else {
            // We hold it and there is nobody to hand it to. Saying so is the
            // honest answer: the tester's next shortcut goes into our window.
            return Err(FocusError::StillForeground);
        };
        unsafe { SetForegroundWindow(handle(previous)) };
        // Again the verdict comes from a READ rather than from the return value.
        // The probe's own header says why: calls about the foreground lied in
        // both directions in `tools/petla-ofiary.ps1`.
        if super::super::foreground_window() == Some(ours) {
            return Err(FocusError::StillForeground);
        }
        Ok(())
    }

    pub fn take_foreground(window: WindowRef) -> Result<(), FocusError> {
        if super::super::foreground_window() == Some(window) {
            return Ok(());
        }
        unsafe { SetForegroundWindow(handle(window)) };
        // The verdict from a READ, as everywhere in this file.
        if super::super::foreground_window() != Some(window) {
            return Err(FocusError::NotTaken);
        }
        Ok(())
    }
}

#[cfg(not(windows))]
mod platform {
    use super::{FocusError, KeyMenuError, WindowRef};

    /// Named rather than "this platform", so the message says something the
    /// reader can act on.
    const SYSTEM: &str = if cfg!(target_os = "macos") {
        "macOS"
    } else if cfg!(target_os = "linux") {
        "Linux"
    } else {
        "this system"
    };

    pub fn refuse_activation(_window: WindowRef) -> Result<(), FocusError> {
        Err(FocusError::Unsupported { system: SYSTEM })
    }

    pub fn hand_back_foreground(
        _ours: WindowRef,
        _previous: Option<WindowRef>,
    ) -> Result<(), FocusError> {
        Err(FocusError::Unsupported { system: SYSTEM })
    }

    pub fn take_foreground(_window: WindowRef) -> Result<(), FocusError> {
        Err(FocusError::Unsupported { system: SYSTEM })
    }

    pub fn no_keyboard_menu(_window: WindowRef) -> Result<(), KeyMenuError> {
        Err(KeyMenuError::Unsupported { system: SYSTEM })
    }
}

/// Whether this build can make a window refuse the focus at all.
///
/// Exists for the same reason as [`crate::can_send`]: so a caller can say so
/// once, in one place, rather than repeating a `cfg` at every call site.
#[must_use]
pub const fn can_refuse_focus() -> bool {
    cfg!(windows)
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;

    #[test]
    fn an_unsupported_system_is_named_rather_than_shrugged_at() {
        // Untouchable rule 1: "not supported" without a name is not something a
        // reader can act on.
        if can_refuse_focus() {
            return;
        }
        let Err(error) = refuse_activation(WindowRef(1)) else {
            panic!("a build with no route must not report success");
        };
        let text = error.to_string();
        assert!(
            text.contains("macOS") || text.contains("Linux") || text.contains("this system"),
            "the message must name the system, got: {text}"
        );
    }

    #[test]
    fn a_refused_style_says_what_it_costs_the_tester() {
        // The type carries the promise, so it is checked on every platform: a
        // caller must be able to tell the tester what stops working, not merely
        // that something went wrong.
        let text = FocusError::StyleRefused.to_string();
        assert!(
            text.contains("focus") && text.contains("click"),
            "got: {text}"
        );
    }

    #[test]
    fn still_holding_the_foreground_says_where_the_value_would_go() {
        let text = FocusError::StillForeground.to_string();
        assert!(
            text.contains("foreground") && text.contains("type into it"),
            "got: {text}"
        );
    }

    #[test]
    fn a_missing_handle_says_that_the_window_is_what_is_missing() {
        // The caller shows this to a tester, so it has to say which of the three
        // things went wrong rather than "focus problem".
        let text = FocusError::HandleUnavailable.to_string();
        assert!(
            text.contains("window handle") && text.contains("style"),
            "got: {text}"
        );
    }

    #[test]
    fn a_window_not_taken_says_where_the_typing_goes() {
        let text = FocusError::NotTaken.to_string();
        assert!(
            text.contains("in front") && text.contains("typed"),
            "got: {text}"
        );
    }

    #[test]
    fn taking_the_foreground_for_no_window_fails_rather_than_reporting_success() {
        // Zero is never a window, so the read-back cannot show it in front. A
        // version that trusted the call instead of the read would say the pack
        // window holds the keyboard while the letters go elsewhere.
        let outcome = take_foreground(WindowRef(0));
        if can_refuse_focus() {
            assert_eq!(outcome, Err(FocusError::NotTaken));
        } else {
            assert!(matches!(outcome, Err(FocusError::Unsupported { .. })));
        }
    }

    #[test]
    fn handing_back_to_a_window_that_never_took_the_focus_is_not_a_failure() {
        // `WindowRef(0)` is never the foreground window - `foreground_window`
        // answers `None` for a null handle - so this is the "we never took it"
        // branch, and it must be a success rather than a complaint.
        if !can_refuse_focus() {
            return;
        }
        assert_eq!(hand_back_foreground(WindowRef(0), None), Ok(()));
    }

    #[test]
    fn keeping_the_menu_off_no_window_is_refused_rather_than_claimed() {
        // Zero is never a window, so the system cannot subclass it. A version
        // that did not read the answer would report the menu kept off while
        // `Alt+Space` still opened it.
        let outcome = no_keyboard_menu(WindowRef(0));
        if can_refuse_focus() {
            assert_eq!(outcome, Err(KeyMenuError::Refused));
        } else {
            assert!(matches!(outcome, Err(KeyMenuError::Unsupported { .. })));
        }
        let text = KeyMenuError::Refused.to_string();
        assert!(
            text.contains("Alt+Space") && text.contains("next key"),
            "the cost is named: {text}"
        );
    }
}
