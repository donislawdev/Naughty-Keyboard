//! What `nkb boundary` prints: the same three shapes as `nkb emit`, written
//! one value at a time.
//!
//! # Why one value at a time
//!
//! A limit of a hundred thousand makes a document of tens of megabytes. Held
//! whole - the values, then the tree of the document, then its text - that is
//! several times its size in memory for a moment, to print something that can
//! be printed as it is made. So the account is counted from the recipes first,
//! and then each value is built, written and let go. A reader that closed the
//! pipe stops the building too.
//!
//! # A document of its own
//!
//! The JSON here is a third contract beside the linter's and the one of `nkb
//! emit`, with a `schema` that moves on its own. The values have no pack, so
//! none of the pack's fields are here, and they have what a catalogue value
//! does not: the limit they answer, their length in its unit, whether that
//! length is inside it, and their size in all four units. `value` and
//! `escaped` are named as `nkb emit` names them, so a script that reads those
//! two reads both documents.

use crate::emit_output::{EmitFormat, Form, csv_cell, encode_base64, not_a_formula};
use crate::english::plural;
use crate::json::Json;
use nkb_adapters::boundary_text::value_name;
use nkb_core::boundary::{BoundaryValue, Limit};
use nkb_core::text::LiteralText;

/// Version of the JSON THIS command prints. An axis of its own: it says
/// nothing about the linter's schema or the one of `nkb emit`, and is raised
/// only when a member is removed or changes its meaning.
const SCHEMA: i64 = 1;

const CSV_HEADER: &str = "id,name,value,fill,length,within,graphemes,code_points,utf16_units,bytes";

/// How the values are to come out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Shape {
    pub(super) format: EmitFormat,
    pub(super) form: Option<Form>,
    pub(super) base64: bool,
}

/// What standard error is told before the first value: how much is coming,
/// counted from the recipes, so it is said before anything is built.
pub(super) fn notes(values: &[BoundaryValue], shape: Shape) -> Vec<String> {
    let mut code_points = 0usize;
    let mut bytes = 0usize;
    for value in values {
        let sizes = value.sizes();
        code_points = code_points.saturating_add(sizes.code_points);
        bytes = bytes.saturating_add(sizes.bytes);
    }
    let mut notes = vec![format!(
        "{}, {}, {}",
        plural(values.len(), "value"),
        plural(code_points, "code point"),
        plural(bytes, "byte")
    )];
    if shape.base64 {
        notes.push("values are base64 encoded".to_owned());
    }
    notes
}

/// Writes the values through `put`, piece by piece. `put` answers whether
/// there is still somebody to write for, and the first no ends the work.
pub(super) fn write(
    limit: Limit,
    values: &[BoundaryValue],
    shape: Shape,
    mut put: impl FnMut(&str) -> bool,
) {
    let form = shape.form.unwrap_or_else(|| shape.format.default_form());
    let opening = match shape.format {
        EmitFormat::Json => Json::open_stream(&head(limit), "values"),
        EmitFormat::Csv => format!("{CSV_HEADER}\n"),
        EmitFormat::Lines => String::new(),
    };
    if !opening.is_empty() && !put(&opening) {
        return;
    }
    for (index, value) in values.iter().enumerate() {
        let text = Text::of(value);
        let piece = match shape.format {
            EmitFormat::Json => as_json(value, &text, shape.base64).stream_item(index == 0),
            EmitFormat::Csv => csv_row(value, &text, form, shape.base64),
            EmitFormat::Lines => format!("{}\n", text.body(form, shape.base64)),
        };
        if !put(&piece) {
            return;
        }
    }
    if shape.format == EmitFormat::Json {
        put(&Json::close_stream(!values.is_empty()));
    }
}

/// The members in front of the values: which document this is, and the limit
/// the values answer.
fn head(limit: Limit) -> Vec<(String, Json)> {
    let bound = |bound: Option<u32>| bound.map_or(Json::Null, |n| Json::Number(i64::from(n)));
    vec![
        ("schema".to_owned(), Json::Number(SCHEMA)),
        (
            "limit".to_owned(),
            Json::Object(vec![
                ("unit".to_owned(), Json::text(limit.unit().id())),
                ("min".to_owned(), bound(limit.min())),
                ("max".to_owned(), bound(limit.max())),
            ]),
        ),
    ]
}

/// One value built: the text as it would arrive in a field, and the same text
/// as a pack file would store it.
struct Text {
    literal: String,
    escaped: String,
}

impl Text {
    fn of(value: &BoundaryValue) -> Self {
        let literal = value.materialise();
        let escaped = LiteralText::new(literal.clone())
            .escape()
            .as_str()
            .to_owned();
        Self { literal, escaped }
    }

    fn body(&self, form: Form, base64: bool) -> String {
        let text = match form {
            Form::Literal => &self.literal,
            Form::Escaped => &self.escaped,
        };
        if base64 {
            encode_base64(text.as_bytes())
        } else {
            text.clone()
        }
    }
}

fn as_json(value: &BoundaryValue, text: &Text, base64: bool) -> Json {
    let sizes = value.sizes();
    Json::Object(vec![
        ("id".to_owned(), Json::text(value.id())),
        ("name".to_owned(), Json::text(value_name(value))),
        // Both forms, always, as in `nkb emit`.
        (
            "value".to_owned(),
            Json::Text(text.body(Form::Literal, base64)),
        ),
        (
            "escaped".to_owned(),
            Json::Text(text.body(Form::Escaped, base64)),
        ),
        (
            "fill".to_owned(),
            value
                .fill()
                .map_or(Json::Null, |fill| Json::text(fill.id())),
        ),
        ("length".to_owned(), Json::Number(i64::from(value.length()))),
        ("within".to_owned(), Json::Bool(value.within())),
        ("graphemes".to_owned(), Json::count(sizes.graphemes)),
        ("code_points".to_owned(), Json::count(sizes.code_points)),
        ("utf16_units".to_owned(), Json::count(sizes.utf16_units)),
        ("bytes".to_owned(), Json::count(sizes.bytes)),
    ])
}

fn csv_row(value: &BoundaryValue, text: &Text, form: Form, base64: bool) -> String {
    let sizes = value.sizes();
    let body = text.body(form, base64);
    let row = [
        value.id(),
        value_name(value),
        // No value here starts like a formula today. The same door as `nkb
        // emit` all the same, so a fill added later cannot be the first to.
        if form == Form::Escaped && !base64 {
            not_a_formula(&body)
        } else {
            body
        },
        value.fill().map_or("", |fill| fill.id()).to_owned(),
        value.length().to_string(),
        value.within().to_string(),
        sizes.graphemes.to_string(),
        sizes.code_points.to_string(),
        sizes.utf16_units.to_string(),
        sizes.bytes.to_string(),
    ];
    let cells: Vec<String> = row.iter().map(|cell| csv_cell(cell)).collect();
    format!("{}\n", cells.join(","))
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;
    use nkb_core::boundary::{Sizes, Unit, values};

    fn limit(unit: Unit, min: Option<u32>, max: Option<u32>) -> Limit {
        Limit::new(unit, min, max).expect("a limit in range")
    }

    fn shape(format: EmitFormat) -> Shape {
        Shape {
            format,
            form: None,
            base64: false,
        }
    }

    fn written(limit: Limit, shape: Shape) -> String {
        let mut out = String::new();
        write(limit, &values(limit), shape, |piece| {
            out.push_str(piece);
            true
        });
        out
    }

    /// The text of one member of the value called `id`, read out of the JSON
    /// by its own line - enough for a document this module indents itself.
    fn member(document: &str, id: &str, key: &str) -> String {
        let start = document
            .find(&format!("\"id\": \"{id}\""))
            .unwrap_or_else(|| panic!("no value called {id}"));
        let wanted = format!("\"{key}\": ");
        let line = document[start..]
            .lines()
            .find(|line| line.trim_start().starts_with(&wanted))
            .unwrap_or_else(|| panic!("{id} has no {key}"));
        line.trim_start()
            .trim_start_matches(&wanted)
            .trim_end_matches(',')
            .to_owned()
    }

    #[test]
    fn the_document_names_its_schema_and_the_limit_before_the_values() {
        let document = written(
            limit(Unit::Bytes, Some(3), Some(50)),
            shape(EmitFormat::Json),
        );
        assert!(
            document.starts_with(
                "{\n  \"schema\": 1,\n  \"limit\": {\n    \"unit\": \"bytes\",\n    \"min\": 3,\n    \
                 \"max\": 50\n  },\n  \"values\": [\n    {\n      \"id\": \"empty\","
            ),
            "{document}"
        );
        assert!(document.ends_with("\n    }\n  ]\n}\n"));
        let only_most = written(
            limit(Unit::Graphemes, None, Some(5)),
            shape(EmitFormat::Json),
        );
        assert!(only_most.contains("\"unit\": \"graphemes\",\n    \"min\": null,\n    \"max\": 5"));
    }

    #[test]
    fn a_value_says_its_fill_its_length_whether_it_is_within_and_its_four_sizes() {
        let document = written(
            limit(Unit::Graphemes, None, Some(50)),
            shape(EmitFormat::Json),
        );
        let of = |id: &str, key: &str| member(&document, id, key);
        assert_eq!(of("two-byte-50", "name"), "\"50 x U+0142\"");
        assert_eq!(of("two-byte-50", "fill"), "\"two-byte\"");
        assert_eq!(of("two-byte-50", "length"), "50");
        assert_eq!(of("two-byte-50", "within"), "true");
        assert_eq!(of("two-byte-50", "graphemes"), "50");
        assert_eq!(of("two-byte-50", "code_points"), "50");
        assert_eq!(of("two-byte-50", "utf16_units"), "50");
        assert_eq!(of("two-byte-50", "bytes"), "100");
        assert_eq!(of("two-byte-51", "within"), "false");
        assert_eq!(of("four-byte-50", "utf16_units"), "100");
        assert_eq!(of("cluster-50", "bytes"), "900");
        assert_eq!(of("empty", "fill"), "null");
        assert_eq!(of("empty", "value"), "\"\"");
        assert_eq!(of("empty", "within"), "true");
    }

    #[test]
    fn the_escaped_form_writes_the_joiner_of_the_cluster_and_nothing_else() {
        let document = written(
            limit(Unit::Graphemes, None, Some(1)),
            shape(EmitFormat::Json),
        );
        assert_eq!(
            member(&document, "cluster-1", "escaped"),
            "\"\u{1F468}\\\\u200D\u{1F469}\\\\u200D\u{1F467}\""
        );
        assert_eq!(member(&document, "two-byte-1", "escaped"), "\"\u{0142}\"");
    }

    #[test]
    fn csv_has_a_header_and_a_row_for_every_value() {
        let asked = limit(Unit::Bytes, None, Some(50));
        let csv = written(asked, shape(EmitFormat::Csv));
        let rows: Vec<&str> = csv.lines().collect();
        assert_eq!(rows[0], CSV_HEADER);
        assert_eq!(rows.len(), values(asked).len() + 1);
        assert_eq!(rows[1], "empty,Empty value,,,0,true,0,0,0,0");
        let over = format!(
            "two-byte-51,1 x a + 25 x U+0142,a{},two-byte,51,false,26,26,26,51",
            "\u{0142}".repeat(25)
        );
        assert!(rows.contains(&over.as_str()), "{csv}");
    }

    #[test]
    fn lines_is_one_value_a_line_and_the_empty_value_is_an_empty_line() {
        let asked = limit(Unit::Graphemes, None, Some(2));
        let lines = written(asked, shape(EmitFormat::Lines));
        let rows: Vec<&str> = lines.split('\n').collect();
        // One more piece than values: the text ends in a line break.
        assert_eq!(rows.len(), values(asked).len() + 1);
        assert_eq!(rows[0], "");
        assert_eq!(rows[1], "a");
        assert_eq!(rows[2], "aa");
        assert_eq!(rows[3], "aaa");
        assert_eq!(rows[4], "*");
        assert_eq!(rows[5], "2*");
        assert_eq!(rows[6], "*3*");
    }

    #[test]
    fn base64_changes_the_values_and_not_the_shape() {
        let asked = limit(Unit::Graphemes, None, Some(2));
        let encoded = Shape {
            base64: true,
            ..shape(EmitFormat::Json)
        };
        let document = written(asked, encoded);
        assert_eq!(member(&document, "letters-2", "value"), "\"YWE=\"");
        assert_eq!(member(&document, "letters-2", "escaped"), "\"YWE=\"");
        assert_eq!(member(&document, "letters-2", "bytes"), "2");
        assert_eq!(
            notes(&values(asked), encoded)[1],
            "values are base64 encoded"
        );
    }

    #[test]
    fn the_account_is_the_sum_of_what_gets_written() {
        let asked = limit(Unit::Utf16, Some(2), Some(9));
        let all = values(asked);
        let mut read = Sizes::of("");
        for value in &all {
            let one = Sizes::of(&value.materialise());
            read.code_points += one.code_points;
            read.bytes += one.bytes;
        }
        assert_eq!(
            notes(&all, shape(EmitFormat::Json)),
            [format!(
                "{} values, {} code points, {} bytes",
                all.len(),
                read.code_points,
                read.bytes
            )]
        );
    }

    #[test]
    fn the_account_of_at_most_fifty_is_the_one_the_website_quotes() {
        let all = values(limit(Unit::Graphemes, None, Some(50)));
        assert_eq!(
            notes(&all, shape(EmitFormat::Lines)),
            ["19 values, 1500 code points, 4350 bytes"]
        );
    }

    #[test]
    fn a_reader_that_left_ends_the_work_at_once() {
        for format in [EmitFormat::Json, EmitFormat::Csv, EmitFormat::Lines] {
            let asked = limit(Unit::Graphemes, None, Some(50));
            let mut pieces = 0;
            write(asked, &values(asked), shape(format), |_| {
                pieces += 1;
                pieces < 3
            });
            assert_eq!(pieces, 3, "{format:?}");
        }
    }
}
