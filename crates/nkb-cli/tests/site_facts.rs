//! The project website says what the program says, because it reads it from here.
//!
//! # What this writes, and why it is a test
//!
//! The website under `web/` shows the catalogue value by value, every command
//! of `nkb` with every option, and the version. None of that is typed into the
//! site. This test asks the program and the catalogue, and keeps the answers in
//! `web/data/facts/`, where the site generator reads them:
//!
//! - `catalogue.json` - every pack and every value, with what the palette shows
//!   for it: the preview with its markers, the recipe of a generated value, the
//!   facts about its shape and its four counts, all worked out by the same
//!   functions the palette uses (`ValueFacts::of`).
//! - `cli.json` - the general help and the help of every command, verbatim, and
//!   what this test reads out of it: the usage, the options, the formats and
//!   the exit codes each help names.
//!
//! With `NKB_WRITE_SITE=1` it writes the files. Without it, it compares them
//! with what the program says now and fails on any difference, so a new option,
//! a changed sentence in a help or a new value cannot reach `main` while the
//! website still says the old thing. The site was made to tell the truth about
//! the program, and on the day it stops, this is what goes red.
//!
//! # What it cannot see
//!
//! Whether the site's own words about an option or a value are right. Those are
//! the site's translations, and the site build refuses an option or a value with
//! no words, or with words written for a sentence the help no longer says - see
//! `web/README.md`. This test closes the half of the chain between the program
//! and the facts.

#![allow(clippy::panic, clippy::expect_used)]

use nkb_adapters::{BuiltInCatalogue, TomlPackFormat};
use nkb_app::{EmitOutcome, PackEntry, ValueFacts, emit_values, list_packs};
use nkb_core::preview::{ShapeFact, ValuePreview};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

// ---- a JSON document, written by hand -------------------------------------

/// A JSON value. The workspace carries no JSON library for tests, and a writer
/// for the six kinds of value is shorter than the argument for adding one.
enum Json {
    Null,
    Bool(bool),
    Number(u64),
    Text(String),
    List(Vec<Json>),
    Object(Vec<(&'static str, Json)>),
}

fn text(value: &str) -> Json {
    Json::Text(value.to_owned())
}

fn maybe_text(value: Option<&str>) -> Json {
    value.map_or(Json::Null, text)
}

fn texts(values: &[String]) -> Json {
    Json::List(values.iter().map(|v| text(v)).collect())
}

fn count(value: usize) -> Json {
    Json::Number(u64::try_from(value).expect("a count fits in 64 bits"))
}

/// Every character outside printable ASCII is written as an escape, so the file
/// holds nothing a reviewer cannot see - the facts carry zero-width spaces,
/// bidirectional controls and emoji.
fn quoted(value: &str, out: &mut String) {
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            ' '..='~' => out.push(c),
            _ => {
                let mut units = [0u16; 2];
                for unit in c.encode_utf16(&mut units) {
                    write!(out, "\\u{unit:04x}").expect("writing to a string cannot fail");
                }
            }
        }
    }
    out.push('"');
}

fn render(value: &Json, depth: usize, out: &mut String) {
    let pad = "  ".repeat(depth + 1);
    let close = "  ".repeat(depth);
    match value {
        Json::Null => out.push_str("null"),
        Json::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Json::Number(n) => write!(out, "{n}").expect("writing to a string cannot fail"),
        Json::Text(t) => quoted(t, out),
        Json::List(items) if items.is_empty() => out.push_str("[]"),
        Json::List(items) => {
            out.push_str("[\n");
            for (k, item) in items.iter().enumerate() {
                out.push_str(&pad);
                render(item, depth + 1, out);
                out.push_str(if k + 1 < items.len() { ",\n" } else { "\n" });
            }
            out.push_str(&close);
            out.push(']');
        }
        Json::Object(fields) if fields.is_empty() => out.push_str("{}"),
        Json::Object(fields) => {
            out.push_str("{\n");
            for (k, (key, item)) in fields.iter().enumerate() {
                out.push_str(&pad);
                quoted(key, out);
                out.push_str(": ");
                render(item, depth + 1, out);
                out.push_str(if k + 1 < fields.len() { ",\n" } else { "\n" });
            }
            out.push_str(&close);
            out.push('}');
        }
    }
}

fn document(value: &Json) -> String {
    let mut out = String::new();
    render(value, 0, &mut out);
    out.push('\n');
    out
}

// ---- the help, read the way a person reads it -----------------------------

/// The sections a help may have. A heading outside this list stops the test:
/// a reader that skipped what it did not know would compare the site with
/// part of the help and pass.
const SECTIONS: [&str; 5] = ["Usage", "Commands", "Options", "Formats", "Exit codes"];

#[derive(Default)]
struct Entry {
    name: String,
    help: String,
}

#[derive(Default)]
struct Help {
    /// `nkb` for the general help, `nkb emit` for a command.
    title: String,
    summary: String,
    usage: Vec<String>,
    about: Vec<String>,
    commands: Vec<Entry>,
    options: Vec<Entry>,
    formats: Vec<Entry>,
    exit_codes: Vec<Entry>,
}

/// Splits `  name    what it does` at the first run of two spaces or more.
fn split_entry(line: &str) -> Option<(String, String)> {
    let body = line.trim_start();
    let gap = body.find("  ")?;
    let name = body[..gap].trim_end();
    let help = body[gap..].trim_start();
    (!name.is_empty() && !help.is_empty()).then(|| (name.to_owned(), help.to_owned()))
}

fn entries_of<'a>(help: &'a mut Help, section: &str) -> &'a mut Vec<Entry> {
    match section {
        "Commands" => &mut help.commands,
        "Options" => &mut help.options,
        "Formats" => &mut help.formats,
        "Exit codes" => &mut help.exit_codes,
        other => panic!("{other} is not a section of entries"),
    }
}

/// Reads one help. Panics on any line it does not understand, with the line.
fn read_help(text: &str) -> Help {
    let mut lines = text.lines();
    let first = lines.next().expect("a help has a first line");
    let (title, summary) = first.split_once(" - ").unwrap_or_else(|| {
        panic!("the first line of a help is `nkb ... - what it does`: {first:?}")
    });
    let mut help = Help {
        title: title.to_owned(),
        summary: summary.to_owned(),
        ..Help::default()
    };
    let mut section: Option<&str> = None;
    let mut paragraph = String::new();
    for line in lines {
        if line.trim().is_empty() {
            section = None;
            if !paragraph.is_empty() {
                help.about.push(std::mem::take(&mut paragraph));
            }
            continue;
        }
        if !line.starts_with(' ') {
            // A heading is short. `nkb send --help` has a sentence at the left
            // edge that ends in a colon, and it is prose.
            if let Some(heading) = line
                .strip_suffix(':')
                .filter(|h| h.split_whitespace().count() <= 3)
            {
                section =
                    Some(SECTIONS.iter().find(|s| **s == heading).unwrap_or_else(|| {
                        panic!("a heading this reader does not know: {line:?}")
                    }));
                continue;
            }
            assert!(
                section.is_none(),
                "prose inside the {section:?} section, with no blank line before it: {line:?}"
            );
            if !paragraph.is_empty() {
                paragraph.push(' ');
            }
            paragraph.push_str(line);
            continue;
        }
        match section {
            None => panic!("an indented line outside every section: {line:?}"),
            Some("Usage") => help.usage.push(line.trim().to_owned()),
            Some(name) => read_entry(name, entries_of(&mut help, name), line),
        }
    }
    if !paragraph.is_empty() {
        help.about.push(paragraph);
    }
    help
}

/// Whether `line` starts an entry of `section` rather than carrying on the one
/// above. Told by shape, because the indents differ: an option may sit six
/// columns in (`      --raw`), and the second line of an exit code five.
fn starts_entry(section: &str, line: &str) -> bool {
    let body = line.trim_start();
    let indent = line.len() - body.len();
    match section {
        "Options" => body.starts_with('-'),
        "Exit codes" => body
            .split_once("  ")
            .is_some_and(|(code, _)| code.chars().all(|c| c.is_ascii_digit())),
        _ => indent == 2,
    }
}

/// An entry is a name and a description, and the lines after it that do not
/// start an entry of their own carry its description on.
fn read_entry(section: &str, entries: &mut Vec<Entry>, line: &str) {
    if !starts_entry(section, line) {
        let last = entries
            .last_mut()
            .unwrap_or_else(|| panic!("a continuation with no entry above it: {line:?}"));
        last.help.push(' ');
        last.help.push_str(line.trim());
        return;
    }
    let (name, help) =
        split_entry(line).unwrap_or_else(|| panic!("an entry with no description: {line:?}"));
    entries.push(Entry { name, help });
}

/// One option as the website shows it.
struct Option_ {
    flags: Vec<String>,
    argument: Option<String>,
    values: Vec<String>,
    help: Option<String>,
}

/// `[--format json|csv|lines]`, `[--escaped|--raw]`, `[--index N]` - the
/// options a usage line names, which is where an option without an entry of
/// its own (`--format`) is found.
fn usage_options(usage: &[String]) -> Vec<Option_> {
    let mut found = Vec::new();
    for line in usage {
        for group in line.split('[').skip(1) {
            let inside = group.split(']').next().unwrap_or("");
            let mut words = inside.split_whitespace();
            let Some(first) = words.next() else { continue };
            if !first.starts_with('-') {
                continue;
            }
            let rest: Vec<&str> = words.collect();
            if first.contains('|') {
                for flag in first.split('|') {
                    found.push(Option_ {
                        flags: vec![flag.to_owned()],
                        argument: None,
                        values: Vec::new(),
                        help: None,
                    });
                }
                continue;
            }
            let spec = rest.join(" ");
            let (argument, values) = if spec.contains('|') {
                (None, spec.split('|').map(str::to_owned).collect())
            } else if spec.is_empty() {
                (None, Vec::new())
            } else {
                (Some(spec), Vec::new())
            };
            found.push(Option_ {
                flags: vec![first.to_owned()],
                argument,
                values,
                help: None,
            });
        }
    }
    found
}

/// The options of one help, in the order a person meets them: those the usage
/// names, in its order, then the entries the usage leaves out (`--help`).
fn options_of(help: &Help) -> Vec<Option_> {
    let mut options = described_options(help);
    let order: Vec<String> = usage_options(&help.usage)
        .into_iter()
        .map(|o| o.flags[0].clone())
        .collect();
    let rank = |o: &Option_| {
        o.flags
            .iter()
            .find_map(|f| order.iter().position(|u| u == f))
            .unwrap_or(order.len())
    };
    options.sort_by_key(rank);
    options
}

/// Every entry of the `Options:` section, and every option the usage names
/// that has no entry.
fn described_options(help: &Help) -> Vec<Option_> {
    let mut from_usage = usage_options(&help.usage);
    let mut options = Vec::new();
    for entry in &help.options {
        let mut flags = Vec::new();
        let mut argument = None;
        for part in entry.name.split(", ") {
            let mut words = part.split_whitespace();
            let flag = words.next().expect("an option entry names a flag");
            assert!(
                flag.starts_with('-'),
                "an option that is not a flag: {:?}",
                entry.name
            );
            flags.push(flag.to_owned());
            if let Some(word) = words.next() {
                argument = Some(word.to_owned());
            }
        }
        let mut values = Vec::new();
        if let Some(k) = from_usage.iter().position(|u| flags.contains(&u.flags[0])) {
            let named = from_usage.remove(k);
            values = named.values;
            argument = argument.or(named.argument);
        }
        options.push(Option_ {
            flags,
            argument,
            values,
            help: Some(entry.help.clone()),
        });
    }
    options.extend(from_usage);
    options
}

// ---- asking the program ----------------------------------------------------

fn nkb(args: &[&str]) -> String {
    let run = Command::new(env!("CARGO_BIN_EXE_nkb"))
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("`nkb {}` must be runnable: {e}", args.join(" ")));
    assert!(
        run.status.success(),
        "`nkb {}` exited with {:?}",
        args.join(" "),
        run.status.code()
    );
    String::from_utf8(run.stdout).expect("the help is UTF-8")
}

fn entries_json(entries: &[Entry]) -> Json {
    Json::List(
        entries
            .iter()
            .map(|e| Json::Object(vec![("name", text(&e.name)), ("help", text(&e.help))]))
            .collect(),
    )
}

fn options_json(options: &[Option_]) -> Json {
    Json::List(
        options
            .iter()
            .map(|o| {
                Json::Object(vec![
                    ("flags", texts(&o.flags)),
                    ("argument", maybe_text(o.argument.as_deref())),
                    ("values", texts(&o.values)),
                    ("help", maybe_text(o.help.as_deref())),
                ])
            })
            .collect(),
    )
}

fn help_json(name: &str, raw: &str, help: &Help) -> Json {
    Json::Object(vec![
        ("name", text(name)),
        ("summary", text(&help.summary)),
        ("usage", texts(&help.usage)),
        ("about", texts(&help.about)),
        ("options", options_json(&options_of(help))),
        ("formats", entries_json(&help.formats)),
        ("exit_codes", entries_json(&help.exit_codes)),
        ("help", text(raw)),
    ])
}

fn cli_facts() -> Json {
    let general_raw = nkb(&["--help"]);
    let general = read_help(&general_raw);
    assert!(
        general.commands.len() >= 7,
        "the general help named {} commands - the reader missed them",
        general.commands.len()
    );
    let version = nkb(&["--version"]);
    let mut commands = Vec::new();
    for entry in &general.commands {
        let name = entry
            .name
            .split_whitespace()
            .next()
            .expect("a command entry starts with its name");
        let raw = nkb(&[name, "--help"]);
        let help = read_help(&raw);
        assert_eq!(
            help.title,
            format!("nkb {name}"),
            "the help of {name} is titled for another command"
        );
        commands.push(help_json(name, &raw, &help));
    }
    Json::Object(vec![
        ("version", text(version.trim())),
        ("general", help_json("nkb", &general_raw, &general)),
        ("commands", Json::List(commands)),
    ])
}

// ---- asking the catalogue --------------------------------------------------

fn shape_json(fact: &ShapeFact) -> Json {
    let (name, number) = match *fact {
        ShapeFact::ZeroWidth(n) => ("zero-width", Some(n)),
        ShapeFact::BidiControl(n) => ("bidi-control", Some(n)),
        ShapeFact::SoftHyphen(n) => ("soft-hyphen", Some(n)),
        ShapeFact::UnusualSpace(n) => ("unusual-space", Some(n)),
        ShapeFact::Tab(n) => ("tab", Some(n)),
        ShapeFact::LineBreak(n) => ("line-break", Some(n)),
        ShapeFact::OtherControl(n) => ("other-control", Some(n)),
        ShapeFact::LeadingSpace => ("leading-space", None),
        ShapeFact::TrailingSpace => ("trailing-space", None),
    };
    Json::Object(vec![
        ("fact", text(name)),
        ("count", number.map_or(Json::Null, count)),
    ])
}

fn preview_json(preview: &ValuePreview) -> (Json, Json) {
    match preview {
        ValuePreview::Text(shown) => (
            Json::Object(vec![
                ("shown", text(&shown.shown)),
                ("elided_total", shown.elided_total.map_or(Json::Null, count)),
            ]),
            Json::Null,
        ),
        ValuePreview::Recipe(recipe) => (
            Json::Null,
            Json::Object(vec![
                ("unit", text(&recipe.unit)),
                ("count", Json::Number(u64::from(recipe.count))),
            ]),
        ),
    }
}

fn catalogue_facts() -> Json {
    let catalogue = BuiltInCatalogue::new();
    let listing = list_packs(&catalogue, &TomlPackFormat).expect("the built-in catalogue lists");
    let mut packs = Vec::new();
    let mut total = 0;
    for entry in listing.entries {
        let PackEntry::Loaded { pack, warnings } = entry else {
            panic!("a built-in pack did not load, and shipped_packs.rs says why");
        };
        let EmitOutcome::Emitted(emission) = emit_values(&catalogue, &TomlPackFormat, &pack.id)
        else {
            panic!("{} does not emit", pack.id);
        };
        assert_eq!(
            emission.values.len(),
            pack.values.len(),
            "{} emits another list",
            pack.id
        );
        let mut values = Vec::new();
        for (value, emitted) in pack.values.iter().zip(&emission.values) {
            assert_eq!(value.id, emitted.id, "{} emits in another order", pack.id);
            let metrics = value
                .body
                .metrics()
                .expect("a shipped value can be measured");
            let literal = value
                .body
                .materialise()
                .expect("a shipped value can be built");
            let facts = ValueFacts::of(&pack, value, &literal, metrics, warnings);
            let (preview, recipe) = preview_json(&facts.preview);
            values.push(Json::Object(vec![
                ("id", text(&emitted.id)),
                ("ref", text(&emitted.reference)),
                ("name", text(&emitted.name)),
                (
                    "escaped",
                    if emitted.generated {
                        Json::Null
                    } else {
                        text(&emitted.escaped)
                    },
                ),
                ("breaks", maybe_text(emitted.breaks.as_deref())),
                ("expect", maybe_text(emitted.expect.as_deref())),
                ("fields", texts(&emitted.fields)),
                ("tags", texts(&emitted.tags)),
                ("risk", text(emitted.risk.as_str())),
                ("since", maybe_text(emitted.since.as_deref())),
                ("source", maybe_text(emitted.source.as_deref())),
                ("generated", Json::Bool(emitted.generated)),
                ("preview", preview),
                ("recipe", recipe),
                (
                    "shape",
                    Json::List(facts.shape.iter().map(shape_json).collect()),
                ),
                (
                    "counts",
                    Json::Object(vec![
                        ("graphemes", count(facts.graphemes)),
                        ("code_points", count(facts.code_points)),
                        ("bytes", count(facts.bytes)),
                        ("utf16_units", count(literal.encode_utf16().count())),
                    ]),
                ),
            ]));
        }
        total += values.len();
        packs.push(Json::Object(vec![
            ("id", text(&pack.id)),
            ("name", text(&pack.name)),
            ("description", text(&pack.description)),
            ("version", text(&pack.version)),
            ("updated", text(&pack.updated)),
            ("license", text(&pack.license)),
            ("language", text(&pack.language)),
            ("fields", texts(&pack.fields)),
            ("tags", texts(&pack.tags)),
            ("value_count", count(values.len())),
            ("values", Json::List(values)),
        ]));
    }
    Json::Object(vec![
        ("pack_count", count(packs.len())),
        ("value_count", count(total)),
        ("packs", Json::List(packs)),
    ])
}

// ---- the files -------------------------------------------------------------

fn facts_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/data/facts")
}

/// Writes the file under `NKB_WRITE_SITE=1`, compares it otherwise.
fn keep(name: &str, facts: &Json) {
    let path = facts_dir().join(name);
    let now = document(facts);
    if std::env::var_os("NKB_WRITE_SITE").is_some_and(|v| v == "1") {
        std::fs::create_dir_all(facts_dir()).expect("web/data/facts can be made");
        std::fs::write(&path, &now)
            .unwrap_or_else(|e| panic!("{} could not be written: {e}", path.display()));
        return;
    }
    let kept = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "{} could not be read ({e}). Write it: NKB_WRITE_SITE=1 cargo test -p nkb-cli --test site_facts",
            path.display()
        )
    });
    if let Some(difference) = difference(&kept, &now) {
        panic!(
            "\nweb/data/facts/{name} says something other than the program says now, \
             {difference}\n\n\
             If the program is right, write the facts again and look at the site's diff:\n  \
             NKB_WRITE_SITE=1 cargo test -p nkb-cli --test site_facts\n"
        );
    }
}

/// Where the kept facts and the program's part ways, or `None` when they do not.
fn difference(kept: &str, now: &str) -> Option<String> {
    if kept == now {
        return None;
    }
    let line = kept
        .lines()
        .zip(now.lines())
        .position(|(a, b)| a != b)
        .unwrap_or_else(|| kept.lines().count().min(now.lines().count()));
    Some(format!(
        "first at line {}.\n  kept: {:?}\n  now:  {:?}",
        line + 1,
        kept.lines().nth(line).unwrap_or(""),
        now.lines().nth(line).unwrap_or("")
    ))
}

#[test]
fn the_site_lists_every_pack_and_value_the_program_ships() {
    keep("catalogue.json", &catalogue_facts());
}

#[test]
fn the_site_lists_every_command_and_option_the_program_answers_to() {
    keep("cli.json", &cli_facts());
}

// ---- the reader, proved on help it was not written from ---------------------

#[test]
fn the_help_reader_keeps_each_section_apart_and_joins_what_carries_on() {
    let help = read_help(
        "nkb demo - a help made for this test\n\nUsage:\n  nkb demo <pack> [--count N] [--form a|b] [--loud|--quiet]\n\n\
         A paragraph about it,\nin two lines.\n\nOptions:\n  --count N      how many\n                 and more about it\n      \
         --loud     louder\n  -h, --help     Show this help and exit with 0\n\nExit codes:\n  0  fine\n  2  called wrongly,\n     \
         and more\n",
    );
    assert_eq!(help.title, "nkb demo");
    assert_eq!(help.about, ["A paragraph about it, in two lines."]);
    assert_eq!(help.options.len(), 3);
    assert_eq!(help.options[0].help, "how many and more about it");
    assert_eq!(help.exit_codes[1].help, "called wrongly, and more");
    let options = options_of(&help);
    let flags: Vec<String> = options.iter().map(|o| o.flags.join(",")).collect();
    assert_eq!(
        flags,
        ["--count", "--form", "--loud", "--quiet", "-h,--help"]
    );
    assert_eq!(options[0].argument.as_deref(), Some("N"));
    assert_eq!(options[1].values, ["a", "b"]);
    assert!(
        options[3].help.is_none(),
        "an option only the usage names has no help line"
    );
    assert_eq!(options[2].help.as_deref(), Some("louder"));
}

#[test]
#[should_panic(expected = "a heading this reader does not know")]
fn the_help_reader_refuses_a_section_it_does_not_know() {
    read_help("nkb demo - x\n\nExamples:\n  nkb demo\n");
}

#[test]
fn a_long_sentence_ending_in_a_colon_is_prose_and_not_a_heading() {
    let help = read_help(
        "nkb demo - x\n\nNothing is sent while a key is held on the keyboard:\nthe command waits.\n",
    );
    assert_eq!(
        help.about,
        ["Nothing is sent while a key is held on the keyboard: the command waits."]
    );
}

#[test]
#[should_panic(expected = "an indented line outside every section")]
fn the_help_reader_refuses_an_indented_line_with_no_section() {
    read_help("nkb demo - x\n\nUsage:\n  nkb demo\n\n  stray\n");
}

#[test]
fn facts_that_differ_from_the_program_are_named_and_equal_ones_pass() {
    assert_eq!(difference("a\nb\n", "a\nb\n"), None);
    let found = difference("a\nb\n", "a\nc\n").expect("a changed line is a difference");
    assert!(found.starts_with("first at line 2."), "{found}");
    assert!(
        difference("a\n", "a\nb\n").is_some(),
        "a line the program added is a difference"
    );
}

#[test]
fn the_facts_are_written_in_printable_ascii_only() {
    let mut out = String::new();
    quoted(&format!("a{}b{}c\"\\", '\u{200B}', '\u{1F600}'), &mut out);
    assert!(
        out.chars().all(|c| c.is_ascii() && !c.is_ascii_control()),
        "{out}"
    );
    assert_eq!(
        out.matches("\\u").count(),
        3,
        "one escape for the space, two for the emoji: {out}"
    );
}
