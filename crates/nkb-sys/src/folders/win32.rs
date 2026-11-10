//! The folder window and Explorer on Windows (`D175`).
//!
//! # The folder window
//!
//! The common item dialog (`IFileOpenDialog`) told to pick folders
//! (`FOS_PICKFOLDERS`) that are folders of the file system
//! (`FOS_FORCEFILESYSTEM`) - not a library, not "This PC". It needs a
//! single-threaded apartment, so it is made on a thread of its own that enters
//! one, and that thread runs its modal loop while the main thread goes on
//! drawing. Owned by the caller's window across the two threads, the way `rfd`
//! 0.18.0 does it: the folder window stays above it and the owner is disabled
//! while it stands.
//!
//! It does not add the folder to the tester's recent places
//! (`FOS_DONTADDTORECENT`) and does not change this process's working directory
//! (`FOS_NOCHANGEDIR`).
//!
//! The positions in the method tables are taken from the `windows` crate 0.62.2
//! (`IModalWindow_Vtbl`, `IFileDialog_Vtbl`, `IShellItem_Vtbl`), not counted from
//! memory, 2026-11-09. `IUnknown` takes 0 to 2 of every table.
//!
//! # Explorer
//!
//! `explorer.exe` from the system's own Windows directory, never by name, which
//! would search the program's directory and the path first. The folder is handed
//! over in quotes, written by hand: Explorer reads its command line by rules of
//! its own, and a comma outside quotes is one of its separators. A quote cannot
//! stand in a Windows file name, so no folder can close the quotes early.

use core::ffi::c_void;
use core::ptr::{null, null_mut};
use std::ffi::OsString;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use windows_sys::Win32::Foundation::ERROR_CANCELLED;
use windows_sys::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoCreateInstance,
    CoInitializeEx, CoTaskMemFree, CoUninitialize,
};
use windows_sys::Win32::System::SystemInformation::GetSystemWindowsDirectoryW;
use windows_sys::Win32::UI::Shell::{
    FOS_DONTADDTORECENT, FOS_FORCEFILESYSTEM, FOS_NOCHANGEDIR, FOS_PATHMUSTEXIST, FOS_PICKFOLDERS,
    FileOpenDialog, SHCreateItemFromParsingName, SIGDN_FILESYSPATH,
};
use windows_sys::core::{GUID, HRESULT, PWSTR};

use super::{PickAnswer, PickError, PickRequest, Picked, ShowError};

const IID_IFILEOPENDIALOG: GUID = GUID::from_u128(0xd57c7288_d4ad_4768_be02_9d969532d960);
const IID_ISHELLITEM: GUID = GUID::from_u128(0x43826d1e_e718_42ee_bc55_a1e261c37bfe);

/// What `Show` answers when the person closed the window without choosing:
/// `HRESULT_FROM_WIN32(ERROR_CANCELLED)`.
const CANCELLED: HRESULT = (0x8007_0000 | ERROR_CANCELLED) as HRESULT;

const SLOT_RELEASE: usize = 2;
const SLOT_SHOW: usize = 3;
const SLOT_SET_OPTIONS: usize = 9;
const SLOT_GET_OPTIONS: usize = 10;
const SLOT_SET_FOLDER: usize = 12;
const SLOT_SET_TITLE: usize = 17;
const SLOT_GET_RESULT: usize = 20;
/// Of `IShellItem`.
const SLOT_GET_DISPLAY_NAME: usize = 5;

type Raw = *mut c_void;

/// The function at `slot` of `object`'s method table.
///
/// # Safety
///
/// `object` must be a live COM interface pointer whose table has `slot`, and
/// `F` must be that method's exact signature.
unsafe fn method<F: Copy>(object: Raw, slot: usize) -> F {
    let table = unsafe { *object.cast::<*const *const c_void>() };
    let entry = unsafe { *table.add(slot) };
    unsafe { core::mem::transmute_copy::<*const c_void, F>(&entry) }
}

/// A COM reference this module holds, released on every path out.
struct Com(Raw);

impl Com {
    /// The reference a call handed out, or `None` when it handed out none.
    /// Every `Com` is made here, so none holds a null pointer for `method` to
    /// read through (CodeQL `rust/access-invalid-pointer` knows only this
    /// shape of the check, not one joined to the call's result by `||`).
    fn owned(raw: Raw) -> Option<Self> {
        if raw.is_null() { None } else { Some(Self(raw)) }
    }
}

impl Drop for Com {
    fn drop(&mut self) {
        let release: unsafe extern "system" fn(Raw) -> u32 =
            unsafe { method(self.0, SLOT_RELEASE) };
        unsafe { release(self.0) };
    }
}

/// The single-threaded apartment of the folder window's thread, left when the
/// thread is done. Declared before the references, so it is dropped after them.
struct Apartment;

impl Apartment {
    fn enter() -> Result<Self, PickError> {
        let entered = unsafe {
            CoInitializeEx(
                null(),
                (COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) as u32,
            )
        };
        // A failure is a system that would not give one, and is not undone.
        // S_OK and S_FALSE both entered, and both are left on drop. The thread
        // is ours and new, so "already in another apartment" cannot be the
        // answer - it would be a failure here like any other.
        if entered < 0 {
            return Err(failed(
                "the system gave the folder window no apartment",
                entered,
            ));
        }
        Ok(Self)
    }
}

impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

fn failed(what: &str, code: HRESULT) -> PickError {
    PickError::Failed(format!("{what} (0x{:08X})", code.cast_unsigned()))
}

/// `text` as the system's wide strings, with the closing zero.
fn wide(text: &std::ffi::OsStr) -> Vec<u16> {
    text.encode_wide().chain(Some(0)).collect()
}

pub(crate) fn pick_folder(request: PickRequest, answer: PickAnswer) {
    super::on_a_thread(
        "nkb-folder-window",
        move || pick_on_this_thread(&request),
        answer,
        |error| {
            Err(PickError::Failed(format!(
                "the system gave the folder window no thread: {error}"
            )))
        },
        |_| Err(PickError::Failed(String::from(super::FAULT))),
    );
}

fn pick_on_this_thread(request: &PickRequest) -> Result<Picked, PickError> {
    let _apartment = Apartment::enter()?;
    let mut raw: Raw = null_mut();
    let made = unsafe {
        CoCreateInstance(
            &FileOpenDialog,
            null_mut(),
            CLSCTX_INPROC_SERVER,
            &IID_IFILEOPENDIALOG,
            &mut raw,
        )
    };
    if made < 0 {
        return Err(failed("the folder window could not be made", made));
    }
    let dialog =
        Com::owned(raw).ok_or_else(|| failed("the folder window could not be made", made))?;
    configure(&dialog, request)?;
    let show: unsafe extern "system" fn(Raw, *mut c_void) -> HRESULT =
        unsafe { method(dialog.0, SLOT_SHOW) };
    let owner = request
        .owner
        .and_then(|handle| isize::try_from(handle).ok())
        .map_or(null_mut(), |handle| handle as *mut c_void);
    let shown = unsafe { show(dialog.0, owner) };
    if shown == CANCELLED {
        return Ok(Picked::Cancelled);
    }
    if shown < 0 {
        return Err(failed("the folder window could not be shown", shown));
    }
    chosen(&dialog).map(Picked::Folder)
}

/// Folders of the file system only, the title, and the folder to open in.
fn configure(dialog: &Com, request: &PickRequest) -> Result<(), PickError> {
    let get_options: unsafe extern "system" fn(Raw, *mut u32) -> HRESULT =
        unsafe { method(dialog.0, SLOT_GET_OPTIONS) };
    let set_options: unsafe extern "system" fn(Raw, u32) -> HRESULT =
        unsafe { method(dialog.0, SLOT_SET_OPTIONS) };
    let mut options = 0u32;
    let read = unsafe { get_options(dialog.0, &mut options) };
    if read < 0 {
        return Err(failed(
            "the folder window's options could not be read",
            read,
        ));
    }
    let wanted = options
        | FOS_PICKFOLDERS
        | FOS_FORCEFILESYSTEM
        | FOS_PATHMUSTEXIST
        | FOS_NOCHANGEDIR
        | FOS_DONTADDTORECENT;
    let set = unsafe { set_options(dialog.0, wanted) };
    if set < 0 {
        return Err(failed("the folder window would not pick folders", set));
    }
    let set_title: unsafe extern "system" fn(Raw, *const u16) -> HRESULT =
        unsafe { method(dialog.0, SLOT_SET_TITLE) };
    let title = wide(std::ffi::OsStr::new(&request.title));
    // A window without our title is still the folder window: not a failure.
    let _ = unsafe { set_title(dialog.0, title.as_ptr()) };
    if let Some(start) = &request.start {
        open_in(dialog, start);
    }
    Ok(())
}

/// Opens the window in `start`. A folder the system cannot parse leaves the
/// window where the system would have opened it, which is no reason to refuse.
fn open_in(dialog: &Com, start: &Path) {
    let path = wide(start.as_os_str());
    let mut raw: Raw = null_mut();
    let parsed = unsafe {
        SHCreateItemFromParsingName(path.as_ptr(), null_mut(), &IID_ISHELLITEM, &mut raw)
    };
    if parsed < 0 {
        return;
    }
    let Some(item) = Com::owned(raw) else {
        return;
    };
    let set_folder: unsafe extern "system" fn(Raw, Raw) -> HRESULT =
        unsafe { method(dialog.0, SLOT_SET_FOLDER) };
    let _ = unsafe { set_folder(dialog.0, item.0) };
}

/// The folder the person chose, as a path of the file system.
fn chosen(dialog: &Com) -> Result<PathBuf, PickError> {
    let get_result: unsafe extern "system" fn(Raw, *mut Raw) -> HRESULT =
        unsafe { method(dialog.0, SLOT_GET_RESULT) };
    let mut raw: Raw = null_mut();
    let got = unsafe { get_result(dialog.0, &mut raw) };
    if got < 0 {
        return Err(failed("the folder chosen could not be read", got));
    }
    let item = Com::owned(raw).ok_or_else(|| failed("the folder chosen could not be read", got))?;
    let display_name: unsafe extern "system" fn(Raw, i32, *mut PWSTR) -> HRESULT =
        unsafe { method(item.0, SLOT_GET_DISPLAY_NAME) };
    let mut name: PWSTR = null_mut();
    let named = unsafe { display_name(item.0, SIGDN_FILESYSPATH, &mut name) };
    if named < 0 || name.is_null() {
        return Err(failed("the folder chosen has no path", named));
    }
    let units = unsafe {
        let mut length = 0;
        while *name.add(length) != 0 {
            length += 1;
        }
        core::slice::from_raw_parts(name, length)
    };
    let path = PathBuf::from(OsString::from_wide(units));
    unsafe { CoTaskMemFree(name.cast()) };
    Ok(path)
}

/// The system's Windows directory, from the system rather than the environment.
fn windows_directory() -> Option<PathBuf> {
    let mut buffer = [0u16; 260];
    let length = unsafe { GetSystemWindowsDirectoryW(buffer.as_mut_ptr(), buffer.len() as u32) };
    let length = usize::try_from(length).ok()?;
    (length > 0 && length < buffer.len())
        .then(|| PathBuf::from(OsString::from_wide(&buffer[..length])))
}

/// Starts Explorer on `folder`, which the caller has found to be a folder.
pub(crate) fn show_folder(folder: &Path) -> Result<(), ShowError> {
    let quoted = quoted(folder).ok_or(ShowError::NotAFolder)?;
    let explorer = windows_directory()
        .map(|directory| directory.join("explorer.exe"))
        .ok_or_else(|| ShowError::Failed(String::from("the Windows directory is unknown")))?;
    Command::new(explorer)
        .raw_arg(quoted)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(drop)
        .map_err(|error| ShowError::Failed(error.to_string()))
}

/// `folder` in quotes, as Explorer's one argument - or `None` for a path that
/// holds a quote or a zero. Neither can stand in a Windows file name, so such
/// a path is no folder, and on the command line a quote would end the argument
/// and let the rest of the path be read as Explorer's own switches. The caller
/// found the path to be a folder, which already rules both out: this keeps the
/// rule beside the command line it protects.
fn quoted(folder: &Path) -> Option<OsString> {
    if folder
        .as_os_str()
        .encode_wide()
        .any(|unit| unit == u16::from(b'"') || unit == 0)
    {
        return None;
    }
    let mut quoted = OsString::from("\"");
    quoted.push(folder.as_os_str());
    quoted.push("\"");
    Some(quoted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_goes_to_explorer_whole_in_one_pair_of_quotes() {
        assert_eq!(
            quoted(Path::new(r"C:\Team packs\a,b")),
            Some(OsString::from(r#""C:\Team packs\a,b""#))
        );
    }

    #[test]
    fn a_path_that_could_end_the_quotes_never_reaches_explorer() {
        assert_eq!(quoted(Path::new(r#"C:\a" /root,"C:\b"#)), None);
        assert_eq!(quoted(Path::new("C:\\a\0b")), None);
    }
}
