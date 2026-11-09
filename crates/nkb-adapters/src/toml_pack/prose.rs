//! E035 and E036: the prose of a pack, judged after parsing (`D174`).
//!
//! Out of `toml_pack.rs` for the reason `body.rs` is (`D113`): the rules live in
//! `nkb_core::prose`, and this file only walks the tables a pack file has and
//! says where each finding is. A pack, a value, a pair, and in a translation the
//! root and every entry - each passes through the one function below.

use nkb_core::lint::{LintProblem, RuleCode};
use nkb_core::prose::{self, ProseFault};
use nkb_core::source_text::SourceText;
use nkb_core::text::escaped_prose;
use toml_edit::TableLike;

use super::span_line;

/// E035 and E036 over the prose fields of one table.
///
/// A list (`authors`, `tags`) is judged item by item, so a finding is about one
/// author rather than about all of them added together. A field of a kind the
/// format cannot read is passed over: E009 has said so already, and the
/// elements of a list are not judged for their kind by any rule, so an element
/// that is not text is not prose anybody is shown.
pub(super) fn check_prose(text: &SourceText<'_>, table: &dyn TableLike) -> Vec<LintProblem> {
    let mut problems = Vec::new();
    for (field, item) in table.iter() {
        let Some(limit) = prose::limit_of(field) else {
            continue;
        };
        let pieces: Vec<&str> = match item.as_array() {
            Some(list) => list.iter().filter_map(|element| element.as_str()).collect(),
            None => item.as_str().into_iter().collect(),
        };
        let line = span_line(text, item);
        for piece in pieces {
            problems.extend(
                prose::check(piece, limit)
                    .into_iter()
                    .map(|fault| finding(field, fault).at(line)),
            );
        }
    }
    problems
}

/// One finding, its subject naming the field and what was found in it:
/// `breaks: \u202E \u000A` for E035 (the characters in the notation of
/// `pack-format.md` 2, an ellipsis when there are more than are named) and
/// `description: 3200000` for E036 (the length in code points).
///
/// Like E018, which names the field together with what it holds, the subject
/// stays text taken from the file rather than a sentence, and `--json` keeps its
/// schema.
fn finding(field: &str, fault: ProseFault) -> LintProblem {
    match fault {
        ProseFault::Unseen { named, more } => {
            let mut subject = format!("{field}:");
            for character in named {
                subject.push(' ');
                subject.push_str(&escaped_prose(&character.to_string()));
            }
            if more {
                subject.push_str(" \u{2026}");
            }
            LintProblem::new(RuleCode::UnseenCharacterInProse).about(subject)
        }
        ProseFault::TooLong { code_points, .. } => {
            LintProblem::new(RuleCode::ProseTooLong).about(format!("{field}: {code_points}"))
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use crate::toml_pack::check;
    use nkb_core::lint::{LintProblem, RuleCode};

    const PACK_ID: &str = "probe";

    /// A pack that passes every rule, with one line replaced by the caller.
    fn pack(name: &str, breaks: &str, tags: &str) -> String {
        format!(
            "format = 1\n\n[pack]\nid = \"probe\"\nname = {name}\ndescription = \"A pack written to be judged.\"\nversion = \"1.0\"\nupdated = 2026-11-09\nlicense = \"CC-BY-4.0\"\nauthors = [\"Naughty Keyboard\"]\nlanguage = \"en\"\ntags = {tags}\n\n[[values]]\nid = \"first\"\nname = \"First\"\nvalue = \"first\"\nbreaks = {breaks}\nexpect = \"Stored and shown exactly as it was typed.\"\nsince = \"1.0\"\n"
        )
    }

    const NAME: &str = "\"Probe\"";
    const BREAKS: &str =
        "\"Sorting and searching treat the record as a different one than the tester typed.\"";
    const TAGS: &str = "[\"probe\"]";

    fn prose_problems(text: &str) -> Vec<LintProblem> {
        check(text, PACK_ID)
            .into_iter()
            .filter(|p| {
                matches!(
                    p.code,
                    RuleCode::UnseenCharacterInProse | RuleCode::ProseTooLong
                )
            })
            .collect()
    }

    #[test]
    fn the_good_pack_has_no_prose_findings() {
        assert!(prose_problems(&pack(NAME, BREAKS, TAGS)).is_empty());
    }

    #[test]
    fn an_escape_in_the_file_is_the_same_character_once_read() {
        // The name holds ESC written as an escape: the file is plain ASCII, and
        // the pack still says `[31m` to every terminal that prints its name.
        let text = pack("\"Probe \\u001B[31m red\"", BREAKS, TAGS);
        let found = prose_problems(&text);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].code, RuleCode::UnseenCharacterInProse);
        assert_eq!(found[0].owner.as_deref(), Some("pack"));
        assert_eq!(found[0].subject.as_deref(), Some(r"name: \u001B"));
        assert_eq!(found[0].line, Some(5), "the line of the name");
    }

    #[test]
    fn a_value_names_the_characters_its_description_holds() {
        let breaks = "\"Shown \\u202Ereversed\\u202C and \\n split over lines, so a reviewer reads another sentence.\"";
        let found = prose_problems(&pack(NAME, breaks, TAGS));
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].owner.as_deref(), Some("first"));
        assert_eq!(
            found[0].subject.as_deref(),
            Some(r"breaks: \u202E \u202C \u000A")
        );
    }

    #[test]
    fn a_list_is_judged_item_by_item() {
        let tags = "[\"probe\", \"no\\u00A0break\"]";
        let found = prose_problems(&pack(NAME, BREAKS, tags));
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].subject.as_deref(), Some(r"tags: \u00A0"));
    }

    #[test]
    fn the_limit_is_allowed_and_one_more_is_not() {
        let at = format!("\"{}\"", "a".repeat(nkb_core::prose::NAME_LIMIT));
        assert!(prose_problems(&pack(&at, BREAKS, TAGS)).is_empty());

        let over = format!("\"{}\"", "a".repeat(nkb_core::prose::NAME_LIMIT + 1));
        let found = prose_problems(&pack(&over, BREAKS, TAGS));
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].code, RuleCode::ProseTooLong);
        assert_eq!(found[0].subject.as_deref(), Some("name: 121"));
    }

    #[test]
    fn a_pair_is_judged_like_a_value() {
        let text = format!(
            "{}\n[[values]]\nid = \"second\"\nname = \"Second\"\nvalue = \"second\"\nbreaks = {BREAKS}\nexpect = \"Stored and shown exactly as it was typed.\"\nsince = \"1.0\"\n\n[[pairs]]\nid = \"both\"\nname = \"Both\\u200B\"\nrelation = \"identity\"\na = \"first\"\nb = \"second\"\nsince = \"1.0\"\n",
            pack(NAME, BREAKS, TAGS)
        );
        let found = prose_problems(&text);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].owner.as_deref(), Some("both"));
        assert_eq!(found[0].subject.as_deref(), Some(r"name: \u200B"));
    }

    #[test]
    fn a_translation_is_judged_at_its_root_its_pack_and_every_entry() {
        let text = format!(
            "format = 1\ntranslates = \"probe\"\nlanguage = \"{}\"\n\n[pack]\nname = \"Sonda\\u2067\"\n\n[values.first]\nbreaks = \"Sortowanie \\u001B[2J i wyszukiwanie traktuje rekord jako inny.\"\n",
            "p".repeat(nkb_core::prose::OTHER_TEXT_LIMIT + 1)
        );
        let found = check(&text, "probe.pl");
        let prose: Vec<(Option<&str>, Option<&str>)> = found
            .iter()
            .filter(|p| {
                matches!(
                    p.code,
                    RuleCode::UnseenCharacterInProse | RuleCode::ProseTooLong
                )
            })
            .map(|p| (p.owner.as_deref(), p.subject.as_deref()))
            .collect();
        assert_eq!(
            prose,
            vec![
                (None, Some("language: 301")),
                (Some("pack"), Some(r"name: \u2067")),
                (Some("first"), Some(r"breaks: \u001B")),
            ],
            "{found:?}"
        );
    }

    #[test]
    fn data_identifiers_and_fields_of_the_wrong_kind_are_left_to_their_own_rules() {
        // The value itself may hold anything: that is E020's ground, not prose.
        let text =
            pack(NAME, BREAKS, TAGS).replace("value = \"first\"", "value = \"first\\u202E\"");
        assert!(prose_problems(&text).is_empty(), "the value is data");

        // An identifier with an override is E012's finding: one fault, one code.
        let id = pack(NAME, BREAKS, TAGS).replace("id = \"first\"", "id = \"first\\u202E\"");
        let codes: Vec<&str> = check(&id, PACK_ID)
            .iter()
            .map(|p| p.code.as_str())
            .collect();
        assert!(codes.contains(&"E012"), "{codes:?}");
        assert!(prose_problems(&id).is_empty(), "{codes:?}");

        // A description of the wrong kind is E009's finding and nobody's prose.
        let wrong = pack(NAME, "42", TAGS);
        let codes: Vec<&str> = check(&wrong, PACK_ID)
            .iter()
            .map(|p| p.code.as_str())
            .collect();
        assert!(codes.contains(&"E009"), "{codes:?}");
        assert!(prose_problems(&wrong).is_empty(), "{codes:?}");
    }
}
