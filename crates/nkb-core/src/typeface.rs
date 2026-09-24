//! What the typeface the product ships draws by itself, and what it leaves to
//! the machine.
//!
//! # The promise this module keeps honest
//!
//! The preview uses ONE named family, the one the product carries inside the
//! binary (`D50`), and for every character that family lacks it falls back to
//! whatever the machine has (`D52`). So what a tester sees depends on the
//! machine for some characters and not for others, and the tool cannot find out
//! at run time which: the toolkit does not report what its fallback drew.
//!
//! What the tool CAN know is its own guarantee - the characters its typeface
//! draws everywhere, read from the typeface's own table rather than from
//! anyone's opinion. `D52` asks the palette to say, for every value, which of
//! the characters on screen lie outside that guarantee. That is not a claim
//! that they will not render. It is the edge of the promise, said out loud,
//! which is untouchable rule 1.
//!
//! # Where the table comes from, and why the core does not know
//!
//! The guarantee is DATA about a file the interface ships, so it lives in the
//! interface and is handed in. The RULE - which characters of a text count - is
//! logic, and lives here. This module never learns which typeface the table
//! describes, which is what lets a second family arrive one day (route C in
//! `D52`) without anything here moving.
//!
//! # What was measured before this was written, 2026-09-23
//!
//! The toolkit's text layout (`parley`, under Slint 1.18.1) chooses a typeface
//! PER CLUSTER, asking each candidate whether it maps every character of the
//! cluster to a glyph. Three consequences, each of which shaped the code below:
//!
//! - 🔴 **there is no exemption for "default ignorable" characters.** The
//!   obvious refinement - `U+FE0F VARIATION SELECTOR-16` draws nothing, so do
//!   not report it - would produce a FALSE guarantee. The layout counts that
//!   selector when choosing a typeface, the shipped one does not map it, and so
//!   `U+2764` followed by it is drawn by the machine's typeface although the
//!   heart itself IS in ours. Reporting the selector is what tells the truth.
//!   ⊕ Since `D79` (2026-09-24) the preview replaces the selector by its
//!   marker, like every character a reviewer of a pack cannot see, so it no
//!   longer reaches the layout and the heart stays in the guarantee. The
//!   measurement above still stands and is WHY that matters - the rule below
//!   exempts what the preview replaces, never what is merely ignorable.
//! - **characters the preview replaces are skipped.** Every character
//!   [`crate::preview::is_invisible`] names is drawn as the marker, never as
//!   itself, so its own glyph is never asked for.
//! - **the answer errs on the side of saying too much, never too little.** The
//!   layout also tries a character's composed and decomposed forms, so a
//!   precomposed letter missing from the table can still be drawn by the
//!   shipped typeface from its pieces. Reported here anyway: a character that
//!   turns out to be fine costs a line of text, a character wrongly promised
//!   costs a tester the answer to "what did I send".
//!
//! # What this module deliberately does not do
//!
//! It does not report CLUSTERS. When one character of a cluster is outside the
//! guarantee, the whole cluster is drawn by another typeface - the `1` of a
//! keycap included. The characters listed are the cause of that, and they are
//! what a tester can look up. The cluster is visible in the preview right above.

use std::cmp::Ordering;
use std::collections::BTreeSet;

use crate::preview::is_invisible;

/// The last code point in the standard.
const MAX_CODE_POINT: u32 = 0x10_FFFF;

/// The code points a typeface draws by itself, on any machine.
///
/// Held as inclusive ranges, sorted and disjoint. That shape is checked when the
/// value is built rather than assumed when it is searched: a lookup over an
/// unsorted table does not fail, it answers wrongly for part of the standard.
///
/// 🔴 Not "coverage". That word is taken three times in this project already -
/// `FieldCoverage`, `PackCatalogue::coverage` and `RuleCoverage` - and a fourth
/// meaning is exactly the drift document 17 section 8 warns about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TypefaceGuarantee {
    ranges: &'static [(u32, u32)],
}

impl TypefaceGuarantee {
    /// Builds a guarantee from inclusive ranges, or refuses them.
    ///
    /// Refused: a range that ends before it starts, a range reaching past
    /// `U+10FFFF`, and ranges out of order or overlapping. Adjacent ranges are
    /// accepted - they are wasteful, not wrong.
    ///
    /// `const` on purpose. A table written into a `static` is checked BY THE
    /// COMPILER, so a malformed one cannot be shipped even by a build that never
    /// runs its tests.
    #[must_use]
    pub const fn new(ranges: &'static [(u32, u32)]) -> Option<Self> {
        let mut rest = ranges;
        let mut previous_end: Option<u32> = None;
        while let [(start, end), tail @ ..] = rest {
            if *start > *end || *end > MAX_CODE_POINT {
                return None;
            }
            if let Some(previous) = previous_end
                && previous >= *start
            {
                return None;
            }
            previous_end = Some(*end);
            rest = tail;
        }
        Some(Self { ranges })
    }

    /// Whether the typeface draws `c` by itself.
    #[must_use]
    pub fn covers(&self, c: char) -> bool {
        let code = u32::from(c);
        self.ranges
            .binary_search_by(|&(start, end)| {
                if end < code {
                    Ordering::Less
                } else if start > code {
                    Ordering::Greater
                } else {
                    Ordering::Equal
                }
            })
            .is_ok()
    }
}

/// The characters of `text` whose appearance depends on the machine.
///
/// Each character once, in the order it first appears - the order a tester
/// reads the preview in. Characters the preview replaces by its marker are left
/// out, because their own glyph is never drawn.
///
/// The caller decides WHICH text: the palette passes what it actually draws,
/// which for a long value is only the part it shows. A character in the elided
/// middle is not on screen, so the typeface is not asked for it.
#[must_use]
pub fn outside_guarantee(text: &str, guarantee: &TypefaceGuarantee) -> Vec<char> {
    let mut seen = BTreeSet::new();
    let mut outside = Vec::new();
    for c in text.chars() {
        if is_invisible(c) || guarantee.covers(c) {
            continue;
        }
        // A set beside the list, because the caller may hand in a whole value
        // rather than a preview, and a value of twenty thousand distinct
        // characters must not cost twenty thousand squared comparisons.
        if seen.insert(c) {
            outside.push(c);
        }
    }
    outside
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;
    use crate::preview::MARKER;

    /// Basic Latin and the Latin-1 letters, plus the marker, with a gap between.
    static SMALL: [(u32, u32); 3] = [(0x0020, 0x007E), (0x00C0, 0x00FF), (0x2423, 0x2423)];

    fn small() -> TypefaceGuarantee {
        TypefaceGuarantee::new(&SMALL).expect("the specimen table is well formed")
    }

    #[test]
    fn a_character_inside_the_guarantee_is_not_reported() {
        assert!(outside_guarantee("abc", &small()).is_empty());
    }

    #[test]
    fn characters_outside_are_reported_once_each_in_the_order_they_appear() {
        // Two ideographs, the first one twice, with covered text around them.
        let found = outside_guarantee("a\u{540D}b\u{524D}c\u{540D}", &small());
        assert_eq!(found, vec!['\u{540D}', '\u{524D}']);
    }

    #[test]
    fn a_character_the_preview_replaces_is_never_reported() {
        // Neither the zero-width space nor the right-to-left override is in the
        // specimen table. The preview draws the marker in their place, so their
        // own glyphs are never asked for.
        let found = outside_guarantee("a\u{200B}b\u{202E}c", &small());
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn the_variation_selector_never_reaches_the_layout_since_d79() {
        // The case the module header is about. Until `D79` the preview drew the
        // selector as itself, the layout counted it, and it sent the heart to
        // the machine's typeface - so it was reported. Since then the preview
        // draws the MARKER in its place, and the marker is a cluster of its own,
        // so the heart's cluster holds the heart alone and stays in the
        // guarantee. Both halves are asked, not assumed.
        static WITH_HEART: [(u32, u32); 2] = [(0x2423, 0x2423), (0x2764, 0x2764)];
        let guarantee = TypefaceGuarantee::new(&WITH_HEART).expect("well formed");
        let drawn = crate::preview::preview("\u{2764}\u{FE0F}").shown;
        assert!(
            !drawn.contains('\u{FE0F}'),
            "the selector reaches the layout: {drawn:?}"
        );
        assert_eq!(crate::graphemes::count(&drawn), 2, "{drawn:?}");
        assert!(outside_guarantee(&drawn, &guarantee).is_empty());
        // The raw value too, for a caller that passes one: the selector is
        // skipped because the preview replaces it, not because it is ignorable.
        assert!(outside_guarantee("\u{2764}\u{FE0F}", &guarantee).is_empty());
    }

    #[test]
    fn the_marker_gets_no_exemption_of_its_own() {
        // The marker has to be IN the shipped guarantee - that is a fact about
        // the typeface, checked where the table lives. Here it is an ordinary
        // character, so a table without it reports it.
        static WITHOUT_MARKER: [(u32, u32); 1] = [(0x0020, 0x007E)];
        let guarantee = TypefaceGuarantee::new(&WITHOUT_MARKER).expect("well formed");
        assert_eq!(outside_guarantee("a\u{2423}", &guarantee), vec![MARKER]);
        assert!(outside_guarantee("a\u{2423}", &small()).is_empty());
    }

    #[test]
    fn an_empty_text_has_nothing_outside() {
        assert!(outside_guarantee("", &small()).is_empty());
    }

    #[test]
    fn an_empty_guarantee_reports_every_visible_character() {
        static NOTHING: [(u32, u32); 0] = [];
        let guarantee = TypefaceGuarantee::new(&NOTHING).expect("an empty table is a table");
        assert_eq!(outside_guarantee("ab a", &guarantee), vec!['a', 'b']);
    }

    #[test]
    fn a_character_above_the_basic_plane_is_looked_up_like_any_other() {
        // Mathematical bold capital H, from the pseudolocalisation value in the
        // shipped packs. Four bytes in UTF-8 and a surrogate pair in UTF-16, so
        // a lookup that went through either encoding would get it wrong.
        static ASTRAL: [(u32, u32); 1] = [(0x1D400, 0x1D406)];
        let guarantee = TypefaceGuarantee::new(&ASTRAL).expect("well formed");
        assert!(guarantee.covers('\u{1D406}'));
        assert!(!guarantee.covers('\u{1D407}'));
    }

    #[test]
    fn every_edge_of_every_range_answers_correctly() {
        let guarantee = small();
        for &(start, end) in &SMALL {
            for (code, inside) in [
                (start - 1, false),
                (start, true),
                (end, true),
                (end + 1, false),
            ] {
                let c = char::from_u32(code).expect("the specimen stays clear of surrogates");
                assert_eq!(
                    guarantee.covers(c),
                    inside,
                    "U+{code:04X} next to the range {start:04X}..={end:04X}"
                );
            }
        }
    }

    #[test]
    fn a_malformed_table_is_refused() {
        static BACKWARDS: [(u32, u32); 1] = [(0x0042, 0x0041)];
        static UNSORTED: [(u32, u32); 2] = [(0x0100, 0x0110), (0x0041, 0x005A)];
        static OVERLAPPING: [(u32, u32); 2] = [(0x0041, 0x005A), (0x005A, 0x0060)];
        static BEYOND: [(u32, u32); 1] = [(0x10_FFFF, 0x11_0000)];
        for (name, table) in [
            ("backwards", &BACKWARDS[..]),
            ("unsorted", &UNSORTED[..]),
            ("overlapping", &OVERLAPPING[..]),
            ("beyond the standard", &BEYOND[..]),
        ] {
            assert!(
                TypefaceGuarantee::new(table).is_none(),
                "a {name} table was accepted"
            );
        }
    }

    #[test]
    fn adjacent_ranges_are_wasteful_but_not_wrong() {
        static ADJACENT: [(u32, u32); 2] = [(0x0041, 0x004D), (0x004E, 0x005A)];
        let guarantee = TypefaceGuarantee::new(&ADJACENT).expect("adjacent ranges are accepted");
        assert!(guarantee.covers('M'));
        assert!(guarantee.covers('N'));
    }
}
