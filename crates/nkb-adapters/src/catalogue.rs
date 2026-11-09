//! The catalogue the palette and `nkb` offer: the packs the tool carries, a
//! folder a team shares and the tester's own folder (`pack-format.md` 12,
//! `D173`).
//!
//! # Three sources, one list, nothing hidden
//!
//! The sources are read in that order, and a pack in a later one overrides a
//! pack of the same identifier in an earlier one: the own folder over the team
//! folder over the built-in packs. The override is never silent - every pack
//! says where it comes from and what it overrides ([`PackOrigin`]), because a
//! tester typing a team's `whitespace` while believing it to be the built-in one
//! types other values than the ones they chose.
//!
//! 🔴 A pack that overrides another and cannot be read, or does not load, is
//! still the one in use: the pack behind it is NOT taken in its place. The
//! person learns that their pack is broken, rather than quietly getting another.
//!
//! # And it says what it did not read
//!
//! A source that was not read - no folder named, a folder that is missing,
//! unreadable or not answering - travels beside the list as the
//! [`CatalogueCoverage`], the same way `nkb lint` carries the rules it did not
//! check beside the problems it found. Untouchable rule 1: a listing from one
//! source of three looks exactly like a complete one.
//!
//! # One snapshot per question
//!
//! [`PackFolders::catalogue`] lists both folders once, and everything the
//! snapshot answers - the list, a pack, where it comes from - agrees with that
//! listing. A new snapshot is taken for each opening of the value window and
//! each choice of a pack, so a pack file fixed on disk is seen at the next
//! look, without a restart.

use std::path::{Path, PathBuf};

use nkb_app::keep_settings::SettingsMessage;
use nkb_app::{
    CatalogueCoverage, CatalogueSource, Found, Listed, PackCatalogue, PackOrigin, PackSource,
    Settings, SettingsNote, SourceError, SourceSkipped,
};

use crate::built_in::BUILT_IN_PACKS;
use crate::pack_folder::{FolderListing, PackFolder};
use crate::settings_file::SettingsFile;

/// The packs carried inside the executable - the first of the three sources,
/// and, on its own, a catalogue told of no folder.
#[derive(Debug, Clone, Copy, Default)]
pub struct BuiltInCatalogue;

impl BuiltInCatalogue {
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    fn holds(id: &str) -> bool {
        BUILT_IN_PACKS.iter().any(|(name, _)| *name == id)
    }
}

impl PackSource for BuiltInCatalogue {
    fn read(&self, id: &str) -> Result<String, SourceError> {
        BUILT_IN_PACKS
            .iter()
            .find(|(name, _)| *name == id)
            .map(|(_, text)| (*text).to_owned())
            .ok_or(SourceError::NotFound)
    }
}

impl PackCatalogue for BuiltInCatalogue {
    fn list(&self) -> Result<Vec<Listed>, SourceError> {
        Ok(BUILT_IN_PACKS
            .iter()
            .map(|(id, _)| Listed {
                id: (*id).to_owned(),
                origin: PackOrigin::built_in(),
            })
            .collect())
    }

    fn find(&self, id: &str) -> Option<Found> {
        Self::holds(id).then(|| Found {
            origin: PackOrigin::built_in(),
            text: self.read(id),
        })
    }

    fn coverage(&self) -> CatalogueCoverage {
        CatalogueCoverage {
            consulted: vec![CatalogueSource::BuiltIn],
            skipped: vec![
                (CatalogueSource::Team, SourceSkipped::NotSet),
                (CatalogueSource::Own, SourceSkipped::NotSet),
            ],
            misnamed: Vec::new(),
        }
    }
}

/// Where the two folders are for this run, or why one is not looked at.
///
/// Built once - from the settings file and, in `nkb`, the `--team-folder`
/// option - and kept for the run: it holds what each folder remembers about
/// questions still hanging on it ([`PackFolder`]).
#[derive(Debug, Clone)]
pub struct PackFolders {
    team: Result<PackFolder, SourceSkipped>,
    own: Result<PackFolder, SourceSkipped>,
}

impl PackFolders {
    /// The team folder and the own folder, each either a path or the reason
    /// there is none.
    #[must_use]
    pub fn new(team: Result<PathBuf, SourceSkipped>, own: Result<PathBuf, SourceSkipped>) -> Self {
        Self {
            team: team.map(PackFolder::new),
            own: own.map(PackFolder::new),
        }
    }

    /// Two folders already made - for the tests, which give them a short wait.
    #[must_use]
    pub fn of(
        team: Result<PackFolder, SourceSkipped>,
        own: Result<PackFolder, SourceSkipped>,
    ) -> Self {
        Self { team, own }
    }

    /// The folders of one run of the palette or of `nkb` (`D173`): the team
    /// folder named on the command line when it is, else the one the settings
    /// name. The own folder is beside the settings file.
    ///
    /// `settings` and `said` are what [`nkb_app::keep_settings::KeptSettings`]
    /// read and said - the palette has them already, and `nkb` reads them the
    /// same way, so the two programs cannot disagree about which folder the
    /// settings name. A file this version cannot use, or a `team-folder` that
    /// is not a full path, is "could not be used", never "not set": the tester
    /// set something, and silence would read as the setting not existing.
    #[must_use]
    pub fn for_run(
        file: &SettingsFile,
        settings: &Settings,
        said: &[SettingsMessage],
        team_option: Option<PathBuf>,
    ) -> Self {
        let team = match (team_option, &settings.team_folder) {
            (Some(path), _) => Ok(path),
            (None, Some(path)) => Ok(PathBuf::from(path)),
            (None, None) if said.iter().any(spoils_the_team_folder) => {
                Err(SourceSkipped::SettingUnusable)
            }
            (None, None) => Err(SourceSkipped::NotSet),
        };
        let own = file.own_packs_folder().ok_or(SourceSkipped::NoPlace);
        Self::new(team, own)
    }

    /// No folder at all: the built-in packs alone.
    #[must_use]
    pub fn none() -> Self {
        Self {
            team: Err(SourceSkipped::NotSet),
            own: Err(SourceSkipped::NotSet),
        }
    }

    /// The team folder as named, or why there is none - for a sentence.
    ///
    /// # Errors
    ///
    /// The reason no team folder is looked at.
    pub fn team(&self) -> Result<&Path, SourceSkipped> {
        self.team.as_ref().map(PackFolder::path).map_err(|why| *why)
    }

    /// The own folder as placed, or why there is none - for a sentence.
    ///
    /// # Errors
    ///
    /// The reason no own folder is looked at.
    pub fn own(&self) -> Result<&Path, SourceSkipped> {
        self.own.as_ref().map(PackFolder::path).map_err(|why| *why)
    }

    /// The folder `source` names, when it is one and is set.
    #[must_use]
    pub fn path_of(&self, source: CatalogueSource) -> Option<&Path> {
        match source {
            CatalogueSource::BuiltIn => None,
            CatalogueSource::Team => self.team().ok(),
            CatalogueSource::Own => self.own().ok(),
        }
    }

    /// Makes the own folder when it is not there yet - the pack folders window's
    /// "Create and open" (`D175`). Reading never makes it (`D173`), the tester's
    /// click does, and a folder that is there already is left as it is.
    ///
    /// # Errors
    ///
    /// The system's, when the folder cannot be made - and `NotFound` when this
    /// machine gives the settings no place, so there is no own folder at all.
    pub fn make_own(&self) -> std::io::Result<()> {
        let own = self
            .own()
            .map_err(|_| std::io::Error::from(std::io::ErrorKind::NotFound))?;
        std::fs::create_dir_all(own)
    }

    /// These folders with another team folder, or none - what the pack folders
    /// window offers before it is saved (`D175`). The own folder is the same
    /// one, with what it remembers about questions still hanging on it.
    #[must_use]
    pub fn with_team(&self, team: Option<PathBuf>) -> Self {
        Self {
            team: team.map(PackFolder::new).ok_or(SourceSkipped::NotSet),
            own: self.own.clone(),
        }
    }

    /// A snapshot of the three sources as they are now.
    #[must_use]
    pub fn catalogue(&self) -> FolderCatalogue {
        let own = layer(&self.own);
        let mut team = layer(&self.team);
        // One folder named twice is read once, as the own folder: otherwise
        // every pack in it would override itself.
        if let (Ok(team_layer), Ok(own_layer)) = (&team, &own)
            && team_layer.listing.resolved == own_layer.listing.resolved
        {
            team = Err(SourceSkipped::SameAsOwn);
        }
        FolderCatalogue { team, own }
    }
}

/// Whether a thing said about the settings means a team folder that was set
/// could not be taken from them.
fn spoils_the_team_folder(said: &SettingsMessage) -> bool {
    match said {
        SettingsMessage::Unusable(_)
        | SettingsMessage::Note(SettingsNote::NotAFolderPath { .. }) => true,
        SettingsMessage::Note(SettingsNote::NotATable { key }) => key == "packs",
        _ => false,
    }
}

/// One folder as one snapshot found it.
#[derive(Debug, Clone)]
struct Layer {
    folder: PackFolder,
    listing: FolderListing,
}

impl Layer {
    fn holds(&self, id: &str) -> bool {
        self.listing
            .packs
            .binary_search_by(|pack| pack.as_str().cmp(id))
            .is_ok()
    }
}

fn layer(folder: &Result<PackFolder, SourceSkipped>) -> Result<Layer, SourceSkipped> {
    let folder = folder.as_ref().map_err(|why| *why)?;
    let listing = folder.list()?;
    Ok(Layer {
        folder: folder.clone(),
        listing,
    })
}

/// What one folder held in a snapshot, as the pack folders window says it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FolderHolds {
    /// Read: `count` packs, of which `instead` are used in place of a pack of
    /// the same name from a source read earlier, and `misnamed` `.toml` files
    /// whose name is not a pack identifier.
    Packs {
        count: usize,
        instead: usize,
        misnamed: usize,
    },
    /// Not read, and why.
    Skipped(SourceSkipped),
}

/// The three sources as one snapshot found them - what the palette, the value
/// window and `nkb` read packs from.
#[derive(Debug, Clone)]
pub struct FolderCatalogue {
    team: Result<Layer, SourceSkipped>,
    own: Result<Layer, SourceSkipped>,
}

impl FolderCatalogue {
    /// What the folder of `source` held in this snapshot, for the pack folders
    /// window (`D175`) - `None` for the built-in packs, which have no folder.
    #[must_use]
    pub fn holds_of(&self, source: CatalogueSource) -> Option<FolderHolds> {
        let layer = match source {
            CatalogueSource::BuiltIn => return None,
            CatalogueSource::Team => &self.team,
            CatalogueSource::Own => &self.own,
        };
        Some(match layer {
            Ok(layer) => FolderHolds::Packs {
                count: layer.listing.packs.len(),
                instead: layer
                    .listing
                    .packs
                    .iter()
                    .filter(|id| {
                        self.origin_of(id).is_some_and(|origin| {
                            origin.source == source && !origin.overrides.is_empty()
                        })
                    })
                    .count(),
                misnamed: layer.listing.misnamed.len(),
            },
            Err(why) => FolderHolds::Skipped(*why),
        })
    }

    /// The sources holding `id`, the one in use first.
    fn holders(&self, id: &str) -> Vec<CatalogueSource> {
        let mut holders = Vec::new();
        if self.own.as_ref().is_ok_and(|own| own.holds(id)) {
            holders.push(CatalogueSource::Own);
        }
        if self.team.as_ref().is_ok_and(|team| team.holds(id)) {
            holders.push(CatalogueSource::Team);
        }
        if BuiltInCatalogue::holds(id) {
            holders.push(CatalogueSource::BuiltIn);
        }
        holders
    }

    fn origin_of(&self, id: &str) -> Option<PackOrigin> {
        let mut holders = self.holders(id).into_iter();
        let source = holders.next()?;
        Some(PackOrigin {
            source,
            overrides: holders.collect(),
        })
    }
}

impl PackSource for FolderCatalogue {
    fn read(&self, id: &str) -> Result<String, SourceError> {
        self.find(id)
            .map_or(Err(SourceError::NotFound), |found| found.text)
    }
}

impl PackCatalogue for FolderCatalogue {
    /// The built-in packs in the order the tool offers them, each replaced in
    /// place by a pack that overrides it. Then the team's own packs, then the
    /// tester's, each alphabetically.
    fn list(&self) -> Result<Vec<Listed>, SourceError> {
        let mut ids: Vec<&str> = BUILT_IN_PACKS.iter().map(|(id, _)| *id).collect();
        for layer in [&self.team, &self.own].into_iter().flatten() {
            for id in &layer.listing.packs {
                if !ids.contains(&id.as_str()) {
                    ids.push(id.as_str());
                }
            }
        }
        Ok(ids
            .into_iter()
            .filter_map(|id| {
                self.origin_of(id).map(|origin| Listed {
                    id: id.to_owned(),
                    origin,
                })
            })
            .collect())
    }

    fn find(&self, id: &str) -> Option<Found> {
        let origin = self.origin_of(id)?;
        let text = match origin.source {
            CatalogueSource::BuiltIn => BuiltInCatalogue.read(id),
            CatalogueSource::Team => read_from(&self.team, id),
            CatalogueSource::Own => read_from(&self.own, id),
        };
        Some(Found { origin, text })
    }

    fn coverage(&self) -> CatalogueCoverage {
        let mut coverage = CatalogueCoverage {
            consulted: vec![CatalogueSource::BuiltIn],
            skipped: Vec::new(),
            misnamed: Vec::new(),
        };
        for (source, layer) in [
            (CatalogueSource::Team, &self.team),
            (CatalogueSource::Own, &self.own),
        ] {
            match layer {
                Ok(layer) => {
                    coverage.consulted.push(source);
                    coverage.misnamed.extend(
                        layer
                            .listing
                            .misnamed
                            .iter()
                            .map(|name| (source, name.clone())),
                    );
                }
                Err(why) => coverage.skipped.push((source, *why)),
            }
        }
        coverage
    }
}

fn read_from(layer: &Result<Layer, SourceSkipped>, id: &str) -> Result<String, SourceError> {
    match layer {
        Ok(layer) => layer.folder.read(id),
        // Unreachable: a source that was not read holds no pack to be found
        // in it. Said as a missing pack rather than trusted to never happen.
        Err(_) => Err(SourceError::NotFound),
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
    use std::time::Duration;

    /// A folder of its own in the system's temporary place, removed when
    /// dropped.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir()
                .join("nkb-catalogue-tests")
                .join(format!("{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("a scratch folder");
            Self(path)
        }
        fn sub(&self, name: &str) -> PathBuf {
            let path = self.0.join(name);
            std::fs::create_dir_all(&path).expect("a scratch sub folder");
            path
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn write(folder: &Path, name: &str, text: &str) {
        std::fs::write(folder.join(name), text).expect("a scratch file");
    }

    fn folders(team: &Path, own: &Path) -> PackFolders {
        PackFolders::of(
            Ok(PackFolder::with_wait(team, Duration::from_secs(5))),
            Ok(PackFolder::with_wait(own, Duration::from_secs(5))),
        )
    }

    /// `D175`: what the pack folders window says of each folder - how many packs,
    /// how many are used in place of another, how many files are not packs - and
    /// why a folder was not read. A team pack the own folder overrides is in the
    /// team folder's count and not among the ones it puts in place of another.
    #[test]
    fn each_folder_says_how_many_packs_it_holds_and_how_many_win() {
        let scratch = Scratch::new("holds");
        let team = scratch.sub("team");
        let own = scratch.sub("own");
        write(&team, "whitespace.toml", "x");
        write(&team, "alpha.toml", "x");
        write(&team, "Not A Pack.toml", "x");
        write(&own, "alpha.toml", "x");
        let catalogue = folders(&team, &own).catalogue();
        assert_eq!(
            catalogue.holds_of(CatalogueSource::Team),
            Some(FolderHolds::Packs {
                count: 2,
                instead: 1,
                misnamed: 1
            })
        );
        assert_eq!(
            catalogue.holds_of(CatalogueSource::Own),
            Some(FolderHolds::Packs {
                count: 1,
                instead: 1,
                misnamed: 0
            })
        );
        assert_eq!(catalogue.holds_of(CatalogueSource::BuiltIn), None);
        let gone = folders(&scratch.0.join("gone"), &own).catalogue();
        assert_eq!(
            gone.holds_of(CatalogueSource::Team),
            Some(FolderHolds::Skipped(SourceSkipped::Missing))
        );
        let later = scratch.0.join("later");
        let unmade = PackFolders::new(Err(SourceSkipped::NotSet), Ok(later.clone()));
        assert!(!later.exists(), "reading made the own folder");
        unmade
            .make_own()
            .expect("the own folder is made on request");
        assert!(later.is_dir());
        unmade
            .make_own()
            .expect("a folder that is there is left as it is");
        let nowhere = PackFolders::new(Err(SourceSkipped::NotSet), Err(SourceSkipped::NoPlace));
        assert_eq!(
            nowhere.make_own().map_err(|error| error.kind()),
            Err(std::io::ErrorKind::NotFound)
        );
        let without = folders(&team, &own).with_team(None).catalogue();
        assert_eq!(
            without.holds_of(CatalogueSource::Team),
            Some(FolderHolds::Skipped(SourceSkipped::NotSet))
        );
    }

    #[test]
    fn the_folders_of_a_run_come_from_the_option_then_the_settings_and_say_why_not() {
        use nkb_app::SettingsUnusable;
        let file = SettingsFile::at(PathBuf::from("base").join("settings.toml"));
        let set = Settings {
            team_folder: Some(String::from("/work/qa")),
            ..Settings::default()
        };
        let none = Settings::default();

        let given = PackFolders::for_run(&file, &set, &[], Some(PathBuf::from("ci/packs")));
        assert_eq!(given.team(), Ok(Path::new("ci/packs")), "the option wins");
        assert_eq!(
            given.own(),
            Ok(PathBuf::from("base").join("packs").as_path())
        );
        let from_file = PackFolders::for_run(&file, &set, &[], None);
        assert_eq!(from_file.team(), Ok(Path::new("/work/qa")));
        assert_eq!(
            PackFolders::for_run(&file, &none, &[], None).team(),
            Err(SourceSkipped::NotSet)
        );
        for spoiled in [
            SettingsMessage::Unusable(SettingsUnusable::NotUtf8),
            SettingsMessage::Note(SettingsNote::NotAFolderPath {
                key: String::from("packs.team-folder"),
            }),
            SettingsMessage::Note(SettingsNote::NotATable {
                key: String::from("packs"),
            }),
        ] {
            assert_eq!(
                PackFolders::for_run(&file, &none, std::slice::from_ref(&spoiled), None).team(),
                Err(SourceSkipped::SettingUnusable),
                "{spoiled:?}"
            );
        }
        // A table of another name is not about the team folder.
        let other = SettingsMessage::Note(SettingsNote::NotATable {
            key: String::from("palette"),
        });
        assert_eq!(
            PackFolders::for_run(&file, &none, &[other], None).team(),
            Err(SourceSkipped::NotSet)
        );
    }

    #[test]
    fn every_listed_pack_can_also_be_read() {
        // The supertrait's promise, checked rather than assumed: a listing that
        // names something unreadable sends a caller to a dead end.
        let catalogue = BuiltInCatalogue::new();
        let listed = catalogue.list().expect("the carried list cannot fail");
        assert!(
            !listed.is_empty(),
            "an empty catalogue would pass every other assertion here"
        );
        for pack in listed {
            let text = catalogue
                .read(&pack.id)
                .unwrap_or_else(|e| panic!("{} was listed and cannot be read: {e}", pack.id));
            assert!(!text.is_empty(), "{} was listed and is empty", pack.id);
        }
    }

    #[test]
    fn a_pack_that_is_not_carried_is_not_found() {
        assert_eq!(
            BuiltInCatalogue::new().read("no-such-pack"),
            Err(SourceError::NotFound)
        );
        assert_eq!(BuiltInCatalogue::new().find("no-such-pack"), None);
    }

    #[test]
    fn with_no_folder_the_snapshot_is_the_built_in_catalogue_and_says_why() {
        let catalogue = PackFolders::none().catalogue();
        assert_eq!(
            catalogue.list().expect("listed"),
            BuiltInCatalogue::new().list().expect("listed")
        );
        let coverage = catalogue.coverage();
        assert_eq!(coverage.consulted, vec![CatalogueSource::BuiltIn]);
        assert_eq!(
            coverage.skipped,
            vec![
                (CatalogueSource::Team, SourceSkipped::NotSet),
                (CatalogueSource::Own, SourceSkipped::NotSet),
            ]
        );
    }

    #[test]
    fn all_three_sources_are_accounted_for_whatever_was_read() {
        // A fourth source added to the format with no line in the account
        // would slip through unmentioned.
        let scratch = Scratch::new("accounted");
        let own = scratch.sub("own");
        for catalogue in [
            PackFolders::none().catalogue(),
            folders(&scratch.0.join("missing"), &own).catalogue(),
        ] {
            let coverage = catalogue.coverage();
            let mut seen: Vec<CatalogueSource> = coverage.consulted.clone();
            seen.extend(coverage.skipped.iter().map(|(source, _)| *source));
            for source in [
                CatalogueSource::BuiltIn,
                CatalogueSource::Team,
                CatalogueSource::Own,
            ] {
                assert_eq!(
                    seen.iter().filter(|seen| **seen == source).count(),
                    1,
                    "{source:?} must be consulted or skipped, once"
                );
            }
        }
    }

    #[test]
    fn the_own_folder_overrides_the_team_folder_overrides_the_built_in_packs() {
        let scratch = Scratch::new("overrides");
        let team = scratch.sub("team");
        let own = scratch.sub("own");
        write(&team, "whitespace.toml", "team whitespace");
        write(&team, "zeta.toml", "team zeta");
        write(&team, "beta.toml", "team beta");
        write(&own, "whitespace.toml", "own whitespace");
        write(&own, "beta.toml", "own beta");
        write(&own, "alpha.toml", "own alpha");
        let catalogue = folders(&team, &own).catalogue();

        let found = catalogue.find("whitespace").expect("held");
        assert_eq!(found.text, Ok("own whitespace".to_owned()));
        assert_eq!(
            found.origin,
            PackOrigin {
                source: CatalogueSource::Own,
                overrides: vec![CatalogueSource::Team, CatalogueSource::BuiltIn],
            }
        );
        let beta = catalogue.find("beta").expect("held");
        assert_eq!(beta.text, Ok("own beta".to_owned()));
        assert_eq!(beta.origin.overrides, vec![CatalogueSource::Team]);
        let unicode = catalogue.find("unicode-text").expect("carried");
        assert_eq!(unicode.origin, PackOrigin::built_in());

        // The built-in order holds, the overriding pack in the place of the
        // one it overrides. The team's new packs next, then the tester's.
        let ids: Vec<String> = catalogue
            .list()
            .expect("listed")
            .into_iter()
            .map(|listed| listed.id)
            .collect();
        let carried: Vec<&str> = BUILT_IN_PACKS.iter().map(|(id, _)| *id).collect();
        assert_eq!(&ids[..carried.len()], carried.as_slice());
        assert_eq!(&ids[carried.len()..], ["beta", "zeta", "alpha"]);
        assert_eq!(
            catalogue.coverage().consulted,
            vec![
                CatalogueSource::BuiltIn,
                CatalogueSource::Team,
                CatalogueSource::Own
            ]
        );
    }

    #[test]
    fn an_overriding_pack_that_cannot_be_read_does_not_uncover_the_one_behind_it() {
        // 🔴 `D173`: the built-in whitespace is NOT handed over in place of a
        // team whitespace that is too large to read.
        let scratch = Scratch::new("no-fallback");
        let team = scratch.sub("team");
        let own = scratch.sub("own");
        let large = usize::try_from(nkb_app::PACK_FILE_LIMIT).expect("fits") + 1;
        write(&team, "whitespace.toml", &"x".repeat(large));
        let found = folders(&team, &own)
            .catalogue()
            .find("whitespace")
            .expect("held");
        assert_eq!(found.origin.source, CatalogueSource::Team);
        assert_eq!(found.text, Err(SourceError::TooLarge));
    }

    #[test]
    fn a_team_folder_that_is_the_own_folder_is_read_once_as_the_own_one() {
        let scratch = Scratch::new("same");
        let own = scratch.sub("own");
        write(&own, "alpha.toml", "alpha");
        let catalogue = folders(&own, &own).catalogue();
        assert_eq!(
            catalogue.find("alpha").expect("held").origin,
            PackOrigin {
                source: CatalogueSource::Own,
                overrides: Vec::new(),
            }
        );
        assert!(
            catalogue
                .coverage()
                .skipped
                .contains(&(CatalogueSource::Team, SourceSkipped::SameAsOwn))
        );
    }

    #[test]
    fn a_folder_that_is_missing_leaves_the_others_offered_and_is_named() {
        let scratch = Scratch::new("missing");
        let own = scratch.sub("own");
        write(&own, "alpha.toml", "alpha");
        write(&own, "Not A Pack.toml", "x");
        let catalogue = folders(&scratch.0.join("nowhere"), &own).catalogue();
        assert!(catalogue.find("alpha").is_some());
        assert!(catalogue.find("whitespace").is_some());
        let coverage = catalogue.coverage();
        assert!(
            coverage
                .skipped
                .contains(&(CatalogueSource::Team, SourceSkipped::Missing))
        );
        assert_eq!(
            coverage.misnamed,
            vec![(CatalogueSource::Own, String::from("Not A Pack.toml"))]
        );
    }
}
