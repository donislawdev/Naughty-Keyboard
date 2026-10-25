//! What the palette draws, apart from the loop that decides it: the view of
//! one turn as plain data, built on the worker from what the sequence says,
//! and put on the window on the main thread. Kept in a file of its own so that
//! the loop and the drawing are found apart - and no file grows past the
//! ceiling of `D113`. The public names are still `nkb_gui::live::<name>`.

use std::cell::Cell;

use nkb_core::hotkeys::HotkeyChord;
use nkb_core::pack::PairHalf;
use nkb_core::sequence::Sequence;
use slint::ComponentHandle;

use super::*;

/// One turn of the loop, as plain data that can cross a thread boundary.
///
/// Every field is a finished string. Nothing in this struct is a Slint type, and
/// that is not an accident: the view is built here, on the worker, and applied
/// there, on the main thread, so the conversion happens in exactly one place.
pub struct View {
    pub(super) pack: String,
    pub(super) counter: String,
    /// What the next press sends, or `None` when it sends nothing to announce
    /// (`UX-GUI-001`). Always computed, never kept: a stale "next" would name a
    /// value the press does not send.
    pub(super) next: Option<NextView>,
    pub(super) value: ValueBand,
    pub(super) messages: Vec<String>,
    /// The standing clipboard bar and its words, or `None` when values are
    /// typed. Two conditions share the one bar: the mode the sequence is in,
    /// and the window in front taking no typing (`D72`). The words differ, so
    /// the view carries them rather than a flag.
    pub(super) clipboard_bar: Option<&'static str>,
    /// Whether the sequence is in clipboard mode - the tester's mode, which a
    /// click turns off, as against the window in front that only the next
    /// window ends (`D99`). Decides which way the route switch fills and
    /// whether the bar carries its Turn off.
    pub(super) clipboard_mode_on: bool,
    /// How a typed value meets the field (`D101`), for the switch under the
    /// hint bar - `None` leaves the switch as it stands, as a press does: only
    /// a choice between presses changes it.
    pub(super) clearing: Option<Clearing>,
    /// The words naming the shortcuts, when the table in effect changed with
    /// this view - `None` leaves the ones on screen.
    pub(super) legend: Option<Legend>,
    /// The Copy button whose copy went through with this view, which says
    /// "Copied" for two seconds - `None` takes the word back (`D121`).
    pub(super) copied: Option<CopyButton>,
}

/// The ways a typed value meets the field, in the order the switch under the
/// hint bar shows them (`UX-GUI-005`, `D101`, the owner's point 2). ONE list
/// for the words and for what a click on each means, so the two cannot drift
/// apart by one index.
pub(super) const CLEARINGS: [Clearing; 2] = [Clearing::Line, Clearing::Keep];

/// How values travel, in the order the other switch shows them (`D99`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Route {
    /// Typed into the field.
    Keyboard,
    /// Put on the clipboard for the tester to paste - clipboard mode.
    Clipboard,
}

/// The same rule as [`CLEARINGS`]: one list for the words and the meaning.
pub(super) const ROUTES: [Route; 2] = [Route::Keyboard, Route::Clipboard];

pub(super) fn clearing_word(clearing: Clearing) -> &'static str {
    match clearing {
        Clearing::Line => i18n::label(PaletteLabel::ClearLineFirst),
        Clearing::Keep => i18n::label(PaletteLabel::InsertAtCursor),
    }
}

pub(super) fn route_word(route: Route) -> &'static str {
    match route {
        Route::Keyboard => i18n::label(PaletteLabel::RouteKeyboard),
        Route::Clipboard => i18n::label(PaletteLabel::RouteClipboard),
    }
}

/// Where `item` stands in `ways`, as the view's index. Every item of the two
/// lists above stands in them, so the fallback is never reached.
pub(super) fn index_in<T: PartialEq>(ways: &[T], item: &T) -> i32 {
    ways.iter()
        .position(|way| way == item)
        .and_then(|at| i32::try_from(at).ok())
        .unwrap_or_default()
}

/// The thing at `index` of `ways`, or `None` for an index the switch never
/// shows - a negative one included.
pub(super) fn at_index<T: Copy>(ways: &[T], index: i32) -> Option<T> {
    usize::try_from(index)
        .ok()
        .and_then(|at| ways.get(at).copied())
}

/// Puts the words of both switches on the palette - their labels and ways,
/// which never change while it runs. On the MAIN thread, once, at start.
pub fn label_switches(palette: &Palette) {
    let words = |ways: Vec<&'static str>| {
        ModelRc::new(VecModel::from(
            ways.into_iter().map(SharedString::from).collect::<Vec<_>>(),
        ))
    };
    palette.set_route_label(i18n::label(PaletteLabel::SendBy).into());
    palette.set_route_options(words(
        ROUTES.iter().map(|&route| route_word(route)).collect(),
    ));
    palette.set_clearing_label(i18n::label(PaletteLabel::EachValue).into());
    palette.set_clearing_options(words(
        CLEARINGS
            .iter()
            .map(|&clearing| clearing_word(clearing))
            .collect(),
    ));
}

/// Puts what the two arrows beside the next value do on the palette (`D120`),
/// for UI Automation - words that never change while it runs. On the MAIN
/// thread, once, at start, beside the switches. The number each arrow carries
/// changes with every view (`show_next`).
pub fn label_arrows(palette: &Palette) {
    palette.set_back_action(i18n::label(PaletteLabel::BackAction).into());
    palette.set_skip_action(i18n::label(PaletteLabel::SkipAction).into());
}

/// Puts the way of clearing in effect on its switch. On the MAIN thread - at
/// start, before the worker's first view, and from every view that carries
/// one, so the switch cannot be set two ways.
pub fn show_clearing(palette: &Palette, clearing: Clearing) {
    palette.set_clearing_selected(index_in(&CLEARINGS, &clearing));
}

/// What a click on way `index` of the clearing switch asks the worker for.
/// `None` for an index the switch does not show.
#[must_use]
pub fn clearing_command(index: i32) -> Option<Command> {
    at_index(&CLEARINGS, index).map(Command::SetClearing)
}

/// What a click on way `index` of the route switch asks the worker for: the
/// two commands the two buttons sent before the switch (`D99`).
#[must_use]
pub fn route_command(index: i32) -> Option<Command> {
    at_index(&ROUTES, index).map(|route| match route {
        Route::Keyboard => Command::TurnOffClipboard,
        Route::Clipboard => Command::UseClipboard,
    })
}

/// The way of clearing the palette starts with: what the tester last chose,
/// or the line. One function for both threads, so they start from one answer.
#[must_use]
pub fn starts_clearing(settings: &Settings) -> Clearing {
    settings.clearing.unwrap_or(Clearing::Line)
}

/// What the value band does with this view.
pub(super) enum ValueBand {
    /// This turn produced no value - a warning, a refusal, a press that
    /// arrived while busy. The palette KEEPS THE PREVIOUS VALUE on screen:
    /// blanking it would take away the thing the message is about.
    Keep,
    /// Another pack is in use now. The value on screen came from the one
    /// before, and under the new pack's name it would be a false statement.
    Clear,
    /// A value went out.
    /// Boxed, because the view of a value is a dozen strings and the other two
    /// variants carry nothing - a band kept would otherwise move them all.
    Show(Box<ValueView>),
}

/// The band over the value band: what the next press sends. Numbers, not
/// sentences: the sentences name the shortcuts in effect, which the main thread
/// knows from the legend (`show_next`).
pub(super) struct NextView {
    /// The value the next press types, or `None` at the end of a pack, where
    /// the press only says so.
    pub(super) index: Option<usize>,
    pub(super) total: usize,
    /// The value the shortcut that steps back types - the core's answer,
    /// `Sequence::previous_index`, or `None` with nothing behind.
    pub(super) previous: Option<usize>,
    /// `None` at the end of a pack.
    pub(super) value: Option<NextValueView>,
    /// Whether the arrow that steps back moves anything (`D120`) - the core's
    /// answer, `Sequence::steps_back`, not a rule of this file's own.
    pub(super) can_go_back: bool,
}

pub(super) struct NextValueView {
    /// What the Copy button beside it copies (`D98`).
    pub(super) key: ValueKey,
    pub(super) name: String,
    /// One line: the same preview the value band draws, which the band elides.
    pub(super) preview: String,
    pub(super) markers: Vec<(String, bool)>,
}

/// Empty by default - the band with nothing sent, which `clear_value` draws.
#[derive(Default)]
pub(super) struct ValueView {
    /// What the Copy button beside it copies (`D98`) - `None` for a value
    /// still on its way, which leaves the key of the last value sent in place
    /// while the button cannot be used.
    pub(super) key: Option<ValueKey>,
    pub(super) name: String,
    /// Where it was typed (UX8, `D104`) - empty for a value on its way and for
    /// one put on the clipboard.
    pub(super) sent_to: String,
    pub(super) reference: String,
    pub(super) counts: String,
    /// The value with invisible characters substituted, or the recipe of a
    /// generated one - ready to draw either way.
    pub(super) preview: String,
    /// Empty unless the preview is a fragment, in which case it says how much.
    pub(super) elided: String,
    /// Empty unless the preview draws something the shipped typeface does not
    /// guarantee, in which case it names those characters (`D52`, `D66`).
    pub(super) not_guaranteed: String,
    /// Every fact about the value on one line, already joined by `i18n`.
    pub(super) shape: String,
    /// Text and whether it is a risk. The COLOUR is the palette's business -
    /// document 13 section 2.1 - so it is not decided here.
    pub(super) markers: Vec<(String, bool)>,
    /// The report block of a value that went out (`D121`), word for word what
    /// the report shortcut copies - empty for a value on its way, whose block
    /// does not exist yet.
    pub(super) report: String,
}

/// The name the palette shows for the pack: its own name when it opened, the
/// identifier that was tried when nothing did.
pub(super) fn shown(sequence: &AdvanceSequence, pack: &str) -> String {
    sequence.pack_name().unwrap_or(pack).to_owned()
}

pub(super) fn counter_of(sequence: &AdvanceSequence) -> String {
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
pub(super) fn with_standing(mut messages: Vec<String>, standing: &Standing) -> Vec<String> {
    if let Some(line) = standing_line(standing) {
        messages.insert(0, line);
    }
    messages
}

/// A view made between presses, with no outcome behind it: the palette
/// opening, a pack chosen, the shortcuts paused, taken again or gone.
///
/// While the shortcuts are held back - paused, or asked of a desktop that has
/// not answered - every such view says so (`held_back`, the sentence that does),
/// first among its own lines and behind the standing sentence: the band is
/// rebuilt from scratch each time, so a line said once would last exactly one
/// view - and a pack chosen in the pack window during the pause would take it
/// away.
pub(super) fn view_between(
    sequence: &AdvanceSequence,
    pack: &str,
    mut messages: Vec<String>,
    value: ValueBand,
    standing: &Standing,
    held_back: Option<String>,
) -> View {
    if let Some(line) = held_back {
        messages.insert(0, line);
    }
    View {
        pack: shown(sequence, pack),
        counter: counter_of(sequence),
        next: next_view(sequence.upcoming().as_ref(), &sequence.sequence()),
        value,
        messages: with_standing(messages, standing),
        clipboard_bar: clipboard_bar(
            sequence.sequence().delivery,
            sequence.clipboard_for_window(),
        ),
        clipboard_mode_on: sequence.sequence().delivery == Delivery::ClipboardMode,
        clearing: Some(sequence.clearing()),
        legend: None,
        copied: None,
    }
}

/// The words of the standing clipboard bar, if it stands.
///
/// The mode wins over the window: in clipboard mode every value goes there
/// anyway, and "for this window" would suggest the others are typed.
pub(super) fn clipboard_bar(delivery: Delivery, for_window: bool) -> Option<&'static str> {
    if delivery == Delivery::ClipboardMode {
        Some(i18n::label(PaletteLabel::ClipboardMode))
    } else if for_window {
        Some(i18n::label(PaletteLabel::ClipboardForWindow))
    } else {
        None
    }
}

pub(super) fn view_of(
    outcome: &Outcome,
    pack_shown: &str,
    pack: &str,
    standing: &Standing,
    bindings: &Bindings,
) -> View {
    View {
        pack: pack_shown.to_owned(),
        counter: outcome
            .sequence
            .counter()
            .map_or_else(String::new, |(done, total)| i18n::counter(done, total)),
        next: next_view(outcome.upcoming.as_ref(), &outcome.sequence),
        value: outcome.sent.as_ref().map_or(ValueBand::Keep, |sent| {
            ValueBand::Show(Box::new(value_view(sent)))
        }),
        messages: with_standing(
            outcome
                .messages
                .iter()
                .map(|message| i18n::message(message, pack, bindings))
                .collect(),
            standing,
        ),
        clipboard_bar: clipboard_bar(outcome.sequence.delivery, outcome.clipboard_for_window),
        clipboard_mode_on: outcome.sequence.delivery == Delivery::ClipboardMode,
        // A press never changes how values meet the field, nor the table.
        clearing: None,
        legend: None,
        copied: None,
    }
}

/// The band of the next value, every line finished (`UX-GUI-001`).
pub(super) fn next_view(upcoming: Option<&UpcomingValue>, sequence: &Sequence) -> Option<NextView> {
    let previous = sequence.previous_index();
    let can_go_back = sequence.steps_back();
    match upcoming? {
        UpcomingValue::EndOfPack { total } => Some(NextView {
            index: None,
            total: *total,
            previous,
            value: None,
            can_go_back,
        }),
        UpcomingValue::Value {
            index,
            total,
            key,
            name,
            preview,
            offensive,
            halves,
            ..
        } => Some(NextView {
            index: Some(*index),
            total: *total,
            previous,
            value: Some(NextValueView {
                key: key.clone(),
                name: name.clone(),
                preview: preview_line(preview).0,
                markers: next_markers(*offensive, halves),
            }),
            can_go_back,
        }),
    }
}

/// The markers of the value the next press sends: the one risk known before a
/// send, then which half of a pair it is (`pack-format.md` 9, `OBS-181`).
/// Warnings, `cleared first` and the rest are about a delivery that has not
/// happened, so they wait for the band of the value that went out.
fn next_markers(offensive: bool, halves: &[PairHalf]) -> Vec<(String, bool)> {
    let mut markers = Vec::new();
    if offensive {
        markers.push((i18n::label(PaletteLabel::Offensive).to_owned(), true));
    }
    // Not a risk: a pair is how the pack meant the value to be typed.
    markers.extend(
        halves
            .iter()
            .map(|half| (i18n::pair_half(*half).to_owned(), false)),
    );
    markers
}

/// The line a preview draws, and - when it is a fragment - the sentence that
/// says how much of the value it is. One function for the band of the value
/// that went out, the band of the one about to go, and every value row of the
/// value window (the owner's point 5), so the three cannot preview one value
/// two ways.
pub(crate) fn preview_line(preview: &ValuePreview) -> (String, String) {
    match preview {
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
    }
}

/// The value band, every line finished.
pub(super) fn value_view(sent: &Sent) -> ValueView {
    ValueView {
        key: Some(sent.key.clone()),
        sent_to: sent.target.as_ref().map(i18n::sent_to).unwrap_or_default(),
        report: sent.report.clone().unwrap_or_default(),
        ..facts_view(&sent.facts, sent.utf16_units, markers_of(sent))
    }
}

/// The value band of any value - one that went out, or one on its way - from
/// its facts, its size in UTF-16 units and the markers it earns.
pub(super) fn facts_view(
    facts: &ValueFacts,
    utf16_units: usize,
    markers: Vec<(String, bool)>,
) -> ValueView {
    let (preview, elided) = preview_line(&facts.preview);
    // Measured on the line the preview DRAWS, not on the whole value: a
    // character in the elided middle never reaches the screen, and for a
    // recipe the line is the unit plus digits and a sign the typeface carries.
    let not_guaranteed =
        i18n::not_guaranteed(&outside_guarantee(&preview, &SHIPPED)).unwrap_or_default();
    ValueView {
        key: None,
        name: facts.name.clone(),
        sent_to: String::new(),
        reference: facts.reference.clone(),
        counts: i18n::counts(facts.graphemes, facts.code_points, facts.bytes, utf16_units),
        preview,
        elided,
        not_guaranteed,
        shape: i18n::shape_line(&facts.shape),
        markers,
        report: String::new(),
    }
}

/// What the palette draws while a value goes (`OBS-160`): the value itself in
/// the value band - the same one the band will show when it is in - and how
/// far it got in the send band.
pub(super) struct InFlightView {
    pub(super) value: ValueView,
    pub(super) counter: String,
    pub(super) fraction: f32,
    pub(super) messages: Vec<String>,
}

pub(super) fn in_flight_view(in_flight: &InFlight, standing: &Standing) -> InFlightView {
    let Progress {
        units_arrived,
        units_total,
    } = in_flight.progress;
    InFlightView {
        value: facts_view(&in_flight.facts, units_total, markers_in_flight(in_flight)),
        counter: i18n::typing_counter(units_arrived, units_total),
        fraction: share(units_arrived, units_total),
        // What the band said was about the value before, and is stale now. The
        // standing sentence is not about any value, so it stays first.
        messages: with_standing(Vec::new(), standing),
    }
}

/// What the message band says while a press waits for the person (`D145` point 76): the
/// sentence while the question stands, and nothing of its own after it. The standing
/// sentence stays in front, as in every view. What the band said before was about the value
/// before and is stale once a press is under way, as it is in a view of a value on its way.
pub(super) fn waiting_messages(waiting: Waiting, standing: &Standing) -> Vec<String> {
    let own = match waiting {
        Waiting::ForConsent => vec![i18n::label(PaletteLabel::WaitingForConsent).to_owned()],
        Waiting::Answered => Vec::new(),
    };
    with_standing(own, standing)
}

/// Runs on the MAIN thread: the message band, and nothing else. The press is still going, so
/// the rest of the palette stays as it is.
pub(super) fn apply_waiting(palette: &Palette, messages: Vec<String>) {
    palette.set_messages(ModelRc::new(VecModel::from(
        messages
            .into_iter()
            .map(SharedString::from)
            .collect::<Vec<_>>(),
    )));
}

/// The markers a value earns before it is in: the two that are facts about
/// the value. `cleared first`, `interrupted` and `on the clipboard` are facts
/// about the delivery, and the delivery is not over.
pub(super) fn markers_in_flight(in_flight: &InFlight) -> Vec<(String, bool)> {
    let mut markers = Vec::new();
    if in_flight.offensive {
        markers.push((i18n::label(PaletteLabel::Offensive).to_owned(), true));
    }
    if in_flight.facts.warnings > 0 {
        markers.push((i18n::warnings(in_flight.facts.warnings), true));
    }
    markers
}

/// The share of a send that arrived, from 0 to 1 - clamped here, so the view
/// only multiplies. A total of zero never reports, and is nothing if it does.
#[allow(
    clippy::cast_precision_loss,
    reason = "a share drawn on a bar a few hundred pixels wide needs no more than f32 holds"
)]
pub(super) fn share(arrived: usize, total: usize) -> f32 {
    if total == 0 {
        return 0.0;
    }
    (arrived as f32 / total as f32).clamp(0.0, 1.0)
}

/// Runs on the MAIN thread: the value on its way and the send band.
pub(super) fn apply_in_flight(palette: &Palette, view: InFlightView) {
    // A send is what the palette is about now, not a copy before it.
    show_copied(palette, None);
    show_value(palette, view.value);
    palette.set_messages(ModelRc::new(VecModel::from(
        view.messages
            .into_iter()
            .map(SharedString::from)
            .collect::<Vec<_>>(),
    )));
    palette.set_sending_label(i18n::label(PaletteLabel::Typing).into());
    palette.set_sending_hint(i18n::label(PaletteLabel::StopTyping).into());
    palette.set_sending_counter(view.counter.into());
    palette.set_sending_fraction(view.fraction);
    palette.set_sending(true);
}

/// What is worth knowing about the value beyond its name and its size.
///
/// Order is fixed rather than sorted: the risk comes first because it is the one
/// a tester must not miss, `product-spec.md` 10.2.
pub(super) fn markers_of(sent: &Sent) -> Vec<(String, bool)> {
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
pub(super) fn show(palette: &Weak<Palette>, view: View) {
    let _ = palette.upgrade_in_event_loop(move |palette| apply(&palette, view));
}

/// Runs on the MAIN thread. Everything Slint touches happens here.
pub(super) fn apply(palette: &Palette, view: View) {
    // Any view of the worker's comes after a send or between two, so no send
    // is on its way any more (`OBS-160`). The reports of a send are all handed
    // over before its outcome, and the main thread takes them in order.
    palette.set_sending(false);
    palette.set_pack(view.pack.into());
    palette.set_counter(view.counter.into());
    // The legend first: the band of the next value names its shortcuts.
    if let Some(legend) = view.legend {
        show_legend(palette, legend);
    }
    show_next(palette, view.next);
    palette.set_clipboard_mode(view.clipboard_bar.is_some());
    if let Some(words) = view.clipboard_bar {
        palette.set_clipboard_mode_label(words.into());
    }
    palette.set_clipboard_mode_on(view.clipboard_mode_on);
    palette.set_route_selected(index_in(
        &ROUTES,
        &if view.clipboard_mode_on {
            Route::Clipboard
        } else {
            Route::Keyboard
        },
    ));
    if let Some(clearing) = view.clearing {
        show_clearing(palette, clearing);
    }
    palette.set_messages(ModelRc::new(VecModel::from(
        view.messages
            .into_iter()
            .map(SharedString::from)
            .collect::<Vec<_>>(),
    )));

    match view.value {
        ValueBand::Keep => {}
        // The band goes back to what it said before anything was sent, which
        // is true again: nothing has gone out of this pack yet.
        ValueBand::Clear => clear_value(palette),
        ValueBand::Show(value) => show_value(palette, *value),
    }

    // 🔴 No state change here, and that is `D83`: until then every view woke
    // the palette and a timer put it back to rest four seconds later, taking
    // the value with it. Whether the palette is compact is the tester's choice
    // alone - see `Collapse`. The one clock left is the word "Copied" on the
    // button whose copy went through (`D121`): it changes nothing the tester
    // chose and moves nothing.
    show_copied(palette, view.copied);
}

/// The value band, on the main thread.
pub(super) fn show_value(palette: &Palette, value: ValueView) {
    fill_value(palette, value);
    palette.set_has_value(true);
}

/// The value band back at its first frame, on the main thread: another pack
/// is in use and nothing has gone out of it yet. Every line goes, not only the
/// switch, because not every line stands behind the switch - the folded
/// band's heading names the value even while it is faded (`D107`), so a name
/// left in its property stood under the new pack's name (measured on the live
/// window, 2026-10-14).
fn clear_value(palette: &Palette) {
    fill_value(palette, ValueView::default());
    palette.set_has_value(false);
}

/// Every line of the value band. The one place that writes them, so the band
/// shown and the band cleared cannot drift apart by a line.
fn fill_value(palette: &Palette, value: ValueView) {
    // With the band it names, in the same turn of the event loop: a click
    // reads the key of the value drawn, never one a view later.
    if let Some(key) = &value.key {
        palette.set_last_key(shown_key(key));
    }
    palette.set_value_name(value.name.into());
    // The switch is read BEFORE the string moves into its property.
    palette.set_has_sent_to(!value.sent_to.is_empty());
    palette.set_value_sent_to(value.sent_to.into());
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
    palette.set_markers(markers_model(value.markers));
    // The switch is read BEFORE the string moves into its property.
    palette.set_has_report(!value.report.is_empty());
    palette.set_report(value.report.into());
}

/// The values the two arrows step to (the owner's point 1, 2026-10-14), or
/// `None` where an arrow moves nothing or reaches the end of the pack - the
/// arrow then carries no number. The walk is the core's: back from the end
/// goes to the last value, forward from the last to the end, forward from the
/// end to the first (`D120`).
pub(super) fn arrow_targets(
    index: Option<usize>,
    total: usize,
    can_go_back: bool,
) -> (Option<usize>, Option<usize>) {
    match index {
        Some(index) => (
            (can_go_back && index > 1).then(|| index - 1),
            (index < total).then_some(index + 1),
        ),
        None => (
            (can_go_back && total > 0).then_some(total),
            (total > 0).then_some(1),
        ),
    }
}

/// The band of the next value, on the main thread (`UX-GUI-001`).
pub(super) fn show_next(palette: &Palette, next: Option<NextView>) {
    let Some(next) = next else {
        palette.set_has_next(false);
        return;
    };
    let (types, steps_back) = named();
    palette.set_next_heading(
        match next.index {
            Some(index) => i18n::next_value(index, next.total, types),
            None => i18n::next_end_of_pack(next.total, types),
        }
        .into(),
    );
    // Always a line, empty with nothing behind: the band keeps its height
    // (`D110`), so the line coming and going moves nothing below it.
    palette.set_previous_line(
        next.previous
            .zip(steps_back)
            .map_or_else(String::new, |(index, chord)| {
                i18n::previous_types(index, chord)
            })
            .into(),
    );
    let (back, skip) = arrow_targets(next.index, next.total, next.can_go_back);
    palette.set_back_label(back.map_or_else(String::new, i18n::back_to).into());
    palette.set_skip_label(skip.map_or_else(String::new, i18n::skip_to).into());
    palette.set_can_go_back(next.can_go_back);
    match next.value {
        Some(value) => {
            palette.set_next_key(shown_key(&value.key));
            palette.set_next_name(value.name.into());
            palette.set_next_preview(value.preview.into());
            palette.set_next_markers(markers_model(value.markers));
            palette.set_next_has_value(true);
        }
        // At the end of a pack the band keeps its lines, drawn empty, so it is
        // as tall as anywhere else in the pack (`D110`) - so what stood on them
        // goes, the markers included, which no `visible` in the view reaches.
        None => {
            palette.set_next_name(SharedString::new());
            palette.set_next_preview(SharedString::new());
            palette.set_next_markers(markers_model(Vec::new()));
            palette.set_next_has_value(false);
        }
    }
    palette.set_has_next(true);
}

/// A key as the palette holds it.
pub(super) fn shown_key(key: &ValueKey) -> ShownKey {
    ShownKey {
        pack: key.pack.as_str().into(),
        value: key.value.as_str().into(),
    }
}

/// Markers as the palette's model - text, and whether it is a risk.
pub(super) fn markers_model(markers: Vec<(String, bool)>) -> ModelRc<Marker> {
    ModelRc::new(VecModel::from(
        markers
            .into_iter()
            .map(|(text, risky)| Marker {
                text: text.into(),
                risky,
            })
            .collect::<Vec<_>>(),
    ))
}

/// Puts the palette into the compact or the expanded state.
///
/// 🔴 The window is never hidden - `OBS-80` measured that `hide()` destroys it
/// on Windows and loses `WS_EX_NOACTIVATE` with it. Compact is the background
/// going translucent and the bands below the header going away.
///
/// The window is as tall as what it shows in either state (`D110`): until
/// 2026-10-07 the expanded palette kept the tallest height it had had, and the
/// room left under the content when a sentence went stayed empty - the owner's
/// point 2. Nothing above the status line changes height by itself any more,
/// so following the content moves only the bottom edge.
///
/// Takes the state rather than flipping it: the worker owns the switch from
/// the start (`Collapse`), so the state it saves is the state drawn.
pub fn set_compact(palette: &Palette, compact: bool) {
    palette.set_compact(compact);
}

/// Opens the whole last value sent under its heading, or folds it back to the
/// heading and the name (the owner's point 7, 2026-10-07). On the MAIN thread.
///
/// A switch of the window alone, so neither the worker nor the settings file
/// hold it: it starts folded at every run, which is what the owner asked for,
/// and a key in the file would be a new public name (`settings-format.md`).
pub fn set_last_sent_open(palette: &Palette, open: bool) {
    palette.set_last_sent_open(open);
}

/// Opens the report block at the end of the opened band, or folds it
/// (`D121`, the owner's point 8). On the MAIN thread, and a switch of the
/// window alone for the reason [`set_last_sent_open`] gives: folded at every
/// start.
fn set_report_open(palette: &Palette, open: bool) {
    palette.set_report_open(open);
}

/// The two folds of the band of the value sent last - the band and its
/// report block (the owner's points 7 and 8): switches of the window alone,
/// so a click flips them here, on the MAIN thread, and nothing else hears of
/// it.
pub fn wire_folds(palette: &Palette) {
    palette.on_toggle_last_sent({
        let palette = palette.as_weak();
        move || {
            if let Some(palette) = palette.upgrade() {
                set_last_sent_open(&palette, !palette.get_last_sent_open());
            }
        }
    });
    palette.on_toggle_report({
        let palette = palette.as_weak();
        move || {
            if let Some(palette) = palette.upgrade() {
                set_report_open(&palette, !palette.get_report_open());
            }
        }
    });
}

/// The words on the palette that name a shortcut: the hint bar, the value
/// band before anything is sent, and the band of the next value. Built from
/// the table in effect rather than written out, so a shortcut the tester
/// changed is the one they show - at start, and again whenever the table
/// changes (K5.3). Kept here, beside the bands that show it, since 2026-10-14:
/// in `live.rs` it stood at the ceiling of `D113`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Legend {
    /// The key and the action of each hinted shortcut, in the table's order.
    pub(super) hints: Vec<(String, String)>,
    pub(super) no_value: String,
    /// The two shortcuts the band of the next value names (the owner's points
    /// 4 and 6, 2026-10-14): the one that types it, and the one that steps
    /// back and types. `None` where the palette cannot name one ([`Apart`]).
    pub(super) next: Option<HotkeyChord>,
    pub(super) previous: Option<HotkeyChord>,
    /// The links under the switches: the way into the shortcuts window, and where
    /// the desktop gives the shortcuts, the entry it wants (`D145` point 68).
    pub(super) links: Vec<ShortcutLink>,
}

/// A shortcut the palette cannot name by a chord of its own, because of what
/// the desktop that chooses the chords did with it (`D145` point 67).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Apart {
    /// The desktop gave none: the person left it out. Nothing responds, so the
    /// hint bar does not list it and the bands name no key for it.
    Left,
    /// The desktop gave one, in words that are none of the chords the palette
    /// knows. The hint bar shows the words as they came, and the bands name no key.
    Worded(String),
}

/// The words naming the shortcuts of `bindings`. One function for the start
/// and for every change, so the two cannot word it differently.
#[must_use]
pub fn legend(bindings: &Bindings) -> Legend {
    legend_of(bindings, &[])
}

/// The same words once a desktop that chooses the chords has answered:
/// `bindings` hold the chords it gave, and `apart` the shortcuts it gave none
/// of, or none of ours.
pub(super) fn legend_of(bindings: &Bindings, apart: &[(HotkeyAction, Apart)]) -> Legend {
    let how = |action: HotkeyAction| {
        apart
            .iter()
            .find(|(one, _)| *one == action)
            .map(|(_, how)| how)
    };
    // The chord of an action, or `None` where it has none to name.
    let named = |action: HotkeyAction| how(action).is_none().then(|| bindings.chord(action));
    Legend {
        // In the order a person reads them, not the table's (`D120`).
        hints: HotkeyAction::READING_ORDER
            .iter()
            .filter(|action| wired(**action))
            .filter_map(|action| {
                let key = match how(*action) {
                    None => i18n::chord(bindings.chord(*action)),
                    Some(Apart::Worded(words)) => words.clone(),
                    Some(Apart::Left) => return None,
                };
                Some((key, i18n::action_name(*action).to_owned()))
            })
            .collect(),
        no_value: i18n::no_value_yet(named(HotkeyAction::NextValue)),
        next: named(HotkeyAction::NextValue),
        previous: named(HotkeyAction::PreviousValue),
        links: vec![ShortcutLink::Window],
    }
}

/// Puts the words naming the shortcuts on the palette. On the MAIN thread.
pub fn show_legend(palette: &Palette, legend: Legend) {
    palette.set_hints(ModelRc::new(VecModel::from(
        legend
            .hints
            .into_iter()
            .map(|(key, action)| HintRow {
                key: key.into(),
                action: action.into(),
            })
            .collect::<Vec<_>>(),
    )));
    palette.set_no_value(legend.no_value.into());
    links::show_links(palette, &legend.links);
    NAMED.set(Some((legend.next, legend.previous)));
}

/// The two chords the band of the next value names, either `None` where the
/// palette cannot name it.
type Named = (Option<HotkeyChord>, Option<HotkeyChord>);

thread_local! {
    /// The two chords of the last legend shown, on the MAIN thread, where the
    /// band of the next value is put into words. `None` until the first
    /// legend, which comes before the first view (`main.rs`).
    static NAMED: Cell<Option<Named>> = const { Cell::new(None) };
}

/// The chords that type the next value and step back to type: the last
/// legend's, or the defaults before there was one.
fn named() -> Named {
    NAMED.get().unwrap_or_else(|| {
        let defaults = nkb_adapters::default_bindings();
        (
            Some(defaults.chord(HotkeyAction::NextValue)),
            Some(defaults.chord(HotkeyAction::PreviousValue)),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live::tests::{a_palette, a_sent, an_outcome, quiet};
    use slint::Model;

    /// `D145` point 76: the sentence of a press that waits for the person goes up in the band
    /// alone and comes down when the question is answered.
    #[test]
    fn the_wait_for_the_person_goes_up_in_the_band_and_comes_down_when_it_is_answered() {
        let palette = a_palette();
        apply_waiting(&palette, waiting_messages(Waiting::ForConsent, &quiet()));
        assert_eq!(palette.get_messages().row_count(), 1);
        assert_eq!(
            palette.get_messages().row_data(0).unwrap_or_default(),
            i18n::label(PaletteLabel::WaitingForConsent)
        );
        apply_waiting(&palette, waiting_messages(Waiting::Answered, &quiet()));
        assert_eq!(
            palette.get_messages().row_count(),
            0,
            "answered, the sentence is not there"
        );
    }

    #[test]
    fn the_wait_for_the_person_is_said_behind_the_standing_sentence_and_taken_down_when_answered() {
        let standing = quiet();
        let sentence = i18n::label(PaletteLabel::WaitingForConsent).to_owned();
        assert_eq!(
            waiting_messages(Waiting::ForConsent, &standing),
            vec![sentence.clone()]
        );
        assert!(waiting_messages(Waiting::Answered, &standing).is_empty());
        assert!(
            standing
                .lock()
                .map(|mut held| *held = Some(String::from("standing")))
                .is_ok()
        );
        // The standing sentence is the tool saying it cannot keep a promise: it outranks.
        assert_eq!(
            waiting_messages(Waiting::ForConsent, &standing),
            vec![String::from("standing"), sentence]
        );
        assert_eq!(
            waiting_messages(Waiting::Answered, &standing),
            vec![String::from("standing")]
        );
    }

    /// The legend of a table alone has the way into the shortcuts window and no more:
    /// what the desktop wants is the worker's to add once the desktop has answered.
    #[test]
    fn the_plain_legend_offers_the_shortcuts_window_and_nothing_else() {
        let table = nkb_adapters::default_bindings();
        assert_eq!(legend(&table).links, vec![ShortcutLink::Window]);
    }

    /// The owner's point 1 (2026-10-14): each arrow carries the number of the
    /// value it steps to, and none where it moves nothing or reaches the end
    /// of the pack - the core's walk (`D120`).
    #[test]
    fn each_arrow_carries_the_value_it_steps_to() {
        assert_eq!(arrow_targets(Some(4), 12, true), (Some(3), Some(5)));
        assert_eq!(arrow_targets(Some(1), 12, false), (None, Some(2)));
        assert_eq!(arrow_targets(Some(12), 12, true), (Some(11), None));
        assert_eq!(arrow_targets(None, 12, true), (Some(12), Some(1)));
        assert_eq!(arrow_targets(None, 0, false), (None, None));
    }

    /// The owner's points 4 and 6: the band names the shortcut that types the
    /// value and the one that steps back, with the values each types, and the
    /// line about the second stands empty with nothing behind.
    #[test]
    fn the_next_band_names_both_shortcuts_and_numbers_both_arrows() {
        let palette = a_palette();
        let at = |index, previous| NextView {
            index: Some(index),
            total: 12,
            previous,
            value: None,
            can_go_back: index > 1,
        };
        show_next(&palette, Some(at(4, Some(2))));
        let defaults = nkb_adapters::default_bindings();
        let types = defaults.chord(HotkeyAction::NextValue);
        let steps_back = defaults.chord(HotkeyAction::PreviousValue);
        assert_eq!(named(), (Some(types), Some(steps_back)), "no legend yet");
        assert_eq!(
            palette.get_next_heading(),
            format!("{} types: value 4 of 12", i18n::chord(types))
        );
        assert_eq!(
            palette.get_previous_line(),
            format!("{} goes back and types value 2", i18n::chord(steps_back))
        );
        assert_eq!(palette.get_back_label(), "3");
        assert_eq!(palette.get_skip_label(), "5");
        show_next(&palette, Some(at(1, None)));
        assert_eq!(palette.get_previous_line(), "", "nothing behind");
        assert_eq!(palette.get_back_label(), "", "nothing before value 1");
    }

    /// A shortcut the desktop gave in words of its own is listed as it came, one
    /// it gave none of is not listed, and the rest of the bar is as it was.
    #[test]
    fn the_hint_bar_lists_a_worded_shortcut_as_it_came_and_leaves_out_one_not_given() {
        let defaults = nkb_adapters::default_bindings();
        let plain = legend(&defaults);
        let words = String::from("Press <Control><Alt>Return");
        let apart = [
            (HotkeyAction::PreviousValue, Apart::Worded(words.clone())),
            (HotkeyAction::RestartPack, Apart::Left),
        ];

        let shown = legend_of(&defaults, &apart);

        let rows = |legend: &Legend| legend.hints.clone();
        let mut expected = rows(&plain);
        let previous = i18n::action_name(HotkeyAction::PreviousValue);
        let restart = i18n::action_name(HotkeyAction::RestartPack);
        expected.retain(|(_, action)| action != restart);
        for (key, action) in &mut expected {
            if action == previous {
                key.clone_from(&words);
            }
        }
        assert_eq!(rows(&shown), expected);
        assert_eq!(shown.next, plain.next, "typing the value is still named");
        assert_eq!(shown.previous, None, "worded, so no chord to name");
        assert_eq!(legend_of(&defaults, &[]), plain);
    }

    /// The bands name no key where the palette cannot: the heading names the
    /// value, the line about the way back stands empty, and the sentence of the
    /// empty band names the action.
    #[test]
    fn the_bands_name_no_key_for_a_shortcut_that_is_apart() {
        let palette = a_palette();
        let defaults = nkb_adapters::default_bindings();
        let at = |index, total| NextView {
            index,
            total,
            previous: Some(2),
            value: None,
            can_go_back: true,
        };
        show_legend(
            &palette,
            legend_of(
                &defaults,
                &[
                    (HotkeyAction::NextValue, Apart::Left),
                    (
                        HotkeyAction::PreviousValue,
                        Apart::Worded(String::from("Press <Control><Alt>Return")),
                    ),
                ],
            ),
        );

        show_next(&palette, Some(at(Some(4), 12)));
        assert_eq!(palette.get_next_heading(), i18n::next_value(4, 12, None));
        assert_eq!(palette.get_previous_line(), "", "no key to name");
        show_next(&palette, Some(at(None, 12)));
        assert_eq!(palette.get_next_heading(), i18n::next_end_of_pack(12, None));
        assert_eq!(palette.get_no_value(), i18n::no_value_yet(None));
    }

    /// Another pack in use (`ValueBand::Clear`): the band goes back to its
    /// first frame on the WINDOW, line by line, not only behind its switch -
    /// the folded band's heading names the value while faded (`D107`), and a
    /// name left there stood under the new pack's name (measured on the live
    /// window, 2026-10-14). That choosing a pack gives `Clear` is
    /// `choosing_another_pack_opens_it_remembers_it_and_clears_the_value`.
    #[test]
    fn another_pack_takes_every_line_of_the_old_value_off_the_window() {
        let palette = a_palette();
        show_value(&palette, value_view(&a_sent()));
        assert!(!palette.get_value_name().is_empty());
        assert!(!palette.get_value_preview().is_empty());
        assert!(palette.get_has_report());

        let mut view = view_of(
            &an_outcome(None, Vec::new()),
            "Whitespace",
            "whitespace",
            &quiet(),
            &nkb_adapters::default_bindings(),
        );
        view.value = ValueBand::Clear;
        apply(&palette, view);

        assert!(!palette.get_has_value());
        for (line, what) in [
            (palette.get_value_name(), "name"),
            (palette.get_value_sent_to(), "where it went"),
            (palette.get_value_reference(), "reference"),
            (palette.get_value_counts(), "counts"),
            (palette.get_value_preview(), "preview"),
            (palette.get_value_elided(), "elision"),
            (palette.get_value_not_guaranteed(), "typeface note"),
            (palette.get_value_shape(), "shape"),
            (palette.get_report(), "report block"),
        ] {
            assert!(line.is_empty(), "the old pack's {what} stayed: {line:?}");
        }
        assert!(!palette.get_has_sent_to());
        assert!(!palette.get_has_shape());
        assert!(!palette.get_has_report(), "the old pack's block stayed");
        assert_eq!(slint::Model::row_count(&palette.get_markers()), 0);
    }

    /// `OBS-181`: the band of the next value says which half of a pair it is,
    /// after the risk and not as one, once for each pair it stands in.
    #[test]
    fn the_next_value_names_its_half_of_each_pair_after_its_risk() {
        let upcoming = |offensive, halves| UpcomingValue::Value {
            index: 1,
            total: 12,
            key: ValueKey {
                pack: "look-alike-pairs".to_owned(),
                value: "latin-small-a".to_owned(),
            },
            name: "Name with a Latin a".to_owned(),
            reference: "look-alike-pairs/latin-small-a".to_owned(),
            preview: ValuePreview::Text(nkb_core::preview::preview("pawel")),
            offensive,
            halves,
        };
        // `None` would be a band with no value - compared, not unwrapped.
        let markers = |offensive, halves| {
            next_view(Some(&upcoming(offensive, halves)), &Sequence::default())
                .and_then(|view| view.value)
                .map(|value| value.markers)
        };
        assert_eq!(
            markers(false, vec![PairHalf::First]),
            Some(vec![("pair 1 of 2".to_owned(), false)])
        );
        assert_eq!(
            markers(true, vec![PairHalf::Second, PairHalf::First]),
            Some(vec![
                ("offensive".to_owned(), true),
                ("pair 2 of 2".to_owned(), false),
                ("pair 1 of 2".to_owned(), false),
            ])
        );
        assert_eq!(markers(false, Vec::new()), Some(Vec::new()));
    }

    /// `D121`: the report block arrives with the value it describes, and a
    /// value on its way has none - its block does not exist yet.
    #[test]
    fn the_report_block_arrives_with_its_value_and_not_before() {
        let palette = a_palette();
        let sent = a_sent();
        show_value(&palette, value_view(&sent));
        assert!(palette.get_has_report());
        assert_eq!(
            palette.get_report(),
            "Value:    unicode-text/zero-width @ pack 1.0"
        );
        show_value(
            &palette,
            facts_view(&sent.facts, sent.utf16_units, Vec::new()),
        );
        assert!(
            !palette.get_has_report(),
            "a value on its way showed a block"
        );
    }
}
