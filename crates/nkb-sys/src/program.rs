//! The name of the program a window belongs to (UX8, `D104`).
//!
//! # Why the palette asks
//!
//! After a value goes out, the palette says where it went - "to chrome.exe, a
//! text field" - so a tester who switched windows a moment before the press
//! sees it at once, not in a ticket a day later (`UX-GUI-013`). The owner chose
//! what may be read: the program and the kind of control, never the window's
//! title, never anything that watches which windows the tester moves between.
//!
//! # What this reads, and what it does not
//!
//! The process behind the window is opened with
//! `PROCESS_QUERY_LIMITED_INFORMATION` - the narrowest right there is, the one
//! `privilege.rs` already uses - and asked for its image path with
//! `QueryFullProcessImageNameW`. Only the FILE NAME leaves this module: the
//! folder is cut off here, because a path can name the user's own folders
//! (`C:\Users\<name>\...`) and nobody asked for it. Nothing of the window is
//! read - not its title, not its class, not its contents - and nothing of the
//! process beyond that one name: no memory, no command line, no version
//! resource. `tests/program_reads_only_its_name.rs` holds that to a list.
//!
//! "Does not read the contents of windows" (untouchable rule 17) stands
//! unchanged: the name of a program is not the content of its window.

use crate::WindowRef;

/// The file name of the program `window` belongs to - `chrome.exe` - or `None`
/// when the window is gone, the process refuses the question (a service, an
/// account of somebody else), or on a system where this is not asked yet.
#[must_use]
pub fn program_name(window: WindowRef) -> Option<String> {
    platform::image_path(window).and_then(|path| file_name(&path))
}

/// The longest path the system hands out, in UTF-16 units - the extended
/// limit, not `MAX_PATH`, so a program in a deep folder is still named.
const PATH_LIMIT: usize = 32_768;

/// The last part of a path, after either kind of slash - or `None` for a path
/// that ends in one or holds nothing. A unit that is not valid UTF-16 becomes
/// U+FFFD: the name is shown, never compared.
fn file_name(path: &[u16]) -> Option<String> {
    let start = path
        .iter()
        .rposition(|unit| *unit == u16::from(b'\\') || *unit == u16::from(b'/'))
        .map_or(0, |at| at + 1);
    let name = path.get(start..)?;
    (!name.is_empty()).then(|| String::from_utf16_lossy(name))
}

#[cfg(windows)]
mod platform {
    use crate::WindowRef;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, HWND};
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
        QueryFullProcessImageNameW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;

    use super::PATH_LIMIT;

    /// A handle this module opened, closed on every path out.
    struct Owned(HANDLE);

    impl Drop for Owned {
        fn drop(&mut self) {
            unsafe { CloseHandle(self.0) };
        }
    }

    pub fn image_path(window: WindowRef) -> Option<Vec<u16>> {
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(window.0 as usize as HWND, &mut pid) };
        if pid == 0 {
            return None;
        }
        let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if process.is_null() {
            return None;
        }
        let process = Owned(process);
        let mut buffer = vec![0u16; PATH_LIMIT];
        // In: the buffer's size in units. Out: the units written, without the
        // terminating zero.
        let mut size = u32::try_from(PATH_LIMIT).unwrap_or(u32::MAX);
        let read = unsafe {
            QueryFullProcessImageNameW(
                process.0,
                PROCESS_NAME_WIN32,
                buffer.as_mut_ptr(),
                &mut size,
            )
        };
        if read == 0 {
            return None;
        }
        buffer.truncate(usize::try_from(size).unwrap_or(0).min(PATH_LIMIT));
        Some(buffer)
    }
}

#[cfg(not(windows))]
mod platform {
    use crate::WindowRef;

    // No route to type into other windows exists here yet, so there is no
    // window a value went to and nothing to name.
    pub fn image_path(_window: WindowRef) -> Option<Vec<u16>> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn units(text: &str) -> Vec<u16> {
        text.encode_utf16().collect()
    }

    #[test]
    fn only_the_file_name_leaves_and_the_folders_stay_behind() {
        let path = units(r"C:\Users\someone\AppData\Local\Programs\tool\tool.exe");
        assert_eq!(file_name(&path).as_deref(), Some("tool.exe"));
        assert_eq!(
            file_name(&units("/usr/bin/gedit")).as_deref(),
            Some("gedit")
        );
        assert_eq!(
            file_name(&units("notepad.exe")).as_deref(),
            Some("notepad.exe")
        );
    }

    #[test]
    fn a_path_ending_in_a_slash_or_holding_nothing_names_nothing() {
        assert_eq!(file_name(&units(r"C:\folder\")), None);
        assert_eq!(file_name(&[]), None);
    }

    #[test]
    fn a_name_that_is_not_valid_utf16_is_still_shown() {
        // A lone surrogate is a legal file name on Windows.
        let mut path = units(r"C:\x\");
        path.extend([0xD800, u16::from(b'a')]);
        assert_eq!(file_name(&path).as_deref(), Some("\u{FFFD}a"));
    }

    #[test]
    fn a_window_that_does_not_exist_has_no_program() {
        assert_eq!(program_name(WindowRef(0)), None);
    }
}
