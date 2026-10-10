//! The words of the three windows besides the palette: the value window, the
//! shortcuts window and the welcome window. A part of the `i18n` module, kept
//! in a file of its own so that a window's words are found in one place - see
//! the module above for why every one of them is a key.

use super::*;

/// The words of the pack window (`ux-spec.md` 5.2 and 6 K).
///
/// Its own enum rather than more `PaletteLabel` variants: the pack window is a
/// second window with its own words, and a label is found by the window it
/// stands in. Like `PaletteLabel`, it lives here and not in `app`, because only
/// a window needs to know that a pack is listed on two lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PacksLabel {
    /// The window's title, which the system shows in the task switcher.
    Title,
    /// The heading above the list.
    Heading,
    /// How many values and packs the list shows, with nothing typed.
    Summary,
    /// The same, while a query hides some of them - so a short list is never
    /// mistaken for a small catalogue.
    SummaryFiltered,
    /// The label above the query line, saying what it searches.
    Search,
    /// The pill on the pack the palette holds now.
    InUse,
    /// The pill on the value the next press sends (`UX-GUI-001`).
    Next,
    /// Beside a pack a query found in a part its row does not show - its id or
    /// its tags (`UX-GUI-008`). Since the owner's points 5 and 6 of 2026-10-07
    /// the row shows the name and the whole description and nothing else, so
    /// this stands apart from the description, at the end of the name's line.
    FoundIn,
    /// Beside a value listed away from its pack - found by a query, or used
    /// last: the pack it is in, and the parts of it the query was found in
    /// that its row does not show (`UX-GUI-008`). Without the places a value
    /// found by its id looked found at random. With none, the pack's name
    /// alone stands there, and it needs no pattern.
    ValueFound,
    /// The parts the two above name, one word each.
    PlaceId,
    PlaceTags,
    PlaceFields,
    /// Two places and three, as one phrase - whole shapes, so a translation
    /// is never built from an English joining rule.
    PlacesTwo,
    PlacesThree,
    /// The section heading over the values a query found, in every pack.
    FoundValues,
    /// The heading that folds the values the tester sent or chose last, in
    /// packs other than the one in use (`UX-GUI-016`), with how many - folded
    /// at every opening (the owner's choice, 2026-10-07), so the number says
    /// what the fold holds.
    Recent,
    /// The section heading over the packs.
    PacksSection,
    /// Said in the palette when the value chosen in the window is not in the
    /// pack the palette holds - the window read the catalogue on its own.
    ValueGone,
    /// The pill on a pack with any offensive value (`product-spec.md` 10.2).
    Offensive,
    /// The second line of a pack that is present and refused.
    Refused,
    /// The pill on it: how many problems block it.
    Problems,
    /// The second line of a pack whose file would not open.
    Unreadable,
    /// The list is empty because nothing matches the query.
    NoMatch,
    /// The catalogue holds no pack at all.
    NoPacks,
    /// The catalogue itself could not be read.
    ListUnavailable,
    /// Which sources the list did not read, and why. 🔴 Said on every opening
    /// while a source is unread: a list from one source of three looks exactly
    /// like a complete one (untouchable rule 1, the same rule `nkb packs` keeps).
    NotRead,
    /// Said instead of `NotRead` when the version simply has no team and own
    /// folders and the built-in packs were read (`UX-GUI-009`).
    BuiltInOnly,
    /// The words for the sources, inside `NotRead`.
    SourceBuiltIn,
    SourceTeam,
    SourceOwn,
    /// The reasons, inside `NotRead`.
    ReasonNotCarried,
    ReasonCannotBeSet,
    ReasonNotSet,
    ReasonUnreadable,
    /// The footer: each key, and what it does here.
    KeyEnter,
    UseSelected,
    KeyEscape,
    Close,
    KeyArrows,
    Move,
    /// The keys that open and fold a pack's values (the owner's point 4).
    KeyFold,
    Fold,
}

pub(super) fn pattern_packs_label(label: PacksLabel) -> &'static str {
    match label {
        PacksLabel::Title => "Naughty Keyboard - find a value",
        PacksLabel::Heading => "Find a value",
        PacksLabel::Summary => "values: {values}, packs: {packs}",
        PacksLabel::SummaryFiltered => {
            "values: {values} of {all_values}, packs: {packs} of {all_packs}"
        }
        PacksLabel::Search => "Search values and packs by name, tag or id",
        PacksLabel::InUse => "in use",
        PacksLabel::Next => "next",
        PacksLabel::FoundIn => "in its {places}",
        PacksLabel::ValueFound => "{pack} - in its {places}",
        PacksLabel::PlaceId => "id",
        PacksLabel::PlaceTags => "tags",
        PacksLabel::PlaceFields => "fields",
        PacksLabel::PlacesTwo => "{first} and {second}",
        PacksLabel::PlacesThree => "{first}, {second} and {third}",
        PacksLabel::FoundValues => "Values",
        PacksLabel::Recent => "Recent ({count})",
        PacksLabel::PacksSection => "Packs",
        PacksLabel::ValueGone => {
            "Value \"{value}\" is not in {pack} any more, so the next value did not change."
        }
        PacksLabel::Offensive => "offensive",
        PacksLabel::Refused => "{id}, does not load",
        PacksLabel::Problems => "problems: {count}",
        PacksLabel::Unreadable => "{id}, cannot be read",
        PacksLabel::NoMatch => "Nothing matches \"{query}\". Press Backspace to widen the search.",
        PacksLabel::NoPacks => {
            "No pack was found, so there is nothing to choose from. The note below says which sources were read."
        }
        PacksLabel::ListUnavailable => {
            "The pack list could not be read, so there is nothing to choose from. Close this window and start the palette with a pack name."
        }
        PacksLabel::NotRead => "Not read: {sources} - {reason}.",
        PacksLabel::BuiltInOnly => {
            "This version lists the built-in packs only - team and own pack folders are not in it yet."
        }
        PacksLabel::SourceBuiltIn => "built-in packs",
        PacksLabel::SourceTeam => "team folder",
        PacksLabel::SourceOwn => "own folder",
        PacksLabel::ReasonNotCarried => "not carried by this build",
        PacksLabel::ReasonCannotBeSet => "cannot be set in this version",
        PacksLabel::ReasonNotSet => "no folder is set",
        PacksLabel::ReasonUnreadable => "the folder could not be read",
        PacksLabel::KeyEnter => "Enter",
        PacksLabel::UseSelected => "Use the selected value or pack",
        PacksLabel::KeyEscape => "Esc",
        PacksLabel::Close => "Close",
        PacksLabel::KeyArrows => "↑ ↓",
        PacksLabel::Move => "Move through the list",
        PacksLabel::KeyFold => "← →",
        PacksLabel::Fold => "Show or hide values",
    }
}

/// A pack window label that carries nothing, ready to show.
///
/// The same division as [`label`]: a pattern with a placeholder has a typed
/// function below, and the test at the bottom refuses one reached from here.
#[must_use]
pub fn packs_label(label: PacksLabel) -> &'static str {
    pattern_packs_label(label)
}

/// How many values and packs the list shows.
#[must_use]
pub fn packs_summary(values: usize, packs: usize) -> String {
    fill(
        pattern_packs_label(PacksLabel::Summary),
        &[
            ("values", &values.to_string()),
            ("packs", &packs.to_string()),
        ],
    )
}

/// The same while a query hides some: of how many in the whole catalogue.
#[must_use]
pub fn packs_summary_filtered(
    values: usize,
    all_values: usize,
    packs: usize,
    all_packs: usize,
) -> String {
    fill(
        pattern_packs_label(PacksLabel::SummaryFiltered),
        &[
            ("values", &values.to_string()),
            ("all_values", &all_values.to_string()),
            ("packs", &packs.to_string()),
            ("all_packs", &all_packs.to_string()),
        ],
    )
}

/// Where a query word was found, among the parts of a pack or a value that its
/// row does not show (`UX-GUI-008`), in the order a row names them.
///
/// Here and not in the window's code, for the reason `PacksLabel` gives: only
/// a window needs to know which parts of a row are on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MatchPlace {
    /// A value's id, or a pack's - since 2026-10-07 the pack row shows its
    /// name and description only (the owner's point 6).
    Id,
    Tags,
    /// The field kinds a value narrows its pack's list to.
    Fields,
}

/// The places as one phrase: `id`, `id and tags`, `id, tags and fields`. In
/// the order of [`MatchPlace`] and each once, however the caller listed them.
fn places(found: &[MatchPlace]) -> String {
    let mut found = found.to_vec();
    found.sort_unstable();
    found.dedup();
    let word = |place: &MatchPlace| {
        pattern_packs_label(match place {
            MatchPlace::Id => PacksLabel::PlaceId,
            MatchPlace::Tags => PacksLabel::PlaceTags,
            MatchPlace::Fields => PacksLabel::PlaceFields,
        })
    };
    match found.as_slice() {
        [] => String::new(),
        [one] => word(one).to_owned(),
        [first, second] => fill(
            pattern_packs_label(PacksLabel::PlacesTwo),
            &[("first", word(first)), ("second", word(second))],
        ),
        // Three at most: `MatchPlace` has three, each kept once.
        [first, second, third, ..] => fill(
            pattern_packs_label(PacksLabel::PlacesThree),
            &[
                ("first", word(first)),
                ("second", word(second)),
                ("third", word(third)),
            ],
        ),
    }
}

/// Beside a pack a query found in a part its row does not show: `in its tags`.
/// `None` when the row shows every word of the query.
#[must_use]
pub fn found_in(found: &[MatchPlace]) -> Option<String> {
    (!found.is_empty()).then(|| {
        fill(
            pattern_packs_label(PacksLabel::FoundIn),
            &[("places", &places(found))],
        )
    })
}

/// Beside a value listed away from its pack: the pack's name, and with
/// `found` the parts of the value a query was found in that its row does not
/// show - `Whitespace - in its id`.
#[must_use]
pub fn value_aside(pack: &str, found: &[MatchPlace]) -> String {
    if found.is_empty() {
        return pack.to_owned();
    }
    fill(
        pattern_packs_label(PacksLabel::ValueFound),
        &[("pack", pack), ("places", &places(found))],
    )
}

/// The heading that folds the values used last, with how many it holds.
#[must_use]
pub fn recent_heading(count: usize) -> String {
    fill(
        pattern_packs_label(PacksLabel::Recent),
        &[("count", &count.to_string())],
    )
}

/// Said when the value chosen in the window is not in the pack in use.
#[must_use]
pub fn value_gone(value: &str, pack: &str) -> String {
    fill(
        pattern_packs_label(PacksLabel::ValueGone),
        &[("value", value), ("pack", pack)],
    )
}

/// The second line of a pack that is present and refused.
#[must_use]
pub fn pack_refused(id: &str) -> String {
    fill(pattern_packs_label(PacksLabel::Refused), &[("id", id)])
}

/// The pill on a refused pack.
#[must_use]
pub fn pack_problems(count: usize) -> String {
    fill(
        pattern_packs_label(PacksLabel::Problems),
        &[("count", &count.to_string())],
    )
}

/// The second line of a pack whose file would not open.
#[must_use]
pub fn pack_unreadable(id: &str) -> String {
    fill(pattern_packs_label(PacksLabel::Unreadable), &[("id", id)])
}

/// What the list says when nothing matches the query.
///
/// The query is copied as typed and never substituted again - a query holding
/// braces is text, not a pattern.
#[must_use]
pub fn no_match(query: &str) -> String {
    fill(
        pattern_packs_label(PacksLabel::NoMatch),
        &[("query", query)],
    )
}

/// One line per reason a source went unread, naming the sources it covers.
///
/// Empty when every source was read. Grouped by reason, so the build today says
/// one line for the team and own folders rather than two lines that differ in
/// one word.
///
/// `UX-GUI-009`: when the only thing missing is that this version HAS no team
/// and own folders, while the built-in packs were read, the line says so in
/// the tester's words (`PacksLabel::BuiltInOnly`). "Not read: team folder, own
/// folder" stood under the list on every opening and read like a failure - and
/// a note that cries wolf teaches the tester to skip the one that will one day
/// name a folder that really could not be read. Every other case keeps "Not
/// read" (untouchable rule 1 asks for the truth, not for alarm).
#[must_use]
pub fn not_read(coverage: &CatalogueCoverage) -> Vec<String> {
    let mut groups: Vec<(PacksLabel, Vec<&'static str>)> = Vec::new();
    for (source, skipped) in &coverage.skipped {
        let reason = match (skipped, source) {
            (SourceSkipped::NotImplementedYet, CatalogueSource::BuiltIn) => {
                PacksLabel::ReasonNotCarried
            }
            (SourceSkipped::NotImplementedYet, CatalogueSource::Team | CatalogueSource::Own) => {
                PacksLabel::ReasonCannotBeSet
            }
            (SourceSkipped::NotConfigured, _) => PacksLabel::ReasonNotSet,
            (SourceSkipped::Unreadable, _) => PacksLabel::ReasonUnreadable,
        };
        let name = pattern_packs_label(match source {
            CatalogueSource::BuiltIn => PacksLabel::SourceBuiltIn,
            CatalogueSource::Team => PacksLabel::SourceTeam,
            CatalogueSource::Own => PacksLabel::SourceOwn,
        });
        match groups.iter_mut().find(|(known, _)| *known == reason) {
            Some((_, names)) => names.push(name),
            None => groups.push((reason, vec![name])),
        }
    }
    // Both folders, and only for the reason that they do not exist yet - the
    // group holds nothing else - with the built-in packs read.
    let built_in_read = coverage.consulted.contains(&CatalogueSource::BuiltIn);
    groups
        .into_iter()
        .map(|(reason, names)| {
            if reason == PacksLabel::ReasonCannotBeSet && names.len() == 2 && built_in_read {
                return pattern_packs_label(PacksLabel::BuiltInOnly).to_owned();
            }
            fill(
                pattern_packs_label(PacksLabel::NotRead),
                &[
                    ("sources", &names.join(", ")),
                    ("reason", pattern_packs_label(reason)),
                ],
            )
        })
        .collect()
}

// ---------------------------------------------------------------------------
// The shortcuts window - its words, and what a change came to
// ---------------------------------------------------------------------------

/// The words of the shortcuts window (`ux-spec.md` 5.4 and 6 L).
///
/// A third window with its own words, for the reason `PacksLabel` gives. The
/// sentences about a change are keys here too, rather than arms matched on
/// `ShortcutChange` and `Refusal`: those two types already key the sentences
/// the palette says about the settings file (`ux-spec.md` 6 J), and one key
/// may name one sentence. The completeness the compiler guards is kept all
/// the same - [`shortcut_answer`] matches `ShortcutChange` and `Refusal`
/// exhaustively, so a new variant does not compile without a sentence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutsLabel {
    /// The window's title, which the system shows in the task switcher.
    Title,
    /// The heading above the list.
    Heading,
    /// How many shortcuts are the tester's own.
    Summary,
    /// What the window does, and that the palette does not answer meanwhile.
    Intro,
    /// The pill on the row being recorded.
    Recording,
    /// The pill on a shortcut that is not the default.
    Changed,
    /// The pill on a shortcut another application held at the last
    /// registration.
    Taken,
    /// The pill on a shortcut the system refused for another reason.
    NotRegistered,
    /// The pill on an action this version does not carry out yet - its
    /// shortcut stays registered, so nobody else takes it (`UX-GUI-006`).
    NotAvailable,
    /// The footer: each key, and what it does here.
    KeyEnter,
    Record,
    KeyDelete,
    Restore,
    KeyArrows,
    Move,
    KeyEscape,
    Close,
    /// Recording has started: what to press.
    Invite,
    /// A recorded shortcut was kept.
    Recorded,
    /// The default was given back.
    Restored,
    /// The action answers the shortcut already.
    AlreadySo,
    /// Another action moved onto the shortcut the settings file wishes for it.
    Also,
    /// Kept, but whether another application holds it could not be asked.
    Unchecked,
    /// Refused: no `Ctrl`, `Alt` or `Win`.
    NoModifier,
    /// Refused: a keyboard layout types a character on it as `AltGr`.
    TypesCharacter,
    /// Refused: another action answers it.
    HeldBy,
    /// Refused: another application holds it.
    TakenElsewhere,
    /// Refused by the system for another reason.
    NotRegisteredCode,
    /// The key pressed cannot end a shortcut.
    NotAKey,
    /// Two keys of the vocabulary were held.
    SeveralKeys,
    /// This system cannot tell which key was pressed.
    CannotTell,
}

/// The words of the welcome window (`ux-spec.md` 5.1 and 6 N, UX7, `D105`).
///
/// A fourth window with its own words, for the reason `PacksLabel` gives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WelcomeLabel {
    /// The window's title, which the system shows in the task switcher.
    Title,
    /// The heading in the window's header.
    Heading,
    /// The first step's heading.
    TryHeading,
    /// The first step: `ux-spec.md` 5.1's own sentence, with the shortcut in
    /// effect.
    TryIt,
    /// What UI Automation calls the box.
    BoxName,
    /// Under the box while nothing has arrived in it.
    Empty,
    /// The second step's heading.
    DoesHeading,
    /// The second step: what the tool does to other windows and what it reads
    /// (`product-spec.md` 10.3).
    Does,
    /// `product-spec.md` 10.2 point 3, said once, here.
    Allowed,
    /// The third step's heading.
    ReadyHeading,
    /// The third step, naming the pack the palette is on.
    Ready,
    /// The same, before the palette has said which pack it is on.
    ReadyNoPack,
    /// The window's main action.
    Start,
    /// What it does, for UI Automation.
    StartAction,
}

fn pattern_welcome_label(label: WelcomeLabel) -> &'static str {
    match label {
        WelcomeLabel::Title => "Naughty Keyboard - welcome",
        WelcomeLabel::Heading => "Welcome to Naughty Keyboard",
        WelcomeLabel::TryHeading => "Try it here first",
        WelcomeLabel::TryIt => "Click the box below and press {shortcut}.",
        WelcomeLabel::BoxName => "The box for the first try",
        WelcomeLabel::Empty => "Nothing has arrived yet.",
        WelcomeLabel::DoesHeading => "What it does",
        WelcomeLabel::Does => {
            "Naughty Keyboard types into the window in front, the way a keyboard does. It does not read what the window shows - only the name of its program and the kind of control that has the keyboard."
        }
        WelcomeLabel::Allowed => "Use it only on systems you are allowed to test.",
        WelcomeLabel::ReadyHeading => "Ready",
        WelcomeLabel::Ready => {
            "Start testing opens the palette: a small window that stays on top and shows what each press sends. It starts on the pack {pack}. Press {shortcut} to choose another pack or value."
        }
        WelcomeLabel::ReadyNoPack => {
            "Start testing opens the palette: a small window that stays on top and shows what each press sends. Press {shortcut} to choose a pack or a value."
        }
        WelcomeLabel::Start => "Start testing",
        WelcomeLabel::StartAction => "Close the welcome and start testing",
    }
}

/// A welcome window label that carries nothing, ready to show. A pattern
/// with a placeholder has a typed function below.
#[must_use]
pub fn welcome_label(label: WelcomeLabel) -> &'static str {
    pattern_welcome_label(label)
}

/// The first step, naming the shortcut that sends the next value.
#[must_use]
pub fn welcome_try_it(next_value: HotkeyChord) -> String {
    fill(
        pattern_welcome_label(WelcomeLabel::TryIt),
        &[("shortcut", &chord(next_value))],
    )
}

/// The third step: the pack the palette is on, when it is known, and the
/// shortcut of the value window.
#[must_use]
pub fn welcome_ready(pack: Option<&str>, open_packs: HotkeyChord) -> String {
    let shortcut = chord(open_packs);
    match pack {
        Some(pack) => fill(
            pattern_welcome_label(WelcomeLabel::Ready),
            &[("pack", pack), ("shortcut", &shortcut)],
        ),
        None => fill(
            pattern_welcome_label(WelcomeLabel::ReadyNoPack),
            &[("shortcut", &shortcut)],
        ),
    }
}

pub(super) fn pattern_shortcuts_label(label: ShortcutsLabel) -> &'static str {
    match label {
        ShortcutsLabel::Title => "Naughty Keyboard - shortcuts",
        ShortcutsLabel::Heading => "Shortcuts",
        ShortcutsLabel::Summary => "changed: {count} of {total}",
        ShortcutsLabel::Intro => {
            "The palette answers these in every application. They are paused while this window is open, and a change takes effect when it closes."
        }
        ShortcutsLabel::Recording => "recording",
        ShortcutsLabel::Changed => "changed",
        ShortcutsLabel::Taken => "taken",
        ShortcutsLabel::NotRegistered => "not registered",
        ShortcutsLabel::NotAvailable => "not available yet",
        ShortcutsLabel::KeyEnter => "Enter",
        ShortcutsLabel::Record => "Record a new shortcut",
        ShortcutsLabel::KeyDelete => "Delete",
        ShortcutsLabel::Restore => "Restore the default",
        ShortcutsLabel::KeyArrows => "↑ ↓",
        ShortcutsLabel::Move => "Move through the list",
        ShortcutsLabel::KeyEscape => "Esc",
        ShortcutsLabel::Close => "Close",
        ShortcutsLabel::Invite => {
            "Press the new shortcut for \"{action}\", or Esc to stop recording."
        }
        ShortcutsLabel::Recorded => "\"{action}\" answers {shortcut} from when this window closes.",
        ShortcutsLabel::Restored => {
            "\"{action}\" answers its default {shortcut} again from when this window closes."
        }
        ShortcutsLabel::AlreadySo => "\"{action}\" already answers {shortcut}.",
        ShortcutsLabel::Also => "\"{action}\" moves to {shortcut}, as the settings file asks.",
        ShortcutsLabel::Unchecked => {
            "Whether another application holds {shortcut} could not be checked: {reason}."
        }
        ShortcutsLabel::NoModifier => {
            "{shortcut} holds no Ctrl, Alt or Win - taken globally it would stop that key working in every application. Add Ctrl, Alt or Win to it."
        }
        ShortcutsLabel::TypesCharacter => {
            "{shortcut} types \"{character}\" as AltGr on a keyboard layout of this computer - taken globally it would stop that character working in every application. Choose a combination without Ctrl+Alt."
        }
        ShortcutsLabel::HeldBy => {
            "{shortcut} is already the shortcut for \"{other}\". Give \"{other}\" another combination first."
        }
        ShortcutsLabel::TakenElsewhere => {
            "{shortcut} is taken by another application, so it was not kept. Close that application or choose another combination."
        }
        ShortcutsLabel::NotRegisteredCode => {
            "{shortcut} could not be registered - the system returned code {code} - so it was not kept."
        }
        ShortcutsLabel::NotAKey => {
            "That key cannot end a shortcut. End it with a letter, a digit, F1 to F24 or Space, or press Esc to stop recording."
        }
        ShortcutsLabel::SeveralKeys => {
            "More than one key is held. Hold the modifiers and press a single key."
        }
        ShortcutsLabel::CannotTell => {
            "This system cannot tell which key was pressed, so a shortcut cannot be recorded here yet."
        }
    }
}

/// A shortcuts window label that carries nothing, ready to show.
///
/// The same division as [`label`]: a pattern with a placeholder has a typed
/// function below, and the test at the bottom refuses one reached from here.
#[must_use]
pub fn shortcuts_label(label: ShortcutsLabel) -> &'static str {
    pattern_shortcuts_label(label)
}

/// How many shortcuts are the tester's own.
#[must_use]
pub fn shortcuts_summary(changed: usize, total: usize) -> String {
    fill(
        pattern_shortcuts_label(ShortcutsLabel::Summary),
        &[
            ("count", &changed.to_string()),
            ("total", &total.to_string()),
        ],
    )
}

/// What to press, once recording has started for `action`.
#[must_use]
pub fn shortcut_invite(action: HotkeyAction) -> String {
    fill(
        pattern_shortcuts_label(ShortcutsLabel::Invite),
        &[("action", action_name(action))],
    )
}

/// What changing `action` came to, a line per fact.
///
/// `asked` is what was asked for - a chord recorded, or `None` for the
/// default back - and `answers` the chord the action answers now, which the
/// window knows and `AlreadySo` does not carry.
///
/// Matches `ShortcutChange` and `Refusal` exhaustively, so a new answer does
/// not compile without its sentence (the reason keys are types, see the top
/// of this file).
#[must_use]
pub fn shortcut_answer(
    action: HotkeyAction,
    asked: Option<HotkeyChord>,
    change: &ShortcutChange,
    answers: HotkeyChord,
) -> Vec<String> {
    let name = action_name(action);
    let about = |label: ShortcutsLabel, held: HotkeyChord| {
        fill(
            pattern_shortcuts_label(label),
            &[("action", name), ("shortcut", &chord(held))],
        )
    };
    match change {
        ShortcutChange::AlreadySo => vec![about(ShortcutsLabel::AlreadySo, answers)],
        ShortcutChange::Changed {
            bindings,
            also,
            unchecked,
        } => {
            let now = bindings.chord(action);
            let kept = if asked.is_some() {
                ShortcutsLabel::Recorded
            } else {
                ShortcutsLabel::Restored
            };
            let mut lines = vec![about(kept, now)];
            lines.extend(also.iter().map(|other| {
                fill(
                    pattern_shortcuts_label(ShortcutsLabel::Also),
                    &[
                        ("action", action_name(*other)),
                        ("shortcut", &chord(bindings.chord(*other))),
                    ],
                )
            }));
            if let Some(why) = unchecked {
                lines.push(fill(
                    pattern_shortcuts_label(ShortcutsLabel::Unchecked),
                    &[("shortcut", &chord(now)), ("reason", &why.to_string())],
                ));
            }
            lines
        }
        ShortcutChange::Refused(refused) => {
            let (label, other, character) = match refused.why {
                Refusal::Problem(ChordProblem::NoCommandModifier) => {
                    (ShortcutsLabel::NoModifier, "", String::new())
                }
                Refusal::TypesCharacter(character) => (
                    ShortcutsLabel::TypesCharacter,
                    "",
                    shown_character(character),
                ),
                Refusal::SameAs(other) => {
                    (ShortcutsLabel::HeldBy, action_name(other), String::new())
                }
            };
            vec![fill(
                pattern_shortcuts_label(label),
                &[
                    ("shortcut", &chord(refused.chord)),
                    ("other", other),
                    ("character", &character),
                ],
            )]
        }
        ShortcutChange::Taken { chord: held } => vec![fill(
            pattern_shortcuts_label(ShortcutsLabel::TakenElsewhere),
            &[("shortcut", &chord(*held))],
        )],
        ShortcutChange::NotRegistered { chord: held, code } => vec![fill(
            pattern_shortcuts_label(ShortcutsLabel::NotRegisteredCode),
            &[("shortcut", &chord(*held)), ("code", &code.to_string())],
        )],
    }
}
