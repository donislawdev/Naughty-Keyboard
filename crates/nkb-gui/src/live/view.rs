//! What the palette draws, apart from the loop that decides it: the view of
//! one turn as plain data, built on the worker from what the sequence says,
//! and put on the window on the main thread. Kept in a file of its own so that
//! the loop and the drawing are found apart - and no file grows past the
//! ceiling of `D113`. The public names are still `nkb_gui::live::<name>`.

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

/// The band over the value band: what the next press sends.
pub(super) struct NextView {
    pub(super) heading: String,
    /// `None` at the end of a pack, where the press only says so.
    pub(super) value: Option<NextValueView>,
}

pub(super) struct NextValueView {
    /// What the Copy button beside it copies (`D98`).
    pub(super) key: ValueKey,
    pub(super) name: String,
    /// One line: the same preview the value band draws, which the band elides.
    pub(super) preview: String,
    pub(super) markers: Vec<(String, bool)>,
}

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
/// While the shortcuts are paused every such view says so, first among its
/// own lines and behind the standing sentence: the band is rebuilt from
/// scratch each time, so a line said once would last exactly one view - and a
/// pack chosen in the pack window during the pause would take it away.
pub(super) fn view_between(
    sequence: &AdvanceSequence,
    pack: &str,
    mut messages: Vec<String>,
    value: ValueBand,
    standing: &Standing,
    paused: bool,
) -> View {
    if paused {
        messages.insert(0, i18n::label(PaletteLabel::ShortcutsPaused).to_owned());
    }
    View {
        pack: shown(sequence, pack),
        counter: counter_of(sequence),
        next: next_view(sequence.upcoming().as_ref()),
        value,
        messages: with_standing(messages, standing),
        clipboard_bar: clipboard_bar(
            sequence.sequence().delivery,
            sequence.clipboard_for_window(),
        ),
        clipboard_mode_on: sequence.sequence().delivery == Delivery::ClipboardMode,
        clearing: Some(sequence.clearing()),
        legend: None,
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
        next: next_view(outcome.upcoming.as_ref()),
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
    }
}

/// The band of the next value, every line finished (`UX-GUI-001`).
pub(super) fn next_view(upcoming: Option<&UpcomingValue>) -> Option<NextView> {
    match upcoming? {
        UpcomingValue::EndOfPack { total } => Some(NextView {
            heading: i18n::next_end_of_pack(*total),
            value: None,
        }),
        UpcomingValue::Value {
            index,
            total,
            key,
            name,
            preview,
            offensive,
            ..
        } => Some(NextView {
            heading: i18n::next_value(*index, *total),
            value: Some(NextValueView {
                key: key.clone(),
                name: name.clone(),
                preview: preview_line(preview).0,
                // The one risk known before a send. Warnings, `cleared first`
                // and the rest are about a delivery that has not happened.
                markers: if *offensive {
                    vec![(i18n::label(PaletteLabel::Offensive).to_owned(), true)]
                } else {
                    Vec::new()
                },
            }),
        }),
    }
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
    if let Some(legend) = view.legend {
        show_legend(palette, legend);
    }

    match view.value {
        ValueBand::Keep => {}
        // The band goes back to what it said before anything was sent, which
        // is true again: nothing has gone out of this pack yet.
        ValueBand::Clear => palette.set_has_value(false),
        ValueBand::Show(value) => show_value(palette, *value),
    }

    // 🔴 No timer and no state change here, and that is `D83`: until then every
    // view woke the palette and a timer put it back to rest four seconds later,
    // taking the value with it. Whether the palette is compact is the tester's
    // choice alone - see `Collapse`.
}

/// The value band, on the main thread.
pub(super) fn show_value(palette: &Palette, value: ValueView) {
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
    palette.set_has_value(true);
}

/// The band of the next value, on the main thread (`UX-GUI-001`).
pub(super) fn show_next(palette: &Palette, next: Option<NextView>) {
    let Some(next) = next else {
        palette.set_has_next(false);
        return;
    };
    palette.set_next_heading(next.heading.into());
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

/// What a click on a Copy button asks the worker for, from the key the
/// palette holds beside the band it stands in (`D98`). On the MAIN thread.
///
/// `None` for a key never set - the button stands only beside a value, so it
/// cannot be reached then, and an empty identifier is said as nothing rather
/// than sent to be refused.
#[must_use]
pub fn copy_command(shown: &ShownKey) -> Option<Command> {
    if shown.pack.is_empty() || shown.value.is_empty() {
        return None;
    }
    Some(Command::Copy(ValueKey {
        pack: shown.pack.to_string(),
        value: shown.value.to_string(),
    }))
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
