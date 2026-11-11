//! The values made for a limit, as the palette's worker meets them (`D178`):
//! the set chosen in the length limit window, what a step to another pack does
//! from it, and what the two threads say of it.
//!
//! # A set is not a pack of the catalogue
//!
//! The sequence holds it where it holds a pack, and nothing here remembers it:
//! the settings keep the pack opened last from the catalogue, so the next run
//! starts there, and a value of the set never becomes a recent one. Its
//! identifier is `boundary`, which a pack in a folder may be called too - so
//! every question "is this the pack in use" asks the sequence whether its
//! values were made, never the name.
//!
//! Out of `live.rs`, which stands at the ceiling of `D113`.

use nkb_adapters::boundary_text::EnglishBoundary;
use nkb_adapters::i18n::InPlay;
use nkb_core::boundary::Limit;

use super::*;

/// What the palette walks, for the sentences that give the command printing
/// the same thing: the limit of a made set, or the pack by its identifier.
pub(super) fn in_play<'a>(sequence: &AdvanceSequence, pack: &'a str) -> InPlay<'a> {
    sequence
        .made_for()
        .map_or(InPlay::Pack(pack), InPlay::Limit)
}

/// What the value window reads when it opens: the pack in use and the value
/// the next press sends - or, in place of the pack, the limit the values in
/// use were made for.
pub(super) fn publish(in_use: &InUse, sequence: &AdvanceSequence, bindings: &Bindings) {
    if let Ok(mut held) = in_use.lock() {
        held.made = sequence.made_for();
        held.pack = match held.made {
            Some(_) => None,
            None => sequence.pack_id().map(str::to_owned),
        };
        held.next = next_id(sequence.upcoming().as_ref());
        held.restart = bindings.named(HotkeyAction::RestartPack);
    }
}

impl Worker<'_> {
    /// Makes the values of `limit` the ones the palette walks, from `start` -
    /// or from the first. The pack the settings remember stays the one opened
    /// last from the catalogue, and the band of the value sent last is cleared,
    /// as it is when another pack opens: that value was another set's.
    pub(super) fn use_limit(&mut self, limit: Limit, start: Option<&str>) -> Carried {
        self.sequence.choose_limit(limit, &EnglishBoundary);
        if let Some(start) = start {
            // The window named a value of the very list made here, so it is
            // there. Were it not, the walk starts at the first value, which
            // the palette shows.
            let _ = self.sequence.choose_next(start);
        }
        Carried {
            view: Some(self.view(Vec::new(), ValueBand::Clear, None)),
            told: None,
            toggled: None,
        }
    }

    /// The pack one step along the value window's list, opened as a choice
    /// there opens it - from its first value, and remembered (`D120`). With no
    /// other pack that opens, the palette says so rather than doing nothing.
    ///
    /// From a made set the step leaves it for the catalogue: forward to the
    /// first pack and back to the last, where the list puts them - the set's
    /// row stands above the packs. It is entered through its window alone,
    /// which is where its limit is typed.
    pub(super) fn step_pack(&mut self, direction: Direction) -> Carried {
        let listing = list_packs(&self.memory.folders.catalogue(), &TomlPackFormat).ok();
        // No pack is called nothing, so a made set is beside every one of them.
        let from = match self.sequence.made_for() {
            Some(_) => "",
            None => self.pack.as_str(),
        };
        let other = listing
            .as_ref()
            .and_then(|listing| listing.neighbour(from, direction))
            .map(str::to_owned);
        let view = match other {
            Some(pack) => choose(
                &mut self.sequence,
                self.memory,
                &mut self.pack,
                &pack,
                self.standing,
                self.hold.held_back(),
            ),
            None => Some(self.view(
                vec![i18n::label(PaletteLabel::NoOtherPack).to_owned()],
                ValueBand::Keep,
                None,
            )),
        };
        Carried {
            view,
            told: None,
            toggled: None,
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
    use crate::live::tests::{Registrar, a_store, with_worker};
    use nkb_core::boundary::{self, Unit};

    fn at_most(most: u32, unit: Unit) -> Limit {
        Limit::new(unit, None, Some(most)).expect("a limit")
    }

    fn drawn(carried: Carried) -> View {
        carried.view.expect("the palette is drawn")
    }

    /// `D178`: the set is in use at once and the palette says so in the place
    /// of the pack's name, nothing is written to the settings, and the value
    /// window is told a limit is in use and no pack.
    #[test]
    fn a_limit_is_in_use_at_once_and_nothing_is_remembered() {
        let store = a_store(false);
        with_worker(&store, &Registrar::default(), false, |worker, memory| {
            let limit = at_most(50, Unit::Bytes);
            let view = drawn(worker.carry_out(Command::UseLimit { limit, start: None }));
            assert_eq!(view.pack, "Length limit: at most 50 bytes");
            assert_eq!(
                view.counter,
                i18n::counter(0, boundary::values(limit).len())
            );
            assert!(matches!(view.value, ValueBand::Clear));
            assert_eq!(worker.sequence.made_for(), Some(limit));
            assert!(
                store.saved.borrow().is_empty(),
                "a made set was written to the settings: {:?}",
                store.saved.borrow()
            );

            let slot = in_use();
            publish(&slot, &worker.sequence, &memory.bindings.get());
            let now = in_use_now(&slot);
            assert_eq!(now.made, Some(limit));
            assert_eq!(now.pack, None, "a made set was published as a pack");
            assert_eq!(now.next.as_deref(), Some("empty"));
        });
    }

    #[test]
    fn the_walk_starts_at_the_value_the_window_named() {
        let store = a_store(false);
        with_worker(&store, &Registrar::default(), false, |worker, _| {
            let limit = at_most(50, Unit::Graphemes);
            let third = boundary::values(limit)[2].id();
            worker.carry_out(Command::UseLimit {
                limit,
                start: Some(third.clone()),
            });
            assert_eq!(
                next_id(worker.sequence.upcoming().as_ref()),
                Some(third),
                "the walk did not start where the window said"
            );
            // A value the set does not hold leaves the walk at its start.
            worker.carry_out(Command::UseLimit {
                limit,
                start: Some("no-such-value".to_owned()),
            });
            assert_eq!(
                next_id(worker.sequence.upcoming().as_ref()).as_deref(),
                Some("empty")
            );
        });
    }

    /// A pack of the catalogue chosen after a set opens, whatever it is
    /// called, and is the one remembered - and a published slot names it.
    #[test]
    fn a_pack_chosen_after_a_set_opens_and_is_remembered() {
        let store = a_store(false);
        with_worker(&store, &Registrar::default(), false, |worker, memory| {
            let before = worker.pack.clone();
            worker.carry_out(Command::UseLimit {
                limit: at_most(8, Unit::Graphemes),
                start: None,
            });
            // The pack that was in use before the set: the names are equal and
            // it still has to open, because the set took its place.
            let view = drawn(worker.carry_out(Command::Choose(before.clone())));
            assert_eq!(worker.sequence.made_for(), None);
            assert_eq!(worker.sequence.pack_id(), Some(before.as_str()));
            assert_ne!(view.pack, "Length limit: at most 8 graphemes");
            let slot = in_use();
            publish(&slot, &worker.sequence, &memory.bindings.get());
            assert_eq!(in_use_now(&slot).pack, Some(before));
            assert_eq!(in_use_now(&slot).made, None);
        });
    }

    /// The step to another pack leaves a set for the ends of the list, as the
    /// set's row stands above the packs.
    #[test]
    fn a_step_from_a_set_goes_to_the_first_pack_and_back_to_the_last() {
        let store = a_store(false);
        with_worker(&store, &Registrar::default(), false, |worker, memory| {
            let listing = list_packs(&memory.folders.catalogue(), &TomlPackFormat)
                .expect("the built-in catalogue");
            let first = listing
                .neighbour("", Direction::Next)
                .expect("a first pack")
                .to_owned();
            let last = listing
                .neighbour("", Direction::Previous)
                .expect("a last pack")
                .to_owned();
            assert_ne!(first, last);
            for (direction, wanted) in [(Direction::Next, &first), (Direction::Previous, &last)] {
                worker.carry_out(Command::UseLimit {
                    limit: at_most(8, Unit::Graphemes),
                    start: None,
                });
                worker.carry_out(Command::Pack(direction));
                assert_eq!(worker.sequence.made_for(), None);
                assert_eq!(worker.sequence.pack_id(), Some(wanted.as_str()));
            }
        });
    }

    /// A pack in a folder may be called `boundary` too. Chosen while a set is
    /// in use it opens: the names are equal, and the set is no pack. Chosen
    /// again, it is the pack in use and nothing moves.
    #[test]
    fn a_pack_called_as_the_set_opens_in_its_place() {
        let own = std::env::temp_dir()
            .join("nkb-live-made-tests")
            .join(format!("own-{}", std::process::id()));
        std::fs::create_dir_all(&own).expect("a folder");
        let text = include_str!("../../../../packs/whitespace.toml").replacen(
            "\"whitespace\"",
            "\"boundary\"",
            1,
        );
        std::fs::write(own.join("boundary.toml"), text).expect("a pack file");
        let store = a_store(false);
        let (kept, _) = KeptSettings::open(&store);
        let memory = Memory {
            kept: RefCell::new(kept),
            store: &store,
            file: String::new(),
            bindings: Cell::new(default_bindings()),
            folders: Folders::new(PackFolders::new(
                Err(nkb_app::SourceSkipped::NotSet),
                Ok(own.clone()),
            )),
        };
        let mut sequence = AdvanceSequence::new(Box::new(SystemClock));
        let mut pack = String::new();
        sequence.choose_limit(at_most(8, Unit::Graphemes), &EnglishBoundary);
        assert_eq!(sequence.pack_id(), Some("boundary"));
        let first = open_pack(&mut sequence, &memory, &mut pack, "boundary");
        let again = open_pack(&mut sequence, &memory, &mut pack, "boundary");
        let _ = std::fs::remove_dir_all(&own);
        assert!(
            matches!(first, PackOpening::New(_)),
            "the pack did not open in place of the set"
        );
        assert_eq!(sequence.made_for(), None);
        assert_eq!(pack, "boundary");
        assert!(matches!(again, PackOpening::Same));
    }

    #[test]
    fn a_sentence_about_a_made_set_names_its_command_and_not_a_pack() {
        let store = a_store(false);
        with_worker(&store, &Registrar::default(), false, |worker, _| {
            assert_eq!(
                in_play(&worker.sequence, &worker.pack),
                InPlay::Pack(worker.pack.as_str())
            );
            let limit = at_most(50, Unit::Bytes);
            worker.carry_out(Command::UseLimit { limit, start: None });
            let failed = Message::CopyFailed {
                detail: "no clipboard".to_owned(),
            };
            let said = i18n::message(
                &failed,
                in_play(&worker.sequence, &worker.pack),
                &default_bindings(),
            );
            assert_eq!(
                said,
                "The value was not copied: no clipboard. The same value is printed by: nkb boundary --max-length 50 --unit bytes"
            );
        });
    }
}
