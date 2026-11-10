//! What a query of the value window is matched in, and the query as words.
//! Out of `picker.rs`, which stands at the ceiling of `D113` - moved word for
//! word.

use nkb_adapters::i18n::MatchPlace;

/// What a query is matched in, lower-cased once: the parts a row shows, and
/// the parts it does not, each under its place (`UX-GUI-008`).
///
/// One structure answers both questions - does the row match, and where - so
/// the two cannot drift: a row is listed exactly when every word is in some
/// part, and it names a place exactly for a word no shown part holds.
#[derive(Debug, Clone, Default)]
pub(super) struct Haystack {
    /// The shown parts, one per line - a word never spans two, because a word
    /// holds no line break.
    shown: String,
    /// The parts the row does not show, in the order of [`MatchPlace`], so the
    /// first that holds a word is the place the row names.
    hidden: Vec<(MatchPlace, String)>,
}

impl Haystack {
    pub(super) fn new<'a>(
        shown: &[&str],
        hidden: impl IntoIterator<Item = (MatchPlace, &'a str)>,
    ) -> Self {
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
    pub(super) fn matches(&self, words: &[String]) -> bool {
        words.iter().all(|word| self.holds(word))
    }

    /// The places a row names: for every word no shown part holds, the first
    /// hidden part that does. Empty when the row shows every word. One per
    /// such word, in the query's order - the words of `i18n` name each place
    /// once and in one order, so that is done in one place.
    pub(super) fn places(&self, words: &[String]) -> Vec<MatchPlace> {
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
pub(super) fn words_of(query: &str) -> Vec<String> {
    query
        .to_lowercase()
        .split_whitespace()
        .map(str::to_owned)
        .collect()
}
