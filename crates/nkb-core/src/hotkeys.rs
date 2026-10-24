//! The global shortcuts the palette listens for, as data.
//!
//! `ux-spec.md` 3 fixes the global actions and a default combination for each.
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
//! # Why the family is `Alt+Shift` and not `Ctrl+Alt` - on Windows
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
//! # Three conventions, all as data
//!
//! macOS keeps `Cmd+Alt+<key>` (`ux-spec.md` 3): there `Option` types
//! characters and `Command` does not. Until `D82` the macOS table did not exist
//! and the registering adapter swapped `ctrl` for the Command bit on every
//! chord. That worked for a table whose every entry held `ctrl`, and it would
//! have rewritten a chord the tester chose. So the tables live here, and the
//! layer that knows the system picks one - this crate has no `cfg` and keeps
//! none.
//!
//! Linux goes back to `Ctrl+Alt+<key>` (`D145`, the owner's choice). The reason
//! `D82` left it is Windows-only: on X11 `AltGr` is `ISO_Level3_Shift`, a
//! modifier of its own, so `Ctrl+Alt` types nothing. And `Alt+Shift` is the chord
//! `grp:alt_shift_toggle` and three other layout toggles take whole, before a
//! grab hears it (`OBS-215`).
//!
//! # The key that types is a fact of the convention too
//!
//! A global shortcut takes its chord from every application, so a chord a
//! keyboard layout types a character on takes that character from them all. On
//! Windows the chord is `Ctrl+Alt` (`AltGr`, `D88`), on macOS `Option` with no
//! `Command` and no `Control` (`D141`), on Linux there is none. The core holds
//! the SHAPE of that chord ([`Convention::can_type`]) and the system answers
//! which character, if any, a layout types there: [`Bindings::typing_refusal`].

/// One of the global actions the palette answers to.
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
    /// Move the next value one forward without typing anything - walking a
    /// pack to see what is in it (`D120`).
    SkipValue,
    /// Move the next value one back without typing anything (`D120`).
    BackOneValue,
    /// Open the next pack of the value window's list, from its first value
    /// (`D120`).
    NextPack,
    /// Open the pack before it in that list, from its first value (`D120`).
    PreviousPack,
}

impl HotkeyAction {
    /// How many actions there are: the size of every table that holds one
    /// entry per action, so a new action changes this number and no other.
    pub const COUNT: usize = 14;

    /// Every action, so a test can state the size of the set and check that the
    /// default table covers it. Rust has no way to enumerate variants, so this
    /// list is the source of truth and the test guards that it stays complete.
    ///
    /// 🔴 The ORDER is not cosmetic: where two wanted shortcuts clash, the one
    /// earlier here keeps its combination (`D89`). A new action goes at the end,
    /// so it never takes a combination from one that was here before it.
    pub const ALL: [HotkeyAction; Self::COUNT] = [
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
        HotkeyAction::SkipValue,
        HotkeyAction::BackOneValue,
        HotkeyAction::NextPack,
        HotkeyAction::PreviousPack,
    ];

    /// Every action in the order a person reads them: what types, what walks
    /// the pack, what changes the pack, the windows, and last the actions this
    /// build does not carry out yet (`D120`). The hint bar and the shortcuts
    /// window list them so.
    ///
    /// Apart from [`Self::ALL`] on purpose: that order decides which of two
    /// clashing shortcuts is kept (`D89`), and a new action must never win
    /// that by where it reads best.
    pub const READING_ORDER: [HotkeyAction; Self::COUNT] = [
        HotkeyAction::NextValue,
        HotkeyAction::PreviousValue,
        HotkeyAction::SkipValue,
        HotkeyAction::BackOneValue,
        HotkeyAction::RestartPack,
        HotkeyAction::NextPack,
        HotkeyAction::PreviousPack,
        HotkeyAction::OpenPacks,
        HotkeyAction::CopyReport,
        HotkeyAction::ToggleVisibility,
        HotkeyAction::RepeatLast,
        HotkeyAction::MarkOk,
        HotkeyAction::MarkProblem,
        HotkeyAction::MarkSuspect,
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
            HotkeyAction::SkipValue => "skip-value",
            HotkeyAction::BackOneValue => "back-one-value",
            HotkeyAction::NextPack => "next-pack",
            HotkeyAction::PreviousPack => "previous-pack",
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
    /// The `Alt+Shift+<key>` shape every default uses on Windows.
    const fn alt_shift(key: HotkeyKey) -> Self {
        Self {
            ctrl: false,
            alt: true,
            shift: true,
            win: false,
            key,
        }
    }

    /// The `Ctrl+Alt+<key>` shape every default uses on Linux.
    const fn ctrl_alt(key: HotkeyKey) -> Self {
        Self {
            ctrl: true,
            alt: true,
            shift: false,
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
    /// These are the Windows words, and the words of the settings file on every
    /// system. What the screen says on macOS, where the tester calls the keys
    /// Option and Command, and on Linux, where the Windows key is Super, is
    /// [`Self::display`].
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

    /// The shortcut as the tester of `convention` calls its keys, for the screen.
    ///
    /// The same chord and the same order of modifiers as [`Self::text`] -
    /// Control, Option, Shift, Command is the order Apple writes them in too -
    /// with the words of the system: `Option+Command+N`, `Ctrl+Alt+Super+N`. Every
    /// word is one [`Self::parse`] reads back, so the screen still shows what the
    /// tester writes to change a shortcut (`OBS-115`).
    #[must_use]
    pub fn display(&self, convention: Convention) -> String {
        let names = match convention {
            Convention::Windows => return self.text(),
            Convention::Linux => ["Ctrl", "Alt", "Shift", "Super"],
            Convention::MacOs => ["Control", "Option", "Shift", "Command"],
        };
        let mut out = String::new();
        for (held, name) in [self.ctrl, self.alt, self.shift, self.win]
            .into_iter()
            .zip(names)
        {
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

    /// Whether `Option` can make a keyboard layout type this chord on macOS:
    /// `Option` held, with or without `Shift`, and neither `Command` nor
    /// `Control`. Such a chord takes a character from every application exactly
    /// when the layout types one on that key (`D141`), which only the layer that
    /// knows the layout can answer.
    ///
    /// Measured on macOS 27.0 with the Polish layout, 2026-10-21: `Option+A`
    /// typed `U+0105` into a field, and the same chord registered as a global
    /// shortcut typed nothing there, 3 presses of 3, with `Shift` as well. With
    /// `Command` the layout types nothing a field takes (`Cmd+Opt+A` is a
    /// shortcut to every application). A chord with `Control` is not asked
    /// about: the layouts answer a control character on every letter, and for the
    /// digits only an application can say what it inserts (the window measured
    /// took `Control+Option+1` as a `1`, which no rule here can generalise).
    #[must_use]
    pub const fn could_type_with_option(&self) -> bool {
        self.alt && !self.ctrl && !self.win
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
    /// `Alt+Shift+<key>` - Windows.
    Windows,
    /// `Ctrl+Alt+<key>` - Linux, where `AltGr` is a key of its own.
    Linux,
    /// `Cmd+Alt+<key>` - macOS.
    MacOs,
}

impl Convention {
    /// Whether a layout of this convention's system can make `chord` type a
    /// character, which a global shortcut on it would then take from every
    /// application: `Ctrl+Alt` is `AltGr` on Windows
    /// ([`HotkeyChord::could_be_altgr`]), `Option` types on macOS
    /// ([`HotkeyChord::could_type_with_option`]). The two shapes exclude each
    /// other, and each system asks only its own.
    ///
    /// Linux asks nothing: `AltGr` is `ISO_Level3_Shift` there, a modifier of its
    /// own that a grab on `Ctrl` and `Alt` does not match, so no chord takes a
    /// character from an application (measured on X11 with the Polish layout,
    /// `D145`).
    ///
    /// Whether the layout DOES type one is the system's answer, asked only when
    /// this one is true.
    #[must_use]
    pub const fn can_type(self, chord: &HotkeyChord) -> bool {
        match self {
            Convention::Windows => chord.could_be_altgr(),
            Convention::Linux => false,
            Convention::MacOs => chord.could_type_with_option(),
        }
    }
}

/// The key each action is bound to by default, the same on every system but one.
///
/// One entry per action, in the order of [`HotkeyAction::ALL`]. The modifiers
/// differ between the conventions, so a tester who moves between systems keeps
/// the letters - all but the one [`macos_key`] names. The letters are the ones
/// `ux-spec.md` 3 chose, so the move from `Ctrl+Alt` to `Alt+Shift` on Windows
/// changed the family and nothing else, and Linux went back to `Ctrl+Alt` with
/// the same letters.
///
/// ⊕ The four of `D120` go in pairs of neighbouring keys, back on the left and
/// forward on the right: `A` | `S` for a value, `Z` | `X` for a pack.
const DEFAULT_KEYS: [(HotkeyAction, HotkeyKey); HotkeyAction::COUNT] = [
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
    (HotkeyAction::SkipValue, HotkeyKey::S),
    (HotkeyAction::BackOneValue, HotkeyKey::A),
    (HotkeyAction::NextPack, HotkeyKey::X),
    (HotkeyAction::PreviousPack, HotkeyKey::Z),
];

/// The key the macOS table binds to `action`, where it is not the key of
/// [`DEFAULT_KEYS`].
///
/// One exception, and a measured one (2026-10-20, macOS 27.0): `Option+Command+
/// Space` is a shortcut of the system - it opens the Finder search window. A
/// registered hot key does not keep it from the system, and Finder takes the front
/// the moment the chord is pressed, so the value window the chord opens closes
/// with it (`OBS-186`). `F` for Find is free in the table. The owner's choice,
/// 2026-10-20.
const fn macos_key(action: HotkeyAction, key: HotkeyKey) -> HotkeyKey {
    match action {
        HotkeyAction::OpenPacks => HotkeyKey::F,
        _ => key,
    }
}

/// One convention's table, built from [`DEFAULT_KEYS`] so the three cannot
/// disagree about which letter belongs to which action - [`macos_key`] apart.
const fn table(convention: Convention) -> [(HotkeyAction, HotkeyChord); HotkeyAction::COUNT] {
    let mut out = [(
        HotkeyAction::NextValue,
        HotkeyChord::alt_shift(HotkeyKey::N),
    ); HotkeyAction::COUNT];
    let mut index = 0;
    while index < DEFAULT_KEYS.len() {
        let (action, key) = DEFAULT_KEYS[index];
        let chord = match convention {
            Convention::Windows => HotkeyChord::alt_shift(key),
            Convention::Linux => HotkeyChord::ctrl_alt(key),
            Convention::MacOs => HotkeyChord::command_alt(macos_key(action, key)),
        };
        out[index] = (action, chord);
        index += 1;
    }
    out
}

/// The default shortcut for every action, in the Windows convention.
///
/// Every default is `Alt+Shift+<key>`: `ux-spec.md` 3 keeps them one family so
/// a tester learns them as a set rather than as so many separate things. The table
/// is checked before any code depends on it - every action present exactly once,
/// and no two actions on the same combination - because a duplicate combination
/// would be a registration that reports the second action as `Taken` against the
/// first.
pub const DEFAULT_BINDINGS: [(HotkeyAction, HotkeyChord); HotkeyAction::COUNT] =
    table(Convention::Windows);

/// The same table in the Linux convention, `Ctrl+Alt+<key>`.
///
/// Not `Alt+Shift` because the layout toggles of X11 (`grp:alt_shift_toggle` and
/// the others the server consumes before a grab hears them) take that chord
/// whole, and a shortcut registered on it never fires (`OBS-215`, `D145`).
/// `Ctrl+Alt` is taken only by `grp:ctrl_alt_toggle`, which is rare.
pub const LINUX_DEFAULT_BINDINGS: [(HotkeyAction, HotkeyChord); HotkeyAction::COUNT] =
    table(Convention::Linux);

/// The same table in the macOS convention, `Cmd+Alt+<key>`.
pub const MACOS_DEFAULT_BINDINGS: [(HotkeyAction, HotkeyChord); HotkeyAction::COUNT] =
    table(Convention::MacOs);

/// The default table a convention uses.
#[must_use]
pub const fn default_bindings(
    convention: Convention,
) -> &'static [(HotkeyAction, HotkeyChord); HotkeyAction::COUNT] {
    match convention {
        Convention::Windows => &DEFAULT_BINDINGS,
        Convention::Linux => &LINUX_DEFAULT_BINDINGS,
        Convention::MacOs => &MACOS_DEFAULT_BINDINGS,
    }
}

/// The shortcut of every action for one run of the palette: the defaults,
/// with the tester's own choices on top where they can be used.
///
/// Every action exactly once, in the order of [`HotkeyAction::ALL`], and no
/// two on the same chord - by construction: the only ways to get one to
/// register are [`Bindings::defaults`], whose tables are checked by the tests
/// below, and [`Bindings::with`], which keeps both properties. So
/// [`Bindings::chord`] needs no `Option`, and registering never reports one
/// action as `Taken` against another of our own. [`Bindings::as_given`] is the
/// table for WORDS only: it keeps the first property and not the second.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bindings {
    /// The convention these bindings follow: it decides which chords a layout
    /// is asked about ([`Convention::can_type`]).
    convention: Convention,
    table: [(HotkeyAction, HotkeyChord); HotkeyAction::COUNT],
}

impl Bindings {
    /// The default table of a convention.
    #[must_use]
    pub const fn defaults(convention: Convention) -> Self {
        Self {
            convention,
            table: *default_bindings(convention),
        }
    }

    /// The chord an action answers to.
    #[must_use]
    pub fn chord(&self, action: HotkeyAction) -> HotkeyChord {
        // Every action is in the table (see the type), so the fallback is never
        // taken. It is the first entry rather than a panic, because the
        // workspace denies panics in product code.
        self.table
            .iter()
            .find_map(|(candidate, chord)| (*candidate == action).then_some(*chord))
            .unwrap_or(self.table[0].1)
    }

    /// Every action with its chord, in the order of [`HotkeyAction::ALL`].
    #[must_use]
    pub fn as_slice(&self) -> &[(HotkeyAction, HotkeyChord)] {
        &self.table
    }

    /// These bindings as a system that chooses the chords holds them: the chord
    /// it gave for an action in place of the one that was asked for.
    ///
    /// For the words that name a shortcut, never to register or to save. The
    /// system's chord is not ours to check - the person chose it in a window of
    /// the desktop - so nothing here refuses one, and two actions may end up on
    /// the same chord, which then names both truthfully. An action the system
    /// gave nothing for keeps the chord asked for, and is the caller's to leave
    /// unnamed.
    #[must_use]
    pub fn as_given(self, given: &[(HotkeyAction, HotkeyChord)]) -> Self {
        let mut table = self.table;
        for (action, chord) in given {
            if let Some(at) = table.iter().position(|(candidate, _)| candidate == action) {
                table[at].1 = *chord;
            }
        }
        Self { table, ..self }
    }

    /// Why a keyboard layout of this system refuses `chord` as a global
    /// shortcut, if it does: `types` names a character the layout makes the
    /// chord type, so taking the chord would take that character from every
    /// application.
    ///
    /// `types` is the system's answer (the core knows no layout) and it is
    /// asked ONLY for a chord the convention says can type ([`Convention::can_type`]):
    /// an `Alt+Shift` chord is never `AltGr` whatever a layout does with
    /// `Ctrl+Alt` on its key, and a `Cmd+Option` chord types nothing. `None`
    /// from `types` means "no reason to refuse", so a chord is refused on a
    /// character the answer can name, never on a guess. The one place this rule
    /// stands, for the settings file and for the window that records a chord.
    #[must_use]
    pub fn typing_refusal(
        &self,
        chord: HotkeyChord,
        types: &dyn Fn(HotkeyChord) -> Option<char>,
    ) -> Option<Refusal> {
        let may_type = self.convention.can_type(&chord);
        let character = may_type.then(|| types(chord)).flatten()?;
        Some(match self.convention {
            // Linux never gets here: `can_type` is false there, so `character` is
            // `None`. The arm names the refusal that would be true of `Ctrl+Alt`.
            Convention::Windows | Convention::Linux => Refusal::TypesCharacter(character),
            Convention::MacOs => Refusal::OptionTypesCharacter(character),
        })
    }

    /// These bindings with the tester's `wanted` chords on top, and every
    /// wanted chord that could not be used, with the reason.
    ///
    /// A wanted chord is refused when:
    /// - it has a [`HotkeyChord::problem`] - it would take a key from every
    ///   application.
    /// - a layout types a character with it ([`Bindings::typing_refusal`]) - it
    ///   would take that character from every application: `AltGr` on Windows,
    ///   `Option` on macOS.
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
    pub fn with(
        self,
        wanted: &[(HotkeyAction, HotkeyChord)],
        types: &dyn Fn(HotkeyChord) -> Option<char>,
    ) -> (Self, Vec<Refused>) {
        let before = self.table;
        let mut table = self.table;
        let mut chosen = [false; HotkeyAction::COUNT];
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
            if let Some(why) = self.typing_refusal(*chord, types) {
                refused.push(Refused {
                    action: *action,
                    chord: *chord,
                    why,
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
        (Self { table, ..self }, refused)
    }
}

/// The position of the wanted chord to send back next, if any clashes: first
/// one shared with an action that is not changing, then the later of two
/// wanted chords that are the same.
fn first_clash(
    table: &[(HotkeyAction, HotkeyChord); HotkeyAction::COUNT],
    chosen: &[bool; HotkeyAction::COUNT],
) -> Option<usize> {
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
    /// A `Ctrl+Alt` chord that is `AltGr` for this character on a layout the
    /// system has - taken globally, the character is gone from every
    /// application.
    TypesCharacter(char),
    /// A chord with `Option` that types this character on a layout the system
    /// has (`D141`) - the same loss, on macOS.
    OptionTypesCharacter(char),
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

    /// The default chord for one action, if the default table names it. Read
    /// straight from the table, so the tests below can hold the table and
    /// [`Bindings::defaults`] to each other.
    fn default_chord(convention: Convention, action: HotkeyAction) -> Option<HotkeyChord> {
        default_bindings(convention)
            .iter()
            .find_map(|(candidate, chord)| (*candidate == action).then_some(*chord))
    }

    const CONVENTIONS: [Convention; 3] =
        [Convention::Windows, Convention::Linux, Convention::MacOs];

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
    fn every_default_on_windows_is_alt_shift_and_never_ctrl_alt() {
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
    fn every_default_on_linux_is_ctrl_alt_and_never_alt_shift() {
        // D145: the layout toggles of X11 take Alt+Shift whole (`OBS-215`), and
        // AltGr is a key of its own there, so Ctrl+Alt takes nothing from a layout.
        for (action, chord) in LINUX_DEFAULT_BINDINGS {
            assert!(
                chord.ctrl && chord.alt && !chord.shift && !chord.win,
                "{action:?} Linux default is not Ctrl+Alt: {chord:?}"
            );
        }
    }

    #[test]
    fn each_convention_answers_with_its_own_table() {
        for (convention, table) in [
            (Convention::Windows, &DEFAULT_BINDINGS),
            (Convention::Linux, &LINUX_DEFAULT_BINDINGS),
            (Convention::MacOs, &MACOS_DEFAULT_BINDINGS),
        ] {
            assert_eq!(default_bindings(convention), table, "{convention:?}");
            assert_eq!(
                Bindings::defaults(convention).as_slice(),
                table,
                "{convention:?}"
            );
        }
        assert_eq!(LINUX_DEFAULT_BINDINGS[0].1.text(), "Ctrl+Alt+N");
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
    fn the_conventions_bind_the_same_key_to_the_same_action_but_the_one_the_system_holds() {
        // Only the modifiers differ, so a tester moving between systems keeps
        // the letters - and a table edited on one side only is caught here. The
        // exception is the value window: `Option+Command+Space` is Finder's.
        for ((action_a, chord_a), (action_b, chord_b)) in
            DEFAULT_BINDINGS.iter().zip(MACOS_DEFAULT_BINDINGS.iter())
        {
            assert_eq!(action_a, action_b);
            if *action_a == HotkeyAction::OpenPacks {
                assert_eq!(chord_a.key, HotkeyKey::Space);
                assert_eq!(chord_b.key, HotkeyKey::F);
            } else {
                assert_eq!(chord_a.key, chord_b.key, "{action_a:?}");
            }
        }
        for ((action_a, chord_a), (action_b, chord_b)) in
            DEFAULT_BINDINGS.iter().zip(LINUX_DEFAULT_BINDINGS.iter())
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
    fn the_screen_says_the_words_of_the_system_and_the_parser_reads_every_one_back() {
        let all = chord(true, true, true, true, HotkeyKey::F12);
        assert_eq!(
            all.display(Convention::MacOs),
            "Control+Option+Shift+Command+F12"
        );
        assert_eq!(all.display(Convention::Windows), all.text());
        assert_eq!(
            all.display(Convention::Linux),
            "Ctrl+Alt+Shift+Super+F12",
            "a Linux keyboard has a Super key, not a Win key"
        );
        assert_eq!(
            MACOS_DEFAULT_BINDINGS[0].1.display(Convention::MacOs),
            "Option+Command+N"
        );
        assert_eq!(
            LINUX_DEFAULT_BINDINGS[0].1.display(Convention::Linux),
            "Ctrl+Alt+N"
        );
        // What the screen shows is what the tester may write to change it, for
        // every key, every combination of modifiers and every convention.
        for key in HotkeyKey::ALL {
            for bits in 0u8..16 {
                let shown = chord(
                    bits & 1 != 0,
                    bits & 2 != 0,
                    bits & 4 != 0,
                    bits & 8 != 0,
                    *key,
                );
                for convention in CONVENTIONS {
                    assert_eq!(
                        HotkeyChord::parse(&shown.display(convention)),
                        Ok(shown),
                        "{convention:?} {shown:?}"
                    );
                }
            }
        }
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

    /// A system whose layouts type nothing under `AltGr`.
    fn no_layout(_: HotkeyChord) -> Option<char> {
        None
    }

    /// A layout like the Polish programmer's one on a single key: `AltGr+A`
    /// types `U+0105`, every other key nothing. Answers for ANY chord it is
    /// asked about, `Alt+Shift+A` included - so a core that asked about a chord
    /// that cannot be `AltGr` would refuse it, and the tests below would see.
    fn altgr_on_a(chord: HotkeyChord) -> Option<char> {
        (chord.key == HotkeyKey::A).then_some('\u{105}')
    }

    const DEFAULTS: Bindings = Bindings::defaults(Convention::Windows);

    #[test]
    fn a_ctrl_alt_chord_the_layout_types_on_is_refused_with_the_character() {
        let wanted = HotkeyChord::parse("Ctrl+Alt+A").expect("reads");
        let (bindings, refused) = DEFAULTS.with(&[(HotkeyAction::NextValue, wanted)], &altgr_on_a);
        assert_eq!(bindings, DEFAULTS);
        assert_eq!(
            refused,
            vec![Refused {
                action: HotkeyAction::NextValue,
                chord: wanted,
                why: Refusal::TypesCharacter('\u{105}'),
            }]
        );
    }

    #[test]
    fn a_ctrl_alt_chord_the_layout_types_nothing_on_is_used() {
        let wanted = HotkeyChord::parse("Ctrl+Alt+F5").expect("reads");
        let (bindings, refused) = DEFAULTS.with(&[(HotkeyAction::NextValue, wanted)], &altgr_on_a);
        assert!(refused.is_empty(), "{refused:?}");
        assert_eq!(bindings.chord(HotkeyAction::NextValue), wanted);
    }

    #[test]
    fn the_layout_is_asked_only_about_a_chord_that_can_be_altgr() {
        let asked = std::cell::RefCell::new(Vec::new());
        let every_key_types = |chord: HotkeyChord| {
            asked.borrow_mut().push(chord);
            Some('x')
        };
        // A combination no default holds, so taking it moves nothing else.
        let alt_shift_k = alt_shift(HotkeyKey::K);
        let with_win = HotkeyChord::parse("Ctrl+Alt+Win+A").expect("reads");
        let altgr = HotkeyChord::parse("Ctrl+Alt+Shift+A").expect("reads");
        let (bindings, refused) = DEFAULTS.with(
            &[
                (HotkeyAction::NextValue, alt_shift_k),
                (HotkeyAction::PreviousValue, with_win),
                (HotkeyAction::RepeatLast, altgr),
            ],
            &every_key_types,
        );
        assert_eq!(*asked.borrow(), vec![altgr]);
        assert_eq!(bindings.chord(HotkeyAction::NextValue), alt_shift_k);
        assert_eq!(bindings.chord(HotkeyAction::PreviousValue), with_win);
        assert_eq!(
            refused,
            vec![Refused {
                action: HotkeyAction::RepeatLast,
                chord: altgr,
                why: Refusal::TypesCharacter('x'),
            }]
        );
    }

    #[test]
    fn two_actions_wanting_one_altgr_chord_are_both_refused_for_the_character() {
        // Refused before the clash is looked at: the sentence names the real
        // reason, not "already the shortcut for" an action that never got it.
        let wanted = HotkeyChord::parse("Ctrl+Alt+A").expect("reads");
        let (bindings, refused) = DEFAULTS.with(
            &[
                (HotkeyAction::NextValue, wanted),
                (HotkeyAction::PreviousValue, wanted),
            ],
            &altgr_on_a,
        );
        assert_eq!(bindings, DEFAULTS);
        assert_eq!(refused.len(), 2, "{refused:?}");
        for refusal in &refused {
            assert_eq!(refusal.why, Refusal::TypesCharacter('\u{105}'));
        }
    }

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

    /// What a desktop gave is named as it gave it, for the actions it gave, and
    /// nothing else moves: the table keeps every action once, in its order.
    #[test]
    fn the_chords_a_system_gave_are_named_in_place_of_the_ones_asked_for() {
        let k = chord(true, false, true, false, HotkeyKey::K);
        let bindings = DEFAULTS.as_given(&[(HotkeyAction::PreviousValue, k)]);
        assert_eq!(bindings.chord(HotkeyAction::PreviousValue), k);
        for action in HotkeyAction::ALL {
            if action != HotkeyAction::PreviousValue {
                assert_eq!(bindings.chord(action), DEFAULTS.chord(action));
            }
        }
        let actions: Vec<HotkeyAction> = bindings.as_slice().iter().map(|(a, _)| *a).collect();
        assert_eq!(actions, HotkeyAction::ALL);
        assert_eq!(
            DEFAULTS.as_given(&[]),
            DEFAULTS,
            "nothing given, nothing moved"
        );
    }

    /// The system's own choice is not checked: a chord the settings would
    /// refuse is named when the desktop holds it, and so is one two actions share.
    #[test]
    fn a_chord_the_system_holds_is_named_even_where_the_settings_would_refuse_it() {
        let alone = chord(false, false, false, false, HotkeyKey::K);
        let shared = DEFAULTS.chord(HotkeyAction::NextValue);
        let bindings = DEFAULTS.as_given(&[
            (HotkeyAction::RepeatLast, alone),
            (HotkeyAction::PreviousValue, shared),
        ]);
        assert_eq!(bindings.chord(HotkeyAction::RepeatLast), alone);
        assert!(alone.problem().is_some(), "a chord the file would refuse");
        assert_eq!(bindings.chord(HotkeyAction::PreviousValue), shared);
        assert_eq!(bindings.chord(HotkeyAction::NextValue), shared);
    }

    #[test]
    fn a_wanted_chord_replaces_the_default_and_the_rest_stay() {
        let m = alt_shift(HotkeyKey::M);
        let (bindings, refused) = DEFAULTS.with(&[(HotkeyAction::NextValue, m)], &no_layout);
        assert!(refused.is_empty(), "{refused:?}");
        assert_eq!(bindings.chord(HotkeyAction::NextValue), m);
        for action in &HotkeyAction::ALL[1..] {
            assert_eq!(bindings.chord(*action), DEFAULTS.chord(*action));
        }
    }

    #[test]
    fn a_chord_that_would_take_a_key_everywhere_is_refused() {
        let plain = HotkeyChord::parse("Shift+M").expect("reads");
        let (bindings, refused) = DEFAULTS.with(&[(HotkeyAction::NextValue, plain)], &no_layout);
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
        let (bindings, refused) = DEFAULTS.with(&[(HotkeyAction::NextValue, p)], &no_layout);
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
    fn the_reading_order_holds_every_action_once() {
        let mut read: Vec<&str> = HotkeyAction::READING_ORDER.iter().map(|a| a.id()).collect();
        let mut all: Vec<&str> = HotkeyAction::ALL.iter().map(|a| a.id()).collect();
        read.sort_unstable();
        all.sort_unstable();
        assert_eq!(read, all);
        read.dedup();
        assert_eq!(read.len(), HotkeyAction::COUNT, "an action is read twice");
    }

    #[test]
    fn two_actions_may_swap_their_chords() {
        let (n, p) = (alt_shift(HotkeyKey::N), alt_shift(HotkeyKey::P));
        let (bindings, refused) = DEFAULTS.with(
            &[
                (HotkeyAction::NextValue, p),
                (HotkeyAction::PreviousValue, n),
            ],
            &no_layout,
        );
        assert!(refused.is_empty(), "{refused:?}");
        assert_eq!(bindings.chord(HotkeyAction::NextValue), p);
        assert_eq!(bindings.chord(HotkeyAction::PreviousValue), n);
    }

    #[test]
    fn of_two_actions_wanting_one_chord_the_later_goes_back() {
        // A combination no default holds - `X` has been `NextPack`'s since `D120`.
        let x = alt_shift(HotkeyKey::J);
        let (bindings, refused) = DEFAULTS.with(
            &[(HotkeyAction::MarkOk, x), (HotkeyAction::NextValue, x)],
            &no_layout,
        );
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
        let (bindings, refused) = DEFAULTS.with(
            &[
                (HotkeyAction::NextValue, r),
                (HotkeyAction::PreviousValue, r),
                (HotkeyAction::RepeatLast, p),
            ],
            &no_layout,
        );
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
        // clashes with an action outside the three, one with a problem, and
        // two `Ctrl+Alt` chords - one the layout types a character on.
        let pool = [
            None,
            Some(alt_shift(HotkeyKey::N)),
            Some(alt_shift(HotkeyKey::P)),
            Some(alt_shift(HotkeyKey::R)),
            Some(alt_shift(HotkeyKey::X)),
            Some(alt_shift(HotkeyKey::Y)),
            Some(alt_shift(HotkeyKey::B)),
            Some(HotkeyChord::parse("Shift+Z").expect("reads")),
            // `AltGr` for a character of the layout below, and `Ctrl+Alt` on a
            // key the layout leaves empty.
            Some(HotkeyChord::parse("Ctrl+Alt+A").expect("reads")),
            Some(HotkeyChord::parse("Ctrl+Alt+F5").expect("reads")),
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
                    let (bindings, refused) = DEFAULTS.with(&wanted, &altgr_on_a);
                    let table = bindings.as_slice();
                    for (i, (_, one)) in table.iter().enumerate() {
                        assert_eq!(one.problem(), None, "{wanted:?}");
                        assert!(
                            !(one.could_be_altgr() && altgr_on_a(*one).is_some()),
                            "{wanted:?} -> {table:?}"
                        );
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
                                match refusal.why {
                                    Refusal::SameAs(owner) => {
                                        assert_eq!(bindings.chord(owner), *chord, "{wanted:?}");
                                    }
                                    Refusal::TypesCharacter(character) => {
                                        assert!(chord.could_be_altgr(), "{wanted:?}");
                                        assert_eq!(altgr_on_a(*chord), Some(character));
                                    }
                                    Refusal::OptionTypesCharacter(_) => {
                                        panic!("Windows asks no Option question: {wanted:?}")
                                    }
                                    Refusal::Problem(problem) => {
                                        assert_eq!(chord.problem(), Some(problem));
                                    }
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
        // Linux defaults have the shape on purpose: there it is not `AltGr`.
        for convention in [Convention::Windows, Convention::MacOs] {
            for (action, default) in default_bindings(convention) {
                assert!(!default.could_be_altgr(), "{convention:?} {action:?}");
            }
        }
    }

    /// A layout like the Polish one on a single key: `Option+A` types `U+0105`,
    /// every other key nothing. Answers for ANY chord it is asked about, so a
    /// core that asked about a chord that cannot type would refuse it.
    fn option_on_a(chord: HotkeyChord) -> Option<char> {
        (chord.key == HotkeyKey::A).then_some('\u{105}')
    }

    const MAC_DEFAULTS: Bindings = Bindings::defaults(Convention::MacOs);

    #[test]
    fn an_option_chord_the_layout_types_on_is_refused_with_the_character() {
        // With and without Shift: Option+Shift types the capital.
        for text in ["Option+A", "Option+Shift+A"] {
            let wanted = HotkeyChord::parse(text).expect("reads");
            let (bindings, refused) =
                MAC_DEFAULTS.with(&[(HotkeyAction::NextValue, wanted)], &option_on_a);
            assert_eq!(bindings, MAC_DEFAULTS, "{text}");
            assert_eq!(
                refused,
                vec![Refused {
                    action: HotkeyAction::NextValue,
                    chord: wanted,
                    why: Refusal::OptionTypesCharacter('\u{105}'),
                }],
                "{text}"
            );
        }
    }

    #[test]
    fn an_option_chord_the_layout_types_nothing_on_is_used() {
        let wanted = HotkeyChord::parse("Option+F5").expect("reads");
        let (bindings, refused) =
            MAC_DEFAULTS.with(&[(HotkeyAction::NextValue, wanted)], &option_on_a);
        assert!(refused.is_empty(), "{refused:?}");
        assert_eq!(bindings.chord(HotkeyAction::NextValue), wanted);
    }

    #[test]
    fn macos_asks_the_layout_only_about_option_without_command_or_control() {
        let asked = std::cell::RefCell::new(Vec::new());
        let every_key_types = |chord: HotkeyChord| {
            asked.borrow_mut().push(chord);
            Some('x')
        };
        let with_command = HotkeyChord::parse("Command+Option+K").expect("reads");
        let with_control = HotkeyChord::parse("Control+Option+K").expect("reads");
        let ctrl_alt = HotkeyChord::parse("Ctrl+Alt+Shift+K").expect("reads");
        let command_only = HotkeyChord::parse("Command+K").expect("reads");
        let option = HotkeyChord::parse("Option+Shift+K").expect("reads");
        let (bindings, refused) = MAC_DEFAULTS.with(
            &[
                (HotkeyAction::NextValue, with_command),
                (HotkeyAction::PreviousValue, with_control),
                (HotkeyAction::RepeatLast, ctrl_alt),
                (HotkeyAction::RestartPack, command_only),
                (HotkeyAction::CopyReport, option),
            ],
            &every_key_types,
        );
        assert_eq!(*asked.borrow(), vec![option]);
        for (action, chord) in [
            (HotkeyAction::NextValue, with_command),
            (HotkeyAction::PreviousValue, with_control),
            (HotkeyAction::RepeatLast, ctrl_alt),
            (HotkeyAction::RestartPack, command_only),
        ] {
            assert_eq!(bindings.chord(action), chord, "{chord:?}");
        }
        assert_eq!(
            refused,
            vec![Refused {
                action: HotkeyAction::CopyReport,
                chord: option,
                why: Refusal::OptionTypesCharacter('x'),
            }]
        );
    }

    #[test]
    fn each_convention_asks_its_own_question_and_not_the_other() {
        // `Alt` alone is the Option shape and `Ctrl+Alt` the AltGr one. Asking
        // Windows about `Alt+A` would refuse a shortcut that takes nothing, and
        // asking macOS about `Ctrl+Alt+A` would refuse one the applications
        // decide.
        let option = HotkeyChord::parse("Alt+A").expect("reads");
        let altgr = HotkeyChord::parse("Ctrl+Alt+A").expect("reads");
        let (bindings, refused) = DEFAULTS.with(&[(HotkeyAction::NextValue, option)], &option_on_a);
        assert!(refused.is_empty(), "{refused:?}");
        assert_eq!(bindings.chord(HotkeyAction::NextValue), option);
        let (bindings, refused) =
            MAC_DEFAULTS.with(&[(HotkeyAction::NextValue, altgr)], &option_on_a);
        assert!(refused.is_empty(), "{refused:?}");
        assert_eq!(bindings.chord(HotkeyAction::NextValue), altgr);
    }

    #[test]
    fn typing_refusal_is_worded_for_the_key_of_the_convention() {
        let altgr = HotkeyChord::parse("Ctrl+Alt+A").expect("reads");
        let option = HotkeyChord::parse("Option+A").expect("reads");
        assert_eq!(
            DEFAULTS.typing_refusal(altgr, &altgr_on_a),
            Some(Refusal::TypesCharacter('\u{105}'))
        );
        assert_eq!(
            MAC_DEFAULTS.typing_refusal(option, &option_on_a),
            Some(Refusal::OptionTypesCharacter('\u{105}'))
        );
        assert_eq!(DEFAULTS.typing_refusal(altgr, &no_layout), None);
        assert_eq!(MAC_DEFAULTS.typing_refusal(option, &no_layout), None);
    }

    #[test]
    fn only_option_without_command_or_control_can_type_with_option() {
        let option = ["Option+N", "Option+Shift+N", "Alt+Space"];
        let not = [
            "Command+Option+N",
            "Control+Option+N",
            "Command+Option+Shift+N",
            "Alt+Shift+Ctrl+N",
            "Shift+N",
            "Command+N",
        ];
        for text in option {
            assert!(
                HotkeyChord::parse(text)
                    .expect("reads")
                    .could_type_with_option(),
                "{text}"
            );
        }
        for text in not {
            assert!(
                !HotkeyChord::parse(text)
                    .expect("reads")
                    .could_type_with_option(),
                "{text}"
            );
        }
    }

    #[test]
    fn no_default_of_any_convention_can_type_a_character() {
        // A default that could type would be refused at the first run, or
        // worse, registered and taking a character from every application.
        for convention in CONVENTIONS {
            for (action, default) in default_bindings(convention) {
                assert!(
                    !convention.can_type(default),
                    "{convention:?} {action:?} {default:?}"
                );
            }
        }
    }

    #[test]
    fn the_question_of_each_convention_is_the_shape_of_its_own_key() {
        let altgr = HotkeyChord::parse("Ctrl+Alt+N").expect("reads");
        let option = HotkeyChord::parse("Option+N").expect("reads");
        assert!(Convention::Windows.can_type(&altgr));
        assert!(!Convention::Windows.can_type(&option));
        assert!(Convention::MacOs.can_type(&option));
        assert!(!Convention::MacOs.can_type(&altgr));
        // `AltGr` is a key of its own on Linux and `Alt` types nothing there.
        assert!(!Convention::Linux.can_type(&altgr));
        assert!(!Convention::Linux.can_type(&option));
    }

    #[test]
    fn a_layout_is_never_asked_about_a_chord_on_linux() {
        // `altgr_on_a` answers for any chord it is asked about, so a refusal here
        // would mean the core asked.
        let linux = Bindings::defaults(Convention::Linux);
        // The first has the shape `AltGr` has on Windows, so the same wish there
        // is refused with the character.
        let ctrl_alt_shift_a = HotkeyChord::parse("Ctrl+Alt+Shift+A").expect("reads");
        let alt_a = HotkeyChord::parse("Alt+A").expect("reads");
        assert_eq!(
            DEFAULTS
                .with(&[(HotkeyAction::NextValue, ctrl_alt_shift_a)], &altgr_on_a)
                .1
                .len(),
            1
        );
        for wanted in [ctrl_alt_shift_a, alt_a] {
            let (bindings, refused) = linux.with(&[(HotkeyAction::NextValue, wanted)], &altgr_on_a);
            assert_eq!(refused, vec![], "{wanted:?}");
            assert_eq!(bindings.chord(HotkeyAction::NextValue), wanted);
        }
    }
}
