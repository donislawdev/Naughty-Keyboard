//! Looking at the catalogue: the use cases behind `nkb packs` and `nkb show`.
//!
//! # Why two commands share one file
//!
//! They are the same operation at two sizes, and the part worth getting right is
//! the part they share: loading a pack **honestly**. `pack-format.md` 11 settles
//! that a pack with one bad value out of thirty-four loads nothing at all, and
//! that warnings are the single exception - a pack carrying them loads normally
//! and shows them. Written twice, that rule would eventually be two rules.
//!
//! # Three outcomes per pack, not two
//!
//! A pack that could not be loaded is not the same as a pack that is not there,
//! and neither is the same as a pack whose file would not open. A listing that
//! rendered all three as absence would send somebody to look for a missing
//! download when what they have is a syntax error on line forty.
//!
//! So a refused pack still appears in the listing, named, with the number of
//! problems and nothing else - the problems themselves belong to `nkb lint`,
//! which is the command whose job that is.

use nkb_core::lint::Severity;
use nkb_core::pack::Pack;

use crate::ports::{CatalogueCoverage, PackCatalogue, PackFormat, PackSource, SourceError};

/// One pack, as far as this run could get with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackEntry {
    /// Loaded. `warnings` is a count rather than a list, because a listing is
    /// not the place to read them and `nkb lint` is.
    Loaded { pack: Box<Pack>, warnings: usize },
    /// Present, readable, and refused: `errors` problems block it. The pack is
    /// absent from the tool and present in the listing, which is the only
    /// combination that does not mislead.
    Refused { id: String, errors: usize },
    /// The file itself would not give up its text.
    Unreadable { id: String, reason: SourceError },
}

impl PackEntry {
    /// The identifier this entry is about, loaded or not.
    #[must_use]
    pub fn id(&self) -> &str {
        match self {
            Self::Loaded { pack, .. } => &pack.id,
            Self::Refused { id, .. } | Self::Unreadable { id, .. } => id,
        }
    }
}

/// The whole catalogue as this run sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listing {
    pub entries: Vec<PackEntry>,
    /// 🔴 Which of the format's three sources were read and which were not.
    ///
    /// Beside the list rather than in a footnote, and never optional: a listing
    /// from one source of three looks exactly like a complete one, and the
    /// person reading it is about to conclude something false about their own
    /// installation. Untouchable rule 1.
    pub coverage: CatalogueCoverage,
}

impl Listing {
    /// Packs this run could actually offer.
    #[must_use]
    pub fn loaded(&self) -> usize {
        self.entries
            .iter()
            .filter(|entry| matches!(entry, PackEntry::Loaded { .. }))
            .count()
    }

    /// Packs that are present and unusable. Worth a separate count because it
    /// answers a different question: not "how much is there" but "what is
    /// broken here".
    #[must_use]
    pub fn unusable(&self) -> usize {
        self.entries.len() - self.loaded()
    }
}

/// Loads one pack the honest way: check first, refuse on any error, parse after.
///
/// The order is the rule from `pack-format.md` 11 in code. Parsing first and
/// checking afterwards would work exactly as well until the day a file parsed
/// into something plausible and wrong.
fn load(format: &dyn PackFormat, id: &str, text: &str) -> PackEntry {
    let problems = format.check(text, id);
    let errors = problems
        .iter()
        .filter(|problem| problem.severity() == Severity::Error)
        .count();

    if errors > 0 {
        return PackEntry::Refused {
            id: id.to_owned(),
            errors,
        };
    }

    let warnings = problems.len() - errors;

    match format.parse(text) {
        Some(pack) => PackEntry::Loaded {
            pack: Box::new(pack),
            warnings,
        },
        // Nothing found a fault and nothing could be built. That combination
        // means the two halves of the format disagree with each other, which is
        // this tool's own defect - reported as one problem rather than as a
        // pack that silently vanished from the listing.
        None => PackEntry::Refused {
            id: id.to_owned(),
            errors: 1,
        },
    }
}

/// Every pack the catalogue holds, with what could be done with each.
///
/// # Errors
///
/// Returns [`SourceError`] only when the catalogue itself cannot be examined.
/// A pack that fails on its own becomes an entry, never an error for the whole
/// run: one bad file must not hide the other nineteen.
pub fn list_packs(
    catalogue: &dyn PackCatalogue,
    format: &dyn PackFormat,
) -> Result<Listing, SourceError> {
    let ids = catalogue.list()?;
    let entries = ids
        .into_iter()
        .map(|id| match catalogue.read(&id) {
            Ok(text) => load(format, &id, &text),
            Err(reason) => PackEntry::Unreadable { id, reason },
        })
        .collect();

    Ok(Listing {
        entries,
        coverage: catalogue.coverage(),
    })
}

/// What came of asking to see one pack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShowOutcome {
    /// Here it is.
    Shown { pack: Box<Pack>, warnings: usize },
    /// It is there and it does not load. The count, not the problems: reading
    /// those is `nkb lint`, and two commands reporting the same list in two
    /// shapes is two things to keep in step.
    Refused { errors: usize },
    /// No pack under that name.
    NotFound,
    /// The file is there and would not open.
    Unreadable,
}

/// One pack, in full.
///
/// Takes a [`PackSource`] rather than a [`PackCatalogue`]: this reads one pack
/// by name and never asks what else exists, so requiring the ability to
/// enumerate would be asking for something it does not use.
#[must_use]
pub fn show_pack(source: &dyn PackSource, format: &dyn PackFormat, id: &str) -> ShowOutcome {
    let text = match source.read(id) {
        Ok(text) => text,
        Err(SourceError::NotFound) => return ShowOutcome::NotFound,
        // Not UTF-8 is E005 and a finding about the pack, which `nkb lint`
        // reports. Here both mean the same thing: no text to show.
        Err(SourceError::Unreadable | SourceError::NotUtf8) => return ShowOutcome::Unreadable,
    };

    match load(format, id, &text) {
        PackEntry::Loaded { pack, warnings } => ShowOutcome::Shown { pack, warnings },
        PackEntry::Refused { errors, .. } => ShowOutcome::Refused { errors },
        PackEntry::Unreadable { .. } => ShowOutcome::Unreadable,
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;
    use crate::ports::{CatalogueSource, Date, SourceSkipped, TranslationCheck, TranslationTarget};
    use nkb_core::lint::{LintProblem, RuleCode};
    use nkb_core::pack::Risk;
    use nkb_core::text::LiteralText;
    use nkb_core::value::ValueBody;
    use std::collections::BTreeMap;

    /// A catalogue with no disk behind it. The reason ports exist.
    struct Shelf {
        packs: BTreeMap<String, Result<String, SourceError>>,
    }

    impl Shelf {
        fn of(entries: &[(&str, &str)]) -> Self {
            Self {
                packs: entries
                    .iter()
                    .map(|(id, text)| ((*id).to_owned(), Ok((*text).to_owned())))
                    .collect(),
            }
        }
    }

    impl PackSource for Shelf {
        fn read(&self, id: &str) -> Result<String, SourceError> {
            self.packs
                .get(id)
                .cloned()
                .unwrap_or(Err(SourceError::NotFound))
        }
    }

    impl PackCatalogue for Shelf {
        fn list(&self) -> Result<Vec<String>, SourceError> {
            Ok(self.packs.keys().cloned().collect())
        }
        fn coverage(&self) -> CatalogueCoverage {
            CatalogueCoverage {
                consulted: vec![CatalogueSource::BuiltIn],
                skipped: vec![(CatalogueSource::Team, SourceSkipped::NotImplementedYet)],
            }
        }
    }

    /// A format that says what the test tells it to, so the use case is
    /// exercised without a parser.
    struct Scripted {
        errors: usize,
        warnings: usize,
        parses: bool,
    }

    impl Scripted {
        fn clean() -> Self {
            Self {
                errors: 0,
                warnings: 0,
                parses: true,
            }
        }
    }

    fn a_pack(id: &str) -> Pack {
        Pack {
            id: id.to_owned(),
            name: "Sample".to_owned(),
            description: "Whatever the test needs it to be.".to_owned(),
            version: "1.0".to_owned(),
            updated: "2026-09-08".to_owned(),
            license: "CC-BY-4.0".to_owned(),
            authors: vec!["Naughty Keyboard".to_owned()],
            language: "en".to_owned(),
            risk: Risk::Normal,
            tags: Vec::new(),
            fields: vec!["any".to_owned()],
            source: None,
            values: vec![nkb_core::pack::PackValue {
                id: "one".to_owned(),
                name: "One".to_owned(),
                body: ValueBody::Literal(LiteralText::new("x")),
                breaks: None,
                expect: None,
                risk: None,
                fields: Vec::new(),
                tags: Vec::new(),
                source: None,
                since: Some("1.0".to_owned()),
                shape: None,
                deprecated: false,
                replaced_by: None,
            }],
            pairs: Vec::new(),
        }
    }

    impl PackFormat for Scripted {
        fn check(&self, _text: &str, _expected_id: &str) -> Vec<LintProblem> {
            let mut problems = Vec::new();
            for _ in 0..self.errors {
                problems.push(LintProblem::new(RuleCode::PackWithoutValues));
            }
            for _ in 0..self.warnings {
                problems.push(LintProblem::new(RuleCode::PackTooLarge));
            }
            problems
        }
        fn translated_pack(&self, _text: &str) -> TranslationTarget {
            TranslationTarget::NotATranslation
        }
        fn check_translation(&self, _text: &str, _translated: &str) -> TranslationCheck {
            TranslationCheck::Compared(Vec::new())
        }
        fn parse(&self, text: &str) -> Option<Pack> {
            self.parses.then(|| a_pack(text.trim()))
        }
        fn skeleton(&self, _id: &str, _today: Date) -> String {
            String::new()
        }
        fn canonical(&self, _text: &str) -> Option<String> {
            None
        }
        fn same_insertions(&self, _before: &str, _after: &str) -> bool {
            true
        }
    }

    #[test]
    fn a_clean_catalogue_lists_every_pack_as_loaded() {
        let shelf = Shelf::of(&[("alpha", "alpha"), ("beta", "beta")]);
        let listing = list_packs(&shelf, &Scripted::clean()).expect("the shelf can be listed");

        assert_eq!(listing.entries.len(), 2);
        assert_eq!(listing.loaded(), 2);
        assert_eq!(listing.unusable(), 0);
    }

    #[test]
    fn warnings_do_not_stop_a_pack_from_loading_and_are_counted() {
        // pack-format.md 11, the one exception to all-or-nothing.
        let shelf = Shelf::of(&[("alpha", "alpha")]);
        let format = Scripted {
            errors: 0,
            warnings: 3,
            parses: true,
        };
        let listing = list_packs(&shelf, &format).expect("the shelf can be listed");

        match listing.entries.first().expect("one entry") {
            PackEntry::Loaded { warnings, .. } => assert_eq!(*warnings, 3),
            other => panic!("a pack with only warnings must load, got {other:?}"),
        }
    }

    #[test]
    fn one_error_removes_the_whole_pack_and_it_still_appears_by_name() {
        // The rule the product turns on itself: loading the good values would
        // put a count in the palette that disagrees with the file.
        let shelf = Shelf::of(&[("alpha", "alpha")]);
        let format = Scripted {
            errors: 1,
            warnings: 0,
            parses: true,
        };
        let listing = list_packs(&shelf, &format).expect("the shelf can be listed");

        assert_eq!(listing.loaded(), 0);
        assert_eq!(listing.unusable(), 1);
        match listing.entries.first().expect("one entry") {
            PackEntry::Refused { id, errors } => {
                assert_eq!(id, "alpha");
                assert_eq!(*errors, 1);
            }
            other => panic!("expected a refusal that still names the pack, got {other:?}"),
        }
        assert_eq!(
            listing.entries.first().expect("one entry").id(),
            "alpha",
            "a refused pack must still be findable by name, or it reads as missing"
        );
    }

    #[test]
    fn a_file_that_checks_clean_and_will_not_parse_is_refused_rather_than_dropped() {
        // The two halves of the format disagreeing. It must not end with the
        // pack quietly absent from the listing.
        let shelf = Shelf::of(&[("alpha", "alpha")]);
        let format = Scripted {
            errors: 0,
            warnings: 0,
            parses: false,
        };
        let listing = list_packs(&shelf, &format).expect("the shelf can be listed");

        assert_eq!(listing.entries.len(), 1);
        assert_eq!(listing.loaded(), 0);
    }

    #[test]
    fn one_broken_pack_does_not_hide_the_others() {
        let shelf = Shelf {
            packs: BTreeMap::from([
                ("alpha".to_owned(), Ok("alpha".to_owned())),
                ("beta".to_owned(), Err(SourceError::Unreadable)),
                ("gamma".to_owned(), Ok("gamma".to_owned())),
            ]),
        };
        let listing = list_packs(&shelf, &Scripted::clean()).expect("the shelf can be listed");

        assert_eq!(listing.entries.len(), 3);
        assert_eq!(listing.loaded(), 2);
        match listing.entries.get(1).expect("three entries") {
            PackEntry::Unreadable { id, reason } => {
                assert_eq!(id, "beta");
                assert_eq!(*reason, SourceError::Unreadable);
            }
            other => panic!("expected an unreadable entry, got {other:?}"),
        }
    }

    #[test]
    fn the_listing_carries_what_the_catalogue_did_not_look_at() {
        let shelf = Shelf::of(&[("alpha", "alpha")]);
        let listing = list_packs(&shelf, &Scripted::clean()).expect("the shelf can be listed");
        assert!(
            !listing.coverage.skipped.is_empty(),
            "a listing with no account of the sources it skipped is the silence \
             untouchable rule 1 forbids"
        );
    }

    #[test]
    fn showing_a_pack_that_is_not_there_says_so_rather_than_showing_an_empty_one() {
        let shelf = Shelf::of(&[("alpha", "alpha")]);
        assert_eq!(
            show_pack(&shelf, &Scripted::clean(), "no-such-pack"),
            ShowOutcome::NotFound
        );
    }

    #[test]
    fn showing_a_refused_pack_reports_the_count_and_not_the_contents() {
        let shelf = Shelf::of(&[("alpha", "alpha")]);
        let format = Scripted {
            errors: 2,
            warnings: 0,
            parses: true,
        };
        assert_eq!(
            show_pack(&shelf, &format, "alpha"),
            ShowOutcome::Refused { errors: 2 }
        );
    }

    #[test]
    fn showing_a_good_pack_hands_over_the_pack_itself() {
        let shelf = Shelf::of(&[("alpha", "alpha")]);
        match show_pack(&shelf, &Scripted::clean(), "alpha") {
            ShowOutcome::Shown { pack, warnings } => {
                assert_eq!(pack.id, "alpha");
                assert_eq!(warnings, 0);
                assert_eq!(pack.values.len(), 1);
            }
            other => panic!("expected the pack, got {other:?}"),
        }
    }
}
