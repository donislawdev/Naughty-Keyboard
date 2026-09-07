//! Starting a pack from nothing: the use case behind `nkb new-pack`.
//!
//! # Half of the published contract
//!
//! There is no English specification of the pack format and there is
//! deliberately not going to be one. A contributor meets the format through the
//! file this command writes and the verdict `nkb lint` gives, which makes these
//! two commands the whole of it - so the skeleton has to pass the linter, and a
//! test says so rather than a comment.
//!
//! # What this refuses to do
//!
//! It writes one file, under the name it was given, and only where nothing is
//! already standing. Both halves of that are deliberate and neither is caution:
//! a name that is not a pack identifier becomes a path if anything lets it, and
//! a file that is already there is somebody's work.

use crate::ports::{Clock, PackFormat, PackSink, SinkError};
use nkb_core::identity::is_pack_id;

/// What came of asking for a new pack.
///
/// Four answers rather than "worked" and "did not", because each of the three
/// failures sends the reader somewhere different: correct the name, pick another
/// one, or fix the folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NewPackOutcome {
    /// The file was written. Carries what to run next.
    Created { file: String },
    /// The name is not a pack identifier, so nothing was written and nothing was
    /// looked for.
    ///
    /// 🔴 Checked before the name reaches anything that turns it into a path.
    /// A pack identifier has no dot and no separator in it, so this is a refusal
    /// rather than a cleaning step - the same gate the validator puts on the
    /// `translates` field, for the same reason.
    NameNotAnIdentifier,
    /// A file of that name is already there. Never overwritten.
    AlreadyExists { file: String },
    /// The folder could not be written to.
    Unwritable { file: String },
}

/// Writes the skeleton for a new pack.
///
/// Takes all three collaborators as parameters, so the whole use case runs in a
/// test with no disk and no clock.
#[must_use]
pub fn new_pack(
    sink: &dyn PackSink,
    format: &dyn PackFormat,
    clock: &dyn Clock,
    id: &str,
) -> NewPackOutcome {
    if !is_pack_id(id) {
        return NewPackOutcome::NameNotAnIdentifier;
    }

    let file = format!("{id}.toml");
    match sink.create(id, &format.skeleton(id, clock.today())) {
        Ok(()) => NewPackOutcome::Created { file },
        Err(SinkError::AlreadyExists) => NewPackOutcome::AlreadyExists { file },
        Err(SinkError::Unwritable) => NewPackOutcome::Unwritable { file },
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
    use crate::ports::{Date, TranslationCheck, TranslationTarget};
    use nkb_core::lint::LintProblem;
    use std::cell::RefCell;

    struct Fixed(Date);
    impl Clock for Fixed {
        fn today(&self) -> Date {
            self.0
        }
    }

    /// A format that reports what it was asked for, so the use case can be
    /// exercised with no parser and no template.
    struct Templating;
    impl PackFormat for Templating {
        fn check(&self, _text: &str, _expected_id: &str) -> Vec<LintProblem> {
            Vec::new()
        }
        fn translated_pack(&self, _text: &str) -> TranslationTarget {
            TranslationTarget::NotATranslation
        }
        fn check_translation(&self, _text: &str, _translated: &str) -> TranslationCheck {
            TranslationCheck::Compared(Vec::new())
        }
        fn skeleton(&self, id: &str, today: Date) -> String {
            format!("id = {id}, written on {today:?}")
        }
    }

    /// Remembers what it was handed, and can be told to fail.
    struct Recording {
        written: RefCell<Vec<(String, String)>>,
        answer: Result<(), SinkError>,
    }

    impl Recording {
        fn accepting() -> Self {
            Self {
                written: RefCell::new(Vec::new()),
                answer: Ok(()),
            }
        }

        fn refusing(error: SinkError) -> Self {
            Self {
                written: RefCell::new(Vec::new()),
                answer: Err(error),
            }
        }
    }

    impl PackSink for Recording {
        fn create(&self, id: &str, text: &str) -> Result<(), SinkError> {
            self.written
                .borrow_mut()
                .push((id.to_owned(), text.to_owned()));
            self.answer
        }
    }

    fn on(day: Date) -> Fixed {
        Fixed(day)
    }

    #[test]
    fn a_new_pack_is_written_under_the_name_it_was_asked_for() {
        let sink = Recording::accepting();
        let outcome = new_pack(&sink, &Templating, &on((2026, 9, 7)), "locale-cz");

        assert_eq!(
            outcome,
            NewPackOutcome::Created {
                file: "locale-cz.toml".to_owned()
            }
        );
        let written = sink.written.borrow();
        assert_eq!(written.len(), 1);
        assert_eq!(written[0].0, "locale-cz");
        assert!(
            written[0].1.contains("(2026, 9, 7)"),
            "the date reaches the template rather than being read inside it: {}",
            written[0].1
        );
    }

    #[test]
    fn a_name_that_is_not_a_pack_identifier_never_reaches_the_sink() {
        // 🔴 The security half, and the same shape as the gate on `translates`:
        // the name is joined to a folder to make a path, so a value carrying a
        // parent marker would write outside the folder it was aimed at. The
        // identifier alphabet has no dot and no separator, which is what makes
        // this a refusal rather than a sanitisation.
        //
        // The count below proves the absence rather than trusting the comment.
        for hostile in [
            "../../../etc/passwd",
            "..",
            "/etc/passwd",
            "C:/Windows/win.ini",
            "locale-cz.toml",
            "Locale-CZ",
            "",
            "a",
        ] {
            let sink = Recording::accepting();
            let outcome = new_pack(&sink, &Templating, &on((2026, 9, 7)), hostile);
            assert_eq!(
                outcome,
                NewPackOutcome::NameNotAnIdentifier,
                "{hostile} must be refused"
            );
            assert_eq!(sink.written.borrow().len(), 0, "{hostile} reached the sink");
        }
    }

    #[test]
    fn a_name_already_taken_is_told_apart_from_a_folder_that_will_not_take_a_file() {
        // Two different repairs: pick another name, or fix the folder. A tool that
        // renders them the same sends the reader to do the wrong one.
        let taken = Recording::refusing(SinkError::AlreadyExists);
        assert_eq!(
            new_pack(&taken, &Templating, &on((2026, 9, 7)), "locale-cz"),
            NewPackOutcome::AlreadyExists {
                file: "locale-cz.toml".to_owned()
            }
        );

        let locked = Recording::refusing(SinkError::Unwritable);
        assert_eq!(
            new_pack(&locked, &Templating, &on((2026, 9, 7)), "locale-cz"),
            NewPackOutcome::Unwritable {
                file: "locale-cz.toml".to_owned()
            }
        );
    }

    #[test]
    fn the_shortest_and_longest_names_the_format_allows_are_both_accepted() {
        // The bounds are data in the core, and a use case that quietly disagreed
        // with them would refuse names the validator accepts - which is the two
        // halves of the contract disagreeing again, in the other direction.
        let sink = Recording::accepting();
        assert!(matches!(
            new_pack(&sink, &Templating, &on((2026, 9, 7)), "ab"),
            NewPackOutcome::Created { .. }
        ));

        let longest = format!("a{}", "b".repeat(39));
        assert_eq!(longest.chars().count(), 40);
        assert!(matches!(
            new_pack(&sink, &Templating, &on((2026, 9, 7)), &longest),
            NewPackOutcome::Created { .. }
        ));
    }
}
