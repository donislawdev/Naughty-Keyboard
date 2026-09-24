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
//! stream. That is why [`Rendered`] keeps `data` and `notes` apart instead of
//! returning one block of text - a note about lost information inside the data
//! file would be the exact fault it warns about.

use crate::english::plural;
use crate::json::Json;
use nkb_app::emit_values::{Emission, EmittedValue};

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

/// What a run of `emit` produced, split by the stream it belongs on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rendered {
    /// Values, and nothing else. Standard output.
    pub data: String,
    /// The account, and every warning. The error stream.
    pub notes: Vec<String>,
}

/// Renders one emission.
#[must_use]
pub fn render(
    emission: &Emission,
    format: EmitFormat,
    form: Option<Form>,
    base64: bool,
) -> Rendered {
    let form = form.unwrap_or_else(|| format.default_form());
    let mut notes = Vec::new();

    // The account comes first, because it is the thing a person reads before
    // deciding whether to let the rest arrive.
    notes.push(format!(
        "{}, {}, {}",
        plural(emission.values.len(), "value"),
        plural(emission.code_points, "code point"),
        plural(emission.bytes, "byte")
    ));
    if emission.warnings > 0 {
        notes.push(format!(
            "the pack carries {} warning{} - run `nkb lint` to read them",
            emission.warnings,
            if emission.warnings == 1 { "" } else { "s" }
        ));
    }
    if base64 {
        notes.push("values are base64 encoded".to_owned());
    }

    let data = match format {
        EmitFormat::Json => as_json(emission, base64),
        EmitFormat::Csv => as_csv(emission, form, base64, &mut notes),
        EmitFormat::Lines => as_lines(emission, form, base64, &mut notes),
    };

    Rendered { data, notes }
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

fn as_json(emission: &Emission, base64: bool) -> String {
    let values = emission
        .values
        .iter()
        .map(|value| {
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
        })
        .collect();

    Json::Object(vec![
        ("schema".to_owned(), Json::Number(SCHEMA)),
        ("values".to_owned(), Json::Array(values)),
    ])
    .render()
}

/// A field the format allows to be absent. `null` rather than an empty string,
/// because "said nothing" and "said nothing in particular" are different facts.
fn optional(value: Option<&str>) -> Json {
    value.map_or(Json::Null, Json::text)
}

const CSV_HEADER: &str =
    "pack,ref,id,name,value,breaks,expect,fields,tags,risk,since,source,generated";

fn as_csv(emission: &Emission, form: Form, base64: bool, notes: &mut Vec<String>) -> String {
    if form == Form::Literal && !base64 {
        notes.push(
            "--raw prints values literally, so a value containing a comma, a quotation mark \
             or a line break is quoted rather than escaped - the file stays valid CSV and the \
             cells hold characters no editor will show you"
                .to_owned(),
        );
    }

    let mut out = String::from(CSV_HEADER);
    out.push('\n');
    for value in &emission.values {
        let row = [
            value.pack.clone(),
            value.reference.clone(),
            value.id.clone(),
            value.name.clone(),
            body_of(value, form, base64),
            value.breaks.clone().unwrap_or_default(),
            value.expect.clone().unwrap_or_default(),
            value.fields.join(" "),
            value.tags.join(" "),
            value.risk.as_str().to_owned(),
            value.since.clone().unwrap_or_default(),
            value.source.clone().unwrap_or_default(),
            value.generated.to_string(),
        ];
        let cells: Vec<String> = row.iter().map(|cell| csv_cell(cell)).collect();
        out.push_str(&cells.join(","));
        out.push('\n');
    }
    out
}

/// One CSV cell, quoted when it has to be.
///
/// RFC 4180: a field holding a comma, a quotation mark or a line break is
/// wrapped in quotation marks, and a quotation mark inside is doubled. Written
/// out because the catalogue contains values designed to break exactly this.
fn csv_cell(text: &str) -> String {
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

fn as_lines(emission: &Emission, form: Form, base64: bool, notes: &mut Vec<String>) -> String {
    let mut out = String::new();
    for value in &emission.values {
        let body = body_of(value, form, base64);
        if body.contains(['\n', '\r']) {
            // 🔴 The refusal that gives this format its meaning. Splitting the
            // value across two rows would hand somebody a file whose row count
            // disagrees with the catalogue, silently.
            notes.push(format!(
                "{}: not printed - the value contains a line break, which this format cannot \
                 carry. Use --escaped, --base64, or --format json",
                value.reference
            ));
            continue;
        }
        out.push_str(&body);
        out.push('\n');
    }
    out
}

/// Standard base64, written out rather than taken from a crate.
///
/// Twenty lines against a dependency in an artifact meant for other people's
/// continuous integration - the same trade the JSON writer next door made, for
/// the same reason.
fn encode_base64(bytes: &[u8]) -> String {
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

    fn emission(values: Vec<EmittedValue>) -> Emission {
        let code_points = values.iter().map(|v| v.literal.chars().count()).sum();
        let bytes = values.iter().map(|v| v.literal.len()).sum();
        Emission {
            values,
            code_points,
            bytes,
            warnings: 0,
        }
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
    fn an_empty_emission_produces_a_header_only_csv_and_no_rows() {
        let rendered = render(&emission(Vec::new()), EmitFormat::Csv, None, false);
        assert_eq!(rendered.data, format!("{CSV_HEADER}\n"));
    }
}
