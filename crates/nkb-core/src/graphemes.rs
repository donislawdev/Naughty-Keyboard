//! Counting what a person calls a character.
//!
//! # Why this is a module and not a call to `chars().count()`
//!
//! `ux-spec.md` 2 sketches three counters under a value, and the reason there
//! are three is measured rather than decorative. The family emoji is ONE thing
//! on screen, FIVE code points, and EIGHTEEN bytes. A tester filing a bug about
//! a field that rejects it needs the number the field's own limit is stated in,
//! and which of the three that is depends on whose field it is. So the tool
//! shows all of them and picks for nobody.
//!
//! Until this module existed the core could answer two of those three
//! (`metrics.rs`), and the palette printed two counters under a sketch that
//! promised three. That was `OBS-106`.
//!
//! # Why the table is ours rather than a dependency
//!
//! `nkb-core` depends on nothing, and that is a closed decision rather than a
//! preference (`architektura.md` 2.1). The alternative on the table was
//! `unicode-segmentation` reached through a port - rejected because
//! `architektura.md` 8 asks for a SECOND concrete variant before an extension
//! point is built, and a port in front of a pure function has none. The data it
//! needs is vendored instead, under `crates/nkb-core/unicode/`, and
//! `tests/unicode_data.rs` proves the generated table still says what those
//! files say, for every one of the 1 114 112 code points.
//!
//! # Why this number can be trusted
//!
//! Not because the rules below look right. Because `tests/grapheme_conformance.rs`
//! runs the official Unicode suite - 766 cases, including every rule that is easy
//! to get wrong - and because `tools/sonda-grafemy` pits this implementation
//! against a completely different one over the whole shipped catalogue.
//!
//! # What this module deliberately does not do
//!
//! It does not extend [`crate::metrics::TextMetrics`]. That type exists to
//! measure a RECIPE without building the text it describes - a value may be a
//! million characters and the warning has to arrive before the memory is eaten.
//! Graphemes cannot be counted that way: repeating a unit changes the clusters
//! at the seams, so `"\u{1F1F5}"` repeated three times is two clusters and not
//! three, and a lone combining mark repeated a thousand times is one. A field
//! that would have to be absent for half the catalogue does not belong in a
//! struct whose promise is that every number in it is correct.

mod table;

/// The version of the standard these rules and this table come from.
///
/// 🔴 Not the newest version, and that is the point. The victim application
/// this product is checked against counts graphemes with a different
/// implementation on purpose, so that one bug cannot stand on both sides of the
/// comparison - and that implementation states 17.0.0. Two implementations of
/// two versions would disagree on real text, and the disagreement would look
/// like a defect in one of them.
pub const UNICODE_VERSION: (u8, u8, u8) = (17, 0, 0);

/// `Grapheme_Cluster_Break`, the property the rules in UAX #29 are written in.
///
/// The numbers are a contract with `table.rs`, which is generated: changing one
/// silently changes the meaning of forty-seven kilobytes of data. The test that
/// rebuilds the table from the Unicode files is what notices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Break {
    Other = 0,
    Cr = 1,
    Lf = 2,
    Control = 3,
    Extend = 4,
    Zwj = 5,
    RegionalIndicator = 6,
    Prepend = 7,
    SpacingMark = 8,
    L = 9,
    V = 10,
    T = 11,
    Lv = 12,
    Lvt = 13,
}

/// `Indic_Conjunct_Break`, which rule GB9c is stated in.
///
/// Added to the standard in 15.1 and easy to miss, because text that needs it
/// renders plausibly without it - the count is simply wrong. Sixteen of the 766
/// conformance cases fail when this is ignored, measured.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Conjunct {
    None = 0,
    Extend = 1,
    Linker = 2,
    Consonant = 3,
}

/// Everything the rules need to know about one character.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Properties {
    cluster_break: Break,
    /// `Extended_Pictographic`, which rule GB11 is stated in. Wider than
    /// "is an emoji": it covers characters that join into emoji sequences.
    pictographic: bool,
    conjunct: Conjunct,
}

impl Properties {
    /// The value for a character in no range of the table.
    ///
    /// Left out of the table on purpose - it is the answer for 1 092 252 of the
    /// 1 114 112 code points, and storing it would make the table fifty times
    /// larger while saying nothing.
    const DEFAULT: Self = Self {
        cluster_break: Break::Other,
        pictographic: false,
        conjunct: Conjunct::None,
    };
}

/// Unpacks the byte the generated table stores.
///
/// The two `_` arms cannot be reached from the shipped table: every byte in it
/// has a low nibble of at most 13 and conjunct bits of at most 3, and
/// `tests/unicode_data.rs` asserts exactly that before it checks anything else.
/// They are written as the default rather than as a panic because a validator
/// that crashes is a crash inside somebody else's pipeline.
const fn unpack(packed: u8) -> Properties {
    let cluster_break = match packed & 0x0F {
        1 => Break::Cr,
        2 => Break::Lf,
        3 => Break::Control,
        4 => Break::Extend,
        5 => Break::Zwj,
        6 => Break::RegionalIndicator,
        7 => Break::Prepend,
        8 => Break::SpacingMark,
        9 => Break::L,
        10 => Break::V,
        11 => Break::T,
        12 => Break::Lv,
        13 => Break::Lvt,
        _ => Break::Other,
    };
    let conjunct = match (packed >> 5) & 0x03 {
        1 => Conjunct::Extend,
        2 => Conjunct::Linker,
        3 => Conjunct::Consonant,
        _ => Conjunct::None,
    };
    Properties {
        cluster_break,
        pictographic: packed & 0x10 != 0,
        conjunct,
    }
}

/// Looks one character up in the generated table.
///
/// Binary search over 1631 ranges - eleven comparisons for the worst character
/// in the standard. A two stage lookup would make it constant, and was not
/// built: the longest value the catalogue can hold is a hundred thousand
/// characters, which is a millisecond of this either way, and a shape nobody
/// needs is a shape somebody later mistakes for a requirement.
fn properties(c: char) -> Properties {
    let code = c as u32;
    let mut low = 0usize;
    let mut high = table::PROPERTIES.len();
    while low < high {
        let middle = low + (high - low) / 2;
        match table::PROPERTIES.get(middle) {
            Some(&(start, end, packed)) => {
                if code < start {
                    high = middle;
                } else if code > end {
                    low = middle + 1;
                } else {
                    return unpack(packed);
                }
            }
            // Unreachable while `middle < high <= len`, and written as the
            // default rather than as a panic for the reason `unpack` gives.
            None => return Properties::DEFAULT,
        }
    }
    Properties::DEFAULT
}

/// What the text before a possible boundary has been doing.
///
/// Three of the rules cannot be decided by looking at two neighbouring
/// characters, and each needs its own memory of what came earlier:
///
/// - GB12 and GB13 need the PARITY of the regional indicators immediately
///   before the boundary, because flags come in pairs and a third one starts a
///   new cluster rather than joining.
/// - GB11 needs to know that an `Extended_Pictographic` character was followed
///   by zero or more `Extend` and then by a zero width joiner.
/// - GB9c needs to know that a consonant was followed by a linker, possibly
///   with `Extend` characters mixed in.
///
/// Each field says what is true of the text ENDING AT the character just read.
#[derive(Debug, Clone, Copy)]
struct Trail {
    /// How many regional indicators end the text so far.
    regional_indicators: usize,
    /// `Extended_Pictographic` followed by `Extend*` ends the text so far.
    pictographic_run: bool,
    /// The same, followed by a zero width joiner. This is GB11's left side.
    pictographic_joined: bool,
    /// `Consonant` followed by `[Extend | Linker]*` ends the text so far.
    conjunct_run: bool,
    /// The same, with at least one linker among them. This is GB9c's left side.
    conjunct_linked: bool,
}

impl Trail {
    fn new() -> Self {
        Self {
            regional_indicators: 0,
            pictographic_run: false,
            pictographic_joined: false,
            conjunct_run: false,
            conjunct_linked: false,
        }
    }

    /// Extends the trail with one more character.
    fn push(&mut self, next: Properties) {
        self.regional_indicators = if next.cluster_break == Break::RegionalIndicator {
            self.regional_indicators.saturating_add(1)
        } else {
            0
        };

        if next.pictographic {
            self.pictographic_run = true;
            self.pictographic_joined = false;
        } else if self.pictographic_run && next.cluster_break == Break::Extend {
            self.pictographic_joined = false;
        } else if self.pictographic_run && next.cluster_break == Break::Zwj {
            self.pictographic_run = false;
            self.pictographic_joined = true;
        } else {
            self.pictographic_run = false;
            self.pictographic_joined = false;
        }

        match next.conjunct {
            Conjunct::Consonant => {
                self.conjunct_run = true;
                self.conjunct_linked = false;
            }
            Conjunct::Linker if self.conjunct_run => self.conjunct_linked = true,
            // An `Extend` inside the run leaves the run exactly as it was: the
            // rule allows them on both sides of the linker.
            Conjunct::Extend if self.conjunct_run => {}
            _ => {
                self.conjunct_run = false;
                self.conjunct_linked = false;
            }
        }
    }
}

/// Whether a cluster boundary falls between two characters.
///
/// The rules are UAX #29 table 1c in order, and the order is the rule: the
/// first one that matches decides, and GB999 catches everything left. Each arm
/// carries its number so that a failing conformance case can be read back to
/// the line that got it wrong.
fn is_boundary(trail: &Trail, before: Properties, after: Properties) -> bool {
    use Break::{
        Control, Cr, Extend, L, Lf, Lv, Lvt, Prepend, RegionalIndicator, SpacingMark, T, V, Zwj,
    };

    match (before.cluster_break, after.cluster_break) {
        (Cr, Lf) => false,              // GB3
        (Cr | Lf | Control, _) => true, // GB4
        (_, Cr | Lf | Control) => true, // GB5
        (L, L | V | Lv | Lvt) => false, // GB6
        (Lv | V, V | T) => false,       // GB7
        (Lvt | T, T) => false,          // GB8
        (_, Extend | Zwj) => false,     // GB9
        (_, SpacingMark) => false,      // GB9a
        (Prepend, _) => false,          // GB9b
        _ if trail.conjunct_linked && after.conjunct == Conjunct::Consonant => false, // GB9c
        _ if trail.pictographic_joined && after.pictographic => false, // GB11
        (_, RegionalIndicator) if trail.regional_indicators % 2 == 1 => false, // GB12, GB13
        _ => true,                      // GB999
    }
}

/// How many grapheme clusters the text holds.
///
/// Empty text is zero clusters, not one. That is GB1 read literally - "break at
/// the start and end of text, UNLESS THE TEXT IS EMPTY" - and it matters here
/// because an empty value is a value this catalogue ships on purpose.
#[must_use]
pub fn count(text: &str) -> usize {
    let mut characters = text.chars();
    let Some(first) = characters.next() else {
        return 0;
    };

    let mut previous = properties(first);
    let mut trail = Trail::new();
    trail.push(previous);
    let mut clusters = 1usize;

    for character in characters {
        let current = properties(character);
        if is_boundary(&trail, previous, current) {
            clusters = clusters.saturating_add(1);
        }
        trail.push(current);
        previous = current;
    }

    clusters
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cp(code: u32) -> String {
        match char::from_u32(code) {
            Some(c) => c.to_string(),
            None => String::new(),
        }
    }

    #[test]
    fn empty_text_is_no_clusters_at_all() {
        assert_eq!(count(""), 0);
    }

    #[test]
    fn ascii_counts_one_per_character() {
        assert_eq!(count("abc"), 3);
    }

    #[test]
    fn the_example_the_specification_used_to_get_wrong() {
        // ab + three zero width spaces + cd. `test-strategy.md` 3 said four for
        // years of drafts. It is seven, because U+200B is Control in this
        // property and therefore its own cluster - unlike the joiner, which
        // joins. That was OBS-105, found by the victim rather than by reading.
        let value = format!("ab{0}{0}{0}cd", cp(0x200B));
        assert_eq!(count(&value), 7);
    }

    #[test]
    fn the_family_emoji_is_one_thing_on_screen() {
        // Five code points, eighteen bytes, one cluster. This single value is
        // why the palette shows three counters instead of one.
        let family = format!(
            "{}{}{}{}{}",
            cp(0x1F468),
            cp(0x200D),
            cp(0x1F469),
            cp(0x200D),
            cp(0x1F467)
        );
        assert_eq!(count(&family), 1);
        assert_eq!(family.chars().count(), 5);
        assert_eq!(family.len(), 18);
    }

    #[test]
    fn a_carriage_return_and_a_line_feed_are_one_cluster_but_the_other_order_is_two() {
        assert_eq!(count("\r\n"), 1);
        assert_eq!(count("\n\r"), 2);
    }

    #[test]
    fn flags_pair_up_and_a_third_indicator_starts_a_new_one() {
        let indicator = cp(0x1F1F5);
        assert_eq!(count(&indicator), 1);
        assert_eq!(count(&indicator.repeat(2)), 1);
        assert_eq!(count(&indicator.repeat(3)), 2);
        assert_eq!(count(&indicator.repeat(4)), 2);
    }

    #[test]
    fn a_combining_mark_joins_whatever_stands_before_it() {
        assert_eq!(count(&format!("e{}", cp(0x0301))), 1);
        // With nothing to attach to it is still one cluster, by GB1.
        assert_eq!(count(&cp(0x0301).repeat(5)), 1);
    }

    #[test]
    fn a_hangul_syllable_spelled_out_in_jamo_is_one_cluster() {
        // L V T, which rules GB6 and GB7 hold together.
        let syllable = format!("{}{}{}", cp(0x1100), cp(0x1161), cp(0x11A8));
        assert_eq!(syllable.chars().count(), 3);
        assert_eq!(count(&syllable), 1);
    }

    #[test]
    fn a_devanagari_conjunct_is_one_cluster() {
        // KA + VIRAMA + SSA. Without GB9c this counts as two, and it renders
        // plausibly either way - which is exactly why the rule is easy to miss.
        let conjunct = format!("{}{}{}", cp(0x0915), cp(0x094D), cp(0x0937));
        assert_eq!(count(&conjunct), 1);
    }

    #[test]
    fn a_skin_tone_modifier_does_not_start_a_second_cluster() {
        let waving = format!("{}{}", cp(0x1F44B), cp(0x1F3FD));
        assert_eq!(count(&waving), 1);
    }

    #[test]
    fn the_table_answers_for_characters_outside_every_range() {
        // A private use character sits in no range at all and must come back as
        // the default rather than as whatever neighbours it in the table.
        assert_eq!(properties('\u{F0000}'), Properties::DEFAULT);
        assert_eq!(count(&cp(0xF0000)), 1);
    }

    #[test]
    fn the_first_and_last_code_points_do_not_fall_off_the_binary_search() {
        assert_eq!(properties('\u{0}').cluster_break, Break::Control);
        assert_eq!(properties('\u{10FFFF}'), Properties::DEFAULT);
    }
}
