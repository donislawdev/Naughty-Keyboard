//! The worker thread: shortcuts on one side, the palette's properties on the
//! other.
//!
//! # Why a thread at all
//!
//! `architektura.md` 6.5 is binding on two points and they pull in the same
//! direction. The main thread belongs to Slint, and a send takes seconds - a
//! hundred characters typed into somebody else's field one keystroke at a time.
//! Doing that on the main thread would freeze the palette for the whole
//! insertion, which is the one moment a tester is looking at it. And the
//! shortcut thread is never the main thread, because the message queue belongs
//! to whichever thread registered.
//!
//! So: this thread registers, waits, sends, and hands finished view data back.
//! Nothing here draws and nothing here holds a Slint component - only a
//! [`slint::Weak`], which is the one handle that may cross a thread boundary.
//!
//! # What the loop measurement changed here
//!
//! `tools/sonda-petla`, measured 2026-09-22 on both 1.17.1 and 1.18.1
//! (`slint.md` 1.9):
//!
//! - work handed to the event loop BEFORE `run()` is delivered once the loop
//!   starts. So this thread may start before the window is running and does not
//!   need a gate, a handshake or a first-view buffer. The first design had all
//!   three.
//! - 🔴 work handed to the loop AFTER it has quit returns `Ok(())` and the
//!   closure NEVER RUNS. That is the same shape as `eprintln!` with no handle
//!   (`slint.md` 2.17): a success returned for work nobody did. Nothing here may
//!   treat that `Ok` as proof that a tester saw anything.
//!
//! # What this module deliberately does not do
//!
//! It does not make the palette refuse the keyboard focus. That runs on the main
//! thread, because only the main thread may touch the window, and it lives in
//! `focus`. What crosses between them is [`crate::focus::Standing`]: a sentence
//! saying the promise could not be kept, which this module reads again on every
//! view because it rebuilds the message band from scratch each time.

use std::cell::{Cell, RefCell};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use nkb_adapters::i18n::PaletteLabel;
use nkb_adapters::{
    BuiltInCatalogue, ClipboardDelivery, DirectInjection, EnglishReport, GlobalShortcuts,
    SettingsFile, TomlPackFormat, default_bindings, i18n,
};
use nkb_app::advance_sequence::{Ports, RouteRequest, Sent};
use nkb_app::ports::{
    HotkeyRegistrar, LiveShortcuts, SettingChange, Settings, SettingsStore, ShortcutRegistration,
    Wait,
};
use nkb_app::{AdvanceSequence, KeptSettings, Opening, Outcome, SettingsMessage, drive_sequence};
use nkb_core::hotkeys::HotkeyAction;
use nkb_core::preview::ValuePreview;
use nkb_core::report::Arrival;
use nkb_core::sequence::Delivery;
use nkb_core::typeface::outside_guarantee;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel, Weak};

use crate::clipboard::SystemClipboard;
use crate::focus::{Standing, standing_line};
use crate::typeface::SHIPPED;
use crate::{Marker, Palette};

/// How long one wait for a press lasts before the stop flag is read again.
///
/// It bounds how long the window can be closed before this thread notices, and
/// nothing else: a press arriving mid-tick is delivered immediately, because the
/// wait returns on the press rather than on the timeout.
const TICK: Duration = Duration::from_millis(100);

/// One turn of the loop, as plain data that can cross a thread boundary.
///
/// Every field is a finished string. Nothing in this struct is a Slint type, and
/// that is not an accident: the view is built here, on the worker, and applied
/// there, on the main thread, so the conversion happens in exactly one place.
pub struct View {
    pack: String,
    counter: String,
    /// `None` means this turn produced no value - a warning, a refusal, a press
    /// that arrived while busy. The palette then KEEPS THE PREVIOUS VALUE on
    /// screen: blanking it would take away the thing the message is about.
    value: Option<ValueView>,
    messages: Vec<String>,
    /// The standing clipboard bar and its words, or `None` when values are
    /// typed. Two conditions share the one bar: the mode the sequence is in,
    /// and the window in front taking no typing (`D72`). The words differ, so
    /// the view carries them rather than a flag.
    clipboard_bar: Option<&'static str>,
}

struct ValueView {
    name: String,
    reference: String,
    counts: String,
    /// The value with invisible characters substituted, or the recipe of a
    /// generated one - ready to draw either way.
    preview: String,
    /// Empty unless the preview is a fragment, in which case it says how much.
    elided: String,
    /// Empty unless the preview draws something the shipped typeface does not
    /// guarantee, in which case it names those characters (`D52`, `D66`).
    not_guaranteed: String,
    /// Every fact about the value on one line, already joined by `i18n`.
    shape: String,
    /// Text and whether it is a risk. The COLOUR is the palette's business -
    /// document 13 section 2.1 - so it is not decided here.
    markers: Vec<(String, bool)>,
}

/// What the worker starts from.
///
/// The settings were read on the main thread, before the window existed,
/// because whether the palette starts collapsed has to be known before it is
/// drawn. Everything after that - saving included - happens here: a settings
/// folder on a network profile can take seconds to write, and the main thread
/// belongs to Slint.
pub struct Start {
    /// The pack named on the command line, if any.
    pub asked: Option<String>,
    /// The pack opened when nothing is asked for or remembered.
    pub default_pack: &'static str,
    /// How the palette was started - `nkb-gui --clipboard` asks for clipboard
    /// mode (`D71`).
    pub route: RouteRequest,
    pub store: SettingsFile,
    pub kept: KeptSettings,
    /// What reading the settings had to say.
    pub said: Vec<SettingsMessage>,
}

/// Whether the palette starts collapsed. One function for both threads, so
/// the window and the worker cannot start from two different answers.
#[must_use]
pub fn starts_compact(settings: &Settings) -> bool {
    settings.compact.unwrap_or(false)
}

/// Runs until the flag is set or the shortcuts die.
///
/// Takes the pack by name rather than a loaded sequence, because the sequence
/// must live on this thread: it is the one gate over the sequence state
/// (`architektura.md` 6a) and it never crosses back.
pub fn drive(palette: &Weak<Palette>, stop: &Arc<AtomicBool>, start: Start, standing: &Standing) {
    let Start {
        asked,
        default_pack,
        route,
        store,
        kept,
        said,
    } = start;
    // Where the settings live, for the sentences that send the tester there.
    let file = store
        .path()
        .map(|path| path.display().to_string())
        .unwrap_or_default();
    let mut sequence = AdvanceSequence::new();
    let mut opening: Vec<String> = said
        .iter()
        .filter_map(|message| i18n::settings_message(message, &file))
        .collect();
    // Created here, on the thread that uses it, and dropped with it - which is
    // when a Linux clipboard stops serving what it holds (`clipboard` says why).
    // Nothing connects until the first write, so a palette that never copies
    // never touches the clipboard at all.
    let clipboard = SystemClipboard::new();
    let by_clipboard = ClipboardDelivery::new(&clipboard);
    let ports = Ports {
        direct: &DirectInjection,
        by_clipboard: &by_clipboard,
        keys: &DirectInjection,
        clipboard: &clipboard,
        report_text: &EnglishReport,
    };

    let mut kept = kept;
    let opened = kept.open_pack(&store, asked.as_deref(), default_pack, &mut |pack| {
        sequence.choose_pack(&BuiltInCatalogue::new(), &TomlPackFormat, pack)
    });
    for note in &opened.notes {
        match note {
            Opening::CouldNotChoose { pack, error } => {
                opening.push(i18n::choose_error(error, pack));
            }
            Opening::Settings(message) => opening.extend(i18n::settings_message(message, &file)),
        }
    }
    // The pack the messages are about - the one opened, or the last one tried.
    let pack = opened.pack.as_str();
    // Captured now, because `drive_sequence` borrows the sequence for the whole
    // loop and the closure that builds each view cannot reach it.
    let pack_shown = sequence.pack_name().unwrap_or(pack).to_owned();

    let live = match GlobalShortcuts.register(default_bindings()) {
        Ok(live) => live,
        Err(error) => {
            // Nothing can drive the sequence, so the palette says why and stays
            // up. Untouchable rule 1: a run that did less than it promised says
            // so, rather than looking alive.
            opening.push(i18n::shortcuts_unavailable(&error));
            show(
                palette,
                opening_view(&pack_shown, &sequence, opening, standing),
            );
            return;
        }
    };
    // A registration that worked says nothing - `i18n::registration` answers
    // `None` for it, so the silence is the dictionary's decision and not this
    // module's.
    for (action, outcome) in live.outcomes() {
        if let Some(line) = i18n::registration(outcome, *action) {
            opening.push(line);
        }
    }
    // Only now, with shortcuts that can drive the palette: a sentence telling
    // the tester to paste each value would promise a flow that a palette with
    // no shortcuts does not have (`D71`). A system with no direct route starts
    // in clipboard mode here, rather than failing the first press.
    opening.extend(
        sequence
            .choose_route(route, &ports)
            .iter()
            .map(|message| i18n::message(message, pack)),
    );
    show(
        palette,
        opening_view(&pack_shown, &sequence, opening, standing),
    );

    let mut keep_going = || !stop.load(Ordering::Relaxed);
    let mut present =
        |outcome: Outcome| show(palette, view_of(&outcome, &pack_shown, pack, standing));
    let collapse = Collapse::new(kept, &store, file);
    let on_toggle = || {
        let (compact, line) = collapse.toggle();
        set_compact_later(palette, compact);
        if let Some(line) = line {
            say_later(palette, line);
        }
    };
    let shortcuts = PaletteShortcuts {
        inner: live.as_ref(),
        on_toggle: &on_toggle,
    };
    let ended = drive_sequence(
        &shortcuts,
        &mut sequence,
        &ports,
        TICK,
        &mut keep_going,
        &mut present,
    );
    // Before anything else: the shortcuts go back to the system. Holding them
    // after the palette is gone would take `Ctrl+Alt+N` away from whoever wants
    // it next.
    drop(live);

    if let Some(line) = i18n::ended(ended) {
        // Reached only while the window is still up - `Ended::Stopped` has no
        // sentence, and the stop flag is set by the window closing. If it is
        // ever reached after the loop has quit, the measurement above says this
        // vanishes silently, and there is nobody left to tell.
        show(
            palette,
            opening_view(&pack_shown, &sequence, vec![line], standing),
        );
    }
}

fn counter_of(sequence: &AdvanceSequence) -> String {
    sequence
        .counter()
        .map_or_else(String::new, |(done, total)| i18n::counter(done, total))
}

/// Puts the standing sentence, if there is one, in front of this view's own.
///
/// 🔴 In FRONT rather than behind: what it says is that the tool could not keep
/// a promise, which outranks anything about the value that just went out. The
/// worker rebuilds this band from scratch on every view, so a sentence produced
/// on the main thread has to be re-read here or it lasts exactly one view.
fn with_standing(mut messages: Vec<String>, standing: &Standing) -> Vec<String> {
    if let Some(line) = standing_line(standing) {
        messages.insert(0, line);
    }
    messages
}

fn opening_view(
    pack_shown: &str,
    sequence: &AdvanceSequence,
    messages: Vec<String>,
    standing: &Standing,
) -> View {
    View {
        pack: pack_shown.to_owned(),
        counter: counter_of(sequence),
        value: None,
        messages: with_standing(messages, standing),
        clipboard_bar: clipboard_bar(
            sequence.sequence().delivery,
            sequence.clipboard_for_window(),
        ),
    }
}

/// The words of the standing clipboard bar, if it stands.
///
/// The mode wins over the window: in clipboard mode every value goes there
/// anyway, and "for this window" would suggest the others are typed.
fn clipboard_bar(delivery: Delivery, for_window: bool) -> Option<&'static str> {
    if delivery == Delivery::ClipboardMode {
        Some(i18n::label(PaletteLabel::ClipboardMode))
    } else if for_window {
        Some(i18n::label(PaletteLabel::ClipboardForWindow))
    } else {
        None
    }
}

fn view_of(outcome: &Outcome, pack_shown: &str, pack: &str, standing: &Standing) -> View {
    View {
        pack: pack_shown.to_owned(),
        counter: outcome
            .sequence
            .counter()
            .map_or_else(String::new, |(done, total)| i18n::counter(done, total)),
        value: outcome.sent.as_ref().map(value_view),
        messages: with_standing(
            outcome
                .messages
                .iter()
                .map(|message| i18n::message(message, pack))
                .collect(),
            standing,
        ),
        clipboard_bar: clipboard_bar(outcome.sequence.delivery, outcome.clipboard_for_window),
    }
}

/// The value band, every line finished.
fn value_view(sent: &Sent) -> ValueView {
    let (preview, elided) = match &sent.facts.preview {
        ValuePreview::Text(text) => (
            text.shown.clone(),
            // An empty string rather than an Option, because the view's switch
            // is set from `is_empty()` and a second representation of "nothing"
            // would be one more thing that can disagree with the first.
            text.elided_total.map_or_else(String::new, |total| {
                i18n::preview_elided(text.shown.chars().count(), total)
            }),
        ),
        // A recipe is the whole value, exactly, so nothing is elided and there
        // is nothing to confess.
        ValuePreview::Recipe(recipe) => (i18n::recipe(recipe.count, &recipe.unit), String::new()),
    };
    // Measured on the line the preview DRAWS, not on the whole value: a
    // character in the elided middle never reaches the screen, and for a
    // recipe the line is the unit plus digits and a sign the typeface carries.
    let not_guaranteed =
        i18n::not_guaranteed(&outside_guarantee(&preview, &SHIPPED)).unwrap_or_default();
    ValueView {
        name: sent.facts.name.clone(),
        reference: sent.facts.reference.clone(),
        counts: i18n::counts(
            sent.facts.graphemes,
            sent.facts.code_points,
            sent.facts.bytes,
            sent.utf16_units,
        ),
        preview,
        elided,
        not_guaranteed,
        shape: i18n::shape_line(&sent.facts.shape),
        markers: markers_of(sent),
    }
}

/// What is worth knowing about the value beyond its name and its size.
///
/// Order is fixed rather than sorted: the risk comes first because it is the one
/// a tester must not miss, `product-spec.md` 10.2.
fn markers_of(sent: &Sent) -> Vec<(String, bool)> {
    let mut markers = Vec::new();
    if sent.offensive {
        markers.push((i18n::label(PaletteLabel::Offensive).to_owned(), true));
    }
    // A risk, right after the other one: the preview and the counts above are
    // the WHOLE value, and the field holds only part of it (`OBS-126`).
    if matches!(sent.arrival, Arrival::Interrupted { .. }) {
        markers.push((i18n::label(PaletteLabel::Interrupted).to_owned(), true));
    }
    if sent.facts.warnings > 0 {
        markers.push((i18n::warnings(sent.facts.warnings), true));
    }
    if sent.cleared {
        markers.push((i18n::label(PaletteLabel::Cleared).to_owned(), false));
    }
    // Never beside `cleared first`: the clipboard route presses nothing, so it
    // clears nothing. It answers the same question from the other side - what
    // is in the field is what the tester pasted.
    if sent.arrival == Arrival::OnClipboard {
        markers.push((i18n::label(PaletteLabel::OnClipboard).to_owned(), false));
    }
    markers
}

/// Hands the view to the thread that owns the window.
///
/// 🔴 The result is dropped ON PURPOSE and the reason is measured: after the
/// event loop has quit this returns `Ok(())` and the closure never runs
/// (`slint.md` 1.9). Checking it would prove nothing, and reacting to it would
/// react to the wrong thing - there is no failure here to report, only a window
/// that is already gone.
fn show(palette: &Weak<Palette>, view: View) {
    let _ = palette.upgrade_in_event_loop(move |palette| apply(&palette, view));
}

/// Runs on the MAIN thread. Everything Slint touches happens here.
fn apply(palette: &Palette, view: View) {
    palette.set_pack(view.pack.into());
    palette.set_counter(view.counter.into());
    palette.set_clipboard_mode(view.clipboard_bar.is_some());
    if let Some(words) = view.clipboard_bar {
        palette.set_clipboard_mode_label(words.into());
    }
    palette.set_messages(ModelRc::new(VecModel::from(
        view.messages
            .into_iter()
            .map(SharedString::from)
            .collect::<Vec<_>>(),
    )));

    if let Some(value) = view.value {
        palette.set_value_name(value.name.into());
        palette.set_value_reference(value.reference.into());
        palette.set_value_counts(value.counts.into());
        palette.set_value_preview(value.preview.into());
        // The switches are read BEFORE the strings move into the properties.
        palette.set_has_elided(!value.elided.is_empty());
        palette.set_has_not_guaranteed(!value.not_guaranteed.is_empty());
        palette.set_has_shape(!value.shape.is_empty());
        palette.set_value_elided(value.elided.into());
        palette.set_value_not_guaranteed(value.not_guaranteed.into());
        palette.set_value_shape(value.shape.into());
        palette.set_markers(ModelRc::new(VecModel::from(
            value
                .markers
                .into_iter()
                .map(|(text, risky)| Marker {
                    text: text.into(),
                    risky,
                })
                .collect::<Vec<_>>(),
        )));
        palette.set_has_value(true);
    }

    // 🔴 No timer and no state change here, and that is `D83`: until then every
    // view woke the palette and a timer put it back to rest four seconds later,
    // taking the value with it. Whether the palette is compact is the tester's
    // choice alone - see `Collapse`.
}

/// Keeps the expanded palette from getting shorter by itself (`D83`).
///
/// Call once, on the main thread, after the window is built. From then on every
/// change in what the content asks for goes through [`hold_height`].
pub fn hold_height_on_change(palette: &Palette) {
    let weak = palette.as_weak();
    palette.on_content_height_changed(move |content| {
        if let Some(palette) = weak.upgrade() {
            hold_height(&palette, content);
        }
    });
}

/// Moves the height floor up to what the content asks for, never down.
///
/// The expanded palette then keeps the height of its tallest content since it
/// was last expanded, so a short value after a long one leaves room below
/// rather than pulling the bottom edge up. Only `set_compact` lowers it.
///
/// Takes the height as an argument rather than reading it, because a read
/// straight after the properties change is stale (`slint.md` 2.27): the palette
/// calls this from its `changed` handler, after the new bands exist.
pub(crate) fn hold_height(palette: &Palette, content: f32) {
    if palette.get_compact() {
        return;
    }
    if content > palette.get_height_floor() {
        palette.set_height_floor(content);
    }
}

/// Puts the palette into the compact or the expanded state.
///
/// 🔴 The window is never hidden - `OBS-80` measured that `hide()` destroys it
/// on Windows and loses `WS_EX_NOACTIVATE` with it. Compact is the background
/// going translucent and the bands below the counter going away.
///
/// The floor starts again from nothing, so the palette expands to fit what it
/// shows now rather than to the tallest thing it ever showed. The height the
/// expanded content asks for arrives through the `changed` handler.
///
/// Takes the state rather than flipping it: the worker owns the switch from
/// the start (`Collapse`), so the state it saves is the state drawn.
pub fn set_compact(palette: &Palette, compact: bool) {
    palette.set_compact(compact);
    palette.set_height_floor(0.0);
}

/// The compact switch and the memory of it, both owned by the worker.
///
/// # Why the worker and not the window
///
/// Until `D84` the main thread flipped the window's own property. With the
/// state remembered, two owners would mean two answers to "is it collapsed":
/// the one drawn and the one saved. So the worker holds the switch from the
/// start, tells the window what to draw, and saves the same value - on this
/// thread, where a slow settings folder costs a press nothing.
struct Collapse<'a> {
    compact: Cell<bool>,
    kept: RefCell<KeptSettings>,
    store: &'a dyn SettingsStore,
    /// Where the settings live, for a sentence saying they were not saved.
    file: String,
}

impl<'a> Collapse<'a> {
    fn new(kept: KeptSettings, store: &'a dyn SettingsStore, file: String) -> Self {
        Self {
            compact: Cell::new(starts_compact(kept.settings())),
            kept: RefCell::new(kept),
            store,
            file,
        }
    }

    /// Flips the switch and remembers it. The new state, and a line to say
    /// when saving failed for a reason not said before.
    fn toggle(&self) -> (bool, Option<String>) {
        let compact = !self.compact.get();
        self.compact.set(compact);
        let line = self
            .kept
            .borrow_mut()
            .keep(self.store, SettingChange::Compact(compact))
            .and_then(|message| i18n::settings_message(&message, &self.file));
        (compact, line)
    }
}

/// The palette's own shortcuts in front of the sequence.
///
/// `ToggleVisibility` is about the window, not about the pack, so it never
/// reaches `AdvanceSequence` - which would answer it with `Unhandled`. Every
/// other press passes through untouched, in order.
///
/// ⚠️ An intercepted press does not end the wait. The wait goes on for what is
/// left of it, and a zero wait - the drain after a send, `W1` - keeps draining.
/// Returning `Nothing` early would end that drain and leave a `NextValue`
/// pressed during the send in the queue, to be acted on afterwards: exactly the
/// queueing `ux-spec.md` 3 rejects.
struct PaletteShortcuts<'a> {
    inner: &'a dyn LiveShortcuts,
    /// What a `ToggleVisibility` press does. A closure rather than the window
    /// handle, so the interception can be tested without a window.
    on_toggle: &'a dyn Fn(),
}

impl LiveShortcuts for PaletteShortcuts<'_> {
    fn outcomes(&self) -> &[(HotkeyAction, ShortcutRegistration)] {
        self.inner.outcomes()
    }

    fn next(&self, wait: Duration) -> Wait {
        let deadline = Instant::now() + wait;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.inner.next(left) {
                Wait::Pressed(HotkeyAction::ToggleVisibility) => (self.on_toggle)(),
                other => return other,
            }
        }
    }
}

/// Hands the compact state to the thread that owns the window.
fn set_compact_later(palette: &Weak<Palette>, compact: bool) {
    // Dropped for the reason `show` gives.
    let _ = palette.upgrade_in_event_loop(move |palette| set_compact(&palette, compact));
}

/// Adds one line to the message band, below what is already there.
///
/// Below, not instead: a line about the settings must not take away "End of
/// pack" or the reason the last value was refused - nor the standing sentence,
/// which the last view already put in front. The next view rebuilds the band
/// as usual.
fn say_later(palette: &Weak<Palette>, line: String) {
    // Dropped for the reason `show` gives.
    let _ = palette.upgrade_in_event_loop(move |palette| {
        let mut lines: Vec<SharedString> = slint::Model::iter(&palette.get_messages()).collect();
        lines.push(line.into());
        palette.set_messages(ModelRc::new(VecModel::from(lines)));
    });
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use std::rc::Rc;

    use nkb_app::ValueFacts;
    use nkb_app::advance_sequence::{Message, Sent};
    use nkb_core::report::Arrival;
    use nkb_core::sequence::{Position, Sequence};
    use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
    use slint::platform::{Platform, PlatformError, WindowAdapter};

    use super::{
        Delivery, Duration, HotkeyAction, LiveShortcuts, Outcome, Palette, PaletteShortcuts,
        ShortcutRegistration, Standing, ValuePreview, Wait, apply, clipboard_bar, hold_height,
        markers_of, set_compact, view_of, with_standing,
    };

    /// No standing sentence: the ordinary case, and the one the field-by-field
    /// test is about.
    fn quiet() -> Standing {
        crate::focus::standing()
    }

    struct Headless {
        window: Rc<MinimalSoftwareWindow>,
    }

    impl Platform for Headless {
        fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
            Ok(self.window.clone())
        }
    }

    /// One platform per process, so every test that needs a window shares this.
    fn a_palette() -> Palette {
        use std::sync::Once;
        static ONCE: Once = Once::new();
        ONCE.call_once(|| {
            let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
            slint::platform::set_platform(Box::new(Headless { window }))
                .expect("no other platform may be installed in this process");
        });
        Palette::new().expect("the palette must build")
    }

    fn a_sent() -> Sent {
        Sent {
            facts: ValueFacts {
                reference: String::from("unicode-text/zero-width"),
                name: String::from("Three zero-width spaces"),
                // Seven, not four. The zero width space is its own cluster,
                // which is the correction `OBS-105` made to the specification
                // itself.
                graphemes: 7,
                code_points: 7,
                bytes: 13,
                warnings: 2,
                // The example from `ux-spec.md` 2, so the field-by-field test
                // below checks the same value the document draws.
                preview: ValuePreview::Text(nkb_core::preview::preview(
                    "ab\u{200B}\u{200B}\u{200B}cd",
                )),
                shape: nkb_core::preview::shape("ab\u{200B}\u{200B}\u{200B}cd"),
            },
            utf16_units: 7,
            offensive: true,
            cleared: true,
            arrival: Arrival::Whole,
        }
    }

    fn an_outcome(sent: Option<Sent>, messages: Vec<Message>) -> Outcome {
        Outcome {
            sequence: Sequence {
                position: Position::Running { done: 7, total: 34 },
                delivery: Delivery::Direct,
            },
            sent,
            messages,
            attempted_send: true,
            clipboard_for_window: false,
        }
    }

    /// 🔴 The reason this module exists at all, and why it is ONE test.
    ///
    /// One test, because the Slint backend starts once per PROCESS and Rust runs
    /// tests on several threads: a second test asking for a window answers
    /// "EventLoop can't be recreated". Measured here rather than worked around
    /// with a mutex, which would only hide the shape.
    ///
    /// What it is for: `apply` copies six strings onto six properties. Two of
    /// them swapped would put the reference where the size goes and the size
    /// where the reference goes - both are short grey monospaced lines one above
    /// the other, so the render test would draw it happily and nobody would see
    /// it. Every number below is different on purpose, so a swap cannot hide
    /// behind a coincidence.
    #[test]
    fn a_view_reaches_the_window_field_by_field() {
        let palette = a_palette();

        // ---- a value went out --------------------------------------------
        apply(
            &palette,
            view_of(
                &an_outcome(Some(a_sent()), Vec::new()),
                "Unicode & text",
                "u",
                &quiet(),
            ),
        );
        assert_eq!(palette.get_pack(), "Unicode & text");
        assert_eq!(palette.get_counter(), "7 / 34");
        assert_eq!(palette.get_value_name(), "Three zero-width spaces");
        assert_eq!(palette.get_value_reference(), "unicode-text/zero-width");
        assert_eq!(
            palette.get_value_counts(),
            "graphemes: 7, code points: 7, bytes: 13, UTF-16 units: 7"
        );
        // The preview and the shape, checked against the sketch in `ux-spec.md` 2
        // rather than against whatever the code happens to produce.
        assert_eq!(palette.get_value_preview(), "ab\u{2423}\u{2423}\u{2423}cd");
        assert_eq!(palette.get_value_shape(), "zero-width \u{D7} 3");
        assert_eq!(
            palette.get_value_elided(),
            "",
            "a short value is shown whole, so nothing is said about eliding"
        );
        // The marker is inside the guarantee, so a value made of Latin letters
        // and invisible characters has nothing to confess.
        assert_eq!(palette.get_value_not_guaranteed(), "");
        assert!(!palette.get_has_not_guaranteed());
        assert_eq!(
            slint::Model::row_count(&palette.get_markers()),
            3,
            "offensive, warnings and cleared all reach the window"
        );
        assert!(
            palette.get_has_value(),
            "a value went out, so the band shows"
        );
        assert!(
            !palette.get_compact(),
            "an outcome never collapses the palette - only the tester does (D83)"
        );
        assert!(
            !palette.get_clipboard_mode(),
            "direct delivery is not clipboard mode"
        );

        // ---- a turn that produced no value --------------------------------
        // The previous value STAYS. Blanking it would take away the value the
        // message is about: "End of pack" beside the value that ended it reads,
        // beside nothing it does not.
        apply(
            &palette,
            view_of(
                &an_outcome(None, vec![Message::EndOfPack { total: 34 }]),
                "p",
                "p",
                &quiet(),
            ),
        );
        assert!(palette.get_has_value(), "the previous value was blanked");
        assert_eq!(palette.get_value_name(), "Three zero-width spaces");
        assert_eq!(
            slint::Model::row_count(&palette.get_messages()),
            1,
            "the message is shown"
        );

        // ---- the preview draws what the shipped typeface does not ----------
        // Two ideographs and an emoji from the shipped packs, none of which
        // DejaVu Sans Mono carries. Named as code points, in reading order.
        apply(
            &palette,
            view_of(
                &an_outcome(Some(sent_of("\u{540D}\u{524D} \u{1F600}")), Vec::new()),
                "p",
                "p",
                &quiet(),
            ),
        );
        assert_eq!(
            palette.get_value_not_guaranteed(),
            "not guaranteed by the bundled font: U+540D U+524D U+1F600"
        );
        assert!(palette.get_has_not_guaranteed());

        // ---- ... but only what the preview actually DRAWS ------------------
        // The emoji sits in the middle of a value long enough to be elided, so
        // it never reaches the screen and the typeface is never asked for it.
        // A note about it would describe a character nobody can see.
        let mut long = "a".repeat(120);
        long.insert(60, '\u{1F600}');
        apply(
            &palette,
            view_of(
                &an_outcome(Some(sent_of(&long)), Vec::new()),
                "p",
                "p",
                &quiet(),
            ),
        );
        assert_eq!(palette.get_value_not_guaranteed(), "");
        assert!(!palette.get_has_not_guaranteed());

        // ---- a generated value shows its recipe ----------------------------
        // `length-bombs/emoji-truncation`, as the core previews it. The recipe
        // IS the preview, so nothing is elided - and the note is measured on
        // the recipe line, whose unit the shipped typeface does not carry.
        let mut generated = a_sent();
        generated.facts.preview = nkb_core::preview::preview_of(&nkb_core::ValueBody::Repeat {
            unit: nkb_core::LiteralText::new("\u{1F600}"),
            count: 64,
        });
        apply(
            &palette,
            view_of(&an_outcome(Some(generated), Vec::new()), "p", "p", &quiet()),
        );
        assert_eq!(palette.get_value_preview(), "64 \u{D7} \"\u{1F600}\"");
        assert_eq!(palette.get_value_elided(), "");
        assert!(!palette.get_has_elided());
        assert_eq!(
            palette.get_value_not_guaranteed(),
            "not guaranteed by the bundled font: U+1F600"
        );

        // ---- the second axis reaches the standing bar ---------------------
        let mut by_clipboard = an_outcome(Some(a_sent()), Vec::new());
        by_clipboard.sequence.delivery = Delivery::ClipboardMode;
        apply(&palette, view_of(&by_clipboard, "p", "p", &quiet()));
        assert!(palette.get_clipboard_mode());
        assert_eq!(palette.get_clipboard_mode_label(), "clipboard mode");

        // ---- and so does the window in front (`D72`), in its own words ---
        let for_window = Outcome {
            clipboard_for_window: true,
            ..an_outcome(Some(a_sent()), Vec::new())
        };
        apply(&palette, view_of(&for_window, "p", "p", &quiet()));
        assert!(palette.get_clipboard_mode());
        assert_eq!(
            palette.get_clipboard_mode_label(),
            "clipboard mode for this window"
        );
        apply(
            &palette,
            view_of(&an_outcome(None, Vec::new()), "p", "p", &quiet()),
        );
        assert!(
            !palette.get_clipboard_mode(),
            "the window left the front, so the bar goes"
        );

        // ---- the window never gets shorter by itself (`D83`) ---------------
        // The heights are given, as the `changed` handler gives them: a read
        // of the content height here would be stale (`slint.md` 2.27). The
        // render test measures the same rule in pixels.
        palette.set_height_floor(0.0);
        hold_height(&palette, 300.0);
        assert_eq!(
            palette.get_height_floor(),
            300.0,
            "the floor follows the content up"
        );
        hold_height(&palette, 120.0);
        assert_eq!(
            palette.get_height_floor(),
            300.0,
            "a shorter content pulled the floor down"
        );

        // ---- the tester collapses it, and only the tester expands it -------
        set_compact(&palette, true);
        assert!(palette.get_compact());
        assert_eq!(
            palette.get_height_floor(),
            0.0,
            "compact starts the floor over"
        );
        hold_height(&palette, 60.0);
        assert_eq!(
            palette.get_height_floor(),
            0.0,
            "the floor moved while compact"
        );
        apply(
            &palette,
            view_of(
                &an_outcome(Some(sent_of("b")), Vec::new()),
                "p",
                "p",
                &quiet(),
            ),
        );
        assert!(
            palette.get_compact(),
            "a value expanded what the tester collapsed"
        );
        set_compact(&palette, false);
        assert!(!palette.get_compact());
        hold_height(&palette, 120.0);
        assert_eq!(
            palette.get_height_floor(),
            120.0,
            "expanding fits what is shown now, not the tallest thing ever shown"
        );
    }

    /// A queue of presses standing in for the system, for the interception test.
    struct Queued(std::cell::RefCell<std::collections::VecDeque<HotkeyAction>>);

    impl LiveShortcuts for Queued {
        fn outcomes(&self) -> &[(HotkeyAction, ShortcutRegistration)] {
            &[]
        }

        fn next(&self, _wait: Duration) -> Wait {
            self.0
                .borrow_mut()
                .pop_front()
                .map_or(Wait::Nothing, Wait::Pressed)
        }
    }

    /// The palette's own shortcut never reaches the sequence, and taking it out
    /// does not end the drain after a send (`W1`).
    #[test]
    fn toggling_is_answered_by_the_palette_and_does_not_cut_the_drain_short() {
        let queued = Queued(std::cell::RefCell::new(
            [
                HotkeyAction::ToggleVisibility,
                HotkeyAction::NextValue,
                HotkeyAction::ToggleVisibility,
            ]
            .into(),
        ));
        let toggles = std::cell::Cell::new(0);
        let on_toggle = || toggles.set(toggles.get() + 1);
        let shortcuts = PaletteShortcuts {
            inner: &queued,
            on_toggle: &on_toggle,
        };
        // A zero wait, as the drain after a send asks: the toggle in front must
        // not hide the NextValue behind it.
        assert_eq!(
            shortcuts.next(Duration::ZERO),
            Wait::Pressed(HotkeyAction::NextValue)
        );
        assert_eq!(shortcuts.next(Duration::ZERO), Wait::Nothing);
        assert_eq!(
            toggles.get(),
            2,
            "both toggles were answered, none was dropped"
        );
    }

    /// Two conditions share one bar. The mode wins, because in clipboard mode
    /// every value goes there and "for this window" would suggest otherwise.
    #[test]
    fn the_clipboard_bar_names_the_mode_before_the_window() {
        assert_eq!(clipboard_bar(Delivery::Direct, false), None);
        assert_eq!(
            clipboard_bar(Delivery::Direct, true),
            Some("clipboard mode for this window")
        );
        assert_eq!(
            clipboard_bar(Delivery::ClipboardMode, false),
            Some("clipboard mode")
        );
        assert_eq!(
            clipboard_bar(Delivery::ClipboardMode, true),
            Some("clipboard mode")
        );
    }

    /// A value carrying `text`, with the preview and shape the product builds.
    fn sent_of(text: &str) -> Sent {
        let base = a_sent();
        Sent {
            facts: ValueFacts {
                preview: ValuePreview::Text(nkb_core::preview::preview(text)),
                shape: nkb_core::preview::shape(text),
                ..base.facts
            },
            ..base
        }
    }

    /// The markers are facts about the value, and every one of them shows.
    #[test]
    fn a_risky_value_carries_all_three_markers_with_risk_first() {
        let markers = markers_of(&a_sent());
        assert_eq!(
            markers.len(),
            3,
            "offensive, warnings and cleared: {markers:?}"
        );
        assert!(markers[0].1, "the risk comes first - product-spec.md 10.2");
        assert!(!markers[2].1, "clearing the field is not a risk");
    }

    /// The standing sentence goes in FRONT of this view's own, and survives a
    /// view that has messages of its own.
    ///
    /// 🔴 The order is the point. What the standing sentence says is that the
    /// tool could not keep a promise - the palette taking the focus, say - and a
    /// tester reading top to bottom has to meet that before "end of pack".
    #[test]
    fn a_standing_sentence_leads_every_message_band() {
        let standing = quiet();
        assert_eq!(
            with_standing(vec![String::from("own")], &standing),
            vec![String::from("own")],
            "nothing standing means nothing added"
        );
        *standing.lock().expect("a fresh lock is not poisoned") = Some(String::from("kept"));
        assert_eq!(
            with_standing(vec![String::from("own")], &standing),
            vec![String::from("kept"), String::from("own")]
        );
        assert_eq!(
            with_standing(Vec::new(), &standing),
            vec![String::from("kept")],
            "a view with nothing to say still carries it"
        );
    }

    /// A settings store that keeps every save and fails when told to.
    struct Remembered {
        saved: std::cell::RefCell<Vec<nkb_app::ports::SettingChange>>,
        fail: bool,
    }

    impl nkb_app::ports::SettingsStore for Remembered {
        fn load(&self) -> nkb_app::ports::SettingsLoad {
            nkb_app::ports::SettingsLoad::Absent
        }
        fn save(
            &self,
            change: &nkb_app::ports::SettingChange,
        ) -> Result<(), nkb_app::ports::SaveError> {
            self.saved.borrow_mut().push(change.clone());
            if self.fail {
                Err(nkb_app::ports::SaveError::Unwritable)
            } else {
                Ok(())
            }
        }
    }

    /// 🔴 The state drawn and the state saved are one value (`D84`): every
    /// toggle flips it, hands it on and saves the same thing.
    #[test]
    fn a_collapse_flips_the_state_and_saves_exactly_the_state_it_draws() {
        use nkb_app::ports::SettingChange;
        let store = Remembered {
            saved: std::cell::RefCell::new(Vec::new()),
            fail: false,
        };
        let (kept, _) = nkb_app::KeptSettings::open(&store);
        let collapse = super::Collapse::new(kept, &store, String::from("f"));
        assert_eq!(collapse.toggle(), (true, None));
        assert_eq!(collapse.toggle(), (false, None));
        assert_eq!(
            *store.saved.borrow(),
            vec![SettingChange::Compact(true), SettingChange::Compact(false)]
        );
    }

    /// A save that fails is said once, and the palette collapses anyway.
    #[test]
    fn a_collapse_that_cannot_be_saved_is_said_once_and_still_happens() {
        let store = Remembered {
            saved: std::cell::RefCell::new(Vec::new()),
            fail: true,
        };
        let (kept, _) = nkb_app::KeptSettings::open(&store);
        let collapse = super::Collapse::new(kept, &store, String::from("f"));
        let (compact, line) = collapse.toggle();
        assert!(compact, "the palette collapsed although the save failed");
        assert!(
            line.is_some_and(|line| line.contains("could not be saved to f")),
            "the first failure is said"
        );
        assert_eq!(collapse.toggle(), (false, None), "and only once");
    }

    /// A value with nothing worth saying about it says nothing.
    #[test]
    fn an_ordinary_value_carries_no_marker() {
        assert!(markers_of(&quiet_sent()).is_empty());
    }

    /// A value with no marker of its own, for the tests that add exactly one.
    fn quiet_sent() -> Sent {
        let base = a_sent();
        Sent {
            facts: ValueFacts {
                warnings: 0,
                ..base.facts
            },
            offensive: false,
            cleared: false,
            ..base
        }
    }

    /// A value cut short says so, as a risk: the preview above it is the whole
    /// value and the field holds a piece of it (`OBS-126`).
    #[test]
    fn a_value_cut_short_is_marked_interrupted_as_a_risk() {
        let cut = Sent {
            arrival: Arrival::Interrupted {
                units_sent: 2,
                units_expected: 7,
            },
            ..quiet_sent()
        };
        assert_eq!(markers_of(&cut), vec![(String::from("interrupted"), true)]);
    }

    /// A value that went to the clipboard says so, where `cleared first` would
    /// stand - never both, because the clipboard route presses nothing.
    #[test]
    fn a_value_on_the_clipboard_is_marked_so_in_place_of_cleared() {
        let pasted = Sent {
            arrival: Arrival::OnClipboard,
            ..quiet_sent()
        };
        assert_eq!(
            markers_of(&pasted),
            vec![(String::from("on the clipboard"), false)]
        );
    }
}
