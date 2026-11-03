//! The characters of a value that are pressed as the keys of their names, on every
//! route that presses keys (`D164`, R17).
//!
//! # What was measured before this was decided
//!
//! A tab and a line feed sent as text, the way every other character goes, did a
//! different thing on each system. On X11 they already went as the Tab and Return
//! keys of the layout, because a line feed through a spare key reached nothing in a
//! Chromium text area (`OBS-211`). On Windows, 2026-11-03, Edge on a page of its
//! own: a line feed as `VK_PACKET` was lost in a text area as well as in a
//! single-line input (`Jan`, line feed, `Kowalski` arrived as `JanKowalski`) while
//! `nkb send` said the application took every keystroke, and a tab went in as a
//! character, also in the input. On macOS a line feed on key code 0 was lost in
//! Chrome and Firefox and arrived in Safari (`OBS-194`), and the Return key with the
//! same character arrived in both.
//!
//! # What is decided here
//!
//! A tab is the Tab key and a line feed is the Return key, as a keyboard types them:
//! in a field that takes several lines a line feed starts a new one, in a
//! single-line field of a form it may send the form, and a tab may move the focus.
//! So a value means the same keys on every system that presses keys. Every other
//! character, a carriage return included, goes as text as before. Under Wayland
//! nothing is pressed: the text is handed to the input method whole, and a tab or a
//! line feed in it is a character.

/// A key a character of a value is pressed as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TextKey {
    Tab,
    Return,
}

/// The key `character` is pressed as, or `None` for a character that goes as text.
pub(crate) const fn text_key(character: u32) -> Option<TextKey> {
    match character {
        0x09 => Some(TextKey::Tab),
        0x0A => Some(TextKey::Return),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tab_and_a_line_feed_are_the_keys_of_their_names() {
        assert_eq!(text_key(0x09), Some(TextKey::Tab));
        assert_eq!(text_key(0x0A), Some(TextKey::Return));
    }

    #[test]
    fn every_other_character_goes_as_text_a_carriage_return_included() {
        for character in [
            0x00, 0x08, 0x0B, 0x0C, 0x0D, 0x20, 0x41, 0x85, 0x2028, 0x2029,
        ] {
            assert_eq!(text_key(character), None, "{character:#x}");
        }
    }
}
