//! The three shapes `nkb emit` prints, and what each of them cannot carry.
//!
//! # Three formats, three different answers about the same question
//!
//! `pack-format.md` 15 settles them, and the reasoning is worth keeping next to
//! the code because each rule looks arbitrary on its own:
//!
//! - **JSON** carries both forms of every value and expands recipes. It is the
//!   only format that survives the whole catalogue, so it is the default.
//! - **CSV** defaults to the ESCAPED form. The catalogue contains a pack built
//!   from commas, quotation marks and line breaks - exporting a catalogue of
//!   hostile values to CSV is an operation capable of destroying its own output.
//!   `--raw` asks for the literal form and gets a note saying what that means.
//! - **lines** cannot carry a value containing a line break, and no amount of
//!   quoting fixes that. It refuses such a value by name rather than splitting
//!   it silently across two rows.
//!
//! # The rule above all three
//!
//! `emit` never loses information silently. Every limitation ends in something
//! written to the error stream, never in a quiet substitution. A tool whose
//! subject is silent data loss cannot be a cause of it.
//!
//! # Which stream
//!
//! `ux-spec.md` 10: values on standard output, everything else on the error
//! stream. That is why [`write()`] puts the values through one door and hands
//! every note to another, and [`notes()`] are told apart from the values before
//! the first one - a note about lost information inside the data file would be
//! the exact fault it warns about.

use crate::english::plural;
use crate::exit::ExitCode;
use crate::json::Json;
use crate::output::{is_open, out};
use nkb_app::emit_values::{Emission, EmittedValue, NotBuilt};
use std::io::Write as _;

/// Version of the JSON THIS command prints.
///
/// 🔴 A different axis from the linter's schema, which happens to be on the
/// same number today. The two describe different documents, are consumed by
/// different scripts and move independently: one going to 2 says nothing about
/// the other. Until 2026-09-08 this was a bare literal here, which is the one
/// spelling nobody finds while searching for the constant - OBS-99.
///
/// Raised only by a breaking change: a member removed, or one whose meaning
/// changed. Adding a member does not raise it, because a consumer that ignores
/// what it does not recognise is unaffected.
const SCHEMA: i64 = 1;

/// Which shape the values come out in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmitFormat {
    Json,
    Csv,
    Lines,
}

impl EmitFormat {
    /// Reads the name a person typed.
    #[must_use]
    pub fn parse(word: &str) -> Option<Self> {
        match word {
            "json" => Some(Self::Json),
            "csv" => Some(Self::Csv),
            "lines" => Some(Self::Lines),
            _ => None,
        }
    }

    /// The form this format uses when nobody says otherwise.
    ///
    /// The two defaults point in opposite directions, and that is deliberate
    /// rather than an inconsistency: CSV is a format a hostile value can break,
    /// so it defaults to safety, and `lines` exists to hand somebody the actual
    /// bytes, so it defaults to fidelity.
    #[must_use]
    pub const fn default_form(self) -> Form {
        match self {
            Self::Csv => Form::Escaped,
            Self::Json | Self::Lines => Form::Literal,
        }
    }
}

/// Which form of the text a caller wants where a format prints only one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Form {
    /// As it would arrive in a field.
    Literal,
    /// As a pack file stores it.
    Escaped,
}

/// What the account on the error stream is made of: the size of everything,
/// counted from the recipes before any value is built.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Account {
    pub values: usize,
    pub code_points: usize,
    pub bytes: usize,
    /// Warnings the pack carries.
    pub warnings: usize,
}

impl Account {
    /// The account of one emission.
    #[must_use]
    pub fn of(emission: &Emission) -> Self {
        Self {
            values: emission.len(),
            code_points: emission.code_points,
            bytes: emission.bytes,
            warnings: emission.warnings,
        }
    }
}

/// What the error stream is told before the first value: the account, the
/// warnings, and what the chosen shape costs. Every one of them is known before
/// anything is built, which is the point - it is what a person reads before
/// deciding whether to let the rest arrive.
#[must_use]
pub fn notes(
    account: &Account,
    format: EmitFormat,
    form: Option<Form>,
    base64: bool,
) -> Vec<String> {
    let form = form.unwrap_or_else(|| format.default_form());
    let mut notes = vec![format!(
        "{}, {}, {}",
        plural(account.values, "value"),
        plural(account.code_points, "code point"),
        plural(account.bytes, "byte")
    )];
    if account.warnings > 0 {
        notes.push(format!(
            "the pack carries {} warning{} - run `nkb lint` to read them",
            account.warnings,
            if account.warnings == 1 { "" } else { "s" }
        ));
    }
    if base64 {
        notes.push("values are base64 encoded".to_owned());
    }
    if format == EmitFormat::Csv && form == Form::Literal && !base64 {
        notes.push(
            "--raw prints values literally, so a value containing a comma, a quotation mark \
             or a line break is quoted rather than escaped - the file stays valid CSV and the \
             cells hold characters no editor will show you. A spreadsheet opening it also runs \
             every value that starts with =, +, - or @ as a formula"
                .to_owned(),
        );
    }
    notes
}

/// Prints one emission: the notes on the error stream, then the values on
/// standard output one at a time, each built when reached (`emit_values`). A
/// closed pipe - `nkb emit pack | head -1` - ends the run quietly with 0 and
/// builds nothing more, a full disk under `> file` ends it with 5 (`output`).
pub(crate) fn print(
    emission: &Emission,
    format: EmitFormat,
    form: Option<Form>,
    base64: bool,
) -> ExitCode {
    for note in notes(&Account::of(emission), format, form, base64) {
        let _ = writeln!(crate::output::err(), "nkb emit: {note}");
    }
    let written = write(
        emission.values(),
        format,
        form,
        base64,
        |piece| {
            out!("{piece}");
            is_open()
        },
        |note| {
            let _ = writeln!(crate::output::err(), "nkb emit: {note}");
        },
    );
    let Err(not_built) = written else {
        return ExitCode::Ok;
    };
    let mut err = crate::output::err();
    let _ = writeln!(
        err,
        "nkb emit: value '{}' passed validation and could not be built ({}).",
        not_built.id, not_built.code
    );
    let _ = writeln!(
        err,
        "This is a fault in the tool, not in the pack. What was printed before it is incomplete."
    );
    ExitCode::ValidationFailed
}

/// Writes the values through `put`, one at a time, each built as it is reached
/// and let go once written - memory holds one value however many the pack has
/// (`nkb_app::emit_values`). `put` answers whether there is still somebody to
/// write for, and the first no ends the work. `note` hears what a format could
/// not carry, when it is met.
///
/// # Errors
///
/// [`NotBuilt`] when a value could not be built after all - the run stops there
/// and the caller says so. Unreachable while the size rules hold
/// ([`Emission::values`]).
pub fn write<I>(
    values: I,
    format: EmitFormat,
    form: Option<Form>,
    base64: bool,
    mut put: impl FnMut(&str) -> bool,
    mut note: impl FnMut(String),
) -> Result<(), NotBuilt>
where
    I: IntoIterator<Item = Result<EmittedValue, NotBuilt>>,
{
    let form = form.unwrap_or_else(|| format.default_form());
    let opening = match format {
        EmitFormat::Json => {
            Json::open_stream(&[("schema".to_owned(), Json::Number(SCHEMA))], "values")
        }
        EmitFormat::Csv => format!("{CSV_HEADER}\n"),
        EmitFormat::Lines => String::new(),
    };
    if !opening.is_empty() && !put(&opening) {
        return Ok(());
    }
    let mut any = false;
    for value in values {
        let value = value?;
        let piece = match format {
            EmitFormat::Json => Some(as_json(&value, base64).stream_item(!any)),
            EmitFormat::Csv => Some(csv_row(&value, form, base64)),
            EmitFormat::Lines => as_line(&value, form, base64, &mut note),
        };
        let Some(piece) = piece else {
            continue;
        };
        any = true;
        if !put(&piece) {
            return Ok(());
        }
    }
    if format == EmitFormat::Json {
        put(&Json::close_stream(any));
    }
    Ok(())
}

/// The characters a spreadsheet reads as the start of a formula when a cell
/// begins with one of them.
const FORMULA_STARTS: [char; 4] = ['=', '+', '-', '@'];

/// The escaped form of a value, with a first character a spreadsheet would run
/// as a formula written as an escape instead.
///
/// `OBS-142`, `D81`: until 2026-09-24 `=1+1`, `+48123456789`, `-1+1` and `@name`
/// from `export-breakers` left this command as bare cells, and a spreadsheet
/// opening the file computed them - the exact failure whose `expect` in that
/// pack reads "escaped on export so the spreadsheet shows the text as entered".
/// Nothing is lost: the escaped form is read back by the same rule as a pack
/// file, so the escape of `=` decodes to `=`. Quoting the cell is not relied on:
/// whether a spreadsheet looks for a formula before or after it removes the
/// quotes was not measured, and an escape does not depend on the answer.
///
/// Every cell of a row goes through this, the value in its escaped form only:
/// `--raw` is literal because somebody asked for it, and its note says what that
/// costs. The other cells - name, breaks, expect, source and the rest - come
/// from a pack file, which may be somebody else's, so a name that starts with
/// `=` must not reach a spreadsheet as a formula either (report of 2026-11-20).
/// A tab or a carriage return in front, which OWASP lists too, never begins a
/// cell here: the escaped form writes both as escapes and the prose of a pack
/// refuses them (`E035`). Base64 cannot start with any of these: `+` and `/`
/// would need a first byte from 0xF8 up, which UTF-8 never has, `=` only pads
/// the end, and `-` and `@` are not in the alphabet.
pub(crate) fn not_a_formula(escaped: &str) -> String {
    let mut rest = escaped.chars();
    match rest.next() {
        Some(first) if FORMULA_STARTS.contains(&first) => {
            format!("{}u{:04X}{}", '\\', u32::from(first), rest.as_str())
        }
        _ => escaped.to_owned(),
    }
}

/// The text of one value in the requested form.
fn body_of(value: &EmittedValue, form: Form, base64: bool) -> String {
    let text = match form {
        Form::Literal => &value.literal,
        Form::Escaped => &value.escaped,
    };
    if base64 {
        encode_base64(text.as_bytes())
    } else {
        text.clone()
    }
}

/// One value as the object `nkb emit` prints for it.
fn as_json(value: &EmittedValue, base64: bool) -> Json {
    Json::Object(vec![
        ("pack".to_owned(), Json::text(&value.pack)),
        ("ref".to_owned(), Json::text(&value.reference)),
        ("id".to_owned(), Json::text(&value.id)),
        ("name".to_owned(), Json::text(&value.name)),
        // Both forms, always. A consumer that wants one takes one, and
        // one that wants to compare them has them side by side.
        (
            "value".to_owned(),
            Json::Text(body_of(value, Form::Literal, base64)),
        ),
        (
            "escaped".to_owned(),
            Json::Text(body_of(value, Form::Escaped, base64)),
        ),
        ("breaks".to_owned(), optional(value.breaks.as_deref())),
        ("expect".to_owned(), optional(value.expect.as_deref())),
        (
            "fields".to_owned(),
            Json::Array(value.fields.iter().map(Json::text).collect()),
        ),
        (
            "tags".to_owned(),
            Json::Array(value.tags.iter().map(Json::text).collect()),
        ),
        ("risk".to_owned(), Json::text(value.risk.as_str())),
        ("since".to_owned(), optional(value.since.as_deref())),
        ("source".to_owned(), optional(value.source.as_deref())),
        ("generated".to_owned(), Json::Bool(value.generated)),
    ])
}

/// A field the format allows to be absent. `null` rather than an empty string,
/// because "said nothing" and "said nothing in particular" are different facts.
fn optional(value: Option<&str>) -> Json {
    value.map_or(Json::Null, Json::text)
}

const CSV_HEADER: &str =
    "pack,ref,id,name,value,breaks,expect,fields,tags,risk,since,source,generated";

/// One row: every cell guarded against a spreadsheet formula, the value only in
/// its escaped form ([`not_a_formula`]), then quoted where it has to be.
fn csv_row(value: &EmittedValue, form: Form, base64: bool) -> String {
    let body = body_of(value, form, base64);
    let value_cell = if form == Form::Escaped && !base64 {
        not_a_formula(&body)
    } else {
        body
    };
    let row = [
        not_a_formula(&value.pack),
        not_a_formula(&value.reference),
        not_a_formula(&value.id),
        not_a_formula(&value.name),
        value_cell,
        not_a_formula(value.breaks.as_deref().unwrap_or_default()),
        not_a_formula(value.expect.as_deref().unwrap_or_default()),
        not_a_formula(&value.fields.join(" ")),
        not_a_formula(&value.tags.join(" ")),
        not_a_formula(value.risk.as_str()),
        not_a_formula(value.since.as_deref().unwrap_or_default()),
        not_a_formula(value.source.as_deref().unwrap_or_default()),
        value.generated.to_string(),
    ];
    let cells: Vec<String> = row.iter().map(|cell| csv_cell(cell)).collect();
    let mut out = cells.join(",");
    out.push('\n');
    out
}

/// One CSV cell, quoted when it has to be.
///
/// RFC 4180: a field holding a comma, a quotation mark or a line break is
/// wrapped in quotation marks, and a quotation mark inside is doubled. Written
/// out because the catalogue contains values designed to break exactly this.
pub(crate) fn csv_cell(text: &str) -> String {
    let needs_quoting = text.contains([',', '"', '\n', '\r']);
    if !needs_quoting {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for character in text.chars() {
        if character == '"' {
            out.push('"');
        }
        out.push(character);
    }
    out.push('"');
    out
}

/// One value as one line, or nothing and a note when the value holds a line break.
fn as_line(
    value: &EmittedValue,
    form: Form,
    base64: bool,
    note: &mut impl FnMut(String),
) -> Option<String> {
    let mut body = body_of(value, form, base64);
    if body.contains(['\n', '\r']) {
        // 🔴 The refusal that gives this format its meaning. Splitting the
        // value across two rows would hand somebody a file whose row count
        // disagrees with the catalogue, silently.
        note(format!(
            "{}: not printed - the value contains a line break, which this format cannot \
             carry. Use --escaped, --base64, or --format json",
            value.reference
        ));
        return None;
    }
    body.push('\n');
    Some(body)
}

/// Standard base64, written out rather than taken from a crate.
///
/// Twenty lines against a dependency in an artifact meant for other people's
/// continuous integration - the same trade the JSON writer next door made, for
/// the same reason.
pub(crate) fn encode_base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);

    for chunk in bytes.chunks(3) {
        let b0 = u32::from(chunk[0]);
        let b1 = chunk.get(1).copied().map_or(0, u32::from);
        let b2 = chunk.get(2).copied().map_or(0, u32::from);
        let triple = (b0 << 16) | (b1 << 8) | b2;

        out.push(ALPHABET[((triple >> 18) & 0x3F) as usize] as char);
        out.push(ALPHABET[((triple >> 12) & 0x3F) as usize] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[((triple >> 6) & 0x3F) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[(triple & 0x3F) as usize] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;
    use nkb_core::pack::Risk;

    fn value(id: &str, literal: &str, escaped: &str) -> EmittedValue {
        EmittedValue {
            pack: "sample".to_owned(),
            reference: format!("sample/{id}"),
            id: id.to_owned(),
            name: format!("Value {id}"),
            literal: literal.to_owned(),
            escaped: escaped.to_owned(),
            breaks: Some("It breaks something.".to_owned()),
            expect: None,
            fields: vec!["any".to_owned()],
            tags: Vec::new(),
            risk: Risk::Normal,
            since: Some("1.0".to_owned()),
            source: None,
            generated: false,
        }
    }

    /// Values already built, and the warnings of the pack they came from.
    struct Sample {
        values: Vec<EmittedValue>,
        warnings: usize,
    }

    fn emission(values: Vec<EmittedValue>) -> Sample {
        Sample {
            values,
            warnings: 0,
        }
    }

    /// Both streams of one run, the way `nkb emit` writes them.
    struct Rendered {
        data: String,
        notes: Vec<String>,
    }

    fn render(sample: &Sample, format: EmitFormat, form: Option<Form>, base64: bool) -> Rendered {
        let account = Account {
            values: sample.values.len(),
            code_points: sample
                .values
                .iter()
                .map(|v| v.literal.chars().count())
                .sum(),
            bytes: sample.values.iter().map(|v| v.literal.len()).sum(),
            warnings: sample.warnings,
        };
        let mut told = notes(&account, format, form, base64);
        let mut data = String::new();
        let mut later = Vec::new();
        write(
            sample.values.iter().cloned().map(Ok),
            format,
            form,
            base64,
            |piece| {
                data.push_str(piece);
                true
            },
            |note| later.push(note),
        )
        .expect("every value is built");
        told.extend(later);
        Rendered { data, notes: told }
    }

    #[test]
    fn the_account_goes_to_the_notes_and_never_into_the_data() {
        // ux-spec.md 10. A count inside the data file would corrupt it for the
        // script that asked for values.
        let rendered = render(
            &emission(vec![value("a", "x", "x")]),
            EmitFormat::Lines,
            None,
            false,
        );
        assert_eq!(rendered.data, "x\n");
        // In the singular: a pack of one value is allowed, and "1 values, 1 code
        // points, 1 bytes" was what it printed until 2026-09-24 (`OBS-120`).
        assert_eq!(
            rendered.notes.first().map(String::as_str),
            Some("1 value, 1 code point, 1 byte"),
            "the account must come first and be in English: {:?}",
            rendered.notes
        );
    }

    #[test]
    fn json_carries_both_forms_of_every_value() {
        let rendered = render(
            &emission(vec![value("zw", "a\u{200B}b", "a\\u200Bb")]),
            EmitFormat::Json,
            None,
            false,
        );
        assert!(rendered.data.contains("\"escaped\""));
        assert!(
            rendered.data.contains("a\\\\u200Bb"),
            "the escaped form must survive JSON escaping intact: {}",
            rendered.data
        );
        assert!(rendered.data.contains("\"schema\": 1"));
    }

    #[test]
    fn csv_defaults_to_the_escaped_form_because_the_catalogue_can_break_csv() {
        let rendered = render(
            &emission(vec![value("zw", "a\u{200B}b", "a\\u200Bb")]),
            EmitFormat::Csv,
            None,
            false,
        );
        assert!(rendered.data.contains("a\\u200Bb"));
        assert!(
            !rendered.data.contains('\u{200B}'),
            "the default must not put an invisible character into a spreadsheet"
        );
    }

    #[test]
    fn csv_quotes_a_value_that_would_otherwise_break_the_file() {
        // The pack `export-breakers` exists to produce exactly this.
        let rendered = render(
            &emission(vec![value("comma", "a,\"b\"", "a,\"b\"")]),
            EmitFormat::Csv,
            Some(Form::Literal),
            false,
        );
        assert!(
            rendered.data.contains("\"a,\"\"b\"\"\""),
            "a comma and a quotation mark must be quoted and doubled: {}",
            rendered.data
        );
    }

    #[test]
    fn csv_escapes_a_first_character_a_spreadsheet_would_run_as_a_formula() {
        // `OBS-142`: the four from `export-breakers`, plus the neighbours that
        // must stay as they are. Built with a char, never written as an escape
        // in this source - an editor turns that sequence into the character.
        let bs = '\\';
        let rendered = render(
            &emission(vec![
                value("eq", "=1+1", "=1+1"),
                value("plus", "+48123456789", "+48123456789"),
                value("minus", "-1+1", "-1+1"),
                value("at", "@name", "@name"),
                value("inside", "a=b", "a=b"),
                value("escaped", " x", &format!("{bs}u0020x")),
            ]),
            EmitFormat::Csv,
            None,
            false,
        );
        let cells: Vec<&str> = rendered
            .data
            .lines()
            .skip(1)
            .map(|row| row.split(',').nth(4).unwrap_or_default())
            .collect();
        assert_eq!(
            cells,
            vec![
                format!("{bs}u003D1+1"),
                format!("{bs}u002B48123456789"),
                format!("{bs}u002D1+1"),
                format!("{bs}u0040name"),
                "a=b".to_owned(),
                format!("{bs}u0020x"),
            ],
            "{}",
            rendered.data
        );

        // Asked for literally, the value stays literal - and the note says what
        // a spreadsheet will do with it.
        let raw = render(
            &emission(vec![value("eq", "=1+1", "=1+1")]),
            EmitFormat::Csv,
            Some(Form::Literal),
            false,
        );
        assert!(raw.data.contains(",=1+1,"), "{}", raw.data);
        assert!(
            raw.notes.iter().any(|n| n.contains("as a formula")),
            "{:?}",
            raw.notes
        );
    }

    #[test]
    fn asking_csv_for_raw_values_says_what_that_costs() {
        let rendered = render(
            &emission(vec![value("a", "x", "x")]),
            EmitFormat::Csv,
            Some(Form::Literal),
            false,
        );
        assert!(
            rendered.notes.iter().any(|n| n.contains("--raw")),
            "a limitation must end in a note, never in a silent substitution"
        );
    }

    #[test]
    fn lines_refuses_a_value_with_a_line_break_and_names_it() {
        // 🔴 The refusal this format exists to make. Splitting the value would
        // give a file whose row count disagrees with the catalogue.
        let rendered = render(
            &emission(vec![
                value("plain", "before", "before"),
                value("newline", "a\nb", "a\\nb"),
            ]),
            EmitFormat::Lines,
            None,
            false,
        );
        assert_eq!(rendered.data, "before\n", "only the printable value is out");
        assert!(
            rendered
                .notes
                .iter()
                .any(|n| n.contains("sample/newline") && n.contains("line break")),
            "the refusal must name the value: {:?}",
            rendered.notes
        );
    }

    #[test]
    fn lines_carries_everything_once_the_escaped_form_is_asked_for() {
        let rendered = render(
            &emission(vec![value("newline", "a\nb", "a\\nb")]),
            EmitFormat::Lines,
            Some(Form::Escaped),
            false,
        );
        assert_eq!(rendered.data, "a\\nb\n");
        assert!(
            !rendered.notes.iter().any(|n| n.contains("not printed")),
            "nothing was refused, so nothing should be reported as refused"
        );
    }

    #[test]
    fn base64_lets_lines_carry_a_value_it_otherwise_could_not() {
        let rendered = render(
            &emission(vec![value("newline", "a\nb", "a\\nb")]),
            EmitFormat::Lines,
            Some(Form::Literal),
            true,
        );
        assert_eq!(rendered.data, "YQpi\n");
        assert!(rendered.notes.iter().any(|n| n.contains("base64")));
    }

    #[test]
    fn base64_matches_the_published_alphabet_and_padding() {
        // Checked against the standard's own examples rather than against
        // itself: a round trip through one implementation proves nothing.
        assert_eq!(encode_base64(b""), "");
        assert_eq!(encode_base64(b"f"), "Zg==");
        assert_eq!(encode_base64(b"fo"), "Zm8=");
        assert_eq!(encode_base64(b"foo"), "Zm9v");
        assert_eq!(encode_base64(b"foob"), "Zm9vYg==");
        assert_eq!(encode_base64(b"fooba"), "Zm9vYmE=");
        assert_eq!(encode_base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn a_multi_byte_character_is_encoded_from_its_bytes() {
        assert_eq!(encode_base64("\u{0105}".as_bytes()), "xIU=");
    }

    #[test]
    fn a_format_name_nobody_defined_is_not_guessed_at() {
        assert_eq!(EmitFormat::parse("json"), Some(EmitFormat::Json));
        assert_eq!(EmitFormat::parse("JSON"), None);
        assert_eq!(EmitFormat::parse("yaml"), None);
    }

    #[test]
    fn a_pack_with_warnings_says_so_on_the_error_stream() {
        let mut with_warnings = emission(vec![value("a", "x", "x")]);
        with_warnings.warnings = 2;
        let rendered = render(&with_warnings, EmitFormat::Lines, None, false);
        assert!(
            rendered.notes.iter().any(|n| n.contains("2 warnings")),
            "a warned pack must not look identical to a clean one: {:?}",
            rendered.notes
        );
    }

    #[test]
    fn csv_guards_every_cell_from_the_pack_file_not_only_the_value() {
        // The report of 2026-11-20: a pack of somebody else's is read like any
        // other, and its name, breaks or source starting with `=` reached the
        // spreadsheet as a formula. Every cell is guarded in both forms. Only
        // the value keeps `--raw` literal, because that is what was asked for.
        let bs = '\\';
        let mut hostile = value("v", "=1+1", "=1+1");
        hostile.name = "=HYPERLINK(1)".to_owned();
        hostile.breaks = Some("+cmd".to_owned());
        hostile.expect = Some("-1".to_owned());
        hostile.source = Some("@someone".to_owned());
        hostile.tags = vec!["=tag".to_owned()];
        for (form, value_cell) in [
            (Form::Escaped, format!("{bs}u003D1+1")),
            (Form::Literal, "=1+1".to_owned()),
        ] {
            let rendered = render(
                &emission(vec![hostile.clone()]),
                EmitFormat::Csv,
                Some(form),
                false,
            );
            let row = rendered.data.lines().nth(1).expect("one row");
            let cells: Vec<&str> = row.split(',').collect();
            assert_eq!(cells[3], format!("{bs}u003DHYPERLINK(1)"), "{row}");
            assert_eq!(cells[4], value_cell, "{row}");
            assert_eq!(cells[5], format!("{bs}u002Bcmd"), "{row}");
            assert_eq!(cells[6], format!("{bs}u002D1"), "{row}");
            assert_eq!(cells[8], format!("{bs}u003Dtag"), "{row}");
            assert_eq!(cells[11], format!("{bs}u0040someone"), "{row}");
            assert!(
                cells
                    .iter()
                    .skip(5)
                    .all(|cell| !cell.starts_with(['=', '+', '-', '@'])),
                "{row}"
            );
        }
    }

    #[test]
    fn json_written_one_value_at_a_time_is_the_document_it_always_was() {
        // Until 2026-11-24 the whole document was built as one tree and rendered
        // at once. Written piece by piece it must be the same bytes, empty,
        // with one value and with several.
        for values in [
            Vec::new(),
            vec![value("a", "x", "x")],
            vec![value("a", "x", "x"), value("b", "a\nb", "a\\nb")],
        ] {
            let whole = Json::Object(vec![
                ("schema".to_owned(), Json::Number(SCHEMA)),
                (
                    "values".to_owned(),
                    Json::Array(values.iter().map(|v| as_json(v, false)).collect()),
                ),
            ])
            .render();
            let streamed = render(&emission(values), EmitFormat::Json, None, false);
            assert_eq!(streamed.data, whole);
        }
    }

    #[test]
    fn a_reader_that_left_stops_the_building() {
        // `nkb emit pack | head -1`: once nobody reads, no further value is
        // built, which matters when each is a million characters.
        let mut asked = 0;
        let values = (0..5).map(|n| {
            asked += 1;
            Ok(value(&format!("v{n}"), "x", "x"))
        });
        let mut pieces = 0;
        let written = write(
            values,
            EmitFormat::Lines,
            None,
            false,
            |_| {
                pieces += 1;
                pieces < 2
            },
            |_| {},
        );
        assert_eq!(written, Ok(()));
        assert_eq!((pieces, asked), (2, 2));
    }

    #[test]
    fn a_value_that_could_not_be_built_stops_the_run_and_is_named() {
        let values = vec![
            Ok(value("a", "x", "x")),
            Err(NotBuilt {
                id: "bomb".to_owned(),
                code: "E026",
            }),
            Ok(value("c", "y", "y")),
        ];
        let mut data = String::new();
        let written = write(
            values,
            EmitFormat::Lines,
            None,
            false,
            |piece| {
                data.push_str(piece);
                true
            },
            |_| {},
        );
        assert_eq!(
            written,
            Err(NotBuilt {
                id: "bomb".to_owned(),
                code: "E026"
            })
        );
        assert_eq!(data, "x\n", "nothing after the value that failed");
    }

    #[test]
    fn an_empty_emission_produces_a_header_only_csv_and_no_rows() {
        let rendered = render(&emission(Vec::new()), EmitFormat::Csv, None, false);
        assert_eq!(rendered.data, format!("{CSV_HEADER}\n"));
    }
}
