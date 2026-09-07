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

use nkb_app::{Clock, Date, PackSink, PackSource, SinkError, SourceError};
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

/// Writes new pack files into one directory.
///
/// Separate from [`DirectoryPackSource`] rather than a second method on it,
/// because the two carry opposite risks and it should take a deliberate act to
/// hand a component the ability to write.
#[derive(Debug, Clone)]
pub struct DirectoryPackSink {
    directory: PathBuf,
}

impl DirectoryPackSink {
    #[must_use]
    pub fn new(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
        }
    }
}

impl PackSink for DirectoryPackSink {
    /// # Why this is one call and not a check followed by a write
    ///
    /// 🔴 `create_new` asks the operating system to make the file **only if it is
    /// not there**, and to say so otherwise - one indivisible step. Asking
    /// whether the file exists and then writing it is two steps with a gap, and
    /// something can arrive in that gap: the shape of race the architecture notes
    /// call TOCTOU. Here what would arrive in the gap is somebody's work, and
    /// what would happen to it is deletion.
    fn create(&self, id: &str, text: &str) -> Result<(), SinkError> {
        let path = self.directory.join(format!("{id}.toml"));
        let mut file = match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                return Err(SinkError::AlreadyExists);
            }
            Err(_) => return Err(SinkError::Unwritable),
        };
        std::io::Write::write_all(&mut file, text.as_bytes()).map_err(|_| SinkError::Unwritable)
    }
}

/// Today's date, from the machine this is running on.
#[derive(Debug, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    /// Falls back to the day the pack format was settled if the machine reports a
    /// time before 1970. That is a clock nobody can trust, and a skeleton with a
    /// wrong date is a smaller problem than one that refuses to be written - the
    /// person editing it will change the date anyway.
    fn today(&self) -> Date {
        let seconds = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_secs());
        // Whole days, which is all a date needs. Leap seconds do not exist in
        // Unix time, so this division is exact rather than approximate.
        civil_from_days(i64::try_from(seconds / 86_400).unwrap_or(0))
    }
}

/// Turns a count of days since 1970-01-01 into a calendar date.
///
/// # Why this is written out rather than taken from a crate
///
/// A date library would be a dependency, and a dependency in this project costs
/// a justification and a licence check against GPL-3.0. This is the published
/// civil-from-days algorithm, it is exact for every date this program will ever
/// see, and it is shorter than the argument for adding a crate to avoid it.
///
/// The arithmetic shifts the year to start in March, which puts the leap day at
/// the end of the year and removes every special case for February.
fn civil_from_days(days: i64) -> Date {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };

    (
        i32::try_from(year + i64::from(month <= 2)).unwrap_or(1970),
        u32::try_from(month).unwrap_or(1),
        u32::try_from(day).unwrap_or(1),
    )
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;

    #[test]
    fn the_calendar_arithmetic_agrees_with_dates_that_can_be_looked_up() {
        // Fixed points rather than a property, because the failure mode of date
        // arithmetic is being wrong by one day in a corner nobody visits. Each of
        // these is a day somebody can check against a calendar.
        for (days, expected) in [
            (0_i64, (1970, 1, 1)),
            (1, (1970, 1, 2)),
            (58, (1970, 2, 28)),
            (59, (1970, 3, 1)),
            // 2000 was a leap year and 1900 was not - the rule that catches naive
            // implementations, and the reason the century terms exist above.
            (11_016, (2000, 2, 29)),
            (11_017, (2000, 3, 1)),
            // 2100 is not a leap year, so this pair straddles a February that has
            // twenty eight days in a year divisible by four.
            (47_540, (2100, 2, 28)),
            (47_541, (2100, 3, 1)),
            (20_703, (2026, 9, 7)),
            (20_704, (2026, 9, 8)),
            (-1, (1969, 12, 31)),
        ] {
            assert_eq!(civil_from_days(days), expected, "day {days}");
        }
    }

    #[test]
    fn a_new_pack_is_written_once_and_never_over_something_already_there() {
        // 🔴 The refusal that keeps somebody's hour of work. Written as a real
        // file rather than against a double, because what is being trusted here is
        // the operating system's behaviour and not ours.
        let dir = std::env::temp_dir().join(format!("nkb-sink-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a temporary directory");
        let sink = DirectoryPackSink::new(&dir);

        assert_eq!(sink.create("locale-cz", "first"), Ok(()));
        let written = |dir: &std::path::Path| {
            std::fs::read_to_string(dir.join("locale-cz.toml")).expect("the file is there")
        };
        assert_eq!(written(&dir), "first");

        assert_eq!(
            sink.create("locale-cz", "second"),
            Err(SinkError::AlreadyExists)
        );
        assert_eq!(
            written(&dir),
            "first",
            "the refused write must leave the file exactly as it was"
        );

        std::fs::remove_file(dir.join("locale-cz.toml")).ok();
        std::fs::remove_dir(&dir).ok();
    }

    #[test]
    fn a_folder_that_is_not_there_is_a_place_that_cannot_be_written_to() {
        // Told apart from a name already taken, because the repairs differ: make
        // the folder, or choose another name.
        let sink = DirectoryPackSink::new("no-such-directory-here");
        assert_eq!(sink.create("locale-cz", "x"), Err(SinkError::Unwritable));
    }

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
