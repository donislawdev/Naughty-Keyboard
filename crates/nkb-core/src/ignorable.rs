//! Characters a reader cannot see, as the Unicode standard lists them.
//!
//! # The question this closes
//!
//! Two rules in this crate ask "can a person see this character": the escaping
//! of the pack format ([`crate::text::needs_escaping`], rule `E020`) and the
//! palette's preview ([`crate::preview::is_invisible`]). Until 2026-09-24 each
//! kept a hand-written list, and both lists were shorter than the principle they
//! serve - `pack-format.md` 2 escapes "every character a reviewer will not see".
//! Measured that day against the vendored `DerivedCoreProperties.txt`: 377
//! assigned code points with `Default_Ignorable_Code_Point` were marked by
//! neither, the Hangul filler U+3164 among them - a known way to make a name
//! look empty - and 272 of them passed `E020` written literally, which is to say
//! invisibly in a review of somebody's pack (`OBS-121`, `D79`).
//!
//! So the list is no longer written by hand. It is the standard's own property,
//! read from the file the grapheme table already comes from, and
//! `tests/unicode_data.rs` compares it with that file for every code point.
//!
//! # What the property leaves out, and who covers it
//!
//! - **controls, spaces and the two separators.** The standard keeps
//!   `White_Space` and most controls out of this property on purpose. They are
//!   named where they are used, in `preview` and `text`.
//! - **U+FFF9 to U+FFFC**, the interlinear annotation controls and the object
//!   replacement character. The standard expects them to be drawn, so they are
//!   not ignorable - and the shipped typeface draws nothing for them (measured
//!   with `D66`). [`crate::text::is_format_character`] names them.
//! - **format characters that ARE drawn** - the Arabic number signs from U+0600,
//!   the Syriac abbreviation mark. Outside the property, and rightly so.
//! - **the Egyptian hieroglyph format controls from U+13430 and the braille
//!   blank U+2800.** Outside the property and outside the shipped typeface, so
//!   the palette names them by code point as characters it cannot guarantee
//!   (`D66`) rather than drawing nothing.

mod table;

/// Whether `character` has the `Default_Ignorable_Code_Point` property.
///
/// Exactly the property of the vendored standard, unassigned code points in its
/// reserved ranges included - a character assigned there tomorrow is invisible
/// by the standard's own promise. Binary search over the table's ranges, a
/// handful of comparisons.
#[must_use]
pub fn is_default_ignorable(character: char) -> bool {
    let code = u32::from(character);
    table::DEFAULT_IGNORABLE
        .binary_search_by(|&(start, end)| {
            if end < code {
                std::cmp::Ordering::Less
            } else if start > code {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_characters_that_opened_obs_121_are_ignorable() {
        // The filler that makes a name look empty, a variation selector, the
        // grapheme joiner, a tag and the Mongolian vowel separator.
        for c in ['\u{3164}', '\u{FE0F}', '\u{034F}', '\u{E0041}', '\u{180E}'] {
            assert!(
                is_default_ignorable(c),
                "U+{:04X} is ignorable in the standard",
                u32::from(c)
            );
        }
    }

    #[test]
    fn drawn_characters_and_spaces_are_not() {
        // A letter, a space (White_Space, so outside the property by
        // definition), an Arabic number sign that is Cf and still drawn, and the
        // annotation anchor the standard expects to be drawn.
        for c in ['a', ' ', '\u{0600}', '\u{FFF9}'] {
            assert!(
                !is_default_ignorable(c),
                "U+{:04X} is not ignorable in the standard",
                u32::from(c)
            );
        }
    }
}
