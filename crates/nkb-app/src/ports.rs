//! Ports: what this layer needs from the outside, stated as traits.
//!
//! Two things are absent on purpose and their absence is the point.
//!
//! There is **no network port**. Not "unused", not "disabled" - absent, so that
//! the promise of never sending anything anywhere is a property of the shape of
//! this program rather than a sentence in a document that nobody can check.
//!
//! There is **no port for reading a target field**. The tool is deliberately
//! blind until the version that introduces the input oracle, and leaving the
//! capability undeclared means it cannot be reached for by accident.

use nkb_core::hotkeys::{HotkeyAction, HotkeyChord};
use nkb_core::keys::KeyChord;
use nkb_core::lint::LintProblem;
use nkb_core::pack::Pack;
use std::fmt;
use std::time::Duration;

/// Why a pack could not be provided. Carries no path and no free text, because
/// the layer that reports this decides how much to reveal to a person.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceError {
    /// Nothing to read at the requested location.
    NotFound,
    /// Present, but unreadable - permissions, a device error, a broken handle.
    Unreadable,
    /// Read, but not valid UTF-8, which the pack format requires.
    NotUtf8,
}

impl fmt::Display for SourceError {
    /// Deliberately terse and English-only: this is a developer-facing marker,
    /// not the text a user reads. User-visible wording is assembled one layer
    /// out, from translation keys.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let marker = match self {
            Self::NotFound => "not-found",
            Self::Unreadable => "unreadable",
            Self::NotUtf8 => "not-utf8",
        };
        f.write_str(marker)
    }
}

/// How much of a value actually reached the field.
///
/// Counted in UTF-16 code units rather than characters, and that is not an
/// implementation detail leaking upwards - it is the only count that cannot
/// lie. A character above the basic plane crosses as a surrogate PAIR, so a
/// delivery cut short can end between the halves; reporting "seven characters
/// arrived" would then be a guess about something that is not a character.
/// The layer that talks to a person converts, and says so.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Delivered {
    pub utf16_units: usize,
}

/// Why a value did not reach the field, or did not reach all of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeliveryError {
    /// This route does not exist on this system. Named, never a shrug -
    /// untouchable rule 1.
    Unsupported { system: String },
    /// Nothing holds the keyboard focus, so there is nowhere to deliver to.
    NoTarget,
    /// A modifier is physically held and did not come up in time, so nothing
    /// was sent: under a held modifier the value's characters mean something
    /// else to many applications. The ordinary case right after a hotkey.
    ModifierHeld { which: String },
    /// Part of the value arrived. The field now holds a fragment, and saying so
    /// is the entire reason this variant is separate from the others: a tool
    /// that reported plain failure here would leave a half-written value in
    /// somebody else's form looking like that application's own doing.
    Partial {
        units_sent: usize,
        units_expected: usize,
    },
}

impl fmt::Display for DeliveryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported { system } => write!(f, "unsupported-on-{system}"),
            Self::NoTarget => f.write_str("no-target"),
            Self::ModifierHeld { which } => write!(f, "modifier-held-{which}"),
            Self::Partial {
                units_sent,
                units_expected,
            } => {
                write!(f, "partial-{units_sent}-of-{units_expected}")
            }
        }
    }
}

/// An opaque handle to whatever will receive the value.
///
/// A number, never a name. `TargetInspector` and the `WindowTitle` type are what
/// will one day carry an application name and a window title, precisely because
/// those need a type that hides them by default - architektura.md section 5.
/// This carries neither: two of these can be compared, and nothing else.
///
/// # Why the port needs it at all
///
/// Measured 2026-09-08, and it is the reason this method exists: `nkb send` run
/// from a terminal reported `sent` while the characters went into the terminal
/// itself. The system had accepted the events, so nothing was false - and the
/// message was still read as "the value is in the field you meant". Untouchable
/// rule 1 says a run that did less than it promised must say so, and without a
/// handle to compare there is nothing to say it with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TargetRef(pub u64);

/// Whether a delivery route can be used here at all.
///
/// Asked BEFORE the work of building a value, so that a system without a route
/// says so instead of failing after the fact. This is also what step 5 of the
/// plan turns into the `degraded` state and the clipboard fallback - but it
/// earns its place today, because macOS and Linux have no route yet and the
/// tool has to be able to say which.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Availability {
    Ready,
    Unavailable { reason: String },
}

/// Puts a value into whatever field currently has the keyboard focus.
///
/// # Why one port with two implementations rather than two paths
///
/// architektura.md section 3 is explicit: `DirectInjection` and
/// `ClipboardDelivery` are two implementations of THIS trait, not two branches
/// in a caller. The moment a caller writes `if clipboard_mode { ... } else`,
/// every later feature has to be written twice and the second copy drifts.
///
/// # What this port deliberately cannot do
///
/// It cannot read. There is no method here that returns what a field contains,
/// or what window is in front, or what any of it is called - architektura.md
/// section 5 makes "does not read the contents of windows" hold because the
/// capability is *undeclared*, and this trait is one of the places where that
/// could quietly stop being true.
pub trait ValueDelivery {
    /// Whether this route works on this machine, asked before any work.
    fn availability(&self) -> Availability;

    /// What would receive a value right now, if anything would.
    ///
    /// Asked twice around a wait, so a caller can notice that nothing moved -
    /// which usually means the value is about to go back into the window it was
    /// launched from.
    fn target(&self) -> Option<TargetRef>;

    /// Sends `text` to the focused field.
    ///
    /// # Errors
    ///
    /// Returns [`DeliveryError`] when there is no route, no target, or when only
    /// part of the value arrived.
    fn deliver(&self, text: &str) -> Result<Delivered, DeliveryError>;
}

/// Why a keystroke sequence was not sent, or not all of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeystrokeError {
    /// This route does not exist on this system. Named, never a shrug.
    Unsupported { system: String },
    /// Nothing holds the keyboard focus, so there is nowhere to press keys.
    NoTarget,
    /// A modifier is physically held on the keyboard and did not come up in
    /// time. The chords were NOT sent: `Home` under a held `Ctrl` is the start
    /// of the document, and `Shift+End` under it is the end of the document,
    /// so the sequence would have selected and deleted far beyond the field.
    /// This is the one race the recipe itself cannot see, and refusing is the
    /// only answer that keeps untouchable rule 17.
    ModifierHeld { which: String },
    /// The system accepted fewer presses than it was handed. The field is in
    /// an unknown state between "untouched" and "cleared", and saying so is
    /// what lets the caller refuse to send a value on top of it.
    Partial {
        chords_sent: usize,
        chords_expected: usize,
    },
}

impl fmt::Display for KeystrokeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported { system } => write!(f, "unsupported-on-{system}"),
            Self::NoTarget => f.write_str("no-target"),
            Self::ModifierHeld { which } => write!(f, "modifier-held-{which}"),
            Self::Partial {
                chords_sent,
                chords_expected,
            } => write!(f, "partial-{chords_sent}-of-{chords_expected}"),
        }
    }
}

/// Presses keys that are NOT content.
///
/// # Exactly two doors, and today one
///
/// `ux-spec.md` 4: clearing the field is the only place where the tool sends
/// keystrokes other than the value, and architektura.md 5 turns that into a
/// count - this trait is to be called from exactly two places, clearing and
/// the paste of the clipboard route, and from nowhere else. The paste does not
/// exist yet, so today the count is ONE, and
/// `crates/nkb-app/tests/keystrokes_have_named_doors.rs` is the guard that
/// names the places and goes red when a third appears.
///
/// # What the vocabulary cannot say
///
/// The argument is a slice of [`KeyChord`], whose only keys are `Home`, `End`
/// and `Delete` and whose only modifier is `Shift`. "Select all" is not a
/// thing this port can be asked for - see `nkb_core::keys`.
///
/// # Errors
///
/// A physically held modifier is a refusal, not a wait forever: the
/// implementation gives it a bounded moment to come up and then returns
/// [`KeystrokeError::ModifierHeld`] without pressing anything.
pub trait KeystrokeSender {
    /// Presses `chords` in order, each as a full press-and-release.
    ///
    /// # Errors
    ///
    /// Returns [`KeystrokeError`] when there is no route, no target, a held
    /// modifier, or when only part of the sequence was accepted.
    fn send_keystrokes(&self, chords: &[KeyChord]) -> Result<(), KeystrokeError>;
}

/// Supplies the raw text of a pack file.
///
/// Returns text rather than a parsed pack on purpose: parsing belongs to the
/// adapter that knows the file format, and a validator has to see the file as
/// it actually is - including the parts a lenient parser would forgive.
pub trait PackSource {
    /// Reads one pack by an identifier the implementation understands.
    ///
    /// # Errors
    ///
    /// Returns [`SourceError`] when the pack is missing, unreadable, or not
    /// valid UTF-8.
    fn read(&self, id: &str) -> Result<String, SourceError>;
}

/// A place that knows WHICH packs exist, not merely how to read one.
///
/// # Why this is a second trait rather than a method on [`PackSource`]
///
/// The two are different needs and only one of them is universal. `nkb lint` and
/// `nkb fmt` are handed a path and read exactly one file: they never ask what
/// else is around, and a source that could only be used by something able to
/// enumerate would be the wrong shape for them. `nkb packs` is the opposite: it
/// has no path at all and the enumeration IS the answer.
///
/// The supertrait says the part that is genuinely true - anything able to list
/// packs can also read one - without forcing the reverse.
pub trait PackCatalogue: PackSource {
    /// Every pack identifier this catalogue holds, in a stable order.
    ///
    /// Stable rather than sorted-here, so that a source with a meaningful order
    /// of its own may keep it. What must not happen is the order changing
    /// between two runs over unchanged data: `nkb packs` output goes into other
    /// people's scripts, and a listing that shuffles turns a diff into noise.
    ///
    /// # Errors
    ///
    /// Returns [`SourceError`] when the catalogue itself cannot be examined.
    /// A catalogue that is present and holds nothing returns an empty list,
    /// which is a different answer and is never folded into the error.
    fn list(&self) -> Result<Vec<String>, SourceError>;

    /// Which of the sources described by `pack-format.md` 12 this stands for,
    /// and which ones were not consulted at all.
    ///
    /// 🔴 Here because untouchable rule 1 forbids the silence, not because a
    /// caller asked. The format defines THREE sources - built in, team, and the
    /// user's own - loaded in that order, with a later one overriding an earlier
    /// one by `pack.id`. A build that consults one of the three and prints a
    /// list looking exactly like a complete one is a run that did less than it
    /// appeared to, which is the one thing a run may never do.
    fn coverage(&self) -> CatalogueCoverage;
}

/// The three places packs come from, and whether this run looked at them.
///
/// A pack the tool did not see is indistinguishable, in a listing, from a pack
/// that does not exist. This type is what lets the difference be printed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogueCoverage {
    /// Sources that were read, named by the format's own words.
    pub consulted: Vec<CatalogueSource>,
    /// Sources that were not, each with the reason - which is never "no reason".
    pub skipped: Vec<(CatalogueSource, SourceSkipped)>,
}

/// One of the three sources `pack-format.md` 12 defines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogueSource {
    /// Delivered with the tool.
    BuiltIn,
    /// A folder named in settings, shared by a team.
    Team,
    /// The user's own folder.
    Own,
}

impl CatalogueSource {
    /// The stable word for machine readable output.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::BuiltIn => "built-in",
            Self::Team => "team",
            Self::Own => "own",
        }
    }
}

/// Why a source was not read. Never a bare "no".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceSkipped {
    /// This build has no settings file yet, so the folder cannot be named.
    NotImplementedYet,
    /// Configured, but nothing is at that location.
    NotConfigured,
    /// Configured and present, and the read failed.
    Unreadable,
}

impl SourceSkipped {
    /// The stable marker for machine readable output.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotImplementedYet => "not-implemented-yet",
            Self::NotConfigured => "not-configured",
            Self::Unreadable => "unreadable",
        }
    }
}

/// Answers the questions that only a parsed pack file can answer.
///
/// # Why this is a port and not a function call
///
/// The layer above owns the order of the checks and the shape of the verdict.
/// The file format belongs one layer out: which parser, which syntax, and where
/// a line number comes from. Stating that boundary as a trait is what lets this
/// use case be exercised with no parser at all, and what keeps the dependency
/// arrow pointing one way rather than resting on somebody remembering it.
///
/// # Two directions, and they are not the same reading
///
/// Most of what follows exists to find fault: it reads a file AS WRITTEN,
/// including the parts a lenient reader would forgive, and returns problems
/// rather than a tidy model. [`PackFormat::parse`] is the other direction - it
/// reads a file AS MEANT and returns the pack.
///
/// ⚠️ Until 2026-09-08 this paragraph said the port does not return a pack, and
/// that was true of every method there was. Keeping the two directions in one
/// trait is deliberate: they are the same knowledge about the same file format,
/// and splitting them would mean two names for one boundary. What must not blur
/// is the order of use - a caller runs `check` first and refuses the pack on any
/// error, because `pack-format.md` 11 loads all of a pack or none of it.
pub trait PackFormat {
    /// Returns every problem the parsed file reveals.
    ///
    /// Reports all of them rather than the first, with the single exception of a
    /// file that does not parse - there is nothing to look inside, so that one
    /// problem comes back alone.
    ///
    /// `expected_id` is the identifier the pack was read under, which for every
    /// implementation of [`PackSource`] that exists is the file name without its
    /// extension. The format requires a pack to declare that same identifier, so
    /// the check needs both halves - and only this layer knows the first one,
    /// while only the implementation below knows where in the file the second one
    /// is written. Splitting the rule between them would leave the line number
    /// behind, which is the difference between a report and a shrug.
    fn check(&self, text: &str, expected_id: &str) -> Vec<LintProblem>;

    /// Which pack this file translates, when it is a translation at all.
    ///
    /// Three answers rather than two, and the third is the reason this is not an
    /// `Option`. "Not a translation" and "a translation naming something that is
    /// not a pack identifier" send the layer above to do different things: the
    /// first means the translation rules do not apply, the second means they
    /// apply and cannot be run. An `Option` would render both as nothing and lose
    /// the second, which is exactly the silence the per file record exists for.
    fn translated_pack(&self, text: &str) -> TranslationTarget;

    /// The problems that only appear when the translation and the pack it
    /// translates are seen together.
    ///
    /// Takes both as text because the layer above holds no parser and the layer
    /// below holds no file system: one of them has to carry the bytes across, and
    /// text is what the source port already deals in.
    fn check_translation(&self, text: &str, translated: &str) -> TranslationCheck;

    /// The pack this file describes, or nothing when it describes none.
    ///
    /// # The other direction of the same format
    ///
    /// Every other method here exists to find fault. This one exists to obey,
    /// and the two must not be confused: `check` is deliberately suspicious and
    /// reads the file as written, while this reads the file as meant. Nothing
    /// here reports a problem and nothing here forgives one - a caller runs
    /// `check` first and refuses on any error, because `pack-format.md` 11 says
    /// a pack with one bad value out of thirty-four loads NOTHING.
    ///
    /// `None` means the text is not a pack file at all, which `E004` has already
    /// said. It is not the answer for a pack that is merely wrong somewhere:
    /// deciding that is the caller's job and it needs the codes, not a shrug.
    fn parse(&self, text: &str) -> Option<Pack>;

    /// The file a brand new pack starts from.
    ///
    /// # Why a template belongs behind the port that reads files
    ///
    /// The trait began as "questions only a parsed file can answer", and this is
    /// not a question. It is here anyway because it is the same knowledge from
    /// the other side: what a pack file looks like. Splitting it into a port of
    /// its own would put one method behind a second name for the same boundary,
    /// and the register of ports is a thing sessions read rather than a place to
    /// file every method separately.
    ///
    /// 🔴 What comes back has to pass [`PackFormat::check`]. There is no written
    /// specification of the format - a contributor meets it through this file and
    /// through the linter's verdict - so the two disagreeing is the two halves of
    /// the published contract disagreeing in front of a stranger.
    fn skeleton(&self, id: &str, today: Date) -> String;

    /// The same file, written the one way the format writes it.
    ///
    /// `None` when the text is not a pack file at all: there is nothing inside to
    /// put in order, and E004 already says so. Told apart from `Some(unchanged)`,
    /// which means the file was read and is already in shape.
    fn canonical(&self, text: &str) -> Option<String>;

    /// Whether two versions of a file would insert the same things.
    ///
    /// 🔴 The guard on the only write path in this tool that touches a file
    /// somebody else wrote. It is asked **after** formatting and **before**
    /// saving, so that a fault in the formatter becomes a refusal instead of a
    /// silently emptied pack. The pack it would damage first is the one made of
    /// characters nobody can see, where a lost escape leaves a value that still
    /// parses and no longer tests anything.
    fn same_insertions(&self, before: &str, after: &str) -> bool;
}

/// A calendar date as year, month and day.
///
/// A tuple rather than a type of its own, because nothing in this program does
/// arithmetic on a date - it writes one into a file and reads one back. A type
/// with no behaviour is a name for a tuple, and the register of ports is long
/// enough already.
pub type Date = (i32, u32, u32);

/// Today's date, from wherever the running program gets it.
///
/// A port rather than a call to the system clock, so that a use case producing
/// dated output can be exercised against a fixed one. The core never asks for
/// the time at all: it receives the answer.
pub trait Clock {
    fn today(&self) -> Date;
}

/// Why a new pack file could not be written.
///
/// 🔴 `AlreadyExists` is the reason this port exists as its own thing rather
/// than a call to write a file. Nothing in this tool overwrites what it did not
/// create: a contributor with an hour of work in `locale-cz.toml` and a
/// half-remembered command has to get a refusal, not a fresh skeleton.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SinkError {
    /// Something is already at that name. Never overwritten, never appended to.
    AlreadyExists,
    /// The place cannot be written to: permissions, a missing folder, a device.
    Unwritable,
}

/// Writes pack files.
///
/// Separate from [`PackSource`] because the two carry opposite risks: reading a
/// pack that is not there costs a message, and writing over one that is costs
/// somebody's work.
///
/// # Two methods, because there are two kinds of write and only one is safe
///
/// [`PackSink::create`] refuses when the name is taken, and [`PackSink::replace`]
/// rewrites a file that is already there. They are separate names rather than a
/// flag on one, so that overwriting somebody's pack is something a caller has to
/// ask for by name and cannot reach by leaving a parameter at its default.
pub trait PackSink {
    /// Creates a pack file under this identifier, and refuses if one is there.
    ///
    /// # Errors
    ///
    /// Returns [`SinkError::AlreadyExists`] when the name is taken and
    /// [`SinkError::Unwritable`] when the write itself fails.
    ///
    /// 🔴 The check and the write are one operation for the implementation to
    /// make indivisible. Asking whether a file exists and then writing it is two
    /// operations with a gap in between, and something can arrive in that gap -
    /// the shape of race the architecture notes call TOCTOU.
    fn create(&self, id: &str, text: &str) -> Result<(), SinkError>;

    /// Rewrites a pack file that is already there.
    ///
    /// # Errors
    ///
    /// Returns [`SinkError::Unwritable`] when the write fails, and
    /// [`SinkError::AlreadyExists`] never - the file existing is the point here
    /// rather than the obstacle.
    ///
    /// 🔴 The write has to be atomic: content under a temporary name, then a
    /// rename into place. A formatter interrupted halfway through a plain write
    /// leaves a truncated pack that still looks like a pack, and the file it was
    /// rewriting is the only copy the contributor had.
    fn replace(&self, id: &str, text: &str) -> Result<(), SinkError>;
}

/// What a file's `translates` field points at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TranslationTarget {
    /// Not a translation: the file declares no `translates`.
    NotATranslation,
    /// A translation of the pack with this identifier, which is known to have the
    /// shape the format requires - so it may be looked for on a disk.
    ///
    /// 🔴 That guarantee is load bearing rather than tidy. A pack file comes from
    /// a stranger, and this identifier is joined to a directory to make a path.
    /// An implementation that returned raw text here would let a pack file reach
    /// outside the folder it lives in, so the shape is checked before the name
    /// leaves this port and never after.
    Pack(String),
    /// A translation whose `translates` names no pack: not text, or text outside
    /// the pack identifier alphabet. The rule about that has already been
    /// reported by [`PackFormat::check`]; nothing here can be resolved.
    Unusable,
}

/// What came of comparing a translation against the pack it translates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TranslationCheck {
    /// The two were compared. Whatever was wrong is in the list, and an empty
    /// list means the comparison happened and found nothing.
    Compared(Vec<LintProblem>),
    /// The pack being translated does not parse, so it holds no identifiers to
    /// compare against and nothing was compared.
    ///
    /// Told apart from `Compared(vec![])` on purpose: an empty comparison and an
    /// impossible one are the two answers this whole mechanism exists to keep
    /// apart. The problem is reported against **that** file when somebody lints
    /// it, not against this one.
    TranslatedPackDidNotParse,
}

/// What became of registering one global shortcut. Three answers, not two -
/// `ux-spec.md` 3 and `D55`.
///
/// The third way a registration can go wrong - accepted by the system, but the
/// event never arrives because something higher intercepts it - is NOT here,
/// because no register call can know it. It is discovered live, by the
/// shortcut-test field, and a value that claimed to know it would be a guess
/// dressed as a fact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutRegistration {
    /// The system accepted it. Whether a press actually arrives is the separate
    /// question above.
    Registered,
    /// The combination is already held - by this tool or by another
    /// application. The one failure that means "pick a different shortcut".
    Taken,
    /// Refused for some other reason, carrying the raw system code. Not every
    /// code can be enumerated, so the truthful thing is the number.
    Failed { code: u32 },
}

/// Why no shortcut could be registered at all - named, never a shrug.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShortcutsUnavailable {
    /// This build has no route to a global shortcut on the named system.
    Unsupported { system: String },
    /// A route exists, but the thread that would hold the shortcuts could not
    /// be started. Separate from `Unsupported` because it is not about the
    /// platform and can pass on a retry.
    CouldNotStart,
}

impl fmt::Display for ShortcutsUnavailable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported { system } => {
                write!(f, "global shortcuts are not implemented on {system} yet")
            }
            Self::CouldNotStart => f.write_str("the shortcut listener could not be started"),
        }
    }
}

/// What waiting for a press produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wait {
    /// A shortcut was pressed, and this is the action bound to it.
    Pressed(HotkeyAction),
    /// The wait ran out with nothing pressed. Not an error: the caller asked to
    /// be woken so it could check whether it should still be running.
    Nothing,
    /// The shortcuts are no longer held - the thread behind them has gone. No
    /// press will ever arrive again, so a caller must not wait for one.
    Gone,
}

/// A set of registered shortcuts, alive for as long as this handle is.
///
/// Dropping the handle releases every shortcut it holds. That is the whole of
/// the release mechanism, on purpose: a tool that pretends to be a keyboard must
/// never leave `Ctrl+Alt+N` taken after it has gone, and a release that had to
/// be remembered would be forgotten on the one exit path nobody tested.
///
/// # Why presses are pulled, not pushed
///
/// The handle yields presses on request rather than calling back. A callback
/// would run on the thread that owns the shortcuts - the one that
/// `architektura.md` 6.5 says must do nothing but hand the event on - and it
/// would leave the app with no say over what happens to a press that arrives
/// while the previous one is still being delivered (`W1`). Pulling puts both
/// decisions where the state is: in `app`.
pub trait LiveShortcuts {
    /// How each requested binding fared, in the order the bindings were given.
    fn outcomes(&self) -> &[(HotkeyAction, ShortcutRegistration)];

    /// Waits up to `wait` for the next press.
    ///
    /// A zero wait asks only for what is already queued, which is how a caller
    /// drains presses that arrived while it was busy. A caller that wants to
    /// stop on its own terms waits in short slices and checks between them;
    /// nothing here blocks for longer than it was asked to.
    fn next(&self, wait: Duration) -> Wait;
}

/// Registers the tool's global shortcuts with the system.
///
/// All bindings go in at once and come back with one outcome each, so a
/// shortcut that is taken is reported beside the ones that are not, rather than
/// aborting the set. What to do about a taken shortcut is the caller's to
/// decide - the palette says so and keeps running; this port only answers
/// truthfully.
///
/// The bindings are `HotkeyAction` with `HotkeyChord`, the core's vocabulary;
/// the mapping to a platform key code belongs to the implementation, exactly as
/// the clearing keys are mapped by the keyboard adapter. The handle is `Send`
/// because the presses are consumed on a worker thread, never on the thread
/// that draws (`architektura.md` 6.5).
pub trait HotkeyRegistrar {
    /// Registers every binding and hands back the live set.
    ///
    /// # Errors
    ///
    /// [`ShortcutsUnavailable`] when nothing could be registered at all - a
    /// system with no route, or a listener that could not start. A binding that
    /// is merely taken is not an error here: it is an outcome on the handle.
    fn register(
        &self,
        bindings: &[(HotkeyAction, HotkeyChord)],
    ) -> Result<Box<dyn LiveShortcuts + Send>, ShortcutsUnavailable>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// The reason ports exist: a use case can be exercised with no file system.
    struct InMemory(HashMap<String, String>);

    impl PackSource for InMemory {
        fn read(&self, id: &str) -> Result<String, SourceError> {
            self.0.get(id).cloned().ok_or(SourceError::NotFound)
        }
    }

    #[test]
    fn a_port_can_be_satisfied_without_touching_a_disk() {
        let mut packs = HashMap::new();
        packs.insert("magic-values".to_owned(), "format = 1\n".to_owned());
        let source = InMemory(packs);

        assert_eq!(source.read("magic-values").as_deref(), Ok("format = 1\n"));
        assert_eq!(source.read("absent"), Err(SourceError::NotFound));
    }

    #[test]
    fn source_errors_render_as_stable_markers() {
        assert_eq!(SourceError::NotFound.to_string(), "not-found");
        assert_eq!(SourceError::NotUtf8.to_string(), "not-utf8");
    }
}
