//! Writing one pack file in canonical shape: the use case behind `nkb fmt`.
//!
//! # Why a formatter is the riskiest command in this tool
//!
//! Everything else here reads. This one rewrites a file somebody curated by
//! hand, and the first pack in the catalogue is made of characters nobody can
//! see - a lost escape leaves a value that still parses, still loads, and no
//! longer tests anything. Nothing on screen would say so.
//!
//! So the order below is: format, **check what came out**, and only then write.
//! The check is a refusal rather than a warning, because a warning about a file
//! that has already been overwritten arrives after the damage.

use crate::ports::{PackFormat, PackSink, PackSource, SinkError, SourceError};

/// What came of asking to format one pack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormatOutcome {
    /// The file was already in shape. Nothing was written, and that is the
    /// answer rather than a failure to act.
    AlreadyCanonical,
    /// The file was rewritten.
    Formatted,
    /// The file is not in shape and nothing was written, because the run was
    /// asked to change nothing.
    WouldFormat,
    /// Not TOML, so there was nothing inside to put in order.
    DidNotParse,
    /// There is no such file.
    NotFound,
    /// The file is there and would not open.
    Unreadable,
    /// The file could not be written back.
    Unwritable,
    /// 🔴 Formatting changed what the pack would insert. Nothing was written.
    ///
    /// This is a fault in this tool rather than in the file, and it is reported
    /// as loudly as the format allows: a formatter that quietly rewrote a value
    /// would be the exact failure this product exists to find in other people's
    /// software.
    WouldChangeValues,
}

/// Formats one pack file in place.
///
/// `dry_run` is the preview the rule set requires of every write: it does the
/// whole job apart from the writing, so the answer it gives is the answer the
/// real run would give rather than a separate opinion about it.
#[must_use]
pub fn format_pack(
    source: &dyn PackSource,
    sink: &dyn PackSink,
    format: &dyn PackFormat,
    id: &str,
    dry_run: bool,
) -> FormatOutcome {
    let text = match source.read(id) {
        Ok(text) => text,
        Err(SourceError::NotFound) => return FormatOutcome::NotFound,
        Err(SourceError::Unreadable) => return FormatOutcome::Unreadable,
        // Not UTF-8 is a finding about the pack, E005, and the linter is where
        // findings are reported. Here it simply means there is no text to format.
        Err(SourceError::NotUtf8) => return FormatOutcome::DidNotParse,
    };

    let Some(canonical) = format.canonical(&text) else {
        return FormatOutcome::DidNotParse;
    };

    if canonical == text {
        return FormatOutcome::AlreadyCanonical;
    }

    // Before anything reaches a disk. Asked of the two texts rather than of the
    // formatter's own opinion of itself, so a fault in the formatter cannot also
    // decide whether the formatter was faulty.
    if !format.same_insertions(&text, &canonical) {
        return FormatOutcome::WouldChangeValues;
    }

    if dry_run {
        return FormatOutcome::WouldFormat;
    }

    match sink.replace(id, &canonical) {
        Ok(()) => FormatOutcome::Formatted,
        // A sink asked to replace has no "already there" answer to give: the
        // file being there is the premise. Both failures mean the same repair.
        Err(SinkError::Unwritable | SinkError::AlreadyExists) => FormatOutcome::Unwritable,
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;
    use crate::ports::{Date, TranslationCheck, TranslationTarget};
    use nkb_core::lint::LintProblem;
    use std::cell::RefCell;

    struct OnePack(&'static str);

    impl PackSource for OnePack {
        fn read(&self, _id: &str) -> Result<String, SourceError> {
            Ok(self.0.to_owned())
        }
    }

    struct Missing(SourceError);

    impl PackSource for Missing {
        fn read(&self, _id: &str) -> Result<String, SourceError> {
            Err(self.0)
        }
    }

    /// Records what it was asked to write, and whether it was asked at all.
    #[derive(Default)]
    struct Recorder {
        written: RefCell<Vec<String>>,
        refuse: bool,
    }

    impl PackSink for Recorder {
        fn create(&self, _id: &str, _text: &str) -> Result<(), SinkError> {
            unreachable!("formatting never creates a file")
        }
        fn replace(&self, _id: &str, text: &str) -> Result<(), SinkError> {
            if self.refuse {
                return Err(SinkError::Unwritable);
            }
            self.written.borrow_mut().push(text.to_owned());
            Ok(())
        }
    }

    /// A formatter with no parser: it returns whatever it was told to.
    struct Fake {
        canonical: Option<&'static str>,
        same: bool,
    }

    impl PackFormat for Fake {
        fn check(&self, _text: &str, _expected_id: &str) -> Vec<LintProblem> {
            Vec::new()
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
            self.canonical.map(ToOwned::to_owned)
        }
        fn same_insertions(&self, _before: &str, _after: &str) -> bool {
            self.same
        }
    }

    fn run(source: &dyn PackSource, sink: &Recorder, format: &Fake, dry: bool) -> FormatOutcome {
        format_pack(source, sink, format, "probe", dry)
    }

    #[test]
    fn a_file_already_in_shape_is_not_rewritten() {
        // Rewriting it would change nothing and touch the modification time of
        // every pack in a repository, which is how a formatter earns a
        // reputation for churn.
        let sink = Recorder::default();
        let outcome = run(
            &OnePack("same"),
            &sink,
            &Fake {
                canonical: Some("same"),
                same: true,
            },
            false,
        );
        assert_eq!(outcome, FormatOutcome::AlreadyCanonical);
        assert!(sink.written.borrow().is_empty());
    }

    #[test]
    fn a_file_out_of_shape_is_written_once_with_the_canonical_text() {
        let sink = Recorder::default();
        let outcome = run(
            &OnePack("messy"),
            &sink,
            &Fake {
                canonical: Some("tidy"),
                same: true,
            },
            false,
        );
        assert_eq!(outcome, FormatOutcome::Formatted);
        assert_eq!(*sink.written.borrow(), vec!["tidy".to_owned()]);
    }

    #[test]
    fn a_dry_run_touches_nothing_and_says_what_would_happen() {
        // The rule set asks for this on every write, with a guard rather than
        // good intentions: a preview that differs from the run it previews is
        // undetectable without reading the code.
        let sink = Recorder::default();
        let outcome = run(
            &OnePack("messy"),
            &sink,
            &Fake {
                canonical: Some("tidy"),
                same: true,
            },
            true,
        );
        assert_eq!(outcome, FormatOutcome::WouldFormat);
        assert!(sink.written.borrow().is_empty(), "a dry run wrote a file");
    }

    #[test]
    fn a_dry_run_over_a_file_in_shape_agrees_with_the_real_run() {
        // The two must not disagree about a file that needs nothing.
        let sink = Recorder::default();
        let format = Fake {
            canonical: Some("same"),
            same: true,
        };
        assert_eq!(
            run(&OnePack("same"), &sink, &format, true),
            FormatOutcome::AlreadyCanonical
        );
        assert_eq!(
            run(&OnePack("same"), &sink, &format, false),
            FormatOutcome::AlreadyCanonical
        );
        assert!(sink.written.borrow().is_empty());
    }

    #[test]
    fn formatting_that_would_change_a_value_refuses_and_writes_nothing() {
        // 🔴 The guard this use case exists around. A fault in the formatter has
        // to stop before the disk, not be reported after it.
        let sink = Recorder::default();
        let outcome = run(
            &OnePack("messy"),
            &sink,
            &Fake {
                canonical: Some("tidy but wrong"),
                same: false,
            },
            false,
        );
        assert_eq!(outcome, FormatOutcome::WouldChangeValues);
        assert!(
            sink.written.borrow().is_empty(),
            "a formatter that changed a value still wrote the file"
        );
    }

    #[test]
    fn a_file_that_does_not_parse_is_left_alone() {
        let sink = Recorder::default();
        let outcome = run(
            &OnePack("format = ="),
            &sink,
            &Fake {
                canonical: None,
                same: true,
            },
            false,
        );
        assert_eq!(outcome, FormatOutcome::DidNotParse);
        assert!(sink.written.borrow().is_empty());
    }

    #[test]
    fn the_three_ways_a_file_cannot_be_read_stay_apart() {
        let sink = Recorder::default();
        let format = Fake {
            canonical: Some("tidy"),
            same: true,
        };
        assert_eq!(
            run(&Missing(SourceError::NotFound), &sink, &format, false),
            FormatOutcome::NotFound
        );
        assert_eq!(
            run(&Missing(SourceError::Unreadable), &sink, &format, false),
            FormatOutcome::Unreadable
        );
        assert_eq!(
            run(&Missing(SourceError::NotUtf8), &sink, &format, false),
            FormatOutcome::DidNotParse
        );
    }

    #[test]
    fn a_write_that_fails_is_reported_rather_than_swallowed() {
        let sink = Recorder {
            written: RefCell::new(Vec::new()),
            refuse: true,
        };
        let outcome = run(
            &OnePack("messy"),
            &sink,
            &Fake {
                canonical: Some("tidy"),
                same: true,
            },
            false,
        );
        assert_eq!(outcome, FormatOutcome::Unwritable);
    }
}
