//! The prose of a pack: the text a person reads about the values, never the
//! values themselves (E035 and E036, `D174`).
//!
//! # Why prose has rules of its own
//!
//! A value may hold any character, and that is its point. The text around it -
//! a name, a description, what a value breaks and what should happen - is read
//! by a tester in a window, by a developer in a terminal and by a reviewer in a
//! pull request. Since packs come from folders (`D173`), that text may come from
//! a stranger, and before these rules `nkb lint` let a line break, a direction
//! override, a terminal escape sequence and a description of megabytes through.
//!
//! An escape written in the file does not help: TOML reads it back as the same
//! character. So the rules judge the text after parsing, not the bytes of the
//! file, and the only way to pass them is to write prose with visible characters.

use crate::text::is_unseen;

/// The most code points a name may hold - of a pack, a value or a pair.
///
/// About five times the longest name in the catalogue the tool carries (55, a
/// value of `passwords`). A window shows a name up to this length whole, so a
/// pack that passes is never cut there.
pub const NAME_LIMIT: usize = 120;

/// The most code points a pack's description may hold.
///
/// About two and a half times the longest in the catalogue (114). Measured on a
/// live window on 2026-11-09: at 1000 the row of one pack filled the whole list.
pub const DESCRIPTION_LIMIT: usize = 300;

/// The most code points `breaks` and `expect` may hold.
///
/// Three times the longest in the catalogue (329, a pair in `search-queries`).
/// These two carry the explanation the catalogue is made of, so they get the
/// most room - and still a limit, because prose this long goes unread.
pub const EXPLANATION_LIMIT: usize = 1000;

/// The most code points any other prose may hold: an author, a tag, a licence,
/// a source, a version.
///
/// The longest of these in the catalogue is a tag of twenty. A source citing a
/// page with its address fits with room to spare.
pub const OTHER_TEXT_LIMIT: usize = 300;

/// Every field of a pack file that holds prose, with its limit.
///
/// 🔴 The data is absent on purpose - `value` and `unit` may hold anything, and
/// that is what they are for. So are the fields a rule of their own already
/// judges by its pattern or vocabulary (`id`, `a`, `b`, `replaced_by`,
/// `translates`, `fields`, `risk`, `type`, `relation`, `week`, `weekday`,
/// `time`): a direction mark in an identifier is that rule's finding, and one
/// fault gets one code.
const PROSE_FIELDS: [(&str, usize); 12] = [
    ("name", NAME_LIMIT),
    ("description", DESCRIPTION_LIMIT),
    ("breaks", EXPLANATION_LIMIT),
    ("expect", EXPLANATION_LIMIT),
    ("authors", OTHER_TEXT_LIMIT),
    ("tags", OTHER_TEXT_LIMIT),
    ("license", OTHER_TEXT_LIMIT),
    ("source", OTHER_TEXT_LIMIT),
    ("version", OTHER_TEXT_LIMIT),
    ("since", OTHER_TEXT_LIMIT),
    ("language", OTHER_TEXT_LIMIT),
    ("shape", OTHER_TEXT_LIMIT),
];

/// How many different unseen characters one finding names.
///
/// A field pasted from a word processor holds the same no-break space many
/// times, and naming it once is the whole repair. A field built to attack holds
/// many different ones, and naming them all would hand the terminal the flood
/// the rule exists to stop.
pub const MOST_NAMED: usize = 4;

/// The most code points `field` may hold, or nothing for a field that is not
/// prose.
#[must_use]
pub fn limit_of(field: &str) -> Option<usize> {
    PROSE_FIELDS
        .iter()
        .find(|(name, _)| *name == field)
        .map(|(_, limit)| *limit)
}

/// What is wrong with one piece of prose. Both may apply to the same text, and
/// both are said: they send the writer to do different things.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProseFault {
    /// E035. The characters a reader would not see or a terminal would act on,
    /// each named once in the order they first appear, at most [`MOST_NAMED`].
    /// `more` says that the text holds others beyond those named.
    Unseen { named: Vec<char>, more: bool },
    /// E036. Longer than the field's limit, in code points.
    TooLong { code_points: usize, limit: usize },
}

/// Judges one piece of prose against both rules.
///
/// `limit` comes from [`limit_of`]. Code points, like every length the format
/// states (`E026`, `E031`): bytes would let `a` and `ą` count differently.
#[must_use]
pub fn check(text: &str, limit: usize) -> Vec<ProseFault> {
    let mut faults = Vec::new();

    let mut named: Vec<char> = Vec::new();
    let mut more = false;
    for character in text.chars().filter(|c| is_unseen(*c)) {
        if named.contains(&character) {
            continue;
        }
        if named.len() == MOST_NAMED {
            more = true;
            break;
        }
        named.push(character);
    }
    if !named.is_empty() {
        faults.push(ProseFault::Unseen { named, more });
    }

    let code_points = text.chars().count();
    if code_points > limit {
        faults.push(ProseFault::TooLong { code_points, limit });
    }
    faults
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prose_of_visible_characters_within_its_limit_passes() {
        assert!(check("Zażółć gęślą jaźń - a name with 🔑 in it", NAME_LIMIT).is_empty());
        // The ordinary space is prose, wherever it stands.
        assert!(check(" leading and trailing ", NAME_LIMIT).is_empty());
    }

    #[test]
    fn every_kind_of_unseen_character_is_named() {
        for character in [
            '\n', '\t', '\r', '\u{1B}', '\u{0}', '\u{85}', '\u{202E}', '\u{2067}', '\u{200B}',
            '\u{200D}', '\u{FE0F}', '\u{AD}', '\u{A0}', '\u{3000}', '\u{2028}', '\u{FFFC}',
        ] {
            let text = format!("before{character}after");
            assert_eq!(
                check(&text, NAME_LIMIT),
                vec![ProseFault::Unseen {
                    named: vec![character],
                    more: false
                }],
                "U+{:04X}",
                character as u32
            );
        }
    }

    #[test]
    fn a_character_is_named_once_and_at_most_four_are_named() {
        let pasted = "one\u{A0}two\u{A0}three\u{A0}four";
        assert_eq!(
            check(pasted, NAME_LIMIT),
            vec![ProseFault::Unseen {
                named: vec!['\u{A0}'],
                more: false
            }]
        );

        let built = "\u{1B}[31m\u{202E}\n\t\u{200B}";
        assert_eq!(
            check(built, NAME_LIMIT),
            vec![ProseFault::Unseen {
                named: vec!['\u{1B}', '\u{202E}', '\n', '\t'],
                more: true
            }]
        );

        let exactly_four = "\u{1B}\u{202E}\n\t";
        assert_eq!(
            check(exactly_four, NAME_LIMIT),
            vec![ProseFault::Unseen {
                named: vec!['\u{1B}', '\u{202E}', '\n', '\t'],
                more: false
            }]
        );
    }

    #[test]
    fn the_limit_is_counted_in_code_points_and_is_itself_allowed() {
        let at = "ą".repeat(NAME_LIMIT);
        assert!(check(&at, NAME_LIMIT).is_empty(), "the limit itself passes");

        let over = "ą".repeat(NAME_LIMIT + 1);
        assert_eq!(
            check(&over, NAME_LIMIT),
            vec![ProseFault::TooLong {
                code_points: NAME_LIMIT + 1,
                limit: NAME_LIMIT
            }]
        );
    }

    #[test]
    fn both_faults_are_said_when_both_apply() {
        let text = format!("\u{1B}[31m{}", "a".repeat(DESCRIPTION_LIMIT));
        assert_eq!(
            check(&text, DESCRIPTION_LIMIT),
            vec![
                ProseFault::Unseen {
                    named: vec!['\u{1B}'],
                    more: false
                },
                ProseFault::TooLong {
                    code_points: DESCRIPTION_LIMIT + 5,
                    limit: DESCRIPTION_LIMIT
                }
            ]
        );
    }

    #[test]
    fn the_prose_fields_are_named_and_the_data_and_identifiers_are_not() {
        assert_eq!(limit_of("name"), Some(NAME_LIMIT));
        assert_eq!(limit_of("description"), Some(DESCRIPTION_LIMIT));
        assert_eq!(limit_of("breaks"), Some(EXPLANATION_LIMIT));
        assert_eq!(limit_of("expect"), Some(EXPLANATION_LIMIT));
        for field in [
            "authors", "tags", "license", "source", "version", "since", "language", "shape",
        ] {
            assert_eq!(limit_of(field), Some(OTHER_TEXT_LIMIT), "{field}");
        }
        for field in [
            "value",
            "unit",
            "id",
            "a",
            "b",
            "replaced_by",
            "translates",
            "fields",
            "risk",
            "type",
            "relation",
            "week",
            "weekday",
            "time",
            "updated",
            "count",
            "month",
            "deprecated",
        ] {
            assert_eq!(limit_of(field), None, "{field}");
        }
    }
}
