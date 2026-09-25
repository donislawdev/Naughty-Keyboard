//! Choosing a pack: the list, the query and the selected row, with no window.
//!
//! # Why this is not in the view and not in `app`
//!
//! Not in the view: which packs match, which row is selected and what Enter
//! does are decisions, and a view holds layout and bindings only (GUI rules 11
//! and 15). Here they are tested without a window.
//!
//! Not in `app`: the command line lists packs (`nkb packs`) but never searches
//! them, so a use case here would have one caller, and `architektura.md` 8 asks
//! for a second concrete variant before an extension point is built. If `nkb`
//! ever grows a search, the filter moves down a layer with its tests.
//!
//! # What the list is made of
//!
//! Every entry `list_packs` returns, in the catalogue's order - including a
//! pack that is present and refused, and one whose file would not open. Those
//! two stay in the list, faded and not choosable: a pack that vanished from a
//! list cannot be asked about (untouchable rule 1). A pack counts as offensive
//! when the pack says so OR any one of its values does, because a tester
//! choosing a pack is about to send every value in it.
//!
//! # The query
//!
//! Words, all of which must appear, case not minded, in the pack's name,
//! description, tags or id. No folding of diacritics: the catalogue's prose is
//! English (`D23`, untouchable rule 8), and a wider match is easier to add than
//! a narrower one is to take back (document 15 section 1, start narrower).

use nkb_adapters::i18n::{self, PacksLabel};
use nkb_app::browse_packs::{Listing, PackEntry};
use nkb_app::ports::SourceError;
use nkb_core::pack::{Pack, Risk};

use crate::query::{KeyPress, Pressed, Query};

/// One pack as the window offers it.
#[derive(Debug, Clone)]
struct Choice {
    id: String,
    title: String,
    detail: String,
    badge: Option<Badge>,
    enabled: bool,
    /// Lower-cased name, description, tags and id - what a query is matched in.
    haystack: String,
}

/// The pill at the end of a row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Badge {
    pub text: String,
    /// A fact, not a colour: which colour a risk wears is the dictionary's
    /// business (document 13 section 2.1).
    pub risky: bool,
}

/// A row as the window draws it: finished strings and facts, no toolkit type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub title: String,
    pub detail: String,
    pub badge: Option<Badge>,
    /// The pack the palette holds now.
    pub current: bool,
    pub enabled: bool,
}

/// What Enter (or a click) comes to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Chosen {
    /// A different pack: the palette opens it.
    Pack(String),
    /// The pack already in use: the window closes and the palette keeps its
    /// place - choosing what is open must not send `7 / 34` back to the start.
    InUse,
    /// No row can be chosen - nothing matches, or nothing loads. The window
    /// stays open, because closing it would look like a choice was made.
    Nothing,
}

/// Why the list is empty, when it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Empty {
    /// The catalogue itself could not be read.
    Unavailable,
    /// The catalogue holds no pack.
    NoPacks,
}

/// The list, the query and the selected row.
#[derive(Debug, Clone)]
pub struct PackPicker {
    choices: Vec<Choice>,
    in_use: Option<String>,
    query: Query,
    /// Indexes into `choices` of the rows the query lets through, in order.
    visible: Vec<usize>,
    /// Position in `visible` of the selected row. Always an enabled one.
    selected: Option<usize>,
    empty: Option<Empty>,
    /// One line per reason a catalogue source was not read.
    notes: Vec<String>,
}

impl PackPicker {
    /// The picker over what `list_packs` found, starting on the pack in use.
    #[must_use]
    pub fn new(listing: Result<Listing, SourceError>, in_use: Option<&str>) -> Self {
        let (choices, empty, notes) = match listing {
            Ok(listing) => {
                let choices: Vec<Choice> = listing.entries.iter().map(choice_of).collect();
                let empty = choices.is_empty().then_some(Empty::NoPacks);
                (choices, empty, i18n::not_read(&listing.coverage))
            }
            Err(_) => (Vec::new(), Some(Empty::Unavailable), Vec::new()),
        };
        let mut picker = Self {
            choices,
            in_use: in_use.map(str::to_owned),
            query: Query::default(),
            visible: Vec::new(),
            selected: None,
            empty,
            notes,
        };
        picker.filter();
        picker.selected = picker
            .position_of_in_use()
            .or_else(|| picker.first_enabled());
        picker
    }

    /// Applies one key press to the query and filters the list again when the
    /// query changed. The answer says whether the press was the query's.
    pub fn press(&mut self, press: KeyPress<'_>) -> Pressed {
        let pressed = self.query.press(press);
        if pressed == Pressed::Changed {
            let kept = self.selected_choice();
            self.filter();
            self.selected = kept
                .and_then(|choice| self.visible.iter().position(|&at| at == choice))
                .or_else(|| self.first_enabled());
        }
        pressed
    }

    /// Moves the selection down, past rows that cannot be chosen. Stays on the
    /// last one rather than wrapping: a list that jumps to its top at the end
    /// loses a tester who pressed once too often.
    pub fn next(&mut self) {
        let from = self.selected.map_or(0, |at| at + 1);
        if let Some(found) = (from..self.visible.len()).find(|&at| self.enabled_at(at)) {
            self.selected = Some(found);
        }
    }

    /// Moves the selection up, the same way.
    pub fn previous(&mut self) {
        let Some(current) = self.selected else {
            self.selected = self.first_enabled();
            return;
        };
        if let Some(found) = (0..current).rev().find(|&at| self.enabled_at(at)) {
            self.selected = Some(found);
        }
    }

    /// What Enter comes to.
    #[must_use]
    pub fn chosen(&self) -> Chosen {
        match self.selected_choice() {
            None => Chosen::Nothing,
            Some(index) => {
                let id = &self.choices[index].id;
                if self.in_use.as_deref() == Some(id.as_str()) {
                    Chosen::InUse
                } else {
                    Chosen::Pack(id.clone())
                }
            }
        }
    }

    /// A click on row `row` of the list as drawn: selects it and says what it
    /// comes to. A click on a row that cannot be chosen chooses nothing and
    /// moves nothing.
    pub fn click(&mut self, row: usize) -> Chosen {
        if row < self.visible.len() && self.enabled_at(row) {
            self.selected = Some(row);
            self.chosen()
        } else {
            Chosen::Nothing
        }
    }

    /// The rows the query lets through.
    #[must_use]
    pub fn rows(&self) -> Vec<Row> {
        self.visible
            .iter()
            .map(|&index| {
                let choice = &self.choices[index];
                Row {
                    title: choice.title.clone(),
                    detail: choice.detail.clone(),
                    badge: choice.badge.clone(),
                    current: self.in_use.as_deref() == Some(choice.id.as_str()),
                    enabled: choice.enabled,
                }
            })
            .collect()
    }

    /// The selected row's position in [`Self::rows`].
    #[must_use]
    pub const fn selected(&self) -> Option<usize> {
        self.selected
    }

    /// The query as typed, for the line to draw.
    #[must_use]
    pub fn query(&self) -> &str {
        self.query.as_str()
    }

    /// How many packs the list shows, and of how many.
    #[must_use]
    pub fn summary(&self) -> String {
        i18n::packs_summary(self.visible.len(), self.choices.len())
    }

    /// What the list says when it has no rows. Empty while it has some.
    #[must_use]
    pub fn empty_text(&self) -> String {
        match self.empty {
            Some(Empty::Unavailable) => i18n::packs_label(PacksLabel::ListUnavailable).to_owned(),
            Some(Empty::NoPacks) => i18n::packs_label(PacksLabel::NoPacks).to_owned(),
            None if self.visible.is_empty() => i18n::no_match(self.query.as_str()),
            None => String::new(),
        }
    }

    /// Which catalogue sources the list did not read, one line per reason.
    #[must_use]
    pub fn notes(&self) -> &[String] {
        &self.notes
    }

    fn filter(&mut self) {
        let lowered = self.query.as_str().to_lowercase();
        let words: Vec<&str> = lowered.split_whitespace().collect();
        self.visible = self
            .choices
            .iter()
            .enumerate()
            .filter(|(_, choice)| words.iter().all(|word| choice.haystack.contains(word)))
            .map(|(index, _)| index)
            .collect();
    }

    fn enabled_at(&self, position: usize) -> bool {
        self.choices[self.visible[position]].enabled
    }

    fn first_enabled(&self) -> Option<usize> {
        (0..self.visible.len()).find(|&at| self.enabled_at(at))
    }

    fn position_of_in_use(&self) -> Option<usize> {
        let in_use = self.in_use.as_deref()?;
        (0..self.visible.len())
            .find(|&at| self.enabled_at(at) && self.choices[self.visible[at]].id == in_use)
    }

    fn selected_choice(&self) -> Option<usize> {
        self.selected.map(|at| self.visible[at])
    }
}

fn choice_of(entry: &PackEntry) -> Choice {
    match entry {
        PackEntry::Loaded { pack, .. } => Choice {
            id: pack.id.clone(),
            title: pack.name.clone(),
            detail: i18n::pack_detail(&pack.id, pack.values.len()),
            badge: is_offensive(pack).then(|| Badge {
                text: i18n::packs_label(PacksLabel::Offensive).to_owned(),
                risky: true,
            }),
            enabled: true,
            haystack: haystack_of(pack),
        },
        PackEntry::Refused { id, errors } => Choice {
            id: id.clone(),
            title: id.clone(),
            detail: i18n::pack_refused(id),
            badge: Some(Badge {
                text: i18n::pack_problems(*errors),
                risky: true,
            }),
            enabled: false,
            haystack: id.to_lowercase(),
        },
        PackEntry::Unreadable { id, .. } => Choice {
            id: id.clone(),
            title: id.clone(),
            detail: i18n::pack_unreadable(id),
            badge: None,
            enabled: false,
            haystack: id.to_lowercase(),
        },
    }
}

/// A pack is offensive when it says so, or when any one of its values does.
fn is_offensive(pack: &Pack) -> bool {
    pack.risk == Risk::Offensive
        || pack
            .values
            .iter()
            .any(|value| pack.risk_of(value) == Risk::Offensive)
}

/// Name, description, tags and id, lower-cased once, one per line - a word
/// never spans two fields, because a word holds no line break.
fn haystack_of(pack: &Pack) -> String {
    let mut fields = vec![
        pack.name.as_str(),
        pack.description.as_str(),
        pack.id.as_str(),
    ];
    fields.extend(pack.tags.iter().map(String::as_str));
    fields.join("\n").to_lowercase()
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;
    use nkb_adapters::{BuiltInCatalogue, TomlPackFormat};
    use nkb_app::browse_packs::list_packs;
    use nkb_app::ports::{CatalogueCoverage, CatalogueSource};

    fn key(text: &str) -> KeyPress<'_> {
        KeyPress {
            text,
            control: false,
            alt: false,
            meta: false,
        }
    }

    fn type_in(picker: &mut PackPicker, text: &str) {
        for character in text.chars() {
            let typed = character.to_string();
            picker.press(key(&typed));
        }
    }

    fn shipped(in_use: Option<&str>) -> PackPicker {
        PackPicker::new(
            list_packs(&BuiltInCatalogue::new(), &TomlPackFormat),
            in_use,
        )
    }

    /// Three loaded packs and two that are not, in catalogue order.
    fn mixed() -> PackPicker {
        let listing = list_packs(&BuiltInCatalogue::new(), &TomlPackFormat)
            .expect("the built-in catalogue must list");
        let mut entries: Vec<PackEntry> = listing
            .entries
            .into_iter()
            .filter(|entry| ["whitespace", "unicode-text", "numbers-extreme"].contains(&entry.id()))
            .collect();
        entries.insert(
            1,
            PackEntry::Refused {
                id: String::from("broken"),
                errors: 3,
            },
        );
        entries.push(PackEntry::Unreadable {
            id: String::from("gone"),
            reason: SourceError::Unreadable,
        });
        PackPicker::new(
            Ok(Listing {
                entries,
                coverage: CatalogueCoverage {
                    consulted: vec![CatalogueSource::BuiltIn],
                    skipped: Vec::new(),
                },
            }),
            Some("whitespace"),
        )
    }

    fn titles(picker: &PackPicker) -> Vec<String> {
        picker.rows().into_iter().map(|row| row.title).collect()
    }

    #[test]
    fn it_opens_on_the_pack_in_use_with_every_pack_listed() {
        let picker = shipped(Some("unicode-text"));
        let rows = picker.rows();
        assert_eq!(rows.len(), 9, "the shipped catalogue has nine packs");
        let selected = picker.selected().expect("a pack is selected");
        assert!(
            rows[selected].current,
            "the selected row is the pack in use"
        );
        assert_eq!(picker.chosen(), Chosen::InUse);
        assert_eq!(picker.summary(), "packs: 9");
        assert_eq!(picker.empty_text(), "");
    }

    #[test]
    fn a_pack_not_in_the_list_starts_on_the_first_that_can_be_chosen() {
        let picker = shipped(Some("no-such-pack"));
        assert_eq!(picker.selected(), Some(0));
    }

    #[test]
    fn words_must_all_match_in_any_field_and_case_is_not_minded() {
        let mut picker = shipped(Some("whitespace"));
        type_in(&mut picker, "UNICODE");
        assert!(titles(&picker).iter().any(|t| t.contains("Unicode")));
        let narrowed = picker.rows().len();
        type_in(&mut picker, " zzz");
        assert!(picker.rows().is_empty(), "every word must match");
        assert_eq!(picker.summary(), "packs: 0 of 9");
        assert_eq!(
            picker.empty_text(),
            "No pack matches \"UNICODE zzz\". Press Backspace to widen the search."
        );
        assert_eq!(picker.chosen(), Chosen::Nothing);
        for _ in 0..4 {
            picker.press(key("\u{8}"));
        }
        assert_eq!(picker.rows().len(), narrowed, "Backspace widens again");
    }

    #[test]
    fn the_id_and_the_tags_are_searched_too() {
        let mut picker = shipped(None);
        type_in(&mut picker, "locale-pl");
        assert_eq!(picker.rows().len(), 1);
        assert_eq!(picker.chosen(), Chosen::Pack(String::from("locale-pl")));
    }

    #[test]
    fn the_selection_stays_on_its_pack_while_the_query_still_shows_it() {
        let mut picker = shipped(Some("unicode-text"));
        type_in(&mut picker, "u");
        assert_eq!(picker.chosen(), Chosen::InUse, "unicode-text still matches");
        type_in(&mut picker, "nicode");
        assert_eq!(picker.chosen(), Chosen::InUse);
    }

    #[test]
    fn refused_and_unreadable_packs_stay_listed_and_are_skipped() {
        let mut picker = mixed();
        let listed = titles(&picker);
        assert_eq!(listed.len(), 5, "nothing vanished: {listed:?}");
        assert_eq!((listed[1].as_str(), listed[4].as_str()), ("broken", "gone"));
        let rows = picker.rows();
        assert!(!rows[1].enabled && !rows[4].enabled);
        assert_eq!(rows[1].detail, "broken, does not load");
        assert_eq!(
            rows[1].badge,
            Some(Badge {
                text: String::from("problems: 3"),
                risky: true
            })
        );
        assert_eq!(rows[4].detail, "gone, cannot be read");

        assert_eq!(picker.selected(), Some(0));
        picker.next();
        assert_eq!(picker.selected(), Some(2), "the refused row is skipped");
        picker.next();
        picker.next();
        assert_eq!(
            picker.selected(),
            Some(3),
            "the unreadable last row is skipped and the selection stays"
        );
        picker.previous();
        picker.previous();
        assert_eq!(picker.selected(), Some(0), "skipped on the way up too");
        picker.previous();
        assert_eq!(picker.selected(), Some(0), "no wrap at the top");
    }

    #[test]
    fn a_click_on_a_row_that_cannot_be_chosen_chooses_nothing() {
        let mut picker = mixed();
        assert_eq!(picker.click(1), Chosen::Nothing);
        assert_eq!(picker.selected(), Some(0), "and moves nothing");
        assert_eq!(picker.click(99), Chosen::Nothing);
        assert_eq!(picker.click(2), Chosen::Pack(String::from("unicode-text")));
        assert_eq!(picker.click(0), Chosen::InUse);
    }

    #[test]
    fn a_pack_is_offensive_when_any_of_its_values_is() {
        let picker = shipped(None);
        let listing = list_packs(&BuiltInCatalogue::new(), &TomlPackFormat)
            .expect("the built-in catalogue must list");
        for (entry, row) in listing.entries.iter().zip(picker.rows()) {
            let PackEntry::Loaded { pack, .. } = entry else {
                continue;
            };
            let any = pack.risk == Risk::Offensive
                || pack
                    .values
                    .iter()
                    .any(|v| pack.risk_of(v) == Risk::Offensive);
            assert_eq!(row.badge.is_some(), any, "{}", pack.id);
        }
    }

    #[test]
    fn a_catalogue_that_cannot_be_read_says_so_and_offers_nothing() {
        let picker = PackPicker::new(Err(SourceError::Unreadable), Some("whitespace"));
        assert!(picker.rows().is_empty());
        assert_eq!(picker.selected(), None);
        assert_eq!(picker.chosen(), Chosen::Nothing);
        assert!(
            picker
                .empty_text()
                .starts_with("The pack list could not be read")
        );
    }

    #[test]
    fn the_shipped_catalogue_says_which_sources_it_did_not_read() {
        let picker = shipped(None);
        assert_eq!(
            picker.notes(),
            ["Not read: team folder, own folder - cannot be set in this version."]
        );
    }
}
