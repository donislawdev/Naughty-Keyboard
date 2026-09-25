//! The catalogue this build can actually offer, and the honest account of it.
//!
//! # One source out of three, and it says so
//!
//! `pack-format.md` 12 defines three sources - built in, a team folder named in
//! settings, and the user's own folder - loaded in that order, with a later one
//! overriding an earlier one by `pack.id`. The settings file has no key for
//! either folder yet (`OBS-78`), so two of the three cannot be located at all.
//!
//! 🔴 The temptation is to print the one list and move on, and untouchable rule
//! 1 exists for exactly that temptation: a run that did less than it promised
//! must say so, in its output and in every artefact it leaves. A pack the tool
//! never looked for is indistinguishable, in a listing, from a pack that is not
//! there - and the person reading that listing is about to conclude their team's
//! pack was not installed properly.
//!
//! So the coverage travels beside the list, the same way `nkb lint` carries the
//! rules it did not check beside the problems it found.

use nkb_app::{
    CatalogueCoverage, CatalogueSource, PackCatalogue, PackSource, SourceError, SourceSkipped,
};

use crate::built_in::BUILT_IN_PACKS;

/// The packs carried inside the executable.
#[derive(Debug, Clone, Copy, Default)]
pub struct BuiltInCatalogue;

impl BuiltInCatalogue {
    #[must_use]
    pub fn new() -> Self {
        Self
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
    fn list(&self) -> Result<Vec<String>, SourceError> {
        Ok(BUILT_IN_PACKS
            .iter()
            .map(|(id, _)| (*id).to_owned())
            .collect())
    }

    fn coverage(&self) -> CatalogueCoverage {
        CatalogueCoverage {
            consulted: vec![CatalogueSource::BuiltIn],
            // Not "none configured" and not an empty list: this build cannot
            // name those folders because it has nowhere to read a setting from.
            // The distinction matters to whoever is wondering why their team's
            // pack is absent.
            skipped: vec![
                (CatalogueSource::Team, SourceSkipped::NotImplementedYet),
                (CatalogueSource::Own, SourceSkipped::NotImplementedYet),
            ],
        }
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

    #[test]
    fn every_listed_pack_can_also_be_read() {
        // The supertrait's promise, checked rather than assumed: a listing that
        // names something unreadable sends a caller to a dead end.
        let catalogue = BuiltInCatalogue::new();
        let ids = catalogue.list().expect("the carried list cannot fail");
        assert!(
            !ids.is_empty(),
            "an empty catalogue would pass every other assertion here"
        );

        for id in ids {
            let text = catalogue
                .read(&id)
                .unwrap_or_else(|e| panic!("{id} was listed and cannot be read: {e}"));
            assert!(!text.is_empty(), "{id} was listed and is empty");
        }
    }

    #[test]
    fn a_pack_that_is_not_carried_is_not_found() {
        assert_eq!(
            BuiltInCatalogue::new().read("no-such-pack"),
            Err(SourceError::NotFound)
        );
    }

    #[test]
    fn the_two_sources_this_build_cannot_reach_are_named_with_a_reason() {
        // 🔴 The assertion untouchable rule 1 turns into code. If a later build
        // starts reading team packs, this fails and forces the account to be
        // updated with it - which is the point, because the alternative is a
        // report that keeps claiming a limit that no longer exists.
        let coverage = BuiltInCatalogue::new().coverage();
        assert_eq!(coverage.consulted, vec![CatalogueSource::BuiltIn]);
        assert_eq!(
            coverage.skipped,
            vec![
                (CatalogueSource::Team, SourceSkipped::NotImplementedYet),
                (CatalogueSource::Own, SourceSkipped::NotImplementedYet),
            ]
        );
    }

    #[test]
    fn all_three_sources_the_format_defines_are_accounted_for() {
        // Not the same assertion as the one above. That one checks the two
        // values. This one checks that nothing was FORGOTTEN - a fourth source
        // added to the format with no line here would slip through unmentioned.
        let coverage = BuiltInCatalogue::new().coverage();
        let mut seen: Vec<CatalogueSource> = coverage.consulted.clone();
        seen.extend(coverage.skipped.iter().map(|(source, _)| *source));
        for source in [
            CatalogueSource::BuiltIn,
            CatalogueSource::Team,
            CatalogueSource::Own,
        ] {
            assert!(
                seen.contains(&source),
                "the source {:?} is neither consulted nor listed as skipped, so a reader \
                 of this report would never learn it exists",
                source
            );
        }
    }
}
