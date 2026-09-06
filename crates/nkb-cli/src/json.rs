//! Writing JSON, in the small amount this tool needs and no more.
//!
//! # Why this is written here rather than taken from a library
//!
//! The output is one shape, produced in one direction, and never read back.
//! Against that, a serialisation library brings a dependency tree and a set of
//! derive macros into the artifact that is meant to go into other people's
//! continuous integration. The trade is not worth it for four kinds of value.
//!
//! # What it has to survive
//!
//! Everything here may end up carrying text taken from a pack file, and a pack
//! file is a catalogue of values chosen because they break parsers. A writer
//! that produced invalid JSON for its own subject matter would be the joke this
//! project cannot afford, so the escaping below is the part that matters and the
//! part that is tested hardest.
//!
//! # Order is part of the contract
//!
//! Members are held in a list rather than a map, so two runs over one file
//! produce the same bytes. A contribution check that stores its own output and
//! compares it later needs that, and a map ordered by hash would take it away
//! for no gain.

use std::fmt::Write as _;

/// A JSON value, in the four shapes this tool emits plus null.
#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Number(i64),
    Text(String),
    Array(Vec<Json>),
    /// Members in written order. Not a map: order is part of the contract.
    Object(Vec<(String, Json)>),
}

impl Json {
    /// Convenience for the common case of a borrowed string.
    #[must_use]
    pub fn text(value: impl Into<String>) -> Self {
        Self::Text(value.into())
    }

    /// A count, which is always small and always fits.
    #[must_use]
    pub fn count(value: usize) -> Self {
        Self::Number(i64::try_from(value).unwrap_or(i64::MAX))
    }

    /// Renders the value with two-space indentation.
    ///
    /// Indented rather than packed onto one line because the first reader of
    /// this output is usually a person looking at a build log, and the second is
    /// a script, which does not care either way.
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = String::new();
        self.write(&mut out, 0);
        out.push('\n');
        out
    }

    fn write(&self, out: &mut String, depth: usize) {
        match self {
            Self::Null => out.push_str("null"),
            Self::Bool(value) => out.push_str(if *value { "true" } else { "false" }),
            Self::Number(value) => {
                // Writing through the formatter rather than pushing a built
                // string keeps this allocation-free, and an integer cannot fail
                // to format.
                let _ = write!(out, "{value}");
            }
            Self::Text(value) => write_string(out, value),
            Self::Array(items) => write_block(out, depth, '[', ']', items.len(), |out, index| {
                items[index].write(out, depth + 1);
            }),
            Self::Object(members) => {
                write_block(out, depth, '{', '}', members.len(), |out, index| {
                    let (key, value) = &members[index];
                    write_string(out, key);
                    out.push_str(": ");
                    value.write(out, depth + 1);
                });
            }
        }
    }
}

/// Shared shape of an array and an object: open, indented items, close.
fn write_block(
    out: &mut String,
    depth: usize,
    open: char,
    close: char,
    len: usize,
    mut item: impl FnMut(&mut String, usize),
) {
    if len == 0 {
        // An empty collection on one line. Three lines to say "nothing here" is
        // noise in a log somebody has to scroll.
        out.push(open);
        out.push(close);
        return;
    }

    out.push(open);
    for index in 0..len {
        if index > 0 {
            out.push(',');
        }
        out.push('\n');
        indent(out, depth + 1);
        item(out, index);
    }
    out.push('\n');
    indent(out, depth);
    out.push(close);
}

fn indent(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push_str("  ");
    }
}

/// Writes a JSON string, escaping what the format requires and two characters
/// it does not.
///
/// The two extras are the line and paragraph separators. Both are valid inside a
/// JSON string and both terminate a statement in JavaScript, so a consumer that
/// pastes this output into a script gets a syntax error from a value that was
/// perfectly legal here. Escaping them costs nothing and removes a trap that
/// this catalogue is more likely than most to walk into.
///
/// Invisible characters that are not control characters - zero width spaces,
/// joiners, direction marks - are written out as themselves. JSON carries them
/// faithfully, and the point of this output is to hand over what the file holds
/// rather than a cleaned-up version of it.
fn write_string(out: &mut String, value: &str) {
    out.push('"');
    for character in value.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{0008}' => out.push_str("\\b"),
            '\u{000C}' => out.push_str("\\f"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            other if (other as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", other as u32);
            }
            other => out.push(other),
        }
    }
    out.push('"');
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;

    fn rendered(value: &Json) -> String {
        value.render().trim_end().to_owned()
    }

    #[test]
    fn the_four_scalars_render_as_json_says_they_should() {
        assert_eq!(rendered(&Json::Null), "null");
        assert_eq!(rendered(&Json::Bool(true)), "true");
        assert_eq!(rendered(&Json::Bool(false)), "false");
        assert_eq!(rendered(&Json::Number(-42)), "-42");
        assert_eq!(rendered(&Json::text("plain")), "\"plain\"");
    }

    #[test]
    fn a_quote_and_a_backslash_survive_intact() {
        // The first two values anybody would break a hand-written writer with.
        assert_eq!(rendered(&Json::text("a\"b")), "\"a\\\"b\"");
        assert_eq!(rendered(&Json::text("a\\b")), "\"a\\\\b\"");
    }

    #[test]
    fn a_value_that_looks_like_an_escape_is_not_turned_into_one() {
        // Six characters that spell an escape sequence. A writer that passed the
        // backslash through would hand the consumer one invisible character
        // instead - which is the exact confusion this project exists to find.
        assert_eq!(rendered(&Json::text("\\u200B")), "\"\\\\u200B\"");
    }

    #[test]
    fn control_characters_are_escaped_because_json_forbids_them_raw() {
        assert_eq!(rendered(&Json::text("a\nb")), "\"a\\nb\"");
        assert_eq!(rendered(&Json::text("a\rb")), "\"a\\rb\"");
        assert_eq!(rendered(&Json::text("a\tb")), "\"a\\tb\"");
        assert_eq!(rendered(&Json::text("a\u{0008}b")), "\"a\\bb\"");
        assert_eq!(rendered(&Json::text("a\u{000C}b")), "\"a\\fb\"");
        // One with no short form of its own.
        assert_eq!(rendered(&Json::text("a\u{0001}b")), "\"a\\u0001b\"");
        assert_eq!(rendered(&Json::text("a\u{001F}b")), "\"a\\u001fb\"");
    }

    #[test]
    fn the_two_separators_that_break_javascript_are_escaped_although_json_allows_them() {
        // Legal in a JSON string, and a statement terminator in JavaScript. A
        // consumer pasting this into a script would get a syntax error from a
        // value that was never wrong.
        assert_eq!(rendered(&Json::text("a\u{2028}b")), "\"a\\u2028b\"");
        assert_eq!(rendered(&Json::text("a\u{2029}b")), "\"a\\u2029b\"");
    }

    #[test]
    fn an_invisible_character_is_carried_rather_than_cleaned_up() {
        // A zero width space is not a control character and JSON carries it. The
        // job of this output is to hand over what the file holds, not a tidier
        // version - and the consumer asked for the data, not for an opinion.
        let rendered = rendered(&Json::text("ab\u{200B}cd"));
        assert!(rendered.contains('\u{200B}'), "{rendered:?}");
    }

    #[test]
    fn text_outside_the_basic_plane_survives() {
        assert_eq!(rendered(&Json::text("\u{1F468}")), "\"\u{1F468}\"");
    }

    #[test]
    fn an_empty_collection_stays_on_one_line() {
        assert_eq!(rendered(&Json::Array(Vec::new())), "[]");
        assert_eq!(rendered(&Json::Object(Vec::new())), "{}");
    }

    #[test]
    fn an_object_keeps_the_order_it_was_written_in() {
        // Two runs over one file must produce the same bytes, or a contribution
        // check comparing its own output turns red for no reason.
        let value = Json::Object(vec![
            ("zebra".to_owned(), Json::Number(1)),
            ("alpha".to_owned(), Json::Number(2)),
        ]);
        let text = rendered(&value);
        let zebra = text.find("zebra").expect("present");
        let alpha = text.find("alpha").expect("present");
        assert!(zebra < alpha, "written order must survive: {text}");
    }

    #[test]
    fn nesting_indents_by_two_spaces_per_level() {
        let value = Json::Object(vec![(
            "outer".to_owned(),
            Json::Array(vec![Json::Object(vec![(
                "inner".to_owned(),
                Json::Bool(true),
            )])]),
        )]);
        let expected = concat!(
            "{\n",
            "  \"outer\": [\n",
            "    {\n",
            "      \"inner\": true\n",
            "    }\n",
            "  ]\n",
            "}"
        );
        assert_eq!(rendered(&value), expected);
    }

    #[test]
    fn the_rendered_document_ends_with_a_newline() {
        // A file without a final newline is the kind of thing that makes a shell
        // prompt land in the middle of somebody's log line.
        assert!(Json::Bool(true).render().ends_with('\n'));
    }

    #[test]
    fn a_key_is_escaped_the_same_way_a_value_is() {
        // Keys come from our own code today, and a rule not yet written could
        // put a name from a pack file here. Escaping only values would be a trap
        // set for a later change.
        let value = Json::Object(vec![("a\"b".to_owned(), Json::Null)]);
        assert!(rendered(&value).contains("\"a\\\"b\""), "{value:?}");
    }
}
