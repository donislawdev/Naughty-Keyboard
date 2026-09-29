//! The nkb palette.
//!
//! A full product in its own right, not a windowless CLI with a window bolted
//! on - D25. It shares `core` and `app` with `nkb` and shares the same catalogue
//! on disk, and neither executable can start the other.
//!
//! # What it does today, and what it does not
//!
//! It opens the palette on a pack, registers the ten global shortcuts and runs
//! the sequence from them: press `Ctrl+Alt+N` in any field and the next value of
//! the pack lands there while the palette says what went out. Two threads, and
//! `live` holds the reason they are two.
//!
//! It also refuses the keyboard focus, which `ux-spec.md` 2 calls the sharpest
//! technical requirement in the product: the window will not activate when
//! clicked, and it hands the foreground back to whoever held it. Measured rather
//! than assumed, and the measurement corrected the plan - `focus` carries the
//! table. Where it cannot be done the palette says so instead of pretending.
//!
//! The catalogue of components stays reachable behind an argument, which is what
//! document 13 section 4 asks for - it is a view for whoever is BUILDING the
//! interface, not for a tester.
//!
//! # No console window, and what had to exist first
//!
//! The attribute below stops Windows from giving this process a console. Without
//! it, a tester launching the palette from the desktop gets a black window
//! beside it with the full path to the binary in its title (`OBS-101`).
//!
//! 🔴 It is UNCONDITIONAL rather than release-only, and that is deliberate: a
//! release that behaves differently from what a session looks at is the failure
//! class of `OBS-83`, where the thing being judged is not the thing being
//! shipped.
//!
//! The attribute could not go in on its own. Slint has no runtime fallback from
//! its hardware renderer to the software one, so a machine without a graphics
//! context gets a `PlatformError` back out of `run()` and nothing else
//! (`OBS-103`). Before this change that error reached a console. After it, with
//! no channel of its own, it would reach NOBODY - which is exactly the silence
//! untouchable rule 1 forbids. So the console disappears in the same commit as
//! `report_window_failure`, which says the failure through standard error when
//! there is one and through a message box when there is not.
//!
//! ⚠️ What this still does NOT do: try the software renderer by itself. Which
//! route Slint 1.17 allows after a failed `run()` is unmeasured - the platform
//! is set once per process - and there is no machine here without OpenGL 2 to
//! measure it on. The gap is named in `OBS-103` and the sentence the tester sees
//! names the environment variable instead. Minimum of rule 1 is to SHOW the
//! failure, not to repair it quietly.
#![cfg_attr(windows, windows_subsystem = "windows")]

use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use nkb_adapters::i18n::PaletteLabel;
use nkb_adapters::{KeptFocus, SettingsFile, default_bindings, i18n, report_window_failure};
use nkb_app::{KeptSettings, RouteRequest};
use nkb_core::hotkeys::{Bindings, HotkeyAction};
use nkb_gui::packs::{Packs, SystemKeyboard};
use nkb_gui::{Gallery, HintRow, PacksWindow, Palette, focus, live};
use slint::{ComponentHandle, ModelRc, VecModel};

/// The pack the palette opens on when the command line names none and the
/// settings remember none.
///
/// One of the three that ship inside the binary (D51), so the palette has
/// something real to show on a machine with no catalogue on disk at all.
const DEFAULT_PACK: &str = "whitespace";

/// Which shortcuts the hint bar names, and in this order.
///
/// Four of the ten, because `ux-spec.md` 2 gives the hint bar four and because a
/// list of ten stops being a hint. These four are the ones the first five
/// minutes need: move through the pack, and get the report out.
///
/// `OpenPacks` opens the pack window (step 7, K3.2c): the worker takes the
/// press before the sequence sees it and asks this thread through the
/// palette's `open-packs` callback.
const HINTED: [HotkeyAction; 4] = [
    HotkeyAction::NextValue,
    HotkeyAction::PreviousValue,
    HotkeyAction::CopyReport,
    HotkeyAction::OpenPacks,
];

fn main() -> ExitCode {
    match start(request()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            // The channel is reported rather than ignored, but there is nothing
            // left to do with `Nowhere` except exit non-zero - which is why the
            // exit code below does not depend on it. Untouchable rule 1 is
            // satisfied by having tried every channel there is, in the order
            // that does not block an unattended run.
            let _channel = report_window_failure(&error.to_string());
            // The same code this binary returned before the attribute went in,
            // so nobody's script changes behaviour. `ux-spec.md` 10 governs the
            // CLI's table and is untouched: this is the other executable.
            ExitCode::FAILURE
        }
    }
}

/// What the command line asked for.
///
/// Deliberately tiny: three shapes, no flags to combine, no parser. `nkb` is
/// the executable with a command surface and `ux-spec.md` 10 is its contract.
/// This one is a window, and a window that grows an option grammar has started
/// to become the other binary.
///
/// ⚠️ The third shape, `--clipboard`, went in on purpose (`D71`) and it is
/// still not a grammar: one word, in the first place only, before the optional
/// pack - `nkb-gui --clipboard whitespace`. Switching the mode while the
/// palette runs belongs to the settings (step 7), never to another global
/// shortcut: each one is a new collision in somebody's application.
enum Request {
    /// The palette, on this pack or on the one it remembers, delivering values
    /// this way.
    Palette {
        pack: Option<String>,
        route: RouteRequest,
    },
    /// The component catalogue - document 13 section 3.
    Gallery,
}

fn request() -> Request {
    let mut arguments = std::env::args().skip(1);
    let first = arguments.next();
    let (route, pack) = match first.as_deref() {
        Some("--gallery") => return Request::Gallery,
        Some("--clipboard") => (RouteRequest::Clipboard, arguments.next()),
        _ => (RouteRequest::Direct, first),
    };
    Request::Palette { pack, route }
}

/// Everything that can fail before there is a window to fail in.
fn start(request: Request) -> Result<(), slint::PlatformError> {
    match request {
        Request::Gallery => Gallery::new()?.run(),
        Request::Palette { pack, route } => run_palette(pack, route),
    }
}

/// The palette, with the shortcuts driving it.
///
/// # The shape of the two threads, and why the join is not optional
///
/// The main thread builds the window and then belongs to Slint. The worker
/// registers the shortcuts, runs the sequence and hands finished views back
/// (`live`). `run()` returns when the window closes. The flag then stops the
/// worker within one tick.
///
/// 🔴 The worker is JOINED, never detached. It owns the registered shortcuts and
/// releases them when its handle drops - a detached thread would leave
/// `Ctrl+Alt+N` and nine others taken from whoever wants them next, for as long
/// as the process lingers. Joining also means a send in flight finishes writing
/// rather than being cut in half inside somebody's field.
fn run_palette(pack: Option<String>, route: RouteRequest) -> Result<(), slint::PlatformError> {
    // 🔴 BEFORE the window exists. Afterwards the answer is the palette itself,
    // and handing the focus back to ourselves is a no-op that reports success -
    // the failure shape this project keeps meeting (`slint.md` 2.17).
    let kept = KeptFocus::remember();
    // Also before the window: whether it starts collapsed has to be known
    // before it is drawn (`D84`). A small file, read once - the saving happens
    // on the worker.
    let store = SettingsFile::for_this_user();
    let (settings, mut said) = KeptSettings::open(&store);
    // The shortcuts of this run, once, here: the hint bar on this thread and
    // the registration on the worker must show and take the same ones. What
    // the tester wrote and could not be used is said with the other settings.
    let (bindings, refused) = settings.bindings(default_bindings());
    said.extend(refused);
    let compact = live::starts_compact(settings.settings());
    let palette = Palette::new()?;
    palette.set_window_title(i18n::label(PaletteLabel::Title).into());
    // Before anything is shown, so the first height the content asks for
    // already sets the floor (`D83`).
    live::hold_height_on_change(&palette);
    // The clipboard bar's words come with each view, because two conditions
    // share the bar and say different things (`D72`) - see `live::View`.
    palette.set_hints(ModelRc::new(VecModel::from(hints(&bindings))));
    // The pack's name opens the pack window when clicked, and UI Automation
    // names that click with the words the hint bar gives the shortcut.
    palette.set_open_packs_label(i18n::action_name(HotkeyAction::OpenPacks).into());
    // Expanded at first run, with the hints up and nothing sent yet -
    // `ux-spec.md` 5.1 - and as the tester left it on every run after that.
    // The worker fills the pack and the counter, because the sequence that
    // knows them lives over there.
    palette.set_compact(compact);
    palette.set_has_value(false);
    palette.set_no_value(i18n::no_value_yet(bindings.chord(HotkeyAction::NextValue)).into());

    // The two threads meet here. `focus` runs on this one and may produce a
    // sentence saying the palette could not refuse the focus. The worker rebuilds
    // the message band on every view and has to find that sentence again.
    let standing = focus::standing();
    focus::refuse_focus(&palette, kept, &standing);

    let stop = Arc::new(AtomicBool::new(false));
    // What the pack window asks of the worker between presses - another pack.
    // The closing goes through `stop`, never through this: a channel whose
    // sender is gone answers "nothing waiting", which must not read as "stop".
    let (choose, commands) = std::sync::mpsc::channel::<live::Command>();
    // The pack in use, published by the worker, read by the pack window.
    let in_use = live::in_use();

    // Created once and shown on request. Unlike the palette it has no flag to
    // lose when hidden, so hiding it between openings costs nothing.
    let packs = Packs::new(
        PacksWindow::new()?,
        Box::new(SystemKeyboard::default()),
        choose,
        Arc::clone(&in_use),
        {
            let palette = palette.as_weak();
            Box::new(move |line| {
                if let Some(palette) = palette.upgrade() {
                    live::say_now(&palette, line);
                }
            })
        },
    );
    palette.on_open_packs({
        let packs = std::rc::Rc::clone(&packs);
        move || packs.open()
    });
    // Closing the palette closes the pack window too - otherwise the event
    // loop would wait for it, and the process would outlive the palette.
    palette.window().on_close_requested({
        let packs = std::rc::Rc::clone(&packs);
        move || {
            packs.close();
            slint::CloseRequestResponse::HideWindow
        }
    });
    let worker = std::thread::spawn({
        let palette = palette.as_weak();
        let stop = Arc::clone(&stop);
        let standing = std::sync::Arc::clone(&standing);
        let in_use = Arc::clone(&in_use);
        let start = live::Start {
            asked: pack,
            default_pack: DEFAULT_PACK,
            route,
            store,
            kept: settings,
            said,
            bindings,
        };
        // Measured: work handed to the event loop before `run()` is delivered
        // once it starts (`slint.md` 1.9), so this thread may say something
        // before the window is running and nothing is lost.
        move || live::drive(&palette, &stop, &commands, &in_use, start, &standing)
    });

    let ran = palette.run();
    stop.store(true, Ordering::Relaxed);
    // A worker that panicked has already lost its shortcuts to its own unwind,
    // and the window is closing either way. Nothing is swallowed that anyone
    // could act on: the workspace denies `unwrap`, `expect` and `panic` in
    // product code, so a panic here is a bug rather than a path.
    drop(worker.join());
    ran
}

/// The hint bar's rows, built from the bindings in effect rather than written
/// out - so a shortcut the tester set in the settings file is the one the bar
/// shows.
fn hints(bindings: &Bindings) -> Vec<HintRow> {
    bindings
        .as_slice()
        .iter()
        .filter(|(action, _)| HINTED.contains(action))
        .map(|(action, chord)| HintRow {
            key: i18n::chord(*chord).into(),
            action: i18n::action_name(*action).into(),
        })
        .collect()
}
