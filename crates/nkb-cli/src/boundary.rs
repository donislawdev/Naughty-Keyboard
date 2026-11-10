//! `nkb boundary` - the values that test the limit a field states.
//!
//! The one command that reads no pack: its values are made from two numbers
//! and a unit (`nkb_core::boundary`). It touches no file and no clock, so it
//! calls the core directly, and the only ways it can end are a success, a
//! wrong call and an answer that could not be written.

mod output;

use crate::emit_output::{EmitFormat, Form};
use crate::english::listed;
use crate::exit::ExitCode;
use crate::help::print_boundary_help;
use crate::output::{is_open, out};
use nkb_core::boundary::{Limit, LimitProblem, MAX_LIMIT, Unit, values};
use output::Shape;
use std::io::Write;

/// An option of this command that takes a word after it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Awaited {
    Max,
    Min,
    Unit,
    Format,
}

impl Awaited {
    fn of(option: &str) -> Option<Self> {
        match option {
            "--max-length" => Some(Self::Max),
            "--min-length" => Some(Self::Min),
            "--unit" => Some(Self::Unit),
            "--format" => Some(Self::Format),
            _ => None,
        }
    }

    const fn option(self) -> &'static str {
        match self {
            Self::Max => "--max-length",
            Self::Min => "--min-length",
            Self::Unit => "--unit",
            Self::Format => "--format",
        }
    }
}

/// What was asked for, as far as the arguments said.
#[derive(Debug, Default, PartialEq, Eq)]
struct Wishes {
    max: Option<u32>,
    min: Option<u32>,
    unit: Option<Unit>,
    format: Option<EmitFormat>,
    form: Option<Form>,
    base64: bool,
}

/// What the arguments came to.
#[derive(Debug, PartialEq, Eq)]
enum Asked {
    Help,
    Values(Limit, Shape),
}

/// Two lines on standard error: what is wrong, and what to do about it.
type Refusal = [String; 2];

fn see_help() -> String {
    "Run 'nkb boundary --help' to see what is available.".to_owned()
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

impl Wishes {
    /// Takes the word after an option. An option given twice is refused
    /// rather than settled quietly, because the two numbers of a limit are
    /// easy to mistype for each other.
    fn take(&mut self, awaited: Awaited, word: Option<&str>) -> Result<(), Refusal> {
        let option = awaited.option();
        let taken = match awaited {
            Awaited::Max => self.max.is_some(),
            Awaited::Min => self.min.is_some(),
            Awaited::Unit => self.unit.is_some(),
            Awaited::Format => self.format.is_some(),
        };
        if taken {
            return Err([format!("{option} was given twice."), see_help()]);
        }
        match awaited {
            Awaited::Max => self.max = Some(a_bound(option, word)?),
            Awaited::Min => self.min = Some(a_bound(option, word)?),
            Awaited::Unit => {
                self.unit = Some(word.and_then(Unit::parse).ok_or_else(|| {
                    let said = word.map_or_else(
                        || format!("{option} needs a name."),
                        |word| format!("unknown unit '{word}'"),
                    );
                    [said, units()]
                })?);
            }
            Awaited::Format => {
                self.format = Some(word.and_then(EmitFormat::parse).ok_or_else(|| {
                    let said = word.map_or_else(
                        || format!("{option} needs a name."),
                        |word| format!("unknown format '{word}'"),
                    );
                    [said, "Available: json, csv, lines.".to_owned()]
                })?);
            }
        }
        Ok(())
    }

    /// `--escaped` and `--raw`: one axis in two directions, as in `nkb emit`.
    fn take_form(&mut self, asked: Form) -> Result<(), Refusal> {
        if self.form.is_some_and(|already| already != asked) {
            return Err([
                "--escaped and --raw ask for opposite things.".to_owned(),
                "Pick one, or neither to take the format's default.".to_owned(),
            ]);
        }
        self.form = Some(asked);
        Ok(())
    }

    /// The limit and the shape these wishes come to.
    fn settled(self) -> Result<Asked, Refusal> {
        let limit = Limit::new(self.unit.unwrap_or(Unit::Graphemes), self.min, self.max).map_err(
            |problem| match problem {
                LimitProblem::NoBound => [
                    "give the limit of the field: --max-length, --min-length or both.".to_owned(),
                    see_help(),
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
                    [out_of_range("a limit"), see_help()]
                }
            },
        )?;
        let shape = Shape {
            format: self.format.unwrap_or(EmitFormat::Json),
            form: self.form,
            base64: self.base64,
        };
        Ok(Asked::Values(limit, shape))
    }
}

fn out_of_range(option: &str) -> String {
    format!("{option} needs a whole number from 1 to {MAX_LIMIT}")
}

fn a_bound(option: &str, word: Option<&str>) -> Result<u32, Refusal> {
    let Some(word) = word else {
        return Err([format!("{}.", out_of_range(option)), see_help()]);
    };
    bound(word).ok_or_else(|| {
        [
            format!("{}, got '{word}'.", out_of_range(option)),
            see_help(),
        ]
    })
}

/// Reads the arguments of `nkb boundary`.
fn asked(args: &[String]) -> Result<Asked, Refusal> {
    let mut wishes = Wishes::default();
    let mut words = args.iter().map(String::as_str);
    while let Some(word) = words.next() {
        match word {
            "-h" | "--help" => return Ok(Asked::Help),
            "--escaped" => wishes.take_form(Form::Escaped)?,
            "--raw" => wishes.take_form(Form::Literal)?,
            "--base64" => wishes.base64 = true,
            other => {
                let Some(awaited) = Awaited::of(other) else {
                    return Err(not_an_option(other));
                };
                wishes.take(awaited, words.next())?;
            }
        }
    }
    wishes.settled()
}

fn not_an_option(word: &str) -> Refusal {
    if word.starts_with('-') {
        return [format!("unknown option '{word}'"), see_help()];
    }
    let options: Vec<&str> = [Awaited::Max, Awaited::Min]
        .into_iter()
        .map(Awaited::option)
        .collect();
    [
        format!("'{word}' is not an option, and this command takes no name."),
        format!("A limit is given with {}.", listed(&options)),
    ]
}

/// `nkb boundary [--max-length N] [--min-length N] [--unit U] [--format F]`
///
/// Values go to standard output and the account of them to standard error, as
/// with `nkb emit`. Nothing here can fail but the call itself and the writing.
pub(crate) fn boundary(args: &[String]) -> ExitCode {
    let mut err = std::io::stderr();
    let (limit, shape) = match asked(args) {
        Ok(Asked::Help) => {
            print_boundary_help();
            return ExitCode::Ok;
        }
        Ok(Asked::Values(limit, shape)) => (limit, shape),
        Err(lines) => {
            let _ = writeln!(err, "nkb boundary: {}", lines[0]);
            let _ = writeln!(err, "{}", lines[1]);
            return ExitCode::Usage;
        }
    };
    let values = values(limit);
    // The account first, counted from the recipes: it is what a person reads
    // before deciding whether to let the rest arrive.
    for note in output::notes(&values, shape) {
        let _ = writeln!(err, "nkb boundary: {note}");
    }
    output::write(limit, &values, shape, |piece| {
        out!("{piece}");
        is_open()
    });
    ExitCode::Ok
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;

    fn read(words: &[&str]) -> Result<Asked, Refusal> {
        let args: Vec<String> = words.iter().map(|&word| word.to_owned()).collect();
        asked(&args)
    }

    fn refused(words: &[&str]) -> String {
        match read(words) {
            Err(lines) => lines.join(" | "),
            Ok(asked) => panic!("{words:?} was taken as {asked:?}"),
        }
    }

    fn taken(words: &[&str]) -> (Limit, Shape) {
        match read(words) {
            Ok(Asked::Values(limit, shape)) => (limit, shape),
            other => panic!("{words:?} came to {other:?}"),
        }
    }

    #[test]
    fn a_most_alone_is_counted_in_graphemes_and_printed_as_json() {
        let (limit, shape) = taken(&["--max-length", "50"]);
        assert_eq!(
            (limit.unit(), limit.min(), limit.max()),
            (Unit::Graphemes, None, Some(50))
        );
        assert_eq!(
            shape,
            Shape {
                format: EmitFormat::Json,
                form: None,
                base64: false
            }
        );
    }

    #[test]
    fn every_option_is_read_in_any_order() {
        let (limit, shape) = taken(&[
            "--base64",
            "--format",
            "csv",
            "--unit",
            "utf16",
            "--raw",
            "--min-length",
            "3",
            "--max-length",
            "100000",
        ]);
        assert_eq!(
            (limit.unit(), limit.min(), limit.max()),
            (Unit::Utf16, Some(3), Some(100_000))
        );
        assert_eq!(
            shape,
            Shape {
                format: EmitFormat::Csv,
                form: Some(Form::Literal),
                base64: true
            }
        );
        let (least, _) = taken(&["--min-length", "8"]);
        assert_eq!((least.min(), least.max()), (Some(8), None));
    }

    #[test]
    fn help_is_help_wherever_it_stands() {
        assert_eq!(read(&["--help"]), Ok(Asked::Help));
        assert_eq!(read(&["--max-length", "50", "-h"]), Ok(Asked::Help));
    }

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
    fn a_wrong_number_is_named_with_the_range_it_had_to_be_in() {
        assert_eq!(
            refused(&["--max-length", "+50"]),
            "--max-length needs a whole number from 1 to 100000, got '+50'. | \
             Run 'nkb boundary --help' to see what is available."
        );
        assert!(
            refused(&["--min-length", "0"])
                .starts_with("--min-length needs a whole number from 1 to 100000, got '0'.")
        );
        // The next option is not a number either, and is not swallowed quietly.
        assert!(refused(&["--max-length", "--unit", "bytes"]).contains("got '--unit'"));
        assert!(
            refused(&["--max-length"])
                .starts_with("--max-length needs a whole number from 1 to 100000. |")
        );
    }

    #[test]
    fn a_call_without_a_bound_or_with_the_bounds_crossed_is_refused() {
        assert!(refused(&[]).starts_with("give the limit of the field"));
        assert!(refused(&["--unit", "bytes"]).starts_with("give the limit of the field"));
        assert_eq!(
            refused(&["--min-length", "51", "--max-length", "50"]),
            "--min-length 51 is above --max-length 50. | \
             The least a field takes cannot be more than the most."
        );
        assert!(read(&["--min-length", "50", "--max-length", "50"]).is_ok());
    }

    #[test]
    fn an_option_given_twice_is_refused_rather_than_settled_quietly() {
        for option in ["--max-length", "--min-length"] {
            assert!(
                refused(&[option, "5", option, "6"])
                    .starts_with(&format!("{option} was given twice.")),
                "{option}"
            );
        }
        assert!(
            refused(&["--max-length", "5", "--unit", "bytes", "--unit", "utf16"])
                .starts_with("--unit was given twice.")
        );
        assert!(
            refused(&["--max-length", "5", "--format", "csv", "--format", "csv"])
                .starts_with("--format was given twice.")
        );
    }

    #[test]
    fn an_unknown_unit_or_format_is_answered_with_the_ones_there_are() {
        assert_eq!(
            refused(&["--max-length", "5", "--unit", "chars"]),
            "unknown unit 'chars' | Available: graphemes, code-points, utf16, bytes."
        );
        assert_eq!(
            refused(&["--max-length", "5", "--unit"]),
            "--unit needs a name. | Available: graphemes, code-points, utf16, bytes."
        );
        assert_eq!(
            refused(&["--max-length", "5", "--format", "xml"]),
            "unknown format 'xml' | Available: json, csv, lines."
        );
    }

    #[test]
    fn opposite_forms_an_unknown_option_and_a_bare_word_are_each_named() {
        assert!(
            refused(&["--max-length", "5", "--raw", "--escaped"])
                .starts_with("--escaped and --raw ask for opposite things.")
        );
        assert!(read(&["--max-length", "5", "--raw", "--raw"]).is_ok());
        assert!(refused(&["--max", "5"]).starts_with("unknown option '--max'"));
        assert_eq!(
            refused(&["50"]),
            "'50' is not an option, and this command takes no name. | \
             A limit is given with --max-length and --min-length."
        );
    }
}
