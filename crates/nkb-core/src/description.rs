//! The sentence rule, and the cheapest honest guess at what language it is in.
//!
//! # Why this is the most important module in the validator
//!
//! `product-spec.md` says a pack enters the catalogue only when a sentence can
//! be written for every value in it saying what that value usually breaks and
//! why - and that a value without such a sentence is a curiosity rather than a
//! test case. Everywhere else that rule is editorial policy, which is another way
//! of saying it depends on somebody's discipline on a particular afternoon. Here
//! it becomes a condition of acceptance.
//!
//! The catalogue is the product. This module is the part of the validator that
//! makes curating it a property of the project rather than a habit.

/// The shortest `breaks` the format accepts, in code points (`pack-format.md` 11).
///
/// Code points rather than bytes or graphemes, for the reason already settled for
/// the size rules in [`crate::value`]: bytes would let `a` and `ą` count
/// differently, and the difference between characters and bytes is the subject of
/// this catalogue rather than a hidden rule inside it.
pub const MIN_BREAKS_CODE_POINTS: usize = 40;

/// What is wrong with a `breaks` that exists but does not do its job.
///
/// Two members rather than one because they send the writer to do different
/// things: one needs more sentence, the other needs a different sentence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreaksFault {
    /// Shorter than the format allows. Carries the length it found, so the
    /// message can say how far off it is instead of only that it is off.
    TooShort { code_points: usize },
    /// Says the name again and nothing else. The name is already on the screen
    /// next to it, so this value arrives with no explanation at all.
    EchoesName,
}

/// Judges one `breaks` against the sentence rule.
///
/// `name` is optional because a value may be missing it - that absence is
/// somebody else's rule, and refusing to judge the description because of it
/// would let two faults hide one another.
#[must_use]
pub fn check_breaks(breaks: &str, name: Option<&str>) -> Option<BreaksFault> {
    let code_points = breaks.chars().count();
    if code_points < MIN_BREAKS_CODE_POINTS {
        // Length first. It is the more concrete repair, and a description short
        // enough to fail this is almost never long enough to fail the other half.
        return Some(BreaksFault::TooShort { code_points });
    }

    if let Some(name) = name
        && normalised(breaks) == normalised(name)
    {
        return Some(BreaksFault::EchoesName);
    }
    None
}

/// Lower case, letters and digits only, single spaces between them.
///
/// Comparing the two raw strings would miss a description that repeats the name
/// with a full stop added, which is exactly the shape this half of the rule is
/// for.
fn normalised(text: &str) -> String {
    let mut out = String::new();
    let mut gap = false;
    for c in text.chars() {
        if c.is_alphanumeric() {
            if gap && !out.is_empty() {
                out.push(' ');
            }
            gap = false;
            out.extend(c.to_lowercase());
        } else {
            gap = true;
        }
    }
    out
}

/// Whether a description carries any sign of being written in English.
///
/// # What this deliberately does not do
///
/// The obvious test - look for letters outside ASCII - would raise a warning on
/// every description that talks about Cyrillic, full width Latin or an Arabic
/// name. Those descriptions are the subject matter of this catalogue, so the
/// obvious test would fire hardest on the best entries in it.
///
/// So the test is the opposite way round: a description of any length that
/// contains **not one** of the function words below is probably not English.
///
/// # What it misses, measured rather than guessed
///
/// Against the 475 English descriptions this project has already written, one
/// raises a false warning - `Rejected outright - no legitimate value contains a
/// zero byte.` Against 1632 Polish sentences, it stays silent on about one in
/// ten, and those are sentences quoting an English phrase.
///
/// It is weakest on German, Dutch and the Scandinavian languages, which share
/// `in`, `is`, `of`, `an`, `over` and `was` with English. It cannot see a
/// machine translation, and it cannot see bad English. It is a warning for that
/// reason: it is evidence, not a verdict.
#[must_use]
pub fn looks_english(text: &str) -> bool {
    let mut word = String::new();
    for c in text.chars().chain(std::iter::once(' ')) {
        if c.is_ascii_alphabetic() {
            word.push(c.to_ascii_lowercase());
        } else {
            if !word.is_empty() && FUNCTION_WORDS.binary_search(&word.as_str()).is_ok() {
                return true;
            }
            word.clear();
        }
    }
    false
}

/// English function words, sorted so that the lookup above is a binary search.
///
/// Chosen for two properties at once: common enough that an English sentence of
/// any length is unlikely to avoid all of them, and not ordinary words in the
/// languages a contributor is most likely to slip into. That second half is why
/// `a`, `i`, `to`, `on`, `do` and `no` are absent - each is an everyday word in
/// Polish, and `no` is a value in this catalogue besides.
const FUNCTION_WORDS: [&str; 71] = [
    "also", "always", "an", "and", "another", "any", "are", "as", "at", "be", "because", "been",
    "before", "but", "by", "can", "could", "did", "does", "each", "even", "every", "for", "from",
    "had", "has", "have", "how", "in", "into", "is", "it", "its", "never", "not", "of", "off",
    "one", "only", "or", "other", "out", "over", "own", "should", "still", "than", "that", "the",
    "their", "them", "then", "there", "these", "they", "this", "those", "two", "up", "was", "were",
    "what", "when", "where", "which", "while", "who", "why", "will", "with", "would",
];

#[cfg(test)]
mod tests {
    use super::*;

    /// A real description from the catalogue, used where the test needs one that
    /// passes so that the failing half is the only thing under test.
    const REAL: &str = "Character counters, comparison and deduplication see a different value than the human does.";

    #[test]
    fn a_real_description_from_the_catalogue_passes_both_halves() {
        assert_eq!(check_breaks(REAL, Some("Three zero-width spaces")), None);
    }

    #[test]
    fn the_length_bound_holds_on_both_sides_of_the_edge() {
        // Off by one here weakens the one rule the catalogue is built on, and
        // does it silently: every description would still look checked.
        let thirty_nine = "x".repeat(39);
        let forty = "x".repeat(40);
        assert_eq!(
            check_breaks(&thirty_nine, None),
            Some(BreaksFault::TooShort { code_points: 39 })
        );
        assert_eq!(check_breaks(&forty, None), None);
    }

    #[test]
    fn an_absent_description_is_measured_as_empty_rather_than_skipped() {
        assert_eq!(
            check_breaks("", Some("Anything")),
            Some(BreaksFault::TooShort { code_points: 0 })
        );
    }

    #[test]
    fn the_length_is_counted_in_code_points_not_bytes() {
        // Forty accented letters are forty characters and eighty bytes. Counting
        // bytes would accept a description that the specification refuses, and
        // would do it only for descriptions not written in English.
        let accented = "ą".repeat(40);
        assert_eq!(accented.len(), 80);
        assert_eq!(check_breaks(&accented, None), None);

        let short = "ą".repeat(39);
        assert_eq!(
            check_breaks(&short, None),
            Some(BreaksFault::TooShort { code_points: 39 })
        );
    }

    #[test]
    fn a_description_that_only_repeats_the_name_is_caught_even_when_long_enough() {
        // The half that length alone cannot catch: a name long enough to clear
        // forty characters, copied into the field that was supposed to explain it.
        let name = "Unicode right to left override character";
        assert_eq!(name.chars().count(), 40);
        assert_eq!(
            check_breaks(name, Some(name)),
            Some(BreaksFault::EchoesName)
        );
    }

    #[test]
    fn punctuation_and_case_do_not_hide_an_echoed_name() {
        let name = "Unicode right-to-left override character";
        let breaks = "  UNICODE RIGHT-TO-LEFT OVERRIDE CHARACTER.  ";
        assert_eq!(
            check_breaks(breaks, Some(name)),
            Some(BreaksFault::EchoesName)
        );
    }

    #[test]
    fn a_name_with_something_said_about_it_is_not_an_echo() {
        let name = "Unicode right-to-left override character";
        let breaks = "Unicode right-to-left override character makes the text render reversed, so the value shown is not the value stored.";
        assert_eq!(check_breaks(breaks, Some(name)), None);
    }

    #[test]
    fn a_missing_name_leaves_the_length_half_working() {
        // Two faults must not hide one another: a value with no name still has a
        // description that can be too short.
        assert_eq!(
            check_breaks("too short", None),
            Some(BreaksFault::TooShort { code_points: 9 })
        );
        assert_eq!(check_breaks(REAL, None), None);
    }

    #[test]
    fn english_descriptions_from_this_repository_are_recognised() {
        for text in [
            REAL,
            // prose-punctuation-exempt: quoted pack prose, which D31 lets use a semicolon
            "Rejected in a numeric field; kept verbatim in a text field.",
            "Stored and returned as the two-character string 'no'.",
            "Nothing on its own - this value only exists so the file is otherwise complete.",
        ] {
            assert!(looks_english(text), "{text}");
        }
    }

    #[test]
    fn a_polish_description_is_reported_as_probably_not_english() {
        assert!(!looks_english(
            "Parsery czytają niecytowane no jako wartość logiczną fałsz."
        ));
    }

    #[test]
    fn a_description_about_another_script_is_not_mistaken_for_another_language() {
        // The failure this rule was designed around. These are the best entries
        // in the catalogue, and the obvious implementation warns on all of them.
        for text in [
            "Full width letters read as Latin to a human and as different characters to a machine.",
            "Right to left text inside a left to right sentence reorders the line: 名前テスト مرحبا.",
            "A byte order mark in the middle of a value survives every copy: Jan\u{FEFF}Kowalski.",
        ] {
            assert!(looks_english(text), "{text}");
        }
    }

    #[test]
    fn the_word_list_is_sorted_because_the_lookup_assumes_it() {
        // A binary search over an unsorted list fails silently and at random,
        // which would turn this warning into noise nobody could reproduce.
        let mut sorted = FUNCTION_WORDS;
        sorted.sort_unstable();
        assert_eq!(sorted, FUNCTION_WORDS);
    }

    #[test]
    fn a_word_is_matched_whole_and_not_inside_another_one() {
        // `it` inside `Kowalski` would make every Polish description look English.
        assert!(!looks_english("Kowalski Andrzejewski Wiśniewski Zieliński"));
        assert!(looks_english("it"));
    }

    #[test]
    fn an_empty_description_is_not_english_and_does_not_stumble() {
        assert!(!looks_english(""));
    }
}
