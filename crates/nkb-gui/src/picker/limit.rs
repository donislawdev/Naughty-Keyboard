//! The row of the value window that opens the length limit window (`D178`):
//! when it stands in the list, what it says, and the number it hands on. Out
//! of `picker.rs`, which stands at the ceiling of `D113`.
//!
//! # A row that is no pack
//!
//! It stands first, above the packs: the values behind it are made for a limit
//! the tester types, not read from a file, so it has no place among the packs
//! and nothing to fold. With nothing typed it is always there, first. With a
//! query it is found by a number: "50" in the search is most likely the limit
//! from a specification, so the row then offers itself with that number as the
//! most, first again, and the window opens with it typed. It is found by its
//! own words too, like any row - and then it stands LAST, below what else the
//! words find: a tester typing the first letters of a pack's name must not
//! have Enter open this window instead.

use nkb_adapters::boundary_text::set_name;
use nkb_adapters::i18n::{self, LimitLabel};
use nkb_core::boundary::{Limit, Unit};

use super::{Haystack, Row, RowKind};

/// Where the row stands in the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Place {
    First,
    Last,
}

/// The row, and the limit the palette walks now, if it walks one.
#[derive(Debug, Clone)]
pub(super) struct LimitRow {
    /// The limit the values in use were made for.
    made: Option<Limit>,
    haystack: Haystack,
}

impl LimitRow {
    pub(super) fn new(made: Option<Limit>) -> Self {
        Self {
            made,
            haystack: Haystack::new(
                &[
                    i18n::limit_label(LimitLabel::Heading),
                    i18n::limit_label(LimitLabel::RowDetail),
                ],
                [],
            ),
        }
    }

    /// Whether the palette walks the values of a limit now.
    pub(super) const fn in_use(&self) -> bool {
        self.made.is_some()
    }

    /// The limit a query that is one number suggests: that number as the most,
    /// counted the way a person counts. `None` for any other query, and for a
    /// number that is no limit - zero, or more than the largest.
    pub(super) fn suggested(query: &str) -> Option<Limit> {
        let number = query.trim();
        if !number.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        Limit::new(Unit::Graphemes, None, Some(number.parse().ok()?)).ok()
    }

    /// Where the row stands in the list a query leaves, if it stands there -
    /// the module's header says which place and why.
    pub(super) fn place(&self, query: &str, words: &[String]) -> Option<Place> {
        if words.is_empty() || Self::suggested(query).is_some() {
            Some(Place::First)
        } else if self.haystack.matches(words) {
            Some(Place::Last)
        } else {
            None
        }
    }

    /// The row as drawn: the name of the limit it would open with - suggested
    /// by the query, or in use - or its own name, and what it is for.
    pub(super) fn row(&self, query: &str, tree: bool) -> Row {
        let suggested = Self::suggested(query);
        Row {
            kind: RowKind::Limit,
            title: suggested.or(self.made).map_or_else(
                || i18n::limit_label(LimitLabel::Heading).to_owned(),
                set_name,
            ),
            detail: i18n::limit_label(LimitLabel::RowDetail).to_owned(),
            // In use, unless the row offers another limit than the one in use.
            current: self.in_use() && suggested.is_none(),
            enabled: true,
            tree,
            ..Row::default()
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
    use nkb_app::ports::SourceError;

    use super::*;
    use crate::picker::tests::{TODAY, listing, of_kind, selected_row, shipped, type_in};
    use crate::picker::{Chosen, Folding, PackPicker};

    /// `D178`: the row of the length limit stands first with nothing typed,
    /// is chosen like any row, and asks for its window with nothing typed in.
    #[test]
    fn the_row_of_the_length_limit_stands_first_and_opens_its_window() {
        let mut picker = shipped(Some("whitespace"));
        let row = picker.rows()[0].clone();
        assert_eq!(row.kind, RowKind::Limit);
        assert_eq!(row.title, "Length limit");
        assert_eq!(row.detail, "Values that test how much text a field takes");
        assert!(row.enabled && row.tree && !row.current);
        assert_eq!(row.fold, None, "it has nothing to fold");
        assert_eq!(
            selected_row(&picker).title,
            "Whitespace",
            "the window still opens on the pack in use"
        );
        assert_eq!(picker.click(0), Chosen::Limit { most: None });
        assert_eq!(picker.toggle(0), Folding::Nothing);
        assert_eq!(picker.fold(), Folding::Nothing);
        assert_eq!(picker.unfold(), Folding::Nothing);
    }

    /// A number in the search is most likely the limit from a specification:
    /// the row offers itself with that number, first in the list.
    #[test]
    fn a_number_in_the_search_suggests_the_limit_with_that_number() {
        let mut picker = shipped(Some("whitespace"));
        type_in(&mut picker, "50");
        let rows = picker.rows();
        assert_eq!(rows[0].kind, RowKind::Limit);
        assert_eq!(rows[0].title, "Length limit: at most 50 graphemes");
        assert!(!rows[0].tree, "a search is a flat list");
        assert_eq!(picker.selected(), Some(0), "Enter opens the window");
        assert_eq!(picker.enter(), Chosen::Limit { most: Some(50) });

        // Also when the row selected before the next digit is still listed:
        // "30" is in the name of a value, and the number is a limit all the
        // same.
        let mut on_a_value = shipped(Some("whitespace"));
        type_in(&mut on_a_value, "3");
        for _ in 0..on_a_value.rows().len() {
            if selected_row(&on_a_value).title == "30 February" {
                break;
            }
            on_a_value.next();
        }
        assert_eq!(selected_row(&on_a_value).title, "30 February");
        type_in(&mut on_a_value, "0");
        assert!(
            on_a_value
                .rows()
                .iter()
                .any(|row| row.title == "30 February"),
            "the value the selection stood on is gone, so this proves nothing"
        );
        assert_eq!(on_a_value.chosen(), Chosen::Limit { most: Some(30) });

        // Not a limit: zero, more than the largest, and anything but digits.
        for query in ["0", "100001", "50 a", "5.0", "-5", "+5", "\u{0665}"] {
            let mut picker = shipped(Some("whitespace"));
            type_in(&mut picker, query);
            assert!(
                picker.rows().iter().all(|row| row.kind != RowKind::Limit),
                "{query:?} suggested a limit"
            );
        }
        let mut largest = shipped(None);
        type_in(&mut largest, " 100000 ");
        assert_eq!(
            largest.enter(),
            Chosen::Limit {
                most: Some(100_000)
            }
        );
    }

    #[test]
    fn the_row_is_found_by_its_own_words_below_what_else_they_find() {
        let mut picker = shipped(Some("whitespace"));
        type_in(&mut picker, "LIMIT length");
        let rows = picker.rows();
        let last = rows.len() - 1;
        assert_eq!(rows[last].kind, RowKind::Limit);
        assert_eq!(rows[last].title, "Length limit", "words suggest no number");
        assert_eq!(picker.click(last), Chosen::Limit { most: None });

        // Words find packs and values first: a tester typing the start of a
        // name must not have Enter open this window instead.
        let mut shared = shipped(Some("whitespace"));
        type_in(&mut shared, "te");
        let rows = shared.rows();
        assert!(rows.len() > 1, "nothing else holds these letters");
        assert_eq!(rows.last().map(|row| row.kind), Some(RowKind::Limit));
        assert_ne!(selected_row(&shared).kind, RowKind::Limit);

        let mut other = shipped(Some("whitespace"));
        type_in(&mut other, "pesel");
        assert!(other.rows().iter().all(|row| row.kind != RowKind::Limit));
    }

    /// While the palette walks the values of a limit no pack is in use: the
    /// window opens on the row of the limit, which names it and says so.
    #[test]
    fn a_limit_in_use_is_where_the_window_opens_and_no_pack_is_in_use() {
        let limit =
            Limit::new(nkb_core::boundary::Unit::Bytes, Some(8), Some(50)).expect("a limit");
        let mut picker =
            PackPicker::new(Ok(listing()), TODAY, None, Some("empty")).with_limit(Some(limit));
        assert_eq!(picker.selected(), Some(0));
        let row = selected_row(&picker);
        assert_eq!(row.title, "Length limit: 8 to 50 bytes");
        assert!(row.current, "the row of the limit in use does not say so");
        assert!(
            of_kind(&picker, RowKind::Pack)
                .iter()
                .all(|row| !row.current),
            "a pack is marked in use beside a limit"
        );
        assert_eq!(
            picker.enter(),
            Chosen::Limit { most: None },
            "the window opens on the limit in use, which it reads itself"
        );
        // Another number in the search offers that one, and is not in use.
        type_in(&mut picker, "64");
        let row = picker.rows()[0].clone();
        assert_eq!(row.title, "Length limit: at most 64 graphemes");
        assert!(!row.current);
    }

    /// A catalogue that could not be read says so in place of its rows, and
    /// one row above the sentence would hide it.
    #[test]
    fn the_row_does_not_stand_over_a_catalogue_that_could_not_be_read() {
        let picker = PackPicker::new(Err(SourceError::Unreadable), TODAY, None, None);
        assert!(picker.rows().is_empty());
        assert_eq!(picker.chosen(), Chosen::Nothing);
    }
}
