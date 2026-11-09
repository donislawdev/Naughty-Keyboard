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
use nkb_adapters::{FOLDER_WAIT, PackFolders};
use nkb_app::{CatalogueSource, Listing, PackEntry, PackOrigin, SourceError, SourceSkipped};
use nkb_core::metrics::TextMetrics;
use nkb_core::next_date::NextDate;
use nkb_core::pack::{Pack, PackValue};
use nkb_core::text::escaped_prose;
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
///
/// A folder is named with its path - this is the tester's own terminal - and
/// the path is shown escaped, as a file name in a team folder is: both come
/// from somebody else (`D173`).
#[must_use]
pub fn coverage_lines(listing: &Listing, folders: &PackFolders) -> Vec<String> {
    let mut lines = Vec::new();
    let named = |source: CatalogueSource| match folders.path_of(source) {
        Some(path) => format!(
            "{} ({})",
            source.as_str(),
            escaped_prose(&path.to_string_lossy())
        ),
        None => source.as_str().to_owned(),
    };

    let consulted: Vec<String> = listing
        .coverage
        .consulted
        .iter()
        .map(|source| named(*source))
        .collect();
    lines.push(if consulted.is_empty() {
        "Read no pack source at all.".to_owned()
    } else {
        format!("Read these pack sources: {}.", consulted.join(", "))
    });

    for (source, reason) in &listing.coverage.skipped {
        lines.push(format!(
            "Not read: {} - {}.",
            named(*source),
            explain_skip(*source, *reason)
        ));
    }
    for (source, name) in &listing.coverage.misnamed {
        lines.push(format!(
            "Not a pack file, so not read: \"{}\" in the {} folder - a pack file is named after its pack, such as whitespace.toml.",
            escaped_prose(name),
            source.as_str()
        ));
    }
    lines
}

/// Why a source went unread, in words rather than in a marker.
fn explain_skip(source: CatalogueSource, reason: SourceSkipped) -> String {
    let words = match (reason, source) {
        (SourceSkipped::NotSet, _) => {
            "no folder is named - give --team-folder, or set team-folder under [packs] in the settings file"
        }
        (SourceSkipped::SettingUnusable, _) => {
            "the settings file names one that could not be used - the palette says why when it starts"
        }
        (SourceSkipped::NoPlace, _) => {
            "this system gives the settings file no place, so there is none for this folder either"
        }
        (SourceSkipped::Missing, CatalogueSource::Own) => {
            "it does not exist yet - make it and put pack files in it to use them"
        }
        (SourceSkipped::Missing, _) => "it does not exist",
        (SourceSkipped::NotAFolder, _) => "it is a file, not a folder",
        (SourceSkipped::Unreadable, _) => "it could not be read",
        (SourceSkipped::Unreachable, _) => {
            return format!("it did not answer within {} seconds", FOLDER_WAIT.as_secs());
        }
        (SourceSkipped::TooManyFiles, _) => {
            return format!(
                "it holds more than {} pack files, so it is likely not a pack folder",
                nkb_adapters::pack_folder::FOLDER_FILES_LIMIT
            );
        }
        (SourceSkipped::SameAsOwn, _) => "it is the own folder, read once as that",
    };
    words.to_owned()
}

/// Where a pack comes from, after its line in the listing - nothing for a
/// built-in pack that overrides nothing, which is every pack until a folder
/// holds one.
fn origin_note(origin: &PackOrigin) -> String {
    let from = match origin.source {
        CatalogueSource::BuiltIn => return String::new(),
        CatalogueSource::Team => "team folder",
        CatalogueSource::Own => "own folder",
    };
    match origin.overrides.first() {
        None => format!("  ({from})"),
        Some(CatalogueSource::Team) => format!("  ({from}, used instead of the team pack)"),
        Some(_) => format!("  ({from}, used instead of the built-in pack)"),
    }
}

/// How to see what is wrong with a refused pack: `nkb lint` on its file, named
/// for a pack from a folder - `nkb lint` reads a path, not a name (R04).
#[must_use]
pub fn lint_hint(id: &str, origin: &PackOrigin, folders: &PackFolders) -> String {
    match folders.path_of(origin.source) {
        Some(folder) => format!(
            "run nkb lint \"{}\"",
            escaped_prose(&folder.join(format!("{id}.toml")).to_string_lossy())
        ),
        None => "run `nkb lint` on it".to_owned(),
    }
}

/// The first line of `nkb show` for a pack that does not come with the tool:
/// where it is, and what it is used instead of. Nothing for a built-in pack.
#[must_use]
pub fn origin_line(origin: &PackOrigin, folders: &PackFolders) -> Vec<String> {
    let from = match origin.source {
        CatalogueSource::BuiltIn => return Vec::new(),
        CatalogueSource::Team => "the team folder",
        CatalogueSource::Own => "your own folder",
    };
    let path = folders
        .path_of(origin.source)
        .map(|path| format!(" ({})", escaped_prose(&path.to_string_lossy())))
        .unwrap_or_default();
    let instead = match origin.overrides.first() {
        None => "",
        Some(CatalogueSource::Team) => ", used instead of the team pack of the same name",
        Some(_) => ", used instead of the built-in pack of the same name",
    };
    vec![format!("From {from}{path}{instead}.")]
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
///
/// A pack from a folder says so after its line, and what it is used instead
/// of: a silent override would have a tester type other values than the ones
/// they think they chose (`pack-format.md` 12). Its name is shown escaped -
/// it comes from whoever wrote the file.
#[must_use]
pub fn listing(listing: &Listing, folders: &PackFolders) -> Vec<String> {
    let mut lines = Vec::new();
    let mut values = 0;

    for entry in &listing.entries {
        let from = origin_note(entry.origin());
        match entry {
            PackEntry::Loaded { pack, warnings, .. } => {
                values += pack.values.len();
                let mut note = String::new();
                if *warnings > 0 {
                    note.push_str(&format!("  ({warnings} warnings)"));
                }
                if pack.carries_offensive() {
                    note.push_str("  (offensive)");
                }
                lines.push(format!(
                    "{:<24}{:>4} values  {}{note}{from}",
                    pack.id,
                    pack.values.len(),
                    escaped_prose(&pack.name)
                ));
            }
            // A refused pack keeps its line. Dropping it would render "present
            // and broken" as "absent", and those need different repairs. One
            // from a folder names its file, which `nkb lint` reads.
            PackEntry::Refused { id, errors, origin } => {
                let lint = lint_hint(id, origin, folders);
                lines.push(format!(
                    "{id:<24}   -- not loaded: {errors} errors - {lint}{from}"
                ));
            }
            PackEntry::Unreadable { id, reason, .. } => lines.push(format!(
                "{id:<24}   -- not loaded: {}{from}",
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
    lines.extend(coverage_lines(listing, folders));
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
    lines.push(format!("    {}", escaped_prose(&value.name)));
    lines.push(format!("    value    {}", written_form(&value.definition)));

    if let Some(metrics) = metrics_of(&value.definition) {
        lines.push(format!(
            "    size     {}, {}",
            plural(metrics.code_points, "code point"),
            plural(metrics.bytes, "byte")
        ));
    }
    if let Some(breaks) = &value.breaks {
        lines.push(format!("    breaks   {}", escaped_prose(breaks)));
    }
    if let Some(expect) = &value.expect {
        lines.push(format!("    expect   {}", escaped_prose(expect)));
    }
    lines.push(String::new());
    lines
}

/// The whole of `nkb show <pack>`.
///
/// Every sentence of the pack is shown escaped (`escaped_prose`): since packs
/// come from folders (`D173`), a name may come from a stranger, and `nkb lint`
/// lets a line break or a terminal's escape sequence through in prose - written
/// out to a terminal, they would forge lines or recolour it.
#[must_use]
pub fn pack(pack: &Pack, warnings: usize) -> Vec<String> {
    let mut lines = vec![
        format!("{} - {}", pack.id, escaped_prose(&pack.name)),
        escaped_prose(&pack.description),
        format!(
            "version {}, updated {}, {}, fields: {}",
            escaped_prose(&pack.version),
            pack.updated,
            escaped_prose(&pack.license),
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
        lines.push(format!("    {}", escaped_prose(&pair.name)));
        lines.push(format!("    a        {}", pair.a));
        lines.push(format!("    b        {}", pair.b));
        if let Some(breaks) = &pair.breaks {
            lines.push(format!("    breaks   {}", escaped_prose(breaks)));
        }
        // The same two sentences a value is shown with: a pair exists for the
        // sentence one value alone cannot carry (`D33`), and what a correct
        // application does is half of it.
        if let Some(expect) = &pair.expect {
            lines.push(format!("    expect   {}", escaped_prose(expect)));
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
