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
//!   three;
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

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use nkb_adapters::i18n::PaletteLabel;
use nkb_adapters::{BuiltInCatalogue, DirectInjection, GlobalShortcuts, TomlPackFormat, i18n};
use nkb_app::ports::HotkeyRegistrar;
use nkb_app::{AdvanceSequence, Outcome, drive_sequence};
use nkb_core::hotkeys::DEFAULT_BINDINGS;
use nkb_core::sequence::Delivery;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel, Weak};

use crate::focus::{Standing, standing_line};
use crate::{Marker, Palette};

/// How long one wait for a press lasts before the stop flag is read again.
///
/// It bounds how long the window can be closed before this thread notices, and
/// nothing else: a press arriving mid-tick is delivered immediately, because the
/// wait returns on the press rather than on the timeout.
const TICK: Duration = Duration::from_millis(100);

/// How long the palette stays bright after a value goes out.
///
/// ⚠️ An estimate, not a measurement: long enough to read four short lines,
/// short enough that the palette is out of the way before the tester looks back
/// at the application. `ux-spec.md` 2 says "a few seconds" and does not pick one.
const BRIGHT_FOR: Duration = Duration::from_secs(4);

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
    degraded: bool,
    /// Whether this view starts the countdown back to the resting state.
    ///
    /// 🔴 False for the OPENING view, and that is a correction rather than a
    /// detail. The first version restarted the countdown on every view, so four
    /// seconds after launch the palette dimmed and took the hint bar with it -
    /// before a first-time tester had read it. `ux-spec.md` 2 ties the bright
    /// state to "after an insertion", and the hints to the first twenty uses.
    /// The opening view is neither.
    transient: bool,
}

struct ValueView {
    name: String,
    reference: String,
    counts: String,
    /// The value with invisible characters substituted, ready to draw.
    preview: String,
    /// Empty unless the preview is a fragment, in which case it says how much.
    elided: String,
    /// Every fact about the value on one line, already joined by `i18n`.
    shape: String,
    /// Text and whether it is a risk. The COLOUR is the palette's business -
    /// document 13 section 2.1 - so it is not decided here.
    markers: Vec<(String, bool)>,
}

/// Runs until the flag is set or the shortcuts die.
///
/// Takes the pack by name rather than a loaded sequence, because the sequence
/// must live on this thread: it is the one gate over the sequence state
/// (`architektura.md` 6a) and it never crosses back.
pub fn drive(palette: &Weak<Palette>, stop: &Arc<AtomicBool>, pack: &str, standing: &Standing) {
    let mut sequence = AdvanceSequence::new();
    let mut opening: Vec<String> = Vec::new();

    if let Err(error) = sequence.choose_pack(&BuiltInCatalogue::new(), &TomlPackFormat, pack) {
        opening.push(i18n::choose_error(&error, pack));
    }
    // Captured now, because `drive_sequence` borrows the sequence for the whole
    // loop and the closure that builds each view cannot reach it.
    let pack_shown = sequence.pack_name().unwrap_or(pack).to_owned();

    let live = match GlobalShortcuts.register(&DEFAULT_BINDINGS) {
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
    show(
        palette,
        opening_view(&pack_shown, &sequence, opening, standing),
    );

    let mut keep_going = || !stop.load(Ordering::Relaxed);
    let mut present =
        |outcome: Outcome| show(palette, view_of(&outcome, &pack_shown, pack, standing));
    let ended = drive_sequence(
        live.as_ref(),
        &mut sequence,
        &DirectInjection,
        &DirectInjection,
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
        degraded: sequence.sequence().delivery == Delivery::Degraded,
        transient: false,
    }
}

fn view_of(outcome: &Outcome, pack_shown: &str, pack: &str, standing: &Standing) -> View {
    View {
        pack: pack_shown.to_owned(),
        counter: outcome
            .sequence
            .counter()
            .map_or_else(String::new, |(done, total)| i18n::counter(done, total)),
        value: outcome.sent.as_ref().map(|sent| ValueView {
            name: sent.name.clone(),
            reference: sent.reference.clone(),
            counts: i18n::counts(sent.code_points, sent.bytes, sent.utf16_units),
            preview: sent.preview.shown.clone(),
            // An empty string rather than an Option, because the view's condition
            // is `!= ""` and a second representation of "nothing" would be one
            // more thing that can disagree with the first.
            elided: sent.preview.elided_total.map_or_else(String::new, |total| {
                i18n::preview_elided(sent.preview.shown.chars().count(), total)
            }),
            shape: i18n::shape_line(&sent.shape),
            markers: markers_of(sent),
        }),
        messages: with_standing(
            outcome
                .messages
                .iter()
                .map(|message| i18n::message(message, pack))
                .collect(),
            standing,
        ),
        degraded: outcome.sequence.delivery == Delivery::Degraded,
        // Every outcome is something that just happened, so every outcome gets
        // looked at and then gets out of the way.
        transient: true,
    }
}

/// What is worth knowing about the value beyond its name and its size.
///
/// Order is fixed rather than sorted: the risk comes first because it is the one
/// a tester must not miss, `product-spec.md` 10.2.
fn markers_of(sent: &nkb_app::advance_sequence::Sent) -> Vec<(String, bool)> {
    let mut markers = Vec::new();
    if sent.offensive {
        markers.push((i18n::label(PaletteLabel::Offensive).to_owned(), true));
    }
    if sent.warnings > 0 {
        markers.push((i18n::warnings(sent.warnings), true));
    }
    if sent.cleared {
        markers.push((i18n::label(PaletteLabel::Cleared).to_owned(), false));
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
    let transient = view.transient;
    palette.set_pack(view.pack.into());
    palette.set_counter(view.counter.into());
    palette.set_degraded(view.degraded);
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
        palette.set_has_shape(!value.shape.is_empty());
        palette.set_value_elided(value.elided.into());
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

    palette.set_showing(true);
    if transient {
        dim_later(palette);
    }
}

// The timer that returns the palette to rest. It has to outlive this call, and
// dropping a `slint::Timer` cancels it - so it is kept here, on the thread that
// started it. A thread local rather than a field because nothing else about the
// palette is owned by this crate's Rust: the window is generated code.
thread_local! {
    static REST: std::cell::RefCell<Option<slint::Timer>> = const { std::cell::RefCell::new(None) };
}

/// Restarts the countdown back to the resting state.
///
/// 🔴 The window is never hidden - `OBS-80` measured that `hide()` destroys it
/// on Windows and loses `WS_EX_NOACTIVATE` with it. Resting is the background
/// going translucent and the bands below the counter going away.
fn dim_later(palette: &Palette) {
    let weak = palette.as_weak();
    let timer = slint::Timer::default();
    timer.start(slint::TimerMode::SingleShot, BRIGHT_FOR, move || {
        if let Some(palette) = weak.upgrade() {
            palette.set_showing(false);
        }
    });
    // Replacing the previous timer cancels it, which is exactly the wanted
    // behaviour: every new value restarts the countdown rather than queueing a
    // second one behind it.
    REST.with_borrow_mut(|slot| *slot = Some(timer));
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use std::rc::Rc;

    use nkb_app::advance_sequence::{Message, Sent};
    use nkb_core::sequence::{Position, Sequence};
    use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
    use slint::platform::{Platform, PlatformError, WindowAdapter};

    use super::{Delivery, Outcome, Palette, Standing, apply, markers_of, view_of, with_standing};

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
            reference: String::from("unicode-text/zero-width"),
            name: String::from("Three zero-width spaces"),
            code_points: 7,
            bytes: 13,
            utf16_units: 7,
            offensive: true,
            warnings: 2,
            cleared: true,
            // The example from `ux-spec.md` 2, so the field-by-field test below
            // checks the same value the document draws.
            preview: nkb_core::preview::preview("ab\u{200B}\u{200B}\u{200B}cd"),
            shape: nkb_core::preview::shape("ab\u{200B}\u{200B}\u{200B}cd"),
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
            "7 code points, 13 bytes, 7 UTF-16 units"
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
        assert_eq!(
            slint::Model::row_count(&palette.get_markers()),
            3,
            "offensive, warnings and cleared all reach the window"
        );
        assert!(
            palette.get_has_value(),
            "a value went out, so the band shows"
        );
        assert!(palette.get_showing(), "an outcome wakes the palette");
        assert!(!palette.get_degraded(), "direct delivery is not degraded");

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

        // ---- the second axis reaches the standing bar ---------------------
        let mut degraded = an_outcome(Some(a_sent()), Vec::new());
        degraded.sequence.delivery = Delivery::Degraded;
        apply(&palette, view_of(&degraded, "p", "p", &quiet()));
        assert!(palette.get_degraded());
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

    /// A value with nothing worth saying about it says nothing.
    #[test]
    fn an_ordinary_value_carries_no_marker() {
        let plain = Sent {
            offensive: false,
            warnings: 0,
            cleared: false,
            ..a_sent()
        };
        assert!(markers_of(&plain).is_empty());
    }
}
