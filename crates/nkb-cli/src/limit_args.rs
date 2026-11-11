//! The limit of a field as a command reads it from its arguments:
//! `--max-length N`, `--min-length N` and `--unit U` (`D177`, `D178`).
//!
//! One reader for `nkb boundary` and `nkb send`, so the two commands cannot
//! take one call two ways: the same numbers are refused, with the same
//! sentences, and the same unit is the default.

use nkb_core::boundary::{Limit, LimitProblem, MAX_LIMIT, Unit};

/// Two lines on standard error: what is wrong, and what to do about it.
pub(crate) type Refusal = [String; 2];

/// An option that gives a part of a limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LimitOption {
    Max,
    Min,
    Unit,
}

impl LimitOption {
    pub(crate) fn of(option: &str) -> Option<Self> {
        match option {
            "--max-length" => Some(Self::Max),
            "--min-length" => Some(Self::Min),
            "--unit" => Some(Self::Unit),
            _ => None,
        }
    }

    pub(crate) const fn option(self) -> &'static str {
        match self {
            Self::Max => "--max-length",
            Self::Min => "--min-length",
            Self::Unit => "--unit",
        }
    }
}

/// The parts of a limit the arguments gave, as far as they gave them.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LimitWishes {
    max: Option<u32>,
    min: Option<u32>,
    unit: Option<Unit>,
}

/// Where to look when a call is wrong.
pub(crate) fn see_help(command: &str) -> String {
    format!("Run 'nkb {command} --help' to see what is available.")
}

fn units() -> String {
    let words: Vec<&str> = Unit::ALL.into_iter().map(Unit::id).collect();
    format!("Available: {}.", words.join(", "))
}

/// A whole number from 1 to the largest limit, written in plain digits.
///
/// Digits only, checked before the number is read: the standard reading of a
/// number also takes a plus sign, and `+50` is a typing mistake here rather
/// than a limit.
fn bound(word: &str) -> Option<u32> {
    if word.is_empty() || !word.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    word.parse()
        .ok()
        .filter(|number| (1..=MAX_LIMIT).contains(number))
}

fn out_of_range(option: &str) -> String {
    format!("{option} needs a whole number from 1 to {MAX_LIMIT}")
}

fn a_bound(option: &str, word: Option<&str>, command: &str) -> Result<u32, Refusal> {
    let Some(word) = word else {
        return Err([format!("{}.", out_of_range(option)), see_help(command)]);
    };
    bound(word).ok_or_else(|| {
        [
            format!("{}, got '{word}'.", out_of_range(option)),
            see_help(command),
        ]
    })
}

impl LimitWishes {
    /// Takes the word after `option`. An option given twice is refused rather
    /// than settled quietly, because the two numbers of a limit are easy to
    /// mistype for each other.
    pub(crate) fn take(
        &mut self,
        option: LimitOption,
        word: Option<&str>,
        command: &str,
    ) -> Result<(), Refusal> {
        let typed = option.option();
        let taken = match option {
            LimitOption::Max => self.max.is_some(),
            LimitOption::Min => self.min.is_some(),
            LimitOption::Unit => self.unit.is_some(),
        };
        if taken {
            return Err([format!("{typed} was given twice."), see_help(command)]);
        }
        match option {
            LimitOption::Max => self.max = Some(a_bound(typed, word, command)?),
            LimitOption::Min => self.min = Some(a_bound(typed, word, command)?),
            LimitOption::Unit => {
                self.unit = Some(word.and_then(Unit::parse).ok_or_else(|| {
                    let said = word.map_or_else(
                        || format!("{typed} needs a name."),
                        |word| format!("unknown unit '{word}'"),
                    );
                    [said, units()]
                })?);
            }
        }
        Ok(())
    }

    /// Whether any part of a limit was given.
    pub(crate) const fn any(self) -> bool {
        self.max.is_some() || self.min.is_some() || self.unit.is_some()
    }

    /// The limit these parts come to, counted in graphemes unless a unit was
    /// given.
    pub(crate) fn settled(self, command: &str) -> Result<Limit, Refusal> {
        Limit::new(self.unit.unwrap_or(Unit::Graphemes), self.min, self.max).map_err(|problem| {
            match problem {
                LimitProblem::NoBound => [
                    "give the limit of the field: --max-length, --min-length or both.".to_owned(),
                    see_help(command),
                ],
                LimitProblem::MinAboveMax => [
                    format!(
                        "--min-length {} is above --max-length {}.",
                        self.min.unwrap_or_default(),
                        self.max.unwrap_or_default()
                    ),
                    "The least a field takes cannot be more than the most.".to_owned(),
                ],
                // `bound` lets no such number through. Said all the same, so
                // that the two checks drifting apart is a sentence, not a crash.
                LimitProblem::MinOutOfRange | LimitProblem::MaxOutOfRange => {
                    [out_of_range("a limit"), see_help(command)]
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_number_is_plain_digits_from_one_to_the_largest_limit() {
        assert_eq!(bound("1"), Some(1));
        assert_eq!(bound("050"), Some(50), "leading zeros are still digits");
        assert_eq!(bound("100000"), Some(MAX_LIMIT));
        for word in [
            "",
            "0",
            "100001",
            "+50",
            "-5",
            "5.0",
            " 5",
            "5 ",
            "0x32",
            "1e3",
            "fifty",
            "99999999999999999999",
            "\u{0665}\u{0660}",
        ] {
            assert_eq!(bound(word), None, "{word:?}");
        }
    }

    #[test]
    fn nothing_given_is_no_limit_and_a_unit_alone_is_part_of_one() {
        let mut wishes = LimitWishes::default();
        assert!(!wishes.any());
        assert_eq!(
            wishes.take(LimitOption::Unit, Some("bytes"), "send"),
            Ok(())
        );
        assert!(wishes.any());
        assert_eq!(
            wishes.settled("send"),
            Err([
                "give the limit of the field: --max-length, --min-length or both.".to_owned(),
                "Run 'nkb send --help' to see what is available.".to_owned(),
            ])
        );
    }

    #[test]
    fn every_option_is_known_by_the_word_it_is_typed_as() {
        for option in [LimitOption::Max, LimitOption::Min, LimitOption::Unit] {
            assert_eq!(LimitOption::of(option.option()), Some(option));
        }
        assert_eq!(LimitOption::of("--max"), None);
        assert_eq!(LimitOption::of("--format"), None);
    }
}
