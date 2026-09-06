//! Two kinds of text, and the single escaping layer between them.
//!
//! # Why two types and not two `String`s
//!
//! A pack file stores a value in its *escaped* form, and the tool sends its
//! *literal* form to somebody else's field. Confusing the two means typing six
//! visible characters `\u200B` into a field that was supposed to receive one
//! invisible one - a silent falsehood produced by the tool whose entire purpose
//! is to find silent falsehoods elsewhere.
//!
//! This is not hypothetical. The project has a rule about it precisely because
//! the mistake kept happening in the documentation, by hand.
//!
//! # There is exactly one escaping layer, and it belongs to TOML
//!
//! The pack format deliberately does not add a second escaping layer on top of
//! the file format. Reading `\u200B` back into a character is therefore the TOML
//! parser's job, not ours - measured, not assumed. What this module owns is the
//! other direction and the question that comes with it:
//!
//! - which characters *must* be escaped when the tool writes a pack file,
//! - how to render them.
//!
//! So there is `escape`, and there is no `unescape`. That asymmetry is the
//! design, not an omission.

use core::fmt;

/// Text as it appears inside a pack file, before the TOML parser touches it.
///
/// Never sent to a field. The only way to produce one is [`escape`].
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EscapedText(String);

/// Text as it will arrive in the target field, after the TOML parser expanded it.
///
/// This is what the tool types into somebody else's application.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LiteralText(String);

impl EscapedText {
    /// Wraps text that is already in escaped form, for example straight from a
    /// pack file that has not been re-encoded.
    #[must_use]
    pub fn from_file(text: impl Into<String>) -> Self {
        Self(text.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl LiteralText {
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self {
        Self(text.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Renders this text in the form a pack file stores.
    #[must_use]
    pub fn escape(&self) -> EscapedText {
        let mut out = String::with_capacity(self.0.len());
        let chars: Vec<char> = self.0.chars().collect();

        for (index, &character) in chars.iter().enumerate() {
            let edge = index == 0 || index == chars.len() - 1;
            if needs_escaping(character, edge) {
                write_escape(&mut out, character);
            } else if character == '\\' {
                // A literal backslash has to survive the round trip, otherwise a
                // value that merely *looks* like an escape sequence would become
                // one. That case is a real test value, not a curiosity.
                out.push_str("\\\\");
            } else {
                out.push(character);
            }
        }
        EscapedText(out)
    }
}

impl fmt::Display for EscapedText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Whether a character must be written escaped rather than literally.
///
/// `at_edge` marks the first and last character of a value, where an ordinary
/// space is meaningful and therefore must not be left bare - a trailing space is
/// one of the most common test values there is, and the one most likely to be
/// eaten by an editor that trims lines on save.
#[must_use]
pub fn needs_escaping(character: char, at_edge: bool) -> bool {
    if character == ' ' {
        return at_edge;
    }
    character.is_control()
        || is_format_character(character)
        || (character.is_whitespace() && character != ' ')
}

/// Characters that carry meaning without carrying a shape: zero width, joiners,
/// direction marks, the byte order mark, and the tag block.
///
/// Note what is **not** here: emoji. They have a visible shape and pretend to be
/// nothing, so they are written literally. A family emoji is a mixture of both
/// rules - the pictures stay literal, the joiners between them are escaped -
/// and that mixture is the whole point, because it is the invisible joiners
/// that make one apparent character count as several.
#[must_use]
pub fn is_format_character(character: char) -> bool {
    matches!(character as u32,
        0x00AD                    // soft hyphen
        | 0x200B..=0x200F         // zero width space, joiners, direction marks
        | 0x202A..=0x202E         // bidirectional overrides
        | 0x2060..=0x2064         // word joiner and invisible operators
        | 0xFEFF                  // byte order mark
        | 0x1D173..=0x1D17A       // musical formatting, outside the basic plane
        | 0xE0001                 // language tag
        | 0xE0020..=0xE007F       // tag characters - invisible, and a known trick
    )
}

fn write_escape(out: &mut String, character: char) {
    let code = character as u32;
    if code <= 0xFFFF {
        out.push_str(&format!("\\u{code:04X}"));
    } else {
        // Outside the basic plane TOML uses the eight digit form. Writing the
        // four digit one here would produce a file that does not parse.
        out.push_str(&format!("\\U{code:08X}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_is_left_alone() {
        let literal = LiteralText::new("Jan Kowalski");
        assert_eq!(literal.escape().as_str(), "Jan Kowalski");
    }

    #[test]
    fn zero_width_space_is_escaped_not_passed_through() {
        let literal = LiteralText::new("Jan\u{200B}Kowalski");
        assert_eq!(literal.escape().as_str(), "Jan\\u200BKowalski");
    }

    #[test]
    fn leading_and_trailing_spaces_are_escaped_inner_ones_are_not() {
        let literal = LiteralText::new(" a b ");
        assert_eq!(literal.escape().as_str(), "\\u0020a b\\u0020");
    }

    #[test]
    fn an_emoji_stays_literal_because_it_pretends_to_be_nothing() {
        // The format escapes what hides, not what is merely large. An emoji has
        // a visible shape, so escaping it would only make the file harder to read.
        let literal = LiteralText::new("\u{1F468}");
        assert_eq!(literal.escape().as_str(), "\u{1F468}");
    }

    #[test]
    fn a_family_emoji_keeps_the_pictures_and_escapes_the_joiners() {
        // The mixed case from the format document, and the reason it matters:
        // written out whole, the file would show one family and nobody would see
        // the two invisible joiners that make counters disagree about its length.
        let literal = LiteralText::new("\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}");
        assert_eq!(
            literal.escape().as_str(),
            "\u{1F468}\\u200D\u{1F469}\\u200D\u{1F467}"
        );
    }

    #[test]
    fn a_format_character_outside_the_basic_plane_uses_the_eight_digit_form() {
        // A tag character: invisible, outside the basic plane, and a known way to
        // smuggle data through text. A four digit escape cannot express it, so
        // getting this wrong produces a file that does not parse.
        let literal = LiteralText::new("a\u{E0041}b");
        assert_eq!(literal.escape().as_str(), "a\\U000E0041b");
    }

    #[test]
    fn a_backslash_survives_so_a_value_cannot_impersonate_an_escape() {
        let literal = LiteralText::new("\\u200B");
        assert_eq!(literal.escape().as_str(), "\\\\u200B");
    }

    #[test]
    fn non_breaking_space_is_escaped_even_in_the_middle() {
        let literal = LiteralText::new("Jan\u{00A0}Kowalski");
        assert_eq!(literal.escape().as_str(), "Jan\\u00A0Kowalski");
    }
}
