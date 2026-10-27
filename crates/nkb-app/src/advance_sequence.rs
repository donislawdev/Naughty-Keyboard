//! The palette's core loop: a shortcut moves the tester through a pack.
//!
//! This is the use case behind "next value", and it holds the one piece of
//! mutable state the whole product has - the [`Sequence`] - which
//! `architektura.md` 6a puts in `app`, behind one gate, and nowhere else. The
//! sequence machine in the core decides WHAT a keystroke means. This decides
//! what to DO about it: load the pack once, deliver a value from memory, and
//! feed the result back to the machine.
//!
//! # Loaded once, delivered from memory - `W5`
//!
//! The pack is read and parsed a single time, when it is chosen, and every send
//! comes from that in-memory copy through [`deliver_value`]. `send_value`, which
//! re-reads the file each time, is right for the one-shot `nkb send` and wrong
//! here: `W5` requires the value shown to the tester to be the very value that
//! was sent, and a re-read between the two could differ if the file changed
//! underneath. The sequence never sees the pack - it holds only a count - so a
//! file changing on disk cannot redefine what `7/34` means mid-walk.
//!
//! # Bridging four delivery outcomes onto the machine
//!
//! The sequence machine models four ends to a send, and this is where a real
//! [`SendOutcome`] is sorted onto them:
//!
//! | delivery outcome | event | why |
//! |---|---|---|
//! | the value landed, or was put on the clipboard | `InsertionFinished` | it is done |
//! | no route on this system, from the send OR from the clearing before it | `InsertionFailed` | direct delivery cannot work. The same value goes to the clipboard now |
//! | part of it landed | `Cancelled` | a fragment was left in the field |
//! | nothing was sent (no target, held modifier, clear failed, clipboard busy or refusing, a character the clipboard cannot carry) | `InsertionRefused` | try again, and do NOT switch routes for a passing hiccup |
//!
//! The last row is why `InsertionRefused` exists: folding it into
//! `InsertionFailed` would turn a held Ctrl+Alt into a permanent clipboard mode.
//! The machine returns the STATE for each. The MESSAGE that says which and with
//! what numbers is built here, because the numbers are facts about delivery.
//!
//! # Two routes, one choice (`D71`)
//!
//! Values reach the field straight, or through the clipboard for the tester to
//! paste. Both are implementations of `ValueDelivery`, and [`AdvanceSequence`]
//! picks one by the sequence's delivery axis in exactly one place. Everything
//! after that choice - building the value, counting it, reporting it - is the
//! same code. On the clipboard route nothing clears the field and NOTHING is
//! pressed: the tester selects and pastes.
//!
//! # The clipboard for one window (`D72`, step 5b)
//!
//! A window running with higher privileges than the tool takes no typing: the
//! system drops every keystroke and reports success (`OBS-128`). The direct
//! route asks before pressing anything and refuses with `HigherPrivileges`. That
//! refusal is not "try again" and not "clipboard for good": `ux-spec.md` 8 asks
//! for clipboard mode FOR THAT WINDOW. So the same value goes to the clipboard in
//! the same press, the tester is told once per window, and the view shows the
//! bar until another window comes to the front - [`AdvanceSequence::on_idle`]
//! notices that. The machine's delivery axis does not move: this is not a mode
//! the sequence is in, it is a fact about the window in front, and the direct
//! route is asked again at every press, so a closed window whose handle comes
//! back on another process cannot keep a value away from a field that takes it.
//!
//! # What this deliberately does not do yet
//!
//! - only `Next`, `Previous`, `Restart` and the report copy are wired as
//!   shortcuts, and the palette's Copy buttons ([`AdvanceSequence::copy_value`])
//!   as a click. Repeat and the result marks belong to later steps, and the
//!   pack window and show/hide never reach this module. An action that is not
//!   wired says so rather than doing nothing in silence.
//! - the send is atomic - one blocking [`deliver_value`] - so `Inserting` is
//!   passed straight through. The progress bar, `Escape` mid-send and the
//!   char-by-char mode (`W2`/`W3`) are the char-by-char delivery, still to come.
//! - it does not watch the target for a change mid-send (`TargetChanged`),
//!   because that needs `TargetInspector`, which does not exist yet. It checks
//!   the target ONCE before each send instead, which is what stops a value from
//!   going into the window the palette was launched from.

use nkb_core::calendar::Day;
use nkb_core::hotkeys::HotkeyAction;
use nkb_core::pack::{Pack, PackValue, PairHalf, Risk};
use nkb_core::preview::{ValuePreview, preview_of};
use nkb_core::report::{Arrival, ReportBlock, Target};
use nkb_core::sequence::{Delivery, Effect, Event, Sequence, Upcoming};
use nkb_core::substitutions::Substitutions;

use crate::load_pack;
use crate::ports::{
    Availability, Clipboard, ClipboardError, Clock, DeliveryError, History, KeystrokeError,
    KeystrokeSender, PackFormat, PackSource, Progress, ReportText, SourceError, StopReason,
    TargetInspector, TargetRef, ValueDelivery, Waiting,
};
use crate::send_value::{Chosen, Clearing, OnTheWay, SendOutcome, ValueFacts, deliver_value};

mod classify;
mod copy;

pub use copy::CopyFrom;

/// Everything an action may reach outside the sequence, one port each.
///
/// A struct rather than loose parameters, because the list grows with the plan:
/// the report copy brought the clipboard and its text, and clipboard mode
/// brought the second route. Loose references would also let two call sites
/// pass them in a different order and still compile, since several of them are
/// the same adapter.
#[derive(Clone, Copy)]
pub struct Ports<'a> {
    /// Straight into the focused field - `DirectInjection`.
    pub direct: &'a dyn ValueDelivery,
    /// Onto the clipboard, for the tester to paste - `ClipboardDelivery`. The
    /// second implementation of the same port, chosen in [`AdvanceSequence`]
    /// by the delivery axis and nowhere else (`D71`).
    pub by_clipboard: &'a dyn ValueDelivery,
    pub keys: &'a dyn KeystrokeSender,
    /// The program and the kind of control a typed value goes to, read at
    /// the press (UX8, `D104`) - `DirectInjection` again, so it reads the
    /// window the direct route types into.
    pub inspector: &'a dyn TargetInspector,
    pub clipboard: &'a dyn Clipboard,
    pub report_text: &'a dyn ReportText,
    /// Hears a value on its way into the field, between two keys of a send
    /// that runs longer than the route's first report (`OBS-160`) - the
    /// palette draws it. Called on the thread that sends, so it must return
    /// quickly: the next key waits for it.
    pub progress: &'a dyn Fn(InFlight),
    /// Hears what the press stands on besides the keys, before the first one: the
    /// window in which the person allows the keys, on a Wayland desktop (`D145`
    /// point 76). Called on the thread that sends, so it must return quickly.
    pub waiting: &'a dyn Fn(Waiting),
    /// Set from another thread to stop a send that is going, before its next key
    /// (R03): the palette sets it when it closes, so a long value does not type
    /// on into a window nobody watches while the process waits for it.
    pub halt: &'a std::sync::atomic::AtomicBool,
}

/// A value on its way into the field, as the palette shows it while it goes
/// (`OBS-160`): which value, whether it is offensive, and how far it got. Owned,
/// because the palette draws it on another thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InFlight {
    pub facts: ValueFacts,
    /// The risk the value band marks first (`product-spec.md` 10.2) - the same
    /// fact a [`Sent`] carries once the value is in.
    pub offensive: bool,
    pub progress: Progress,
}

/// How the palette was asked to deliver values, before the first press.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteRequest {
    /// Straight into the field, where this system has a way to.
    Direct,
    /// Through the clipboard - `nkb-gui --clipboard`.
    Clipboard,
}

/// The core loop and its one piece of state.
#[derive(Debug)]
pub struct AdvanceSequence {
    sequence: Sequence,
    loaded: Option<Loaded>,
    /// The last value that went out, whole or in part - what a report block
    /// describes. A refusal leaves it alone, because nothing new reached the
    /// field, and the palette keeps showing the same value for the same reason.
    last: Option<LastSent>,
    /// The window whose values go to the clipboard because it runs with higher
    /// privileges (`D72`). Set when the direct route refuses it, cleared by the
    /// next press that the direct route takes and by [`Self::on_idle`] when
    /// another window comes to the front. A number, never a name.
    window_on_clipboard: Option<TargetRef>,
    /// How a TYPED value meets what the field holds - the tester's choice
    /// (`UX-GUI-005`, `D101`). The line by default, which is what the sequence
    /// needs to put one value in a field at a time. The clipboard route clears
    /// nothing whatever this says: it presses nothing (`D71`).
    clearing: Clearing,
    /// Where today comes from. Asked at every press and every view, never
    /// once at the start: a palette stays open for days, and a `next-date`
    /// value has to name the date that is next on the day it is typed.
    clock: Box<dyn Clock + Send>,
}

/// Which value a report block would describe, and how much of it arrived.
///
/// An index into the held pack rather than a copy of the block: the pack is
/// loaded once and never re-read under the sequence (`W5`), so the block built
/// from it on `Ctrl+Alt+B` is the value that went out - and the send itself pays
/// nothing for a report that is usually never asked for.
#[derive(Debug, Clone)]
struct LastSent {
    /// One-based, like the machine's positions.
    index: usize,
    arrival: Arrival,
    /// Where it was typed, read at the press (`D104`) - `None` when it went to
    /// the clipboard.
    target: Option<Target>,
    /// The day it went out on, so that the block quotes the date that was
    /// typed even when it is asked for after midnight (`D133`).
    on: Day,
}

/// A pack held in memory for the length of the sequence.
#[derive(Debug)]
struct Loaded {
    pack: Pack,
    /// Warnings the pack carried. It loaded anyway - warnings never block - but
    /// a send should not hide them, so they travel out with each `Sent`.
    warnings: usize,
}

/// Why a pack could not be chosen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChooseError {
    /// No pack of that name.
    NotFound,
    /// Present but unreadable, or not valid UTF-8.
    Unreadable,
    /// The pack has errors, so it loads not at all - `pack-format.md` 11.
    Refused { errors: usize },
}

/// What the palette shows after one action. Holds no toolkit and no sentence -
/// the sequence for the counter, the value that went out, and message KEYS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub sequence: Sequence,
    /// The value that just reached the field, whole or in part, when one did.
    pub sent: Option<Sent>,
    /// What the palette must say, in order. Keys and numbers, never words.
    pub messages: Vec<Message>,
    /// Whether this action went as far as the field - the machine asked for a
    /// send and the send was attempted, landed or not. A caller draining
    /// presses that queued in the meantime keys on this: presses queued behind
    /// an attempt were pressed while the tool was busy (`W1`). Presses queued
    /// behind an instant answer, such as an end-of-pack warning, were not.
    pub attempted_send: bool,
    /// Whether values for the window in front go to the clipboard because it
    /// runs with higher privileges (`D72`). Beside `sequence.delivery`, not in
    /// it: the bar says so for as long as that window stays in front, and the
    /// sequence is not in clipboard mode (`ux-spec.md` 8).
    pub clipboard_for_window: bool,
    /// What the next press of "next value" will do, AFTER this action - the
    /// palette shows it before the tester presses (`UX-GUI-001`).
    pub upcoming: Option<UpcomingValue>,
}

/// What the next press of "next value" will send, as the palette shows it
/// before it goes (`UX-GUI-001`). Read from the pack held in memory, so it is
/// the very value the press sends (`W5`), and built from the core's
/// [`Sequence::upcoming`], so it cannot name another one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpcomingValue {
    /// Value `index` of `total`.
    Value {
        index: usize,
        total: usize,
        /// The value by identifiers - its own is what the value window marks,
        /// and both are what the Copy button beside it copies (`D98`).
        key: ValueKey,
        name: String,
        /// `pack-id/value-id`, as in a [`ValueFacts`].
        reference: String,
        /// The same preview the value band shows once the value is in - from
        /// the recipe, so a million-character value is not built to show it.
        preview: ValuePreview,
        /// The effective risk, as a [`Sent`] carries it.
        offensive: bool,
        /// Which half of a pair the value is, once for each pair it stands in
        /// and in the order of the pairs in the file - usually none or one.
        /// `pack-format.md` 9 has the palette say which half is next.
        halves: Vec<PairHalf>,
    },
    /// The press only says the pack is finished.
    EndOfPack { total: usize },
}

/// A value named by identifiers: its pack's and its own (`D98`).
///
/// What a Copy button holds rather than a position, because the sequence may
/// move between a click and its turn - an identifier still names the value
/// the tester saw, or none. [`AdvanceSequence::copy_value`] takes the two.
/// Defined in the core since the settings remember values by it too
/// (`[find] recent`, `UX-GUI-016`) and its written form is one function.
pub use nkb_core::identity::ValueKey;

/// What the palette shows about the value that just went out - whole, or cut
/// short part-way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sent {
    /// The value by identifiers - what the Copy button beside it copies
    /// (`D98`), so a click copies this value and not the one the sequence
    /// moved to since.
    pub key: ValueKey,
    /// Which value, what it looks like and how large it is - the same facts
    /// `nkb send` reports. Its preview is a fragment when `elided_total` is
    /// `Some`, and 🔴 the palette must then say the total: a preview that
    /// silently showed a fragment would claim to answer "what did I send"
    /// while hiding most of it.
    pub facts: ValueFacts,
    /// The value's size in UTF-16 units - what crossed the wire when it arrived
    /// whole, and the whole value's size when it did not ([`Sent::arrival`]
    /// says how much of it crossed then).
    pub utf16_units: usize,
    /// Whether the value is offensive - the effective risk, the value's own if
    /// it declares one and otherwise the pack's. The palette marks it before a
    /// tester can wonder, `product-spec.md` 10.2.
    pub offensive: bool,
    /// Whether the field was cleared before the value went in.
    pub cleared: bool,
    /// How the value reached the field: typed whole, cut short, or put on the
    /// clipboard for the tester to paste. The SAME answer the report block
    /// gives, from the same function - until `OBS-126` the palette had a flag
    /// for the clipboard and nothing for a fragment, so it showed the value
    /// before beside a sentence about the one cut short.
    pub arrival: Arrival,
    /// The program and the kind of control it was typed into, read at the
    /// press (UX8, `UX-GUI-013`, `D104`) - the same answer the report block
    /// gives. `None` when it went to the clipboard: it is wherever the tester
    /// pastes it.
    pub target: Option<Target>,
    /// The report block of this value, word for word what the report shortcut
    /// copies while it is the last value sent (`D121`, the owner's point 8):
    /// built at the press from the same block and the same text port, so the
    /// block the palette shows is the block a ticket gets. `None` only where
    /// the block cannot be built, which a value that just went out never is.
    pub report: Option<String>,
}

/// A thing the palette must say. A key with its numbers - untouchable rule 9
/// keeps the words one layer further out, in the text module the GUI owns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    /// The pack is finished. The next press starts over.
    EndOfPack { total: usize },
    /// A step back found the first value already next - nothing moved
    /// (`D120`).
    AtFirstValue { total: usize },
    /// The target moved but the counter did not, quoting both numbers.
    CounterKept { done: usize, total: usize },
    /// A step back that types found nothing behind: nothing typed yet, or
    /// only the first value (the owner's point 6, 2026-10-14).
    NothingBefore,
    /// No pack has been chosen yet.
    NoPack,
    /// Nothing holds the keyboard focus, so nothing was sent.
    NoTarget,
    /// A modifier is physically held, so nothing was sent - release it and try
    /// again. Names which one.
    ModifierHeld { key: String },
    /// Caps Lock is on, so nothing was sent - turn it off and try again.
    CapsLockOn,
    /// The field could not be cleared, so the value was not sent. The field is
    /// in an unknown state between its old content and empty.
    ClearingFailed,
    /// The value was left half-written in the field: how far it got, of how
    /// much, and why it stopped (`D95`).
    Interrupted {
        units_sent: usize,
        units_expected: usize,
        reason: StopReason,
    },
    /// The keys stopped before any of them reached the field, so it is as the
    /// tester left it and the counter stays (`OBS-157`): said with why.
    NothingArrived { reason: StopReason },
    /// The field was cleared, and then the value's keys stopped before any of
    /// them reached it - the field is EMPTY, not as the tester left it, and the
    /// counter stays (`OBS-157`).
    ClearedThenNothingArrived { reason: StopReason },
    /// The value went in, but the route could not follow the application's
    /// queue, so characters may be missing if it was busy (`D95`, `OBS-158`).
    /// Said on every such send - silence here is the loss `D95` closes.
    NotPaced,
    /// The value was handed to the desktop's input method, which gives it to
    /// whatever has the keyboard focus (Wayland, `D145`). Nothing after that can
    /// be followed, not even whether a field had the focus, so it is never said
    /// to be IN the field. Said on every such send.
    HandedOver,
    /// The value went in, and a text service the system has switched on could
    /// have changed it on the way: the first letter made a capital, two spaces a
    /// period, a straight quote a curly one (`D139`). Names which. The field may
    /// hold something else than the value shown, and nothing in it says so.
    MayBeAltered(Substitutions),
    /// This build has no direct route on `system`, so values go to the
    /// clipboard from now on, replacing what the tester had copied. Said when
    /// the palette starts on such a system, or when a send finds out.
    ///
    /// 🔴 Names the cause the tool KNOWS. Until `D71` this was `Degraded`,
    /// whose sentence blamed the application for ignoring simulated input - a
    /// cause the tool cannot detect before the input oracle (`OBS-42`).
    NoDirectRoute { system: String },
    /// The system will not let this program type until the person allows it -
    /// the accessibility permission on macOS - so values go to the clipboard
    /// from now on, replacing what the tester had copied. Said when the palette
    /// starts without the permission, or when a send finds it gone. Unlike
    /// [`Message::NoDirectRoute`] the tester can change this one, and the
    /// sentence says where (`D138`, `OBS-70`).
    NeedsPermission,
    /// The window in front runs with higher privileges than the tool, so its
    /// values go to the clipboard, replacing what the tester had copied. Said
    /// once per window, when its first value goes there (`D72`). A cause the
    /// tool READ - the integrity level of the window's process - never a guess:
    /// a level it could not read sends as before and says nothing.
    HigherPrivileges,
    /// The keyboard focus is surely not in a text field, so nothing was sent
    /// and nothing was cleared (`D73`). A cause the tool READ - the control
    /// type of the focused element - never a guess: a focus it could not read
    /// sends as before and says nothing.
    NoTextField,
    /// The value went in, but the field was NOT cleared first, because the
    /// focus could not be confirmed as a text field and the clearing keys could
    /// reach beyond it there (`D76`). Said on every such send: the value sits
    /// on top of whatever the field held, and the tester has to know that.
    ClearingSkipped,
    /// The value went in, but the field was NOT cleared first, because the
    /// focus is a terminal: the clearing keys would go to the program running
    /// in it, which may act beyond the line (`OBS-141`). A separate sentence
    /// from [`Message::ClearingSkipped`], because here the system DOES report a
    /// field - saying it does not would be false.
    ClearingSkippedInTerminal,
    /// The value went in, but the field was NOT cleared first, because the
    /// window shows the system nothing inside it - Firefox while it keeps
    /// accessibility services out (`D125`). A separate sentence from
    /// [`Message::ClearingSkipped`], because this cause is known and the
    /// tester can change it: the sentence names the setting.
    ClearingSkippedClosedToAccessibility,
    /// The value went in, but the field was NOT cleared first, because this build cannot
    /// press the clearing keys on this system, though it types. A separate sentence from
    /// [`Message::ClearingSkipped`], because the focus may well be a field and saying the
    /// system does not report one would be false.
    ClearingSkippedNoKeys,
    /// Clipboard mode is on because the tester asked for it: values go to the
    /// clipboard, replacing what was there. Once, at the start, and again each
    /// time the tester turns the mode on in the palette (`D99`).
    ClipboardMode,
    /// The tester turned clipboard mode off: values are typed into the field
    /// from the next press (`D99`).
    ClipboardModeOff,
    /// Another application held the clipboard, and the value was not placed
    /// on it. Passing, so the tester is told to press again.
    ClipboardBusy,
    /// Another send of this program was typing on the same display for longer
    /// than one is waited for, so the value was not typed (`D157`). Passing, so
    /// the tester is told to press again once it has finished.
    TypingBusy,
    /// The clipboard refused the value, in the library's own words.
    ClipboardFailed { detail: String },
    /// The value holds a character the clipboard does not carry whole, so it
    /// was refused by name rather than pasted cut short. `U+0000` today.
    NotForClipboard { id: String, character: char },
    /// The recipe describes more text than the format allows. Should not reach
    /// here for a validated pack, and said plainly rather than swallowed.
    ValueTooLarge { id: String },
    /// The shortcut is not wired in this build. Said, never ignored.
    Unhandled { action: HotkeyAction },
    /// The shortcut arrived while the previous one was still being handled -
    /// a send in flight, or the wait for a held modifier - and was dropped, not
    /// queued. `W1`: replaying it afterwards would be exactly the queueing that
    /// `ux-spec.md` 3 rejects, and dropping it in silence would break rule 1.
    PressedWhileBusy { action: HotkeyAction },
    /// The report block of the last value is on the clipboard.
    ReportCopied { reference: String },
    /// Nothing has gone out yet, so there is no value to describe.
    NothingToReport,
    /// Another application held the clipboard, and nothing was copied. Passing,
    /// so the tester is told to press again.
    ReportBusy,
    /// The clipboard refused outright, and nothing was copied. The library's
    /// own words travel with it, because they are what a ticket about the tool
    /// needs.
    ReportFailed { detail: String },
    /// The value the tester copied with a Copy button is on the clipboard
    /// (`UX-GUI-003`, `D98`). Nothing reached a field, so the counter did not
    /// move.
    ValueCopied { reference: String },
    /// Another application held the clipboard, and the value was not copied.
    /// Passing, so the tester is told to click again.
    CopyBusy,
    /// The clipboard refused the value outright, in the library's own words.
    CopyFailed { detail: String },
    /// The value a Copy button named is not in the pack in use any more:
    /// another pack was opened between the click and its turn. Nothing was
    /// copied, because copying another value than the one on screen would be
    /// worse than copying none.
    CopyGone { reference: String },
}

impl AdvanceSequence {
    /// A sequence with no pack yet, reading today from `clock`.
    #[must_use]
    pub fn new(clock: Box<dyn Clock + Send>) -> Self {
        Self {
            sequence: Sequence::default(),
            loaded: None,
            last: None,
            window_on_clipboard: None,
            clearing: Clearing::Line,
            clock,
        }
    }

    /// The counter the palette shows, `n / total`, or `None` before a pack.
    #[must_use]
    pub fn counter(&self) -> Option<(usize, usize)> {
        self.sequence.counter()
    }

    /// The chosen pack's display name, if a pack is chosen.
    #[must_use]
    pub fn pack_name(&self) -> Option<&str> {
        self.loaded.as_ref().map(|loaded| loaded.pack.name.as_str())
    }

    /// The chosen pack's identifier, if a pack is chosen - the name a pack is
    /// asked for by, where [`Self::pack_name`] is the one it is shown by.
    #[must_use]
    pub fn pack_id(&self) -> Option<&str> {
        self.loaded.as_ref().map(|loaded| loaded.pack.id.as_str())
    }

    /// The current sequence, for a caller that renders both of its axes.
    #[must_use]
    pub fn sequence(&self) -> Sequence {
        self.sequence
    }

    /// How a typed value meets what the field holds (`D101`).
    #[must_use]
    pub const fn clearing(&self) -> Clearing {
        self.clearing
    }

    /// Chooses how the next typed values meet what the field holds: the line
    /// cleared first, or the value at the cursor (`UX-GUI-005`, `D101`).
    ///
    /// Nothing is sent and nothing is said - the palette shows the choice.
    /// Called between presses, like every choice of the tester's, so a value
    /// in flight keeps the way it started with. `Keep` presses fewer keys than
    /// `Line`, never more: the promise that clearing never reaches beyond the
    /// field (untouchable rule 17) holds either way.
    pub fn set_clearing(&mut self, clearing: Clearing) {
        self.clearing = clearing;
    }

    /// Makes value `id` of the pack in use the one the next press sends - the
    /// value window (`UX-GUI-001`). Nothing is sent.
    ///
    /// By identifier rather than by number: the window lists the catalogue as
    /// it read it, and the pack held here is the one every press sends from
    /// (`W5`). A number from there could name another value here, an
    /// identifier names this one or none. `false` when no value has that
    /// identifier, when no pack is chosen, or when the sequence refused (a value
    /// still in flight) - and then nothing moved.
    pub fn choose_next(&mut self, id: &str) -> bool {
        let Some(at) = self
            .loaded
            .as_ref()
            .and_then(|loaded| loaded.pack.values.iter().position(|value| value.id == id))
        else {
            return false;
        };
        let index = at + 1;
        let step = self.sequence.apply(Event::SetNext { index });
        let chosen = matches!(
            step.sequence.upcoming(),
            Some(Upcoming::Value { index: next, .. }) if next == index
        ) && step.effects.is_empty();
        if chosen {
            self.sequence = step.sequence;
        }
        chosen
    }

    /// Makes the first value of the pack in use the next one again, nothing
    /// sent - the start of testing after the welcome window's box, where the
    /// tester may have sent a value or two to try it (the owner's point 1,
    /// `D109`). Without it the first real field would get value 3, and values
    /// 1 and 2 would have gone only into the box.
    ///
    /// What went out last stays: it did go out, into the box, and the report
    /// block of it is still true. `false` with no pack, an empty one, or while
    /// a value is in flight - then nothing moved.
    pub fn start_over(&mut self) -> bool {
        let Some(first) = self
            .loaded
            .as_ref()
            .and_then(|loaded| loaded.pack.values.first())
            .map(|value| value.id.clone())
        else {
            return false;
        };
        self.choose_next(&first)
    }

    /// What the next press of "next value" will do, with what the palette
    /// shows of the value - see [`UpcomingValue`]. `None` where the core says
    /// nothing is upcoming, and where the pack does not hold the value the
    /// core names (a disagreement the send itself refuses too).
    #[must_use]
    pub fn upcoming(&self) -> Option<UpcomingValue> {
        match self.sequence.upcoming()? {
            Upcoming::EndOfPack { total } => Some(UpcomingValue::EndOfPack { total }),
            Upcoming::Value { index, total } => {
                let loaded = self.loaded.as_ref()?;
                let value = loaded.pack.values.get(index.checked_sub(1)?)?;
                let halves = loaded.pack.halves_of(&value.id).map(|(_, half)| half);
                Some(UpcomingValue::Value {
                    index,
                    total,
                    key: ValueKey {
                        pack: loaded.pack.id.clone(),
                        value: value.id.clone(),
                    },
                    name: value.name.clone(),
                    reference: format!("{}/{}", loaded.pack.id, value.id),
                    preview: preview_of(&value.definition.on(self.clock.today())),
                    offensive: offensive(&loaded.pack, value),
                    halves: halves.collect(),
                })
            }
        }
    }

    /// Whether values for the window in front go to the clipboard because it
    /// runs with higher privileges - see [`Outcome::clipboard_for_window`].
    #[must_use]
    pub fn clipboard_for_window(&self) -> bool {
        self.window_on_clipboard.is_some()
    }

    /// Between presses: notices that the window whose values went to the
    /// clipboard is no longer in front, so the bar can go (`ux-spec.md` 8).
    ///
    /// Asks for the window in front only while that state holds, and asks for
    /// nothing else - the tool does not watch which windows the tester visits.
    /// `None` when nothing changed, so the caller draws nothing.
    pub fn on_idle(&mut self, ports: &Ports<'_>) -> Option<Outcome> {
        let window = self.window_on_clipboard?;
        if ports.direct.target() == Some(window) {
            return None;
        }
        self.window_on_clipboard = None;
        Some(self.settled(None, Vec::new(), false))
    }

    /// Reads, validates and chooses a pack, moving the sequence to `ready`.
    ///
    /// Called at startup and again whenever the tester picks another pack.
    /// `check` runs first and the pack is refused on any error -
    /// `pack-format.md` 11 loads all of a pack or none of it - which is what
    /// `load_pack::load` does. A refused pack leaves everything as it was: the
    /// pack in use, its place in it and the last value all stay.
    ///
    /// ⚠️ Called only between actions, never while a value is going in. The
    /// machine would refuse `PackChosen` in `inserting`, but the loaded pack is
    /// swapped before the machine is asked. Unreachable today, because a send
    /// is one blocking call inside [`Self::on_action`] and the sequence is never
    /// left `inserting` between calls. Delivery one character at a time (`W2`)
    /// has to ask the machine first.
    ///
    /// # Errors
    ///
    /// [`ChooseError`] when the pack is missing, unreadable, or has errors.
    pub fn choose_pack(
        &mut self,
        source: &dyn PackSource,
        format: &dyn PackFormat,
        pack_id: &str,
    ) -> Result<(), ChooseError> {
        let text = match source.read(pack_id) {
            Ok(text) => text,
            Err(SourceError::NotFound) => return Err(ChooseError::NotFound),
            Err(SourceError::Unreadable | SourceError::NotUtf8 | SourceError::TooLarge) => {
                return Err(ChooseError::Unreadable);
            }
        };
        let loaded =
            load_pack::load(format, pack_id, &text).map_err(|refused| ChooseError::Refused {
                errors: refused.errors,
            })?;
        let total = loaded.pack.values.len();
        // A new pack makes the old index point at somebody else's value.
        self.last = None;
        self.loaded = Some(Loaded {
            pack: loaded.pack,
            warnings: loaded.warnings,
        });
        self.sequence = self.sequence.apply(Event::PackChosen { total }).sequence;
        Ok(())
    }

    /// Settles how values will travel, before the first press, and says what
    /// the tester has to know about it.
    ///
    /// Called once at start, after the shortcuts are known to work - a palette
    /// that nothing can drive must not tell the tester to paste each value.
    ///
    /// Direct delivery with no route on this system starts in clipboard mode AT
    /// ONCE: failing the first press to discover what `availability` already
    /// knew would be the pretending `ux-spec.md` 8 forbids.
    #[must_use]
    pub fn choose_route(&mut self, request: RouteRequest, ports: &Ports<'_>) -> Vec<Message> {
        let announcement = match request {
            RouteRequest::Clipboard => Message::ClipboardMode,
            RouteRequest::Direct => match ports.direct.availability() {
                Availability::Ready => return Vec::new(),
                Availability::Unavailable { reason } => Message::NoDirectRoute { system: reason },
                Availability::NeedsPermission => Message::NeedsPermission,
            },
        };
        let step = self.sequence.apply(Event::UseClipboard);
        self.sequence = step.sequence;
        // Said only when the machine actually entered the mode: a second
        // request finds it on already, and the warning is owed once.
        if step.effects.contains(&Effect::AnnounceClipboardMode) {
            vec![announcement]
        } else {
            step.effects.iter().filter_map(announce).collect()
        }
    }

    /// Turns clipboard mode off - the tester's click in the palette (`D99`).
    ///
    /// Only where the direct route is there to take the values: a system
    /// without one keeps the mode and says why, in the sentence that put it
    /// there. Turning on goes through [`Self::choose_route`] with
    /// [`RouteRequest::Clipboard`], which already warns that the clipboard is
    /// replaced.
    #[must_use]
    pub fn leave_clipboard(&mut self, ports: &Ports<'_>) -> Vec<Message> {
        if self.sequence.delivery == Delivery::Direct {
            return Vec::new();
        }
        match ports.direct.availability() {
            Availability::Ready => {}
            Availability::Unavailable { reason } => {
                return vec![Message::NoDirectRoute { system: reason }];
            }
            Availability::NeedsPermission => return vec![Message::NeedsPermission],
        }
        let step = self.sequence.apply(Event::UseDirect);
        self.sequence = step.sequence;
        if step.effects.contains(&Effect::AnnounceDirect) {
            vec![Message::ClipboardModeOff]
        } else {
            step.effects.iter().filter_map(announce).collect()
        }
    }

    /// Whether [`Self::on_action`] carries `action` out in this build, rather
    /// than answering [`Message::Unhandled`].
    ///
    /// Asked by whoever shows the tester which shortcuts do something - the
    /// palette's hint bar names every one of them and no other (`UX-GUI-007`).
    /// A question here rather than a list there: the list would be a second
    /// copy of the match below, free to drift from it, and a test holds this
    /// answer to what `on_action` really does for every action.
    ///
    /// `NextPack` and `PreviousPack` are not among them, and that is not a gap:
    /// a pack is opened where the settings remember it, which is the palette's
    /// business (`D120`) - the same path the value window takes.
    #[must_use]
    pub const fn handles(action: HotkeyAction) -> bool {
        matches!(
            action,
            HotkeyAction::NextValue
                | HotkeyAction::PreviousValue
                | HotkeyAction::RestartPack
                | HotkeyAction::CopyReport
                | HotkeyAction::SkipValue
                | HotkeyAction::BackOneValue
        )
    }

    /// Applies one shortcut and, if it means sending, does the send.
    ///
    /// The target is checked ONCE before a send: nothing focused means the value
    /// would go into the window the palette was launched from, so the send is
    /// refused before a single key leaves.
    #[must_use]
    pub fn on_action(&mut self, action: HotkeyAction, ports: &Ports<'_>) -> Outcome {
        let event = match action {
            HotkeyAction::NextValue => Event::Next,
            HotkeyAction::PreviousValue => Event::Previous,
            HotkeyAction::RestartPack => Event::Restart,
            // Walking the pack without typing (`D120`): the next value moves,
            // nothing is sent, so the path below that sends is never taken.
            HotkeyAction::SkipValue => Event::Skip,
            HotkeyAction::BackOneValue => Event::Back,
            HotkeyAction::CopyReport => return self.copy_report(ports),
            other => {
                return self.settled(None, vec![Message::Unhandled { action: other }], false);
            }
        };

        let step = self.sequence.apply(event);
        let Some(index) = send_index(&step) else {
            // Nothing to send - a warning, an end-of-pack, a no-op. The state
            // moves and the machine's own effects become the messages.
            self.sequence = step.sequence;
            return self.settled(
                None,
                step.effects.iter().filter_map(announce).collect(),
                false,
            );
        };

        // The machine has put us in `Inserting` and asked for value `index`.
        self.sequence = step.sequence;
        self.send(index, ports)
    }

    /// Copies the report block of the last value that went out.
    ///
    /// Answers at once and moves nothing: the sequence has no event for a
    /// report, and a press queued behind this one was not pressed while busy.
    ///
    /// 🔴 One of the THREE places the tool uses the clipboard - untouchable rule
    /// 17 and `tests/clipboard_has_named_doors.rs` - and the only one with a
    /// door of its own: clipboard mode and the Copy buttons take the clipboard
    /// route. Nothing is written unless the tester asked, and nothing is
    /// reported as copied that the clipboard did not take. The block stays in
    /// the system's clipboard history on purpose (`D68`). Values do not (`D71`).
    fn copy_report(&self, ports: &Ports<'_>) -> Outcome {
        let message = match self.last_block() {
            None => Message::NothingToReport,
            Some(Err(id)) => Message::ValueTooLarge { id },
            Some(Ok(block)) => {
                let text = ports.report_text.report_text(&block);
                match ports.clipboard.put_text(&text, History::Keep) {
                    Ok(()) => Message::ReportCopied {
                        reference: block.reference,
                    },
                    Err(ClipboardError::Busy) => Message::ReportBusy,
                    Err(ClipboardError::Failed { detail }) => Message::ReportFailed { detail },
                }
            }
        };
        self.settled(None, vec![message], false)
    }

    /// The block for the last value that went out, if one did.
    ///
    /// `Err` carries the value's identifier when the recipe cannot be built -
    /// unreachable for a value that was just sent, and said rather than
    /// swallowed if it ever is.
    fn last_block(&self) -> Option<Result<ReportBlock, String>> {
        let last = self.last.as_ref()?;
        let loaded = self.loaded.as_ref()?;
        let value = loaded.pack.values.get(last.index.checked_sub(1)?)?;
        Some(
            ReportBlock::describe(&loaded.pack, value, last.on, last.arrival)
                .map(|block| ReportBlock {
                    target: last.target.clone(),
                    ..block
                })
                .map_err(|_| value.id.clone()),
        )
    }

    /// Delivers value `index` from the held pack and settles the sequence.
    ///
    /// # At most two attempts, and why there is a second
    ///
    /// A direct send that finds no route switches the sequence to clipboard
    /// mode, and the machine asks for the SAME value again in the same press -
    /// "switching to clipboard mode, press paste now" (`product-spec.md` 9.3,
    /// `D71`). The machine asks that once by construction. Were it ever to ask
    /// a second time, the flight is ended here instead of repeated: a value must
    /// never be left in flight, and a route must never be asked forever.
    fn send(&mut self, index: usize, ports: &Ports<'_>) -> Outcome {
        let first = self.attempt(index, ports);
        let Some(again) = first.again else {
            return self.settled(first.sent, first.messages, first.attempted);
        };
        let second = self.attempt(again, ports);
        if second.again.is_some() {
            self.sequence = self.sequence.apply(Event::InsertionRefused).sequence;
        }
        let mut messages = first.messages;
        messages.extend(second.messages);
        // The first attempt reached the route, so a press queued behind this
        // one was pressed while the tool was busy - whatever the second did.
        self.settled(second.sent, messages, true)
    }

    /// One try at value `index`, by the route the delivery axis names.
    fn attempt(&mut self, index: usize, ports: &Ports<'_>) -> Attempt {
        // 🔴 THE choice between the two implementations, and the only one
        // (`D71`, `ports.rs` on `ValueDelivery`). The clipboard route clears
        // nothing, because it presses nothing: the tester selects and pastes.
        let (route, clearing) = match self.sequence.delivery {
            Delivery::Direct => (ports.direct, self.clearing),
            Delivery::ClipboardMode => (ports.by_clipboard, Clearing::Keep),
        };
        let on_clipboard = self.sequence.delivery == Delivery::ClipboardMode;

        // A SendValue effect only comes from a chosen pack, so this is present.
        // If it somehow is not, say so rather than reach into a `None`.
        let Some(loaded) = self.loaded.as_ref() else {
            return refuse(&mut self.sequence, vec![Message::NoPack]);
        };

        // Refuse before touching the field if nothing is focused: the value
        // would otherwise land in the launching window. Measured as real in the
        // step-2 probe, not hypothetical. The clipboard route always has a
        // target, because its value does not go to the focused window at all.
        //
        // 🔴 Read ONCE (`D148`). What the tester is told, the field the
        // clearing is allowed in and the keys of the value are all about this
        // window. A window that comes to the front later - during the wait for
        // a held modifier, or between the clearing and the value - gets nothing.
        let Some(target) = route.target() else {
            return refuse(&mut self.sequence, vec![Message::NoTarget]);
        };

        let Some(value) = loaded.pack.values.get(index - 1) else {
            // The machine asked for an index the pack does not hold, which would
            // be a machine/pack disagreement rather than a delivery problem.
            return refuse(&mut self.sequence, Vec::new());
        };
        let offensive = offensive(&loaded.pack, value);
        let key = ValueKey {
            pack: loaded.pack.id.clone(),
            value: value.id.clone(),
        };
        // Where a typed value goes, read at the press and before any key
        // (UX8, `D104`): the program and the kind of control. Never for the
        // clipboard route - its value goes wherever the tester pastes it.
        let inspected = if on_clipboard {
            None
        } else {
            Some(ports.inspector.inspect(target))
        };
        let mut on_clipboard = on_clipboard;
        let mut tell = |way: OnTheWay<'_>| match way {
            OnTheWay::Typing(sending) => (ports.progress)(InFlight {
                facts: sending.facts.clone(),
                offensive,
                progress: sending.progress,
            }),
            OnTheWay::Waiting(waiting) => (ports.waiting)(waiting),
        };
        let chosen = Chosen {
            pack: &loaded.pack,
            value,
            warnings: loaded.warnings,
            on: self.clock.today(),
        };
        let means = |delivery| crate::send_value::Means {
            delivery,
            keys: ports.keys,
            halt: ports.halt,
        };
        let mut outcome = deliver_value(chosen, target, means(route), clearing, &mut tell);

        // `D72`: the direct route refused a window with higher privileges before
        // pressing a single key. Not a hiccup to retry and not a system without
        // a route: THIS window takes no typing, so the same value goes to the
        // clipboard now, and the tester hears why once per window.
        let mut messages = Vec::new();
        if !on_clipboard && refused_for_privileges(&outcome) {
            if self.window_on_clipboard != Some(target) {
                messages.push(Message::HigherPrivileges);
            }
            self.window_on_clipboard = Some(target);
            on_clipboard = true;
            // Meant for the same window: the clipboard takes it whatever the
            // target says, and the tester carries it there.
            outcome = deliver_value(
                chosen,
                target,
                means(ports.by_clipboard),
                Clearing::Keep,
                &mut tell,
            );
        } else {
            // The direct route took this window, or the sequence is in clipboard
            // mode anyway: no window of ours is waiting on the clipboard.
            self.window_on_clipboard = None;
        }

        // The `D72` detour above went to the clipboard: the window that was read
        // received nothing.
        let target = if on_clipboard { None } else { inspected };
        let arrival = arrival_of(&outcome, on_clipboard);
        if let Some(arrival) = arrival {
            self.last = Some(LastSent {
                index,
                arrival,
                target: target.clone(),
                on: chosen.on,
            });
        }
        // Only when THIS value became the last one: otherwise the block would
        // describe the value before it.
        let report = arrival
            .and_then(|_| self.last_block())
            .and_then(Result::ok)
            .map(|block| ports.report_text.report_text(&block));
        let (event, sent, classified) = classify::classify(outcome, offensive, on_clipboard, &key);
        let sent = sent.map(|sent| Sent {
            target,
            report,
            ..sent
        });
        messages.extend(classified);
        let step = self.sequence.apply(event);
        self.sequence = step.sequence;
        Attempt {
            sent,
            messages,
            attempted: true,
            again: send_index(&step),
        }
    }

    /// Wraps the current state with what the send produced.
    ///
    /// `attempted` is true only when [`deliver_value`] actually ran. The
    /// refusals before it (no pack, no target, an index the pack does not hold)
    /// answer instantly, so a press queued behind them was not pressed while
    /// the tool was busy and must not be reported as if it were.
    fn settled(&self, sent: Option<Sent>, messages: Vec<Message>, attempted: bool) -> Outcome {
        Outcome {
            sequence: self.sequence,
            sent,
            messages,
            attempted_send: attempted,
            clipboard_for_window: self.clipboard_for_window(),
            upcoming: self.upcoming(),
        }
    }
}

/// Whether a value is offensive: its own risk when it declares one, the pack's
/// otherwise. One rule for the value about to go and the value that went.
fn offensive(pack: &Pack, value: &PackValue) -> bool {
    value.risk.unwrap_or(pack.risk) == Risk::Offensive
}

/// What one attempt at a value produced, before the sequence is settled.
struct Attempt {
    sent: Option<Sent>,
    messages: Vec<Message>,
    /// Whether the route was reached at all - see [`Outcome::attempted_send`].
    attempted: bool,
    /// The value the machine asks for once more, by index - only after a
    /// direct send found no route and the sequence moved to the clipboard.
    again: Option<usize>,
}

/// Refuses before the route was reached: the sequence goes back to where it
/// stood, and nothing new is in the field.
///
/// Takes the sequence rather than `self`, because the caller still holds the
/// pack borrowed and the two fields are separate.
fn refuse(sequence: &mut Sequence, messages: Vec<Message>) -> Attempt {
    *sequence = sequence.apply(Event::InsertionRefused).sequence;
    Attempt {
        sent: None,
        messages,
        attempted: false,
        again: None,
    }
}

/// How the value reached the field, when any of it did.
///
/// `None` for every refusal: nothing new is in the field, so the report block
/// keeps describing the value before.
fn arrival_of(outcome: &SendOutcome, on_clipboard: bool) -> Option<Arrival> {
    match outcome {
        SendOutcome::Sent { .. } if on_clipboard => Some(Arrival::OnClipboard),
        SendOutcome::Sent { .. } => Some(Arrival::Whole),
        SendOutcome::Interrupted {
            units_sent,
            units_expected,
            ..
        } => Some(Arrival::Interrupted {
            units_sent: *units_sent,
            units_expected: *units_expected,
        }),
        // `deliver_value` reports a fragment as `Interrupted`, so this is
        // reached only if a route ever reports one another way - and the block
        // still describes the value the fragment belongs to.
        SendOutcome::NotDelivered {
            error:
                DeliveryError::Partial {
                    units_sent,
                    units_expected,
                    ..
                },
            ..
        } => Some(Arrival::Interrupted {
            units_sent: *units_sent,
            units_expected: *units_expected,
        }),
        _ => None,
    }
}

/// Whether the direct route refused because the window in front runs with
/// higher privileges - from the clearing, which goes first, or from the send.
/// Either way no key was pressed (`D72`).
fn refused_for_privileges(outcome: &SendOutcome) -> bool {
    matches!(
        outcome,
        SendOutcome::NotCleared {
            error: KeystrokeError::HigherPrivileges
        } | SendOutcome::NotDelivered {
            error: DeliveryError::HigherPrivileges,
            ..
        }
    )
}

/// The `index` a step asks to send, if it asks at all.
fn send_index(step: &nkb_core::sequence::Step) -> Option<usize> {
    step.effects.iter().find_map(|effect| match effect {
        Effect::SendValue { index } => Some(*index),
        _ => None,
    })
}

/// The message a machine effect stands for, for the effects that reach a caller
/// outside a send. `SendValue` is handled by delivering, not by a message, and
/// the announce effects that only appear during a send are supplied from the
/// delivery outcome instead.
fn announce(effect: &Effect) -> Option<Message> {
    match effect {
        Effect::AnnounceEndOfPack { total } => Some(Message::EndOfPack { total: *total }),
        Effect::AnnounceAtFirst { total } => Some(Message::AtFirstValue { total: *total }),
        Effect::AnnounceCounterKept { done, total } => Some(Message::CounterKept {
            done: *done,
            total: *total,
        }),
        Effect::AnnounceNothingBefore => Some(Message::NothingBefore),
        Effect::AnnounceNoPack => Some(Message::NoPack),
        Effect::AnnounceNoTarget => Some(Message::NoTarget),
        // These arise only while sending, and the send path builds a richer
        // message from the delivery outcome. Seeing one here would mean a send
        // effect leaked onto the no-send path, so it is dropped rather than
        // shown half-formed. Entering clipboard mode is said by the caller that
        // knows WHY it was entered - the tester's request or a missing route -
        // because the two sentences differ and the machine knows neither.
        // Leaving it is said by `leave_clipboard`, the one caller that asks.
        Effect::SendValue { .. }
        | Effect::AnnounceStillInserting
        | Effect::AnnounceInterrupted { .. }
        | Effect::AnnounceClipboardMode
        | Effect::AnnounceDirect => None,
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;
    use crate::test_support::*;
    use nkb_core::report::ControlKind;
    use nkb_core::sequence::{Delivery, Position};

    #[test]
    fn choosing_a_pack_moves_to_ready_with_the_right_total() {
        let advance = chosen(Risk::Normal);
        assert_eq!(advance.counter(), Some((0, 3)));
        assert_eq!(advance.pack_name(), Some("Sample pack"));
        assert_eq!(advance.pack_id(), Some("sample"));
    }

    /// The palette chooses packs while it runs. One that is refused must not
    /// take the tester's place in the pack in use, nor the value the report
    /// block is about.
    #[test]
    fn a_pack_refused_while_running_leaves_the_pack_in_use_its_place_and_its_last_value() {
        let mut advance = chosen(Risk::Normal);
        let kit = Kit::ready();
        let ports = kit.ports();
        let _ = advance.on_action(HotkeyAction::NextValue, &ports);

        let refused = advance.choose_pack(&FakeSource, &FakeFormat::with_errors(2), "broken");
        assert_eq!(refused, Err(ChooseError::Refused { errors: 2 }));
        let missing = advance.choose_pack(&MissingSource, &FakeFormat::with_errors(0), "absent");
        assert_eq!(missing, Err(ChooseError::NotFound));

        assert_eq!(advance.pack_id(), Some("sample"));
        assert_eq!(advance.counter(), Some((1, 3)), "the place was kept");
        let report = advance.on_action(HotkeyAction::CopyReport, &ports);
        assert_eq!(
            report.messages,
            vec![Message::ReportCopied {
                reference: String::from("sample/one")
            }],
            "the last value is still the one the report is about"
        );
    }

    #[test]
    fn a_value_on_its_way_is_heard_with_its_risk_in_the_order_the_route_said() {
        // `OBS-160`: the palette draws the value while it goes. It hears the
        // value the machine asked for, whether it is offensive, and how far it
        // got - every report, in order - and it is the value that went out.
        let mut advance = chosen(Risk::Offensive);
        let steps = [
            Progress {
                units_arrived: 2,
                units_total: 5,
            },
            Progress {
                units_arrived: 4,
                units_total: 5,
            },
        ];
        let kit = Kit::with_delivery(FakeDelivery::ready().reporting(&steps));
        let outcome = advance.on_action(HotkeyAction::NextValue, &kit.ports());
        let sent = outcome.sent.expect("the value went out");
        let heard = kit.heard.borrow();
        assert_eq!(
            heard.iter().map(|h| h.progress).collect::<Vec<_>>(),
            steps.to_vec()
        );
        assert!(heard.iter().all(|h| h.offensive), "the risk travels too");
        assert!(
            heard.iter().all(|h| h.facts == sent.facts),
            "the value heard on the way is the value sent"
        );
    }

    #[test]
    fn a_send_too_short_to_report_is_heard_not_at_all() {
        let mut advance = chosen(Risk::Normal);
        let kit = Kit::ready();
        let _ = advance.on_action(HotkeyAction::NextValue, &kit.ports());
        assert!(kit.heard.borrow().is_empty());
    }

    #[test]
    fn the_first_next_sends_value_one_and_advances_the_counter() {
        let mut advance = chosen(Risk::Normal);
        let outcome = advance.on_action(HotkeyAction::NextValue, &Kit::ready().ports());
        let sent = outcome.sent.expect("value one must have been sent");
        assert_eq!(sent.facts.reference, "sample/one");
        assert_eq!(sent.facts.name, "Value one");
        assert!(sent.cleared, "the palette clears by default");
        assert_eq!(advance.counter(), Some((1, 3)));
        assert!(outcome.messages.is_empty());
    }

    /// `D101`: at the cursor, a press types the value and presses NO other key -
    /// the clearing recipe never reaches the keyboard port - and the value is
    /// not marked cleared. Back to the line, the next press clears again. The
    /// sequence moves the same either way.
    #[test]
    fn at_the_cursor_a_value_goes_in_with_no_clearing_and_the_line_comes_back() {
        let mut advance = chosen(Risk::Normal);
        let kit = Kit::ready();
        assert_eq!(advance.clearing(), Clearing::Line, "the line by default");
        advance.set_clearing(Clearing::Keep);
        let outcome = advance.on_action(HotkeyAction::NextValue, &kit.ports());
        let sent = outcome.sent.expect("value one went in at the cursor");
        assert!(!sent.cleared, "nothing was cleared, and the value says so");
        assert_eq!(
            *kit.keys.requests.borrow(),
            0,
            "a key other than the value's"
        );
        assert!(outcome.messages.is_empty(), "{:?}", outcome.messages);
        assert_eq!(advance.counter(), Some((1, 3)));

        advance.set_clearing(Clearing::Line);
        let outcome = advance.on_action(HotkeyAction::NextValue, &kit.ports());
        assert!(outcome.sent.expect("value two").cleared);
        assert_eq!(*kit.keys.requests.borrow(), 1, "the line is cleared again");
        assert_eq!(advance.counter(), Some((2, 3)));
    }

    /// `D145` point 76: the clearing is the first thing a press does, and on a Wayland
    /// desktop the first one waits for the person. The palette hears the wait through the
    /// ports, in order, and the value still goes in once it is over. At the cursor nothing
    /// is pressed, so nothing is waited for.
    #[test]
    fn a_press_that_waits_for_the_person_to_allow_the_keys_is_heard_through_the_ports() {
        let mut advance = chosen(Risk::Normal);
        let kit = Kit::with_keys(FakeKeys::asking_the_person());
        let outcome = advance.on_action(HotkeyAction::NextValue, &kit.ports());
        assert!(outcome.sent.expect("the value went in").cleared);
        assert_eq!(
            *kit.waited.borrow(),
            [Waiting::Asking, Waiting::ForConsent, Waiting::Answered]
        );

        advance.set_clearing(Clearing::Keep);
        let outcome = advance.on_action(HotkeyAction::NextValue, &kit.ports());
        assert!(outcome.sent.is_some());
        assert_eq!(
            kit.waited.borrow().len(),
            3,
            "a press at the cursor waited for no one"
        );
    }

    #[test]
    fn walking_the_whole_pack_ends_exhausted() {
        let mut advance = chosen(Risk::Normal);
        for _ in 0..3 {
            let _ = advance.on_action(HotkeyAction::NextValue, &Kit::ready().ports());
        }
        assert_eq!(advance.counter(), Some((3, 3)));
        let outcome = advance.on_action(HotkeyAction::NextValue, &Kit::ready().ports());
        assert_eq!(outcome.messages, vec![Message::EndOfPack { total: 3 }]);
        assert!(outcome.sent.is_none());
    }

    #[test]
    fn no_target_refuses_the_send_without_degrading() {
        let mut advance = chosen(Risk::Normal);
        let outcome = advance.on_action(
            HotkeyAction::NextValue,
            &Kit::with_delivery(FakeDelivery::without_target()).ports(),
        );
        assert_eq!(outcome.messages, vec![Message::NoTarget]);
        assert!(outcome.sent.is_none());
        assert_eq!(advance.sequence().position, Position::Ready { total: 3 });
        assert_eq!(advance.sequence().delivery, Delivery::Direct);
    }

    #[test]
    fn no_route_switches_to_the_clipboard_and_puts_that_same_value_there() {
        // D71, product-spec.md 9.3: "switching to clipboard mode, press paste
        // now" - in the same press, with the value that did not arrive.
        let mut advance = chosen(Risk::Normal);
        let kit = Kit::with_delivery(FakeDelivery::failing(DeliveryError::Unsupported {
            system: "macOS".to_owned(),
        }));
        let outcome = advance.on_action(HotkeyAction::NextValue, &kit.ports());
        assert_eq!(
            outcome.messages,
            vec![Message::NoDirectRoute {
                system: "macOS".to_owned()
            }]
        );
        assert_eq!(advance.sequence().delivery, Delivery::ClipboardMode);
        assert_eq!(*kit.by_clipboard.handed.borrow(), vec!["alpha"]);
        let sent = outcome.sent.expect("value one went to the clipboard");
        assert_eq!(sent.facts.reference, "sample/one");
        assert!(sent.arrival == Arrival::OnClipboard);
        assert!(!sent.cleared, "the clipboard route clears nothing");
        assert_eq!(
            advance.counter(),
            Some((1, 3)),
            "placed on the clipboard counts as sent - ux-spec.md 8"
        );
        assert!(outcome.attempted_send);
    }

    #[test]
    fn no_route_found_by_the_clearing_is_no_route_not_a_failed_clear() {
        // OBS-129: the clearing goes first, so on a system with no route the
        // keys answer before the value can. It used to say the field "may hold
        // part of its old content" about a field nothing had touched.
        let mut advance = chosen(Risk::Normal);
        let kit = Kit::with_keys(FakeKeys::failing(KeystrokeError::Unsupported {
            system: "Linux".to_owned(),
        }));
        let outcome = advance.on_action(HotkeyAction::NextValue, &kit.ports());
        assert_eq!(
            outcome.messages,
            vec![Message::NoDirectRoute {
                system: "Linux".to_owned()
            }]
        );
        assert!(!outcome.messages.contains(&Message::ClearingFailed));
        assert_eq!(advance.sequence().delivery, Delivery::ClipboardMode);
        assert!(
            kit.direct.handed.borrow().is_empty(),
            "the value never went direct"
        );
        assert_eq!(*kit.by_clipboard.handed.borrow(), vec!["alpha"]);
    }

    #[test]
    fn in_clipboard_mode_nothing_is_pressed_and_every_value_goes_to_the_clipboard() {
        let mut advance = chosen(Risk::Normal);
        let kit = Kit::ready();
        assert_eq!(
            advance.choose_route(RouteRequest::Clipboard, &kit.ports()),
            vec![Message::ClipboardMode]
        );
        let _ = advance.on_action(HotkeyAction::NextValue, &kit.ports());
        let _ = advance.on_action(HotkeyAction::NextValue, &kit.ports());
        let outcome = advance.on_action(HotkeyAction::PreviousValue, &kit.ports());

        assert_eq!(
            *kit.keys.requests.borrow(),
            0,
            "clipboard mode presses NOTHING - the tester pastes (D71)"
        );
        assert!(kit.direct.handed.borrow().is_empty());
        assert_eq!(
            *kit.by_clipboard.handed.borrow(),
            vec!["alpha", "beta", "alpha"]
        );
        let sent = outcome.sent.expect("value one went to the clipboard again");
        assert!(sent.arrival == Arrival::OnClipboard && !sent.cleared);
        assert!(
            outcome.messages.is_empty(),
            "the bar says it, not every press"
        );
    }

    #[test]
    fn a_system_without_a_route_starts_in_clipboard_mode_and_says_why() {
        let mut advance = chosen(Risk::Normal);
        let kit = Kit::with_delivery(FakeDelivery::unavailable("macOS"));
        assert_eq!(
            advance.choose_route(RouteRequest::Direct, &kit.ports()),
            vec![Message::NoDirectRoute {
                system: "macOS".to_owned()
            }]
        );
        assert_eq!(advance.sequence().delivery, Delivery::ClipboardMode);
        assert_eq!(
            advance.counter(),
            Some((0, 3)),
            "choosing a route sends nothing"
        );
        assert!(kit.by_clipboard.handed.borrow().is_empty());
    }

    #[test]
    fn a_system_with_a_route_stays_direct_and_says_nothing() {
        let mut advance = chosen(Risk::Normal);
        assert!(
            advance
                .choose_route(RouteRequest::Direct, &Kit::ready().ports())
                .is_empty()
        );
        assert_eq!(advance.sequence().delivery, Delivery::Direct);
    }

    #[test]
    fn asking_for_clipboard_mode_twice_warns_once() {
        let mut advance = chosen(Risk::Normal);
        let kit = Kit::ready();
        let _ = advance.choose_route(RouteRequest::Clipboard, &kit.ports());
        assert!(
            advance
                .choose_route(RouteRequest::Clipboard, &kit.ports())
                .is_empty()
        );
    }

    #[test]
    fn turning_clipboard_mode_off_types_the_next_value_into_the_field() {
        // D99: the tester's click. The value after it is typed, the one before
        // it stays where it went, and the place in the pack carries on.
        let mut advance = chosen(Risk::Normal);
        let kit = Kit::ready();
        let _ = advance.choose_route(RouteRequest::Clipboard, &kit.ports());
        let _ = advance.on_action(HotkeyAction::NextValue, &kit.ports());

        assert_eq!(
            advance.leave_clipboard(&kit.ports()),
            vec![Message::ClipboardModeOff]
        );
        assert_eq!(advance.sequence().delivery, Delivery::Direct);
        assert_eq!(advance.counter(), Some((1, 3)), "turning off sends nothing");

        let outcome = advance.on_action(HotkeyAction::NextValue, &kit.ports());
        assert_eq!(*kit.direct.handed.borrow(), vec!["beta"]);
        assert_eq!(*kit.by_clipboard.handed.borrow(), vec!["alpha"]);
        assert_eq!(advance.counter(), Some((2, 3)));
        assert!(
            outcome.messages.is_empty(),
            "a typed value after the switch is an ordinary send"
        );
    }

    #[test]
    fn turning_off_clipboard_mode_that_is_not_on_says_nothing() {
        let mut advance = chosen(Risk::Normal);
        let kit = Kit::ready();
        assert!(advance.leave_clipboard(&kit.ports()).is_empty());
        assert_eq!(advance.sequence().delivery, Delivery::Direct);

        let _ = advance.choose_route(RouteRequest::Clipboard, &kit.ports());
        let _ = advance.leave_clipboard(&kit.ports());
        assert!(
            advance.leave_clipboard(&kit.ports()).is_empty(),
            "a second click on a mode already off said something"
        );

        // Not in the mode on a system without a route - before the start route
        // is chosen. Asking about the route there would say values now go to
        // the clipboard, which they do not.
        let mut not_yet = chosen(Risk::Normal);
        let without = Kit::with_delivery(FakeDelivery::unavailable("macOS"));
        assert!(not_yet.leave_clipboard(&without.ports()).is_empty());
        assert_eq!(not_yet.sequence().delivery, Delivery::Direct);
    }

    #[test]
    fn a_system_without_a_route_keeps_clipboard_mode_and_says_why() {
        // Leaving would promise typing this build cannot do: the mode stays and
        // the sentence is the one that put it there.
        let mut advance = chosen(Risk::Normal);
        let kit = Kit::with_delivery(FakeDelivery::unavailable("macOS"));
        let _ = advance.choose_route(RouteRequest::Direct, &kit.ports());
        assert_eq!(
            advance.leave_clipboard(&kit.ports()),
            vec![Message::NoDirectRoute {
                system: "macOS".to_owned()
            }]
        );
        assert_eq!(advance.sequence().delivery, Delivery::ClipboardMode);
    }

    #[test]
    fn a_route_the_system_holds_shut_starts_in_clipboard_mode_and_says_how_to_open_it() {
        // `D138`: the permission is the tester's to give, so the sentence is not
        // the one for a build that cannot type, and the click that would leave
        // the mode finds the same door shut and says the same thing.
        let mut advance = chosen(Risk::Normal);
        let kit = Kit::with_delivery(FakeDelivery::needs_permission());
        assert_eq!(
            advance.choose_route(RouteRequest::Direct, &kit.ports()),
            vec![Message::NeedsPermission]
        );
        assert_eq!(advance.sequence().delivery, Delivery::ClipboardMode);
        assert_eq!(
            advance.leave_clipboard(&kit.ports()),
            vec![Message::NeedsPermission]
        );
        assert_eq!(advance.sequence().delivery, Delivery::ClipboardMode);
    }

    #[test]
    fn a_permission_taken_back_while_running_moves_the_value_to_the_clipboard() {
        // The route was open at the start and the system refuses now: the press
        // that found out is not lost, the same value goes to the clipboard.
        let mut advance = chosen(Risk::Normal);
        let kit = Kit::with_delivery(FakeDelivery::failing(DeliveryError::NeedsPermission));
        let outcome = advance.on_action(HotkeyAction::NextValue, &kit.ports());
        assert_eq!(outcome.messages, vec![Message::NeedsPermission]);
        assert_eq!(advance.sequence().delivery, Delivery::ClipboardMode);
        assert_eq!(*kit.by_clipboard.handed.borrow(), vec!["alpha"]);
        assert!(
            outcome
                .sent
                .is_some_and(|sent| sent.arrival == Arrival::OnClipboard)
        );
        assert_eq!(advance.counter(), Some((1, 3)));
    }

    #[test]
    fn a_permission_found_missing_by_the_clearing_is_the_same_move() {
        // The clearing goes first, so on a machine that holds the route shut the
        // keys answer before the value can - and it is no failed clear.
        let mut advance = chosen(Risk::Normal);
        let kit = Kit::with_keys(FakeKeys::failing(KeystrokeError::NeedsPermission));
        let outcome = advance.on_action(HotkeyAction::NextValue, &kit.ports());
        assert_eq!(outcome.messages, vec![Message::NeedsPermission]);
        assert!(!outcome.messages.contains(&Message::ClearingFailed));
        assert_eq!(advance.sequence().delivery, Delivery::ClipboardMode);
        assert!(kit.direct.handed.borrow().is_empty());
    }

    #[test]
    fn a_busy_route_refuses_the_value_in_its_own_words_and_keeps_the_counter() {
        let mut advance = chosen(Risk::Normal);
        // The keys of the display, held by another send of this program (`D157`).
        let typing = Kit::with_delivery(FakeDelivery::failing(DeliveryError::Busy));
        let outcome = advance.on_action(HotkeyAction::NextValue, &typing.ports());
        assert_eq!(outcome.messages, vec![Message::TypingBusy]);
        assert_eq!(
            advance.sequence().delivery,
            Delivery::Direct,
            "no clipboard"
        );
        assert_eq!(advance.counter(), Some((0, 3)));
        // The clipboard, held by another application.
        let kit = Kit::with_by_clipboard(FakeDelivery::failing(DeliveryError::Busy));
        let _ = advance.choose_route(RouteRequest::Clipboard, &kit.ports());
        let outcome = advance.on_action(HotkeyAction::NextValue, &kit.ports());
        assert_eq!(outcome.messages, vec![Message::ClipboardBusy]);
        assert!(outcome.sent.is_none());
        assert_eq!(advance.sequence().position, Position::Ready { total: 3 });
        assert_eq!(advance.sequence().delivery, Delivery::ClipboardMode);
    }

    #[test]
    fn a_refusing_clipboard_passes_its_words_on_for_a_value_too() {
        let mut advance = chosen(Risk::Normal);
        let kit = Kit::with_by_clipboard(FakeDelivery::failing(DeliveryError::Refused {
            detail: "no display".to_owned(),
        }));
        let _ = advance.choose_route(RouteRequest::Clipboard, &kit.ports());
        let outcome = advance.on_action(HotkeyAction::NextValue, &kit.ports());
        assert_eq!(
            outcome.messages,
            vec![Message::ClipboardFailed {
                detail: "no display".to_owned()
            }]
        );
        assert_eq!(advance.counter(), Some((0, 3)));
    }

    #[test]
    fn a_value_the_clipboard_cannot_carry_is_refused_by_name() {
        let mut advance = chosen(Risk::Normal);
        let kit = Kit::with_by_clipboard(FakeDelivery::failing(DeliveryError::CannotCarry {
            character: '\0',
        }));
        let _ = advance.choose_route(RouteRequest::Clipboard, &kit.ports());
        let outcome = advance.on_action(HotkeyAction::NextValue, &kit.ports());
        assert_eq!(
            outcome.messages,
            vec![Message::NotForClipboard {
                id: "one".to_owned(),
                character: '\0'
            }]
        );
        assert_eq!(advance.counter(), Some((0, 3)), "OBS-130: it stays put");
    }

    #[test]
    fn a_failing_clipboard_after_the_switch_is_said_after_the_switch() {
        // Both halves reach the tester, in order: why the mode changed, then
        // why this value is not on the clipboard either.
        let mut advance = chosen(Risk::Normal);
        let kit = Kit {
            direct: FakeDelivery::failing(DeliveryError::Unsupported {
                system: "macOS".to_owned(),
            }),
            by_clipboard: FakeDelivery::failing(DeliveryError::Busy),
            ..Kit::ready()
        };
        let outcome = advance.on_action(HotkeyAction::NextValue, &kit.ports());
        assert_eq!(
            outcome.messages,
            vec![
                Message::NoDirectRoute {
                    system: "macOS".to_owned()
                },
                Message::ClipboardBusy
            ]
        );
        assert!(!advance.sequence().is_inserting(), "nothing left in flight");
        assert_eq!(advance.sequence().delivery, Delivery::ClipboardMode);
        assert_eq!(advance.counter(), Some((0, 3)));
    }

    #[test]
    fn the_report_of_a_pasted_value_says_it_went_by_the_clipboard() {
        let mut advance = chosen(Risk::Normal);
        let kit = Kit::ready();
        let _ = advance.choose_route(RouteRequest::Clipboard, &kit.ports());
        let _ = advance.on_action(HotkeyAction::NextValue, &kit.ports());
        let _ = advance.on_action(HotkeyAction::CopyReport, &kit.ports());
        assert_eq!(kit.text.blocks.borrow()[0].arrival, Arrival::OnClipboard);
    }

    #[test]
    fn a_partial_send_reports_a_fragment_and_does_not_degrade() {
        let mut advance = chosen(Risk::Normal);
        let outcome = advance.on_action(
            HotkeyAction::NextValue,
            &Kit::with_delivery(FakeDelivery::failing(DeliveryError::Partial {
                units_sent: 3,
                units_expected: 5,
                reason: StopReason::NotTaking,
            }))
            .ports(),
        );
        assert_eq!(
            outcome.messages,
            vec![Message::Interrupted {
                units_sent: 3,
                units_expected: 5,
                reason: StopReason::NotTaking,
            }]
        );
        assert_eq!(advance.sequence().delivery, Delivery::Direct);
    }

    #[test]
    fn a_held_modifier_refuses_and_names_it_without_degrading() {
        let mut advance = chosen(Risk::Normal);
        let outcome = advance.on_action(
            HotkeyAction::NextValue,
            &Kit::with_delivery(FakeDelivery::failing(DeliveryError::ModifierHeld {
                which: "Ctrl".to_owned(),
            }))
            .ports(),
        );
        assert_eq!(
            outcome.messages,
            vec![Message::ModifierHeld {
                key: "Ctrl".to_owned()
            }]
        );
        assert_eq!(advance.sequence().delivery, Delivery::Direct);
        assert_eq!(advance.sequence().position, Position::Ready { total: 3 });
    }

    #[test]
    fn caps_lock_on_refuses_without_degrading_and_the_same_value_waits() {
        // A state somebody left on: nothing was sent, the route stays direct and
        // the counter stays, so the press after Caps Lock is off sends this value.
        let mut advance = chosen(Risk::Normal);
        let outcome = advance.on_action(
            HotkeyAction::NextValue,
            &Kit::with_delivery(FakeDelivery::failing(DeliveryError::CapsLockOn)).ports(),
        );
        assert_eq!(outcome.messages, vec![Message::CapsLockOn]);
        assert!(outcome.sent.is_none());
        assert_eq!(advance.sequence().delivery, Delivery::Direct);
        assert_eq!(advance.sequence().position, Position::Ready { total: 3 });
    }

    #[test]
    fn a_failure_to_clear_refuses_the_value() {
        let mut advance = chosen(Risk::Normal);
        let outcome = advance.on_action(
            HotkeyAction::NextValue,
            &Kit::with_keys(FakeKeys::failing(KeystrokeError::ModifierHeld {
                which: "Alt".to_owned(),
            }))
            .ports(),
        );
        assert_eq!(
            outcome.messages,
            vec![Message::ModifierHeld {
                key: "Alt".to_owned()
            }]
        );
        assert!(outcome.sent.is_none());
        assert_eq!(advance.sequence().delivery, Delivery::Direct);
    }

    #[test]
    fn the_value_shown_as_next_is_the_value_the_next_press_sends() {
        // UX-GUI-001: the palette names the next value before the press, and W5
        // asks the value shown to be the value sent - before the press as well.
        let mut advance = chosen(Risk::Normal);
        let kit = Kit::ready();
        let mut promised = advance.upcoming();
        for _ in 0..3 {
            let Some(UpcomingValue::Value {
                reference,
                name,
                key,
                ..
            }) = promised.clone()
            else {
                panic!(
                    "a three-value pack has a value upcoming before each of three presses: {promised:?}"
                );
            };
            let outcome = advance.on_action(HotkeyAction::NextValue, &kit.ports());
            let sent = outcome.sent.as_ref().expect("a value goes out");
            assert_eq!(sent.facts.reference, reference);
            assert_eq!(sent.facts.name, name);
            // `D98`: the Copy button beside the next value copies the value the
            // press sends, and the one beside the value sent copies that value.
            assert_eq!(sent.key, key);
            assert_eq!(format!("{}/{}", key.pack, key.value), reference);
            // What the outcome carries is what the sequence says AFTER the press.
            assert_eq!(outcome.upcoming, advance.upcoming());
            promised = outcome.upcoming;
        }
        assert_eq!(promised, Some(UpcomingValue::EndOfPack { total: 3 }));
        let warned = advance.on_action(HotkeyAction::NextValue, &kit.ports());
        assert!(
            warned.sent.is_none(),
            "the end of the pack is said, not sent"
        );
        assert!(matches!(
            warned.upcoming,
            Some(UpcomingValue::Value { index: 1, .. })
        ));
    }

    #[test]
    fn a_value_chosen_in_the_value_window_is_the_next_one_sent() {
        let mut advance = chosen(Risk::Normal);
        let kit = Kit::ready();
        assert!(advance.choose_next("three"));
        assert!(matches!(
            advance.upcoming(),
            Some(UpcomingValue::Value { index: 3, key, .. }) if key.value == "three"
        ));
        let outcome = advance.on_action(HotkeyAction::NextValue, &kit.ports());
        assert_eq!(
            outcome
                .sent
                .expect("the chosen value goes out")
                .facts
                .reference,
            "sample/three"
        );
        // From the end of the pack, back to its first value.
        assert!(advance.choose_next("one"));
        assert!(matches!(
            advance.upcoming(),
            Some(UpcomingValue::Value { index: 1, .. })
        ));
    }

    /// `D109`: after the welcome window's box, testing starts at value 1 - and
    /// the value that went into the box is still the last one sent.
    #[test]
    fn starting_over_makes_value_one_next_and_keeps_the_last_value_sent() {
        let mut advance = chosen(Risk::Normal);
        let kit = Kit::ready();
        let _ = advance.on_action(HotkeyAction::NextValue, &kit.ports());
        let _ = advance.on_action(HotkeyAction::NextValue, &kit.ports());
        assert!(matches!(
            advance.upcoming(),
            Some(UpcomingValue::Value { index: 3, .. })
        ));
        assert!(advance.start_over());
        assert!(matches!(
            advance.upcoming(),
            Some(UpcomingValue::Value { index: 1, .. })
        ));
        let block = advance.on_action(HotkeyAction::CopyReport, &kit.ports());
        assert!(
            block.messages.iter().any(|message| matches!(
                message,
                Message::ReportCopied { reference } if reference == "sample/two"
            )),
            "the value that went into the box is still the last one sent: {:?}",
            block.messages
        );
        assert!(
            !AdvanceSequence::new(Box::new(StoppedClock::default())).start_over(),
            "no pack, nothing to start over"
        );
    }

    #[test]
    fn an_unknown_value_or_no_pack_moves_nothing() {
        let mut advance = chosen(Risk::Normal);
        let before = advance.sequence();
        assert!(!advance.choose_next("nope"));
        assert_eq!(advance.sequence(), before);
        assert!(!AdvanceSequence::new(Box::new(StoppedClock::default())).choose_next("one"));
    }

    #[test]
    fn an_offensive_value_is_marked_before_it_goes_and_nothing_is_upcoming_without_a_pack() {
        assert!(matches!(
            chosen(Risk::Offensive).upcoming(),
            Some(UpcomingValue::Value {
                index: 1,
                total: 3,
                offensive: true,
                ..
            })
        ));
        assert!(matches!(
            chosen(Risk::Normal).upcoming(),
            Some(UpcomingValue::Value {
                offensive: false,
                ..
            })
        ));
        assert_eq!(
            AdvanceSequence::new(Box::new(StoppedClock::default())).upcoming(),
            None
        );
    }

    #[test]
    fn the_next_value_says_which_half_of_a_pair_it_is_and_a_lone_one_says_nothing() {
        let mut pack = a_pack(
            vec![
                a_value("one", "alpha", None),
                a_value("two", "beta", None),
                a_value("three", "gamma", None),
            ],
            Risk::Normal,
        );
        pack.pairs = vec![nkb_core::pack::PackPair {
            id: "one-and-two".to_owned(),
            name: "One against two".to_owned(),
            relation: "look-alike".to_owned(),
            a: "one".to_owned(),
            b: "two".to_owned(),
            breaks: None,
            expect: None,
            since: Some("1.0".to_owned()),
        }];
        let mut advance = AdvanceSequence::new(Box::new(StoppedClock::default()));
        advance
            .choose_pack(&FakeSource, &FakeFormat::of(pack), "sample")
            .expect("a scripted pack with no errors loads");
        let halves = |advance: &AdvanceSequence| {
            let Some(UpcomingValue::Value { halves, .. }) = advance.upcoming() else {
                panic!("a value is upcoming: {:?}", advance.upcoming());
            };
            halves
        };
        assert_eq!(halves(&advance), [PairHalf::First]);
        assert!(advance.choose_next("two"));
        assert_eq!(halves(&advance), [PairHalf::Second]);
        assert!(advance.choose_next("three"));
        assert!(halves(&advance).is_empty());
    }

    #[test]
    fn an_unwired_action_says_so_rather_than_doing_nothing() {
        let mut advance = chosen(Risk::Normal);
        let outcome = advance.on_action(HotkeyAction::MarkProblem, &Kit::ready().ports());
        assert_eq!(
            outcome.messages,
            vec![Message::Unhandled {
                action: HotkeyAction::MarkProblem
            }]
        );
        assert_eq!(advance.counter(), Some((0, 3)));
    }

    /// 🔴 `handles` is what the palette shows the tester as working, so it is
    /// held to what a press really does - for every action, both ways: an
    /// action it claims is never answered `Unhandled`, and one it does not
    /// claim always is. Through fakes, so a send here types into nothing.
    #[test]
    fn handles_says_exactly_which_actions_are_carried_out() {
        let kit = Kit::ready();
        for action in HotkeyAction::ALL {
            let mut advance = chosen(Risk::Normal);
            let outcome = advance.on_action(action, &kit.ports());
            let unhandled = outcome.messages.contains(&Message::Unhandled { action });
            assert_eq!(
                AdvanceSequence::handles(action),
                !unhandled,
                "{action:?}: handles() says {}, the press answered {:?}",
                AdvanceSequence::handles(action),
                outcome.messages
            );
        }
    }

    #[test]
    fn a_step_moves_the_next_value_and_types_nothing() {
        // `D120`: the band shows a different value and the field is untouched -
        // no key leaves for a skip or a step back, on either route.
        let kit = Kit::ready();
        let mut advance = chosen(Risk::Normal);
        let skipped = advance.on_action(HotkeyAction::SkipValue, &kit.ports());
        assert!(skipped.sent.is_none());
        assert!(skipped.messages.is_empty(), "{:?}", skipped.messages);
        assert!(
            matches!(
                skipped.upcoming,
                Some(UpcomingValue::Value { index: 2, .. })
            ),
            "{:?}",
            skipped.upcoming
        );
        let back = advance.on_action(HotkeyAction::BackOneValue, &kit.ports());
        assert!(matches!(
            back.upcoming,
            Some(UpcomingValue::Value { index: 1, .. })
        ));
        let first = advance.on_action(HotkeyAction::BackOneValue, &kit.ports());
        assert!(first.sent.is_none());
        assert!(
            matches!(first.messages.as_slice(), [Message::AtFirstValue { .. }]),
            "{:?}",
            first.messages
        );
        assert!(
            kit.direct.handed.borrow().is_empty(),
            "a step typed a value"
        );
        assert!(
            kit.by_clipboard.handed.borrow().is_empty(),
            "a step copied a value"
        );
        assert_eq!(*kit.keys.requests.borrow(), 0, "a step pressed a key");
    }

    #[test]
    fn next_with_no_pack_chosen_says_no_pack() {
        let mut advance = AdvanceSequence::new(Box::new(StoppedClock::default()));
        let outcome = advance.on_action(HotkeyAction::NextValue, &Kit::ready().ports());
        assert_eq!(outcome.messages, vec![Message::NoPack]);
        assert!(outcome.sent.is_none());
    }

    #[test]
    fn a_missing_pack_is_reported_not_panicked() {
        let mut advance = AdvanceSequence::new(Box::new(StoppedClock::default()));
        let result = advance.choose_pack(&MissingSource, &FakeFormat::with_errors(0), "absent");
        assert_eq!(result, Err(ChooseError::NotFound));
    }

    #[test]
    fn a_pack_with_errors_is_refused_and_says_how_many() {
        let mut advance = AdvanceSequence::new(Box::new(StoppedClock::default()));
        let result = advance.choose_pack(&FakeSource, &FakeFormat::with_errors(3), "sample");
        assert_eq!(result, Err(ChooseError::Refused { errors: 3 }));
    }

    #[test]
    fn an_offensive_value_is_marked_from_the_pack_default() {
        let mut advance = chosen(Risk::Offensive);
        let outcome = advance.on_action(HotkeyAction::NextValue, &Kit::ready().ports());
        let sent = outcome.sent.expect("sent");
        assert!(sent.offensive, "an offensive pack marks its values");
    }

    /// Nothing behind: the press types nothing and says so - not the sentence
    /// about a new field, which it said until the owner's point 6 (`D127`).
    #[test]
    fn previous_at_the_start_types_nothing_and_says_there_is_nothing_behind() {
        let mut advance = chosen(Risk::Normal);
        let outcome = advance.on_action(HotkeyAction::PreviousValue, &Kit::ready().ports());
        assert_eq!(outcome.messages, vec![Message::NothingBefore]);
        assert!(outcome.sent.is_none());
    }

    // ---- the report block: one of the two doors to the clipboard -----------

    #[test]
    fn a_report_before_anything_went_out_writes_nothing_and_says_so() {
        let mut advance = chosen(Risk::Normal);
        let kit = Kit::ready();

        let outcome = advance.on_action(HotkeyAction::CopyReport, &kit.ports());

        assert_eq!(outcome.messages, vec![Message::NothingToReport]);
        assert!(
            kit.clipboard.puts.borrow().is_empty(),
            "nothing was asked for, so nothing touches the clipboard"
        );
        assert!(!outcome.attempted_send, "a report answers at once");
    }

    #[test]
    fn a_report_after_a_send_copies_the_block_of_that_value_once() {
        let mut advance = chosen(Risk::Normal);
        // The sends on their own kit: each builds its block for the palette
        // (`D121`), and this test counts the report's.
        let _ = advance.on_action(HotkeyAction::NextValue, &Kit::ready().ports());
        let _ = advance.on_action(HotkeyAction::NextValue, &Kit::ready().ports());
        let kit = Kit::ready();

        let outcome = advance.on_action(HotkeyAction::CopyReport, &kit.ports());

        assert_eq!(
            outcome.messages,
            vec![Message::ReportCopied {
                reference: "sample/two".to_owned()
            }]
        );
        assert_eq!(*kit.clipboard.puts.borrow(), vec!["report of sample/two"]);
        assert_eq!(
            *kit.clipboard.histories.borrow(),
            vec![History::Keep],
            "the block stays in the clipboard history on purpose (D68)"
        );
        let blocks = kit.text.blocks.borrow();
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].pack_version, "1.0");
        assert_eq!(blocks[0].arrival, Arrival::Whole);
        assert!(
            outcome.sent.is_none(),
            "the palette keeps the value it shows"
        );
        assert_eq!(advance.counter(), Some((2, 3)), "a report moves nothing");
    }

    /// `D121`: the block the palette shows with a value is the block the
    /// report shortcut copies while that value is the last one - the same
    /// block, through the same text port.
    #[test]
    fn the_block_shown_with_a_value_is_the_block_the_report_copies() {
        let mut advance = chosen(Risk::Normal);
        let kit = Kit::ready();
        let _ = advance.on_action(HotkeyAction::NextValue, &kit.ports());
        let sent = advance
            .on_action(HotkeyAction::NextValue, &kit.ports())
            .sent
            .expect("value two went out");
        let _ = advance.on_action(HotkeyAction::CopyReport, &kit.ports());

        assert_eq!(sent.report.as_deref(), Some("report of sample/two"));
        assert_eq!(*kit.clipboard.puts.borrow(), vec!["report of sample/two"]);
        let blocks = kit.text.blocks.borrow();
        assert_eq!(
            blocks.len(),
            3,
            "one block per value sent, and the report's"
        );
        assert_eq!(
            blocks[1], blocks[2],
            "the report copied the block shown with value two"
        );
    }

    #[test]
    fn a_refused_send_leaves_the_report_on_the_value_before_it() {
        // Nothing new reached the field, so the block describes what is there -
        // the same value the palette keeps on screen after a refusal.
        let mut advance = chosen(Risk::Normal);
        let _ = advance.on_action(HotkeyAction::NextValue, &Kit::ready().ports());
        let _ = advance.on_action(
            HotkeyAction::NextValue,
            &Kit::with_delivery(FakeDelivery::without_target()).ports(),
        );
        let kit = Kit::ready();

        let outcome = advance.on_action(HotkeyAction::CopyReport, &kit.ports());

        assert_eq!(
            outcome.messages,
            vec![Message::ReportCopied {
                reference: "sample/one".to_owned()
            }]
        );
    }

    #[test]
    fn an_interrupted_send_is_reported_as_interrupted_not_skipped() {
        // Part of the value is in the field. A block that described the value
        // before would be about something the application no longer holds.
        let mut advance = chosen(Risk::Normal);
        let _ = advance.on_action(HotkeyAction::NextValue, &Kit::ready().ports());
        let _ = advance.on_action(
            HotkeyAction::NextValue,
            &Kit::with_delivery(FakeDelivery::failing(DeliveryError::Partial {
                units_sent: 2,
                units_expected: 4,
                reason: StopReason::NotTaking,
            }))
            .ports(),
        );
        let kit = Kit::ready();

        let _ = advance.on_action(HotkeyAction::CopyReport, &kit.ports());

        let blocks = kit.text.blocks.borrow();
        assert_eq!(blocks[0].reference, "sample/two");
        assert_eq!(
            blocks[0].arrival,
            Arrival::Interrupted {
                units_sent: 2,
                units_expected: 4
            }
        );
    }

    // ---- `OBS-126`: the palette shows the value that was cut short -------------

    #[test]
    fn an_interrupted_send_shows_the_value_it_cut_short_and_the_block_agrees() {
        // The palette used to keep value ONE on screen beside a sentence about
        // value TWO being half-written, while the block described value two.
        // Now both answer from the same function, and this holds them to it.
        let mut advance = chosen(Risk::Normal);
        let _ = advance.on_action(HotkeyAction::NextValue, &Kit::ready().ports());
        let outcome = advance.on_action(
            HotkeyAction::NextValue,
            &Kit::with_delivery(FakeDelivery::failing(DeliveryError::Partial {
                units_sent: 2,
                units_expected: 4,
                reason: StopReason::NotTaking,
            }))
            .ports(),
        );
        let shown = outcome.sent.expect("the value cut short is shown");
        assert_eq!(shown.facts.reference, "sample/two");
        let cut = Arrival::Interrupted {
            units_sent: 2,
            units_expected: 4,
        };
        assert_eq!(shown.arrival, cut);
        assert_eq!(
            advance.counter(),
            Some((1, 3)),
            "a value cut short is shown, and it is still not a value sent"
        );

        let kit = Kit::ready();
        let _ = advance.on_action(HotkeyAction::CopyReport, &kit.ports());
        let blocks = kit.text.blocks.borrow();
        assert_eq!(blocks[0].reference, shown.facts.reference);
        assert_eq!(blocks[0].arrival, shown.arrival);
    }

    #[test]
    fn an_interrupted_send_on_an_uncleared_field_says_both_worst_first() {
        // The field held something before, the clearing kept its keys out, and
        // then the value stopped part-way: two true things, and the fragment is
        // the worse one. Until `OBS-126` the second was not said at all.
        let mut advance = chosen(Risk::Normal);
        let kit = Kit {
            direct: FakeDelivery::failing(DeliveryError::Partial {
                units_sent: 1,
                units_expected: 5,
                reason: StopReason::NotTaking,
            }),
            keys: FakeKeys::failing(KeystrokeError::FieldUnconfirmed),
            ..Kit::ready()
        };
        let outcome = advance.on_action(HotkeyAction::NextValue, &kit.ports());
        assert_eq!(
            outcome.messages,
            vec![
                Message::Interrupted {
                    units_sent: 1,
                    units_expected: 5,
                    reason: StopReason::NotTaking,
                },
                Message::ClearingSkipped,
            ]
        );
        let shown = outcome.sent.expect("the value cut short is shown");
        assert!(!shown.cleared);
        assert_eq!(
            shown.utf16_units, 5,
            "the size of the whole value, not of the fragment"
        );
    }

    #[test]
    fn a_busy_clipboard_is_named_and_nothing_is_claimed_copied() {
        let mut advance = chosen(Risk::Normal);
        let _ = advance.on_action(HotkeyAction::NextValue, &Kit::ready().ports());

        let outcome = advance.on_action(
            HotkeyAction::CopyReport,
            &Kit::with_clipboard(FakeClipboard::failing(ClipboardError::Busy)).ports(),
        );

        assert_eq!(outcome.messages, vec![Message::ReportBusy]);
    }

    #[test]
    fn a_refusing_clipboard_passes_its_own_words_on() {
        let mut advance = chosen(Risk::Normal);
        let _ = advance.on_action(HotkeyAction::NextValue, &Kit::ready().ports());

        let outcome = advance.on_action(
            HotkeyAction::CopyReport,
            &Kit::with_clipboard(FakeClipboard::failing(ClipboardError::Failed {
                detail: "no display".to_owned(),
            }))
            .ports(),
        );

        assert_eq!(
            outcome.messages,
            vec![Message::ReportFailed {
                detail: "no display".to_owned()
            }]
        );
    }

    #[test]
    fn sending_never_touches_the_clipboard() {
        // The other half of the promise: a whole walk through the pack, with
        // every refusal in between, writes nothing until the tester asks.
        let mut advance = chosen(Risk::Normal);
        let kit = Kit::ready();
        for _ in 0..5 {
            let _ = advance.on_action(HotkeyAction::NextValue, &kit.ports());
        }
        let _ = advance.on_action(HotkeyAction::PreviousValue, &kit.ports());
        let _ = advance.on_action(HotkeyAction::RestartPack, &kit.ports());

        // The blocks ARE built - one per value that went out, for the palette
        // to show (`D121`) - and none of them reaches the clipboard.
        assert!(kit.clipboard.puts.borrow().is_empty());
        assert!(
            kit.by_clipboard.handed.borrow().is_empty(),
            "in direct mode the clipboard route is never taken for a window that takes typing (D71, D72)"
        );
    }

    // ---- `D72`: a window running with higher privileges ------------------------

    /// The direct route as it answers for a window with higher privileges: the
    /// clearing goes first, and it refuses before pressing anything.
    fn a_higher_window() -> Kit {
        let kit = Kit::with_keys(FakeKeys::failing(KeystrokeError::HigherPrivileges));
        kit.direct.set_target(Some(TargetRef(7)));
        kit
    }

    #[test]
    fn a_window_with_higher_privileges_gets_the_same_value_on_the_clipboard_in_the_same_press() {
        let mut advance = chosen(Risk::Normal);
        let kit = a_higher_window();
        let outcome = advance.on_action(HotkeyAction::NextValue, &kit.ports());

        assert!(
            kit.direct.handed.borrow().is_empty(),
            "nothing may be typed at a window that drops typing"
        );
        assert_eq!(*kit.by_clipboard.handed.borrow(), vec!["alpha"]);
        assert_eq!(outcome.messages, vec![Message::HigherPrivileges]);
        let sent = outcome.sent.expect("value one went to the clipboard");
        assert!(sent.arrival == Arrival::OnClipboard && !sent.cleared);
        assert_eq!(
            advance.counter(),
            Some((1, 3)),
            "the value went out, so the counter moves"
        );
        assert!(
            outcome.clipboard_for_window,
            "the bar says so for this window"
        );
        assert_eq!(
            outcome.sequence.delivery,
            Delivery::Direct,
            "a window is not a mode: the sequence stays on direct delivery (ux-spec.md 8)"
        );
    }

    #[test]
    fn the_window_is_named_once_and_its_next_values_go_the_same_way() {
        let mut advance = chosen(Risk::Normal);
        let kit = a_higher_window();
        let _ = advance.on_action(HotkeyAction::NextValue, &kit.ports());
        let second = advance.on_action(HotkeyAction::NextValue, &kit.ports());

        assert!(
            second.messages.is_empty(),
            "the bar says it, not every press"
        );
        assert!(second.sent.expect("value two went out").arrival == Arrival::OnClipboard);
        assert!(second.clipboard_for_window);
        assert_eq!(*kit.by_clipboard.handed.borrow(), vec!["alpha", "beta"]);
    }

    #[test]
    fn another_window_in_front_takes_typing_again_and_the_bar_goes() {
        let mut advance = chosen(Risk::Normal);
        let higher = a_higher_window();
        let _ = advance.on_action(HotkeyAction::NextValue, &higher.ports());

        let ordinary = Kit::ready();
        ordinary.direct.set_target(Some(TargetRef(8)));
        let outcome = advance.on_action(HotkeyAction::NextValue, &ordinary.ports());

        assert_eq!(*ordinary.direct.handed.borrow(), vec!["beta"]);
        assert!(ordinary.by_clipboard.handed.borrow().is_empty());
        assert!(outcome.sent.expect("typed").arrival != Arrival::OnClipboard);
        assert!(!outcome.clipboard_for_window);
        assert!(outcome.messages.is_empty());
    }

    #[test]
    fn coming_back_to_the_higher_window_names_it_again() {
        // "Once per window" is once per VISIT: the tester left and came back, and
        // the reason the value is on the clipboard is news again.
        let mut advance = chosen(Risk::Normal);
        let higher = a_higher_window();
        let _ = advance.on_action(HotkeyAction::NextValue, &higher.ports());
        let ordinary = Kit::ready();
        ordinary.direct.set_target(Some(TargetRef(8)));
        let _ = advance.on_action(HotkeyAction::NextValue, &ordinary.ports());
        let back = advance.on_action(HotkeyAction::NextValue, &higher.ports());
        assert_eq!(back.messages, vec![Message::HigherPrivileges]);
    }

    #[test]
    fn a_refusal_from_the_send_itself_takes_the_same_way() {
        // The clearing went through and the window changed before the value:
        // the send refuses on its own, and the value still reaches the clipboard
        // rather than nowhere.
        let mut advance = chosen(Risk::Normal);
        let kit = Kit::with_delivery(FakeDelivery::failing(DeliveryError::HigherPrivileges));
        let outcome = advance.on_action(HotkeyAction::NextValue, &kit.ports());

        assert_eq!(*kit.by_clipboard.handed.borrow(), vec!["alpha"]);
        assert_eq!(outcome.messages, vec![Message::HigherPrivileges]);
        assert!(outcome.sent.expect("on the clipboard").arrival == Arrival::OnClipboard);
    }

    #[test]
    fn the_report_block_says_the_tester_pasted_it() {
        let mut advance = chosen(Risk::Normal);
        let kit = a_higher_window();
        let _ = advance.on_action(HotkeyAction::NextValue, &kit.ports());
        let _ = advance.on_action(HotkeyAction::CopyReport, &kit.ports());
        let blocks = kit.text.blocks.borrow();
        let block = blocks.first().expect("a block was written");
        assert_eq!(block.arrival, Arrival::OnClipboard);
    }

    #[test]
    fn clipboard_mode_never_asks_about_the_window() {
        // In clipboard mode nothing is typed, so there is nothing a window could
        // refuse - and no second bar beside the first.
        let mut advance = chosen(Risk::Normal);
        let kit = a_higher_window();
        let _ = advance.choose_route(RouteRequest::Clipboard, &kit.ports());
        let outcome = advance.on_action(HotkeyAction::NextValue, &kit.ports());
        assert!(outcome.messages.is_empty());
        assert!(!outcome.clipboard_for_window);
        assert_eq!(*kit.keys.requests.borrow(), 0);
    }

    #[test]
    fn idling_lets_the_bar_go_only_when_another_window_comes_forward() {
        let mut advance = chosen(Risk::Normal);
        let kit = a_higher_window();
        let _ = advance.on_action(HotkeyAction::NextValue, &kit.ports());

        assert_eq!(
            advance.on_idle(&kit.ports()),
            None,
            "the same window is still in front"
        );
        kit.direct.set_target(Some(TargetRef(8)));
        let outcome = advance
            .on_idle(&kit.ports())
            .expect("the window left, so the bar goes");
        assert!(!outcome.clipboard_for_window);
        assert!(outcome.sent.is_none() && outcome.messages.is_empty());
        assert!(!outcome.attempted_send);
        assert_eq!(
            advance.on_idle(&kit.ports()),
            None,
            "said once, not every tick"
        );
    }

    #[test]
    fn idling_without_the_bar_changes_nothing() {
        let mut advance = chosen(Risk::Normal);
        let kit = Kit::ready();
        let _ = advance.on_action(HotkeyAction::NextValue, &kit.ports());
        kit.direct.set_target(Some(TargetRef(9)));
        assert_eq!(advance.on_idle(&kit.ports()), None);
    }

    // ---- `D73`: the keyboard focus is not in a text field ----------------------

    #[test]
    fn a_focus_outside_a_field_refuses_before_the_clearing_and_keeps_the_counter() {
        // The clearing goes first, so on a list of files this is where
        // `Home`, `Shift+End`, `Delete` are stopped (`OBS-135`).
        let mut advance = chosen(Risk::Normal);
        let kit = Kit::with_keys(FakeKeys::failing(KeystrokeError::NoTextField));
        let outcome = advance.on_action(HotkeyAction::NextValue, &kit.ports());

        assert!(
            kit.direct.handed.borrow().is_empty(),
            "nothing may be typed when the focus is on a button or a list item"
        );
        assert!(
            kit.by_clipboard.handed.borrow().is_empty(),
            "not a window problem, so not the clipboard either - the tester clicks into a field"
        );
        assert_eq!(outcome.messages, vec![Message::NoTextField]);
        assert!(outcome.sent.is_none());
        assert_eq!(
            advance.counter(),
            Some((0, 3)),
            "nothing went out, so the counter stays and the same value goes next"
        );
        assert!(!outcome.clipboard_for_window);
    }

    // ---- `D76`: a focus that could not be confirmed as a text field --------------

    #[test]
    fn an_unconfirmed_field_gets_the_value_uncleared_the_counter_moves_and_it_is_said() {
        // The door pressed nothing: an application that does not say what holds
        // its focus may be a grid, where the recipe emptied a whole row. The
        // value is not refused - it lands where the tester put it, on top of
        // what was there, and the palette says so.
        let mut advance = chosen(Risk::Normal);
        let kit = Kit::with_keys(FakeKeys::failing(KeystrokeError::FieldUnconfirmed));
        let outcome = advance.on_action(HotkeyAction::NextValue, &kit.ports());

        assert_eq!(*kit.direct.handed.borrow(), vec!["alpha"]);
        assert!(kit.by_clipboard.handed.borrow().is_empty());
        assert_eq!(outcome.messages, vec![Message::ClearingSkipped]);
        let sent = outcome.sent.expect("the value went in");
        assert!(
            !sent.cleared,
            "the marker must not say the field was cleared"
        );
        assert_eq!(advance.counter(), Some((1, 3)));
    }

    // ---- `OBS-141`: a terminal ---------------------------------------------------

    #[test]
    fn a_terminal_gets_the_value_uncleared_the_counter_moves_and_the_terminal_is_named() {
        // The door pressed nothing: in a terminal the clearing keys go to the
        // program running in it. The value lands, and the sentence names the
        // terminal - the unconfirmed-field one would say the system does not
        // report a field, which here it does.
        let mut advance = chosen(Risk::Normal);
        let kit = Kit::with_keys(FakeKeys::failing(KeystrokeError::InTerminal));
        let outcome = advance.on_action(HotkeyAction::NextValue, &kit.ports());

        assert_eq!(*kit.direct.handed.borrow(), vec!["alpha"]);
        assert!(kit.by_clipboard.handed.borrow().is_empty());
        assert_eq!(outcome.messages, vec![Message::ClearingSkippedInTerminal]);
        let sent = outcome.sent.expect("the value went in");
        assert!(!sent.cleared);
        assert_eq!(advance.counter(), Some((1, 3)));
    }

    // ---- `D125`: a window closed to accessibility ----------------------------------

    #[test]
    fn a_window_closed_to_accessibility_gets_the_value_uncleared_and_its_setting_is_named() {
        // The door pressed nothing, for the reason of an unconfirmed field: the
        // window shows nothing inside it. The value lands as it does there, and
        // the sentence is the one that names the setting - not the general one,
        // which a tester took for a fault of the tool (`OBS-175`).
        let mut advance = chosen(Risk::Normal);
        let kit = Kit::with_keys(FakeKeys::failing(KeystrokeError::ClosedToAccessibility));
        let outcome = advance.on_action(HotkeyAction::NextValue, &kit.ports());

        assert_eq!(*kit.direct.handed.borrow(), vec!["alpha"]);
        assert!(kit.by_clipboard.handed.borrow().is_empty());
        assert_eq!(
            outcome.messages,
            vec![Message::ClearingSkippedClosedToAccessibility]
        );
        let sent = outcome.sent.expect("the value went in");
        assert!(!sent.cleared);
        assert_eq!(advance.counter(), Some((1, 3)));
    }

    #[test]
    fn a_system_that_cannot_press_the_clearing_keys_gets_the_value_uncleared_and_says_why() {
        let mut advance = chosen(Risk::Normal);
        let kit = Kit::with_keys(FakeKeys::failing(KeystrokeError::NoClearingKeys));
        let outcome = advance.on_action(HotkeyAction::NextValue, &kit.ports());

        assert_eq!(*kit.direct.handed.borrow(), vec!["alpha"]);
        assert!(kit.by_clipboard.handed.borrow().is_empty());
        assert_eq!(outcome.messages, vec![Message::ClearingSkippedNoKeys]);
        assert!(!outcome.sent.expect("the value went in").cleared);
        assert_eq!(advance.counter(), Some((1, 3)));
    }

    // ---- `OBS-157`, `D95`: nothing arrived, and a send that could not be paced ----

    #[test]
    fn a_cleared_field_whose_value_reached_nothing_is_said_empty_and_the_counter_stays() {
        // Measured: a hook swallowing only characters let the clearing through
        // and none of the value. The field is EMPTY - not as the tester left
        // it, and not holding a fragment - and the same press tries again.
        let mut advance = chosen(Risk::Normal);
        let kit = Kit::with_delivery(FakeDelivery::failing(DeliveryError::NothingArrived {
            reason: StopReason::Dropped,
        }));
        let outcome = advance.on_action(HotkeyAction::NextValue, &kit.ports());
        assert_eq!(
            outcome.messages,
            vec![Message::ClearedThenNothingArrived {
                reason: StopReason::Dropped
            }]
        );
        assert!(
            outcome.sent.is_none(),
            "nothing of the value is in the field"
        );
        assert_eq!(advance.counter(), Some((0, 3)), "the counter stays");
        assert_eq!(advance.sequence().delivery, Delivery::Direct);
    }

    #[test]
    fn an_uncleared_field_whose_value_reached_nothing_is_as_it_was() {
        let mut advance = chosen(Risk::Normal);
        let kit = Kit {
            direct: FakeDelivery::failing(DeliveryError::NothingArrived {
                reason: StopReason::FocusMoved,
            }),
            keys: FakeKeys::failing(KeystrokeError::FieldUnconfirmed),
            ..Kit::ready()
        };
        let outcome = advance.on_action(HotkeyAction::NextValue, &kit.ports());
        // Not "Not cleared first ... the value went in on top": nothing went in.
        assert_eq!(
            outcome.messages,
            vec![Message::NothingArrived {
                reason: StopReason::FocusMoved
            }]
        );
        assert_eq!(advance.counter(), Some((0, 3)));
    }

    #[test]
    fn clearing_keys_that_reached_nothing_leave_the_field_as_it_was_and_send_nothing() {
        // Measured under another program's `BlockInput`: the field kept its own
        // content, and "it may hold part of its old content" was the wrong worry.
        let mut advance = chosen(Risk::Normal);
        let kit = Kit::with_keys(FakeKeys::failing(KeystrokeError::NothingArrived {
            reason: StopReason::Dropped,
        }));
        let outcome = advance.on_action(HotkeyAction::NextValue, &kit.ports());
        assert!(kit.direct.handed.borrow().is_empty(), "no value on top");
        assert!(kit.by_clipboard.handed.borrow().is_empty());
        assert_eq!(
            outcome.messages,
            vec![Message::NothingArrived {
                reason: StopReason::Dropped
            }]
        );
        assert_eq!(advance.counter(), Some((0, 3)));
    }

    #[test]
    fn a_value_sent_without_following_the_application_says_characters_may_be_missing() {
        // `D95` point 2: the route could not join the focused queue, so it
        // typed as before `D95` - which is when characters vanish (`OBS-158`).
        // The value counts as sent, and the tester is told to check it.
        let mut advance = chosen(Risk::Normal);
        let kit = Kit::with_delivery(FakeDelivery::unpaced());
        let outcome = advance.on_action(HotkeyAction::NextValue, &kit.ports());
        assert_eq!(outcome.messages, vec![Message::NotPaced]);
        assert!(outcome.sent.is_some());
        assert_eq!(advance.counter(), Some((1, 3)));

        let mut paced = chosen(Risk::Normal);
        let quiet = paced.on_action(HotkeyAction::NextValue, &Kit::ready().ports());
        assert!(quiet.messages.is_empty(), "a paced send says nothing extra");
    }

    #[test]
    fn a_value_handed_to_the_input_method_is_sent_and_never_said_to_be_in_the_field() {
        // `D145`: on Wayland the text is committed from an input method engine, and
        // not even whether a field had the focus can be followed afterwards. The value
        // counts as sent, and the tester is told that it was handed over.
        let mut advance = chosen(Risk::Normal);
        let kit = Kit::with_delivery(FakeDelivery::handing_over());
        let outcome = advance.on_action(HotkeyAction::NextValue, &kit.ports());
        assert_eq!(outcome.messages, vec![Message::HandedOver]);
        assert!(outcome.sent.is_some());
        assert_eq!(advance.counter(), Some((1, 3)));
    }

    #[test]
    fn a_value_the_system_may_have_changed_says_which_services_could_have_acted() {
        // `D139`: the value went in and counts as sent, and the tester is told
        // that a text service of the system could have changed it - which ones.
        let mut advance = chosen(Risk::Normal);
        let services = Substitutions {
            capitalization: true,
            period: true,
            ..Substitutions::NONE
        };
        let kit = Kit::with_delivery(FakeDelivery::altering(services));
        let outcome = advance.on_action(HotkeyAction::NextValue, &kit.ports());
        assert_eq!(outcome.messages, vec![Message::MayBeAltered(services)]);
        assert!(outcome.sent.is_some());
        assert_eq!(advance.counter(), Some((1, 3)));

        // Nothing on, or nothing that could act: nothing said.
        let mut quiet = chosen(Risk::Normal);
        let none = quiet.on_action(
            HotkeyAction::NextValue,
            &Kit::with_delivery(FakeDelivery::altering(Substitutions::NONE)).ports(),
        );
        assert!(none.messages.is_empty());
    }

    #[test]
    fn a_refusal_from_the_send_itself_says_the_same_and_the_value_waits() {
        // Without clearing - or when the focus moved between the clearing and
        // the value - the send refuses on its own.
        let mut advance = chosen(Risk::Normal);
        let kit = Kit::with_delivery(FakeDelivery::failing(DeliveryError::NoTextField));
        let first = advance.on_action(HotkeyAction::NextValue, &kit.ports());
        assert_eq!(first.messages, vec![Message::NoTextField]);
        assert!(kit.by_clipboard.handed.borrow().is_empty());

        // The tester clicks into a field: the next press sends the SAME value.
        let ordinary = Kit::ready();
        let second = advance.on_action(HotkeyAction::NextValue, &ordinary.ports());
        assert_eq!(*ordinary.direct.handed.borrow(), vec!["alpha"]);
        assert!(second.messages.is_empty());
        assert_eq!(advance.counter(), Some((1, 3)));
    }

    fn in_a_terminal() -> Target {
        Target {
            program: Some(String::from("WindowsTerminal.exe")),
            field: ControlKind::Terminal,
        }
    }

    #[test]
    fn a_typed_value_says_where_it_went_as_read_once_at_the_press() {
        let mut advance = chosen(Risk::Normal);
        let kit = Kit::with_inspector(in_a_terminal());
        kit.direct.set_target(Some(TargetRef(42)));

        let outcome = advance.on_action(HotkeyAction::NextValue, &kit.ports());
        let _ = advance.on_action(HotkeyAction::CopyReport, &kit.ports());

        assert_eq!(
            outcome.sent.and_then(|sent| sent.target),
            Some(in_a_terminal())
        );
        assert_eq!(
            *kit.inspector.asked.borrow(),
            vec![TargetRef(42)],
            "asked once, about the window of the press"
        );
        let blocks = kit.text.blocks.borrow();
        assert_eq!(
            blocks.first().and_then(|block| block.target.clone()),
            Some(in_a_terminal()),
            "the block says what the palette said"
        );
    }

    #[test]
    fn one_press_reads_its_window_once_and_aims_the_clearing_and_the_value_at_it() {
        // `D148`: read again for each step, a window that came to the front
        // between them would get keys meant for the field confirmed in the first.
        let mut advance = chosen(Risk::Normal);
        let kit = Kit::ready();
        kit.direct.set_target(Some(TargetRef(42)));

        let _ = advance.on_action(HotkeyAction::NextValue, &kit.ports());

        assert_eq!(kit.direct.targets_asked.get(), 1, "read once, at the press");
        assert_eq!(*kit.inspector.asked.borrow(), vec![TargetRef(42)]);
        assert_eq!(
            *kit.keys.aimed.borrow(),
            vec![TargetRef(42)],
            "the clearing"
        );
        assert_eq!(*kit.direct.aimed.borrow(), vec![TargetRef(42)], "the value");
    }

    #[test]
    fn the_palettes_flag_to_stop_reaches_the_clearing_and_the_value() {
        // R03: the route reads the flag between keys. A port that was handed
        // another flag, or none, would type on after the palette closed.
        let kit = Kit::ready();
        kit.halt.store(true, std::sync::atomic::Ordering::Relaxed);
        let _ = chosen(Risk::Normal).on_action(HotkeyAction::NextValue, &kit.ports());
        // The clearing, then the value.
        let handed = (kit.keys.halted.take(), kit.direct.halted.take());
        assert_eq!(handed, (vec![true], vec![true]));
    }

    #[test]
    fn a_value_put_on_the_clipboard_names_no_target_and_asks_nothing() {
        let mut advance = chosen(Risk::Normal);
        let kit = Kit::with_inspector(in_a_terminal());
        let _ = advance.choose_route(RouteRequest::Clipboard, &kit.ports());

        let outcome = advance.on_action(HotkeyAction::NextValue, &kit.ports());
        let _ = advance.on_action(HotkeyAction::CopyReport, &kit.ports());

        assert_eq!(outcome.sent.and_then(|sent| sent.target), None);
        assert!(kit.inspector.asked.borrow().is_empty());
        let blocks = kit.text.blocks.borrow();
        assert_eq!(blocks.first().and_then(|block| block.target.clone()), None);
    }

    #[test]
    fn a_value_sent_to_the_clipboard_for_a_window_with_higher_privileges_names_no_target() {
        // `D72`: the window was read at the press, and then received nothing -
        // the value went to the clipboard for the tester to paste.
        let mut advance = chosen(Risk::Normal);
        let kit = a_higher_window();

        let outcome = advance.on_action(HotkeyAction::NextValue, &kit.ports());
        let _ = advance.on_action(HotkeyAction::CopyReport, &kit.ports());

        let sent = outcome.sent.expect("the value went to the clipboard");
        assert_eq!(sent.arrival, Arrival::OnClipboard);
        assert_eq!(sent.target, None);
        let blocks = kit.text.blocks.borrow();
        assert_eq!(blocks.first().and_then(|block| block.target.clone()), None);
    }

    /// A value of type `next-date`: the last Sunday of October at 02:30, the hour
    /// the autumn clock change repeats in central Europe. The Copy buttons'
    /// tests use it too (`copy.rs`).
    pub(super) fn a_clock_change(id: &str) -> PackValue {
        use nkb_core::calendar::Weekday;
        use nkb_core::next_date::{NextDate, TimeOfDay, Week};
        let rule = NextDate::new(10, Week::Last, Weekday::Sunday, TimeOfDay::parse("02:30"))
            .expect("a month of the year");
        PackValue {
            definition: nkb_core::value::Definition::NextDate(rule),
            ..a_value(id, "", None)
        }
    }

    /// A calendar day, for a test that names one.
    pub(super) fn on_day(year: i32, month: u32, day: u32) -> Day {
        Day::from_civil(year, month, day).expect("a real day")
    }

    #[test]
    fn a_next_date_value_is_the_date_ahead_on_the_day_it_is_typed_and_its_block_keeps_that_day() {
        // `D133`. The palette stays open for days, so the date is worked out at
        // every view and every press - and the block quotes the date that was
        // TYPED, even when it is asked for once the rule has moved on.
        let clock = MovingClock::on(on_day(2026, 10, 15));
        let mut advance = AdvanceSequence::new(Box::new(clock.clone()));
        let pack = a_pack(vec![a_clock_change("dst-ambiguous")], Risk::Normal);
        advance
            .choose_pack(&FakeSource, &FakeFormat::of(pack), "sample")
            .expect("a scripted pack with no errors loads");
        let kit = Kit::ready();
        let typed = |text: &str| {
            nkb_core::value::ValueBody::Literal(nkb_core::text::LiteralText::new(text))
        };
        let shown = |advance: &AdvanceSequence| match advance.upcoming() {
            Some(UpcomingValue::Value { preview, .. }) => preview,
            other => panic!("expected a value to come, got {other:?}"),
        };

        assert_eq!(shown(&advance), preview_of(&typed("2026-10-25 02:30")));
        let _ = advance.on_action(HotkeyAction::NextValue, &kit.ports());
        assert_eq!(*kit.direct.handed.borrow(), vec!["2026-10-25 02:30"]);

        // From 24 October in UTC, the change of 2026 is today somewhere, and
        // the date ahead is next year's.
        clock.set(on_day(2026, 10, 24));
        let _ = advance.on_action(HotkeyAction::CopyReport, &kit.ports());
        // The last block is the copied one: the view after the send asked for
        // one too, before the clock moved.
        assert_eq!(
            kit.text.blocks.borrow().last().map(|block| &block.typed),
            Some(&nkb_core::report::typed_of(&typed("2026-10-25 02:30"))),
            "the block quotes what went out, not what would go out now"
        );
        assert!(advance.start_over());
        assert_eq!(shown(&advance), preview_of(&typed("2027-10-31 02:30")));
        let _ = advance.on_action(HotkeyAction::NextValue, &kit.ports());
        assert_eq!(
            *kit.direct.handed.borrow(),
            vec!["2026-10-25 02:30", "2027-10-31 02:30"]
        );
    }
}
