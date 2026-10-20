//! The four text-service preferences, read from the global domain.
//!
//! Booleans only: a key that holds anything else is treated as not written.
//! `crate::text_services` has the keys and what an absent one means.

use core::ffi::CStr;

use super::ffi::{self, Owned};
use crate::text_services::TextServices;

/// The boolean a global preference holds, or `None` when nobody wrote the key
/// (or it holds something that is not a boolean).
fn flag(key: &CStr) -> Option<bool> {
    let name = ffi::cf_string(key)?;
    let value = Owned::new(unsafe {
        ffi::CFPreferencesCopyAppValue(name.0, ffi::kCFPreferencesAnyApplication)
    })?;
    if unsafe { ffi::CFGetTypeID(value.0) != ffi::CFBooleanGetTypeID() } {
        return None;
    }
    Some(unsafe { ffi::CFBooleanGetValue(value.0) } != 0)
}

/// The services that are on, asked fresh.
pub(crate) fn text_services() -> TextServices {
    TextServices::from_preferences(
        flag(c"NSAutomaticCapitalizationEnabled"),
        flag(c"NSAutomaticPeriodSubstitutionEnabled"),
        flag(c"NSAutomaticQuoteSubstitutionEnabled"),
    )
}
