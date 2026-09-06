//! Reading pack files off a disk.
//!
//! Everything this module knows about is paths and bytes. It knows nothing about
//! what a pack means, which is why the layer above can be exercised with no file
//! system at all.
//!
//! # Untrusted input
//!
//! A pack file is a file from a stranger. It is read as bytes and turned into
//! text, and no part of that turns it into an instruction: the format has no
//! includes, no addresses and nothing executable, so reading a pack can do
//! nothing but add values to a list.

use nkb_app::{PackSource, SourceError};
use std::path::{Path, PathBuf};

/// Reads packs from one directory, addressing them by their pack identifier.
///
/// The identifier is the file name without its extension, which is the same
/// thing the format requires the pack to declare - so the mapping needs no index
/// and no configuration.
#[derive(Debug, Clone)]
pub struct DirectoryPackSource {
    directory: PathBuf,
}

impl DirectoryPackSource {
    #[must_use]
    pub fn new(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
        }
    }

    /// Splits a path a person typed into the directory to read from and the
    /// identifier to read, so that `nkb lint packs/whitespace.toml` works without
    /// the layer above ever learning what a path is.
    ///
    /// Returns nothing when the path has no file name at all - a bare directory,
    /// or a path ending in a parent marker.
    #[must_use]
    pub fn split(path: &Path) -> Option<(Self, String)> {
        let stem = path.file_stem()?.to_str()?.to_owned();
        let directory = path
            .parent()
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
        let directory = if directory.as_os_str().is_empty() {
            PathBuf::from(".")
        } else {
            directory
        };
        Some((Self::new(directory), stem))
    }

    #[must_use]
    pub fn path_of(&self, id: &str) -> PathBuf {
        self.directory.join(format!("{id}.toml"))
    }
}

impl PackSource for DirectoryPackSource {
    fn read(&self, id: &str) -> Result<String, SourceError> {
        let path = self.path_of(id);
        let bytes = std::fs::read(&path).map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => SourceError::NotFound,
            _ => SourceError::Unreadable,
        })?;

        // Read as bytes and converted here rather than read as text, so that a
        // file which is not valid UTF-8 comes back as its own answer instead of
        // as a generic read failure. The format has a rule about exactly that,
        // and it cannot fire on an error that lost the distinction.
        String::from_utf8(bytes).map_err(|_| SourceError::NotUtf8)
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;

    #[test]
    fn a_path_splits_into_a_directory_and_an_identifier() {
        let (source, id) = DirectoryPackSource::split(Path::new("packs/whitespace.toml"))
            .expect("a path with a file name splits");
        assert_eq!(id, "whitespace");
        assert_eq!(
            source.path_of(&id),
            PathBuf::from("packs").join("whitespace.toml")
        );
    }

    #[test]
    fn a_bare_file_name_reads_from_the_current_directory() {
        // The shape a contributor actually types, standing in the pack folder.
        let (source, id) =
            DirectoryPackSource::split(Path::new("whitespace.toml")).expect("splits");
        assert_eq!(id, "whitespace");
        assert_eq!(
            source.path_of(&id),
            PathBuf::from(".").join("whitespace.toml")
        );
    }

    #[test]
    fn a_translation_file_keeps_its_language_in_the_identifier() {
        // `unicode-text.pl.toml` is one file whose stem is `unicode-text.pl`.
        // Losing the language half here would make the tool read the source pack
        // when asked for its translation.
        let (_, id) =
            DirectoryPackSource::split(Path::new("unicode-text.pl.toml")).expect("splits");
        assert_eq!(id, "unicode-text.pl");
    }

    #[test]
    fn a_path_without_a_file_name_does_not_pretend_to_be_a_pack() {
        assert!(DirectoryPackSource::split(Path::new("..")).is_none());
    }

    #[test]
    fn a_missing_file_is_reported_as_missing_rather_than_as_unreadable() {
        // The two mean different things to whoever runs this: one is a typo in a
        // name, the other is a permission or a device. Collapsing them would send
        // the reader to fix the wrong thing.
        let source = DirectoryPackSource::new("no-such-directory-here");
        assert_eq!(source.read("absent"), Err(SourceError::NotFound));
    }
}
