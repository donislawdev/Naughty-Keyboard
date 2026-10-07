//! The box the welcome window offers for the first try (`ux-spec.md` 5.1, UX7,
//! `UX-GUI-011`): the first value goes in here, before it goes anywhere else.
//!
//! # Why a box of our own
//!
//! Without it the first press of a tester who has just installed the tool lands
//! in somebody else's application - the very thing a careful tester will not do
//! before seeing what the tool sends. The box takes the palette's value the way
//! any field does: the same shortcut, the same route, the same clearing recipe.
//!
//! # Why not a text field (`D85`)
//!
//! A Slint text field copies and pastes on its own, and untouchable rule 17
//! names the three places the clipboard is used. So the box is the query line's
//! twin: a focus scope hands each key press here, this type keeps the text, and
//! the view draws it as plain text with the invisible characters shown as the
//! palette shows them. Nothing to copy from, nothing to paste into.
//!
//! # What a press does
//!
//! The palette clears a line with `Home`, `Shift+End`, `Delete` (`D54`), so the
//! box understands exactly those - a caret, a selection made with `Shift`, and
//! deleting it - plus `Backspace` for a tester typing by hand. Every other
//! character is the value's: a tab, a line break and an escape included, because
//! values carry them and a box that dropped them would show a value the tool did
//! not send. What it cannot tell apart, said rather than discovered: a value
//! holding `U+0008` or `U+007F` arrives exactly as the `Backspace` and `Delete`
//! keys do, so the box treats it as those keys, and one holding `U+0010` to
//! `U+0019` arrives as a modifier key - Slint reports `Shift`, `Ctrl`, `Alt` and
//! their kin as those code points when they go down (found by the window test:
//! the recipe's `Shift` landed in the box as `U+0010`) - so the box drops it.
//! None of them is in the shipped catalogue. The arrows are not understood -
//! nothing the palette sends uses them.

use nkb_core::graphemes::last_cluster_start;

use crate::query::Pressed;

/// One key press, as the window reports it. The toolkit's own names for the
/// modifiers, as in [`crate::query::KeyPress`], plus `Shift`, which the
/// clearing recipe holds over `End`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrialKey<'a> {
    pub text: &'a str,
    pub control: bool,
    pub alt: bool,
    pub shift: bool,
    pub meta: bool,
}

/// The toolkit's codes for the keys the box understands - read in
/// `i-slint-common` 1.18.1, `key_codes.rs`.
const BACKSPACE: &str = "\u{8}";
const DELETE: &str = "\u{7f}";
const HOME: &str = "\u{F729}";
const END: &str = "\u{F72B}";

/// The text of the box, a caret in it and, with `Shift`, a selection.
///
/// Positions are byte offsets that always fall on a character boundary.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Trial {
    text: String,
    caret: usize,
    /// Where a selection began - `None` when nothing is selected.
    anchor: Option<usize>,
}

impl Trial {
    /// The whole text.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The text before the caret - drawn before the caret's mark.
    #[must_use]
    pub fn before(&self) -> &str {
        &self.text[..self.caret]
    }

    /// The text after the caret.
    #[must_use]
    pub fn after(&self) -> &str {
        &self.text[self.caret..]
    }

    /// Applies one key press.
    pub fn press(&mut self, key: TrialKey<'_>) -> Pressed {
        if is_shortcut(key) {
            return Pressed::NotOurs;
        }
        match key.text {
            HOME => self.move_to(0, key.shift),
            END => self.move_to(self.text.len(), key.shift),
            BACKSPACE => self.delete_or(|trial| {
                let start = last_cluster_start(trial.before());
                (start, trial.caret)
            }),
            DELETE => self.delete_or(|trial| {
                let end = trial
                    .after()
                    .chars()
                    .next()
                    .map_or(trial.caret, |next| trial.caret + next.len_utf8());
                (trial.caret, end)
            }),
            text if is_value_text(text) => {
                self.remove_selection();
                self.text.insert_str(self.caret, text);
                self.caret += text.len();
                Pressed::Changed
            }
            _ => Pressed::NotOurs,
        }
    }

    /// Moves the caret, selecting from where it stood when `extend`.
    fn move_to(&mut self, to: usize, extend: bool) -> Pressed {
        if extend {
            self.anchor.get_or_insert(self.caret);
        } else {
            self.anchor = None;
        }
        self.caret = to;
        Pressed::Changed
    }

    /// Deletes the selection, or - without one - the range `range` names.
    fn delete_or(&mut self, range: impl Fn(&Self) -> (usize, usize)) -> Pressed {
        if self.remove_selection() {
            return Pressed::Changed;
        }
        let (start, end) = range(self);
        if start == end {
            return Pressed::Unchanged;
        }
        self.text.replace_range(start..end, "");
        self.caret = start;
        Pressed::Changed
    }

    /// Removes the selected text, if any, and puts the caret where it began.
    fn remove_selection(&mut self) -> bool {
        let Some(anchor) = self.anchor.take() else {
            return false;
        };
        let (start, end) = (anchor.min(self.caret), anchor.max(self.caret));
        if start == end {
            return false;
        }
        self.text.replace_range(start..end, "");
        self.caret = start;
        true
    }
}

/// A press that means a command rather than a character - the query line's
/// rule (`crate::query`), so `Ctrl+V` does not type a "v" and nothing pastes.
fn is_shortcut(key: TrialKey<'_>) -> bool {
    key.meta || (key.control != key.alt && key.text.is_ascii())
}

/// Text that belongs to a value: anything but the toolkit's codes for keys
/// that type nothing - the modifiers (`U+0010` to `U+0019`, `i-slint-common`
/// 1.18.1 `key_codes.rs`) and the private use area - and the two editing keys.
fn is_value_text(text: &str) -> bool {
    !text.is_empty()
        && text != BACKSPACE
        && text != DELETE
        && !text.chars().any(|c| {
            matches!(
                c,
                '\u{10}'..='\u{19}'
                    | '\u{E000}'..='\u{F8FF}'
                    | '\u{F0000}'..='\u{FFFFD}'
                    | '\u{100000}'..='\u{10FFFD}'
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(text: &str) -> TrialKey<'_> {
        TrialKey {
            text,
            control: false,
            alt: false,
            shift: false,
            meta: false,
        }
    }

    fn shifted(text: &str) -> TrialKey<'_> {
        TrialKey {
            shift: true,
            ..key(text)
        }
    }

    fn typed(trial: &mut Trial, text: &str) {
        for c in text.chars() {
            let mut buffer = [0u8; 4];
            assert_eq!(
                trial.press(key(c.encode_utf8(&mut buffer))),
                Pressed::Changed
            );
        }
    }

    #[test]
    fn the_clearing_recipe_empties_the_line_and_the_next_value_stands_alone() {
        let mut trial = Trial::default();
        typed(&mut trial, "Kowalski ");
        // Home, Shift+End, Delete - what the palette presses before a value.
        assert_eq!(trial.press(key(HOME)), Pressed::Changed);
        assert_eq!(trial.press(shifted(END)), Pressed::Changed);
        assert_eq!(trial.press(key(DELETE)), Pressed::Changed);
        assert_eq!(trial.text(), "");
        typed(&mut trial, " Kowalski");
        assert_eq!(trial.text(), " Kowalski");
        assert_eq!((trial.before(), trial.after()), (" Kowalski", ""));
    }

    #[test]
    fn without_the_recipe_the_value_goes_in_at_the_caret() {
        // `Clearing::Keep` (`D101`): only the value's characters arrive.
        let mut trial = Trial::default();
        typed(&mut trial, "old");
        let _ = trial.press(key(HOME));
        typed(&mut trial, "new ");
        assert_eq!(trial.text(), "new old");
        assert_eq!((trial.before(), trial.after()), ("new ", "old"));
    }

    #[test]
    fn every_character_of_a_value_stays_tab_line_break_and_escape_included() {
        let mut trial = Trial::default();
        typed(&mut trial, "a\tb\nc\u{1b}d\u{200B}e");
        assert_eq!(trial.text(), "a\tb\nc\u{1b}d\u{200B}e");
    }

    #[test]
    fn backspace_takes_what_a_person_sees_and_delete_takes_one_character() {
        let mut trial = Trial::default();
        typed(&mut trial, "ae\u{301}");
        assert_eq!(trial.press(key(BACKSPACE)), Pressed::Changed);
        assert_eq!(trial.text(), "a", "the letter and its accent went together");
        let _ = trial.press(key(HOME));
        assert_eq!(trial.press(key(DELETE)), Pressed::Changed);
        assert_eq!(trial.text(), "");
        assert_eq!(trial.press(key(DELETE)), Pressed::Unchanged);
        assert_eq!(trial.press(key(BACKSPACE)), Pressed::Unchanged);
    }

    #[test]
    fn a_selection_is_replaced_by_what_is_typed_over_it() {
        let mut trial = Trial::default();
        typed(&mut trial, "abc");
        let _ = trial.press(key(HOME));
        let _ = trial.press(shifted(END));
        typed(&mut trial, "x");
        assert_eq!(trial.text(), "x");
        // A move without Shift drops the selection, so Delete removes nothing.
        typed(&mut trial, "yz");
        let _ = trial.press(shifted(HOME));
        let _ = trial.press(key(END));
        assert_eq!(trial.press(key(DELETE)), Pressed::Unchanged);
        assert_eq!(trial.text(), "xyz");
    }

    #[test]
    fn a_shortcut_or_a_key_that_types_nothing_is_not_the_box_s() {
        let mut trial = Trial::default();
        let paste = TrialKey {
            control: true,
            ..key("v")
        };
        assert_eq!(trial.press(paste), Pressed::NotOurs);
        assert_eq!(trial.press(key("\u{F702}")), Pressed::NotOurs, "an arrow");
        // A modifier on its own, as Slint reports the key going down.
        assert_eq!(trial.press(key("\u{10}")), Pressed::NotOurs, "Shift alone");
        assert_eq!(trial.press(key("\u{12}")), Pressed::NotOurs, "Alt alone");
        assert_eq!(trial.press(key("")), Pressed::NotOurs);
        // AltGr is Ctrl and Alt together: a character, not a shortcut.
        let altgr = TrialKey {
            control: true,
            alt: true,
            ..key("\u{105}")
        };
        assert_eq!(trial.press(altgr), Pressed::Changed);
        assert_eq!(trial.text(), "\u{105}");
    }
}
