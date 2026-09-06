//! The shape of a pack identifier and of a value identifier.
//!
//! # Why these are two functions and not one with a flag
//!
//! The two patterns differ in both ends. A pack identifier must start with a
//! letter and is at least two characters long; a value identifier may start with
//! a digit and may be a single character. `bool-no` and `len-255` are real value
//! identifiers from the catalogue, and a shared implementation with a switch
//! would invite one of the two to drift while the other stayed correct.
//!
//! # Why there is no regular expression here
//!
//! The core depends on nothing, which is the property that makes the dependency
//! rule enforceable rather than merely stated. A pattern this small costs less to
//! write out than the argument for adding a crate to read it.
//!
//! 🔴 An identifier is assigned once and never reused, never renamed and never
//! renumbered (`pack-format.md` 7). These functions decide what may enter that
//! one-way door, which is why the bounds below are written out rather than
//! approximated.

/// Shortest and longest pack identifier, in characters, from `pack-format.md` 7:
/// `^[a-z][a-z0-9-]{1,39}$`.
const PACK_ID_LENGTH: (usize, usize) = (2, 40);

/// The same for a value identifier: `^[a-z0-9][a-z0-9-]{0,47}$`.
const VALUE_ID_LENGTH: (usize, usize) = (1, 48);

/// Whether a pack identifier has the shape the format requires.
///
/// The file a pack lives in is named after this identifier, so a name outside
/// this alphabet is also a file name that behaves differently on three operating
/// systems.
#[must_use]
pub fn is_pack_id(text: &str) -> bool {
    has_shape(text, PACK_ID_LENGTH, |first| first.is_ascii_lowercase())
}

/// Whether a value identifier has the shape the format requires.
#[must_use]
pub fn is_value_id(text: &str) -> bool {
    has_shape(text, VALUE_ID_LENGTH, |first| {
        first.is_ascii_lowercase() || first.is_ascii_digit()
    })
}

/// The half both patterns share: a first character, a tail, and a length.
///
/// Length is counted in characters rather than bytes. For an identifier that
/// passes, the two numbers are equal - every character allowed here is one byte.
/// They differ only for input that is already rejected, and counting characters
/// is what makes the bound mean what the specification says it means.
fn has_shape(text: &str, length: (usize, usize), first_allowed: fn(char) -> bool) -> bool {
    let (min, max) = length;
    let count = text.chars().count();
    if count < min || count > max {
        return false;
    }

    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        // Unreachable while `min` is at least one, and written as a value rather
        // than as an assumption: the bounds above are data and may be edited.
        return false;
    };
    if !first_allowed(first) {
        return false;
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_identifiers_the_catalogue_already_uses_are_accepted() {
        for id in [
            "unicode-text",
            "magic-values",
            "export-breakers",
            "locale-pl",
        ] {
            assert!(is_pack_id(id), "{id}");
        }
        for id in [
            "zero-width-space-x3",
            "bool-no",
            "trailing-space",
            "len-255",
            "0",
        ] {
            assert!(is_value_id(id), "{id}");
        }
    }

    #[test]
    fn a_pack_identifier_must_begin_with_a_letter_and_a_value_one_need_not() {
        // The one place the two patterns genuinely differ at the front. A shared
        // implementation would be tempted to make them agree.
        assert!(!is_pack_id("0-day"));
        assert!(is_value_id("0-day"));
    }

    #[test]
    fn upper_case_and_underscores_and_spaces_are_refused_in_both() {
        for id in ["Whitespace", "magic_values", "magic values", "magic.values"] {
            assert!(!is_pack_id(id), "{id}");
            assert!(!is_value_id(id), "{id}");
        }
    }

    #[test]
    fn a_single_character_is_a_value_identifier_and_not_a_pack_one() {
        assert!(is_value_id("a"));
        assert!(!is_pack_id("a"));
        assert!(is_pack_id("ab"));
    }

    #[test]
    fn the_length_bounds_hold_on_both_sides_of_the_edge() {
        // Off by one here would let an identifier through that a second
        // implementation of the format would reject, which is the one kind of
        // disagreement a published pattern exists to prevent.
        let forty = "a".repeat(40);
        let forty_one = "a".repeat(41);
        assert!(is_pack_id(&forty));
        assert!(!is_pack_id(&forty_one));

        let forty_eight = "a".repeat(48);
        let forty_nine = "a".repeat(49);
        assert!(is_value_id(&forty_eight));
        assert!(!is_value_id(&forty_nine));
    }

    #[test]
    fn an_empty_identifier_is_refused_rather_than_crashing() {
        assert!(!is_pack_id(""));
        assert!(!is_value_id(""));
    }

    #[test]
    fn a_hyphen_may_not_start_an_identifier_but_may_end_one() {
        // Ending in a hyphen is untidy and the pattern allows it. Written down so
        // that a later session tightening the rule knows it is changing the
        // format rather than fixing a bug.
        assert!(!is_pack_id("-leading"));
        assert!(!is_value_id("-leading"));
        assert!(is_pack_id("trailing-"));
        assert!(is_value_id("trailing-"));
    }

    #[test]
    fn a_length_is_counted_in_characters_so_a_wide_identifier_cannot_slip_through() {
        // Twenty two characters, forty four bytes. Counting bytes would refuse
        // this for the wrong reason and hide the real one, which is the alphabet.
        let cyrillic = "аааааааааааааааааааааа";
        assert_eq!(cyrillic.chars().count(), 22);
        assert!(cyrillic.len() > 40);
        assert!(!is_pack_id(cyrillic));
    }
}
