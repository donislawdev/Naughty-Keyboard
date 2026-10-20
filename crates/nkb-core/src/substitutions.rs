//! What the automatic text services of a system may do to text that is typed.
//!
//! # Why this exists
//!
//! A value is typed as a person would type it, and on macOS the text system of
//! most applications works on what is typed: the first letter of a sentence is
//! capitalised, two spaces after a word become a period and a space, and in a
//! rich text document a straight quote becomes a curly one. Measured 2026-10-20 on
//! macOS 27.0: `ab` + three zero-width spaces + `cd` arrived in TextEdit as `Ab...`,
//! `Jan` + three spaces + `Kowalski` as `Jan.  Kowalski` (in Chrome as
//! `J.  n Kowalski`, and in Safari the spaces landed two characters late), and
//! `He said "hi"` as `He said` in curly quotes in a rich text document. The value
//! in the field is then not the value the tester chose, and nothing in the field
//! says so.
//!
//! Not every service is listed: two hyphens did not become a dash and three
//! dots did not become an ellipsis in either kind of document, and what was not
//! seen is not claimed.
//!
//! The tool cannot stop it - the services live in the application - and it does
//! not look into the field to see (untouchable rule 17). What it can do is say
//! when a setting that is on could act on the text it is about to type, which
//! needs two facts: which settings are on (a system fact, asked by `nkb-sys`) and
//! which of them COULD act on this text (a function of the text, here).
//!
//! The question is "could", never "did": the same services act in some
//! applications and not in others (Safari left the capital alone, TextEdit in
//! plain text left the quotes alone), and which one a field belongs to is not
//! something the tool asks.

/// A set of the three text services, each on or off - used both for the ones a
/// system has switched on and for the ones a text could trigger.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Substitutions {
    /// The first letter of a sentence is made a capital.
    pub capitalization: bool,
    /// Two spaces after a word become a period and a space.
    pub period: bool,
    /// A straight quote becomes a curly one.
    pub quotes: bool,
}

impl Substitutions {
    /// None of the three.
    pub const NONE: Self = Self {
        capitalization: false,
        period: false,
        quotes: false,
    };

    /// Whether none of the three is set.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        !(self.capitalization || self.period || self.quotes)
    }

    /// The ones set in both - the services that are on AND could act.
    #[must_use]
    pub const fn both(self, other: Self) -> Self {
        Self {
            capitalization: self.capitalization && other.capitalization,
            period: self.period && other.period,
            quotes: self.quotes && other.quotes,
        }
    }
}

/// The marks that open a quotation or a bracket, which the first letter after
/// them still counts as the first of its sentence: measured in TextEdit,
/// `"jan kowalski"@example.com` arrived as `"Jan kowalski"@example.com`.
const OPENS_A_SENTENCE: [char; 11] = [
    '"', '\'', '\u{201C}', '\u{2018}', '(', '[', '{', '\u{BF}', '\u{A1}', '\u{AB}', '\u{2039}',
];

/// Whether `c` is a letter that has a capital different from itself.
fn has_capital(c: char) -> bool {
    c.is_lowercase() && !c.to_uppercase().eq(core::iter::once(c))
}

/// The services that COULD act on `text` if they were on - a function of the
/// text alone.
///
/// - capitalisation: a lowercase letter that opens the text, follows a line
///   break, or follows the end of a sentence (`.`, `!` or `?` and a space). The
///   text does not know what the field already holds, so a text typed in the
///   middle of a sentence is flagged too: "could", not "did".
/// - period: a word, then two spaces.
/// - quotes: a straight `"` or `'`.
#[must_use]
pub fn could_act_on(text: &str) -> Substitutions {
    let mut found = Substitutions::NONE;
    let mut previous: Option<char> = None;
    let mut before_previous: Option<char> = None;
    let mut sentence_start = true;
    for c in text.chars() {
        if sentence_start && has_capital(c) {
            found.capitalization = true;
        }
        if !c.is_whitespace() && !OPENS_A_SENTENCE.contains(&c) {
            sentence_start = false;
        }
        if matches!(c, '\n' | '\r') {
            sentence_start = true;
        }
        if c == ' ' && previous == Some(' ') {
            // A word, then two spaces: a space before them is no word.
            if before_previous.is_some_and(|word| !word.is_whitespace()) {
                found.period = true;
            }
        }
        if c == ' ' && matches!(previous, Some('.' | '!' | '?')) {
            sentence_start = true;
        }
        if c == '"' || c == '\'' {
            found.quotes = true;
        }
        before_previous = previous;
        previous = Some(c);
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn only(capitalization: bool, period: bool, quotes: bool) -> Substitutions {
        Substitutions {
            capitalization,
            period,
            quotes,
        }
    }

    #[test]
    fn a_lowercase_letter_that_opens_the_text_could_be_capitalised() {
        // Measured in TextEdit: `ab...` arrived as `Ab...`.
        assert_eq!(could_act_on("abcd"), only(true, false, false));
        assert_eq!(could_act_on("  abcd"), only(true, false, false));
    }

    #[test]
    fn a_capital_a_digit_or_a_letter_without_a_capital_is_left_alone() {
        for text in ["Abcd", "1abc", "\u{4E2D}\u{6587}", "-abc", ""] {
            assert!(!could_act_on(text).capitalization, "{text:?}");
        }
    }

    #[test]
    fn a_lowercase_letter_after_a_sentence_end_or_a_line_break_could_be_capitalised() {
        assert!(could_act_on("Done. next").capitalization);
        assert!(could_act_on("Done! next").capitalization);
        assert!(could_act_on("Done?  next").capitalization);
        assert!(could_act_on("Done\nnext").capitalization);
        assert!(could_act_on("Done\r\nnext").capitalization);
        // Measured in TextEdit: a quotation mark in front does not hide it.
        assert!(could_act_on("\"jan kowalski\"@example.com").capitalization);
        assert!(could_act_on("(note) done").capitalization);
        // A period inside a word is not the end of a sentence.
        assert!(!could_act_on("Done.next").capitalization);
        assert!(!could_act_on("Done, next").capitalization);
    }

    #[test]
    fn two_spaces_after_a_word_could_become_a_period() {
        // Measured: `Jan` + three spaces + `Kowalski` arrived as `Jan.  Kowalski`.
        assert!(could_act_on("Jan  Kowalski").period);
        assert!(could_act_on("Jan   Kowalski").period);
        assert!(!could_act_on("Jan Kowalski").period);
        // Spaces at the start follow no word.
        assert!(!could_act_on("  Jan").period);
        // Other spaces are not the space bar.
        assert!(!could_act_on("Jan\u{2003}\u{2003}Kowalski").period);
        assert!(!could_act_on("Jan\u{A0}\u{A0}Kowalski").period);
    }

    #[test]
    fn a_straight_quote_could_become_a_curly_one() {
        // Measured in a rich text document: `"hi"` and `it's` came out curly.
        assert!(could_act_on("O'Brien").quotes);
        assert!(could_act_on("Jan \"Janek\" Kowalski").quotes);
        assert!(!could_act_on("O\u{2019}Brien").quotes);
    }

    #[test]
    fn two_hyphens_and_three_dots_are_not_claimed() {
        // Neither was changed in plain or rich text (measured), so neither is listed.
        assert!(could_act_on("A--b... Done").is_empty());
    }

    #[test]
    fn the_services_that_are_on_and_could_act_are_their_intersection() {
        let on = only(true, true, false);
        let text = could_act_on("Jan  kowalski \"x\"");
        assert_eq!(on.both(text), only(false, true, false));
        assert!(on.both(could_act_on("Abc")).is_empty());
        assert!(Substitutions::NONE.is_empty());
        assert!(!only(false, false, true).is_empty());
    }
}
