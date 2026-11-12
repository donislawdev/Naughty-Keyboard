//! Which applications a send does not follow by joining their input queue
//! (`D182`, `OBS-249`).
//!
//! A send follows the application's pace by joining our thread to the input
//! queue of the thread that holds the focus and reading its key state (`D95`).
//! WPF does not survive that. Measured 2026-11-12 on Windows Server 2025
//! (`tools/petla-aplikacji.ps1 -Slad` and a twin of this send in C#, 1000 and
//! 11 characters into a WPF `TextBox` and `RichTextBox`): while another thread
//! is joined to its queue, WPF hands each change of text to the Text Services
//! Framework through a call that waits in a COM modal loop, and that loop takes
//! the key messages arriving meanwhile out of the queue and drops them. The key
//! state still moves, so a joined send counts them as taken. From 1 to 205 of
//! 1000 were lost in every way of following that was tried (each event, each
//! unit, one unit ahead, joined only for each read, a pause after each unit),
//! and none without the join at any pace (all at once, 1 ms and 5 ms apart).
//! So the queue of a WPF window is not joined: the send goes on confirmed by
//! the system alone (`D94`) and says that it could not follow the application
//! (`D95` point 2).
//!
//! WPF is told by the class of the window that holds the focus. Every WPF
//! window is an `HwndSource`, whose class is `HwndWrapper[` followed by the
//! name of the application's domain and an id. Only the beginning is read: the
//! buffer is one unit longer than the longest prefix below, so the system cuts
//! the name off before the part that names the program. The list is closed and
//! measured, like the terminal classes of `field.rs` (`D77`): a new entry is a
//! measurement, not a guess.

/// The beginnings of the window classes whose input queue a send does not join.
pub(crate) const UNFOLLOWED_CLASSES: &[&str] = &["HwndWrapper["];

/// How many UTF-16 units of a window class are read: the longest prefix in
/// [`UNFOLLOWED_CLASSES`], and not one more.
pub(crate) const CLASS_UNITS_READ: usize = 12;

/// Whether a send may join the input queue of a window whose class begins with
/// `class` - at most [`CLASS_UNITS_READ`] units of it.
pub(crate) fn joinable(class: &[u16]) -> bool {
    !UNFOLLOWED_CLASSES.iter().any(|prefix| {
        let mut units = class.iter().copied();
        prefix.encode_utf16().all(|unit| units.next() == Some(unit))
    })
}

#[cfg(windows)]
pub(crate) use platform::window_joinable;

#[cfg(windows)]
mod platform {
    use super::{CLASS_UNITS_READ, joinable};
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetClassNameW;

    /// The buffer the class is read into: the system writes at most one unit
    /// fewer than its length, then the terminating zero.
    const BUFFER_UNITS: usize = CLASS_UNITS_READ + 1;

    /// Whether a send may join the input queue of `window`'s thread. A class
    /// that cannot be read is joined, as every window was before `D182`.
    pub(crate) fn window_joinable(window: HWND) -> bool {
        let mut class = [0u16; BUFFER_UNITS];
        let length = unsafe { GetClassNameW(window, class.as_mut_ptr(), BUFFER_UNITS as i32) };
        let read = usize::try_from(length).unwrap_or(0).min(CLASS_UNITS_READ);
        joinable(&class[..read])
    }
}

#[cfg(test)]
mod tests {
    use super::{CLASS_UNITS_READ, UNFOLLOWED_CLASSES, joinable};

    /// The class as the system gives it, cut at [`CLASS_UNITS_READ`] units as
    /// the read cuts it.
    fn read(class: &str) -> Vec<u16> {
        class.encode_utf16().take(CLASS_UNITS_READ).collect()
    }

    #[test]
    fn a_wpf_window_is_not_joined() {
        // The classes WPF gives its windows: the application domain's name and
        // an id after the bracket - never read, the read stops before them.
        for class in [
            "HwndWrapper[pwsh;;8c3f0a51-62b4-4d1f-9e0a-0f5d2c7d9a11]",
            "HwndWrapper[DefaultDomain;;5a8e]",
            "HwndWrapper[",
        ] {
            assert!(!joinable(&read(class)), "{class} was joined");
        }
    }

    #[test]
    fn every_other_window_is_joined_as_before() {
        // Classes of the applications the send was measured in (`D95`, `D181`),
        // near misses of the WPF prefix, and nothing at all.
        for class in [
            "Notepad",
            "Edit",
            "Chrome_WidgetWin_1",
            "MozillaWindowClass",
            "ConsoleWindowClass",
            "CASCADIA_HOSTING_WINDOW_CLASS",
            "WindowsForms10.EDIT.app.0.141b42a_r6_ad1",
            "Winit Thread Event Target",
            "HwndWrapper",
            "hwndwrapper[",
            "XHwndWrapper[",
            "",
        ] {
            assert!(joinable(&read(class)), "{class} was not joined");
        }
    }

    #[test]
    fn the_read_reaches_the_end_of_the_longest_prefix_and_no_further() {
        // Shorter, and a WPF window would be told by part of its prefix only.
        // Longer, and the read would reach the name of the program.
        let longest = UNFOLLOWED_CLASSES
            .iter()
            .map(|prefix| prefix.encode_utf16().count())
            .max();
        assert_eq!(longest, Some(CLASS_UNITS_READ));
    }
}
