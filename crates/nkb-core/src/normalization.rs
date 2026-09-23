//! Whether normalizing a text could change it.
//!
//! # Who asks
//!
//! The report block (`report.rs`). Its `Typed:` line keeps visible characters
//! as themselves, and a tracker that normalizes what it stores to NFC or NFKC
//! rewrites some of them silently - `e` followed by a combining acute becomes
//! one precomposed letter, a fullwidth letter becomes a plain one. For such a
//! value the block spells the value out in code points as well (`OBS-123`,
//! `D70`), and this module decides which values those are.
//!
//! # The quick check from UAX #15, not a normalizer
//!
//! Normalizing a text needs decomposition and composition tables - a whole
//! normalizer, several times the data this crate carries. Knowing that
//! normalization CANNOT change a text needs far less. UAX #15 section 9 defines
//! a quick check that answers from one property per character: a text whose
//! every character answers `Yes`, and whose combining marks stand in canonical
//! order, is already normalized.
//!
//! So [`may_change`] says `false` only when that is certain, and `true` in
//! every other case - including a `Maybe` character with nothing to compose
//! with, like a combining acute after `x`. The error is one-sided on purpose:
//! it may add a line a normalizer would have spared, never drop one it needed.
//!
//! One table serves both forms. Every code point NFC's check does not answer
//! `Yes` for, NFKC's does not either - measured on 17.0.0, and
//! `tests/unicode_data.rs` keeps it true for all 1 114 112.
//!
//! # Canonical order is part of the check
//!
//! Two combining marks out of canonical order are reordered by both forms even
//! when each answers `Yes` on its own - `a`, U+0310, U+0316 comes back as `a`,
//! U+0316, U+0310. So the check also carries `Canonical_Combining_Class` and
//! answers "maybe" when a mark follows one of a higher class, exactly as the
//! algorithm in UAX #15 does.
//!
//! A cheaper stand-in was measured and turned down (`D70`): every mark whose
//! class is not zero is `Grapheme_Cluster_Break=Extend`, so "two `Extend` in a
//! row" would have caught every reordering. It would also have caught every
//! keycap - `#`, U+FE0F, U+20E3 are all class zero and the last two are both
//! `Extend` - and marks already in order, so the first emoji pack would have
//! earned lines it did not need.

mod table;

/// Whether normalizing `text` to NFC or NFKC could give a different text.
///
/// `false` is certain: the text is already in both forms. `true` means it may
/// not be - the module documentation says why that is the loose side.
///
/// The quick check of UAX #15 section 9, with `Maybe` counted as a change.
#[must_use]
pub fn may_change(text: &str) -> bool {
    let mut last_class = 0u8;
    for character in text.chars() {
        let class = combining_class(character);
        if class != 0 && last_class > class {
            return true;
        }
        if !quick_check_yes(character) {
            return true;
        }
        last_class = class;
    }
    false
}

/// Whether `NFKC_Quick_Check` answers `Yes` for one character.
///
/// Binary search over the 301 ranges that answer otherwise - nine comparisons
/// at worst, for a check that runs when a tester copies one report block.
fn quick_check_yes(character: char) -> bool {
    let code = u32::from(character);
    table::NOT_QUICK_YES
        .binary_search_by(|&(start, end)| {
            if end < code {
                std::cmp::Ordering::Less
            } else if start > code {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_err()
}

/// `Canonical_Combining_Class` of one character, 0 for the ones in no range.
///
/// Binary search over 403 ranges, for the same reason as the quick check.
fn combining_class(character: char) -> u8 {
    let code = u32::from(character);
    table::COMBINING_CLASS
        .binary_search_by(|&(start, end, _)| {
            if end < code {
                std::cmp::Ordering::Less
            } else if start > code {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .ok()
        .and_then(|index| table::COMBINING_CLASS.get(index))
        .map_or(0, |&(_, _, class)| class)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Every expectation below was checked against Python's `unicodedata`
    // (Unicode 16.0) on 2026-09-23 - a different implementation, so a mistake
    // in this module cannot stand on both sides of the comparison.

    #[test]
    fn plain_text_and_the_empty_text_are_left_alone() {
        assert!(!may_change(""));
        assert!(!may_change("Hello, world"));
    }

    #[test]
    fn a_precomposed_letter_is_already_normalized() {
        assert!(!may_change("\u{00E9}"));
    }

    #[test]
    fn a_letter_and_its_combining_accent_compose() {
        assert!(may_change("e\u{0301}"));
    }

    #[test]
    fn a_compatibility_character_is_changed_by_nfkc() {
        assert!(may_change("\u{FF41}"), "fullwidth a");
        assert!(may_change("\u{1D407}"), "mathematical bold H");
        assert!(may_change("\u{00A0}"), "no-break space becomes a space");
    }

    #[test]
    fn marks_out_of_canonical_order_are_caught_though_each_says_yes() {
        // U+0310 is class 230 and U+0316 class 220, and both answer Yes on
        // their own - only the ordering rule sees this. NFC returns them swapped.
        assert!(quick_check_yes('\u{0310}') && quick_check_yes('\u{0316}'));
        assert_eq!(
            (combining_class('\u{0310}'), combining_class('\u{0316}')),
            (230, 220)
        );
        assert!(may_change("a\u{0310}\u{0316}"));
    }

    #[test]
    fn marks_already_in_canonical_order_are_left_alone() {
        assert!(!may_change("a\u{0316}\u{0310}"));
        // Equal classes keep their order: the rule is "higher before lower",
        // never "equal".
        assert!(!may_change("a\u{0310}\u{0310}"));
    }

    #[test]
    fn a_keycap_is_left_alone() {
        // The case that turned the "two Extend in a row" stand-in down (`D70`):
        // U+FE0F and U+20E3 are both Extend and both class zero.
        assert!(!may_change("#\u{FE0F}\u{20E3}"));
    }

    #[test]
    fn a_starter_between_two_marks_resets_the_order() {
        // Class 230 then a class-zero letter then class 220: nothing to reorder,
        // because canonical ordering never moves a mark across a starter.
        assert!(!may_change("a\u{0310}b\u{0316}"));
    }

    #[test]
    fn jamo_that_compose_into_a_syllable_are_caught() {
        assert!(may_change("\u{1100}\u{1161}"));
    }

    #[test]
    fn an_emoji_with_a_skin_tone_is_left_alone() {
        assert!(!may_change("\u{1F44B}\u{1F3FD}"));
    }

    #[test]
    fn the_loose_side_is_the_documented_one() {
        // This does not change under normalization - there is nothing before
        // the acute that composes with it - and it is counted anyway, because
        // `Maybe` is. That costs one line in a ticket rather than a missing one.
        assert!(may_change("x\u{0301}"));
    }

    #[test]
    fn the_ends_of_the_tables_do_not_fall_off_the_search() {
        assert!(quick_check_yes('\u{0}'));
        assert!(!quick_check_yes('\u{00A0}'), "the first range");
        assert!(!quick_check_yes('\u{2FA1D}'), "the end of the last range");
        assert!(quick_check_yes('\u{2FA1E}'));
        assert!(quick_check_yes('\u{10FFFF}'));

        assert_eq!(combining_class('\u{0}'), 0);
        assert_eq!(combining_class('\u{0300}'), 230, "the first range");
        assert_eq!(
            combining_class('\u{0314}'),
            230,
            "the end of the first range"
        );
        assert_eq!(combining_class('\u{1E94A}'), 7, "the last range");
        assert_eq!(combining_class('\u{10FFFF}'), 0);
    }
}
