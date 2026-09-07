//! Ports: what this layer needs from the outside, stated as traits.
//!
//! Two things are absent on purpose and their absence is the point.
//!
//! There is **no network port**. Not "unused", not "disabled" - absent, so that
//! the promise of never sending anything anywhere is a property of the shape of
//! this program rather than a sentence in a document that nobody can check.
//!
//! There is **no port for reading a target field**. The tool is deliberately
//! blind until the version that introduces the input oracle, and leaving the
//! capability undeclared means it cannot be reached for by accident.

use nkb_core::lint::LintProblem;
use std::fmt;

/// Why a pack could not be provided. Carries no path and no free text, because
/// the layer that reports this decides how much to reveal to a person.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceError {
    /// Nothing to read at the requested location.
    NotFound,
    /// Present, but unreadable - permissions, a device error, a broken handle.
    Unreadable,
    /// Read, but not valid UTF-8, which the pack format requires.
    NotUtf8,
}

impl fmt::Display for SourceError {
    /// Deliberately terse and English-only: this is a developer-facing marker,
    /// not the text a user reads. User-visible wording is assembled one layer
    /// out, from translation keys.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let marker = match self {
            Self::NotFound => "not-found",
            Self::Unreadable => "unreadable",
            Self::NotUtf8 => "not-utf8",
        };
        f.write_str(marker)
    }
}

/// Supplies the raw text of a pack file.
///
/// Returns text rather than a parsed pack on purpose: parsing belongs to the
/// adapter that knows the file format, and a validator has to see the file as
/// it actually is - including the parts a lenient parser would forgive.
pub trait PackSource {
    /// Reads one pack by an identifier the implementation understands.
    ///
    /// # Errors
    ///
    /// Returns [`SourceError`] when the pack is missing, unreadable, or not
    /// valid UTF-8.
    fn read(&self, id: &str) -> Result<String, SourceError>;
}

/// Answers the questions that only a parsed pack file can answer.
///
/// # Why this is a port and not a function call
///
/// The layer above owns the order of the checks and the shape of the verdict.
/// The file format belongs one layer out: which parser, which syntax, and where
/// a line number comes from. Stating that boundary as a trait is what lets this
/// use case be exercised with no parser at all, and what keeps the dependency
/// arrow pointing one way rather than resting on somebody remembering it.
///
/// Note what this port does **not** do: it does not return a pack. A validator
/// has to see a file as it actually is, including the parts a lenient reader
/// would forgive, so it asks for problems rather than for a tidy model.
pub trait PackFormat {
    /// Returns every problem the parsed file reveals.
    ///
    /// Reports all of them rather than the first, with the single exception of a
    /// file that does not parse - there is nothing to look inside, so that one
    /// problem comes back alone.
    ///
    /// `expected_id` is the identifier the pack was read under, which for every
    /// implementation of [`PackSource`] that exists is the file name without its
    /// extension. The format requires a pack to declare that same identifier, so
    /// the check needs both halves - and only this layer knows the first one,
    /// while only the implementation below knows where in the file the second one
    /// is written. Splitting the rule between them would leave the line number
    /// behind, which is the difference between a report and a shrug.
    fn check(&self, text: &str, expected_id: &str) -> Vec<LintProblem>;

    /// Which pack this file translates, when it is a translation at all.
    ///
    /// Three answers rather than two, and the third is the reason this is not an
    /// `Option`. "Not a translation" and "a translation naming something that is
    /// not a pack identifier" send the layer above to do different things: the
    /// first means the translation rules do not apply, the second means they
    /// apply and cannot be run. An `Option` would render both as nothing and lose
    /// the second, which is exactly the silence the per file record exists for.
    fn translated_pack(&self, text: &str) -> TranslationTarget;

    /// The problems that only appear when the translation and the pack it
    /// translates are seen together.
    ///
    /// Takes both as text because the layer above holds no parser and the layer
    /// below holds no file system: one of them has to carry the bytes across, and
    /// text is what the source port already deals in.
    fn check_translation(&self, text: &str, translated: &str) -> TranslationCheck;
}

/// What a file's `translates` field points at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TranslationTarget {
    /// Not a translation: the file declares no `translates`.
    NotATranslation,
    /// A translation of the pack with this identifier, which is known to have the
    /// shape the format requires - so it may be looked for on a disk.
    ///
    /// 🔴 That guarantee is load bearing rather than tidy. A pack file comes from
    /// a stranger, and this identifier is joined to a directory to make a path.
    /// An implementation that returned raw text here would let a pack file reach
    /// outside the folder it lives in, so the shape is checked before the name
    /// leaves this port and never after.
    Pack(String),
    /// A translation whose `translates` names no pack: not text, or text outside
    /// the pack identifier alphabet. The rule about that has already been
    /// reported by [`PackFormat::check`]; nothing here can be resolved.
    Unusable,
}

/// What came of comparing a translation against the pack it translates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TranslationCheck {
    /// The two were compared. Whatever was wrong is in the list, and an empty
    /// list means the comparison happened and found nothing.
    Compared(Vec<LintProblem>),
    /// The pack being translated does not parse, so it holds no identifiers to
    /// compare against and nothing was compared.
    ///
    /// Told apart from `Compared(vec![])` on purpose: an empty comparison and an
    /// impossible one are the two answers this whole mechanism exists to keep
    /// apart. The problem is reported against **that** file when somebody lints
    /// it, not against this one.
    TranslatedPackDidNotParse,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// The reason ports exist: a use case can be exercised with no file system.
    struct InMemory(HashMap<String, String>);

    impl PackSource for InMemory {
        fn read(&self, id: &str) -> Result<String, SourceError> {
            self.0.get(id).cloned().ok_or(SourceError::NotFound)
        }
    }

    #[test]
    fn a_port_can_be_satisfied_without_touching_a_disk() {
        let mut packs = HashMap::new();
        packs.insert("magic-values".to_owned(), "format = 1\n".to_owned());
        let source = InMemory(packs);

        assert_eq!(source.read("magic-values").as_deref(), Ok("format = 1\n"));
        assert_eq!(source.read("absent"), Err(SourceError::NotFound));
    }

    #[test]
    fn source_errors_render_as_stable_markers() {
        assert_eq!(SourceError::NotFound.to_string(), "not-found");
        assert_eq!(SourceError::NotUtf8.to_string(), "not-utf8");
    }
}
