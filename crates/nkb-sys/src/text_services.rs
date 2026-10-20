//! Which automatic text services the person has switched on (macOS).
//!
//! # What this reads, and what it does not
//!
//! Three booleans of the user's own preferences - the global keys the keyboard
//! settings write, below. Nothing of any window, any field or any application:
//! the answer would be the same with every window closed. The tool asks so that
//! it can say, after a value that the services could have changed, that they are
//! on (`nkb_core::substitutions`).
//!
//! | service | key in the global domain |
//! |---|---|
//! | capitalise the first letter of a sentence | `NSAutomaticCapitalizationEnabled` |
//! | a period after two spaces | `NSAutomaticPeriodSubstitutionEnabled` |
//! | curly quotes | `NSAutomaticQuoteSubstitutionEnabled` |
//!
//! # A key nobody wrote
//!
//! The keyboard settings write a key only once someone has changed the switch.
//! Measured 2026-10-20 on macOS 27.0, a Mac nobody had touched held the first two
//! keys set to true and the quotes key absent - and a rich text document there
//! made curly quotes, so an absent quotes key means ON. The other two keys were
//! never absent on the machine measured, so what their absence means is not known
//! and `UNSET_MEANS` says off, because the tool states only what it has seen.

/// The three services, on or off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TextServices {
    pub capitalization: bool,
    pub period: bool,
    pub quotes: bool,
}

/// What a key that is not in the preferences means, per service: only the
/// quotes were measured with the key absent (TextEdit, 2026-10-20).
#[cfg(any(target_os = "macos", test))]
pub(crate) const UNSET_MEANS: TextServices = TextServices {
    capitalization: false,
    period: false,
    quotes: true,
};

#[cfg(any(target_os = "macos", test))]
impl TextServices {
    /// The services from what the three keys held: `None` for a key nobody wrote.
    pub(crate) const fn from_preferences(
        capitalization: Option<bool>,
        period: Option<bool>,
        quotes: Option<bool>,
    ) -> Self {
        Self {
            capitalization: unwrap_or(capitalization, UNSET_MEANS.capitalization),
            period: unwrap_or(period, UNSET_MEANS.period),
            quotes: unwrap_or(quotes, UNSET_MEANS.quotes),
        }
    }
}

#[cfg(any(target_os = "macos", test))]
const fn unwrap_or(value: Option<bool>, default: bool) -> bool {
    match value {
        Some(value) => value,
        None => default,
    }
}

/// The text services that are on for this user. All off on a system that has
/// none of these (Windows and Linux text controls do not rewrite what is typed).
#[must_use]
pub fn text_services() -> TextServices {
    #[cfg(target_os = "macos")]
    {
        crate::macos::text_services()
    }
    #[cfg(not(target_os = "macos"))]
    {
        TextServices::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_that_is_set_wins_over_the_default() {
        let all_off = TextServices::from_preferences(Some(false), Some(false), Some(false));
        assert_eq!(all_off, TextServices::default());
        let all_on = TextServices::from_preferences(Some(true), Some(true), Some(true));
        assert_eq!(
            all_on,
            TextServices {
                capitalization: true,
                period: true,
                quotes: true
            }
        );
    }

    #[test]
    fn a_key_nobody_wrote_means_the_default_of_the_text_system() {
        assert_eq!(
            TextServices::from_preferences(None, None, None),
            UNSET_MEANS
        );
    }

    #[test]
    fn this_system_may_have_no_services_at_all() {
        // Windows and Linux answer all off, whatever the build asked.
        if cfg!(not(target_os = "macos")) {
            assert_eq!(text_services(), TextServices::default());
        }
    }
}
