//! Does `parse` bring back everything the file says, or does it drop fields?
//!
//! # Why this guard exists, and why it exists BEFORE the fields it is missing
//!
//! `nkb emit --format json` is specified in pack-format.md 15 to print `since`
//! among other fields. It was found, while designing that command, that the
//! model does not carry `since` at all - even though the format requires the
//! field, the schema registers its kind, and every value in `packs/whitespace.toml`
//! sets it. Nothing noticed, because nothing was looking.
//!
//! That is the shape of fault this file exists to prevent from recurring. The
//! validator sees a field, the file carries it, the model silently does not, and
//! the loss only surfaces years later when some command needs to print it.
//!
//! # How it looks rather than assumes
//!
//! Rust has no reflection, so a hand-written list of "fields the model has"
//! would be a second list to keep in step with the first - and a session adding
//! a field could update both lists and still not touch the model.
//!
//! So the check goes the other way round. Each field in the sample pack carries
//! a CANARY: a value that appears nowhere else. The parsed model is then
//! rendered with `{:?}`, which prints every field a struct actually has, and the
//! canary is looked for in the result. A field the model does not hold cannot
//! print its canary, and a field the model holds but `parse` never fills cannot
//! either. Both failures are the same failure, and both turn this red.
//!
//! # What this does NOT catch
//!
//! - **A field read into the wrong place.** The canary appears somewhere in the
//!   output, so `since` landing in `name` would pass. Checking placement needs a
//!   getter per field, which is the hand-written list this design avoids.
//! - **A field the format defines and this sample forgets.** The sample is
//!   written by hand from the specification's tables, so it inherits their
//!   completeness rather than proving it.
//! - **Anything about the pack's own semantics.** This asks only whether the
//!   bytes survived the trip.

// A failed expectation in a test is a failed test.
#![allow(clippy::expect_used)]

use nkb_adapters::TomlPackFormat;
use nkb_app::ports::PackFormat;
use nkb_core::lint::Severity;

/// A pack that sets every field the format defines for a pack, a value and a
/// pair. Raw string: the escapes below belong to TOML, not to Rust.
const EVERY_FIELD: &str = r##"format = 1

[pack]
id          = "every-field"
name        = "Every field"
description = "A pack that sets every field the format defines, so that a dropped one is visible."
version     = "1.0"
updated     = 2026-09-08
license     = "CC-BY-4.0"
authors     = ["Naughty Keyboard"]
language    = "en"
tags        = ["parsing", "comparison"]
fields      = ["any"]
risk        = "normal"
source      = "https://example.invalid/pack-attribution-canary"

[[values]]
id          = "first-value"
name        = "First value"
type        = "literal"
value       = "Kowalski\u0020"
breaks      = "Login and e-mail comparisons differ between browser and server, so the account cannot be found."
expect      = "Trimmed everywhere or preserved everywhere, never one on sign-up and the other on sign-in."
fields      = ["username"]
tags        = ["trimming"]
risk        = "normal"
source      = "https://example.invalid/value-attribution-canary"
since       = "7.3"
shape       = "one canary mark at the end"
deprecated  = false

[[values]]
id          = "second-value"
name        = "Second value"
type        = "repeat"
unit        = "\u0020"
count       = 12
breaks      = "A run of spaces survives the form and collapses in the report, so two records stop matching."
expect      = "Either collapsed on the way in or preserved all the way through, and the same on both."
fields      = ["username"]
tags        = ["trimming"]
risk        = "normal"
source      = "https://example.invalid/value-attribution-canary"
since       = "7.5"
deprecated  = true
replaced_by = "first-value"

[[pairs]]
id       = "first-against-second"
name     = "First against second"
relation = "range"
a        = "first-value"
b        = "second-value"
breaks   = "One trailing space and twelve of them are handled by different code paths in most forms."
expect    = "The same trimming rule applies to one space and to twelve."
since    = "7.4"
"##;

/// Every canary, with the field it stands for. The message names the field, so
/// a failure says what was lost rather than that something was.
const CANARIES: [(&str, &str); 21] = [
    ("pack.id", "every-field"),
    ("pack.name", "Every field"),
    ("pack.description", "so that a dropped one is visible"),
    ("pack.version", "1.0"),
    ("pack.updated", "2026-09-08"),
    ("pack.license", "CC-BY-4.0"),
    ("pack.authors", "Naughty Keyboard"),
    ("pack.language", "en"),
    ("pack.tags", "comparison"),
    ("pack.fields", "any"),
    ("pack.source", "pack-attribution-canary"),
    ("value.id", "second-value"),
    ("value.name", "First value"),
    ("value.breaks", "cannot be found"),
    ("value.expect", "on sign-up and the other on sign-in"),
    ("value.source", "value-attribution-canary"),
    ("value.since", "7.3"),
    ("value.shape", "one canary mark at the end"),
    ("value.replaced_by", "first-value"),
    ("pair.relation", "range"),
    ("pair.since", "7.4"),
];

#[test]
fn the_sample_pack_is_accepted_so_this_test_measures_what_it_claims_to() {
    // The negative control. If the sample stopped being a valid pack, `parse`
    // would return nothing and every canary would go missing at once - which
    // would look exactly like the model losing all of its fields.
    let problems = TomlPackFormat.check(EVERY_FIELD, "every-field");
    let errors: Vec<_> = problems
        .iter()
        .filter(|p| p.severity() == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "the sample pack is not valid, so this file proves nothing: {errors:?}"
    );
}

#[test]
fn parsing_a_pack_keeps_every_field_the_format_defines() {
    let pack = TomlPackFormat
        .parse(EVERY_FIELD)
        .expect("the sample pack must parse");
    let rendered = format!("{pack:?}");

    let mut lost = Vec::new();
    for (field, canary) in CANARIES {
        if !rendered.contains(canary) {
            lost.push(field);
        }
    }

    assert!(
        lost.is_empty(),
        "parse() dropped {} of {} fields: {lost:?}\n\
         A field the file carries and the model does not is a silent loss - it \
         reaches nothing downstream and nobody finds out until a command needs \
         to print it.",
        lost.len(),
        CANARIES.len()
    );
}

#[test]
fn a_missing_canary_really_is_detected() {
    // Proves the mechanism rather than trusting it: a canary that is not in the
    // sample must be reported as lost. Without this, a `contains` that always
    // returned true would make the test above pass for the wrong reason.
    let pack = TomlPackFormat
        .parse(EVERY_FIELD)
        .expect("the sample pack must parse");
    let rendered = format!("{pack:?}");
    assert!(
        !rendered.contains("a-canary-that-is-nowhere-in-the-sample"),
        "the search finds text that is not there, so every other assertion here \
         is meaningless"
    );
}
