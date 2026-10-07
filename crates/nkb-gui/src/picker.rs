//! Finding a value or a pack: the list, the query and the selected row, with no
//! window.
//!
//! # Why this is not in the view and not in `app`
//!
//! Not in the view: which rows match, which one is selected and what Enter
//! does are decisions, and a view holds layout and bindings only (GUI rules 11
//! and 15). Here they are tested without a window.
//!
//! Not in `app`: the command line lists packs (`nkb packs`) but never searches
//! them, so a use case here would have one caller, and `architektura.md` 8 asks
//! for a second concrete variant before an extension point is built. If `nkb`
//! ever grows a search, the filter moves down a layer with its tests.
//!
//! # What the list is made of (UX2, `UX-GUI-001` and `UX-GUI-002`)
//!
//! Three kinds of row: a section heading, a value and a pack. With nothing
//! typed, the values of the pack in use come first, in the pack's order and
//! with the next one selected, then every pack - so the next value is chosen
//! and the pack changed without typing a letter. With a query, the values that
//! match come first, from every pack, then the packs that match.
//!
//! A fourth row opens the section of the pack in use: `Restart pack`, under
//! the action's own name and with its shortcut at the end (`UX-GUI-007`).
//! Until it, the way back to the start of a pack was a shortcut the hint bar
//! did not name, or choosing value 1 here - which does the same, and which
//! nothing said does the same. So the row does exactly that: it makes the
//! first value the next one, as the shortcut does (`Sequence::restart` and
//! `set_next(1)` reach one position). One line with a key combination, the
//! shape of a row in the shortcuts window, because the first render drew it
//! as a name over a detail line - the twin of a value called "Restart pack".
//! Not with a query: it belongs to the pack in use, not to what a search found.
//!
//! Every entry `list_packs` returns is a pack row, including a pack that is
//! present and refused, and one whose file would not open. Those two stay in
//! the list, faded and not choosable: a pack that vanished from a list cannot
//! be asked about (untouchable rule 1). A heading is never chosen either. A
//! pack counts as offensive when the pack says so OR any one of its values
//! does, because a tester choosing a pack is about to send every value in it.
//!
//! # The query
//!
//! Words, all of which must appear, case not minded. A pack matches in its
//! name, description, tags or id. A value matches in its OWN name, id, tags and
//! fields, and not in the name of its pack: "unicode-text" finds the pack, not
//! every one of its values (document 15 section 1, start narrower). No folding of
//! diacritics: the catalogue's prose is English (`D23`, untouchable rule 8), and
//! a wider match is easier to add than a narrower one is to take back.
//!
//! A row found by a part it does not show says which (`UX-GUI-008`): a value's
//! id, tags or fields, a pack's tags - "Whitespace - value 5 of 12 - in its id".
//! Without it a row found there looked found at random: the audit's example
//! was the pack Whitespace, listed for "unicode" by a tag. A word a shown part
//! holds is explained by the row and named nowhere. The pack's description
//! counts as shown, though the view elides a long one, so a word only in its
//! cut-off end is not named - the place goes before the description, and a
//! place for a part the row prints is a second answer to one question.
//!
//! # Opening without a known next value
//!
//! The window opens on the next value when the palette published one, and on
//! the pack in use when it did not (the end of a pack, a value in flight). Not
//! on the first value: Enter pressed out of habit would then move the tester
//! back to the start of the pack, where Enter on the pack in use only closes.

use nkb_adapters::i18n::{self, MatchPlace, PacksLabel};
use nkb_app::browse_packs::{Listing, PackEntry};
use nkb_app::ports::SourceError;
use nkb_core::hotkeys::HotkeyAction;
use nkb_core::pack::{Pack, PackValue, Risk};

use crate::query::{KeyPress, Pressed, Query};

/// One pack as the window offers it.
#[derive(Debug, Clone)]
struct PackChoice {
    id: String,
    title: String,
    detail: PackDetail,
    badge: Option<Badge>,
    enabled: bool,
    /// Name, description and id shown, tags not.
    haystack: Haystack,
    /// Empty for a pack that does not load.
    values: Vec<ValueChoice>,
}

/// The second line of a pack row, as far as it is known before a query.
#[derive(Debug, Clone)]
enum PackDetail {
    /// A pack that loads. The line is built when drawn, because a query adds
    /// the parts of the pack it was found in that the row does not show.
    Loaded { description: String },
    /// A pack that does not load: the line is the same whatever is typed.
    Fixed(String),
}

/// One value as the window offers it.
#[derive(Debug, Clone)]
struct ValueChoice {
    id: String,
    name: String,
    offensive: bool,
    /// Its own name shown, its id, tags and fields not.
    haystack: Haystack,
}

/// What a query is matched in, lower-cased once: the parts a row shows, and
/// the parts it does not, each under its place (`UX-GUI-008`).
///
/// One structure answers both questions - does the row match, and where - so
/// the two cannot drift: a row is listed exactly when every word is in some
/// part, and it names a place exactly for a word no shown part holds.
#[derive(Debug, Clone, Default)]
struct Haystack {
    /// The shown parts, one per line - a word never spans two, because a word
    /// holds no line break.
    shown: String,
    /// The parts the row does not show, in the order of [`MatchPlace`], so the
    /// first that holds a word is the place the row names.
    hidden: Vec<(MatchPlace, String)>,
}

impl Haystack {
    fn new<'a>(shown: &[&str], hidden: impl IntoIterator<Item = (MatchPlace, &'a str)>) -> Self {
        let mut hidden: Vec<(MatchPlace, String)> = hidden
            .into_iter()
            .map(|(place, part)| (place, part.to_lowercase()))
            .collect();
        // Stable, so the parts of one place keep their order.
        hidden.sort_by_key(|(place, _)| *place);
        Self {
            shown: shown.join("\n").to_lowercase(),
            hidden,
        }
    }

    fn holds(&self, word: &str) -> bool {
        self.shown.contains(word) || self.hidden.iter().any(|(_, part)| part.contains(word))
    }

    /// Whether every word is somewhere in it.
    fn matches(&self, words: &[String]) -> bool {
        words.iter().all(|word| self.holds(word))
    }

    /// The places a row names: for every word no shown part holds, the first
    /// hidden part that does. Empty when the row shows every word. One per
    /// such word, in the query's order - the words of `i18n` name each place
    /// once and in one order, so that is done in one place.
    fn places(&self, words: &[String]) -> Vec<MatchPlace> {
        words
            .iter()
            .filter(|word| !self.shown.contains(word.as_str()))
            .filter_map(|word| {
                self.hidden
                    .iter()
                    .find(|(_, part)| part.contains(word.as_str()))
                    .map(|(place, _)| *place)
            })
            .collect()
    }
}

/// The query as words, lower-cased - all of which must match.
fn words_of(query: &str) -> Vec<String> {
    query
        .to_lowercase()
        .split_whitespace()
        .map(str::to_owned)
        .collect()
}

/// What a row of the list is, by index into the packs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Item {
    Heading(Section),
    Pack(usize),
    Value {
        pack: usize,
        value: usize,
    },
    /// Start this pack - the one in use - from its first value.
    Restart(usize),
}

/// Which section a heading opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    /// The values of the pack in use, with nothing typed.
    InUse(usize),
    /// The values a query found.
    Found,
    /// The packs.
    Packs,
}

/// The pill at the end of a row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Badge {
    pub text: String,
    /// A fact, not a colour: which colour a risk wears is the dictionary's
    /// business (document 13 section 2.1).
    pub risky: bool,
    /// It names where the tester stands - the value the next press sends.
    pub current: bool,
}

/// What a row is, for the window to draw it the right way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowKind {
    Heading,
    Pack,
    Value,
    /// The row that starts the pack in use again.
    Restart,
}

/// A row as the window draws it: finished strings and facts, no toolkit type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub kind: RowKind,
    pub title: String,
    pub detail: String,
    pub badge: Option<Badge>,
    /// The pack the palette holds now.
    pub current: bool,
    pub enabled: bool,
    /// The key combination at the end of the row - the restart row's own
    /// shortcut. `None` for every other row.
    pub key: Option<String>,
}

/// What Enter (or a click) comes to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Chosen {
    /// A different pack: the palette opens it, from its first value.
    Pack(String),
    /// The pack already in use: the window closes and the palette keeps its
    /// place - choosing what is open must not send `7 / 34` back to the start.
    InUse,
    /// A value: the palette opens its pack when that is another one, and the
    /// value becomes the one the next press sends. By identifiers, because the
    /// palette sends from the pack it holds and looks the value up there.
    Value { pack: String, value: String },
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
    packs: Vec<PackChoice>,
    in_use: Option<String>,
    /// The identifier of the value the next press sends, in the pack in use.
    next: Option<String>,
    query: Query,
    /// The rows the query lets through, in order.
    visible: Vec<Item>,
    /// Position in `visible` of the selected row. Always a choosable one.
    selected: Option<usize>,
    empty: Option<Empty>,
    /// One line per reason a catalogue source was not read.
    notes: Vec<String>,
    /// The restart shortcut as the hint bar words it, for the restart row.
    restart_key: Option<String>,
}

impl PackPicker {
    /// The picker over what `list_packs` found, opening on the next value of
    /// the pack in use - or on that pack, when no next value is known.
    #[must_use]
    pub fn new(
        listing: Result<Listing, SourceError>,
        in_use: Option<&str>,
        next: Option<&str>,
    ) -> Self {
        let (packs, empty, notes) = match listing {
            Ok(listing) => {
                let packs: Vec<PackChoice> = listing.entries.iter().map(choice_of).collect();
                let empty = packs.is_empty().then_some(Empty::NoPacks);
                (packs, empty, i18n::not_read(&listing.coverage))
            }
            Err(_) => (Vec::new(), Some(Empty::Unavailable), Vec::new()),
        };
        let mut picker = Self {
            packs,
            in_use: in_use.map(str::to_owned),
            next: next.map(str::to_owned),
            query: Query::default(),
            visible: Vec::new(),
            selected: None,
            empty,
            notes,
            restart_key: None,
        };
        picker.filter();
        picker.selected = picker
            .position_of_next()
            .or_else(|| picker.position_of_in_use())
            .or_else(|| picker.first_enabled());
        picker
    }

    /// The same picker, its restart row ending in `key` - the restart
    /// shortcut of the table in effect, worded as the hint bar words it.
    #[must_use]
    pub fn with_restart_key(mut self, key: Option<String>) -> Self {
        self.restart_key = key;
        self
    }

    /// Applies one key press to the query and filters the list again when the
    /// query changed. The answer says whether the press was the query's.
    pub fn press(&mut self, press: KeyPress<'_>) -> Pressed {
        let pressed = self.query.press(press);
        if pressed == Pressed::Changed {
            let kept = self.selected_item();
            self.filter();
            self.selected = kept
                .and_then(|item| self.visible.iter().position(|&at| at == item))
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
        match self.selected_item() {
            Some(Item::Pack(at)) => {
                let id = &self.packs[at].id;
                if self.in_use.as_deref() == Some(id.as_str()) {
                    Chosen::InUse
                } else {
                    Chosen::Pack(id.clone())
                }
            }
            Some(Item::Value { pack, value }) => Chosen::Value {
                pack: self.packs[pack].id.clone(),
                value: self.packs[pack].values[value].id.clone(),
            },
            // The first value as the next one: where the restart shortcut puts
            // the sequence. The row stands only over a pack with values.
            Some(Item::Restart(pack)) => {
                self.packs[pack]
                    .values
                    .first()
                    .map_or(Chosen::Nothing, |first| Chosen::Value {
                        pack: self.packs[pack].id.clone(),
                        value: first.id.clone(),
                    })
            }
            // A heading is never selected, so this is "nothing selected".
            Some(Item::Heading(_)) | None => Chosen::Nothing,
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
        let words = words_of(self.query.as_str());
        self.visible
            .iter()
            .map(|&item| self.row_of(item, &words))
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

    /// How many values and packs the list shows - and, while a query hides
    /// some, of how many in the whole catalogue.
    #[must_use]
    pub fn summary(&self) -> String {
        let values = self
            .visible
            .iter()
            .filter(|item| matches!(item, Item::Value { .. }))
            .count();
        let packs = self
            .visible
            .iter()
            .filter(|item| matches!(item, Item::Pack(_)))
            .count();
        if self.query.as_str().is_empty() {
            i18n::packs_summary(values, packs)
        } else {
            let all_values = self.packs.iter().map(|pack| pack.values.len()).sum();
            i18n::packs_summary_filtered(values, all_values, packs, self.packs.len())
        }
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
        let words = words_of(self.query.as_str());
        let matches = |haystack: &Haystack| haystack.matches(&words);
        let mut visible = Vec::new();
        if words.is_empty() {
            if let Some(at) = self.in_use_index()
                && !self.packs[at].values.is_empty()
            {
                visible.push(Item::Heading(Section::InUse(at)));
                visible.push(Item::Restart(at));
                visible.extend(
                    (0..self.packs[at].values.len()).map(|value| Item::Value { pack: at, value }),
                );
            }
            if !self.packs.is_empty() {
                visible.push(Item::Heading(Section::Packs));
                visible.extend((0..self.packs.len()).map(Item::Pack));
            }
        } else {
            let values: Vec<Item> = self
                .packs
                .iter()
                .enumerate()
                .flat_map(|(pack, choice)| {
                    choice
                        .values
                        .iter()
                        .enumerate()
                        .filter(|(_, value)| matches(&value.haystack))
                        .map(move |(value, _)| Item::Value { pack, value })
                })
                .collect();
            if !values.is_empty() {
                visible.push(Item::Heading(Section::Found));
                visible.extend(values);
            }
            let packs: Vec<Item> = (0..self.packs.len())
                .filter(|&at| matches(&self.packs[at].haystack))
                .map(Item::Pack)
                .collect();
            if !packs.is_empty() {
                visible.push(Item::Heading(Section::Packs));
                visible.extend(packs);
            }
        }
        self.visible = visible;
    }

    /// Row `item` as drawn, `words` being the query's.
    fn row_of(&self, item: Item, words: &[String]) -> Row {
        match item {
            Item::Heading(section) => Row {
                kind: RowKind::Heading,
                title: match section {
                    Section::InUse(at) => i18n::values_in(&self.packs[at].title),
                    Section::Found => i18n::packs_label(PacksLabel::FoundValues).to_owned(),
                    Section::Packs => i18n::packs_label(PacksLabel::PacksSection).to_owned(),
                },
                detail: String::new(),
                badge: None,
                current: false,
                enabled: false,
                key: None,
            },
            Item::Pack(at) => {
                let choice = &self.packs[at];
                let detail = match &choice.detail {
                    PackDetail::Loaded { description } => i18n::pack_detail(
                        &choice.id,
                        choice.values.len(),
                        description,
                        &choice.haystack.places(words),
                    ),
                    PackDetail::Fixed(detail) => detail.clone(),
                };
                Row {
                    kind: RowKind::Pack,
                    title: choice.title.clone(),
                    detail,
                    badge: choice.badge.clone(),
                    current: self.in_use.as_deref() == Some(choice.id.as_str()),
                    enabled: choice.enabled,
                    key: None,
                }
            }
            Item::Value { pack, value } => {
                let owner = &self.packs[pack];
                let choice = &owner.values[value];
                let is_next = self.in_use.as_deref() == Some(owner.id.as_str())
                    && self.next.as_deref() == Some(choice.id.as_str());
                // One pill: "next" over "offensive" - the palette's next band
                // already marks the risk of the next value.
                let badge = if is_next {
                    Some(Badge {
                        text: i18n::packs_label(PacksLabel::Next).to_owned(),
                        risky: false,
                        current: true,
                    })
                } else if choice.offensive {
                    Some(Badge {
                        text: i18n::packs_label(PacksLabel::Offensive).to_owned(),
                        risky: true,
                        current: false,
                    })
                } else {
                    None
                };
                Row {
                    kind: RowKind::Value,
                    title: choice.name.clone(),
                    detail: i18n::value_detail(
                        &owner.title,
                        value + 1,
                        owner.values.len(),
                        &choice.haystack.places(words),
                    ),
                    badge,
                    current: false,
                    enabled: true,
                    key: None,
                }
            }
            // The action's own name and its shortcut, the words the hint bar
            // gives them, so the row and the bar teach one thing.
            Item::Restart(_) => Row {
                kind: RowKind::Restart,
                title: i18n::action_name(HotkeyAction::RestartPack).to_owned(),
                detail: String::new(),
                badge: None,
                current: false,
                enabled: true,
                key: self.restart_key.clone(),
            },
        }
    }

    fn enabled_at(&self, position: usize) -> bool {
        match self.visible[position] {
            Item::Heading(_) => false,
            Item::Pack(at) => self.packs[at].enabled,
            Item::Value { .. } | Item::Restart(_) => true,
        }
    }

    fn first_enabled(&self) -> Option<usize> {
        (0..self.visible.len()).find(|&at| self.enabled_at(at))
    }

    fn in_use_index(&self) -> Option<usize> {
        let in_use = self.in_use.as_deref()?;
        self.packs
            .iter()
            .position(|pack| pack.enabled && pack.id == in_use)
    }

    fn position_of_in_use(&self) -> Option<usize> {
        let at = self.in_use_index()?;
        self.visible.iter().position(|&item| item == Item::Pack(at))
    }

    fn position_of_next(&self) -> Option<usize> {
        let pack = self.in_use_index()?;
        let next = self.next.as_deref()?;
        let value = self.packs[pack]
            .values
            .iter()
            .position(|choice| choice.id == next)?;
        self.visible
            .iter()
            .position(|&item| item == Item::Value { pack, value })
    }

    fn selected_item(&self) -> Option<Item> {
        self.selected.map(|at| self.visible[at])
    }
}

fn choice_of(entry: &PackEntry) -> PackChoice {
    match entry {
        PackEntry::Loaded { pack, .. } => PackChoice {
            id: pack.id.clone(),
            title: pack.name.clone(),
            detail: PackDetail::Loaded {
                description: pack.description.clone(),
            },
            badge: is_offensive(pack).then(|| Badge {
                text: i18n::packs_label(PacksLabel::Offensive).to_owned(),
                risky: true,
                current: false,
            }),
            enabled: true,
            haystack: haystack_of(pack),
            values: pack
                .values
                .iter()
                .map(|value| ValueChoice {
                    id: value.id.clone(),
                    name: value.name.clone(),
                    offensive: pack.risk_of(value) == Risk::Offensive,
                    haystack: value_haystack_of(value),
                })
                .collect(),
        },
        PackEntry::Refused { id, errors } => PackChoice {
            id: id.clone(),
            title: id.clone(),
            detail: PackDetail::Fixed(i18n::pack_refused(id)),
            badge: Some(Badge {
                text: i18n::pack_problems(*errors),
                risky: true,
                current: false,
            }),
            enabled: false,
            // The id is all the window knows of it, and the row shows it.
            haystack: Haystack::new(&[id], []),
            values: Vec::new(),
        },
        PackEntry::Unreadable { id, .. } => PackChoice {
            id: id.clone(),
            title: id.clone(),
            detail: PackDetail::Fixed(i18n::pack_unreadable(id)),
            badge: None,
            enabled: false,
            haystack: Haystack::new(&[id], []),
            values: Vec::new(),
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

/// Name, description and id, which the pack's row shows, and its tags, which
/// it does not.
fn haystack_of(pack: &Pack) -> Haystack {
    Haystack::new(
        &[
            pack.name.as_str(),
            pack.description.as_str(),
            pack.id.as_str(),
        ],
        pack.tags.iter().map(|tag| (MatchPlace::Tags, tag.as_str())),
    )
}

/// The value's own name, which its row shows, and its id, tags and fields,
/// which it does not.
fn value_haystack_of(value: &PackValue) -> Haystack {
    Haystack::new(
        &[value.name.as_str()],
        std::iter::once((MatchPlace::Id, value.id.as_str()))
            .chain(
                value
                    .tags
                    .iter()
                    .map(|tag| (MatchPlace::Tags, tag.as_str())),
            )
            .chain(
                value
                    .fields
                    .iter()
                    .map(|field| (MatchPlace::Fields, field.as_str())),
            ),
    )
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

    fn listing() -> Listing {
        list_packs(&BuiltInCatalogue::new(), &TomlPackFormat)
            .expect("the built-in catalogue must list")
    }

    fn shipped(in_use: Option<&str>) -> PackPicker {
        PackPicker::new(Ok(listing()), in_use, None)
    }

    /// The id of value `index` (from 1) of a shipped pack.
    fn value_id(pack: &str, index: usize) -> String {
        listing()
            .entries
            .iter()
            .find_map(|entry| match entry {
                PackEntry::Loaded { pack: loaded, .. } if loaded.id == pack => {
                    Some(loaded.values[index - 1].id.clone())
                }
                _ => None,
            })
            .expect("the shipped pack exists")
    }

    /// Three loaded packs and two that are not, in catalogue order, nothing in
    /// use - so the list is the packs section alone.
    fn mixed() -> PackPicker {
        let mut entries: Vec<PackEntry> = listing()
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
            None,
            None,
        )
    }

    fn of_kind(picker: &PackPicker, kind: RowKind) -> Vec<Row> {
        picker
            .rows()
            .into_iter()
            .filter(|row| row.kind == kind)
            .collect()
    }

    #[test]
    fn it_opens_on_the_next_value_above_every_pack() {
        let next = value_id("whitespace", 3);
        let picker = PackPicker::new(Ok(listing()), Some("whitespace"), Some(&next));
        let rows = picker.rows();
        assert_eq!(rows[0].kind, RowKind::Heading);
        assert_eq!(rows[0].title, "Values in Whitespace");
        assert_eq!(of_kind(&picker, RowKind::Value).len(), 12);
        assert_eq!(
            of_kind(&picker, RowKind::Pack).len(),
            9,
            "every pack, below"
        );
        let selected = picker.selected().expect("a row is selected");
        assert_eq!(rows[selected].kind, RowKind::Value);
        assert_eq!(rows[selected].detail, "Whitespace - value 3 of 12");
        assert_eq!(
            rows[selected].badge,
            Some(Badge {
                text: String::from("next"),
                risky: false,
                current: true
            })
        );
        assert_eq!(
            picker.chosen(),
            Chosen::Value {
                pack: String::from("whitespace"),
                value: next
            },
            "Enter on the next value changes nothing - it is the next value already"
        );
        assert_eq!(picker.summary(), "values: 12, packs: 9");
        assert_eq!(picker.empty_text(), "");
    }

    /// `UX-GUI-007`: the way back to the start of the pack in use stands where
    /// its values are listed, under the action's own name, and it does what
    /// the restart shortcut does - the first value becomes the next one.
    #[test]
    fn the_pack_in_use_opens_with_a_restart_row_that_makes_value_one_next() {
        let next = value_id("whitespace", 3);
        let mut picker = PackPicker::new(Ok(listing()), Some("whitespace"), Some(&next))
            .with_restart_key(Some(String::from("Alt+Shift+0")));
        let rows = picker.rows();
        assert_eq!(rows[0].kind, RowKind::Heading);
        assert_eq!(
            rows[1],
            Row {
                kind: RowKind::Restart,
                title: String::from("Restart pack"),
                detail: String::new(),
                badge: None,
                current: false,
                enabled: true,
                key: Some(String::from("Alt+Shift+0")),
            },
            "the restart row opens the section, above value one"
        );
        assert_eq!(rows[2].detail, "Whitespace - value 1 of 12");
        assert_eq!(of_kind(&picker, RowKind::Restart).len(), 1);
        assert_eq!(
            picker.summary(),
            "values: 12, packs: 9",
            "the restart row is not a value"
        );
        assert_eq!(
            picker.click(1),
            Chosen::Value {
                pack: String::from("whitespace"),
                value: value_id("whitespace", 1)
            }
        );

        // At the end of a pack the window opens on the pack in use, and the
        // way back to the start is still there.
        let ended = shipped(Some("unicode-text"));
        assert_eq!(of_kind(&ended, RowKind::Restart).len(), 1);
        assert_eq!(
            ended.chosen(),
            Chosen::InUse,
            "Enter out of habit still only closes"
        );

        // It belongs to the pack in use: not without one, and not in a search.
        assert!(of_kind(&shipped(None), RowKind::Restart).is_empty());
        type_in(&mut picker, "space");
        assert!(of_kind(&picker, RowKind::Restart).is_empty());
    }

    #[test]
    fn without_a_next_value_it_opens_on_the_pack_in_use_never_on_value_one() {
        // Enter out of habit must not send the tester back to the start.
        let picker = shipped(Some("unicode-text"));
        let selected = picker.selected().expect("a row is selected");
        let rows = picker.rows();
        assert_eq!(rows[selected].kind, RowKind::Pack);
        assert!(
            rows[selected].current,
            "the selected row is the pack in use"
        );
        assert_eq!(picker.chosen(), Chosen::InUse);
    }

    #[test]
    fn a_pack_not_in_the_list_starts_on_the_first_that_can_be_chosen() {
        let picker = shipped(Some("no-such-pack"));
        let rows = picker.rows();
        assert_eq!(rows[0].kind, RowKind::Heading, "only the packs section");
        assert_eq!(
            picker.selected(),
            Some(1),
            "the first pack, past its heading"
        );
    }

    #[test]
    fn a_value_is_found_by_its_own_name_in_any_pack() {
        // The owner's case: PESEL is a value of `locale-pl`, whose name, tags
        // and description never say "pesel".
        let mut picker = shipped(Some("whitespace"));
        type_in(&mut picker, "pesel");
        let rows = picker.rows();
        assert_eq!(rows[0].title, "Values");
        let values = of_kind(&picker, RowKind::Value);
        assert!(!values.is_empty(), "PESEL is found");
        assert!(
            values.iter().all(|row| row.title.contains("PESEL")),
            "{values:?}"
        );
        assert!(
            values
                .iter()
                .all(|row| row.detail.starts_with("Polish locale - value ")),
            "the row says which pack: {values:?}"
        );
        assert!(of_kind(&picker, RowKind::Pack).is_empty());
        let Chosen::Value { pack, value } = picker.chosen() else {
            panic!("the first value found is selected: {:?}", picker.chosen());
        };
        assert_eq!(pack, "locale-pl");
        assert!(value.starts_with("pesel"), "{value}");
    }

    #[test]
    fn a_pack_id_finds_the_pack_and_not_its_values() {
        let mut picker = shipped(Some("whitespace"));
        type_in(&mut picker, "unicode-text");
        assert!(
            of_kind(&picker, RowKind::Value).is_empty(),
            "values do not match on their pack"
        );
        assert_eq!(of_kind(&picker, RowKind::Pack).len(), 1);
        assert_eq!(picker.chosen(), Chosen::Pack(String::from("unicode-text")));
    }

    #[test]
    fn words_must_all_match_and_case_is_not_minded() {
        let mut picker = shipped(Some("whitespace"));
        type_in(&mut picker, "UNICODE");
        assert!(
            picker
                .rows()
                .iter()
                .any(|row| row.title.contains("Unicode"))
        );
        let narrowed = picker.rows().len();
        type_in(&mut picker, " zzz");
        assert!(picker.rows().is_empty(), "every word must match");
        assert_eq!(picker.summary(), "values: 0 of 102, packs: 0 of 9");
        assert_eq!(
            picker.empty_text(),
            "Nothing matches \"UNICODE zzz\". Press Backspace to widen the search."
        );
        assert_eq!(picker.chosen(), Chosen::Nothing);
        for _ in 0..4 {
            picker.press(key("\u{8}"));
        }
        assert_eq!(picker.rows().len(), narrowed, "Backspace widens again");
    }

    /// Whitespace alone, its first value given tags and fields of its own -
    /// the shipped values have none, and a team pack's may.
    fn tagged() -> PackPicker {
        let entries = listing()
            .entries
            .into_iter()
            .filter_map(|entry| match entry {
                PackEntry::Loaded { mut pack, warnings } if pack.id == "whitespace" => {
                    pack.values[0].tags = vec![String::from("needle-tag")];
                    pack.values[0].fields = vec![String::from("needle-field")];
                    Some(PackEntry::Loaded { pack, warnings })
                }
                _ => None,
            })
            .collect();
        PackPicker::new(
            Ok(Listing {
                entries,
                coverage: CatalogueCoverage {
                    consulted: vec![CatalogueSource::BuiltIn],
                    skipped: Vec::new(),
                },
            }),
            None,
            None,
        )
    }

    /// `UX-GUI-008`: a value found by its id says so, and one found by what
    /// its row shows says nothing more.
    #[test]
    fn a_value_found_by_its_id_says_so() {
        for (query, detail) in [
            ("nbsp", "Whitespace - value 5 of 12 - in its id"),
            ("words nbsp", "Whitespace - value 5 of 12 - in its id"),
            ("between words", "Whitespace - value 5 of 12"),
        ] {
            let mut picker = shipped(Some("whitespace"));
            type_in(&mut picker, query);
            let values = of_kind(&picker, RowKind::Value);
            let row = values
                .iter()
                .find(|row| row.title == "Non-breaking space between words")
                .unwrap_or_else(|| panic!("{query:?} finds the value: {values:?}"));
            assert_eq!(row.detail, detail, "{query:?}");
        }
    }

    /// A word a shown part holds is named nowhere, and every other word gets
    /// the FIRST hidden part that holds it - each place once, in one order.
    #[test]
    fn the_row_names_each_hidden_place_a_word_needed_once_and_in_order() {
        for (query, detail) in [
            ("needle", "Whitespace - value 1 of 12 - in its tags"),
            (
                "needle-tag needle",
                "Whitespace - value 1 of 12 - in its tags",
            ),
            ("needle-field", "Whitespace - value 1 of 12 - in its fields"),
            (
                "needle-field needle-tag",
                "Whitespace - value 1 of 12 - in its tags and fields",
            ),
            (
                "trailing needle-field",
                "Whitespace - value 1 of 12 - in its fields",
            ),
            (
                "trailing-space needle-field needle-tag",
                "Whitespace - value 1 of 12 - in its id, tags and fields",
            ),
            // "space" is in the id too, and the name shows it first.
            ("trailing space", "Whitespace - value 1 of 12"),
        ] {
            let mut picker = tagged();
            type_in(&mut picker, query);
            let values = of_kind(&picker, RowKind::Value);
            let row = values
                .iter()
                .find(|row| row.title == "Trailing space")
                .unwrap_or_else(|| panic!("{query:?} finds the value: {values:?}"));
            assert_eq!(row.detail, detail, "{query:?}");
        }
    }

    /// The audit's case: Whitespace listed for "unicode" with no reason in
    /// sight - a tag. The place stands before the description, which the view
    /// elides at its end.
    #[test]
    fn a_pack_found_by_a_tag_says_so_before_its_description() {
        let mut picker = shipped(None);
        type_in(&mut picker, "unicode");
        let packs = of_kind(&picker, RowKind::Pack);
        let row = |title: &str| {
            packs
                .iter()
                .find(|row| row.title == title)
                .unwrap_or_else(|| panic!("{title} is listed: {packs:?}"))
                .detail
                .clone()
        };
        assert_eq!(
            row("Whitespace"),
            "whitespace, values: 12 - in its tags - Characters that take up space, or claim to, \
             and are impossible to see in a form."
        );
        assert_eq!(
            row("Unicode and text"),
            "unicode-text, values: 12 - Characters that look innocent and break counting, \
             comparison and display.",
            "found by its name and id, which the row shows"
        );
    }

    #[test]
    fn the_id_and_the_tags_of_a_pack_are_searched_too() {
        let mut picker = shipped(None);
        type_in(&mut picker, "locale-pl");
        assert_eq!(of_kind(&picker, RowKind::Pack).len(), 1);
        assert_eq!(picker.chosen(), Chosen::Pack(String::from("locale-pl")));
    }

    #[test]
    fn the_selection_stays_on_its_row_while_the_query_still_shows_it() {
        let mut picker = shipped(Some("unicode-text"));
        type_in(&mut picker, "u");
        assert_eq!(picker.chosen(), Chosen::InUse, "unicode-text still matches");
        type_in(&mut picker, "nicode");
        assert_eq!(picker.chosen(), Chosen::InUse);
    }

    #[test]
    fn a_heading_is_never_selected_nor_clicked() {
        let next = value_id("whitespace", 1);
        let mut picker = PackPicker::new(Ok(listing()), Some("whitespace"), Some(&next));
        assert_eq!(
            picker.selected(),
            Some(2),
            "value one, under its heading and the restart row"
        );
        picker.previous();
        assert_eq!(picker.selected(), Some(1), "up to the restart row");
        picker.previous();
        assert_eq!(
            picker.selected(),
            Some(1),
            "the heading above is skipped and there is no wrap"
        );
        assert_eq!(picker.click(0), Chosen::Nothing);
        assert_eq!(
            picker.selected(),
            Some(1),
            "a click on a heading moves nothing"
        );
        // Down from the last value of the pack: past the "Packs" heading.
        for _ in 0..13 {
            picker.next();
        }
        let rows = picker.rows();
        let at = picker.selected().expect("a row is selected");
        assert_eq!(rows[at].kind, RowKind::Pack);
        assert_eq!(rows[at - 1].kind, RowKind::Heading);
    }

    #[test]
    fn refused_and_unreadable_packs_stay_listed_and_are_skipped() {
        let mut picker = mixed();
        let rows = picker.rows();
        let titles: Vec<&str> = rows.iter().map(|row| row.title.as_str()).collect();
        assert_eq!(
            titles.len(),
            6,
            "a heading and five packs, nothing vanished: {titles:?}"
        );
        assert_eq!((titles[2], titles[5]), ("broken", "gone"));
        assert!(!rows[2].enabled && !rows[5].enabled);
        assert_eq!(rows[2].detail, "broken, does not load");
        assert_eq!(
            rows[2].badge,
            Some(Badge {
                text: String::from("problems: 3"),
                risky: true,
                current: false
            })
        );
        assert_eq!(rows[5].detail, "gone, cannot be read");

        assert_eq!(picker.selected(), Some(1));
        picker.next();
        assert_eq!(picker.selected(), Some(3), "the refused row is skipped");
        picker.next();
        picker.next();
        assert_eq!(
            picker.selected(),
            Some(4),
            "the unreadable last row is skipped and the selection stays"
        );
        picker.previous();
        picker.previous();
        assert_eq!(picker.selected(), Some(1), "skipped on the way up too");
        picker.previous();
        assert_eq!(
            picker.selected(),
            Some(1),
            "the heading is skipped and there is no wrap"
        );
    }

    #[test]
    fn a_click_on_a_row_that_cannot_be_chosen_chooses_nothing() {
        let mut picker = mixed();
        assert_eq!(picker.click(2), Chosen::Nothing);
        assert_eq!(picker.selected(), Some(1), "and moves nothing");
        assert_eq!(picker.click(99), Chosen::Nothing);
        assert_eq!(picker.click(3), Chosen::Pack(String::from("unicode-text")));
    }

    #[test]
    fn a_pack_row_says_what_the_pack_is_for() {
        // UX-GUI-008: the query searches the description, so the row shows it.
        let picker = shipped(None);
        let listing = listing();
        for (entry, row) in listing.entries.iter().zip(of_kind(&picker, RowKind::Pack)) {
            let PackEntry::Loaded { pack, .. } = entry else {
                continue;
            };
            assert_eq!(
                row.detail,
                format!(
                    "{}, values: {} - {}",
                    pack.id,
                    pack.values.len(),
                    pack.description
                )
            );
        }
    }

    #[test]
    fn a_pack_is_offensive_when_any_of_its_values_is() {
        let picker = shipped(None);
        let listing = listing();
        for (entry, row) in listing.entries.iter().zip(of_kind(&picker, RowKind::Pack)) {
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
    fn an_offensive_value_says_so_unless_it_is_the_next_one() {
        // The shipped catalogue has no offensive value today (`injections`
        // waits on OBS-130), so the test marks one pack offensive itself.
        let offensive_whitespace = || {
            let mut catalogue = listing();
            for entry in &mut catalogue.entries {
                if let PackEntry::Loaded { pack, .. } = entry
                    && pack.id == "whitespace"
                {
                    pack.risk = Risk::Offensive;
                }
            }
            catalogue
        };
        let pack = String::from("whitespace");
        let value = value_id("whitespace", 2);
        let picker = PackPicker::new(Ok(offensive_whitespace()), Some(&pack), None);
        let marked = of_kind(&picker, RowKind::Value)
            .into_iter()
            .filter(|row| row.badge.as_ref().is_some_and(|badge| badge.risky))
            .count();
        assert_eq!(
            marked, 12,
            "every value of an offensive pack in use is marked"
        );
        let picker = PackPicker::new(Ok(offensive_whitespace()), Some(&pack), Some(&value));
        let at = picker.selected().expect("the next value is selected");
        let badge = picker.rows()[at]
            .badge
            .clone()
            .expect("the next value has a pill");
        assert!(
            badge.current && !badge.risky,
            "the next pill wins: {badge:?}"
        );
    }

    #[test]
    fn a_catalogue_that_cannot_be_read_says_so_and_offers_nothing() {
        let picker = PackPicker::new(Err(SourceError::Unreadable), Some("whitespace"), None);
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
            [
                "This version lists the built-in packs only - team and own pack folders are not in it yet."
            ]
        );
    }
}
