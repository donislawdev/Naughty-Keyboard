//! The pack templates the website offers, judged by the validator they teach.
//!
//! # Why the templates are held to more than a stranger's pack
//!
//! `web/assets/templates/` holds the packs a person downloads to start a pack of
//! their own, and the page that teaches the format quotes its examples from
//! them. A template is the first pack file somebody reads, and whatever it does
//! they copy - so a warning in one is taught to everybody who starts from it.
//! Every template passes every rule with no warning and no rule left unchecked,
//! the bar `shipped_packs.rs` sets for the built-in packs.
//!
//! # What else could go wrong without a sound
//!
//! - A template on disk that no page shows, or a page that names a template
//!   that is not there. The site build stops on the second, and only this test
//!   sees the first, in either language.
//! - A template that sends its reader to a page of the site that does not
//!   exist. Its comments name two, and a renamed page would leave every
//!   downloaded copy pointing nowhere.

#![allow(clippy::panic, clippy::expect_used)]

use nkb_adapters::{DirectoryPackSource, TomlPackFormat};
use nkb_app::{LintOutcome, lint_pack};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const SITE: &str = "https://naughtykeyboard.donislawdev.com/";

fn web() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web")
}

fn templates() -> Vec<PathBuf> {
    let dir = web().join("assets/templates");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{} must exist: {e}", dir.display()))
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "toml"))
        .collect();
    files.sort();
    assert!(
        files.len() >= 4,
        "{} holds {} templates - this test would pass by looking at almost nothing",
        dir.display(),
        files.len()
    );
    files
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{} must read: {e}", path.display()))
}

#[test]
fn every_template_passes_every_rule_with_no_warning() {
    for path in templates() {
        let (source, id) = DirectoryPackSource::split(&path)
            .unwrap_or_else(|why| panic!("{} must name a pack file: {why:?}", path.display()));
        let LintOutcome::Judged(report) = lint_pack(&source, &TomlPackFormat, &id) else {
            panic!("{} could not be read", path.display());
        };
        let found: Vec<String> = report
            .problems
            .iter()
            .map(|p| format!("{}:{} {}", id, p.line.unwrap_or(0), p.code.as_str()))
            .collect();
        assert!(found.is_empty(), "a template must be clean: {found:?}");
        let skipped: Vec<&str> = report.skipped.iter().map(|s| s.reason.as_str()).collect();
        assert!(skipped.is_empty(), "{id} left rules unchecked: {skipped:?}");
    }
}

/// The templates a page shows, by the shortcode that shows one.
fn shown_on(page: &Path) -> BTreeSet<String> {
    const OPEN: &str = "{{< template \"";
    let text = read(page);
    let mut shown = BTreeSet::new();
    let mut rest = text.as_str();
    while let Some(at) = rest.find(OPEN) {
        rest = &rest[at + OPEN.len()..];
        let end = rest.find('"').expect("a template name ends with a quote");
        shown.insert(rest[..end].to_owned());
    }
    shown
}

#[test]
fn every_template_is_shown_in_every_language_and_nothing_else_is() {
    let on_disk: BTreeSet<String> = templates()
        .iter()
        .map(|path| {
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .expect("a template is named in UTF-8")
                .to_owned()
        })
        .collect();
    for language in ["en", "pl"] {
        let page = web().join(format!("content/{language}/docs/pack-templates.md"));
        assert_eq!(
            shown_on(&page),
            on_disk,
            "{} shows other templates than web/assets/templates holds",
            page.display()
        );
    }
}

#[test]
fn every_page_a_template_points_to_is_on_the_site() {
    let mut pointed = 0;
    for path in templates() {
        let text = read(&path);
        let mut rest = text.as_str();
        while let Some(at) = rest.find(SITE) {
            rest = &rest[at + SITE.len()..];
            let end = rest.find(|c: char| c.is_whitespace()).unwrap_or(rest.len());
            let page = rest[..end].trim_end_matches('/');
            let content = web().join("content/en").join(page);
            assert!(
                content.with_extension("md").is_file() || content.join("_index.md").is_file(),
                "{} points to {SITE}{page}/, and the site has no such page",
                path.display()
            );
            pointed += 1;
        }
    }
    assert!(
        pointed >= 4,
        "the templates point to {pointed} pages - the scan found nothing"
    );
}
