//! Turning a listing and a pack into the lines a person reads.
//!
//! # Nothing here builds a value
//!
//! A generated value is shown as its RECIPE - `" " x 1000` - and never as
//! the thousand characters it stands for. That is `architektura.md` 6.1, and it
//! is the same requirement arriving from two directions: the size has to be
//! known before the text exists, or the promised warning about a length bomb
//! comes after the bomb. And the preview would freeze on exactly the values it
//! was built to show.
//!
//! A literal value is shown ESCAPED, which is the only form that can be read at
//! all when the value is made of characters nobody can see. Escaped from the
//! parsed text rather than copied out of the file, so what is printed describes
//! the value rather than the author's spelling of it.
//!
//! # No colour, no width guessing
//!
//! Document 09 forbids colour and progress when the output is not a terminal,
//! and the simplest way to obey a rule about a condition is not to have the
//! behaviour the condition applies to.

use crate::english::plural;
use nkb_app::{CatalogueSource, Listing, PackEntry, SourceError, SourceSkipped};
use nkb_core::metrics::TextMetrics;
use nkb_core::next_date::NextDate;
use nkb_core::pack::{Pack, PackValue};
use nkb_core::value::{Definition, ValueBody};

/// How a value would be written in a pack file: the recipe, or the escaped text.
///
/// The one place both shapes are rendered, so that `nkb show` and anything after
/// it cannot drift into describing the same value two ways. A `next-date` value
/// is shown as its rule, never as today's date: `nkb show` describes the pack,
/// and the pack says "the next one", not a day.
#[must_use]
pub fn written_form(definition: &Definition) -> String {
    match definition {
        Definition::Fixed(ValueBody::Literal(text)) => text.escape().as_str().to_owned(),
        Definition::Fixed(ValueBody::Repeat { unit, count }) => {
            format!("{} x {count}", unit.escape().as_str())
        }
        Definition::NextDate(rule) => rule_in_words(rule),
    }
}

/// A `next-date` rule as an English sentence: `next date: the last Sunday of
/// March, 02:30`.
fn rule_in_words(rule: &NextDate) -> String {
    const MONTHS: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    let month = usize::try_from(rule.month())
        .ok()
        .and_then(|month| MONTHS.get(month.checked_sub(1)?))
        .copied()
        .unwrap_or("?");
    // The vocabulary words of the format, with the weekday capitalised the way
    // English writes it.
    let weekday = rule.weekday().name();
    let mut capitalised = weekday.chars();
    let weekday: String = capitalised
        .next()
        .map(|first| first.to_ascii_uppercase())
        .into_iter()
        .chain(capitalised)
        .collect();
    let mut words = format!("next date: the {} {weekday} of {month}", rule.week().name());
    if let Some(time) = rule.time() {
        words.push_str(", ");
        words.push_str(&time.written());
    }
    words
}

/// The size a value would have, from its recipe.
fn metrics_of(definition: &Definition) -> Option<TextMetrics> {
    definition.metrics()
}

/// One line describing what the run did and did not look at.
///
/// 🔴 Printed on every run of `nkb packs`, not only when something is missing.
/// A listing assembled from one source of three is indistinguishable from a
/// complete one, and the person reading it is about to draw a false conclusion
/// about their own installation. Untouchable rule 1.
#[must_use]
pub fn coverage_lines(listing: &Listing) -> Vec<String> {
    let mut lines = Vec::new();

    let consulted: Vec<&str> = listing
        .coverage
        .consulted
        .iter()
        .map(|source| source.as_str())
        .collect();
    lines.push(if consulted.is_empty() {
        "Read no pack source at all.".to_owned()
    } else {
        format!("Read these pack sources: {}.", consulted.join(", "))
    });

    for (source, reason) in &listing.coverage.skipped {
        lines.push(format!(
            "Not read: {} - {}.",
            source.as_str(),
            explain_skip(*source, *reason)
        ));
    }
    lines
}

/// Why a source went unread, in words rather than in a marker.
fn explain_skip(source: CatalogueSource, reason: SourceSkipped) -> &'static str {
    match reason {
        // Named for what it is. "Not configured" would be a lie here: there is
        // nowhere to configure it yet, and somebody would go looking for the
        // setting.
        SourceSkipped::NotImplementedYet => match source {
            CatalogueSource::BuiltIn => "this build carries none",
            CatalogueSource::Team | CatalogueSource::Own => {
                "the settings file has no key for this folder yet, so it cannot be named"
            }
        },
        SourceSkipped::NotConfigured => "no folder is configured for it",
        SourceSkipped::Unreadable => "the folder is configured and could not be read",
    }
}

fn source_error(reason: SourceError) -> String {
    match reason {
        SourceError::NotFound => "the file is gone".to_owned(),
        SourceError::Unreadable => "the file would not open".to_owned(),
        SourceError::NotUtf8 => "the file is not valid UTF-8, which the format requires".to_owned(),
        SourceError::TooLarge => format!(
            "the file is larger than {}, the most a pack file may be",
            crate::lint_report::pack_file_limit()
        ),
    }
}

/// The whole of `nkb packs`.
#[must_use]
pub fn listing(listing: &Listing) -> Vec<String> {
    let mut lines = Vec::new();
    let mut values = 0;

    for entry in &listing.entries {
        match entry {
            PackEntry::Loaded { pack, warnings } => {
                values += pack.values.len();
                let mut note = String::new();
                if *warnings > 0 {
                    note.push_str(&format!("  ({warnings} warnings)"));
                }
                if pack.carries_offensive() {
                    note.push_str("  (offensive)");
                }
                lines.push(format!(
                    "{:<24}{:>4} values  {}{note}",
                    pack.id,
                    pack.values.len(),
                    pack.name
                ));
            }
            // A refused pack keeps its line. Dropping it would render "present
            // and broken" as "absent", and those need different repairs.
            PackEntry::Refused { id, errors } => lines.push(format!(
                "{id:<24}   -- not loaded: {errors} errors - run `nkb lint` on it"
            )),
            PackEntry::Unreadable { id, reason } => lines.push(format!(
                "{id:<24}   -- not loaded: {}",
                source_error(*reason)
            )),
        }
    }

    if listing.entries.is_empty() {
        lines.push("No packs.".to_owned());
    }

    lines.push(String::new());
    lines.push(format!(
        "{} of {} packs loaded, {values} values.",
        listing.loaded(),
        listing.entries.len()
    ));
    lines.extend(coverage_lines(listing));
    lines
}

fn value_lines(pack: &Pack, value: &PackValue) -> Vec<String> {
    let mut lines = Vec::new();

    let mut marks = String::new();
    if pack.risk_of(value) == nkb_core::pack::Risk::Offensive {
        marks.push_str("  [offensive]");
    }
    if value.deprecated {
        marks.push_str("  [deprecated]");
        if let Some(replacement) = &value.replaced_by {
            marks.push_str(&format!(" -> {replacement}"));
        }
    }
    lines.push(format!("  {}{marks}", value.id));
    lines.push(format!("    {}", value.name));
    lines.push(format!("    value    {}", written_form(&value.definition)));

    if let Some(metrics) = metrics_of(&value.definition) {
        lines.push(format!(
            "    size     {}, {}",
            plural(metrics.code_points, "code point"),
            plural(metrics.bytes, "byte")
        ));
    }
    if let Some(breaks) = &value.breaks {
        lines.push(format!("    breaks   {breaks}"));
    }
    if let Some(expect) = &value.expect {
        lines.push(format!("    expect   {expect}"));
    }
    lines.push(String::new());
    lines
}

/// The whole of `nkb show <pack>`.
#[must_use]
pub fn pack(pack: &Pack, warnings: usize) -> Vec<String> {
    let mut lines = vec![
        format!("{} - {}", pack.id, pack.name),
        pack.description.clone(),
        format!(
            "version {}, updated {}, {}, fields: {}",
            pack.version,
            pack.updated,
            pack.license,
            if pack.fields.is_empty() {
                "-".to_owned()
            } else {
                pack.fields.join(", ")
            }
        ),
    ];

    if warnings > 0 {
        // pack-format.md 11: a pack with warnings loads, and the warnings are
        // visible in its information. This is that sentence, obeyed.
        lines.push(format!(
            "{warnings} warnings - run `nkb lint` on this pack to read them"
        ));
    }
    lines.push(String::new());

    for value in &pack.values {
        lines.extend(value_lines(pack, value));
    }

    for pair in &pack.pairs {
        lines.push(format!("  {} [pair: {}]", pair.id, pair.relation));
        lines.push(format!("    {}", pair.name));
        lines.push(format!("    a        {}", pair.a));
        lines.push(format!("    b        {}", pair.b));
        if let Some(breaks) = &pair.breaks {
            lines.push(format!("    breaks   {breaks}"));
        }
        // The same two sentences a value is shown with: a pair exists for the
        // sentence one value alone cannot carry (`D33`), and what a correct
        // application does is half of it.
        if let Some(expect) = &pair.expect {
            lines.push(format!("    expect   {expect}"));
        }
        lines.push(String::new());
    }

    lines.push(format!(
        "{} values, {} pairs.",
        pack.values.len(),
        pack.pairs.len()
    ));
    lines
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;
    use nkb_core::text::LiteralText;

    #[test]
    fn an_invisible_value_is_printed_escaped_and_not_as_nothing() {
        // The point of the whole command. Printing the literal would put an
        // invisible character on the terminal and tell the reader nothing.
        let body = ValueBody::Literal(LiteralText::new("\u{200B}"));
        let shown = written_form(&body.into());
        assert_eq!(shown, "\\u200B");
        assert!(shown.is_ascii());
    }

    #[test]
    fn a_generated_value_is_printed_as_its_recipe_and_never_expanded() {
        // 🔴 architektura.md 6.1. A million-character value must cost a short
        // line here, not a million characters of work.
        let body = ValueBody::Repeat {
            unit: LiteralText::new(" "),
            count: 1_000_000,
        };
        let shown = written_form(&body.into());
        assert_eq!(shown, "\\u0020 x 1000000");
        assert!(
            shown.len() < 40,
            "the recipe was expanded: {} characters came out",
            shown.len()
        );
    }

    #[test]
    fn the_size_of_a_recipe_is_known_without_building_it() {
        let body = ValueBody::Repeat {
            unit: LiteralText::new("ab"),
            count: 500_000,
        };
        let metrics = metrics_of(&body.into()).expect("a measurable recipe");
        assert_eq!(metrics.code_points, 1_000_000);
    }

    #[test]
    fn a_pair_is_shown_with_both_of_its_sentences() {
        use nkb_adapters::{BuiltInCatalogue, TomlPackFormat};
        use nkb_app::ports::{PackFormat, PackSource};
        let text = BuiltInCatalogue::new()
            .read("look-alike-pairs")
            .expect("the shipped pack with pairs is built in");
        let shipped = TomlPackFormat.parse(&text).expect("a shipped pack parses");
        let lines = pack(&shipped, 0);
        let at = lines
            .iter()
            .position(|line| line == "  latin-vs-cyrillic-a [pair: look-alike]")
            .expect("the first pair is listed");
        assert_eq!(
            lines.get(at + 1..at + 6).expect("five lines under it"),
            [
                "    Latin a against Cyrillic a",
                "    a        latin-small-a",
                "    b        cyrillic-small-a",
                "    breaks   Two visually identical usernames become two accounts, and support cannot tell which is which.",
                "    expect   Either both accepted as distinct and shown as distinct, or the second rejected as a duplicate.",
            ]
        );
    }

    #[test]
    fn a_next_date_value_is_shown_as_its_rule_and_never_as_a_date() {
        // `nkb show` describes the pack, and the pack says "the next one".
        use nkb_core::calendar::Weekday;
        use nkb_core::next_date::{TimeOfDay, Week};
        let rule = |week, weekday, time: Option<&str>| {
            let time = time.and_then(TimeOfDay::parse);
            Definition::NextDate(NextDate::new(3, week, weekday, time).expect("March"))
        };
        assert_eq!(
            written_form(&rule(Week::Last, Weekday::Sunday, Some("02:30"))),
            "next date: the last Sunday of March, 02:30"
        );
        assert_eq!(
            written_form(&rule(Week::First, Weekday::Wednesday, None)),
            "next date: the first Wednesday of March"
        );
        let metrics = metrics_of(&rule(Week::Last, Weekday::Sunday, Some("02:30")));
        assert_eq!(metrics.map(|size| size.code_points), Some(16));
    }
}
