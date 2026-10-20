//! The call behind `nkb_sys::layout::option_character`: what the keyboard layouts
//! of the Mac type for a key held with `Option`, asked of the layouts' own tables.
//!
//! # Why a copy
//!
//! The tables come from Text Input Sources, and its header (`TextInputSources.h`)
//! says: "TextInputSources API is not thread safe. If you are a UI application,
//! you must call TextInputSources API on the main thread." A shortcut is
//! considered on whatever thread the question reached, the worker of the palette
//! among them. So the enabled layouts are READ on the main thread only, each
//! layout's table copied out, and the question is answered from the copy on any
//! thread - `UCKeyTranslate` is a function of the table it is handed and keeps
//! nothing.
//!
//! The copy is taken afresh each time the main thread asks (the palette starts
//! there, and so does the window that records a shortcut), which costs two
//! thousand bytes and a fifth of a millisecond for the layout measured. A layout
//! the tester enables later is seen the next time the main thread asks. A thread
//! that is not the main one NEVER reads, and when nothing was ever read it has
//! nothing to say, which the caller takes as "no reason to refuse" (`D88`).

use std::sync::{Mutex, PoisonError};

use super::ffi::{self, Owned};
use crate::layout::{Translated, option_character_via};
use crate::macos_hotkeys::key_code;

/// More layouts than anyone enables. A larger count is not trusted, the same rule
/// as for the layouts on Windows.
const LAYOUT_LIMIT: isize = 64;

/// The most bytes one layout's table may have. The tables measured are a few
/// thousand, and a size past this is not copied.
const TABLE_LIMIT: isize = 4 << 20;

/// `kUCKeyActionDown`.
const KEY_DOWN: u16 = 0;

/// What the main thread read: the tables of the enabled layouts, and the kind of
/// keyboard they are asked for.
struct Snapshot {
    keyboard_type: u32,
    /// Each table in 8-byte words, because the system reads the table through
    /// 16- and 32-bit fields and a byte buffer has no such alignment.
    tables: Vec<Vec<u64>>,
}

static SNAPSHOT: Mutex<Snapshot> = Mutex::new(Snapshot {
    keyboard_type: 0,
    tables: Vec::new(),
});

fn on_main_thread() -> bool {
    unsafe { ffi::pthread_main_np() == 1 }
}

/// Copies the bytes of a `CFData` into words, or `None` for a table that is
/// empty or absurdly large.
fn copy_table(data: ffi::CfRef) -> Option<Vec<u64>> {
    let length = unsafe { ffi::CFDataGetLength(data) };
    let bytes = unsafe { ffi::CFDataGetBytePtr(data) };
    if bytes.is_null() || length <= 0 || length > TABLE_LIMIT {
        return None;
    }
    let length = usize::try_from(length).ok()?;
    let mut words = vec![0u64; length.div_ceil(size_of::<u64>())];
    // SAFETY: `length` bytes are readable at `bytes` (the data says so), and
    // `words` holds at least that many, each of which is written before it is read.
    unsafe {
        core::ptr::copy_nonoverlapping(bytes, words.as_mut_ptr().cast::<u8>(), length);
    }
    Some(words)
}

/// The tables of the enabled keyboard layouts, or `None` when the list could not
/// be made. MAIN THREAD ONLY.
fn read_enabled() -> Option<Snapshot> {
    // `includeAllInstalled` false: the layouts the tester can switch to.
    let list = Owned::new(unsafe { ffi::TISCreateInputSourceList(core::ptr::null(), 0) })?;
    let count = unsafe { ffi::CFArrayGetCount(list.0) }.min(LAYOUT_LIMIT);
    let mut tables = Vec::new();
    for index in 0..count {
        let source = unsafe { ffi::CFArrayGetValueAtIndex(list.0, index) };
        if source.is_null() {
            continue;
        }
        // Not owned: the value belongs to the source. An input source with no
        // layout of its own, such as the character palette, has none.
        let data = unsafe {
            ffi::TISGetInputSourceProperty(source, ffi::kTISPropertyUnicodeKeyLayoutData)
        };
        if data.is_null() {
            continue;
        }
        if let Some(table) = copy_table(data) {
            tables.push(table);
        }
    }
    Some(Snapshot {
        keyboard_type: u32::from(unsafe { ffi::LMGetKbdType() }),
        tables,
    })
}

/// Reads the layouts again, when this is the main thread. A failed reading leaves
/// what was read before.
pub(crate) fn refresh() {
    if !on_main_thread() {
        return;
    }
    if let Some(fresh) = read_enabled() {
        *SNAPSHOT.lock().unwrap_or_else(PoisonError::into_inner) = fresh;
    }
}

/// One `UCKeyTranslate` call against one table.
fn translate(
    table: &[u64],
    keyboard_type: u32,
    code: u16,
    modifiers: u32,
    dead_in: u32,
) -> Option<Translated> {
    let mut dead = dead_in;
    let mut written = 0usize;
    let mut buffer = [0u16; 8];
    let status = unsafe {
        ffi::UCKeyTranslate(
            table.as_ptr().cast(),
            code,
            KEY_DOWN,
            modifiers,
            keyboard_type,
            0,
            &raw mut dead,
            buffer.len(),
            &raw mut written,
            buffer.as_mut_ptr(),
        )
    };
    if status != ffi::NO_ERR {
        return None;
    }
    Some(Translated {
        units: buffer[..written.min(buffer.len())].to_vec(),
        dead,
    })
}

/// The first character any enabled layout types for `vk` with `Option`, if any.
pub(crate) fn option_character(vk: u16, shift: bool) -> Option<char> {
    let code = u16::try_from(key_code(vk)?).ok()?;
    refresh();
    let snapshot = SNAPSHOT.lock().unwrap_or_else(PoisonError::into_inner);
    snapshot.tables.iter().find_map(|table| {
        option_character_via(
            &|code, modifiers, dead| {
                translate(table, snapshot.keyboard_type, code, modifiers, dead)
            },
            code,
            shift,
        )
    })
}
