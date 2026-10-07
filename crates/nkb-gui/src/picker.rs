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
//! # What the list is made of - a tree of packs (the owner's points 4 to 6)
//!
//! With nothing typed the list is a TREE: every pack, with its name and the
//! whole of what it is for, and every one FOLDED - its values stand under it
//! only once the tester opens it. Above the packs, a fold of the values the
//! tester sent or chose last in other packs (`UX-GUI-016`), folded as well.
//! Until 2026-10-07 the window opened with the values of the pack in use
//! spread out and the next one selected (UX2), and the packs - what the window
//! is opened for most - stood a dozen rows down with their descriptions cut
//! off. The owner asked for the values folded "by design" and for the
//! descriptions to be readable. So the window opens on the pack in use, where
//! Enter only closes, and a pack row carries no id and no count: the name, and
//! the description in full.
//!
//! Each value row says what the value types, in the palette's own preview
//! (`live::preview_line`, one function for both) - `Kowalski␣`, `255 × "a"` -
//! where until then it said which pack and which position (the owner's point
//! 5: "what does it do, like the main window shows what goes in"). A value
//! listed away from its pack - used last, or found - names its pack beside it.
//!
//! With a query, the values that match come first, from every pack, then the
//! packs that match: a flat list, where folds do not apply - a search that
//! hid what it found inside a closed pack would be no search.
//!
//! The pack in use, opened, starts with `Restart pack`, under the action's own
//! name and with its shortcut at the end (`UX-GUI-007`): it makes the first
//! value the next one, as the shortcut does (`Sequence::restart` and
//! `set_next(1)` reach one position).
//!
//! Every entry `list_packs` returns is a pack row, including a pack that is
//! present and refused, and one whose file would not open. Those two stay in
//! the list, faded, not choosable and not foldable: a pack that vanished from a
//! list cannot be asked about (untouchable rule 1). A heading is never chosen
//! either. A pack counts as offensive when the pack says so OR any one of its
//! values does, because a tester choosing a pack is about to send every value
//! in it.
//!
//! # Folding and choosing
//!
//! Enter and a click CHOOSE - a pack, a value - as they always did. The mark at
//! a row's left and the arrows FOLD: right opens a closed fold and steps into
//! an open one, left closes an open fold and steps out of a value to its pack.
//! The fold of the values used last has nothing to choose, so Enter and a
//! click on it fold it too. What is open lasts while the window is open: the
//! window builds a fresh picker on every opening, so every opening starts
//! folded.
//!
//! # The query
//!
//! Words, all of which must appear, case not minded. A pack matches in its
//! name, description, id or tags. A value matches in its OWN name, id, tags and
//! fields, and not in the name of its pack: "unicode-text" finds the pack, not
//! every one of its values (document 15 section 1, start narrower). No folding of
//! diacritics: the catalogue's prose is English (`D23`, untouchable rule 8), and
//! a wider match is easier to add than a narrower one is to take back.
//!
//! A row found by a part it does not show says which (`UX-GUI-008`): a value's
//! id, tags or fields, a pack's id or tags - "in its tags". Without it a row
//! found there looked found at random: the audit's example was the pack
//! Whitespace, listed for "unicode" by a tag. A word a shown part holds is
//! explained by the row and named nowhere.

use std::collections::BTreeSet;

use nkb_adapters::i18n::{self, MatchPlace, PacksLabel};
use nkb_app::browse_packs::{Listing, PackEntry};
use nkb_app::ports::SourceError;
use nkb_core::hotkeys::HotkeyAction;
use nkb_core::identity::ValueKey;
use nkb_core::pack::{Pack, PackValue, Risk};
use nkb_core::preview::preview_of;

use crate::live::preview_line;
use crate::query::{KeyPress, Pressed, Query};

/// One pack as the window offers it.
#[derive(Debug, Clone)]
struct PackChoice {
    id: String,
    title: String,
    /// What the pack is for, or - for a pack that does not load - why not.
    detail: String,
    badge: Option<Badge>,
    enabled: bool,
    /// Name and description shown, id and tags not.
    haystack: Haystack,
    /// Empty for a pack that does not load.
    values: Vec<ValueChoice>,
}

/// One value as the window offers it.
#[derive(Debug, Clone)]
struct ValueChoice {
    id: String,
    name: String,
    /// What it types, as the palette previews it - one line.
    preview: String,
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
    /// The fold of the values used last.
    RecentFold,
    Pack(usize),
    /// A value under its open pack, or found by a query.
    Value {
        pack: usize,
        value: usize,
    },
    /// A value under the open fold of the values used last - the same value
    /// may stand under its own open pack too, so it is a row of its own kind.
    Recent {
        pack: usize,
        value: usize,
    },
    /// Start this pack - the one in use - from its first value.
    Restart(usize),
}

/// Which section a heading opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RowKind {
    #[default]
    Heading,
    /// The fold of the values used last - a heading that opens and closes.
    Fold,
    Pack,
    Value,
    /// The row that starts the pack in use again.
    Restart,
}

/// A row as the window draws it: finished strings and facts, no toolkit type.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Row {
    pub kind: RowKind,
    pub title: String,
    /// The second line: what a pack is for, or what a value types.
    pub detail: String,
    /// Beside the row, in the muted role: the pack of a value listed away
    /// from it, and where a query found a row in a part it does not show.
    pub aside: Option<String>,
    pub badge: Option<Badge>,
    /// The pack the palette holds now.
    pub current: bool,
    pub enabled: bool,
    /// The key combination at the end of the row - the restart row's own
    /// shortcut. `None` for every other row.
    pub key: Option<String>,
    /// `Some(open)` for a row that folds what stands under it.
    pub fold: Option<bool>,
    /// The row stands in the tree - nothing is typed - so it keeps the column
    /// of the fold marks, mark or not.
    pub tree: bool,
    /// The row stands under another one in the tree.
    pub child: bool,
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
    /// A fold opened or closed: the list changed shape, and the window stays.
    Folded,
    /// No row can be chosen - nothing matches, or nothing loads. The window
    /// stays open, because closing it would look like a choice was made.
    Nothing,
}

/// What a fold key or a click on a fold mark did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Folding {
    /// Nothing - not a row that folds, or a query is typed.
    Nothing,
    /// The selection moved, the rows stand as they were.
    Moved,
    /// A fold opened or closed: the rows changed.
    Reshaped,
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
    /// The values the tester used last, the most recent first.
    recent: Vec<ValueKey>,
    /// Whether the fold of the values used last is open.
    recent_open: bool,
    /// The packs whose values stand under them, by index.
    open: BTreeSet<usize>,
}

impl PackPicker {
    /// The picker over what `list_packs` found, every fold closed, opening on
    /// the pack in use - or on the first pack that can be chosen.
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
            recent: Vec::new(),
            recent_open: false,
            open: BTreeSet::new(),
        };
        picker.filter();
        picker.select_on_opening();
        picker
    }

    /// The same picker with the values the tester used last in a fold above
    /// the packs, the most recent first (`UX-GUI-016`). Opening still selects
    /// the pack in use - the recent values are a way back, not where the
    /// tester is.
    #[must_use]
    pub fn with_recent(mut self, recent: &[ValueKey]) -> Self {
        recent.clone_into(&mut self.recent);
        self.filter();
        self.select_on_opening();
        self
    }

    /// Where the selection stands when the window opens: the pack in use,
    /// where Enter out of habit only closes - never on a value, where it would
    /// move the tester's place.
    fn select_on_opening(&mut self) {
        self.selected = self.position_of_in_use().or_else(|| self.first_enabled());
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
                .and_then(|item| self.position_or_parent(item))
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

    /// What Enter comes to, without doing it - a fold answers nothing here,
    /// because opening it is what [`Self::enter`] does.
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
            Some(Item::Value { pack, value } | Item::Recent { pack, value }) => Chosen::Value {
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
            Some(Item::Heading(_) | Item::RecentFold) | None => Chosen::Nothing,
        }
    }

    /// Enter: what [`Self::chosen`] says - or, on the fold of the values used
    /// last, which has nothing to choose, opening or closing it.
    pub fn enter(&mut self) -> Chosen {
        if self.selected_item() == Some(Item::RecentFold) {
            self.flip(Item::RecentFold);
            return Chosen::Folded;
        }
        self.chosen()
    }

    /// A click on row `row` of the list as drawn: selects it and says what it
    /// comes to - the fold of the values used last opens or closes. A click
    /// on a row that cannot be chosen chooses nothing and moves nothing.
    pub fn click(&mut self, row: usize) -> Chosen {
        if row < self.visible.len() && self.enabled_at(row) {
            self.selected = Some(row);
            self.enter()
        } else {
            Chosen::Nothing
        }
    }

    /// A click on the fold mark of row `row`: opens or closes it, and the row
    /// becomes the selected one.
    pub fn toggle(&mut self, row: usize) -> Folding {
        let Some(&item) = self.visible.get(row) else {
            return Folding::Nothing;
        };
        if self.fold_of(item).is_none() {
            return Folding::Nothing;
        }
        self.flip(item);
        Folding::Reshaped
    }

    /// The right arrow: opens the selected fold, or steps into it when it is
    /// open already.
    pub fn unfold(&mut self) -> Folding {
        let Some(item) = self.selected_item() else {
            return Folding::Nothing;
        };
        match self.fold_of(item) {
            Some(false) => {
                self.flip(item);
                Folding::Reshaped
            }
            Some(true) => {
                let before = self.selected;
                self.next();
                if self.selected == before {
                    Folding::Nothing
                } else {
                    Folding::Moved
                }
            }
            None => Folding::Nothing,
        }
    }

    /// The left arrow: closes the selected fold, or steps out of a value to
    /// the row it stands under.
    pub fn fold(&mut self) -> Folding {
        let Some(item) = self.selected_item() else {
            return Folding::Nothing;
        };
        if self.fold_of(item) == Some(true) {
            self.flip(item);
            return Folding::Reshaped;
        }
        if !self.in_tree() {
            return Folding::Nothing;
        }
        let parent = match item {
            Item::Value { pack, .. } | Item::Restart(pack) => Item::Pack(pack),
            Item::Recent { .. } => Item::RecentFold,
            Item::Heading(_) | Item::RecentFold | Item::Pack(_) => return Folding::Nothing,
        };
        match self.visible.iter().position(|&at| at == parent) {
            Some(at) => {
                self.selected = Some(at);
                Folding::Moved
            }
            None => Folding::Nothing,
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

    /// How many values and packs the catalogue holds - and, while a query
    /// hides some, how many of them it shows. With nothing typed the values
    /// are folded, so the count is the catalogue's, not the rows'.
    #[must_use]
    pub fn summary(&self) -> String {
        let all_values = self.packs.iter().map(|pack| pack.values.len()).sum();
        if self.in_tree() {
            return i18n::packs_summary(all_values, self.packs.len());
        }
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
        i18n::packs_summary_filtered(values, all_values, packs, self.packs.len())
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

    /// Whether the list is the tree - nothing typed.
    fn in_tree(&self) -> bool {
        self.query.as_str().trim().is_empty()
    }

    fn filter(&mut self) {
        let words = words_of(self.query.as_str());
        let matches = |haystack: &Haystack| haystack.matches(&words);
        let mut visible = Vec::new();
        if words.is_empty() {
            let recent = self.recent_items();
            if !recent.is_empty() {
                visible.push(Item::RecentFold);
                if self.recent_open {
                    visible.extend(recent);
                }
            }
            if !self.packs.is_empty() {
                visible.push(Item::Heading(Section::Packs));
            }
            let in_use = self.in_use_index();
            for at in 0..self.packs.len() {
                visible.push(Item::Pack(at));
                if self.open.contains(&at) && self.foldable(at) {
                    if in_use == Some(at) {
                        visible.push(Item::Restart(at));
                    }
                    visible.extend(
                        (0..self.packs[at].values.len())
                            .map(|value| Item::Value { pack: at, value }),
                    );
                }
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

    /// Opens or closes `item`'s fold, and keeps it the selected row.
    fn flip(&mut self, item: Item) {
        match item {
            Item::RecentFold => self.recent_open = !self.recent_open,
            Item::Pack(at) => {
                if !self.open.remove(&at) {
                    self.open.insert(at);
                }
            }
            Item::Heading(_) | Item::Value { .. } | Item::Recent { .. } | Item::Restart(_) => {
                return;
            }
        }
        self.filter();
        self.selected = self
            .visible
            .iter()
            .position(|&at| at == item)
            .or_else(|| self.first_enabled());
    }

    /// Whether `item` folds, and if so whether it is open. Nothing folds while
    /// a query is typed.
    fn fold_of(&self, item: Item) -> Option<bool> {
        if !self.in_tree() {
            return None;
        }
        match item {
            Item::RecentFold => Some(self.recent_open),
            Item::Pack(at) if self.foldable(at) => Some(self.open.contains(&at)),
            _ => None,
        }
    }

    /// A pack folds when it loads and has values to show.
    fn foldable(&self, at: usize) -> bool {
        self.packs[at].enabled && !self.packs[at].values.is_empty()
    }

    /// Where `item` stands now, or - for a value folded away, in the tree -
    /// the row it stands under. A selection that survives the query being
    /// erased lands on the pack of the value it was on, not on the top.
    fn position_or_parent(&self, item: Item) -> Option<usize> {
        let find = |wanted: Item| self.visible.iter().position(|&at| at == wanted);
        find(item).or_else(|| match item {
            Item::Value { pack, .. } | Item::Restart(pack) => find(Item::Pack(pack)),
            Item::Recent { .. } => find(Item::RecentFold),
            Item::Heading(_) | Item::RecentFold | Item::Pack(_) => None,
        })
    }

    /// Row `item` as drawn, `words` being the query's.
    fn row_of(&self, item: Item, words: &[String]) -> Row {
        let tree = words.is_empty();
        match item {
            Item::Heading(section) => Row {
                kind: RowKind::Heading,
                title: i18n::packs_label(match section {
                    Section::Found => PacksLabel::FoundValues,
                    Section::Packs => PacksLabel::PacksSection,
                })
                .to_owned(),
                tree,
                ..Row::default()
            },
            Item::RecentFold => Row {
                kind: RowKind::Fold,
                title: i18n::recent_heading(self.recent_items().len()),
                enabled: true,
                fold: Some(self.recent_open),
                tree,
                ..Row::default()
            },
            Item::Pack(at) => {
                let choice = &self.packs[at];
                Row {
                    kind: RowKind::Pack,
                    title: choice.title.clone(),
                    detail: choice.detail.clone(),
                    aside: if choice.enabled {
                        i18n::found_in(&choice.haystack.places(words))
                    } else {
                        None
                    },
                    badge: choice.badge.clone(),
                    current: self.in_use.as_deref() == Some(choice.id.as_str()),
                    enabled: choice.enabled,
                    fold: self.fold_of(item),
                    tree,
                    ..Row::default()
                }
            }
            Item::Value { pack, value } => {
                let owner = &self.packs[pack];
                let choice = &owner.values[value];
                let is_next = self.in_use.as_deref() == Some(owner.id.as_str())
                    && self.next.as_deref() == Some(choice.id.as_str());
                Row {
                    kind: RowKind::Value,
                    title: choice.name.clone(),
                    detail: choice.preview.clone(),
                    // Under its own pack in the tree the pack goes without
                    // saying. Found, it is named, with where the query hit.
                    aside: (!tree)
                        .then(|| i18n::value_aside(&owner.title, &choice.haystack.places(words))),
                    badge: value_badge(choice, is_next),
                    enabled: true,
                    tree,
                    child: tree,
                    ..Row::default()
                }
            }
            Item::Recent { pack, value } => {
                let owner = &self.packs[pack];
                let choice = &owner.values[value];
                Row {
                    kind: RowKind::Value,
                    title: choice.name.clone(),
                    detail: choice.preview.clone(),
                    aside: Some(i18n::value_aside(&owner.title, &[])),
                    // Never the next one: the fold holds other packs' values.
                    badge: value_badge(choice, false),
                    enabled: true,
                    tree,
                    child: true,
                    ..Row::default()
                }
            }
            // The action's own name and its shortcut, the words the hint bar
            // gives them, so the row and the bar teach one thing.
            Item::Restart(_) => Row {
                kind: RowKind::Restart,
                title: i18n::action_name(HotkeyAction::RestartPack).to_owned(),
                enabled: true,
                key: self.restart_key.clone(),
                tree,
                child: true,
                ..Row::default()
            },
        }
    }

    /// The recent values as rows: each that the catalogue still has, in a pack
    /// that loads and is not the one in use, in the order used.
    fn recent_items(&self) -> Vec<Item> {
        let in_use = self.in_use_index();
        self.recent
            .iter()
            .filter_map(|key| {
                let pack = self
                    .packs
                    .iter()
                    .position(|pack| pack.enabled && pack.id == key.pack)?;
                if Some(pack) == in_use {
                    return None;
                }
                let value = self.packs[pack]
                    .values
                    .iter()
                    .position(|value| value.id == key.value)?;
                Some(Item::Recent { pack, value })
            })
            .collect()
    }

    fn enabled_at(&self, position: usize) -> bool {
        match self.visible[position] {
            Item::Heading(_) => false,
            Item::Pack(at) => self.packs[at].enabled,
            Item::RecentFold | Item::Value { .. } | Item::Recent { .. } | Item::Restart(_) => true,
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

    fn selected_item(&self) -> Option<Item> {
        self.selected.map(|at| self.visible[at])
    }
}

/// The one pill of a value row: "next" over "offensive" - the palette's next
/// band already marks the risk of the next value.
fn value_badge(choice: &ValueChoice, is_next: bool) -> Option<Badge> {
    if is_next {
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
    }
}

fn choice_of(entry: &PackEntry) -> PackChoice {
    match entry {
        PackEntry::Loaded { pack, .. } => PackChoice {
            id: pack.id.clone(),
            title: pack.name.clone(),
            detail: pack.description.clone(),
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
                    preview: preview_line(&preview_of(&value.body)).0,
                    offensive: pack.risk_of(value) == Risk::Offensive,
                    haystack: value_haystack_of(value),
                })
                .collect(),
        },
        PackEntry::Refused { id, errors } => PackChoice {
            id: id.clone(),
            title: id.clone(),
            detail: i18n::pack_refused(id),
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
            detail: i18n::pack_unreadable(id),
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

/// Name and description, which the pack's row shows, and its id and tags,
/// which it does not.
fn haystack_of(pack: &Pack) -> Haystack {
    Haystack::new(
        &[pack.name.as_str(), pack.description.as_str()],
        std::iter::once((MatchPlace::Id, pack.id.as_str()))
            .chain(pack.tags.iter().map(|tag| (MatchPlace::Tags, tag.as_str()))),
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

    fn selected_row(picker: &PackPicker) -> Row {
        picker.rows()[picker.selected().expect("a row is selected")].clone()
    }

    /// The owner's points 4 and 6: the packs, every one folded, each with what
    /// it is for - and the window on the pack in use, where Enter only closes.
    #[test]
    fn it_opens_on_the_pack_in_use_with_every_pack_folded() {
        let next = value_id("whitespace", 3);
        let picker = PackPicker::new(Ok(listing()), Some("whitespace"), Some(&next));
        let rows = picker.rows();
        assert_eq!(rows[0].kind, RowKind::Heading);
        assert_eq!(rows[0].title, "Packs");
        assert!(
            of_kind(&picker, RowKind::Value).is_empty(),
            "no value is spread out on opening"
        );
        let packs = of_kind(&picker, RowKind::Pack);
        assert_eq!(packs.len(), 9);
        assert!(
            packs
                .iter()
                .all(|row| row.fold == Some(false) && row.tree && !row.child),
            "every pack folds, and every one is closed: {packs:?}"
        );
        let selected = selected_row(&picker);
        assert_eq!(selected.title, "Whitespace");
        assert!(selected.current, "the selected row is the pack in use");
        assert_eq!(picker.chosen(), Chosen::InUse);
        assert_eq!(
            picker.summary(),
            "values: 102, packs: 9",
            "the catalogue's numbers, not the rows' - the values are folded"
        );
        assert_eq!(picker.empty_text(), "");
    }

    /// Point 6: a pack row is its name and the whole of what it is for -
    /// no id and no count before the description to push it out of sight.
    #[test]
    fn a_pack_row_says_what_the_pack_is_for_and_nothing_before_it() {
        let picker = shipped(None);
        let listing = listing();
        for (entry, row) in listing.entries.iter().zip(of_kind(&picker, RowKind::Pack)) {
            let PackEntry::Loaded { pack, .. } = entry else {
                continue;
            };
            assert_eq!(row.title, pack.name);
            assert_eq!(row.detail, pack.description);
            assert_eq!(row.aside, None, "{}", pack.id);
        }
    }

    /// Point 4 and 5: opened, a pack lists its values under it, each saying
    /// what it types in the palette's own preview - and folds back.
    #[test]
    fn an_opened_pack_lists_its_values_with_what_each_types_and_folds_back() {
        let mut picker = shipped(Some("whitespace"));
        let whitespace = picker.selected().expect("the pack in use is selected");
        assert_eq!(picker.unfold(), Folding::Reshaped);
        let rows = picker.rows();
        assert_eq!(rows[whitespace].fold, Some(true));
        assert_eq!(
            picker.selected(),
            Some(whitespace),
            "the pack stays selected"
        );
        let values = of_kind(&picker, RowKind::Value);
        assert_eq!(values.len(), 12);
        assert!(
            values
                .iter()
                .all(|row| row.child && row.tree && row.aside.is_none())
        );
        let zero_width = values
            .iter()
            .find(|row| row.title == "Three zero-width spaces")
            .expect("a shipped value");
        assert_eq!(
            zero_width.detail, "ab\u{2423}\u{2423}\u{2423}cd",
            "the invisible characters as the palette's marker"
        );

        // The right arrow again steps into the open fold.
        assert_eq!(picker.unfold(), Folding::Moved);
        assert_eq!(
            selected_row(&picker).kind,
            RowKind::Restart,
            "the pack in use opens with the way back to its start"
        );
        // Left from inside goes back to the pack, and left again folds it.
        assert_eq!(picker.fold(), Folding::Moved);
        assert_eq!(picker.selected(), Some(whitespace));
        assert_eq!(picker.fold(), Folding::Reshaped);
        assert!(of_kind(&picker, RowKind::Value).is_empty());
        assert_eq!(
            picker.fold(),
            Folding::Nothing,
            "a closed pack folds no further"
        );

        // A generated value says its recipe, never a hundred letters (D67).
        let mut picker = shipped(None);
        let at = picker
            .rows()
            .iter()
            .position(|row| row.title == "Length bombs")
            .expect("a shipped pack");
        assert_eq!(picker.toggle(at), Folding::Reshaped);
        let len_255 = of_kind(&picker, RowKind::Value)
            .into_iter()
            .find(|row| row.title == "255 characters")
            .expect("a shipped value");
        assert_eq!(len_255.detail, "255 \u{D7} \"a\"");
        assert_eq!(
            picker.selected(),
            Some(at),
            "the clicked mark selects its pack"
        );
    }

    /// The previews are the palette's own - one function for both, so the
    /// window and the palette cannot tell two stories about one value.
    #[test]
    fn every_value_previews_as_the_palette_previews_it() {
        let mut picker = shipped(None);
        let packs: Vec<usize> = picker
            .rows()
            .iter()
            .enumerate()
            .filter(|(_, row)| row.fold.is_some())
            .map(|(at, _)| at)
            .collect();
        // Opened from the last up, so the positions above stay where they are.
        for at in packs.into_iter().rev() {
            picker.toggle(at);
        }
        let shown: Vec<String> = of_kind(&picker, RowKind::Value)
            .into_iter()
            .map(|row| row.detail)
            .collect();
        let expected: Vec<String> = listing()
            .entries
            .iter()
            .filter_map(|entry| match entry {
                PackEntry::Loaded { pack, .. } => Some(pack.values.clone()),
                _ => None,
            })
            .flatten()
            .map(|value| preview_line(&preview_of(&value.body)).0)
            .collect();
        assert_eq!(shown.len(), 102);
        assert_eq!(shown, expected);
    }

    /// `UX-GUI-007`: the way back to the start of the pack in use stands first
    /// under it, under the action's own name, and it does what the restart
    /// shortcut does - the first value becomes the next one.
    #[test]
    fn the_pack_in_use_opens_with_a_restart_row_that_makes_value_one_next() {
        let next = value_id("whitespace", 3);
        let mut picker = PackPicker::new(Ok(listing()), Some("whitespace"), Some(&next))
            .with_restart_key(Some(String::from("Alt+Shift+0")));
        let at = picker.selected().expect("the pack in use is selected");
        picker.unfold();
        let rows = picker.rows();
        assert_eq!(
            rows[at + 1],
            Row {
                kind: RowKind::Restart,
                title: String::from("Restart pack"),
                enabled: true,
                key: Some(String::from("Alt+Shift+0")),
                tree: true,
                child: true,
                ..Row::default()
            },
            "the restart row opens the pack, above value one"
        );
        assert_eq!(rows[at + 2].title, "Trailing space");
        assert_eq!(of_kind(&picker, RowKind::Restart).len(), 1);
        // The next value wears the pill, in the accent's role.
        assert_eq!(
            rows[at + 4].badge,
            Some(Badge {
                text: String::from("next"),
                risky: false,
                current: true
            })
        );
        assert_eq!(
            picker.click(at + 1),
            Chosen::Value {
                pack: String::from("whitespace"),
                value: value_id("whitespace", 1)
            }
        );

        // Another pack, opened, has no restart row: it belongs to the pack in
        // use. Not in a search either.
        let mut other = shipped(Some("whitespace"));
        let unicode = other
            .rows()
            .iter()
            .position(|row| row.title == "Unicode and text")
            .expect("a shipped pack");
        other.toggle(unicode);
        assert!(of_kind(&other, RowKind::Restart).is_empty());
        type_in(&mut picker, "space");
        assert!(of_kind(&picker, RowKind::Restart).is_empty());
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
        assert!(
            rows.iter()
                .all(|row| !row.tree && !row.child && row.fold.is_none())
        );
        let values = of_kind(&picker, RowKind::Value);
        assert!(!values.is_empty(), "PESEL is found");
        assert!(
            values.iter().all(|row| row.title.contains("PESEL")),
            "{values:?}"
        );
        assert!(
            values
                .iter()
                .all(|row| row.aside.as_deref() == Some("Polish locale")),
            "the row says which pack: {values:?}"
        );
        assert!(of_kind(&picker, RowKind::Pack).is_empty());
        let Chosen::Value { pack, value } = picker.chosen() else {
            panic!("the first value found is selected: {:?}", picker.chosen());
        };
        assert_eq!(pack, "locale-pl");
        assert!(value.starts_with("pesel"), "{value}");
        // Folding keys do nothing in a search.
        assert_eq!(picker.unfold(), Folding::Nothing);
        assert_eq!(picker.fold(), Folding::Nothing);
    }

    #[test]
    fn a_pack_id_finds_the_pack_and_not_its_values() {
        let mut picker = shipped(Some("whitespace"));
        type_in(&mut picker, "unicode-text");
        assert!(
            of_kind(&picker, RowKind::Value).is_empty(),
            "values do not match on their pack"
        );
        let packs = of_kind(&picker, RowKind::Pack);
        assert_eq!(packs.len(), 1);
        assert_eq!(
            packs[0].aside.as_deref(),
            Some("in its id"),
            "the row no longer shows the id, so it says it was found there"
        );
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
        for (query, aside) in [
            ("nbsp", "Whitespace - in its id"),
            ("words nbsp", "Whitespace - in its id"),
            ("between words", "Whitespace"),
        ] {
            let mut picker = shipped(Some("whitespace"));
            type_in(&mut picker, query);
            let values = of_kind(&picker, RowKind::Value);
            let row = values
                .iter()
                .find(|row| row.title == "Non-breaking space between words")
                .unwrap_or_else(|| panic!("{query:?} finds the value: {values:?}"));
            assert_eq!(row.aside.as_deref(), Some(aside), "{query:?}");
        }
    }

    /// A word a shown part holds is named nowhere, and every other word gets
    /// the FIRST hidden part that holds it - each place once, in one order.
    #[test]
    fn the_row_names_each_hidden_place_a_word_needed_once_and_in_order() {
        for (query, aside) in [
            ("needle", "Whitespace - in its tags"),
            ("needle-tag needle", "Whitespace - in its tags"),
            ("needle-field", "Whitespace - in its fields"),
            (
                "needle-field needle-tag",
                "Whitespace - in its tags and fields",
            ),
            ("trailing needle-field", "Whitespace - in its fields"),
            (
                "trailing-space needle-field needle-tag",
                "Whitespace - in its id, tags and fields",
            ),
            // "space" is in the id too, and the name shows it first.
            ("trailing space", "Whitespace"),
        ] {
            let mut picker = tagged();
            type_in(&mut picker, query);
            let values = of_kind(&picker, RowKind::Value);
            let row = values
                .iter()
                .find(|row| row.title == "Trailing space")
                .unwrap_or_else(|| panic!("{query:?} finds the value: {values:?}"));
            assert_eq!(row.aside.as_deref(), Some(aside), "{query:?}");
        }
    }

    /// The audit's case: Whitespace listed for "unicode" with no reason in
    /// sight - a tag. Said beside the name, apart from the description.
    #[test]
    fn a_pack_found_by_a_tag_says_so_beside_its_name() {
        let mut picker = shipped(None);
        type_in(&mut picker, "unicode");
        let packs = of_kind(&picker, RowKind::Pack);
        let row = |title: &str| {
            packs
                .iter()
                .find(|row| row.title == title)
                .unwrap_or_else(|| panic!("{title} is listed: {packs:?}"))
                .clone()
        };
        assert_eq!(row("Whitespace").aside.as_deref(), Some("in its tags"));
        assert_eq!(
            row("Whitespace").detail,
            "Characters that take up space, or claim to, and are impossible to see in a form."
        );
        assert_eq!(
            row("Unicode and text").aside,
            None,
            "found by its name, which the row shows"
        );
    }

    fn recent_key(pack: &str, value: &str) -> ValueKey {
        ValueKey {
            pack: pack.to_owned(),
            value: value.to_owned(),
        }
    }

    /// `UX-GUI-016` and the owner's choice of 2026-10-07: the values used last
    /// stand in a fold above the packs, closed, with how many it holds. Enter
    /// or a click opens it, and each value there names its pack.
    #[test]
    fn the_values_used_last_stand_in_a_closed_fold_on_top() {
        let next = value_id("whitespace", 3);
        let mut picker = PackPicker::new(Ok(listing()), Some("whitespace"), Some(&next))
            .with_recent(&[
                recent_key("locale-pl", "pesel-valid"),
                recent_key("whitespace", "trailing-space"),
                recent_key("unicode-text", &value_id("unicode-text", 2)),
            ]);
        let rows = picker.rows();
        assert_eq!(rows[0].kind, RowKind::Fold);
        assert_eq!(
            rows[0].title, "Recent (2)",
            "a value of the pack in use is under its own pack, not here"
        );
        assert_eq!(rows[0].fold, Some(false));
        assert_eq!(
            rows[1].title, "Packs",
            "closed: nothing between it and the packs"
        );
        assert_eq!(
            selected_row(&picker).title,
            "Whitespace",
            "the window still opens on the pack in use"
        );

        assert_eq!(
            picker.click(0),
            Chosen::Folded,
            "a click on the fold opens it"
        );
        let rows = picker.rows();
        assert_eq!(rows[0].fold, Some(true));
        assert_eq!(rows[1].title, "PESEL with a valid checksum");
        assert_eq!(rows[1].aside.as_deref(), Some("Polish locale"));
        assert!(rows[1].child);
        assert_eq!(rows[2].aside.as_deref(), Some("Unicode and text"));
        assert_eq!(
            picker.click(1),
            Chosen::Value {
                pack: String::from("locale-pl"),
                value: String::from("pesel-valid")
            },
            "a recent value is chosen like any other"
        );
        // Left from a recent value steps out to its fold, Enter folds it.
        assert_eq!(picker.fold(), Folding::Moved);
        assert_eq!(picker.selected(), Some(0));
        assert_eq!(picker.enter(), Chosen::Folded);
        assert_eq!(picker.rows()[1].title, "Packs");

        // The same value may stand under its own opened pack too - two rows,
        // each choosing it.
        let mut both =
            shipped(Some("whitespace")).with_recent(&[recent_key("locale-pl", "pesel-valid")]);
        both.click(0);
        let locale = both
            .rows()
            .iter()
            .position(|row| row.title == "Polish locale")
            .expect("a shipped pack");
        both.toggle(locale);
        let pesel: Vec<usize> = both
            .rows()
            .iter()
            .enumerate()
            .filter(|(_, row)| row.title == "PESEL with a valid checksum")
            .map(|(at, _)| at)
            .collect();
        assert_eq!(pesel.len(), 2);
        for at in pesel {
            assert_eq!(
                both.click(at),
                Chosen::Value {
                    pack: String::from("locale-pl"),
                    value: String::from("pesel-valid")
                }
            );
        }
    }

    /// A remembered value the catalogue no longer has, or whose pack does not
    /// load, is not listed - and with nothing to list there is no fold.
    #[test]
    fn a_recent_value_that_cannot_be_chosen_is_not_listed() {
        let gone = [
            recent_key("whitespace", "no-such-value"),
            recent_key("no-such-pack", "a"),
            recent_key("broken", "a"),
            recent_key("whitespace", "trailing-space"),
        ];
        let picker = mixed().with_recent(&gone);
        assert_eq!(
            picker.rows()[0].title,
            "Recent (1)",
            "nothing is in use in this list, so Whitespace's value is a recent one"
        );

        let picker = mixed().with_recent(&gone[..3]);
        assert_eq!(picker.rows()[0].title, "Packs", "no fold over nothing");
    }

    /// A query searches the whole catalogue, so the fold is gone while one is
    /// typed, and back when it is erased - as it was left.
    #[test]
    fn a_query_hides_the_recent_values_and_erasing_it_brings_the_tree_back() {
        let mut picker =
            shipped(Some("whitespace")).with_recent(&[recent_key("locale-pl", "pesel-valid")]);
        picker.click(0);
        type_in(&mut picker, "p");
        assert!(picker.rows().iter().all(|row| row.kind != RowKind::Fold));
        picker.press(key("\u{8}"));
        assert_eq!(
            picker.rows()[0].fold,
            Some(true),
            "the fold as the tester left it"
        );
    }

    /// A value selected in a search lands on its pack when the query is erased
    /// and the value is folded away - not on the top of the list.
    #[test]
    fn erasing_the_query_keeps_the_selection_on_the_pack_of_the_value() {
        let mut picker = shipped(Some("whitespace"));
        type_in(&mut picker, "pesel");
        assert!(matches!(picker.chosen(), Chosen::Value { .. }));
        for _ in 0..5 {
            picker.press(key("\u{8}"));
        }
        assert_eq!(selected_row(&picker).title, "Polish locale");
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
        let mut picker = shipped(None);
        assert_eq!(
            picker.selected(),
            Some(1),
            "the first pack, under its heading"
        );
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
        assert_eq!(
            picker.toggle(0),
            Folding::Nothing,
            "a heading does not fold"
        );
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
        assert_eq!(
            rows[2].fold, None,
            "a pack that does not load does not fold"
        );
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
        assert_eq!(picker.toggle(2), Folding::Nothing);

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
        assert_eq!(picker.toggle(99), Folding::Nothing);
        assert_eq!(picker.click(3), Chosen::Pack(String::from("unicode-text")));
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
        let mut picker = PackPicker::new(Ok(offensive_whitespace()), Some(&pack), None);
        picker.unfold();
        let marked = of_kind(&picker, RowKind::Value)
            .into_iter()
            .filter(|row| row.badge.as_ref().is_some_and(|badge| badge.risky))
            .count();
        assert_eq!(
            marked, 12,
            "every value of an offensive pack in use is marked"
        );
        let mut picker = PackPicker::new(Ok(offensive_whitespace()), Some(&pack), Some(&value));
        picker.unfold();
        let badge = of_kind(&picker, RowKind::Value)
            .into_iter()
            .find(|row| row.title == "Leading space")
            .and_then(|row| row.badge)
            .expect("the next value has a pill");
        assert!(
            badge.current && !badge.risky,
            "the next pill wins: {badge:?}"
        );
    }

    #[test]
    fn a_catalogue_that_cannot_be_read_says_so_and_offers_nothing() {
        let mut picker = PackPicker::new(Err(SourceError::Unreadable), Some("whitespace"), None);
        assert!(picker.rows().is_empty());
        assert_eq!(picker.selected(), None);
        assert_eq!(picker.chosen(), Chosen::Nothing);
        assert_eq!(picker.enter(), Chosen::Nothing);
        assert_eq!(picker.unfold(), Folding::Nothing);
        assert_eq!(picker.fold(), Folding::Nothing);
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
