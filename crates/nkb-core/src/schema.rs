//! What kind of value each field of the format holds.
//!
//! # Why one table and not one per section
//!
//! A field name has **one kind wherever it appears**. `id` is text in a pack and
//! text in a value, `tags` is a list in both, `since` and `version` are both
//! text. That is a property worth keeping rather than an accident worth
//! preserving: a format where `risk` means text in one table and a number in
//! another is a format nobody can read without a lookup, and over years somebody
//! will get the lookup wrong.
//!
//! So the table below is flat, and the test at the bottom is what stops a later
//! field from quietly breaking the property.
//!
//! # Why this lives in the core rather than beside the parser
//!
//! Two layers need it and they need it for different reasons. The adapter asks
//! "does this item match", which needs a parser. The command line asks "what
//! should it have been", which needs words. Putting the answer in one place is
//! what keeps the two from drifting - the same shape already used for the
//! identifier patterns.

/// The kinds of value the format uses. Deliberately few: a format with eleven
/// kinds is a format that needs a parser to read the specification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    Text,
    WholeNumber,
    Date,
    List,
    TrueOrFalse,
}

impl FieldKind {
    /// Named for a person rather than for a parser. `E009` says what a field
    /// should have been, and "a whole number" tells a contributor more than
    /// `integer` does.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::WholeNumber => "a whole number",
            Self::Date => "a date",
            Self::List => "a list",
            Self::TrueOrFalse => "true or false",
        }
    }
}

use FieldKind::{Date, List, Text, TrueOrFalse, WholeNumber};

/// Every field the format defines, and the kind it holds.
///
/// `format` is absent on purpose: E001 owns it. A file declaring `format = "1"`
/// is refused by the rule about the format version, which arrives with the
/// better message, and two codes for one mistake help nobody.
const FIELDS: [(&str, FieldKind); 22] = [
    ("id", Text),
    ("name", Text),
    ("description", Text),
    ("version", Text),
    ("updated", Date),
    ("license", Text),
    ("authors", List),
    ("language", Text),
    ("tags", List),
    ("fields", List),
    ("risk", Text),
    ("source", Text),
    ("value", Text),
    ("breaks", Text),
    ("expect", Text),
    ("since", Text),
    ("shape", Text),
    ("deprecated", TrueOrFalse),
    ("replaced_by", Text),
    ("type", Text),
    ("unit", Text),
    ("count", WholeNumber),
];

/// The kind a field holds, or nothing for a name the format does not define.
///
/// Nothing rather than a guess: an unknown field is a different question, with
/// no rule of its own yet, and answering it here would turn one silence into a
/// wrong answer.
#[must_use]
pub fn kind_of(field: &str) -> Option<FieldKind> {
    FIELDS
        .iter()
        .find(|(name, _)| *name == field)
        .map(|(_, kind)| *kind)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn the_fields_the_format_documents_are_all_here() {
        // Taken from the two data model tables of the specification. A field
        // missing from this list is a field E009 cannot judge, silently.
        for field in [
            "id",
            "name",
            "description",
            "version",
            "updated",
            "license",
            "authors",
            "language",
            "value",
            "breaks",
            "expect",
            "since",
            "count",
            "unit",
            "type",
            "deprecated",
            "replaced_by",
        ] {
            assert!(kind_of(field).is_some(), "{field}");
        }
    }

    #[test]
    fn a_field_name_carries_one_kind_wherever_it_appears() {
        // The property the flat table exists to keep. A second entry for a name
        // already listed would make the answer depend on search order.
        let mut seen: HashMap<&str, FieldKind> = HashMap::new();
        for (name, kind) in FIELDS {
            assert!(
                seen.insert(name, kind).is_none(),
                "{name} is listed twice, so its kind depends on search order"
            );
        }
        assert_eq!(seen.len(), FIELDS.len());
    }

    #[test]
    fn the_format_version_is_not_here_because_another_rule_owns_it() {
        // E001 refuses a file whose `format` is not a number this build reads,
        // and it says which version it found. E009 repeating that would be one
        // mistake wearing two codes.
        assert_eq!(kind_of("format"), None);
    }

    #[test]
    fn a_field_the_format_does_not_define_has_no_kind() {
        assert_eq!(kind_of("braeks"), None);
        assert_eq!(kind_of(""), None);
    }

    #[test]
    fn the_kinds_are_named_for_a_person_not_for_a_parser() {
        assert_eq!(FieldKind::WholeNumber.as_str(), "a whole number");
        assert_eq!(FieldKind::Text.as_str(), "text");
    }
}
