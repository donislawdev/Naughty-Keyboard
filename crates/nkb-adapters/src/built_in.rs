//! The catalogue delivered with the tool, carried inside the executable.
//!
//! # Why the bytes are in the binary and not in a folder beside it
//!
//! Three reasons out of the documents, none of them taste.
//!
//! `product-spec.md` 11 records that the portable, no-install build is
//! sometimes **the only form that gets through a corporation at all**. An
//! executable that needs a `packs/` folder next to it is not portable in that
//! sense: it is an executable plus an invitation to lose half of itself.
//!
//! `pack-format.md` 11 says built-in packs are validated **when a release is
//! built, not on every run**. That is exactly what embedding means - the check
//! happens once, where it can block a release, rather than on a user's machine
//! where its only options are to slow the start or to be ignored.
//!
//! And it removes a whole class of failure a portable executable has no way out
//! of: a folder moved, renamed, or left behind by a copy. The tool cannot lose
//! packs it is made of.
//!
//! # Why the list is written out and not discovered
//!
//! `include_str!` needs a literal path, so the list has to exist somewhere. That
//! constraint is turned into the same shape the interface uses for its
//! components: ONE file that answers "what is here", with a guard that fails if
//! it disagrees with the folder in either direction. A pack added to `packs/`
//! and not listed here would ship as a file nobody can load, and a line here
//! naming a file that is gone would not compile.

/// Every pack the tool carries, as `(identifier, file contents)`.
///
/// The identifier is the file name without its extension, which `E010` requires
/// the pack to declare as its own `id` - so the mapping needs no index.
///
/// 🔴 Order is the order packs are offered in. `catalog-v0.1.md` 0 gives the
/// catalogue a deliberate release order - whitespace first, because it hits the
/// most common and cheapest-to-fix class of bug - and alphabetical sorting would
/// silently discard that. Add a pack where it belongs, not at the end.
pub const BUILT_IN_PACKS: &[(&str, &str)] = &[
    ("whitespace", include_str!("../../../packs/whitespace.toml")),
    (
        "unicode-text",
        include_str!("../../../packs/unicode-text.toml"),
    ),
    (
        "length-bombs",
        include_str!("../../../packs/length-bombs.toml"),
    ),
    (
        "magic-values",
        include_str!("../../../packs/magic-values.toml"),
    ),
    (
        "numbers-extreme",
        include_str!("../../../packs/numbers-extreme.toml"),
    ),
    (
        "dates-impossible",
        include_str!("../../../packs/dates-impossible.toml"),
    ),
    (
        "export-breakers",
        include_str!("../../../packs/export-breakers.toml"),
    ),
    ("locale-pl", include_str!("../../../packs/locale-pl.toml")),
    // `injections` comes here in the catalogue's order, when it ships.
    (
        "filenames-paths",
        include_str!("../../../packs/filenames-paths.toml"),
    ),
];

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::path::{Path, PathBuf};

    fn packs_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packs")
    }

    fn files_on_disk() -> BTreeSet<String> {
        std::fs::read_dir(packs_dir())
            .expect("the packs folder must be readable")
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "toml"))
            .filter_map(|path| path.file_stem()?.to_str().map(ToOwned::to_owned))
            .collect()
    }

    #[test]
    fn the_list_holds_exactly_what_the_packs_folder_holds() {
        let on_disk = files_on_disk();
        assert!(
            !on_disk.is_empty(),
            "no .toml in {} - this test would otherwise pass by comparing two empty sets",
            packs_dir().display()
        );

        let listed: BTreeSet<String> = BUILT_IN_PACKS
            .iter()
            .map(|(id, _)| (*id).to_owned())
            .collect();

        let missing: Vec<_> = on_disk.difference(&listed).collect();
        assert!(
            missing.is_empty(),
            "these packs are in packs/ and are NOT carried by the binary: {missing:?}. \
             A pack that ships as a file nobody can load is worse than one that does not \
             ship, because the folder says otherwise."
        );

        let extra: Vec<_> = listed.difference(&on_disk).collect();
        assert!(
            extra.is_empty(),
            "these packs are listed here and are not in packs/: {extra:?}"
        );
    }

    #[test]
    fn every_carried_pack_declares_the_identifier_it_is_carried_under() {
        // E010 in the small: the mapping from name to identifier is what lets
        // `unicode-text/zero-width-space-x3` name a file and a line with no
        // index anywhere. If it drifted, everything downstream would look up
        // the wrong pack while reporting success.
        for (id, text) in BUILT_IN_PACKS {
            let pack = crate::read_pack::parse(text)
                .unwrap_or_else(|| panic!("the carried pack {id} must parse"));
            assert_eq!(
                &pack.id, id,
                "carried under {id} and declares {} inside",
                pack.id
            );
        }
    }

    #[test]
    fn the_carried_bytes_are_the_bytes_on_disk() {
        // include_str! happens at build time. If someone edits a pack and the
        // build is not rerun, the binary and the repository disagree - and the
        // one that ships is the stale one.
        for (id, text) in BUILT_IN_PACKS {
            let path = packs_dir().join(format!("{id}.toml"));
            let on_disk = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("{} must be readable: {e}", path.display()));
            assert_eq!(
                on_disk.replace("\r\n", "\n"),
                text.replace("\r\n", "\n"),
                "the carried copy of {id} differs from packs/{id}.toml"
            );
        }
    }
}
