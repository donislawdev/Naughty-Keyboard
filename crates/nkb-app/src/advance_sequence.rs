//! The palette's core loop: a shortcut moves the tester through a pack.
//!
//! This is the use case behind "next value", and it holds the one piece of
//! mutable state the whole product has - the [`Sequence`] - which
//! `architektura.md` 6a puts in `app`, behind one gate, and nowhere else. The
//! sequence machine in the core decides WHAT a keystroke means; this decides
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
//! | no route on this system, from the send OR from the clearing before it | `InsertionFailed` | direct delivery cannot work; the same value goes to the clipboard now |
//! | part of it landed | `Cancelled` | a fragment was left in the field |
//! | nothing was sent (no target, held modifier, clear failed, clipboard busy or refusing, a character the clipboard cannot carry) | `InsertionRefused` | try again, and do NOT switch routes for a passing hiccup |
//!
//! The last row is why `InsertionRefused` exists: folding it into
//! `InsertionFailed` would turn a held Ctrl+Alt into a permanent clipboard mode.
//! The machine returns the STATE for each; the MESSAGE that says which and with
//! what numbers is built here, because the numbers are facts about delivery.
//!
//! # Two routes, one choice (`D71`)
//!
//! Values reach the field straight, or through the clipboard for the tester to
//! paste. Both are implementations of `ValueDelivery`, and [`AdvanceSequence`]
//! picks one by the sequence's delivery axis in exactly one place; everything
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
//! - only `Next`, `Previous`, `Restart` and the report copy are wired. Repeat,
//!   the result marks, the pack search and show/hide belong to later steps; an
//!   action that is not wired says so rather than doing nothing in silence;
//! - the send is atomic - one blocking [`deliver_value`] - so `Inserting` is
//!   passed straight through. The progress bar, `Escape` mid-send and the
//!   char-by-char mode (`W2`/`W3`) are the char-by-char delivery, still to come;
//! - it does not watch the target for a change mid-send (`TargetChanged`),
//!   because that needs `TargetInspector`, which does not exist yet. It checks
//!   the target ONCE before each send instead, which is what stops a value from
//!   going into the window the palette was launched from.

use nkb_core::hotkeys::HotkeyAction;
use nkb_core::pack::{Pack, Risk};
use nkb_core::preview::{ShapeFact, ValuePreview};
use nkb_core::report::{Arrival, ReportBlock};
use nkb_core::sequence::{Delivery, Effect, Event, Sequence};

use crate::load_pack;
use crate::ports::{
    Availability, Clipboard, ClipboardError, DeliveryError, History, KeystrokeError,
    KeystrokeSender, PackFormat, PackSource, ReportText, SourceError, TargetRef, ValueDelivery,
};
use crate::send_value::{Clearing, SendOutcome, deliver_value};

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
    pub clipboard: &'a dyn Clipboard,
    pub report_text: &'a dyn ReportText,
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
#[derive(Debug, Default)]
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
}

/// Which value a report block would describe, and how much of it arrived.
///
/// An index into the held pack rather than a copy of the block: the pack is
/// loaded once and never re-read under the sequence (`W5`), so the block built
/// from it on `Ctrl+Alt+B` is the value that went out - and the send itself pays
/// nothing for a report that is usually never asked for.
#[derive(Debug, Clone, Copy)]
struct LastSent {
    /// One-based, like the machine's positions.
    index: usize,
    arrival: Arrival,
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
    /// The value that just reached the field, when one did.
    pub sent: Option<Sent>,
    /// What the palette must say, in order. Keys and numbers, never words.
    pub messages: Vec<Message>,
    /// Whether this action went as far as the field - the machine asked for a
    /// send and the send was attempted, landed or not. A caller draining
    /// presses that queued in the meantime keys on this: presses queued behind
    /// an attempt were pressed while the tool was busy (`W1`); presses queued
    /// behind an instant answer, such as an end-of-pack warning, were not.
    pub attempted_send: bool,
    /// Whether values for the window in front go to the clipboard because it
    /// runs with higher privileges (`D72`). Beside `sequence.delivery`, not in
    /// it: the bar says so for as long as that window stays in front, and the
    /// sequence is not in clipboard mode (`ux-spec.md` 8).
    pub clipboard_for_window: bool,
}

/// What the palette shows about the value that just went out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sent {
    /// `pack-id/value-id`, the same shape `emit` prints.
    pub reference: String,
    pub name: String,
    /// Clusters, as a person sees them - the counter `ux-spec.md` 2 puts first.
    pub graphemes: usize,
    pub code_points: usize,
    pub bytes: usize,
    /// What actually crossed the wire. Differs from `code_points` exactly when
    /// the value holds characters above the basic plane.
    pub utf16_units: usize,
    /// Whether the value is offensive - the effective risk, the value's own if
    /// it declares one and otherwise the pack's. The palette marks it before a
    /// tester can wonder, `product-spec.md` 10.2.
    pub offensive: bool,
    pub warnings: usize,
    /// Whether the field was cleared before the value went in.
    pub cleared: bool,
    /// Whether the value went to the clipboard for the tester to paste, rather
    /// than into the field. Never together with `cleared`: on the clipboard
    /// route nothing presses a key, so nothing clears the field.
    pub on_clipboard: bool,
    /// The value as a person can see it: invisible characters substituted by
    /// `nkb_core::preview::MARKER`, long values elided at both ends, generated
    /// values shown as their recipe.
    ///
    /// 🔴 The palette must say the total when a text preview's `elided_total`
    /// is `Some`. A preview that silently showed a fragment would be the tool
    /// claiming to answer "what did I send" while hiding most of it.
    pub preview: ValuePreview,
    /// What the value is made of. Facts, in a fixed order that puts what changes
    /// how the field READS above what changes where the text sits.
    pub shape: Vec<ShapeFact>,
}

/// A thing the palette must say. A key with its numbers - untouchable rule 9
/// keeps the words one layer further out, in the text module the GUI owns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    /// The pack is finished. The next press starts over.
    EndOfPack { total: usize },
    /// The target moved but the counter did not, quoting both numbers.
    CounterKept { done: usize, total: usize },
    /// No pack has been chosen yet.
    NoPack,
    /// Nothing holds the keyboard focus, so nothing was sent.
    NoTarget,
    /// A modifier is physically held, so nothing was sent - release it and try
    /// again. Names which one.
    ModifierHeld { key: String },
    /// The field could not be cleared, so the value was not sent. The field is
    /// in an unknown state between its old content and empty.
    ClearingFailed,
    /// The value was left half-written in the field: how far it got, of how much.
    Interrupted {
        units_sent: usize,
        units_expected: usize,
    },
    /// This build has no direct route on `system`, so values go to the
    /// clipboard from now on, replacing what the tester had copied. Said when
    /// the palette starts on such a system, or when a send finds out.
    ///
    /// 🔴 Names the cause the tool KNOWS. Until `D71` this was `Degraded`,
    /// whose sentence blamed the application for ignoring simulated input - a
    /// cause the tool cannot detect before the input oracle (`OBS-42`).
    NoDirectRoute { system: String },
    /// The window in front runs with higher privileges than the tool, so its
    /// values go to the clipboard, replacing what the tester had copied. Said
    /// once per window, when its first value goes there (`D72`). A cause the
    /// tool READ - the integrity level of the window's process - never a guess:
    /// a level it could not read sends as before and says nothing.
    HigherPrivileges,
    /// Clipboard mode is on because the tester asked for it: values go to the
    /// clipboard, replacing what was there. Once, at the start.
    ClipboardMode,
    /// Another application held the clipboard, and the value was not placed
    /// on it. Passing, so the tester is told to press again.
    ClipboardBusy,
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
}

impl AdvanceSequence {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
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

    /// The current sequence, for a caller that renders both of its axes.
    #[must_use]
    pub fn sequence(&self) -> Sequence {
        self.sequence
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
    /// Called once at startup, since the pack search that would call it again is
    /// a later step. `check` runs first and the pack is refused on any error -
    /// `pack-format.md` 11 loads all of a pack or none of it - which is what
    /// `load_pack::load` does.
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
            Err(SourceError::Unreadable | SourceError::NotUtf8) => {
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
    /// 🔴 One of the TWO doors to the clipboard - untouchable rule 17 and
    /// `tests/clipboard_has_named_doors.rs`. Nothing is written unless the
    /// tester asked, and nothing is reported as copied that the clipboard did
    /// not take. The block stays in the system's clipboard history on purpose
    /// (`D68`); the values of clipboard mode do not (`D71`).
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
        let last = self.last?;
        let loaded = self.loaded.as_ref()?;
        let value = loaded.pack.values.get(last.index.checked_sub(1)?)?;
        Some(ReportBlock::describe(&loaded.pack, value, last.arrival).map_err(|_| value.id.clone()))
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
            Delivery::Direct => (ports.direct, Clearing::Line),
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
        if route.target().is_none() {
            return refuse(&mut self.sequence, vec![Message::NoTarget]);
        }

        let Some(value) = loaded.pack.values.get(index - 1) else {
            // The machine asked for an index the pack does not hold, which would
            // be a machine/pack disagreement rather than a delivery problem.
            return refuse(&mut self.sequence, Vec::new());
        };
        let offensive = value.risk.unwrap_or(loaded.pack.risk) == Risk::Offensive;
        let id = value.id.clone();
        let window = route.target();
        let mut on_clipboard = on_clipboard;
        let mut outcome = deliver_value(
            &loaded.pack,
            value,
            route,
            ports.keys,
            clearing,
            loaded.warnings,
        );

        // `D72`: the direct route refused a window with higher privileges before
        // pressing a single key. Not a hiccup to retry and not a system without
        // a route: THIS window takes no typing, so the same value goes to the
        // clipboard now, and the tester hears why once per window.
        let mut messages = Vec::new();
        if !on_clipboard && refused_for_privileges(&outcome) {
            if self.window_on_clipboard != window {
                messages.push(Message::HigherPrivileges);
            }
            self.window_on_clipboard = window;
            on_clipboard = true;
            outcome = deliver_value(
                &loaded.pack,
                value,
                ports.by_clipboard,
                ports.keys,
                Clearing::Keep,
                loaded.warnings,
            );
        } else {
            // The direct route took this window, or the sequence is in clipboard
            // mode anyway: no window of ours is waiting on the clipboard.
            self.window_on_clipboard = None;
        }

        if let Some(arrival) = arrival_of(&outcome, on_clipboard) {
            self.last = Some(LastSent { index, arrival });
        }
        let (event, sent, classified) = classify(outcome, offensive, on_clipboard, &id);
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
        }
    }
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
        SendOutcome::NotDelivered {
            error:
                DeliveryError::Partial {
                    units_sent,
                    units_expected,
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
        Effect::AnnounceCounterKept { done, total } => Some(Message::CounterKept {
            done: *done,
            total: *total,
        }),
        Effect::AnnounceNoPack => Some(Message::NoPack),
        Effect::AnnounceNoTarget => Some(Message::NoTarget),
        // These arise only while sending, and the send path builds a richer
        // message from the delivery outcome. Seeing one here would mean a send
        // effect leaked onto the no-send path, so it is dropped rather than
        // shown half-formed. Entering clipboard mode is said by the caller that
        // knows WHY it was entered - the tester's request or a missing route -
        // because the two sentences differ and the machine knows neither.
        Effect::SendValue { .. }
        | Effect::AnnounceStillInserting
        | Effect::AnnounceInterrupted { .. }
        | Effect::AnnounceClipboardMode => None,
    }
}

/// Sorts a delivery outcome onto a sequence event and the message that goes with
/// it. `offensive` rides along so a landed value can be marked, `on_clipboard`
/// so it can say which route it took, and `id` so a refusal can name it.
fn classify(
    outcome: SendOutcome,
    offensive: bool,
    on_clipboard: bool,
    id: &str,
) -> (Event, Option<Sent>, Vec<Message>) {
    match outcome {
        SendOutcome::Sent {
            reference,
            name,
            graphemes,
            code_points,
            bytes,
            utf16_units,
            warnings,
            cleared,
            preview,
            shape,
        } => (
            Event::InsertionFinished,
            Some(Sent {
                reference,
                name,
                graphemes,
                code_points,
                bytes,
                utf16_units,
                offensive,
                warnings,
                cleared,
                on_clipboard,
                preview,
                shape,
            }),
            Vec::new(),
        ),
        // No route at all: the one outcome that moves to the clipboard, and the
        // machine asks for the same value there in the same press.
        SendOutcome::NotDelivered {
            error: DeliveryError::Unsupported { system },
            ..
        } => (
            Event::InsertionFailed,
            None,
            vec![Message::NoDirectRoute { system }],
        ),
        // A fragment was left: interrupted, with how far it got.
        SendOutcome::NotDelivered {
            error:
                DeliveryError::Partial {
                    units_sent,
                    units_expected,
                },
            ..
        } => (
            Event::Cancelled,
            None,
            vec![Message::Interrupted {
                units_sent,
                units_expected,
            }],
        ),
        // Nothing was sent - the field is untouched. Refuse, do NOT degrade.
        SendOutcome::NotDelivered {
            error: DeliveryError::NoTarget,
            ..
        } => (Event::InsertionRefused, None, vec![Message::NoTarget]),
        SendOutcome::NotDelivered {
            error: DeliveryError::ModifierHeld { which },
            ..
        } => (
            Event::InsertionRefused,
            None,
            vec![Message::ModifierHeld { key: which }],
        ),
        // `attempt` sends this value to the clipboard before classifying, so
        // this arm is reached only if a route other than the direct one ever
        // reported it. Nothing was pressed: refuse, and say why.
        SendOutcome::NotDelivered {
            error: DeliveryError::HigherPrivileges,
            ..
        } => (
            Event::InsertionRefused,
            None,
            vec![Message::HigherPrivileges],
        ),
        // The three below are reported by the clipboard route alone - the
        // direct one presses keys and has no clipboard to be busy or to refuse.
        // A direct route that began to report them would need sentences of its
        // own; the words here name the clipboard, because only it gets here.
        // Nothing reached the clipboard in any of them, so the counter stays.
        SendOutcome::NotDelivered {
            error: DeliveryError::Busy,
            ..
        } => (Event::InsertionRefused, None, vec![Message::ClipboardBusy]),
        SendOutcome::NotDelivered {
            error: DeliveryError::Refused { detail },
            ..
        } => (
            Event::InsertionRefused,
            None,
            vec![Message::ClipboardFailed { detail }],
        ),
        SendOutcome::NotDelivered {
            error: DeliveryError::CannotCarry { character },
            ..
        } => (
            Event::InsertionRefused,
            None,
            vec![Message::NotForClipboard {
                id: id.to_owned(),
                character,
            }],
        ),
        // Clearing failed, so the value never went. The reason names both the
        // event and the message.
        SendOutcome::NotCleared { error } => {
            let (event, message) = after_clearing(error);
            (event, None, vec![message])
        }
        // A validated pack cannot describe a value this large, but if one did,
        // nothing was sent and it is said plainly.
        SendOutcome::ValueTooLarge { id, .. } => (
            Event::InsertionRefused,
            None,
            vec![Message::ValueTooLarge { id }],
        ),
        // deliver_value returns only the four families above. The load-path
        // outcomes cannot arise from an already-loaded value; if one somehow
        // did, nothing is known to have been sent, so refuse and stay quiet
        // rather than invent a message.
        SendOutcome::NotFound
        | SendOutcome::Unreadable
        | SendOutcome::Refused { .. }
        | SendOutcome::NoSuchIndex { .. }
        | SendOutcome::RouteUnavailable { .. } => (Event::InsertionRefused, None, Vec::new()),
    }
}

/// What a clearing that did not go through means. Nothing was sent either way;
/// the reason tells the machine where to go and the tester what to do.
fn after_clearing(error: KeystrokeError) -> (Event, Message) {
    match error {
        // 🔴 No route to press keys is no route at all - the SAME fact a send
        // would have reported one step later, and the same answer: clipboard
        // mode, with the value going there now. Until `D71` it was sorted as a
        // failed clear, "the field may hold part of its old content", about a
        // field nothing had touched - and it never reached clipboard mode,
        // because the clear goes first (`OBS-129`).
        KeystrokeError::Unsupported { system } => {
            (Event::InsertionFailed, Message::NoDirectRoute { system })
        }
        KeystrokeError::NoTarget => (Event::InsertionRefused, Message::NoTarget),
        KeystrokeError::ModifierHeld { which } => (
            Event::InsertionRefused,
            Message::ModifierHeld { key: which },
        ),
        // A partial clear leaves the field in an unknown state: it cannot be
        // trusted to be clear, and nothing goes on top of it.
        KeystrokeError::Partial { .. } => (Event::InsertionRefused, Message::ClearingFailed),
        // Rerouted to the clipboard in `attempt` before it gets here; reached
        // only if that ever stops. No key was pressed and the field is intact.
        KeystrokeError::HigherPrivileges => (Event::InsertionRefused, Message::HigherPrivileges),
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
    use nkb_core::sequence::{Delivery, Position};

    #[test]
    fn choosing_a_pack_moves_to_ready_with_the_right_total() {
        let advance = chosen(Risk::Normal);
        assert_eq!(advance.counter(), Some((0, 3)));
        assert_eq!(advance.pack_name(), Some("Sample pack"));
    }

    #[test]
    fn the_first_next_sends_value_one_and_advances_the_counter() {
        let mut advance = chosen(Risk::Normal);
        let outcome = advance.on_action(HotkeyAction::NextValue, &Kit::ready().ports());
        let sent = outcome.sent.expect("value one must have been sent");
        assert_eq!(sent.reference, "sample/one");
        assert_eq!(sent.name, "Value one");
        assert!(sent.cleared, "the palette clears by default");
        assert_eq!(advance.counter(), Some((1, 3)));
        assert!(outcome.messages.is_empty());
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
        assert_eq!(sent.reference, "sample/one");
        assert!(sent.on_clipboard);
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
        assert!(sent.on_clipboard && !sent.cleared);
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
    fn a_busy_clipboard_refuses_the_value_and_keeps_the_counter() {
        let mut advance = chosen(Risk::Normal);
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
            }))
            .ports(),
        );
        assert_eq!(
            outcome.messages,
            vec![Message::Interrupted {
                units_sent: 3,
                units_expected: 5
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

    #[test]
    fn next_with_no_pack_chosen_says_no_pack() {
        let mut advance = AdvanceSequence::new();
        let outcome = advance.on_action(HotkeyAction::NextValue, &Kit::ready().ports());
        assert_eq!(outcome.messages, vec![Message::NoPack]);
        assert!(outcome.sent.is_none());
    }

    #[test]
    fn a_missing_pack_is_reported_not_panicked() {
        let mut advance = AdvanceSequence::new();
        let result = advance.choose_pack(&MissingSource, &FakeFormat::with_errors(0), "absent");
        assert_eq!(result, Err(ChooseError::NotFound));
    }

    #[test]
    fn a_pack_with_errors_is_refused_and_says_how_many() {
        let mut advance = AdvanceSequence::new();
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

    #[test]
    fn previous_at_the_start_keeps_the_counter_and_says_so() {
        let mut advance = chosen(Risk::Normal);
        let outcome = advance.on_action(HotkeyAction::PreviousValue, &Kit::ready().ports());
        assert_eq!(
            outcome.messages,
            vec![Message::CounterKept { done: 0, total: 3 }]
        );
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
        let kit = Kit::ready();
        let _ = advance.on_action(HotkeyAction::NextValue, &kit.ports());
        let _ = advance.on_action(HotkeyAction::NextValue, &kit.ports());

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

        assert!(kit.clipboard.puts.borrow().is_empty());
        assert!(kit.text.blocks.borrow().is_empty());
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
        assert!(sent.on_clipboard && !sent.cleared);
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
        assert!(second.sent.expect("value two went out").on_clipboard);
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
        assert!(!outcome.sent.expect("typed").on_clipboard);
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
        assert!(outcome.sent.expect("on the clipboard").on_clipboard);
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
}
