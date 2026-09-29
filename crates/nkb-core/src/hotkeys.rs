//! The global shortcuts the palette listens for, as data.
//!
//! `ux-spec.md` 3 fixes ten global actions and a default combination for each.
//! Those combinations are DATA, in one place, the same way the clearing recipe
//! in [`crate::keys`] is - so that the palette, the registrar and the hint bar
//! all read one source rather than three that drift.
//!
//! # Platform-neutral on purpose
//!
//! A key here is a LOGICAL key, not an operating-system virtual-key code. The
//! mapping to a `VK_*` (or a macOS key code) belongs to the layer that talks to
//! the system, exactly as `chord_for` in the keyboard adapter maps the clearing
//! keys. This module knows nothing of any system, which is what keeps it in the
//! core.
//!
//! # Why the family is `Alt+Shift` and not `Ctrl+Alt`
//!
//! Until `D82` every default was `Ctrl+Alt+<key>`, as `ux-spec.md` 3 first
//! proposed. On Windows `Ctrl+Alt` IS `AltGr`, and a global shortcut matches the
//! modifiers exactly, so `Ctrl+Alt+N` swallowed `AltGr+N` - the letter `ń` on the
//! Polish layout - in every application for as long as the palette ran. Measured
//! on the owner's machine (layout `0415`) with the system's own layout table:
//! `Ctrl+Alt` types a character on ten letters there, and `Ctrl+Alt+Space` was
//! held by another application. Microsoft's keyboard guidelines say the same in
//! one line: do not use `Ctrl+Alt` combinations.
//!
//! `Alt+Shift` cannot collide with `AltGr` on ANY layout, because `AltGr` adds
//! `Ctrl` and the combination then no longer matches. Measured free of other
//! applications on the same machine. What it still costs is named in `D82`.
//!
//! # Two conventions, both as data
//!
//! macOS keeps `Cmd+Alt+<key>` (`ux-spec.md` 3): there `Option` types
//! characters and `Command` does not. Until `D82` the macOS table did not exist
//! and the registering adapter swapped `ctrl` for the Command bit on every
//! chord. That worked for a table whose every entry held `ctrl`, and it would
//! have rewritten a chord the tester chose. So both tables live here, and the
//! layer that knows the system picks one - this crate has no `cfg` and keeps
//! none.

/// One of the ten global actions the palette answers to.
///
/// Only a subset is wired to the sequence in the first pass of step 3 - moving
/// through a pack. The rest name shortcuts the later steps fill in (marking a
/// result, copying the report block, opening the pack search), and they are
/// here now so the combinations can be RESERVED at registration rather than
/// left free to be taken by something else in the meantime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HotkeyAction {
    /// Clear the field and send the next value in the pack.
    NextValue,
    /// Send the previous value.
    PreviousValue,
    /// Send the current value again.
    RepeatLast,
    /// Return to the first value of the pack.
    RestartPack,
    /// Copy the bug-report block for the last value to the clipboard.
    CopyReport,
    /// Mark the last result as working.
    MarkOk,
    /// Mark the last result as a problem.
    MarkProblem,
    /// Mark the last result as suspect.
    MarkSuspect,
    /// Open the pack search.
    OpenPacks,
    /// Collapse the palette to its pack band, or expand it again - never
    /// `hide()`, `OBS-80`. The tester's choice, never a timer's (`D83`).
    ToggleVisibility,
}

impl HotkeyAction {
    /// Every action, so a test can state the size of the set and check that the
    /// default table covers it. Rust has no way to enumerate variants, so this
    /// list is the source of truth and the test guards that it stays complete.
    pub const ALL: [HotkeyAction; 10] = [
        HotkeyAction::NextValue,
        HotkeyAction::PreviousValue,
        HotkeyAction::RepeatLast,
        HotkeyAction::RestartPack,
        HotkeyAction::CopyReport,
        HotkeyAction::MarkOk,
        HotkeyAction::MarkProblem,
        HotkeyAction::MarkSuspect,
        HotkeyAction::OpenPacks,
        HotkeyAction::ToggleVisibility,
    ];

    /// The action's name where a tester writes it - a key of the `[shortcuts]`
    /// table in the settings file (`settings-format.md` 2).
    ///
    /// A public name from the day it is written into somebody's file
    /// (untouchable rule 3): renaming one is a change of the settings schema,
    /// not a tidy-up. English, lower case, words joined by `-`, like a pack
    /// identifier.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            HotkeyAction::NextValue => "next-value",
            HotkeyAction::PreviousValue => "previous-value",
            HotkeyAction::RepeatLast => "repeat-last",
            HotkeyAction::RestartPack => "restart-pack",
            HotkeyAction::CopyReport => "copy-report",
            HotkeyAction::MarkOk => "mark-ok",
            HotkeyAction::MarkProblem => "mark-problem",
            HotkeyAction::MarkSuspect => "mark-suspect",
            HotkeyAction::OpenPacks => "open-packs",
            HotkeyAction::ToggleVisibility => "toggle-visibility",
        }
    }

    /// The action an [`HotkeyAction::id`] names, written exactly - a file key
    /// is not guessed at.
    #[must_use]
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|action| action.id() == id)
    }
}

/// Declares [`HotkeyKey`] from ONE table: the variant and the name a tester
/// writes for it. The enum, the list of every key and the name cannot disagree,
/// because there is nothing to keep in step by hand.
macro_rules! hotkey_keys {
    ($(#[$meta:meta])* $($variant:ident => $name:literal),+ $(,)?) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum HotkeyKey {
            $($variant),+
        }

        impl HotkeyKey {
            /// Every key a shortcut can end in, in the order of the table.
            pub const ALL: &'static [HotkeyKey] = &[$(HotkeyKey::$variant),+];

            /// The name a tester reads and writes: `N`, `0`, `F5`, `Space`.
            #[must_use]
            pub const fn name(self) -> &'static str {
                match self {
                    $(HotkeyKey::$variant => $name),+
                }
            }
        }
    };
}

hotkey_keys! {
    /// A logical key a shortcut ends in.
    ///
    /// Widened for configurable shortcuts (`K4`, 2026-09-29) from the ten keys
    /// the defaults use - and widened on purpose, not to "every key": letters,
    /// digits, function keys and Space. What stays out, and why:
    ///
    /// - punctuation - which character a key types, and on Windows even which
    ///   key code it sends, depends on the layout, so a name in the settings
    ///   file would mean different keys on different machines.
    /// - the number pad - its keys change meaning with Num Lock.
    /// - the editing and moving keys (arrows, Home, End, Page Up and Down,
    ///   Insert, Delete, Backspace, Tab, Enter) - taken globally they break the
    ///   very field under test.
    /// - Escape - `ux-spec.md` 3 keeps it the one key the tool never takes
    ///   globally.
    ///
    /// Adding a key is one line here and one in the layer that maps keys to the
    /// system, whose match is exhaustive, so the compiler names the second.
    A => "A", B => "B", C => "C", D => "D", E => "E", F => "F", G => "G",
    H => "H", I => "I", J => "J", K => "K", L => "L", M => "M", N => "N",
    O => "O", P => "P", Q => "Q", R => "R", S => "S", T => "T", U => "U",
    V => "V", W => "W", X => "X", Y => "Y", Z => "Z",
    Digit0 => "0", Digit1 => "1", Digit2 => "2", Digit3 => "3", Digit4 => "4",
    Digit5 => "5", Digit6 => "6", Digit7 => "7", Digit8 => "8", Digit9 => "9",
    F1 => "F1", F2 => "F2", F3 => "F3", F4 => "F4", F5 => "F5", F6 => "F6",
    F7 => "F7", F8 => "F8", F9 => "F9", F10 => "F10", F11 => "F11", F12 => "F12",
    F13 => "F13", F14 => "F14", F15 => "F15", F16 => "F16", F17 => "F17",
    F18 => "F18", F19 => "F19", F20 => "F20", F21 => "F21", F22 => "F22",
    F23 => "F23", F24 => "F24",
    Space => "Space",
}

impl HotkeyKey {
    /// The key a name stands for, in any letter case - `n` and `space` too.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|key| key.name().eq_ignore_ascii_case(name))
    }
}

/// A shortcut: its modifiers and its key.
///
/// `win` is the Windows/Command key. The defaults do not use it, but the type
/// can express it so that a configured shortcut - and the macOS convention the
/// adapter builds - has somewhere to live without widening later.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HotkeyChord {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub win: bool,
    pub key: HotkeyKey,
}

impl HotkeyChord {
    /// The `Alt+Shift+<key>` shape every default uses on Windows and Linux.
    const fn alt_shift(key: HotkeyKey) -> Self {
        Self {
            ctrl: false,
            alt: true,
            shift: true,
            win: false,
            key,
        }
    }

    /// The `Cmd+Alt+<key>` shape every default uses on macOS. `win` is the
    /// Command key there.
    const fn command_alt(key: HotkeyKey) -> Self {
        Self {
            ctrl: false,
            alt: true,
            shift: false,
            win: true,
            key,
        }
    }

    /// The shortcut as text: `Alt+Shift+N`.
    ///
    /// The one way a chord is written - in the settings file and, through the
    /// translation layer, on the screen. Modifiers in a fixed order (`Ctrl`,
    /// `Alt`, `Shift`, `Win`), then the key, joined by `+`, so the same chord is
    /// always the same text and [`HotkeyChord::parse`] reads it back unchanged.
    ///
    /// ⚠️ These are the Windows and Linux words. On macOS the tester calls the
    /// keys Option and Command (`OBS-115`), and the palette does not run there
    /// yet (`OBS-70`). The parser already takes the macOS words, so a file
    /// written with them stays readable when this learns to write them.
    #[must_use]
    pub fn text(&self) -> String {
        let mut out = String::new();
        for (held, name) in [
            (self.ctrl, "Ctrl"),
            (self.alt, "Alt"),
            (self.shift, "Shift"),
            (self.win, "Win"),
        ] {
            if held {
                out.push_str(name);
                out.push('+');
            }
        }
        out.push_str(self.key.name());
        out
    }

    /// Reads a shortcut written as text.
    ///
    /// Modifiers in any order and any letter case, the key last, `+` between
    /// them, blanks around a name ignored: `alt + shift + n` is `Alt+Shift+N`.
    /// A modifier may be named the way either system names it - `Ctrl` or
    /// `Control`, `Alt` or `Option`, `Win`, `Cmd`, `Command` or `Super` - and
    /// the key by its [`HotkeyKey::name`].
    ///
    /// Only the grammar: whether the chord is a good shortcut is
    /// [`HotkeyChord::problem`]'s question.
    ///
    /// # Errors
    ///
    /// [`ChordError`], naming the part that could not be read.
    pub fn parse(text: &str) -> Result<Self, ChordError> {
        if text.trim().is_empty() {
            return Err(ChordError::Empty);
        }
        let mut chord = Self {
            ctrl: false,
            alt: false,
            shift: false,
            win: false,
            key: HotkeyKey::Space,
        };
        let mut key = None;
        for part in text.split('+').map(str::trim) {
            if part.is_empty() {
                return Err(ChordError::EmptyPart);
            }
            if let Some(modifier) = Modifier::from_name(part) {
                if key.is_some() {
                    return Err(ChordError::KeyNotLast(part.to_owned()));
                }
                let held = modifier.flag(&mut chord);
                if *held {
                    return Err(ChordError::RepeatedModifier(part.to_owned()));
                }
                *held = true;
            } else if let Some(found) = HotkeyKey::from_name(part) {
                if key.is_some() {
                    return Err(ChordError::TwoKeys(part.to_owned()));
                }
                key = Some(found);
            } else {
                return Err(ChordError::Unknown(part.to_owned()));
            }
        }
        chord.key = key.ok_or(ChordError::NoKey)?;
        Ok(chord)
    }

    /// What is wrong with this chord as a GLOBAL shortcut, if anything.
    ///
    /// A global shortcut takes its combination from every application for as
    /// long as the palette runs. Without `Ctrl`, `Alt` or `Win` the combination
    /// is a key the applications use on their own - a letter, `Shift+letter`,
    /// Space, `F5` - so the tester would lose it everywhere, the field under
    /// test included.
    ///
    /// Not asked here, because the answer lives in the system: whether a
    /// `Ctrl+Alt` chord is `AltGr` for a character of the current layout
    /// ([`HotkeyChord::could_be_altgr`]), and whether another application holds
    /// the combination (registration reports that).
    #[must_use]
    pub const fn problem(&self) -> Option<ChordProblem> {
        if !(self.ctrl || self.alt || self.win) {
            return Some(ChordProblem::NoCommandModifier);
        }
        None
    }

    /// Whether `AltGr` can produce this chord: `Ctrl` and `Alt` without `Win`,
    /// with or without `Shift`. On Windows `AltGr` IS `Ctrl+Alt` (`D82`), so such
    /// a chord takes a character from every application exactly when the
    /// layout types one on that key - which only the layer that knows the
    /// layout can answer.
    #[must_use]
    pub const fn could_be_altgr(&self) -> bool {
        self.ctrl && self.alt && !self.win
    }
}

/// One of the four modifiers, as a name in text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Modifier {
    Ctrl,
    Alt,
    Shift,
    Win,
}

impl Modifier {
    /// The names either system uses. Closed on purpose: a word outside it is
    /// reported as unknown rather than guessed.
    const NAMES: [(&'static str, Modifier); 9] = [
        ("Ctrl", Modifier::Ctrl),
        ("Control", Modifier::Ctrl),
        ("Alt", Modifier::Alt),
        ("Option", Modifier::Alt),
        ("Shift", Modifier::Shift),
        ("Win", Modifier::Win),
        ("Cmd", Modifier::Win),
        ("Command", Modifier::Win),
        ("Super", Modifier::Win),
    ];

    fn from_name(name: &str) -> Option<Self> {
        Self::NAMES.iter().find_map(|(candidate, modifier)| {
            name.eq_ignore_ascii_case(candidate).then_some(*modifier)
        })
    }

    fn flag(self, chord: &mut HotkeyChord) -> &mut bool {
        match self {
            Modifier::Ctrl => &mut chord.ctrl,
            Modifier::Alt => &mut chord.alt,
            Modifier::Shift => &mut chord.shift,
            Modifier::Win => &mut chord.win,
        }
    }
}

/// Why a text is not a shortcut. Each carries the part that could not be
/// read, as the tester wrote it, so the sentence can point at it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChordError {
    /// Nothing written.
    Empty,
    /// Two `+` in a row, or one at an end: `Alt++N`, `Alt+Shift+`.
    EmptyPart,
    /// A word that names neither a modifier nor a key: `Alt+Shfit+N`, `Alt+;`.
    Unknown(String),
    /// Modifiers only: `Alt+Shift`.
    NoKey,
    /// A second key: `Alt+N+P`.
    TwoKeys(String),
    /// A modifier after the key: `N+Alt`.
    KeyNotLast(String),
    /// The same modifier twice, under any of its names: `Alt+Option+N`.
    RepeatedModifier(String),
}

/// Why a chord that reads fine is still not a global shortcut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChordProblem {
    /// No `Ctrl`, `Alt` or `Win` - see [`HotkeyChord::problem`].
    NoCommandModifier,
}

/// Which set of default shortcuts a system follows.
///
/// Chosen by the layer that knows the system. The core only holds the tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Convention {
    /// `Alt+Shift+<key>` - Windows and Linux.
    WindowsAndLinux,
    /// `Cmd+Alt+<key>` - macOS.
    MacOs,
}

/// The key each action is bound to by default, the same on every system.
///
/// One entry per action, in the order of [`HotkeyAction::ALL`]. Only the
/// modifiers differ between the conventions, so a tester who moves between
/// systems keeps the letters. The letters are the ones `ux-spec.md` 3 chose, so
/// the move from `Ctrl+Alt` changed the family and nothing else.
const DEFAULT_KEYS: [(HotkeyAction, HotkeyKey); 10] = [
    (HotkeyAction::NextValue, HotkeyKey::N),
    (HotkeyAction::PreviousValue, HotkeyKey::P),
    (HotkeyAction::RepeatLast, HotkeyKey::R),
    (HotkeyAction::RestartPack, HotkeyKey::Digit0),
    (HotkeyAction::CopyReport, HotkeyKey::B),
    (HotkeyAction::MarkOk, HotkeyKey::Digit1),
    (HotkeyAction::MarkProblem, HotkeyKey::Digit2),
    (HotkeyAction::MarkSuspect, HotkeyKey::Digit3),
    (HotkeyAction::OpenPacks, HotkeyKey::Space),
    (HotkeyAction::ToggleVisibility, HotkeyKey::H),
];

/// One convention's table, built from [`DEFAULT_KEYS`] so the two cannot
/// disagree about which letter belongs to which action.
const fn table(convention: Convention) -> [(HotkeyAction, HotkeyChord); 10] {
    let mut out = [(
        HotkeyAction::NextValue,
        HotkeyChord::alt_shift(HotkeyKey::N),
    ); 10];
    let mut index = 0;
    while index < DEFAULT_KEYS.len() {
        let (action, key) = DEFAULT_KEYS[index];
        let chord = match convention {
            Convention::WindowsAndLinux => HotkeyChord::alt_shift(key),
            Convention::MacOs => HotkeyChord::command_alt(key),
        };
        out[index] = (action, chord);
        index += 1;
    }
    out
}

/// The default shortcut for every action, in the Windows and Linux convention.
///
/// Every default is `Alt+Shift+<key>`: `ux-spec.md` 3 keeps them one family so
/// a tester learns them as a set rather than as ten separate things. The table
/// is checked before any code depends on it - every action present exactly once,
/// and no two actions on the same combination - because a duplicate combination
/// would be a registration that reports the second action as `Taken` against the
/// first.
pub const DEFAULT_BINDINGS: [(HotkeyAction, HotkeyChord); 10] = table(Convention::WindowsAndLinux);

/// The same table in the macOS convention, `Cmd+Alt+<key>`.
pub const MACOS_DEFAULT_BINDINGS: [(HotkeyAction, HotkeyChord); 10] = table(Convention::MacOs);

/// The default table a convention uses.
#[must_use]
pub const fn default_bindings(
    convention: Convention,
) -> &'static [(HotkeyAction, HotkeyChord); 10] {
    match convention {
        Convention::WindowsAndLinux => &DEFAULT_BINDINGS,
        Convention::MacOs => &MACOS_DEFAULT_BINDINGS,
    }
}

/// The default chord for one action, if the default table names it.
///
/// Total over [`HotkeyAction::ALL`] by the test below, so the `Option` is really
/// only `None` for a hypothetical action added without a default - which the
/// test refuses.
#[must_use]
pub fn default_chord(convention: Convention, action: HotkeyAction) -> Option<HotkeyChord> {
    default_bindings(convention)
        .iter()
        .find_map(|(candidate, chord)| (*candidate == action).then_some(*chord))
}

/// The shortcut of every action for one run of the palette: the defaults,
/// with the tester's own choices on top where they can be used.
///
/// Every action exactly once, in the order of [`HotkeyAction::ALL`], and no
/// two on the same chord - by construction: the only ways to get one are
/// [`Bindings::defaults`], whose tables are checked by the tests below, and
/// [`Bindings::with`], which keeps both properties. So [`Bindings::chord`]
/// needs no `Option`, and registering never reports one action as `Taken`
/// against another of our own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bindings([(HotkeyAction, HotkeyChord); 10]);

impl Bindings {
    /// The default table of a convention.
    #[must_use]
    pub const fn defaults(convention: Convention) -> Self {
        Self(*default_bindings(convention))
    }

    /// The chord an action answers to.
    #[must_use]
    pub fn chord(&self, action: HotkeyAction) -> HotkeyChord {
        // Every action is in the table (see the type), so the fallback is never
        // taken. It is the first entry rather than a panic, because the
        // workspace denies panics in product code.
        self.0
            .iter()
            .find_map(|(candidate, chord)| (*candidate == action).then_some(*chord))
            .unwrap_or(self.0[0].1)
    }

    /// Every action with its chord, in the order of [`HotkeyAction::ALL`].
    #[must_use]
    pub fn as_slice(&self) -> &[(HotkeyAction, HotkeyChord)] {
        &self.0
    }

    /// These bindings with the tester's `wanted` chords on top, and every
    /// wanted chord that could not be used, with the reason.
    ///
    /// A wanted chord is refused when:
    /// - it has a [`HotkeyChord::problem`] - it would take a key from every
    ///   application.
    /// - another action ends up answering to the same chord. First a wanted
    ///   chord that is already the chord of an action NOT changing goes back
    ///   (that action keeps it for good), then, of two wanted chords that
    ///   are the same, the later in [`HotkeyAction::ALL`] goes back. Going
    ///   back may make another wanted chord clash, so this repeats until
    ///   nothing does - at most once per action. Two actions swapping their
    ///   chords is not a clash: only the result is compared.
    ///
    /// The action a clash names is read from the FINISHED table, so the
    /// sentence "already the shortcut for X" is true of the palette the tester
    /// gets, even when three wanted chords knocked each other back in a row.
    ///
    /// A wanted chord an action already has is no choice at all, and never
    /// refused. A refused action keeps the chord it had here. An action named
    /// twice in `wanted` takes the later chord - a table in the settings file
    /// cannot name a key twice, so this only decides what a caller gets.
    #[must_use]
    pub fn with(self, wanted: &[(HotkeyAction, HotkeyChord)]) -> (Self, Vec<Refused>) {
        let before = self.0;
        let mut table = self.0;
        let mut chosen = [false; 10];
        let mut refused = Vec::new();
        for (action, chord) in wanted {
            let Some(at) = table.iter().position(|(candidate, _)| candidate == action) else {
                continue;
            };
            if let Some(problem) = chord.problem() {
                refused.push(Refused {
                    action: *action,
                    chord: *chord,
                    why: Refusal::Problem(problem),
                });
                continue;
            }
            table[at].1 = *chord;
            chosen[at] = *chord != before[at].1;
        }
        let mut clashed = Vec::new();
        while let Some(at) = first_clash(&table, &chosen) {
            clashed.push((table[at].0, table[at].1));
            table[at].1 = before[at].1;
            chosen[at] = false;
        }
        for (action, chord) in clashed {
            let owner = table
                .iter()
                .find_map(|(other, held)| (*held == chord && *other != action).then_some(*other));
            // Someone holds it: a chord goes back only while another action
            // has it, and the holder of a chord goes back only to hand it to
            // an action that keeps it. The fallback names no action falsely -
            // it names the refused one, which the sentence cannot confuse
            // with another.
            refused.push(Refused {
                action,
                chord,
                why: Refusal::SameAs(owner.unwrap_or(action)),
            });
        }
        (Self(table), refused)
    }
}

/// The position of the wanted chord to send back next, if any clashes: first
/// one shared with an action that is not changing, then the later of two
/// wanted chords that are the same.
fn first_clash(table: &[(HotkeyAction, HotkeyChord); 10], chosen: &[bool; 10]) -> Option<usize> {
    let shares = |at: usize, with_chosen: bool| {
        table.iter().enumerate().any(|(other, (_, chord))| {
            other != at && chosen[other] == with_chosen && *chord == table[at].1
        })
    };
    let wanted = || (0..table.len()).filter(|at| chosen[*at]);
    wanted()
        .find(|at| shares(*at, false))
        .or_else(|| wanted().rev().find(|at| shares(*at, true)))
}

/// A chord the tester wanted for an action and did not get.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Refused {
    pub action: HotkeyAction,
    /// The chord as wanted.
    pub chord: HotkeyChord,
    pub why: Refusal,
}

/// Why a wanted chord was not used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// Not a good global shortcut at all.
    Problem(ChordProblem),
    /// Another action answers to this chord.
    SameAs(HotkeyAction),
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;

    const CONVENTIONS: [Convention; 2] = [Convention::WindowsAndLinux, Convention::MacOs];

    #[test]
    fn every_action_has_exactly_one_default() {
        // The registrar reserves a combination per action, so a missing default
        // is an action that can never be pressed, and a doubled one is a table
        // that disagrees with itself.
        for convention in CONVENTIONS {
            let table = default_bindings(convention);
            for action in HotkeyAction::ALL {
                let count = table
                    .iter()
                    .filter(|(candidate, _)| *candidate == action)
                    .count();
                assert_eq!(
                    count, 1,
                    "{convention:?}: {action:?} must have exactly one default binding"
                );
            }
        }
    }

    #[test]
    fn no_two_actions_share_a_combination() {
        // Applied to this table before any code leaned on it - the lesson from
        // OBS-48. Two actions on one combination would make the second register
        // as Taken against the first, silently losing a shortcut.
        for convention in CONVENTIONS {
            let table = default_bindings(convention);
            for (i, (action_a, chord_a)) in table.iter().enumerate() {
                for (action_b, chord_b) in &table[i + 1..] {
                    assert_ne!(
                        chord_a, chord_b,
                        "{convention:?}: {action_a:?} and {action_b:?} share a combination"
                    );
                }
            }
        }
    }

    #[test]
    fn every_default_on_windows_and_linux_is_alt_shift_and_never_ctrl_alt() {
        // D82: Ctrl+Alt is AltGr on Windows, so a default holding both would
        // swallow a letter of somebody's alphabet in every application while the
        // palette runs. One family, so a tester can guess the rest from one.
        for (action, chord) in DEFAULT_BINDINGS {
            assert!(
                chord.alt && chord.shift && !chord.ctrl && !chord.win,
                "{action:?} default is not Alt+Shift: {chord:?}"
            );
        }
    }

    #[test]
    fn every_default_on_macos_is_command_alt() {
        // Option alone types characters on macOS, Command does not.
        for (action, chord) in MACOS_DEFAULT_BINDINGS {
            assert!(
                chord.win && chord.alt && !chord.ctrl && !chord.shift,
                "{action:?} macOS default is not Cmd+Alt: {chord:?}"
            );
        }
    }

    #[test]
    fn both_conventions_bind_the_same_key_to_the_same_action() {
        // Only the modifiers differ, so a tester moving between systems keeps
        // the letters - and a table edited on one side only is caught here.
        for ((action_a, chord_a), (action_b, chord_b)) in
            DEFAULT_BINDINGS.iter().zip(MACOS_DEFAULT_BINDINGS.iter())
        {
            assert_eq!(action_a, action_b);
            assert_eq!(chord_a.key, chord_b.key, "{action_a:?}");
        }
    }

    #[test]
    fn default_chord_is_total_over_the_action_set() {
        for convention in CONVENTIONS {
            for action in HotkeyAction::ALL {
                assert!(
                    default_chord(convention, action).is_some(),
                    "{convention:?}: {action:?} has no default chord"
                );
            }
        }
    }

    fn chord(ctrl: bool, alt: bool, shift: bool, win: bool, key: HotkeyKey) -> HotkeyChord {
        HotkeyChord {
            ctrl,
            alt,
            shift,
            win,
            key,
        }
    }

    #[test]
    fn the_keys_are_letters_digits_function_keys_and_space_each_named_once() {
        assert_eq!(HotkeyKey::ALL.len(), 26 + 10 + 24 + 1);
        for (i, a) in HotkeyKey::ALL.iter().enumerate() {
            for b in &HotkeyKey::ALL[i + 1..] {
                assert_ne!(a, b);
                assert!(!a.name().eq_ignore_ascii_case(b.name()), "{a:?} {b:?}");
            }
            assert_eq!(HotkeyKey::from_name(a.name()), Some(*a));
            assert_eq!(HotkeyKey::from_name(&a.name().to_lowercase()), Some(*a));
        }
    }

    #[test]
    fn every_chord_is_read_back_from_its_own_text() {
        // All sixteen modifier sets on every key: what `text` writes into the
        // settings file, `parse` must read back as the same chord.
        for key in HotkeyKey::ALL {
            for bits in 0..16u8 {
                let written = chord(
                    bits & 1 != 0,
                    bits & 2 != 0,
                    bits & 4 != 0,
                    bits & 8 != 0,
                    *key,
                );
                assert_eq!(
                    HotkeyChord::parse(&written.text()),
                    Ok(written),
                    "{}",
                    written.text()
                );
            }
        }
    }

    #[test]
    fn the_text_is_the_modifiers_in_a_fixed_order_then_the_key() {
        let all = chord(true, true, true, true, HotkeyKey::F12);
        assert_eq!(all.text(), "Ctrl+Alt+Shift+Win+F12");
        assert_eq!(DEFAULT_BINDINGS[0].1.text(), "Alt+Shift+N");
        assert_eq!(
            chord(false, true, true, false, HotkeyKey::Space).text(),
            "Alt+Shift+Space"
        );
        assert_eq!(
            chord(false, true, false, true, HotkeyKey::Digit0).text(),
            "Alt+Win+0"
        );
    }

    #[test]
    fn a_person_may_write_it_in_any_case_order_and_either_systems_words() {
        let alt_shift_n = chord(false, true, true, false, HotkeyKey::N);
        for text in [
            "Alt+Shift+N",
            "alt+shift+n",
            "Shift+Alt+N",
            " Alt + Shift + N ",
            "Option+Shift+n",
        ] {
            assert_eq!(HotkeyChord::parse(text), Ok(alt_shift_n), "{text}");
        }
        let command = chord(false, true, false, true, HotkeyKey::Space);
        for text in [
            "Cmd+Alt+Space",
            "Command+Option+space",
            "Super+Alt+Space",
            "Win+Alt+Space",
        ] {
            assert_eq!(HotkeyChord::parse(text), Ok(command), "{text}");
        }
        assert_eq!(
            HotkeyChord::parse("Control+F1"),
            Ok(chord(true, false, false, false, HotkeyKey::F1))
        );
    }

    #[test]
    fn a_text_that_is_not_a_shortcut_names_the_part_that_is_wrong() {
        let cases: [(&str, ChordError); 11] = [
            ("", ChordError::Empty),
            ("   ", ChordError::Empty),
            ("Alt++N", ChordError::EmptyPart),
            ("Alt+Shift+", ChordError::EmptyPart),
            ("Alt+Shfit+N", ChordError::Unknown(String::from("Shfit"))),
            ("Alt+;", ChordError::Unknown(String::from(";"))),
            ("Alt+Esc", ChordError::Unknown(String::from("Esc"))),
            ("Alt+Shift", ChordError::NoKey),
            ("Alt+N+P", ChordError::TwoKeys(String::from("P"))),
            ("N+Alt", ChordError::KeyNotLast(String::from("Alt"))),
            (
                "Alt+Option+N",
                ChordError::RepeatedModifier(String::from("Option")),
            ),
        ];
        for (text, expected) in cases {
            assert_eq!(HotkeyChord::parse(text), Err(expected), "{text:?}");
        }
    }

    #[test]
    fn a_global_shortcut_needs_ctrl_alt_or_win() {
        for text in ["N", "Shift+N", "F5", "Space", "Shift+Space"] {
            let parsed = HotkeyChord::parse(text).expect("reads");
            assert_eq!(
                parsed.problem(),
                Some(ChordProblem::NoCommandModifier),
                "{text}"
            );
        }
        for text in ["Alt+N", "Ctrl+N", "Win+N", "Ctrl+Shift+F5"] {
            let parsed = HotkeyChord::parse(text).expect("reads");
            assert_eq!(parsed.problem(), None, "{text}");
        }
        for convention in CONVENTIONS {
            for (action, default) in default_bindings(convention) {
                assert_eq!(default.problem(), None, "{convention:?} {action:?}");
            }
        }
    }

    fn alt_shift(key: HotkeyKey) -> HotkeyChord {
        chord(false, true, true, false, key)
    }

    const DEFAULTS: Bindings = Bindings::defaults(Convention::WindowsAndLinux);

    #[test]
    fn the_default_bindings_are_the_default_table() {
        for convention in CONVENTIONS {
            let bindings = Bindings::defaults(convention);
            assert_eq!(bindings.as_slice(), default_bindings(convention));
            for action in HotkeyAction::ALL {
                assert_eq!(
                    Some(bindings.chord(action)),
                    default_chord(convention, action)
                );
            }
        }
    }

    #[test]
    fn every_action_has_an_id_of_its_own_that_reads_back() {
        for (i, action) in HotkeyAction::ALL.iter().enumerate() {
            assert_eq!(HotkeyAction::from_id(action.id()), Some(*action));
            for other in &HotkeyAction::ALL[i + 1..] {
                assert_ne!(action.id(), other.id());
            }
            assert!(
                action
                    .id()
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c == '-'),
                "{}",
                action.id()
            );
        }
        assert_eq!(HotkeyAction::from_id("Next-Value"), None, "written exactly");
    }

    #[test]
    fn a_wanted_chord_replaces_the_default_and_the_rest_stay() {
        let m = alt_shift(HotkeyKey::M);
        let (bindings, refused) = DEFAULTS.with(&[(HotkeyAction::NextValue, m)]);
        assert!(refused.is_empty(), "{refused:?}");
        assert_eq!(bindings.chord(HotkeyAction::NextValue), m);
        for action in &HotkeyAction::ALL[1..] {
            assert_eq!(bindings.chord(*action), DEFAULTS.chord(*action));
        }
    }

    #[test]
    fn a_chord_that_would_take_a_key_everywhere_is_refused() {
        let plain = HotkeyChord::parse("Shift+M").expect("reads");
        let (bindings, refused) = DEFAULTS.with(&[(HotkeyAction::NextValue, plain)]);
        assert_eq!(bindings, DEFAULTS);
        assert_eq!(
            refused,
            vec![Refused {
                action: HotkeyAction::NextValue,
                chord: plain,
                why: Refusal::Problem(ChordProblem::NoCommandModifier),
            }]
        );
    }

    #[test]
    fn a_chord_another_action_keeps_is_refused_and_names_that_action() {
        let p = alt_shift(HotkeyKey::P);
        let (bindings, refused) = DEFAULTS.with(&[(HotkeyAction::NextValue, p)]);
        assert_eq!(bindings, DEFAULTS);
        assert_eq!(
            refused,
            vec![Refused {
                action: HotkeyAction::NextValue,
                chord: p,
                why: Refusal::SameAs(HotkeyAction::PreviousValue),
            }]
        );
    }

    #[test]
    fn two_actions_may_swap_their_chords() {
        let (n, p) = (alt_shift(HotkeyKey::N), alt_shift(HotkeyKey::P));
        let (bindings, refused) = DEFAULTS.with(&[
            (HotkeyAction::NextValue, p),
            (HotkeyAction::PreviousValue, n),
        ]);
        assert!(refused.is_empty(), "{refused:?}");
        assert_eq!(bindings.chord(HotkeyAction::NextValue), p);
        assert_eq!(bindings.chord(HotkeyAction::PreviousValue), n);
    }

    #[test]
    fn of_two_actions_wanting_one_chord_the_later_goes_back() {
        let x = alt_shift(HotkeyKey::X);
        let (bindings, refused) =
            DEFAULTS.with(&[(HotkeyAction::MarkOk, x), (HotkeyAction::NextValue, x)]);
        assert_eq!(bindings.chord(HotkeyAction::NextValue), x);
        assert_eq!(
            bindings.chord(HotkeyAction::MarkOk),
            DEFAULTS.chord(HotkeyAction::MarkOk)
        );
        assert_eq!(
            refused,
            vec![Refused {
                action: HotkeyAction::MarkOk,
                chord: x,
                why: Refusal::SameAs(HotkeyAction::NextValue),
            }]
        );
    }

    #[test]
    fn the_action_a_clash_names_holds_the_chord_in_the_finished_table() {
        // Three wanted chords knocking each other back in a row: previous goes
        // back from R, which frees P for... nobody, because repeat wanted P and
        // now clashes with previous, goes back to R, and pushes next back too.
        // Every sentence must name who holds the chord at the END.
        let (p, r) = (alt_shift(HotkeyKey::P), alt_shift(HotkeyKey::R));
        let (bindings, refused) = DEFAULTS.with(&[
            (HotkeyAction::NextValue, r),
            (HotkeyAction::PreviousValue, r),
            (HotkeyAction::RepeatLast, p),
        ]);
        assert_eq!(bindings, DEFAULTS);
        assert_eq!(refused.len(), 3, "{refused:?}");
        for refusal in &refused {
            let Refusal::SameAs(owner) = refusal.why else {
                panic!("{refusal:?}");
            };
            assert_eq!(bindings.chord(owner), refusal.chord, "{refusal:?}");
            assert_ne!(owner, refusal.action);
        }
    }

    #[test]
    fn whatever_is_wanted_the_result_is_a_table_with_no_clash_and_every_refusal_true() {
        // Every combination of what three actions may want from a pool that
        // holds their own defaults, each other's, two new chords, one that
        // clashes with an action outside the three, and one with a problem.
        let pool = [
            None,
            Some(alt_shift(HotkeyKey::N)),
            Some(alt_shift(HotkeyKey::P)),
            Some(alt_shift(HotkeyKey::R)),
            Some(alt_shift(HotkeyKey::X)),
            Some(alt_shift(HotkeyKey::Y)),
            Some(alt_shift(HotkeyKey::B)),
            Some(HotkeyChord::parse("Shift+Z").expect("reads")),
        ];
        let actions = [
            HotkeyAction::NextValue,
            HotkeyAction::PreviousValue,
            HotkeyAction::RepeatLast,
        ];
        for a in pool {
            for b in pool {
                for c in pool {
                    let wanted: Vec<(HotkeyAction, HotkeyChord)> = actions
                        .iter()
                        .zip([a, b, c])
                        .filter_map(|(action, chord)| chord.map(|chord| (*action, chord)))
                        .collect();
                    let (bindings, refused) = DEFAULTS.with(&wanted);
                    let table = bindings.as_slice();
                    for (i, (_, one)) in table.iter().enumerate() {
                        assert_eq!(one.problem(), None, "{wanted:?}");
                        for (_, other) in &table[i + 1..] {
                            assert_ne!(one, other, "{wanted:?} -> {table:?}");
                        }
                    }
                    for (action, chord) in &wanted {
                        let refusal = refused.iter().find(|r| r.action == *action);
                        match refusal {
                            None => assert_eq!(bindings.chord(*action), *chord, "{wanted:?}"),
                            Some(refusal) => {
                                assert_eq!(bindings.chord(*action), DEFAULTS.chord(*action));
                                assert_ne!(*chord, DEFAULTS.chord(*action), "own default refused");
                                if let Refusal::SameAs(owner) = refusal.why {
                                    assert_eq!(bindings.chord(owner), *chord, "{wanted:?}");
                                }
                            }
                        }
                    }
                    assert!(refused.len() <= wanted.len());
                    for action in HotkeyAction::ALL {
                        if !wanted.iter().any(|(w, _)| *w == action) {
                            assert_eq!(bindings.chord(action), DEFAULTS.chord(action));
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn only_ctrl_alt_without_win_can_be_altgr() {
        let altgr_like = ["Ctrl+Alt+N", "Ctrl+Alt+Shift+N"];
        let not = ["Alt+Shift+N", "Ctrl+N", "Ctrl+Alt+Win+N", "Alt+N"];
        for text in altgr_like {
            assert!(
                HotkeyChord::parse(text).expect("reads").could_be_altgr(),
                "{text}"
            );
        }
        for text in not {
            assert!(
                !HotkeyChord::parse(text).expect("reads").could_be_altgr(),
                "{text}"
            );
        }
        for convention in CONVENTIONS {
            for (action, default) in default_bindings(convention) {
                assert!(!default.could_be_altgr(), "{convention:?} {action:?}");
            }
        }
    }
}
