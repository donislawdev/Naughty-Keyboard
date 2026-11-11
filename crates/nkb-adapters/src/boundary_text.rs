//! The words of the values made for a limit (`D178`): what the set is called,
//! what each value is called, what it breaks and what to expect of the field.
//!
//! # English, always - as a pack's prose is
//!
//! The palette holds the values made for a limit as it holds a pack, and a
//! pack's prose is English whatever language the palette speaks (`D23`,
//! untouchable rule 8): the name of a value goes into a ticket, and a team's
//! tracker is one language. So these are keys with exactly one value each,
//! like the report block's (`report_text.rs`), and `nkb boundary` prints the
//! same names from the same function - the command and the palette cannot
//! call one value two things.

use nkb_app::ports::BoundaryText;
use nkb_core::boundary::{BoundaryValue, Fill, Limit, Recipe, Unit};

use crate::i18n::fill;

/// The words of a set made for a limit, in English.
#[derive(Debug, Clone, Copy, Default)]
pub struct EnglishBoundary;

impl BoundaryText for EnglishBoundary {
    fn set_name(&self, limit: Limit) -> String {
        set_name(limit)
    }

    fn value_name(&self, value: &BoundaryValue) -> String {
        value_name(value)
    }

    fn breaks(&self, value: &BoundaryValue) -> String {
        pattern_boundary_phrase(match value.fill() {
            None => BoundaryPhrase::BreaksEmpty,
            Some(Fill::Letters) => BoundaryPhrase::BreaksLetters,
            Some(Fill::Counterstring) => BoundaryPhrase::BreaksCounterstring,
            Some(Fill::TwoByte | Fill::ThreeByte) => BoundaryPhrase::BreaksBytes,
            Some(Fill::FourByte) => BoundaryPhrase::BreaksFourByte,
            Some(Fill::Cluster) => BoundaryPhrase::BreaksCluster,
        })
        .to_owned()
    }

    fn expected(&self, limit: Limit, value: &BoundaryValue) -> String {
        let phrase = match (value.within(), value.length()) {
            (true, 0) => BoundaryPhrase::ExpectEmptyAccepted,
            (true, _) => BoundaryPhrase::ExpectAccepted,
            (false, 0) => BoundaryPhrase::ExpectEmptyUnder,
            (false, length) if limit.max().is_some_and(|most| length > most) => {
                BoundaryPhrase::ExpectOver
            }
            (false, _) => BoundaryPhrase::ExpectUnder,
        };
        fill(
            pattern_boundary_phrase(phrase),
            &[
                ("length", &value.length().to_string()),
                ("bounds", &bounds(limit)),
            ],
        )
    }
}

/// One sentence or name this module writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoundaryPhrase {
    /// The name of the value with no characters.
    NameEmpty,
    NameCounterstring,
    /// A character repeated: how many, and which.
    NameRepeated,
    /// The same with letters `a` in front.
    NamePadded,
    /// What the palette calls the set, where it would name a pack.
    SetName,
    BoundsAtMost,
    BoundsAtLeast,
    BoundsBetween,
    /// The command that makes the same values, for a report block.
    Command,
    CommandMin,
    CommandMax,
    ExpectAccepted,
    /// Nothing typed, and no least to be under.
    ExpectEmptyAccepted,
    ExpectOver,
    ExpectUnder,
    /// Nothing typed, under a least.
    ExpectEmptyUnder,
    BreaksEmpty,
    BreaksLetters,
    BreaksCounterstring,
    /// The fills of two and of three bytes: one code point, more bytes.
    BreaksBytes,
    BreaksFourByte,
    BreaksCluster,
}

fn pattern_boundary_phrase(phrase: BoundaryPhrase) -> &'static str {
    match phrase {
        BoundaryPhrase::NameEmpty => "Empty value",
        BoundaryPhrase::NameCounterstring => "Counterstring of {length}",
        BoundaryPhrase::NameRepeated => "{count} x {character}",
        BoundaryPhrase::NamePadded => "{padding} x a + {repeated}",
        BoundaryPhrase::SetName => "Length limit: {bounds}",
        BoundaryPhrase::BoundsAtMost => "at most {max} {unit}",
        BoundaryPhrase::BoundsAtLeast => "at least {min} {unit}",
        BoundaryPhrase::BoundsBetween => "{min} to {max} {unit}",
        BoundaryPhrase::Command => "nkb boundary{bounds} --unit {unit}",
        BoundaryPhrase::CommandMin => " --min-length {min}",
        BoundaryPhrase::CommandMax => " --max-length {max}",
        BoundaryPhrase::ExpectAccepted => {
            "Accepted: a length of {length} is inside the limit of {bounds}."
        }
        BoundaryPhrase::ExpectEmptyAccepted => {
            "Accepted, unless the field must be filled in: the limit of {bounds} sets no least."
        }
        BoundaryPhrase::ExpectOver => {
            "Refused, or cut to the limit: a length of {length} is over the limit of {bounds}."
        }
        BoundaryPhrase::ExpectUnder => {
            "Refused: a length of {length} is under the limit of {bounds}."
        }
        BoundaryPhrase::ExpectEmptyUnder => "Refused: nothing is under the limit of {bounds}.",
        BoundaryPhrase::BreaksEmpty => {
            "A field that must be filled in, and code that reads the first character of what it was given."
        }
        BoundaryPhrase::BreaksLetters => {
            "A limit that is one off: the check lets one character too many through, or stops one short."
        }
        BoundaryPhrase::BreaksCounterstring => {
            "A field that cuts what it is given instead of refusing it: the last number left says where it cut."
        }
        BoundaryPhrase::BreaksBytes => {
            "A limit counted in bytes where the specification counts characters: the field takes fewer of these than it says."
        }
        BoundaryPhrase::BreaksFourByte => {
            "A limit counted in UTF-16 units or in bytes: this character is two units and four bytes, and a cut can go through the middle of it."
        }
        BoundaryPhrase::BreaksCluster => {
            "A limit counted in code points, UTF-16 units or bytes: one character to the eye is five code points, eight units and eighteen bytes."
        }
    }
}

/// The unit of a limit as a noun after a number. English inflects it, and a
/// text that is English by decision may (`report_text.rs`).
fn pattern_unit_noun(unit: Unit, one: bool) -> &'static str {
    match unit {
        Unit::Graphemes if one => "grapheme",
        Unit::Graphemes => "graphemes",
        Unit::CodePoints if one => "code point",
        Unit::CodePoints => "code points",
        Unit::Utf16 if one => "UTF-16 unit",
        Unit::Utf16 => "UTF-16 units",
        Unit::Bytes if one => "byte",
        Unit::Bytes => "bytes",
    }
}

/// What a person calls the value: how it is made, in plain ASCII. A character
/// outside ASCII is named by its code points, because a name is read in a
/// terminal and in a bug report, and one of these characters is joined by
/// characters nobody can see.
#[must_use]
pub fn value_name(value: &BoundaryValue) -> String {
    match value.recipe() {
        Recipe::Empty => pattern_boundary_phrase(BoundaryPhrase::NameEmpty).to_owned(),
        Recipe::Counterstring { length } => fill(
            pattern_boundary_phrase(BoundaryPhrase::NameCounterstring),
            &[("length", &length.to_string())],
        ),
        Recipe::Repeated {
            padding,
            character,
            count,
        } => {
            let repeated = fill(
                pattern_boundary_phrase(BoundaryPhrase::NameRepeated),
                &[
                    ("count", &count.to_string()),
                    ("character", &spelled(character)),
                ],
            );
            if padding == 0 {
                return repeated;
            }
            // The padding is always the letter a (`nkb_core::boundary`).
            fill(
                pattern_boundary_phrase(BoundaryPhrase::NamePadded),
                &[("padding", &padding.to_string()), ("repeated", &repeated)],
            )
        }
    }
}

fn spelled(character: &str) -> String {
    if character.is_ascii() {
        return character.to_owned();
    }
    let points: Vec<String> = character
        .chars()
        .map(|point| format!("U+{:04X}", u32::from(point)))
        .collect();
    points.join(" ")
}

/// What the palette calls the set: `Length limit: at most 50 bytes`.
#[must_use]
pub fn set_name(limit: Limit) -> String {
    fill(
        pattern_boundary_phrase(BoundaryPhrase::SetName),
        &[("bounds", &bounds(limit))],
    )
}

/// The limit in words: `at most 50 bytes`, `at least 8 graphemes`,
/// `8 to 50 code points`. The noun follows the last number.
fn bounds(limit: Limit) -> String {
    let (phrase, last) = match (limit.min(), limit.max()) {
        (Some(_), Some(most)) => (BoundaryPhrase::BoundsBetween, most),
        (None, Some(most)) => (BoundaryPhrase::BoundsAtMost, most),
        (Some(least), None) => (BoundaryPhrase::BoundsAtLeast, least),
        // A limit has a bound (`Limit::new`). Said as no number rather than
        // reached into.
        (None, None) => (BoundaryPhrase::BoundsAtMost, 0),
    };
    let number = |bound: Option<u32>| bound.map_or_else(String::new, |bound| bound.to_string());
    fill(
        pattern_boundary_phrase(phrase),
        &[
            ("min", &number(limit.min())),
            ("max", &number(limit.max())),
            ("unit", pattern_unit_noun(limit.unit(), last == 1)),
        ],
    )
}

/// The command that makes the values of `limit`, every option written out:
/// `nkb boundary --max-length 50 --unit bytes`. The words of the options and
/// of the unit are the command's own (`ux-spec.md` 10).
#[must_use]
pub fn command(limit: Limit) -> String {
    let option = |phrase, name, bound: Option<u32>| {
        bound.map_or_else(String::new, |bound| {
            fill(
                pattern_boundary_phrase(phrase),
                &[(name, &bound.to_string())],
            )
        })
    };
    let bounds = option(BoundaryPhrase::CommandMin, "min", limit.min())
        + &option(BoundaryPhrase::CommandMax, "max", limit.max());
    fill(
        pattern_boundary_phrase(BoundaryPhrase::Command),
        &[("bounds", &bounds), ("unit", limit.unit().id())],
    )
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;
    use nkb_core::boundary::values;

    fn limit(unit: Unit, min: Option<u32>, max: Option<u32>) -> Limit {
        Limit::new(unit, min, max).expect("a limit in range")
    }

    fn named(limit: Limit, id: &str) -> BoundaryValue {
        values(limit)
            .into_iter()
            .find(|value| value.id() == id)
            .unwrap_or_else(|| panic!("no value called {id}"))
    }

    #[test]
    fn a_name_is_the_recipe_in_plain_ascii() {
        let bytes = limit(Unit::Bytes, None, Some(50));
        let names: Vec<String> = ["empty", "letters-49", "counterstring-50", "two-byte-50"]
            .iter()
            .map(|id| value_name(&named(bytes, id)))
            .collect();
        assert_eq!(
            names,
            [
                "Empty value",
                "49 x a",
                "Counterstring of 50",
                "25 x U+0142"
            ]
        );
        // One letter in front, so the limit falls inside the last character.
        assert_eq!(
            value_name(&named(bytes, "two-byte-51")),
            "1 x a + 25 x U+0142"
        );
        // The family, whose joiners nobody can see, by its code points.
        assert_eq!(
            value_name(&named(bytes, "cluster-50")),
            "14 x a + 2 x U+1F468 U+200D U+1F469 U+200D U+1F467"
        );
        for value in values(bytes) {
            assert!(value_name(&value).is_ascii(), "{}", value.id());
        }
    }

    #[test]
    fn what_a_name_says_about_the_padding_is_what_the_text_starts_with() {
        let bytes = values(limit(Unit::Bytes, None, Some(50)));
        let names: Vec<String> = bytes.iter().map(value_name).collect();
        for expected in [
            "Counterstring of 51",
            "1 x a + 24 x U+0142",
            "2 x a + 16 x U+20AC",
            "3 x a + 12 x U+1F600",
            "15 x a + 2 x U+1F468 U+200D U+1F469 U+200D U+1F467",
        ] {
            assert!(names.iter().any(|name| name == expected), "{expected}");
        }
        for (value, name) in bytes.iter().zip(&names) {
            if let Recipe::Repeated { padding, .. } = value.recipe() {
                let text = value.materialise();
                let letters = text.chars().take_while(|&c| c == 'a').count();
                assert!(letters >= padding as usize, "{name}");
            }
        }
    }

    #[test]
    fn the_set_is_named_by_its_limit_and_the_noun_follows_the_number() {
        let name = |unit, min, max| set_name(limit(unit, min, max));
        assert_eq!(
            name(Unit::Bytes, None, Some(50)),
            "Length limit: at most 50 bytes"
        );
        assert_eq!(
            name(Unit::Graphemes, Some(8), None),
            "Length limit: at least 8 graphemes"
        );
        assert_eq!(
            name(Unit::CodePoints, Some(8), Some(50)),
            "Length limit: 8 to 50 code points"
        );
        assert_eq!(
            name(Unit::Utf16, None, Some(1)),
            "Length limit: at most 1 UTF-16 unit"
        );
        assert_eq!(
            name(Unit::Graphemes, Some(1), None),
            "Length limit: at least 1 grapheme"
        );
    }

    #[test]
    fn the_command_writes_every_option_in_the_words_of_the_command() {
        assert_eq!(
            command(limit(Unit::Graphemes, None, Some(50))),
            "nkb boundary --max-length 50 --unit graphemes"
        );
        assert_eq!(
            command(limit(Unit::CodePoints, Some(8), None)),
            "nkb boundary --min-length 8 --unit code-points"
        );
        assert_eq!(
            command(limit(Unit::Utf16, Some(8), Some(50))),
            "nkb boundary --min-length 8 --max-length 50 --unit utf16"
        );
    }

    #[test]
    fn what_to_expect_follows_which_side_of_the_limit_the_length_is_on() {
        let both = limit(Unit::Graphemes, Some(8), Some(50));
        let expected = |id: &str| EnglishBoundary.expected(both, &named(both, id));
        assert_eq!(
            expected("empty"),
            "Refused: nothing is under the limit of 8 to 50 graphemes."
        );
        assert_eq!(
            expected("letters-7"),
            "Refused: a length of 7 is under the limit of 8 to 50 graphemes."
        );
        assert_eq!(
            expected("letters-8"),
            "Accepted: a length of 8 is inside the limit of 8 to 50 graphemes."
        );
        assert_eq!(
            expected("letters-50"),
            "Accepted: a length of 50 is inside the limit of 8 to 50 graphemes."
        );
        assert_eq!(
            expected("letters-51"),
            "Refused, or cut to the limit: a length of 51 is over the limit of 8 to 50 graphemes."
        );

        let most = limit(Unit::Bytes, None, Some(50));
        assert_eq!(
            EnglishBoundary.expected(most, &named(most, "empty")),
            "Accepted, unless the field must be filled in: the limit of at most 50 bytes sets no least."
        );
    }

    #[test]
    fn every_value_says_what_it_breaks_and_what_to_expect_in_whole_sentences() {
        for unit in Unit::ALL {
            let limit = limit(unit, Some(3), Some(20));
            for value in values(limit) {
                for sentence in [
                    EnglishBoundary.breaks(&value),
                    EnglishBoundary.expected(limit, &value),
                ] {
                    assert!(sentence.ends_with('.'), "{sentence}");
                    assert!(!sentence.contains('{'), "an unfilled place: {sentence}");
                }
            }
        }
    }
}
