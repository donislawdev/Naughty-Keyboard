//! The line a tester types to find a pack, built one key press at a time.
//!
//! # Why the pack search has no text field (`OBS-145`)
//!
//! A Slint text field copies and pastes on its own: `Ctrl+C` and `Ctrl+X` write
//! the clipboard, `Ctrl+V` reads it, and on Linux a selection is a clipboard of
//! its own. Untouchable rule 17 promises the clipboard in two places, and a
//! field in the pack window would have been a third that no Rust call shows.
//! Swallowing the paste keys inside the field would hold the promise by
//! discipline - until the toolkit learns one more key. So the window has no
//! field: a focus scope hands every key press to [`Query::press`], and the view
//! draws [`Query::as_str`] as plain text. There is nothing to copy from and
//! nothing to paste into, by construction, and `clipboard_has_named_doors.rs`
//! keeps refusing every text field without an exception.
//!
//! What that costs, said here so nobody rediscovers it as a defect: no caret to
//! move, no selection, and no input-method composition - the toolkit enables an
//! input method only for its own text input. A search over pack names, tags and
//! descriptions needs none of the first two. The third means a query in Chinese
//! or Japanese cannot be typed, and the catalogue's text is English.
//!
//! # No toolkit here
//!
//! GUI rule 15: this type decides what a press means and knows nothing of the
//! window. The view reports the text a key produced and the modifiers held, and
//! gets back whether the press was the query's - so a key the query does not
//! want travels on instead of vanishing.

use nkb_core::graphemes::last_cluster_start;

/// What the window reports about one key press.
///
/// The toolkit's own names for the modifiers, not the physical keys: on macOS
/// Slint reports `Cmd` as `control` and `Ctrl` as `meta`, the way Qt does, so
/// `control` is "the shortcut modifier" on every system.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyPress<'a> {
    /// The text the key produced, or the toolkit's code for a key that
    /// produces none (a control character, or a character from the private
    /// use area for keys such as the arrows).
    pub text: &'a str,
    pub control: bool,
    pub alt: bool,
    pub meta: bool,
}

/// What a press did to the query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pressed {
    /// The query is different now, and the list has to be filtered again.
    Changed,
    /// The press was the query's and changed nothing - Backspace on an empty
    /// line. Consumed all the same, so it does not travel on.
    Unchanged,
    /// Not a press the query takes: a shortcut, or a key that types nothing.
    NotOurs,
}

/// The Backspace key, as the toolkit reports it.
const BACKSPACE: &str = "\u{8}";

/// The query of the pack search.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Query {
    text: String,
}

impl Query {
    /// The query as the window draws it and the list is filtered by.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// Applies one key press.
    ///
    /// Backspace erases the last character a person SEES - a grapheme cluster,
    /// so a letter with a combining accent goes in one press rather than
    /// leaving a bare letter behind. Text is appended when it is text and the
    /// press is not a shortcut. Everything else is not the query's.
    pub fn press(&mut self, press: KeyPress<'_>) -> Pressed {
        if press.text == BACKSPACE {
            if self.text.is_empty() {
                return Pressed::Unchanged;
            }
            self.text.truncate(last_cluster_start(&self.text));
            return Pressed::Changed;
        }
        if is_shortcut(press) || !is_typed_text(press.text) {
            return Pressed::NotOurs;
        }
        self.text.push_str(press.text);
        Pressed::Changed
    }
}

/// A press that means a command rather than a character.
///
/// `meta` always: the Windows key, and `Ctrl` on macOS. `control` or `alt`
/// ALONE with ASCII text: `Ctrl+V`, `Ctrl+A`, `Cmd+V`, `Alt+letter` - the paste
/// shortcut must not type a "v" into the query.
///
/// Both together are `AltGr` on Windows, where the system reports the right Alt
/// as `Ctrl+Alt`: the Polish layout types `U+0105` and the German one "@" that way.
/// Slint clears both flags when it can see the key was remapped, and when it
/// cannot, text that arrives with both held is still a character. Non-ASCII
/// text with one of them is a character too: `Option+a` on macOS types `U+00E5`.
fn is_shortcut(press: KeyPress<'_>) -> bool {
    press.meta || (press.control != press.alt && press.text.is_ascii())
}

/// Text a person typed, as opposed to the toolkit's code for a key.
///
/// The toolkit names keys that type nothing with control characters (Tab,
/// Enter, Escape, Delete, the modifiers themselves) and with characters from
/// the private use area (the arrows, Home, the function keys) - read in
/// `i-slint-common` 1.18.1, `key_codes.rs`. Neither kind is ever the query.
fn is_typed_text(text: &str) -> bool {
    !text.is_empty() && text.chars().all(|c| !c.is_control() && !is_private_use(c))
}

const fn is_private_use(c: char) -> bool {
    matches!(
        c,
        '\u{E000}'..='\u{F8FF}' | '\u{F0000}'..='\u{FFFFD}' | '\u{100000}'..='\u{10FFFD}'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(text: &str) -> KeyPress<'_> {
        KeyPress {
            text,
            control: false,
            alt: false,
            meta: false,
        }
    }

    fn typed(presses: &[KeyPress<'_>]) -> String {
        let mut query = Query::default();
        for press in presses {
            query.press(*press);
        }
        query.as_str().to_owned()
    }

    #[test]
    fn letters_and_spaces_are_the_query() {
        assert_eq!(typed(&[key("u"), key("n"), key(" "), key("i")]), "un i");
    }

    #[test]
    fn a_paste_shortcut_types_nothing_on_any_system() {
        // Windows and Linux report `Ctrl+V`, macOS reports `Cmd+V` as control.
        for letter in ["v", "V", "c", "x", "a"] {
            let press = KeyPress {
                control: true,
                ..key(letter)
            };
            let mut query = Query::default();
            assert_eq!(query.press(press), Pressed::NotOurs, "{letter}");
            assert_eq!(query.as_str(), "");
        }
    }

    #[test]
    fn meta_and_a_lone_alt_are_shortcuts_too() {
        let win = KeyPress {
            meta: true,
            ..key("v")
        };
        let alt = KeyPress {
            alt: true,
            ..key("f")
        };
        assert_eq!(typed(&[win, alt]), "");
    }

    #[test]
    fn altgr_types_its_letter_whichever_flags_slint_leaves_on() {
        // Slint clears both flags when it recognises AltGr. When it does not,
        // the press arrives with both held, or - behind a fake left Ctrl -
        // with control alone. All three are the letter.
        let cleared = key("\u{0105}");
        let both = KeyPress {
            control: true,
            alt: true,
            ..key("\u{0105}")
        };
        let control_only = KeyPress {
            control: true,
            ..key("\u{0105}")
        };
        let at_sign = KeyPress {
            control: true,
            alt: true,
            ..key("@")
        };
        assert_eq!(
            typed(&[cleared, both, control_only, at_sign]),
            "\u{0105}\u{0105}\u{0105}@"
        );
    }

    #[test]
    fn option_on_macos_types_its_character() {
        let option_a = KeyPress {
            alt: true,
            ..key("\u{00E5}")
        };
        assert_eq!(typed(&[option_a]), "\u{00E5}");
    }

    #[test]
    fn keys_that_type_nothing_are_not_the_query() {
        // Tab, Enter, Escape, Delete, Shift, and the up arrow as Slint names it.
        for code in ["\t", "\n", "\u{1b}", "\u{7f}", "\u{10}", "\u{F700}", ""] {
            let mut query = Query::default();
            assert_eq!(query.press(key(code)), Pressed::NotOurs, "{code:?}");
        }
    }

    #[test]
    fn backspace_erases_what_a_person_sees_as_one_character() {
        let mut query = Query::default();
        for press in [key("e"), key("\u{0301}"), key("x")] {
            query.press(press);
        }
        assert_eq!(query.press(key(BACKSPACE)), Pressed::Changed);
        assert_eq!(query.as_str(), "e\u{0301}");
        assert_eq!(query.press(key(BACKSPACE)), Pressed::Changed);
        assert_eq!(query.as_str(), "");
    }

    #[test]
    fn backspace_on_an_empty_query_is_consumed_and_changes_nothing() {
        let mut query = Query::default();
        assert_eq!(query.press(key(BACKSPACE)), Pressed::Unchanged);
        let with_ctrl = KeyPress {
            control: true,
            ..key(BACKSPACE)
        };
        assert_eq!(query.press(with_ctrl), Pressed::Unchanged);
    }
}
