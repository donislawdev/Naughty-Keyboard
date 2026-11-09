//! Choosing a pack: reading it from the catalogue, checking it and holding it
//! for the sequence, with where it came from (`D173`). Apart from the loop in
//! `advance_sequence.rs`, which stands at the edge of the ceiling of `D113`.

use super::{AdvanceSequence, Loaded};
use crate::load_pack;
use crate::ports::{PackCatalogue, PackFormat, PackOrigin, SourceError};
use nkb_core::sequence::Event;

/// Why a pack could not be chosen.
///
/// The ones about a file carry where the file was found, so the sentence can
/// name the file to look at - a pack in a folder is not the built-in pack of
/// the same name, and `nkb lint` reads a path, not a name (`OBS-235`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChooseError {
    /// No pack of that name.
    NotFound,
    /// Present but unreadable, or not valid UTF-8.
    Unreadable { origin: PackOrigin },
    /// Larger than the limit a pack file may have (`PACK_FILE_LIMIT`), so it
    /// was not read - told apart from `Unreadable` because the repair differs
    /// (`OBS-235` pkt 2).
    TooLarge { origin: PackOrigin },
    /// The pack has errors, so it loads not at all - `pack-format.md` 11.
    Refused { errors: usize, origin: PackOrigin },
}

impl AdvanceSequence {
    /// Where the chosen pack came from and what it overrides, if a pack is
    /// chosen (`D173`).
    #[must_use]
    pub fn pack_origin(&self) -> Option<&PackOrigin> {
        self.loaded.as_ref().map(|loaded| &loaded.origin)
    }

    /// Reads, validates and chooses a pack, moving the sequence to `ready`.
    ///
    /// Called at startup and again whenever the tester picks another pack.
    /// `check` runs first and the pack is refused on any error -
    /// `pack-format.md` 11 loads all of a pack or none of it - which is what
    /// `load_pack::load` does. A refused pack leaves everything as it was: the
    /// pack in use, its place in it and the last value all stay.
    ///
    /// ⚠️ Called only between actions, never while a value is going in. The
    /// machine would refuse `PackChosen` in `inserting`, but the loaded pack is
    /// swapped before the machine is asked. Unreachable today, because a send
    /// is one blocking call inside [`Self::on_action`] and the sequence is never
    /// left `inserting` between calls. Delivery one character at a time (`W2`)
    /// has to ask the machine first.
    ///
    /// The pack comes from the source in use for its identifier
    /// (`pack-format.md` 12): a pack in a folder that cannot be read or does
    /// not load is refused, and the built-in pack it overrides is NOT taken in
    /// its place - the tester would type other values than the ones they chose
    /// (`D173`).
    ///
    /// # Errors
    ///
    /// [`ChooseError`] when the pack is missing, unreadable, or has errors.
    pub fn choose_pack(
        &mut self,
        catalogue: &dyn PackCatalogue,
        format: &dyn PackFormat,
        pack_id: &str,
    ) -> Result<(), ChooseError> {
        let found = catalogue.find(pack_id).ok_or(ChooseError::NotFound)?;
        let origin = found.origin;
        let text = match found.text {
            Ok(text) => text,
            Err(SourceError::NotFound) => return Err(ChooseError::NotFound),
            Err(SourceError::TooLarge) => return Err(ChooseError::TooLarge { origin }),
            Err(SourceError::Unreadable | SourceError::NotUtf8) => {
                return Err(ChooseError::Unreadable { origin });
            }
        };
        let loaded = match load_pack::load(format, pack_id, &text) {
            Ok(loaded) => loaded,
            Err(refused) => {
                return Err(ChooseError::Refused {
                    errors: refused.errors,
                    origin,
                });
            }
        };
        let total = loaded.pack.values.len();
        // A new pack makes the old index point at somebody else's value.
        self.last = None;
        self.loaded = Some(Loaded {
            pack: loaded.pack,
            warnings: loaded.warnings,
            origin,
        });
        self.sequence = self.sequence.apply(Event::PackChosen { total }).sequence;
        Ok(())
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
    use crate::ports::CatalogueSource;
    use crate::test_support::*;
    use nkb_core::hotkeys::HotkeyAction;
    use nkb_core::pack::Risk;
    use nkb_core::report::PackFolder;

    fn team_over_built_in() -> PackOrigin {
        PackOrigin {
            source: CatalogueSource::Team,
            overrides: vec![CatalogueSource::BuiltIn],
        }
    }

    #[test]
    fn a_pack_from_a_folder_keeps_where_it_came_from_and_the_block_names_the_folder() {
        // `D173`: a team's pack holds other values than the built-in pack of
        // the same name, so the block a developer reads names the folder.
        let mut advance = AdvanceSequence::new(Box::new(StoppedClock::default()));
        let pack = a_pack(vec![a_value("one", "alpha", None)], Risk::Normal);
        let source = FolderSource {
            origin: team_over_built_in(),
            text: Ok("scripted".to_owned()),
        };
        advance
            .choose_pack(&source, &FakeFormat::of(pack), "sample")
            .expect("a scripted pack with no errors loads");
        assert_eq!(advance.pack_origin(), Some(&team_over_built_in()));

        let _ = advance.on_action(HotkeyAction::NextValue, &Kit::ready().ports());
        let kit = Kit::ready();
        let _ = advance.on_action(HotkeyAction::CopyReport, &kit.ports());
        let blocks = kit.text.blocks.borrow();
        assert_eq!(blocks[0].folder, Some(PackFolder::Team));
    }

    #[test]
    fn a_built_in_pack_leaves_the_block_without_a_folder() {
        let mut advance = chosen(Risk::Normal);
        assert_eq!(advance.pack_origin(), Some(&PackOrigin::built_in()));
        let _ = advance.on_action(HotkeyAction::NextValue, &Kit::ready().ports());
        let kit = Kit::ready();
        let _ = advance.on_action(HotkeyAction::CopyReport, &kit.ports());
        assert_eq!(kit.text.blocks.borrow()[0].folder, None);
    }

    #[test]
    fn a_folder_pack_that_cannot_be_used_is_refused_under_its_origin_and_nothing_changes() {
        // The built-in pack it overrides is NOT taken instead, and the pack in
        // use stays.
        let mut advance = chosen(Risk::Normal);
        let _ = advance.on_action(HotkeyAction::NextValue, &Kit::ready().ports());
        let refused = advance.choose_pack(
            &FolderSource {
                origin: team_over_built_in(),
                text: Ok("scripted".to_owned()),
            },
            &FakeFormat::with_errors(2),
            "sample",
        );
        assert_eq!(
            refused,
            Err(ChooseError::Refused {
                errors: 2,
                origin: team_over_built_in()
            })
        );
        for (error, expected) in [
            (
                SourceError::TooLarge,
                ChooseError::TooLarge {
                    origin: team_over_built_in(),
                },
            ),
            (
                SourceError::NotUtf8,
                ChooseError::Unreadable {
                    origin: team_over_built_in(),
                },
            ),
            (
                SourceError::Unreadable,
                ChooseError::Unreadable {
                    origin: team_over_built_in(),
                },
            ),
        ] {
            let source = FolderSource {
                origin: team_over_built_in(),
                text: Err(error),
            };
            assert_eq!(
                advance.choose_pack(&source, &FakeFormat::with_errors(0), "sample"),
                Err(expected)
            );
        }
        assert_eq!(advance.counter(), Some((1, 3)), "the pack in use stays");
        assert_eq!(advance.pack_origin(), Some(&PackOrigin::built_in()));
    }
}
