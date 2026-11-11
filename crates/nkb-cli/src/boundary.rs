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
use crate::limit_args::{LimitOption, LimitWishes, Refusal, see_help};
use crate::output::{is_open, out};
use nkb_core::boundary::{Limit, values};
use output::Shape;
use std::io::Write;

/// The name of this command, for the sentences that send a person to its help.
const COMMAND: &str = "boundary";

/// An option of this command that takes a word after it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Awaited {
    Limit(LimitOption),
    Format,
}

impl Awaited {
    fn of(option: &str) -> Option<Self> {
        if option == "--format" {
            return Some(Self::Format);
        }
        LimitOption::of(option).map(Self::Limit)
    }
}

/// What was asked for, as far as the arguments said.
#[derive(Debug, Default, PartialEq, Eq)]
struct Wishes {
    limit: LimitWishes,
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

impl Wishes {
    /// Takes the word after an option. An option given twice is refused
    /// rather than settled quietly, as `limit_args` refuses a part of the
    /// limit given twice.
    fn take(&mut self, awaited: Awaited, word: Option<&str>) -> Result<(), Refusal> {
        let Awaited::Limit(option) = awaited else {
            return self.take_format(word);
        };
        self.limit.take(option, word, COMMAND)
    }

    fn take_format(&mut self, word: Option<&str>) -> Result<(), Refusal> {
        if self.format.is_some() {
            return Err(["--format was given twice.".to_owned(), see_help(COMMAND)]);
        }
        self.format = Some(word.and_then(EmitFormat::parse).ok_or_else(|| {
            let said = word.map_or_else(
                || "--format needs a name.".to_owned(),
                |word| format!("unknown format '{word}'"),
            );
            [said, "Available: json, csv, lines.".to_owned()]
        })?);
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
        let limit = self.limit.settled(COMMAND)?;
        let shape = Shape {
            format: self.format.unwrap_or(EmitFormat::Json),
            form: self.form,
            base64: self.base64,
        };
        Ok(Asked::Values(limit, shape))
    }
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
        return [format!("unknown option '{word}'"), see_help(COMMAND)];
    }
    let options: Vec<&str> = [LimitOption::Max, LimitOption::Min]
        .into_iter()
        .map(LimitOption::option)
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
    use nkb_core::boundary::Unit;

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
