//! The file `nkb new-pack` writes.
//!
//! # This is the format's public face
//!
//! There is no English specification of the pack format and there is
//! deliberately not going to be one (`decision-log.md` D20): a contributor meets
//! the format through the skeleton this module writes and the verdict `nkb lint`
//! gives. Everything a person needs to know before their first pull request has
//! to be visible in this file, because there is nowhere else to look.
//!
//! That makes the comments below part of the product rather than decoration.
//!
//! # It has to pass the validator, and there is a test that says so
//!
//! A skeleton the linter refuses would mean step one of the contributor path
//! hands somebody a file that step three rejects. Every rule added to the
//! validator is a constraint on this text, and rules get added - so
//! `skeleton_lints.rs` runs the real linter over the real output rather than
//! trusting that the two stayed in step.
//!
//! # Why it lives with the parser and not with the use case
//!
//! This is TOML. What TOML looks like is the one thing this layer knows and the
//! layer above deliberately does not.

// The date comes from the layer above rather than from a clock reached for here:
// a module that reads the current time cannot be tested against a fixed one.
pub use nkb_app::Date;

/// The skeleton for a new pack, ready to be written to `<id>.toml`.
///
/// The identifier is placed rather than fixed, because E010 requires the pack to
/// declare the same name its file carries.
#[must_use]
pub fn for_pack(id: &str, today: Date) -> String {
    let (year, month, day) = today;
    format!(
        "\
# A new pack, ready to edit. Three things to know before you start.
#
# 1. The sentence in `breaks` is the point. It says what this value usually
#    breaks in an application, and why. Everything else here is bookkeeping -
#    that sentence is the reason a catalogue is worth more than a list, and it
#    is the only part nobody can write for you.
#
# 2. A character that cannot be seen is written escaped, never literally. A
#    trailing space is `{backslash}u0020`, a non-breaking space `{backslash}u00A0`. Written out, it
#    survives one editor and vanishes in the next, and the value silently stops
#    testing what it says it tests.
#
# 3. Run `nkb lint {id}.toml` when you are done. It reports everything it finds
#    in one pass, and it also tells you which rules it could not check.
#
# The pack format is not frozen yet: it freezes with the first public release
# that ships packs. Until then a field may still change, and this file says so
# rather than letting silence read as stability.

format = 1

[pack]
id          = \"{id}\"
name        = \"Give this pack a short name\"
description = \"One or two sentences: what this pack is for, and what kind of application it is aimed at.\"
version     = \"1.0\"
updated     = {year:04}-{month:02}-{day:02}
license     = \"CC-BY-4.0\"
authors     = [\"Your name or handle\"]
language    = \"en\"
tags        = [\"replace\", \"these\"]
fields      = [\"any\"]
risk        = \"normal\"

# One example value, to be replaced. Copy this block for each further value, and
# remember that the order here is the order a tester meets them: the strongest
# values belong at the top.
[[values]]
id     = \"trailing-space\"
name   = \"Trailing space\"
value  = \"Kowalski{backslash}u0020\"
breaks = \"Login and e-mail comparisons differ between the browser and the server, so the account is created and then cannot be found again.\"
expect = \"Trimmed everywhere or preserved everywhere - never one on sign-up and the other on sign-in.\"
since  = \"1.0\"
",
        backslash = '\\',
    )
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;

    const TODAY: Date = (2026, 9, 7);

    #[test]
    fn the_date_is_written_the_way_toml_reads_a_date() {
        // Zero padded and unquoted. Quoted, it would be text in a field the
        // format declares as a date, and E009 would refuse the file this command
        // just produced.
        let text = for_pack("locale-cz", (2026, 1, 3));
        assert!(text.contains("updated     = 2026-01-03"), "{text}");
        assert!(!text.contains("updated     = \""), "the date is not text");
    }

    #[test]
    fn the_escape_in_the_example_is_written_as_characters_and_not_as_the_character() {
        // 🔴 The mistake this file exists to teach, made inside the file that
        // teaches it. Six ASCII characters, not one invisible one - if this ever
        // renders as a real space, the skeleton demonstrates the opposite of what
        // its own comment says.
        let text = for_pack("locale-cz", TODAY);
        assert!(text.contains("Kowalski\\u0020"), "{text}");
        assert!(
            !text.contains("Kowalski \""),
            "the escape expanded into a real space"
        );
    }

    #[test]
    fn every_line_of_prose_a_contributor_reads_is_present() {
        // The three things somebody has to know before their first pull request.
        // Losing one to a tidy-up is losing the only place it is written down.
        let text = for_pack("locale-cz", TODAY);
        for expected in [
            "The sentence in `breaks` is the point",
            "written escaped, never literally",
            "nkb lint locale-cz.toml",
            "not frozen yet",
        ] {
            assert!(text.contains(expected), "missing: {expected}");
        }
    }
}
