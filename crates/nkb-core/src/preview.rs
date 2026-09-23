//! Showing a value to a person, when half the catalogue is invisible.
//!
//! # The requirement, and the one sentence that shapes it
//!
//! `ux-spec.md` 2 sketches four lines for a value: its name, a PREVIEW, its
//! SHAPE, and the counters. The preview and the shape are here. The sketch also
//! says what they are for: "if the preview shows empty rectangles, the tool stops
//! answering the question `what did I just send`".
//!
//! The division of labour comes from the sketch and it is what keeps this simple:
//!
//! ```text
//! ab␣␣␣cd                               the preview says WHERE
//! 3 zero-width spaces · trailing space   the shape says WHAT
//! ```
//!
//! So the preview needs ONE marker rather than nine symbols. A reader sees at a
//! glance that something sits between `ab` and `cd`, and the line below names it.
//! Nine different symbols would each need a glyph, each need a translation, and
//! would still not say "three of them, and the last one is at the end".
//!
//! # Why the marker is this character
//!
//! `U+2423 OPEN BOX`, taken from the sketch rather than chosen here. Measured
//! 2026-09-23 by reading the `cmap` of the typeface the product ships
//! (DejaVu Sans Mono 2.37, `D50`): the glyph IS there, so the marker cannot
//! itself become the empty rectangle this module exists to prevent. That is not
//! a detail - a marker outside the guarantee (`D52`) would fail exactly on the
//! machines the guarantee was written for. Since `D66` that reading is a test
//! rather than a note: `nkb-gui`'s guarantee table must cover the marker.
//!
//! # What this module deliberately does not do
//!
//! - **it does not count anything.** The counters under the preview come from
//!   [`crate::metrics`] and [`crate::graphemes`], which are separate on purpose:
//!   this module answers "what does this look like", those answer "how much of
//!   it is there", and a value can need one without the other;
//! - **it does not know which characters the shipped typeface covers.** The
//!   guarantee table is `D52` point 1 and lives in `gui`, read from the file's
//!   `cmap`. The rule that uses it is `D52` point 2 and sits beside this module,
//!   in [`crate::typeface`] - separate because a preview is needed by every
//!   surface and a typeface only by the one that draws;
//! - **it does not decide how many digits a number gets.** [`preview_of`] shows
//!   a generated value as its recipe, `ux-spec.md` 2's `100 000 × "a"`, and
//!   carries the count as a number. Grouping its digits is a locale's business
//!   and the palette has no locale mechanism (`OBS-120` is the same gap), so the
//!   sketch's space is not reproduced anywhere - the counters print plain digits
//!   too, and the two lines must read alike.

use crate::value::ValueBody;

/// What stands in for a character with no visible glyph.
///
/// `U+2423 OPEN BOX`, from the sketch in `ux-spec.md` 2. Verified against the
/// `cmap` of the shipped typeface rather than assumed - see the module header.
pub const MARKER: char = '␣';

/// How many code points the preview shows before it starts eliding.
///
/// `ux-spec.md` 2: "above a hundred characters the preview shows the beginning,
/// the end and the length". A hundred is the document's number, not one chosen
/// here.
pub const PREVIEW_LIMIT: usize = 100;

/// How many code points are kept at each end of an elided preview.
///
/// 🔴 Both ends, never just the head. The document says why in as many words:
/// "half the values in the catalogue are interesting exactly at the end, where
/// the space or the invisible character sits". A preview that showed only the
/// beginning would hide the very thing most of these values are about.
const KEPT_PER_END: usize = PREVIEW_LIMIT / 2;

/// A value as a person can see it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preview {
    /// The text with every invisible character replaced by [`MARKER`].
    pub shown: String,
    /// How many code points the whole value has, when `shown` is only part of it.
    ///
    /// `None` means `shown` is the whole value. The caller must say the number
    /// out loud when it is `Some`: a preview that silently showed a fragment
    /// would be the tool claiming to answer "what did I send" while hiding most
    /// of it, which untouchable rule 1 forbids.
    pub elided_total: Option<usize>,
}

/// One true thing about the value's composition.
///
/// Categories rather than characters, and that is the point. A tester needs "3
/// zero-width spaces", not "U+200B U+200B U+200B": the first is readable at a
/// glance and the second is the escaped form, which `nkb emit` already prints.
///
/// The set comes from MEASURING the catalogue rather than from imagination.
/// Measured 2026-09-23 over the three shipped packs: `Cc` (BEL, TAB, LF, CR),
/// `Zs` (SPACE, NO-BREAK SPACE, OGHAM SPACE MARK) and `Cf` (SOFT HYPHEN,
/// ZERO WIDTH SPACE, RIGHT-TO-LEFT OVERRIDE, ZERO WIDTH NO-BREAK SPACE). Every
/// variant below has at least one real value behind it, except where its comment
/// says otherwise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShapeFact {
    /// Characters that occupy no width at all: `U+200B`, the joiners, the BOM.
    ZeroWidth(usize),
    /// Characters that reorder what follows them. Their own class, because the
    /// damage they do is visual rather than textual, and a tester who does not
    /// know one is there will read the field wrongly rather than see a defect.
    BidiControl(usize),
    /// `U+00AD`, which is invisible until the text wraps and then is not.
    SoftHyphen(usize),
    /// A space that is not `U+0020`: no-break, ogham, en, em, ideographic.
    /// Indistinguishable from an ordinary space on screen and different to every
    /// trimmer, which is what makes it worth its own line.
    UnusualSpace(usize),
    /// Horizontal tabs.
    Tab(usize),
    /// Line feeds and carriage returns, counted together: a value carrying either
    /// will surprise a single-line field the same way.
    LineBreak(usize),
    /// Any other control character, `U+0007` among them.
    OtherControl(usize),
    /// The value begins with `U+0020`.
    LeadingSpace,
    /// The value ends with `U+0020`. The single most common reason a field looks
    /// right and compares wrong.
    TrailingSpace,
}

/// Whether `c` has no visible glyph of its own, so the preview must stand in.
///
/// Deliberately NOT "is it whitespace". `U+00A0` is whitespace and needs marking;
/// `U+200B` is not whitespace by most definitions and needs marking more.
#[must_use]
pub fn is_invisible(c: char) -> bool {
    matches!(
        c,
        // Controls, both C0 and C1.
        '\u{0}'..='\u{1F}' | '\u{7F}'..='\u{9F}'
        // Every space separator, the ordinary one included: this tool exists
        // because a trailing space is invisible, so the preview shows it.
        | ' ' | '\u{A0}' | '\u{1680}' | '\u{2000}'..='\u{200A}'
        | '\u{202F}' | '\u{205F}' | '\u{3000}'
        // Format characters: zero-width, joiners, bidi, the soft hyphen, the BOM.
        | '\u{AD}' | '\u{061C}' | '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}'
        | '\u{2060}'..='\u{2064}' | '\u{2066}'..='\u{206F}' | '\u{FEFF}'
        // Line and paragraph separators.
        | '\u{2028}' | '\u{2029}'
    )
}

/// Builds the preview: markers for the invisible, elision for the long.
#[must_use]
pub fn preview(text: &str) -> Preview {
    let total = text.chars().count();
    if total <= PREVIEW_LIMIT {
        return Preview {
            shown: text.chars().map(substitute).collect(),
            elided_total: None,
        };
    }
    // Both ends, because the interesting character is as often at the end as at
    // the beginning. The join is left to the caller's sentence rather than baked
    // in here: an ellipsis in the middle would be text, and text belongs in the
    // dictionary (untouchable rule 9).
    let head: String = text.chars().take(KEPT_PER_END).map(substitute).collect();
    let tail: String = text
        .chars()
        .skip(total - KEPT_PER_END)
        .map(substitute)
        .collect();
    let mut shown = head;
    shown.push_str(&tail);
    Preview {
        shown,
        elided_total: Some(total),
    }
}

fn substitute(c: char) -> char {
    if is_invisible(c) { MARKER } else { c }
}

/// A value as the palette shows it: its text, or the recipe that makes it.
///
/// An enum rather than a flag on [`Preview`], so that a consumer which forgets
/// the recipe does not compile - a flag would let it print a bare unit, `a`,
/// for a value of a hundred thousand characters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValuePreview {
    /// The value itself, markers substituted and long text elided at both ends.
    Text(Preview),
    /// A generated value, shown as what generates it (`ux-spec.md` 2).
    Recipe(Recipe),
}

/// A generated value as a person reads it: `255 × "a"`.
///
/// The words around the two fields are the palette's business, not the core's
/// (untouchable rule 9), so this carries data only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recipe {
    /// The repeated unit, invisible characters substituted by [`MARKER`] - a
    /// unit of three zero-width spaces must not preview as `1000 × ""`.
    pub unit: String,
    /// How many times the unit repeats.
    pub count: u32,
}

/// What the palette shows for a value, worked out from the recipe.
///
/// 🔴 A generated value is shown as its RECIPE whenever the unit fits in a
/// preview, at every length. Measured on the shipped packs: `len-254`,
/// `len-255` and `len-256` all previewed as the same wall of `a`, and the one
/// thing those three values exist to tell apart sat in a footnote. `64 × "😀"`
/// answers "what did I send" exactly; sixty-four emoji have to be counted.
///
/// A unit longer than a preview (the format allows it, no catalogue value has
/// one) falls back to the text, elided at both ends like any long value - and
/// built WITHOUT building the value, which may be a million characters.
#[must_use]
pub fn preview_of(body: &ValueBody) -> ValuePreview {
    match body {
        ValueBody::Literal(text) => ValuePreview::Text(preview(text.as_str())),
        ValueBody::Repeat { unit, count } => {
            let unit = unit.as_str();
            if unit.chars().count() <= PREVIEW_LIMIT {
                ValuePreview::Recipe(Recipe {
                    unit: unit.chars().map(substitute).collect(),
                    count: *count,
                })
            } else {
                ValuePreview::Text(preview_repeated(unit, *count))
            }
        }
    }
}

/// The preview of `unit` repeated `count` times, without the repetition.
///
/// Exactly what [`preview`] would return for the built text - the test below
/// holds it to that - at the cost of the preview rather than of the value.
fn preview_repeated(unit: &str, count: u32) -> Preview {
    let unit_length = unit.chars().count();
    let total = unit_length.saturating_mul(usize::try_from(count).unwrap_or(usize::MAX));
    if total <= PREVIEW_LIMIT {
        return Preview {
            shown: unit.chars().cycle().take(total).map(substitute).collect(),
            elided_total: None,
        };
    }
    // Character `i` of the built text is character `i % unit_length` of the
    // unit, so the tail starts that far into a unit. `unit_length` is not zero
    // here: an empty unit makes `total` zero, which returned above.
    let tail_start = (total - KEPT_PER_END) % unit_length;
    let head = unit.chars().cycle().take(KEPT_PER_END);
    let tail = unit.chars().cycle().skip(tail_start).take(KEPT_PER_END);
    Preview {
        shown: head.chain(tail).map(substitute).collect(),
        elided_total: Some(total),
    }
}

/// Everything worth saying about what the value is made of.
///
/// Order is FIXED rather than sorted, and it runs from "changes what you read"
/// to "changes where the text sits": a reader stops at the first line and must
/// meet the worst thing first. Sorting by count would put a stray tab above a
/// right-to-left override.
#[must_use]
pub fn shape(text: &str) -> Vec<ShapeFact> {
    let mut zero_width = 0;
    let mut bidi = 0;
    let mut soft_hyphen = 0;
    let mut unusual_space = 0;
    let mut tab = 0;
    let mut line_break = 0;
    let mut other_control = 0;

    for c in text.chars() {
        match c {
            '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{2060}' | '\u{FEFF}' => zero_width += 1,
            // `U+061C` since 2026-09-23 (`OBS-121`, `OBS-125`): a direction
            // mark like `U+200E`, and until then shown as nothing and no fact.
            '\u{061C}'
            | '\u{200E}'
            | '\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2066}'..='\u{2069}' => bidi += 1,
            '\u{AD}' => soft_hyphen += 1,
            '\t' => tab += 1,
            '\n' | '\r' | '\u{2028}' | '\u{2029}' => line_break += 1,
            '\u{A0}'
            | '\u{1680}'
            | '\u{2000}'..='\u{200A}'
            | '\u{202F}'
            | '\u{205F}'
            | '\u{3000}' => unusual_space += 1,
            // The ordinary space is not a fact on its own - it is only worth
            // saying when it sits at an end, which the two facts below cover.
            ' ' => {}
            other if is_invisible(other) => other_control += 1,
            _ => {}
        }
    }

    let mut facts = Vec::new();
    // Reordering first: a tester who misses this reads the field backwards.
    if bidi > 0 {
        facts.push(ShapeFact::BidiControl(bidi));
    }
    if zero_width > 0 {
        facts.push(ShapeFact::ZeroWidth(zero_width));
    }
    if unusual_space > 0 {
        facts.push(ShapeFact::UnusualSpace(unusual_space));
    }
    if soft_hyphen > 0 {
        facts.push(ShapeFact::SoftHyphen(soft_hyphen));
    }
    if line_break > 0 {
        facts.push(ShapeFact::LineBreak(line_break));
    }
    if tab > 0 {
        facts.push(ShapeFact::Tab(tab));
    }
    if other_control > 0 {
        facts.push(ShapeFact::OtherControl(other_control));
    }
    // Position last, because it qualifies the value rather than describing its
    // contents - and because it is the line a tester reads when everything else
    // looked fine.
    if text.starts_with(' ') {
        facts.push(ShapeFact::LeadingSpace);
    }
    if text.ends_with(' ') {
        facts.push(ShapeFact::TrailingSpace);
    }
    facts
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;

    /// The example from `ux-spec.md` 2, character for character.
    ///
    /// 🔴 This is the one test that must not be adjusted to match the code. The
    /// sketch in the document is the contract: `ab` plus three zero-width spaces
    /// plus `cd` previews as `ab␣␣␣cd` and its shape names three zero-width
    /// spaces. If the code disagrees, the code is wrong.
    #[test]
    fn the_example_from_the_specification_comes_out_as_the_specification_draws_it() {
        let value = "ab\u{200B}\u{200B}\u{200B}cd";
        let shown = preview(value);
        assert_eq!(shown.shown, "ab␣␣␣cd");
        assert_eq!(shown.elided_total, None);
        assert_eq!(shape(value), vec![ShapeFact::ZeroWidth(3)]);
    }

    #[test]
    fn a_trailing_space_is_both_marked_and_named() {
        // Both, and that is the design: the marker says WHERE, the fact says WHAT.
        // A version with only one of them would leave a tester either unable to
        // see the space or unable to tell it from a zero-width one.
        let shown = preview("Kowalski ");
        assert_eq!(shown.shown, "Kowalski␣");
        assert_eq!(shape("Kowalski "), vec![ShapeFact::TrailingSpace]);
    }

    #[test]
    fn a_leading_space_and_a_trailing_space_are_two_separate_facts() {
        assert_eq!(
            shape(" x "),
            vec![ShapeFact::LeadingSpace, ShapeFact::TrailingSpace]
        );
    }

    #[test]
    fn an_ordinary_space_in_the_middle_is_marked_but_is_not_a_fact() {
        // It IS substituted, because a space is invisible and this tool is about
        // invisible things. It is NOT a fact, because "1 space" in the middle of
        // a name says nothing a reader did not already know.
        assert_eq!(preview("Jan Kowalski").shown, "Jan␣Kowalski");
        assert!(shape("Jan Kowalski").is_empty());
    }

    #[test]
    fn a_value_with_nothing_invisible_is_shown_unchanged_and_has_no_shape() {
        assert_eq!(preview("Kowalski").shown, "Kowalski");
        assert!(shape("Kowalski").is_empty());
        // Non-ASCII visible text must survive untouched: the preview exists to
        // show these, not to mangle them.
        assert_eq!(preview("中文日本語").shown, "中文日本語");
        assert!(shape("中文日本語").is_empty());
    }

    #[test]
    fn the_reordering_character_is_named_before_anything_else() {
        // Order is the point. A right-to-left override makes the field read
        // backwards, so it must not sit below a tab count.
        let value = "\tx\u{202E}y\u{200B}";
        let facts = shape(value);
        assert_eq!(facts[0], ShapeFact::BidiControl(1), "facts: {facts:?}");
        assert!(facts.contains(&ShapeFact::ZeroWidth(1)));
        assert!(facts.contains(&ShapeFact::Tab(1)));
    }

    #[test]
    fn a_long_value_keeps_both_ends_and_reports_the_whole_length() {
        // 🔴 Both ends. The document says half the catalogue is interesting at
        // the END, so a head-only preview would hide the point of those values.
        let mut value = "A".repeat(PREVIEW_LIMIT * 2);
        value.push(' ');
        let shown = preview(&value);
        assert_eq!(shown.elided_total, Some(PREVIEW_LIMIT * 2 + 1));
        assert_eq!(
            shown.shown.chars().count(),
            PREVIEW_LIMIT,
            "an elided preview shows exactly the limit"
        );
        assert!(
            shown.shown.ends_with(MARKER),
            "the trailing space must survive elision: {}",
            shown.shown
        );
        assert!(shown.shown.starts_with('A'));
    }

    #[test]
    fn a_value_exactly_at_the_limit_is_not_elided() {
        // The boundary, stated rather than left to chance: at the limit the whole
        // value is shown, above it the elision starts.
        let at = "A".repeat(PREVIEW_LIMIT);
        assert_eq!(preview(&at).elided_total, None);
        let over = "A".repeat(PREVIEW_LIMIT + 1);
        assert_eq!(preview(&over).elided_total, Some(PREVIEW_LIMIT + 1));
    }

    #[test]
    fn an_empty_value_previews_as_nothing_rather_than_as_a_marker() {
        // `len-0` is a real catalogue entry. A marker here would claim a
        // character that is not there.
        assert_eq!(preview("").shown, String::new());
        assert_eq!(preview("").elided_total, None);
        assert!(shape("").is_empty());
    }

    /// Every character the shipped packs actually carry is accounted for.
    ///
    /// Measured 2026-09-23 over `whitespace`, `unicode-text` and `length-bombs`.
    /// A category silently missing here would show as a value whose shape line is
    /// empty although something invisible is inside it - the exact failure this
    /// module exists to prevent.
    /// Every character that changes the direction of text is ONE fact, the one
    /// the core puts first. Not measured in the shipped packs - none of them
    /// carries an isolate or `U+061C` - which is exactly why it is written out:
    /// the first Arabic pack must not be the test.
    #[test]
    fn every_direction_character_is_a_reordering_fact_and_is_marked() {
        for c in [
            '\u{061C}', '\u{200E}', '\u{200F}', '\u{202A}', '\u{202E}', '\u{2066}', '\u{2067}',
            '\u{2068}', '\u{2069}',
        ] {
            assert_eq!(
                shape(&format!("x{c}y")),
                vec![ShapeFact::BidiControl(1)],
                "U+{:04X} is not counted as reordering",
                c as u32
            );
            assert!(is_invisible(c), "U+{:04X} is drawn as itself", c as u32);
        }
    }

    #[test]
    fn every_invisible_character_measured_in_the_shipped_packs_has_a_fact() {
        for (c, expected) in [
            ('\u{7}', ShapeFact::OtherControl(1)),
            ('\t', ShapeFact::Tab(1)),
            ('\n', ShapeFact::LineBreak(1)),
            ('\r', ShapeFact::LineBreak(1)),
            ('\u{A0}', ShapeFact::UnusualSpace(1)),
            ('\u{AD}', ShapeFact::SoftHyphen(1)),
            ('\u{1680}', ShapeFact::UnusualSpace(1)),
            ('\u{200B}', ShapeFact::ZeroWidth(1)),
            ('\u{202E}', ShapeFact::BidiControl(1)),
            ('\u{FEFF}', ShapeFact::ZeroWidth(1)),
        ] {
            let value = format!("x{c}y");
            assert_eq!(
                shape(&value),
                vec![expected],
                "U+{:04X} has no fact of its own",
                c as u32
            );
            assert!(
                is_invisible(c),
                "U+{:04X} is a fact but not substituted in the preview",
                c as u32
            );
            assert_eq!(
                preview(&value).shown,
                format!("x{MARKER}y"),
                "U+{:04X} is not substituted",
                c as u32
            );
        }
    }

    /// The marker itself must be a visible character.
    ///
    /// A marker that `is_invisible` accepted would be replaced by itself
    /// forever - harmless by luck rather than by design, and a trap for whoever
    /// changes the constant next.
    #[test]
    fn the_marker_is_not_itself_something_the_preview_would_replace() {
        assert!(!is_invisible(MARKER));
        assert_eq!(MARKER.to_string().chars().count(), 1);
    }

    fn repeat(unit: &str, count: u32) -> ValueBody {
        ValueBody::Repeat {
            unit: crate::text::LiteralText::new(unit),
            count,
        }
    }

    fn recipe(unit: &str, count: u32) -> ValuePreview {
        ValuePreview::Recipe(Recipe {
            unit: unit.to_owned(),
            count,
        })
    }

    #[test]
    fn a_written_value_previews_as_its_text() {
        let body = ValueBody::Literal(crate::text::LiteralText::new("ab\u{200B}cd"));
        assert_eq!(
            preview_of(&body),
            ValuePreview::Text(preview("ab\u{200B}cd"))
        );
    }

    #[test]
    fn the_three_lengths_around_the_classic_limit_are_told_apart() {
        // The reason this exists. As text, all three previewed as a hundred `a`
        // and differed only in the elision note.
        assert_eq!(preview_of(&repeat("a", 254)), recipe("a", 254));
        assert_eq!(preview_of(&repeat("a", 255)), recipe("a", 255));
        assert_eq!(preview_of(&repeat("a", 256)), recipe("a", 256));
    }

    #[test]
    fn a_short_generated_value_is_a_recipe_too() {
        // Sixty-four emoji fit in a preview and would still have to be counted.
        assert_eq!(
            preview_of(&repeat("\u{1F600}", 64)),
            recipe("\u{1F600}", 64)
        );
    }

    #[test]
    fn an_invisible_unit_is_shown_by_its_marker() {
        assert_eq!(
            preview_of(&repeat("\u{200B}\u{200B}", 1000)),
            recipe("\u{2423}\u{2423}", 1000)
        );
        assert_eq!(preview_of(&repeat(" ", 3)), recipe("\u{2423}", 3));
    }

    #[test]
    fn an_empty_unit_says_so_rather_than_disappearing() {
        assert_eq!(preview_of(&repeat("", 5)), recipe("", 5));
    }

    #[test]
    fn a_unit_at_the_limit_is_a_recipe_and_one_past_it_is_text() {
        let at_limit = "b".repeat(PREVIEW_LIMIT);
        assert_eq!(preview_of(&repeat(&at_limit, 3)), recipe(&at_limit, 3));
        let past_limit = "b".repeat(PREVIEW_LIMIT + 1);
        assert!(matches!(
            preview_of(&repeat(&past_limit, 3)),
            ValuePreview::Text(_)
        ));
    }

    #[test]
    fn a_long_unit_previews_exactly_as_its_built_text_would() {
        // The oracle is the function every written value already goes through,
        // fed the text built the slow way. Units of awkward lengths, with an
        // invisible character in them so the substitution is checked too, and
        // counts that put the tail at every offset into a unit.
        for unit_length in [101, 150, 257] {
            let unit: String = (0..unit_length)
                .map(|i| match i % 7 {
                    0 => '\u{200B}',
                    1 => ' ',
                    other => char::from(b'a' + u8::try_from(other).expect("small")),
                })
                .collect();
            for count in [1, 2, 3, 7] {
                let built = unit.repeat(usize::try_from(count).expect("small"));
                assert_eq!(
                    preview_of(&repeat(&unit, count)),
                    ValuePreview::Text(preview(&built)),
                    "unit of {unit_length} code points, repeated {count} times"
                );
            }
        }
    }
}
