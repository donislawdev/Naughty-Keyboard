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
