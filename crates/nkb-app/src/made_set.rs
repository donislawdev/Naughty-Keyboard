//! The values made for a limit, held as a pack (`D178`).
//!
//! A specification says "at most 50 characters", and the tester wants the
//! values that test that sentence the way a pack is tested: walked under the
//! palette's shortcut, or one of them typed by `nkb send`. So the set is built
//! in memory as a [`Pack`], and both take it from here - the palette and the
//! command cannot type two texts for one value or call it two names.
//!
//! # Nothing is built here
//!
//! Every value stays the recipe [`nkb_core::boundary`] gave it. The largest
//! limit describes values of more than a megabyte, and they become text when
//! one is typed and not before (`architektura.md` 6.1).

use nkb_core::boundary::{self, BoundaryValue, Limit};
use nkb_core::pack::{Pack, PackValue, Risk};

use crate::ports::BoundaryText;

/// The identifier a made set has where a pack has its own: the first half of
/// `boundary/two-byte-51`, the name of the command that makes the same values.
pub const MADE_SET_ID: &str = "boundary";

/// The values that answer one limit, in the order `nkb boundary` prints them,
/// with the words that go with each.
///
/// Made only by [`MadeSet::new`], so the pack inside is always the one the
/// core gives for the limit it carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MadeSet {
    limit: Limit,
    pack: Pack,
    /// Whether each value's length is inside the limit, in the order of the
    /// values.
    within: Vec<bool>,
}

impl MadeSet {
    /// Makes the values that answer `limit`, named and described by `text`.
    ///
    /// Cannot fail: a [`Limit`] is checked where it is made, and the values
    /// come from the core, not from a file.
    #[must_use]
    pub fn new(limit: Limit, text: &dyn BoundaryText) -> Self {
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
        Self {
            limit,
            pack,
            within,
        }
    }

    /// The values as a pack holds them: identifier, names, the two sentences
    /// of each, and its body.
    #[must_use]
    pub const fn pack(&self) -> &Pack {
        &self.pack
    }

    /// The limit, the pack and whether each value is inside the limit, for a
    /// holder that keeps them apart.
    pub(crate) fn into_parts(self) -> (Limit, Pack, Vec<bool>) {
        (self.limit, self.pack, self.within)
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
    use crate::test_support::FakeBoundaryText;
    use nkb_core::boundary::Unit;

    #[test]
    fn every_value_of_the_core_is_held_in_its_order_with_its_words_and_its_side_of_the_limit() {
        let limit = Limit::new(Unit::Utf16, Some(3), Some(5)).expect("in range");
        let set = MadeSet::new(limit, &FakeBoundaryText);
        let values = boundary::values(limit);
        let pack = set.pack();
        assert_eq!(pack.id, MADE_SET_ID);
        assert_eq!(pack.name, "set for utf16");
        assert_eq!(pack.values.len(), values.len());
        for (held, value) in pack.values.iter().zip(&values) {
            assert_eq!(held.id, value.id());
            assert_eq!(held.name, format!("name of {}", value.id()));
            assert_eq!(held.breaks, Some(format!("breaks of {}", value.id())));
            assert_eq!(held.expect, Some(format!("expected of {}", value.id())));
            assert_eq!(held.definition, value.body().into());
        }
        let (kept, _, within) = set.into_parts();
        assert_eq!(kept, limit);
        assert_eq!(
            within,
            values.iter().map(BoundaryValue::within).collect::<Vec<_>>()
        );
        assert!(within.contains(&true) && within.contains(&false));
    }
}
