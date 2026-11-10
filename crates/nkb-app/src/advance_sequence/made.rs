//! A set of values made for a limit, held where a pack is held (`D178`).
//!
//! A specification says "at most 50 characters", and the tester wants the
//! values that test that sentence under the same shortcut that walks a pack.
//! So the set is built in memory as a [`Pack`] and handed to the sequence the
//! way a chosen pack is: the walk, the value shown before it goes, the Copy
//! buttons, clipboard mode and the report block are the code they already
//! were, and none of them learns a second kind of thing to hold.
//!
//! # What tells a made set from a pack
//!
//! A flag beside the pack held, never its identifier: a pack in a folder may
//! be called `boundary` too. The flag carries what a file cannot say - the
//! limit, and for each value whether its length is inside it - and the report
//! block takes the limit from it, so a ticket gives the command that makes the
//! same values rather than a pack version nobody can look up.
//!
//! # Nothing is built here
//!
//! Every value stays the recipe [`nkb_core::boundary`] gave it. The largest
//! limit describes values of more than a megabyte, and they become text when
//! one is typed and not before (`architektura.md` 6.1).

use nkb_core::boundary::{self, BoundaryValue, Limit};
use nkb_core::pack::{Pack, PackValue, Risk};
use nkb_core::sequence::Event;

use super::{AdvanceSequence, Loaded};
use crate::ports::{BoundaryText, PackOrigin};

/// The identifier a made set has where a pack has its own: the first half of
/// `boundary/two-byte-51`, the name of the command that makes the same values.
pub const MADE_SET_ID: &str = "boundary";

/// What a set was made for, beside the pack that holds its values.
#[derive(Debug)]
pub(super) struct Made {
    pub(super) limit: Limit,
    /// Whether each value's length is inside the limit, in the order of the
    /// values.
    within: Vec<bool>,
}

impl Loaded {
    /// Whether value `index`, counted from one, is inside the limit the set
    /// was made for. `None` for a pack of the catalogue.
    pub(super) fn within(&self, index: usize) -> Option<bool> {
        let made = self.made.as_ref()?;
        made.within.get(index.checked_sub(1)?).copied()
    }
}

impl AdvanceSequence {
    /// The limit the values in use were made for, when the tool made them.
    #[must_use]
    pub fn made_for(&self) -> Option<Limit> {
        self.loaded.as_ref()?.made.as_ref().map(|made| made.limit)
    }

    /// Makes the values that answer `limit` and chooses them, moving the
    /// sequence to `ready` as a chosen pack does.
    ///
    /// Cannot fail: a [`Limit`] is checked where it is made, and the values
    /// come from the core, not from a file. Like a chosen pack it starts at
    /// the first value and forgets the value sent last, which was another
    /// set's.
    ///
    /// ⚠️ Called only between actions, as [`Self::choose_pack`] is.
    pub fn choose_limit(&mut self, limit: Limit, text: &dyn BoundaryText) {
        let values = boundary::values(limit);
        let within = values.iter().map(BoundaryValue::within).collect();
        let pack = Pack {
            id: MADE_SET_ID.to_owned(),
            name: text.set_name(limit),
            description: String::new(),
            // The version of the tool that made the values: what "the same
            // values" depends on, where a pack has a version of its own.
            version: env!("CARGO_PKG_VERSION").to_owned(),
            updated: String::new(),
            license: String::new(),
            authors: Vec::new(),
            language: "en".to_owned(),
            risk: Risk::Normal,
            tags: Vec::new(),
            fields: Vec::new(),
            source: None,
            values: values
                .iter()
                .map(|value| PackValue {
                    id: value.id(),
                    name: text.value_name(value),
                    definition: value.body().into(),
                    breaks: Some(text.breaks(value)),
                    expect: Some(text.expected(limit, value)),
                    risk: None,
                    fields: Vec::new(),
                    tags: Vec::new(),
                    source: None,
                    since: None,
                    shape: None,
                    deprecated: false,
                    replaced_by: None,
                })
                .collect(),
            pairs: Vec::new(),
        };
        let total = pack.values.len();
        self.last = None;
        self.loaded = Some(Loaded {
            pack,
            warnings: 0,
            // The tool carries it, as it carries a built-in pack, and it
            // overrides nothing.
            origin: PackOrigin::built_in(),
            made: Some(Made { limit, within }),
        });
        self.sequence = self.sequence.apply(Event::PackChosen { total }).sequence;
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
    use crate::advance_sequence::UpcomingValue;
    use crate::test_support::*;
    use nkb_core::boundary::Unit;
    use nkb_core::hotkeys::HotkeyAction;

    /// Words that say which question was asked, so a test can tell the name
    /// from the two sentences.
    struct Words;

    impl BoundaryText for Words {
        fn set_name(&self, limit: Limit) -> String {
            format!("set for {}", limit.unit().id())
        }
        fn value_name(&self, value: &BoundaryValue) -> String {
            format!("name of {}", value.id())
        }
        fn breaks(&self, value: &BoundaryValue) -> String {
            format!("breaks of {}", value.id())
        }
        fn expected(&self, _limit: Limit, value: &BoundaryValue) -> String {
            format!("expected of {}", value.id())
        }
    }

    fn at_most_fifty_bytes() -> Limit {
        Limit::new(Unit::Bytes, None, Some(50)).expect("in range")
    }

    fn made() -> AdvanceSequence {
        let mut advance = AdvanceSequence::new(Box::new(StoppedClock::default()));
        advance.choose_limit(at_most_fifty_bytes(), &Words);
        advance
    }

    #[test]
    fn a_limit_is_chosen_as_a_pack_is_and_stands_at_its_first_value() {
        let advance = made();
        let values = boundary::values(at_most_fifty_bytes());
        assert_eq!(advance.counter(), Some((0, values.len())));
        assert_eq!(advance.pack_id(), Some(MADE_SET_ID));
        assert_eq!(advance.pack_name(), Some("set for bytes"));
        assert_eq!(advance.made_for(), Some(at_most_fifty_bytes()));
        let Some(UpcomingValue::Value {
            index,
            name,
            reference,
            within,
            ..
        }) = advance.upcoming()
        else {
            panic!("the first value is upcoming");
        };
        assert_eq!(index, 1);
        assert_eq!(name, "name of empty");
        assert_eq!(reference, "boundary/empty");
        assert_eq!(within, Some(true));
    }

    #[test]
    fn every_press_types_the_text_the_core_makes_and_says_which_side_of_the_limit_it_is() {
        // What `nkb boundary` prints and what the palette types are one text:
        // both come from the body of the value.
        let mut advance = made();
        let kit = Kit::ready();
        for value in boundary::values(at_most_fifty_bytes()) {
            let Some(UpcomingValue::Value { within, .. }) = advance.upcoming() else {
                panic!("{} is upcoming", value.id());
            };
            assert_eq!(
                within,
                Some(value.within()),
                "{} before it goes",
                value.id()
            );
            let outcome = advance.on_action(HotkeyAction::NextValue, &kit.ports());
            let sent = outcome.sent.expect("a ready route takes every value");
            assert_eq!(sent.within, Some(value.within()), "{} sent", value.id());
            assert_eq!(sent.key.value, value.id());
            assert_eq!(
                kit.direct.handed.borrow().last().map(String::as_str),
                Some(value.materialise().as_str()),
                "{}",
                value.id()
            );
        }
        assert!(matches!(
            advance.upcoming(),
            Some(UpcomingValue::EndOfPack { .. })
        ));
    }

    #[test]
    fn the_block_of_a_made_value_carries_the_limit_and_the_words_it_was_given() {
        let mut advance = made();
        let kit = Kit::ready();
        let _ = advance.on_action(HotkeyAction::NextValue, &kit.ports());
        let _ = advance.on_action(HotkeyAction::CopyReport, &kit.ports());
        let blocks = kit.text.blocks.borrow();
        let block = blocks.last().expect("a block of the value sent");
        assert_eq!(block.made_for, Some(at_most_fifty_bytes()));
        assert_eq!(block.folder, None);
        assert_eq!(block.reference, "boundary/empty");
        assert_eq!(block.breaks.as_deref(), Some("breaks of empty"));
        assert_eq!(block.expected.as_deref(), Some("expected of empty"));
    }

    #[test]
    fn a_pack_chosen_after_a_made_set_is_a_pack_again() {
        let mut advance = made();
        let _ = advance.on_action(HotkeyAction::NextValue, &Kit::ready().ports());
        let pack = a_pack(
            vec![a_value("one", "alpha", None)],
            nkb_core::pack::Risk::Normal,
        );
        advance
            .choose_pack(&FakeSource, &FakeFormat::of(pack), "sample")
            .expect("a scripted pack with no errors loads");
        assert_eq!(advance.made_for(), None);
        let Some(UpcomingValue::Value { within, .. }) = advance.upcoming() else {
            panic!("the pack's first value is upcoming");
        };
        assert_eq!(within, None);
        let kit = Kit::ready();
        let outcome = advance.on_action(HotkeyAction::NextValue, &kit.ports());
        assert_eq!(outcome.sent.expect("sent").within, None);
        let _ = advance.on_action(HotkeyAction::CopyReport, &kit.ports());
        assert_eq!(kit.text.blocks.borrow()[0].made_for, None);
    }

    #[test]
    fn choosing_a_limit_forgets_the_value_sent_last_as_choosing_a_pack_does() {
        let mut advance = chosen(nkb_core::pack::Risk::Normal);
        let _ = advance.on_action(HotkeyAction::NextValue, &Kit::ready().ports());
        advance.choose_limit(at_most_fifty_bytes(), &Words);
        let kit = Kit::ready();
        let _ = advance.on_action(HotkeyAction::CopyReport, &kit.ports());
        assert!(
            kit.text.blocks.borrow().is_empty(),
            "no block of a value the set does not hold"
        );
    }
}
