//! The one way a pack file becomes a pack.
//!
//! # Why this is a module and not eight lines repeated
//!
//! `architektura.md` 3 calls the order binding: a caller runs `check`, refuses
//! the pack on ANY error, and only then calls `parse` - because
//! `pack-format.md` 11 loads all of a pack or none of it. Reversed, it would
//! work exactly as well until the day a file parsed into something plausible
//! and wrong.
//!
//! That order was written out three times, once per use case, and it grew with
//! them: `nkb packs`, then `emit`, then `send`. Ahead are `LoadCatalog`,
//! `GenerateBoundaries` and everything the palette does. A safety rule copied
//! per caller is a rule that holds until somebody writes the fourth caller in a
//! hurry, and nothing would have said so - OBS-96.
//!
//! # What makes the copies dangerous rather than merely repetitive
//!
//! 🔴 [`PackFormat::parse`] does not check `format = 1`. The version is guarded
//! by `check` alone. The reading half never looks at that field. So a caller
//! that reached for `parse` on its own would load a pack written for a format
//! this build does not understand, and nothing would report it - while the
//! `format` field exists precisely so that cannot happen.
//!
//! Guarded by `tests/one_way_into_a_pack.rs`, which fails if `parse` is called
//! anywhere in this layer but here.
//!
//! # What this deliberately does not do
//!
//! It does not read. The three callers reach their source differently - one is
//! handed the text by a listing that already enumerated it, two ask a
//! `PackSource` themselves - and each maps a missing or unreadable file onto its
//! own outcome. Folding the read in here would mean inventing a shared error
//! type for the one part that is genuinely different in each.

use nkb_core::lint::Severity;
use nkb_core::pack::Pack;

use crate::ports::PackFormat;

/// A pack that loaded, and what was said about it on the way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedPack {
    pub pack: Pack,
    /// Warnings do not stop a pack. `pack-format.md` 11 makes them the single
    /// exception to all-or-nothing: a pack carrying them loads normally and
    /// shows them.
    pub warnings: usize,
}

/// Why a pack that was read did not become a pack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Refused {
    /// How many errors block it. Never zero: a refusal with nothing to point at
    /// would be a refusal nobody can act on.
    pub errors: usize,
}

/// Turns the text of a pack file into a pack, the honest way.
///
/// `check` first, refuse on any error, `parse` after.
///
/// # Errors
///
/// Returns [`Refused`] with the number of errors that block the pack, and with
/// exactly one error when nothing found a fault and nothing could be built.
/// That combination means the two halves of the format disagree with each other,
/// which is this tool's own defect - reported as a refusal rather than as a pack
/// that quietly vanished, for the same reason a listing never drops one.
pub fn load(format: &dyn PackFormat, id: &str, text: &str) -> Result<LoadedPack, Refused> {
    let problems = format.check(text, id);
    let errors = problems
        .iter()
        .filter(|problem| problem.severity() == Severity::Error)
        .count();
    if errors > 0 {
        return Err(Refused { errors });
    }

    let warnings = problems.len() - errors;

    let Some(pack) = format.parse(text) else {
        return Err(Refused { errors: 1 });
    };

    Ok(LoadedPack { pack, warnings })
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;
    use crate::ports::{Date, TranslationCheck, TranslationTarget};
    use nkb_core::lint::{LintProblem, RuleCode};

    /// A format that answers exactly what a test tells it to, so that the order
    /// of the two calls is what is being examined rather than any parser.
    struct Scripted {
        problems: Vec<LintProblem>,
        pack: Option<Pack>,
        /// What actually happened, in order. The point of the whole fixture.
        calls: std::cell::RefCell<Vec<&'static str>>,
    }

    impl Scripted {
        fn new(problems: Vec<LintProblem>, pack: Option<Pack>) -> Self {
            Self {
                problems,
                pack,
                calls: std::cell::RefCell::new(Vec::new()),
            }
        }

        fn calls(&self) -> Vec<&'static str> {
            self.calls.borrow().clone()
        }
    }

    impl PackFormat for Scripted {
        fn check(&self, _text: &str, _expected_id: &str) -> Vec<LintProblem> {
            self.calls.borrow_mut().push("check");
            self.problems.clone()
        }

        fn parse(&self, _text: &str) -> Option<Pack> {
            self.calls.borrow_mut().push("parse");
            self.pack.clone()
        }

        fn translated_pack(&self, _text: &str) -> TranslationTarget {
            TranslationTarget::NotATranslation
        }

        fn check_translation(&self, _text: &str, _translated: &str) -> TranslationCheck {
            TranslationCheck::Compared(Vec::new())
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

    /// Written out rather than defaulted: Pack has no Default, and inventing
    /// one for a test would be a second way to build a pack.
    fn a_pack() -> Pack {
        Pack {
            id: "probe".to_owned(),
            name: "Probe".to_owned(),
            description: "A pack for the tests in this module.".to_owned(),
            version: "1.0".to_owned(),
            updated: "2026-09-08".to_owned(),
            license: "CC-BY-4.0".to_owned(),
            authors: vec!["Naughty Keyboard".to_owned()],
            language: "en".to_owned(),
            risk: nkb_core::pack::Risk::Normal,
            tags: Vec::new(),
            fields: vec!["any".to_owned()],
            source: None,
            values: Vec::new(),
            pairs: Vec::new(),
        }
    }

    #[test]
    fn a_clean_pack_is_checked_first_and_parsed_second() {
        // The binding order, asserted as an order rather than as an outcome. A
        // version that parsed first would return the same pack here.
        let format = Scripted::new(Vec::new(), Some(a_pack()));
        let loaded = load(&format, "probe", "format = 1\n").expect("a clean pack loads");

        assert_eq!(format.calls(), vec!["check", "parse"]);
        assert_eq!(loaded.pack.id, "probe");
        assert_eq!(loaded.warnings, 0);
    }

    #[test]
    fn one_error_means_the_file_is_never_parsed_at_all() {
        // Not merely "returns an error". `pack-format.md` 11 refuses the whole
        // pack, and a parse that ran anyway would be a parse of a file already
        // known to be wrong - which is where a plausible and wrong model comes
        // from.
        let format = Scripted::new(
            vec![LintProblem::new(RuleCode::NotValidToml).at(1)],
            Some(a_pack()),
        );

        assert_eq!(
            load(&format, "probe", "").unwrap_err(),
            Refused { errors: 1 }
        );
        assert_eq!(format.calls(), vec!["check"], "parse must not have run");
    }

    #[test]
    fn warnings_do_not_stop_a_pack_and_are_carried_out() {
        let format = Scripted::new(
            vec![LintProblem::new(RuleCode::PackWithoutTags).at(3)],
            Some(a_pack()),
        );
        let loaded = load(&format, "probe", "format = 1\n").expect("a warning does not block");

        assert_eq!(loaded.warnings, 1);
        assert_eq!(format.calls(), vec!["check", "parse"]);
    }

    #[test]
    fn a_file_that_checks_clean_and_will_not_parse_is_refused_rather_than_lost() {
        // The tool's own defect. Reported as one error, never as an absent pack:
        // a pack that vanishes from a listing sends somebody to look for a
        // missing download.
        let format = Scripted::new(Vec::new(), None);

        assert_eq!(
            load(&format, "probe", "format = 1\n").unwrap_err(),
            Refused { errors: 1 }
        );
        assert_eq!(format.calls(), vec!["check", "parse"]);
    }

    #[test]
    fn errors_are_counted_and_warnings_are_not_counted_among_them() {
        let format = Scripted::new(
            vec![
                LintProblem::new(RuleCode::NotValidToml).at(1),
                LintProblem::new(RuleCode::PackWithoutTags).at(2),
                LintProblem::new(RuleCode::MissingRequiredField).at(3),
            ],
            Some(a_pack()),
        );

        assert_eq!(
            load(&format, "probe", "").unwrap_err(),
            Refused { errors: 2 }
        );
    }
}
