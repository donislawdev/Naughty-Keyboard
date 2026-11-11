//! The length limit window's form, with no window: two numbers typed a digit
//! at a time, a unit, and the values they give (`D178`).
//!
//! # Why this is not in the view
//!
//! What a digit does to a number, which pair of numbers is a limit and what the
//! list holds are decisions, and a view holds layout and bindings only (GUI
//! rules 11 and 15). Here they are tested without a window.
//!
//! # A number without a text field
//!
//! The window has no text field, for the reason the search has none
//! (`OBS-145`, `D85`): the toolkit's own copies and pastes by itself, and
//! untouchable rule 17 names every place the clipboard is used. So a number is
//! built from key presses, like the query: a digit is added, Backspace takes
//! the last one away, and nothing else is the number's. What that costs is
//! said here once - a limit cannot be pasted in.
//!
//! # Nothing is built here
//!
//! The values are the recipes [`nkb_core::boundary`] gives, and their sizes are
//! counted from the recipes. The largest limit describes values of more than a
//! megabyte, and every digit typed makes the list again - so nothing here may
//! cost what building one of them would.

use nkb_adapters::boundary_text::value_name;
use nkb_adapters::i18n;
use nkb_core::boundary::{self, BoundaryValue, Limit, LimitProblem, MAX_LIMIT, Unit};

use crate::query::{BACKSPACE, KeyPress, Pressed, is_shortcut};

/// Which of the two numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bound {
    /// The most the field takes.
    Most,
    /// The least it takes.
    Least,
}

/// How many digits a number may have: as many as the largest limit. A seventh
/// could only make a number that is refused, and one that no longer fits the
/// type it is read into is a failure nobody needs to be able to type.
const MAX_DIGITS: usize = MAX_LIMIT.ilog10() as usize + 1;

/// One number, as its digits.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Number {
    digits: String,
}

impl Number {
    fn of(number: Option<u32>) -> Self {
        Self {
            digits: number.map_or_else(String::new, |number| number.to_string()),
        }
    }

    /// The number typed, or `None` while nothing is. Every string of at most
    /// [`MAX_DIGITS`] digits is a number, so nothing typed can fail to be one.
    fn value(&self) -> Option<u32> {
        self.digits.parse().ok()
    }

    /// Applies one key press: a digit, or Backspace. A lone zero gives way to
    /// the digit typed after it, so a number never starts with one.
    fn press(&mut self, press: KeyPress<'_>) -> Pressed {
        if press.text == BACKSPACE {
            return if self.digits.pop().is_some() {
                Pressed::Changed
            } else {
                Pressed::Unchanged
            };
        }
        let mut typed = press.text.chars();
        let (Some(digit), None) = (typed.next(), typed.next()) else {
            return Pressed::NotOurs;
        };
        if is_shortcut(press) || !digit.is_ascii_digit() {
            return Pressed::NotOurs;
        }
        if self.digits == "0" {
            self.digits.clear();
        }
        if self.digits.len() >= MAX_DIGITS {
            // The number's all the same: a digit that does not fit is not
            // handed on to mean something else.
            return Pressed::Unchanged;
        }
        self.digits.push(digit);
        Pressed::Changed
    }
}

/// What the window opens with: the limit of the values in use, the number the
/// tester typed in the value window's search, or nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Opening {
    pub most: Option<u32>,
    pub least: Option<u32>,
    pub unit: Unit,
}

impl Opening {
    /// Nothing typed, counted the way a person counts.
    pub const EMPTY: Self = Self {
        most: None,
        least: None,
        unit: Unit::Graphemes,
    };

    /// The limit some values were made for, to change from there.
    #[must_use]
    pub const fn of(limit: Limit) -> Self {
        Self {
            most: limit.max(),
            least: limit.min(),
            unit: limit.unit(),
        }
    }

    /// A most, as the value window's search suggests one.
    #[must_use]
    pub const fn at_most(most: u32) -> Self {
        Self {
            most: Some(most),
            ..Self::EMPTY
        }
    }
}

/// One value of the list, as finished words and one fact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValueLine {
    /// What the value is called - in a ticket too.
    pub name: String,
    /// How long it is, counted the way the limit is.
    pub length: String,
    /// Which side of the limit that length is on, in words.
    pub side: String,
}

/// What the window's main action hands the palette.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Use {
    pub limit: Limit,
    /// The value to start from, by identifier - `None` for the first, where a
    /// set starts anyway.
    pub start: Option<String>,
}

/// The two numbers, the unit, and what they give.
#[derive(Debug, Clone)]
pub struct LimitForm {
    most: Number,
    least: Number,
    unit: Unit,
    /// The values of the limit the numbers make now - or why they make none.
    made: Result<(Limit, Vec<BoundaryValue>), LimitProblem>,
    /// The row the walk starts from. Always a row of the list.
    selected: usize,
}

impl LimitForm {
    #[must_use]
    pub fn new(opening: Opening) -> Self {
        let mut form = Self {
            most: Number::of(opening.most),
            least: Number::of(opening.least),
            unit: opening.unit,
            made: Err(LimitProblem::NoBound),
            selected: 0,
        };
        form.make();
        form
    }

    /// Makes the list again from the numbers and the unit, and starts it from
    /// its first value: a row of another list is not this one's.
    fn make(&mut self) {
        self.made = Limit::new(self.unit, self.least.value(), self.most.value())
            .map(|limit| (limit, boundary::values(limit)));
        self.selected = 0;
    }

    /// Applies one key press to the number `bound`. The answer says whether the
    /// press was the number's.
    pub fn press(&mut self, bound: Bound, press: KeyPress<'_>) -> Pressed {
        let number = match bound {
            Bound::Most => &mut self.most,
            Bound::Least => &mut self.least,
        };
        let pressed = number.press(press);
        if pressed == Pressed::Changed {
            self.make();
        }
        pressed
    }

    /// Counts in the unit at `index` of [`Unit::ALL`]. `false` when that is the
    /// unit already, or no unit.
    pub fn set_unit(&mut self, index: usize) -> bool {
        match Unit::ALL.get(index) {
            Some(&unit) if unit != self.unit => {
                self.unit = unit;
                self.make();
                true
            }
            _ => false,
        }
    }

    /// The unit one step along the switch. Stays at either end rather than
    /// wrapping, as the list does.
    pub fn step_unit(&mut self, forward: bool) -> bool {
        let at = self.unit_index();
        let to = if forward {
            at.saturating_add(1)
        } else {
            at.saturating_sub(1)
        };
        self.set_unit(to)
    }

    /// Moves the start one value down. Stays on the last.
    pub fn next(&mut self) {
        if self.selected + 1 < self.values().len() {
            self.selected += 1;
        }
    }

    /// Moves the start one value up. Stays on the first.
    pub fn previous(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    /// Starts from row `row` of the list - a click on it. A row the list does
    /// not have changes nothing.
    pub fn select(&mut self, row: usize) {
        if row < self.values().len() {
            self.selected = row;
        }
    }

    /// The digits of `bound` as typed, for its line to draw.
    #[must_use]
    pub fn digits(&self, bound: Bound) -> &str {
        match bound {
            Bound::Most => &self.most.digits,
            Bound::Least => &self.least.digits,
        }
    }

    /// Whether the number `bound` is the one that stands in the way of a list.
    /// Nothing typed at all is no fault of either.
    #[must_use]
    pub fn at_fault(&self, bound: Bound) -> bool {
        matches!(
            (bound, &self.made),
            (Bound::Most, Err(LimitProblem::MaxOutOfRange))
                | (
                    Bound::Least,
                    Err(LimitProblem::MinOutOfRange | LimitProblem::MinAboveMax)
                )
        )
    }

    /// Where the unit stands in [`Unit::ALL`], the order of the switch.
    #[must_use]
    pub fn unit_index(&self) -> usize {
        Unit::ALL
            .iter()
            .position(|&unit| unit == self.unit)
            .unwrap_or(0)
    }

    fn values(&self) -> &[BoundaryValue] {
        self.made.as_ref().map_or(&[], |(_, values)| values)
    }

    /// The list: every value with its name, its length and its side.
    #[must_use]
    pub fn lines(&self) -> Vec<ValueLine> {
        self.values()
            .iter()
            .map(|value| ValueLine {
                name: value_name(value),
                length: i18n::limit_length(self.unit, value.length()),
                side: i18n::limit_side(value.within()).to_owned(),
            })
            .collect()
    }

    /// The row the walk starts from, or `None` while the list is empty.
    #[must_use]
    pub fn selected(&self) -> Option<usize> {
        (!self.values().is_empty()).then_some(self.selected)
    }

    /// How much the values come to together, in the words of `nkb boundary`'s
    /// own account. Empty while there are none.
    #[must_use]
    pub fn account(&self) -> String {
        let values = self.values();
        if values.is_empty() {
            return String::new();
        }
        let (code_points, bytes) = values.iter().fold((0usize, 0usize), |sum, value| {
            let sizes = value.sizes();
            (
                sum.0.saturating_add(sizes.code_points),
                sum.1.saturating_add(sizes.bytes),
            )
        });
        i18n::limit_account(values.len(), code_points, bytes)
    }

    /// What the list says in place of its rows. Empty while it has some.
    #[must_use]
    pub fn empty_text(&self) -> String {
        match self.made {
            Ok(_) => String::new(),
            Err(problem) => i18n::limit_problem(problem),
        }
    }

    /// What the main action hands the palette, or `None` while the numbers
    /// make no limit - the action is off then.
    #[must_use]
    pub fn chosen(&self) -> Option<Use> {
        let (limit, values) = self.made.as_ref().ok()?;
        let start = (self.selected > 0)
            .then(|| values.get(self.selected).map(BoundaryValue::id))
            .flatten();
        Some(Use {
            limit: *limit,
            start,
        })
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;

    fn key(text: &str) -> KeyPress<'_> {
        KeyPress {
            text,
            control: false,
            alt: false,
            meta: false,
        }
    }

    fn type_in(form: &mut LimitForm, bound: Bound, text: &str) {
        let mut buffer = [0u8; 4];
        for character in text.chars() {
            form.press(bound, key(character.encode_utf8(&mut buffer)));
        }
    }

    fn typed(most: &str, least: &str, unit: Unit) -> LimitForm {
        let mut form = LimitForm::new(Opening {
            unit,
            ..Opening::EMPTY
        });
        type_in(&mut form, Bound::Most, most);
        type_in(&mut form, Bound::Least, least);
        form
    }

    #[test]
    fn nothing_typed_is_an_invitation_and_no_fault() {
        let form = LimitForm::new(Opening::EMPTY);
        assert!(form.lines().is_empty());
        assert_eq!(form.selected(), None);
        assert_eq!(form.chosen(), None);
        assert_eq!(form.account(), "");
        assert_eq!(
            form.empty_text(),
            i18n::limit_problem(LimitProblem::NoBound),
            "nothing typed is an invitation"
        );
        assert!(!form.at_fault(Bound::Most) && !form.at_fault(Bound::Least));
        assert_eq!(form.unit_index(), 0, "it opens counting graphemes");
    }

    /// The list is `nkb boundary`'s own: the same values in the same order
    /// under the same names, with the same account.
    #[test]
    fn digits_make_the_values_the_command_makes() {
        let form = typed("50", "", Unit::Bytes);
        let limit = Limit::new(Unit::Bytes, None, Some(50)).expect("a limit");
        let values = boundary::values(limit);
        let lines = form.lines();
        assert_eq!(lines.len(), values.len());
        for (line, value) in lines.iter().zip(&values) {
            assert_eq!(line.name, value_name(value));
        }
        assert_eq!(
            lines[0],
            ValueLine {
                name: "Empty value".to_owned(),
                length: "bytes: 0".to_owned(),
                side: "inside the limit".to_owned(),
            }
        );
        let over = lines
            .iter()
            .find(|line| line.name == "51 x a")
            .expect("one letter over the limit");
        assert_eq!(over.length, "bytes: 51");
        assert_eq!(over.side, "outside the limit");
        assert_eq!(form.empty_text(), "");
        // The numbers `nkb boundary --max-length 50 --unit bytes` prints.
        assert_eq!(
            form.account(),
            "values: 19 \u{b7} code points: 542 \u{b7} bytes: 900"
        );
        assert_eq!(form.chosen(), Some(Use { limit, start: None }));
    }

    #[test]
    fn a_least_alone_and_both_together_are_limits_too() {
        let least = typed("", "8", Unit::Graphemes);
        assert_eq!(
            least.chosen().map(|chosen| chosen.limit),
            Limit::new(Unit::Graphemes, Some(8), None).ok()
        );
        let both = typed("50", "8", Unit::CodePoints);
        assert_eq!(
            both.chosen().map(|chosen| chosen.limit),
            Limit::new(Unit::CodePoints, Some(8), Some(50)).ok()
        );
        assert!(both.lines().len() > least.lines().len());
    }

    #[test]
    fn only_digits_and_backspace_are_the_number_s() {
        let mut form = LimitForm::new(Opening::EMPTY);
        for other in [
            "a", "-", ".", " ", "\t", "\n", "\u{1b}", "\u{F700}", "", "12",
        ] {
            assert_eq!(
                form.press(Bound::Most, key(other)),
                Pressed::NotOurs,
                "{other:?}"
            );
        }
        // An Arabic-Indic five is a digit to Unicode and not one of these.
        assert_eq!(form.press(Bound::Most, key("\u{0665}")), Pressed::NotOurs);
        let with_control = KeyPress {
            control: true,
            ..key("5")
        };
        assert_eq!(form.press(Bound::Most, with_control), Pressed::NotOurs);
        assert_eq!(form.digits(Bound::Most), "");

        assert_eq!(form.press(Bound::Most, key("5")), Pressed::Changed);
        assert_eq!(form.press(Bound::Most, key("0")), Pressed::Changed);
        assert_eq!(form.digits(Bound::Most), "50");
        assert_eq!(form.digits(Bound::Least), "", "the other number moved");
        assert_eq!(form.press(Bound::Most, key(BACKSPACE)), Pressed::Changed);
        assert_eq!(form.digits(Bound::Most), "5");
        assert_eq!(form.press(Bound::Most, key(BACKSPACE)), Pressed::Changed);
        assert_eq!(form.press(Bound::Most, key(BACKSPACE)), Pressed::Unchanged);
        assert_eq!(
            form.empty_text(),
            i18n::limit_problem(LimitProblem::NoBound)
        );
    }

    #[test]
    fn a_number_never_starts_with_a_zero_and_holds_six_digits() {
        let mut form = typed("0", "", Unit::Graphemes);
        assert_eq!(form.digits(Bound::Most), "0");
        assert!(form.at_fault(Bound::Most), "zero is no limit");
        type_in(&mut form, Bound::Most, "07");
        assert_eq!(form.digits(Bound::Most), "7");
        assert!(!form.at_fault(Bound::Most));

        let mut form = typed("100000", "", Unit::Graphemes);
        assert!(form.chosen().is_some(), "the largest limit is one");
        assert_eq!(form.press(Bound::Most, key("0")), Pressed::Unchanged);
        assert_eq!(form.digits(Bound::Most), "100000");
        assert_eq!(MAX_DIGITS, 6);
    }

    #[test]
    fn the_number_at_fault_is_named_and_the_action_is_off() {
        let over = typed("100001", "", Unit::Graphemes);
        assert!(over.at_fault(Bound::Most) && !over.at_fault(Bound::Least));
        assert_eq!(over.chosen(), None);
        assert_eq!(
            over.empty_text(),
            "The most a field takes is a number from 1 to 100000."
        );

        let least_over = typed("", "999999", Unit::Graphemes);
        assert!(least_over.at_fault(Bound::Least) && !least_over.at_fault(Bound::Most));
        assert_eq!(
            least_over.empty_text(),
            "The least a field takes is a number from 1 to 100000."
        );

        let crossed = typed("8", "50", Unit::Graphemes);
        assert!(crossed.at_fault(Bound::Least) && !crossed.at_fault(Bound::Most));
        assert_eq!(crossed.chosen(), None);
        assert_eq!(
            crossed.empty_text(),
            "The least is more than the most, so no length fits both."
        );
        assert!(crossed.lines().is_empty());
        assert_eq!(crossed.account(), "");
    }

    #[test]
    fn the_unit_steps_along_the_switch_and_stops_at_its_ends() {
        let mut form = typed("50", "", Unit::Graphemes);
        assert!(!form.step_unit(false), "stepped before the first unit");
        assert!(!form.set_unit(0), "the unit in effect is no change");
        assert!(form.step_unit(true));
        assert_eq!(form.unit_index(), 1);
        assert_eq!(form.lines()[1].length, "code points: 49");
        assert!(form.set_unit(3));
        assert!(!form.step_unit(true), "stepped past the last unit");
        assert!(!form.set_unit(4), "there is no fifth unit");
        assert_eq!(
            form.chosen().map(|chosen| chosen.limit.unit()),
            Some(Unit::Bytes)
        );
    }

    #[test]
    fn the_start_moves_within_the_list_and_goes_back_to_the_top_with_a_new_one() {
        let mut form = typed("50", "", Unit::Graphemes);
        let count = form.lines().len();
        assert_eq!(form.selected(), Some(0));
        form.previous();
        assert_eq!(form.selected(), Some(0), "moved above the first");
        form.next();
        form.next();
        assert_eq!(form.selected(), Some(2));
        let limit = Limit::new(Unit::Graphemes, None, Some(50)).expect("a limit");
        assert_eq!(
            form.chosen(),
            Some(Use {
                limit,
                start: Some(boundary::values(limit)[2].id()),
            })
        );
        form.select(count);
        assert_eq!(form.selected(), Some(2), "a row the list does not have");
        form.select(count - 1);
        form.next();
        assert_eq!(form.selected(), Some(count - 1), "moved below the last");

        type_in(&mut form, Bound::Most, "0");
        assert_eq!(form.selected(), Some(0), "the start outlived its list");
        form.next();
        form.set_unit(3);
        assert_eq!(form.selected(), Some(0));
    }

    #[test]
    fn it_opens_on_the_limit_in_use_or_on_the_number_from_the_search() {
        let limit = Limit::new(Unit::Utf16, Some(8), Some(50)).expect("a limit");
        let form = LimitForm::new(Opening::of(limit));
        assert_eq!(form.digits(Bound::Most), "50");
        assert_eq!(form.digits(Bound::Least), "8");
        assert_eq!(form.unit_index(), 2);
        assert_eq!(form.chosen(), Some(Use { limit, start: None }));

        let suggested = LimitForm::new(Opening::at_most(255));
        assert_eq!(suggested.digits(Bound::Most), "255");
        assert_eq!(suggested.digits(Bound::Least), "");
        assert_eq!(
            suggested.chosen().map(|chosen| chosen.limit),
            Limit::new(Unit::Graphemes, None, Some(255)).ok()
        );
    }

    /// The largest limit in the unit that makes the largest values: the list
    /// is made from recipes, so this is arithmetic and not megabytes of text.
    #[test]
    fn the_largest_limit_is_listed_without_building_a_value() {
        let started = std::time::Instant::now();
        let form = typed("100000", "1", Unit::Graphemes);
        assert!(!form.lines().is_empty());
        assert!(form.account().starts_with("values: "));
        assert!(
            started.elapsed() < std::time::Duration::from_secs(1),
            "took {:?}",
            started.elapsed()
        );
    }
}
