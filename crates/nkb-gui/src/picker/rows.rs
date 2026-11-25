//! One pack of the catalogue as a row of the value window: its words, its pill
//! and what a search finds it by. Out of `picker.rs`, which stands at the
//! ceiling of `D113`.

use nkb_adapters::i18n::{self, MatchPlace, PacksLabel};
use nkb_app::browse_packs::{Listing, PackEntry};
use nkb_app::ports::SourceError;
use nkb_core::calendar::Day;
use nkb_core::pack::{Pack, PackValue, Risk};
use nkb_core::preview::preview_of;
use nkb_core::text::{SHOWN_DESCRIPTION, SHOWN_NAME, shown_prose};

use super::{Badge, Empty, Haystack, PackChoice, ValueChoice};
use crate::live::preview_line;

/// The packs as the window offers them, made from what `list_packs` found:
/// the part of a list that does not depend on the pack in use, and the part
/// that takes time - a preview of every value. Made where the packs are read,
/// so the main thread only puts the list together (`packs/reading.rs`).
pub struct PackChoices {
    pub(super) packs: Vec<PackChoice>,
    pub(super) empty: Option<Empty>,
}

impl PackChoices {
    /// The choices over `listing`, the previews of `today`.
    #[must_use]
    pub fn of(listing: Result<Listing, SourceError>, today: Day) -> Self {
        match listing {
            Ok(listing) => {
                let packs: Vec<PackChoice> = listing
                    .entries
                    .iter()
                    .map(|entry| choice_of(entry, today))
                    .collect();
                let empty = packs.is_empty().then_some(Empty::NoPacks);
                Self { packs, empty }
            }
            Err(_) => Self {
                packs: Vec::new(),
                empty: Some(Empty::Unavailable),
            },
        }
    }
}

/// A row of the list. `today` is the day a `next-date` value names in its
/// preview - the date the window shows is the date a press would type.
///
/// Since packs come from folders (`D173`), every word of a pack is shown
/// through [`shown_prose`]: escaped, so a direction override in a stranger's
/// name cannot reorder the row, and cut, so a description of megabytes - which
/// `nkb lint` accepts - cannot freeze the window. A pack from a folder says so
/// before its description, and what it is used instead of.
fn choice_of(entry: &PackEntry, today: Day) -> PackChoice {
    let source = i18n::pack_source(entry.origin());
    let with_source = |detail: String| match &source {
        Some(source) => format!("{source} {detail}"),
        None => detail,
    };
    match entry {
        PackEntry::Loaded { pack, origin, .. } => PackChoice {
            id: pack.id.clone(),
            title: shown_prose(&pack.name, SHOWN_NAME),
            detail: with_source(shown_prose(&pack.description, SHOWN_DESCRIPTION)),
            // The risk outranks where the pack is from: one pill per row.
            badge: if is_offensive(pack) {
                Some(Badge {
                    text: i18n::packs_label(PacksLabel::Offensive).to_owned(),
                    risky: true,
                    current: false,
                })
            } else {
                i18n::source_badge(origin).map(|text| Badge {
                    text: text.to_owned(),
                    risky: false,
                    current: false,
                })
            },
            enabled: true,
            haystack: haystack_of(pack),
            values: pack
                .values
                .iter()
                .map(|value| ValueChoice {
                    id: value.id.clone(),
                    name: shown_prose(&value.name, SHOWN_NAME),
                    preview: preview_line(&preview_of(&value.definition.on(today))).0,
                    offensive: pack.risk_of(value) == Risk::Offensive,
                    halves: pack.halves_of(&value.id).map(|(_, half)| half).collect(),
                    haystack: value_haystack_of(pack, value),
                })
                .collect(),
        },
        PackEntry::Refused { id, errors, .. } => PackChoice {
            id: id.clone(),
            title: id.clone(),
            detail: with_source(i18n::pack_refused(id)),
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
            detail: with_source(i18n::pack_unreadable(id)),
            badge: None,
            enabled: false,
            haystack: Haystack::new(&[id], []),
            values: Vec::new(),
        },
    }
}

/// The one pill of a value row: "next" over "offensive" - the palette's next
/// band already marks the risk of the next value.
pub(super) fn value_badge(choice: &ValueChoice, is_next: bool) -> Option<Badge> {
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

/// The value's own name, which its row shows, and its id, tags and fields and
/// the name and id of each pair it is a half of, which it does not - a pair's
/// name is the test the two values make together (`D134`).
fn value_haystack_of(pack: &Pack, value: &PackValue) -> Haystack {
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
            )
            .chain(pack.halves_of(&value.id).flat_map(|(pair, _)| {
                [
                    (MatchPlace::Pair, pair.name.as_str()),
                    (MatchPlace::Pair, pair.id.as_str()),
                ]
            })),
    )
}
