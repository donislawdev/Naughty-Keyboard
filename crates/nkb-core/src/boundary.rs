//! Values that answer the limit a field states, rather than a list of odd
//! characters.
//!
//! A specification says "at most 50 characters". This module turns that one
//! sentence into the values worth trying: nothing, one under, exactly at and
//! one over, each made of characters that weigh differently in the four units
//! a limit can be counted in.
//!
//! # Why four units
//!
//! "50 characters" is one of four different numbers, and the person testing
//! the field rarely knows which. A browser's `maxlength` and most managed
//! runtimes count UTF-16 units, several databases count code points, others
//! count bytes, and the person reading the specification counts what they can
//! see. Fifty of a two-byte letter is fifty of three of those and a hundred of
//! the fourth.
//!
//! # Why a value is padded at the front
//!
//! Asked for 51 bytes of a two-byte letter, the answer is one `a` and then
//! twenty-five letters, never twenty-five letters and one `a`. The heavy
//! character stands at the end, so a field that cuts at 50 bytes cuts through
//! the middle of it - the fault a limit counted in bytes or UTF-16 units is
//! most likely to hide, and one that no value made of whole characters up to
//! the limit can provoke.
//!
//! # A value is a recipe here as well
//!
//! Nothing in this module builds text to measure it. [`BoundaryValue::sizes`]
//! answers from the recipe, and [`BoundaryValue::materialise`] is the separate,
//! explicit step. The text itself is built by [`crate::value::ValueBody`], the
//! body every other value of the tool has ([`BoundaryValue::body`]): what the
//! command prints and what the palette types come from one builder.

use crate::graphemes;
use crate::text::LiteralText;
use crate::value::ValueBody;

/// The largest limit this module answers.
///
/// At this limit the heaviest value is 100 001 clusters of five code points,
/// which stays under [`crate::value::MAX_VALUE_CODE_POINTS`] - the same ceiling
/// every other value of the tool lives under. Widening it later breaks nobody.
pub const MAX_LIMIT: u32 = 100_000;

/// What a limit is counted in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    /// Characters as a person counts them: grapheme clusters.
    Graphemes,
    /// Unicode scalar values.
    CodePoints,
    /// UTF-16 code units, what a browser's `maxlength` counts.
    Utf16,
    /// Bytes of UTF-8.
    Bytes,
}

impl Unit {
    /// Every unit, in the order the tool names them everywhere.
    pub const ALL: [Self; 4] = [Self::Graphemes, Self::CodePoints, Self::Utf16, Self::Bytes];

    /// The word a person types and a document carries. A public name.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Graphemes => "graphemes",
            Self::CodePoints => "code-points",
            Self::Utf16 => "utf16",
            Self::Bytes => "bytes",
        }
    }

    /// Reads the word a person typed.
    #[must_use]
    pub fn parse(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|unit| unit.id() == word)
    }
}

/// How long a text is, in each of the four units.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sizes {
    pub graphemes: usize,
    pub code_points: usize,
    pub utf16_units: usize,
    pub bytes: usize,
}

impl Sizes {
    /// Measures text by reading it. Meant for the short unit of a recipe and
    /// for tests - a whole value is measured by [`BoundaryValue::sizes`].
    #[must_use]
    pub fn of(text: &str) -> Self {
        Self {
            graphemes: graphemes::count(text),
            code_points: text.chars().count(),
            utf16_units: text.encode_utf16().count(),
            bytes: text.len(),
        }
    }

    /// The one number the given unit asks about.
    #[must_use]
    pub const fn in_unit(self, unit: Unit) -> usize {
        match unit {
            Unit::Graphemes => self.graphemes,
            Unit::CodePoints => self.code_points,
            Unit::Utf16 => self.utf16_units,
            Unit::Bytes => self.bytes,
        }
    }

    /// The same text `times` over, with `padding` single-byte letters in front.
    ///
    /// Saturating, so that no recipe can make this arithmetic the place a run
    /// ends. Every recipe [`values`] gives is far below that.
    const fn repeated(self, times: usize, padding: usize) -> Self {
        const fn grown(one: usize, times: usize, padding: usize) -> usize {
            one.saturating_mul(times).saturating_add(padding)
        }
        Self {
            graphemes: grown(self.graphemes, times, padding),
            code_points: grown(self.code_points, times, padding),
            utf16_units: grown(self.utf16_units, times, padding),
            bytes: grown(self.bytes, times, padding),
        }
    }
}

/// What a value is filled with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fill {
    /// The letter `a`: one of everything, and accepted by a field that takes
    /// letters only.
    Letters,
    /// Numbers and stars that say where each star stands, so a field that cut
    /// the value shows where it cut.
    Counterstring,
    /// A letter of two bytes.
    TwoByte,
    /// A character of three bytes.
    ThreeByte,
    /// A character of four bytes, which is also two UTF-16 units.
    FourByte,
    /// One visible character made of five code points.
    Cluster,
}

/// The letter every value is padded with, and the fill of [`Fill::Letters`].
const LETTER: &str = "a";

impl Fill {
    /// Every fill, in the order values come out.
    pub const ALL: [Self; 6] = [
        Self::Letters,
        Self::Counterstring,
        Self::TwoByte,
        Self::ThreeByte,
        Self::FourByte,
        Self::Cluster,
    ];

    /// The name a document carries and a value identifier starts with. Public.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Letters => "letters",
            Self::Counterstring => "counterstring",
            Self::TwoByte => "two-byte",
            Self::ThreeByte => "three-byte",
            Self::FourByte => "four-byte",
            Self::Cluster => "cluster",
        }
    }

    /// The character this fill repeats. A counterstring repeats nothing.
    ///
    /// Each is one visible character, and they differ in what a machine counts:
    /// a small l with stroke, the euro sign, the grinning face and the family
    /// of man, woman and girl joined by zero width joiners.
    ///
    /// None of them changes under any Unicode normal form, and that is why the
    /// two-byte letter is not one with an accent: a field that normalises its
    /// text would change the length of such a value, and the value would stop
    /// measuring the limit alone.
    const fn character(self) -> Option<&'static str> {
        match self {
            Self::Letters => Some(LETTER),
            Self::Counterstring => None,
            Self::TwoByte => Some("\u{0142}"),
            Self::ThreeByte => Some("\u{20AC}"),
            Self::FourByte => Some("\u{1F600}"),
            Self::Cluster => Some("\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}"),
        }
    }

    /// The recipe for exactly `length` of `unit`, or `None` when not even one
    /// of this fill's characters fits - the value would be padding alone, which
    /// is the value [`Fill::Letters`] already gives.
    fn recipe(self, length: u32, unit: Unit) -> Option<Recipe> {
        let Some(character) = self.character() else {
            return Some(Recipe::Counterstring { length });
        };
        let weight = u32::try_from(Sizes::of(character).in_unit(unit)).ok()?;
        let count = length.checked_div(weight)?;
        (count > 0).then_some(Recipe::Repeated {
            padding: length % weight,
            character,
            count,
        })
    }
}

/// Why a limit cannot be answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimitProblem {
    /// Neither a least nor a most was given.
    NoBound,
    /// The least is outside 1 to [`MAX_LIMIT`].
    MinOutOfRange,
    /// The most is outside 1 to [`MAX_LIMIT`].
    MaxOutOfRange,
    /// The least is above the most.
    MinAboveMax,
}

/// The limit a field states: a least, a most or both, counted in one unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limit {
    unit: Unit,
    min: Option<u32>,
    max: Option<u32>,
}

impl Limit {
    /// A limit that can be answered.
    ///
    /// # Errors
    ///
    /// The [`LimitProblem`] that names what is wrong with the two numbers.
    pub fn new(unit: Unit, min: Option<u32>, max: Option<u32>) -> Result<Self, LimitProblem> {
        let in_range = |bound: u32| (1..=MAX_LIMIT).contains(&bound);
        if min.is_none() && max.is_none() {
            return Err(LimitProblem::NoBound);
        }
        if min.is_some_and(|bound| !in_range(bound)) {
            return Err(LimitProblem::MinOutOfRange);
        }
        if max.is_some_and(|bound| !in_range(bound)) {
            return Err(LimitProblem::MaxOutOfRange);
        }
        if let (Some(least), Some(most)) = (min, max)
            && least > most
        {
            return Err(LimitProblem::MinAboveMax);
        }
        Ok(Self { unit, min, max })
    }

    #[must_use]
    pub const fn unit(self) -> Unit {
        self.unit
    }

    #[must_use]
    pub const fn min(self) -> Option<u32> {
        self.min
    }

    #[must_use]
    pub const fn max(self) -> Option<u32> {
        self.max
    }

    /// Whether a value of this length is inside the limit - what the field is
    /// expected to accept, when it counts in the unit the limit names.
    #[must_use]
    pub fn holds(self, length: u32) -> bool {
        self.min.is_none_or(|least| length >= least) && self.max.is_none_or(|most| length <= most)
    }

    /// The lengths worth trying: nothing, and one under, at and one over each
    /// bound. Ascending, each once.
    fn lengths(self) -> Vec<u32> {
        let mut lengths = vec![0];
        for bound in [self.min, self.max].into_iter().flatten() {
            lengths.extend([bound - 1, bound, bound + 1]);
        }
        lengths.sort_unstable();
        lengths.dedup();
        lengths
    }
}

/// How a value is made, without the value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recipe {
    /// No text at all.
    Empty,
    /// `padding` letters `a`, then `character` repeated `count` times.
    Repeated {
        padding: u32,
        character: &'static str,
        count: u32,
    },
    /// The counterstring of this many characters.
    Counterstring { length: u32 },
}

/// One value of the answer to a limit.
///
/// Made by [`values`] and nowhere else, which is what keeps every recipe
/// under the ceiling of [`MAX_LIMIT`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoundaryValue {
    fill: Option<Fill>,
    length: u32,
    within: bool,
    recipe: Recipe,
}

impl BoundaryValue {
    /// What it is filled with. `None` for the empty value.
    #[must_use]
    pub const fn fill(&self) -> Option<Fill> {
        self.fill
    }

    /// How long it is, in the unit of the limit.
    #[must_use]
    pub const fn length(&self) -> u32 {
        self.length
    }

    /// Whether that length is inside the limit. Nothing more than that: a
    /// field that must be filled in still refuses the empty value.
    #[must_use]
    pub const fn within(&self) -> bool {
        self.within
    }

    #[must_use]
    pub const fn recipe(&self) -> Recipe {
        self.recipe
    }

    /// The identifier of this value among the values of one answer: the fill
    /// and the length, or `empty`. A public name.
    #[must_use]
    pub fn id(&self) -> String {
        match self.fill {
            Some(fill) => format!("{}-{}", fill.id(), self.length),
            None => "empty".to_owned(),
        }
    }

    /// The four sizes of this value, from the recipe alone.
    #[must_use]
    pub fn sizes(&self) -> Sizes {
        match self.recipe {
            Recipe::Empty => Sizes::of(""),
            Recipe::Repeated {
                padding,
                character,
                count,
            } => Sizes::of(character).repeated(count as usize, padding as usize),
            // Digits and stars: one of every unit each.
            Recipe::Counterstring { length } => Sizes::of(LETTER).repeated(length as usize, 0),
        }
    }

    /// The value as the body every other value of the tool has: what the
    /// palette holds, previews, types and reports.
    ///
    /// No padding is a plain repeat, because a body says each text one way.
    #[must_use]
    pub fn body(&self) -> ValueBody {
        match self.recipe {
            Recipe::Empty => ValueBody::Literal(LiteralText::new("")),
            Recipe::Repeated {
                padding: 0,
                character,
                count,
            } => ValueBody::Repeat {
                unit: LiteralText::new(character),
                count,
            },
            Recipe::Repeated {
                padding,
                character,
                count,
            } => ValueBody::Padded {
                padding: LiteralText::new(LETTER),
                padding_count: padding,
                unit: LiteralText::new(character),
                count,
            },
            Recipe::Counterstring { length } => ValueBody::Counterstring { length },
        }
    }

    /// Builds the text. The one step here that costs as much as the value is
    /// long, and never a way to measure it.
    #[must_use]
    pub fn materialise(&self) -> String {
        self.body().build()
    }
}

/// Every value that answers `limit`, in the order of testing: the empty value,
/// then fill by fill, each from its shortest to its longest.
#[must_use]
pub fn values(limit: Limit) -> Vec<BoundaryValue> {
    let lengths = limit.lengths();
    let mut values = vec![BoundaryValue {
        fill: None,
        length: 0,
        within: limit.holds(0),
        recipe: Recipe::Empty,
    }];
    for fill in Fill::ALL {
        for &length in lengths.iter().filter(|&&length| length > 0) {
            if let Some(recipe) = fill.recipe(length, limit.unit) {
                values.push(BoundaryValue {
                    fill: Some(fill),
                    length,
                    within: limit.holds(length),
                    recipe,
                });
            }
        }
    }
    values
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;

    fn at_most(unit: Unit, max: u32) -> Vec<BoundaryValue> {
        values(Limit::new(unit, None, Some(max)).expect("a limit in range"))
    }

    fn ids(values: &[BoundaryValue]) -> Vec<String> {
        values.iter().map(BoundaryValue::id).collect()
    }

    fn named<'a>(values: &'a [BoundaryValue], id: &str) -> &'a BoundaryValue {
        values
            .iter()
            .find(|value| value.id() == id)
            .unwrap_or_else(|| panic!("no value called {id}"))
    }

    #[test]
    fn every_unit_is_read_back_from_its_own_word() {
        for unit in Unit::ALL {
            assert_eq!(Unit::parse(unit.id()), Some(unit));
        }
        // The words are a public contract: an option takes them and a document
        // carries them.
        assert_eq!(
            Unit::ALL.map(Unit::id),
            ["graphemes", "code-points", "utf16", "bytes"]
        );
        assert_eq!(Unit::parse("chars"), None);
        assert_eq!(Unit::parse("Bytes"), None, "the words are lower case");
        assert_eq!(Unit::parse(""), None);
    }

    #[test]
    fn the_characters_weigh_what_the_module_says_they_weigh() {
        // Pinned, because every recipe is arithmetic on these sixteen numbers.
        let weights = |fill: Fill| {
            let sizes = Sizes::of(fill.character().expect("a fill with a character"));
            [
                sizes.graphemes,
                sizes.code_points,
                sizes.utf16_units,
                sizes.bytes,
            ]
        };
        assert_eq!(weights(Fill::Letters), [1, 1, 1, 1]);
        assert_eq!(weights(Fill::TwoByte), [1, 1, 1, 2]);
        assert_eq!(weights(Fill::ThreeByte), [1, 1, 1, 3]);
        assert_eq!(weights(Fill::FourByte), [1, 1, 2, 4]);
        assert_eq!(weights(Fill::Cluster), [1, 5, 8, 18]);
        assert_eq!(Fill::Counterstring.character(), None);
    }

    #[test]
    fn the_fills_are_the_characters_that_no_normal_form_changes() {
        // A field that normalises its text must still get the length asked
        // for. The tool's own check answers for the composed forms only, and
        // an accented letter passes it while the decomposed forms take it
        // apart - so the characters are pinned here, after being checked
        // against all four forms outside the tool.
        let characters: Vec<Option<&str>> = Fill::ALL.into_iter().map(Fill::character).collect();
        assert_eq!(
            characters,
            [
                Some("a"),
                None,
                Some("\u{0142}"),
                Some("\u{20AC}"),
                Some("\u{1F600}"),
                Some("\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}"),
            ]
        );
        for character in characters.into_iter().flatten() {
            assert!(
                !crate::normalization::may_change(character),
                "{character:?}"
            );
        }
    }

    #[test]
    fn a_limit_needs_a_bound_in_range_and_a_least_not_above_the_most() {
        let limit = |min, max| Limit::new(Unit::Graphemes, min, max);
        assert_eq!(limit(None, None), Err(LimitProblem::NoBound));
        assert_eq!(limit(Some(0), None), Err(LimitProblem::MinOutOfRange));
        assert_eq!(limit(None, Some(0)), Err(LimitProblem::MaxOutOfRange));
        assert_eq!(
            limit(None, Some(MAX_LIMIT + 1)),
            Err(LimitProblem::MaxOutOfRange)
        );
        assert_eq!(
            limit(Some(MAX_LIMIT + 1), None),
            Err(LimitProblem::MinOutOfRange)
        );
        assert_eq!(limit(Some(51), Some(50)), Err(LimitProblem::MinAboveMax));
        assert!(limit(Some(50), Some(50)).is_ok(), "exactly 50 is a limit");
        assert!(limit(Some(1), Some(MAX_LIMIT)).is_ok());
        assert!(limit(Some(8), None).is_ok());
    }

    #[test]
    fn at_most_fifty_is_nothing_then_six_fills_of_three_lengths() {
        let values = at_most(Unit::Graphemes, 50);
        assert_eq!(
            ids(&values),
            [
                "empty",
                "letters-49",
                "letters-50",
                "letters-51",
                "counterstring-49",
                "counterstring-50",
                "counterstring-51",
                "two-byte-49",
                "two-byte-50",
                "two-byte-51",
                "three-byte-49",
                "three-byte-50",
                "three-byte-51",
                "four-byte-49",
                "four-byte-50",
                "four-byte-51",
                "cluster-49",
                "cluster-50",
                "cluster-51",
            ]
        );
    }

    #[test]
    fn only_the_length_over_the_most_is_outside() {
        let values = at_most(Unit::Graphemes, 50);
        for value in &values {
            assert_eq!(value.within, value.length <= 50, "{}", value.id());
        }
        assert!(named(&values, "empty").within, "nothing is at most 50");
    }

    #[test]
    fn a_least_puts_the_empty_value_and_the_one_under_outside() {
        let limit = Limit::new(Unit::Graphemes, Some(3), Some(50)).expect("in range");
        let values = values(limit);
        let lengths: Vec<u32> = values
            .iter()
            .filter(|value| value.fill == Some(Fill::Letters))
            .map(|value| value.length)
            .collect();
        assert_eq!(lengths, [2, 3, 4, 49, 50, 51]);
        assert!(!named(&values, "empty").within);
        assert!(!named(&values, "letters-2").within);
        assert!(named(&values, "letters-3").within);
        assert!(named(&values, "letters-50").within);
        assert!(!named(&values, "letters-51").within);
    }

    #[test]
    fn bounds_that_touch_give_each_length_once() {
        // At most 1: one under is nothing, which the empty value already is.
        let one = at_most(Unit::Graphemes, 1);
        assert_eq!(
            ids(&one)[..3],
            ["empty", "letters-1", "letters-2"],
            "no letters-0"
        );
        // Exactly 4, and 3 to 4: the lengths of the two bounds overlap.
        let exactly = values(Limit::new(Unit::Bytes, Some(4), Some(4)).expect("in range"));
        let letters = |values: &[BoundaryValue]| -> Vec<u32> {
            values
                .iter()
                .filter(|value| value.fill == Some(Fill::Letters))
                .map(|value| value.length)
                .collect()
        };
        assert_eq!(letters(&exactly), [3, 4, 5]);
        let close = values(Limit::new(Unit::Bytes, Some(3), Some(4)).expect("in range"));
        assert_eq!(letters(&close), [2, 3, 4, 5]);
    }

    #[test]
    fn fifty_of_a_two_byte_letter_is_a_hundred_bytes() {
        // The sentence the feature was asked for with.
        let values = at_most(Unit::Graphemes, 50);
        let sizes = named(&values, "two-byte-50").sizes();
        assert_eq!(
            sizes,
            Sizes {
                graphemes: 50,
                code_points: 50,
                utf16_units: 50,
                bytes: 100
            }
        );
        let emoji = named(&values, "four-byte-50").sizes();
        assert_eq!((emoji.utf16_units, emoji.bytes), (100, 200));
        let family = named(&values, "cluster-50").sizes();
        assert_eq!(
            (family.code_points, family.utf16_units, family.bytes),
            (250, 400, 900)
        );
    }

    #[test]
    fn a_byte_limit_falls_inside_the_last_character_of_the_value_over_it() {
        let values = at_most(Unit::Bytes, 50);
        let over = named(&values, "two-byte-51");
        assert_eq!(
            over.recipe,
            Recipe::Repeated {
                padding: 1,
                character: "\u{0142}",
                count: 25
            }
        );
        let text = over.materialise();
        assert_eq!(text.len(), 51);
        assert!(text.starts_with('a'), "the padding stands in front");
        assert!(
            !text.is_char_boundary(50),
            "a cut at 50 bytes goes through the last letter"
        );
        // The value at the limit is whole characters and nothing else.
        assert_eq!(
            named(&values, "two-byte-50").recipe,
            Recipe::Repeated {
                padding: 0,
                character: "\u{0142}",
                count: 25
            }
        );
        assert!(
            !named(&values, "cluster-51")
                .materialise()
                .is_char_boundary(50)
        );
    }

    #[test]
    fn a_utf16_limit_falls_between_the_two_halves_of_a_pair() {
        let values = at_most(Unit::Utf16, 50);
        let text = named(&values, "four-byte-51").materialise();
        let units: Vec<u16> = text.encode_utf16().collect();
        assert_eq!(units.len(), 51);
        assert!(
            String::from_utf16(&units[..50]).is_err(),
            "the first 50 units end in half a pair"
        );
        assert!(String::from_utf16(&units[..49]).is_ok());
    }

    #[test]
    fn a_fill_too_heavy_for_the_length_is_left_out_rather_than_padded_whole() {
        // 18 bytes a family: at most 2 bytes holds none of it, and a value of
        // padding alone would be the letters value twice.
        let values = at_most(Unit::Bytes, 2);
        assert_eq!(
            ids(&values),
            [
                "empty",
                "letters-1",
                "letters-2",
                "letters-3",
                "counterstring-1",
                "counterstring-2",
                "counterstring-3",
                "two-byte-2",
                "two-byte-3",
                "three-byte-3",
            ]
        );
    }

    #[test]
    fn every_value_is_as_long_as_it_says_in_every_unit_and_no_text_comes_twice() {
        // The recipe is trusted everywhere else, so here the text is built and
        // counted by reading it - with the grapheme counter the palette uses.
        let limits = [
            (None, Some(1)),
            (Some(1), None),
            (Some(3), Some(20)),
            (None, Some(50)),
        ];
        for unit in Unit::ALL {
            for (min, max) in limits {
                let limit = Limit::new(unit, min, max).expect("in range");
                let mut seen = std::collections::BTreeSet::new();
                for value in values(limit) {
                    let text = value.materialise();
                    let read = Sizes::of(&text);
                    assert_eq!(value.sizes(), read, "{} in {}", value.id(), unit.id());
                    assert_eq!(
                        read.in_unit(unit),
                        value.length as usize,
                        "{} in {}",
                        value.id(),
                        unit.id()
                    );
                    // The body the palette holds says the same text and the
                    // same size, and passes the rules every value passes.
                    let body = value.body();
                    assert_eq!(body.materialise().as_deref(), Ok(text.as_str()));
                    let promised = body.metrics().expect("a measurable recipe");
                    assert_eq!(
                        (promised.code_points, promised.bytes),
                        (read.code_points, read.bytes),
                        "{} in {}",
                        value.id(),
                        unit.id()
                    );
                    assert!(seen.insert(text), "{} repeats a text", value.id());
                }
            }
        }
    }

    #[test]
    fn the_largest_limit_is_measured_without_being_built_and_stays_under_the_ceiling() {
        for unit in Unit::ALL {
            let limit = Limit::new(unit, Some(1), Some(MAX_LIMIT)).expect("in range");
            for value in values(limit) {
                assert!(
                    value.sizes().code_points <= crate::value::MAX_VALUE_CODE_POINTS,
                    "{} in {}",
                    value.id(),
                    unit.id()
                );
                // What lets `materialise` build without asking: the rules of
                // a value hold for every body this module makes.
                assert_eq!(value.body().check_size(), Ok(()), "{}", value.id());
            }
        }
    }
}
