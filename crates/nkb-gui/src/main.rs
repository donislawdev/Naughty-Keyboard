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

use std::cell::RefCell;
use std::process::ExitCode;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use nkb_adapters::i18n::PaletteLabel;
use nkb_adapters::{
    KeptFocus, SettingsFile, altgr_character, default_bindings, i18n, report_window_failure,
    screens,
};
use nkb_app::{KeptSettings, RouteRequest};
use nkb_core::hotkeys::HotkeyAction;
use nkb_core::screens::Point;
use nkb_gui::packs::{Packs, SystemKeyboard};
use nkb_gui::shortcuts::{Shortcuts, System};
use nkb_gui::welcome::Welcome;
use nkb_gui::{Gallery, PacksWindow, Palette, ShortcutsWindow, WelcomeWindow, focus, live};
use slint::ComponentHandle;

/// The pack the palette opens on when the command line names none and the
/// settings remember none.
///
/// One of the packs that ship inside the binary (D51), so the palette has
/// something real to show on a machine with no catalogue on disk at all.
const DEFAULT_PACK: &str = "whitespace";

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
/// palette runs is a click on the palette (`D99`), never another global
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
    // the tester wrote and could not be used is said with the other settings -
    // a `Ctrl+Alt` chord a keyboard layout types a character on included (K4d).
    let (bindings, refused) = settings.bindings(default_bindings(), &altgr_character);
    said.extend(refused);
    let compact = live::starts_compact(settings.settings());
    // Asked here, before the settings move to the worker (UX7, `D103`).
    let welcome_due = settings.welcome_due();
    let palette = Palette::new()?;
    // Where the tester left it on this layout of screens (UX7). Asked after the
    // window is built, because from then on the process sees physical pixels
    // (`nkb_adapters::screens`), and before it is shown, because the backend
    // keeps a place given to a window not yet created and creates it there
    // (i-slint-backend-winit 1.18.1, `set_position`).
    let (place, off_screen) = settings.position_on(screens::layout().as_ref());
    said.extend(off_screen);
    if let Some(at) = place {
        palette
            .window()
            .set_position(slint::PhysicalPosition::new(at.x, at.y));
    }
    palette.set_window_title(i18n::label(PaletteLabel::Title).into());
    // The clipboard bar's words come with each view, because two conditions
    // share the bar and say different things (`D72`) - see `live::View`. The
    // words naming the shortcuts come from the table of this run, by the same
    // function the worker uses when the table changes.
    live::show_legend(&palette, live::legend(&bindings));
    // The pack's name opens the pack window when clicked, and UI Automation
    // names that click with the words the hint bar gives the shortcut.
    palette.set_open_packs_label(i18n::action_name(HotkeyAction::OpenPacks).into());
    // The link under the hint bar opens the shortcuts window (K5.5).
    palette.set_shortcuts_link(i18n::label(PaletteLabel::ShortcutsLink).into());
    // The value band's heading: since UX-GUI-001 the band of the next value
    // stands above it, and two values on screen need telling apart.
    palette.set_last_sent_label(i18n::label(PaletteLabel::LastSent).into());
    // Folded at every start, opened by a click on the heading (point 7).
    palette.set_last_sent_action(i18n::label(PaletteLabel::LastSentAction).into());
    palette.on_toggle_last_sent({
        let palette = palette.as_weak();
        move || {
            if let Some(palette) = palette.upgrade() {
                live::set_last_sent_open(&palette, !palette.get_last_sent_open());
            }
        }
    });
    // The Copy buttons beside the next value and the last one sent (`D98`):
    // one word on both, and what each does in UI Automation's words.
    palette.set_copy_label(i18n::label(PaletteLabel::Copy).into());
    palette.set_copy_next_action(i18n::label(PaletteLabel::CopyNext).into());
    palette.set_copy_last_action(i18n::label(PaletteLabel::CopyLast).into());
    // The two switches under the hint bar (point 2), and the way out of
    // clipboard mode on the standing bar (`D99`).
    live::label_switches(&palette);
    live::label_arrows(&palette);
    palette.set_turn_off_label(i18n::label(PaletteLabel::TurnOff).into());
    palette.set_turn_off_action(i18n::label(PaletteLabel::TurnOffClipboard).into());
    // The button at the end of the pack band, in both states (`UX-GUI-004`).
    palette.set_collapse_label(i18n::label(PaletteLabel::Collapse).into());
    palette.set_expand_label(i18n::label(PaletteLabel::Expand).into());
    // Expanded at first run, with the hints up and nothing sent yet -
    // `ux-spec.md` 5.1 - and as the tester left it on every run after that.
    // The worker fills the pack and the counter, because the sequence that
    // knows them lives over there.
    palette.set_compact(compact);
    palette.set_has_value(false);
    // How a typed value meets the field, from the first frame (`D101`): the
    // worker starts from the same answer and says it again with its first view.
    live::show_clearing(&palette, live::starts_clearing(settings.settings()));

    // The two threads meet here. `focus` runs on this one and may produce a
    // sentence saying the palette could not refuse the focus. The worker rebuilds
    // the message band on every view and has to find that sentence again.
    let standing = focus::standing();

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
        choose.clone(),
        Arc::clone(&in_use),
        say_in(&palette),
    );
    palette.on_copy_next(copy_on_click(&palette, &choose, Palette::get_next_key));
    palette.on_copy_last(copy_on_click(&palette, &choose, Palette::get_last_key));
    // The arrows that walk the pack without typing (`D120`): the worker holds the
    // sequence, so the click only asks - the same way the shortcuts go.
    palette.on_back_one_value(ask_on_click(&choose, live::Command::Back));
    palette.on_skip_value(ask_on_click(&choose, live::Command::Skip));
    // Clipboard mode on and off (`D99`): the worker holds the sequence, so the
    // click only asks.
    palette.on_choose_route(ask_for_way(&choose, live::route_command));
    palette.on_turn_off_clipboard(ask_on_click(&choose, live::Command::TurnOffClipboard));
    // Collapse and expand: the worker owns the switch and remembers it (`D84`),
    // so the click asks, as the shortcut does.
    palette.on_toggle_compact(ask_on_click(&choose, live::Command::ToggleCompact));
    // The way of meeting the field (`D101`) - the worker holds the choice, and
    // its view brings back the way in effect.
    palette.on_choose_clearing(ask_for_way(&choose, live::clearing_command));
    // The same way: created once, shown on request - and the one window the
    // worker's answers about shortcuts are delivered to (`shortcuts::tell`).
    let shortcuts = Shortcuts::new(
        ShortcutsWindow::new()?,
        Box::new(SystemKeyboard::default()),
        Box::new(System),
        choose.clone(),
        say_in(&palette),
    );
    palette.on_open_shortcuts({
        let shortcuts = std::rc::Rc::clone(&shortcuts);
        move || shortcuts.open()
    });
    palette.on_open_packs({
        let packs = std::rc::Rc::clone(&packs);
        move || packs.open()
    });
    // Showing the palette, once: at start, or - on the first run - when the
    // welcome goes (the owner's point 1, `D109`). A palette that cannot be
    // shown ends the run with the error, as `run()` did before.
    let failed: Rc<RefCell<Option<slint::PlatformError>>> = Rc::default();
    let appear = {
        let palette = palette.as_weak();
        let standing = Arc::clone(&standing);
        let failed = Rc::clone(&failed);
        move || {
            let Some(palette) = palette.upgrade() else {
                return;
            };
            if let Err(error) = show_palette(&palette, kept, &standing) {
                *failed.borrow_mut() = Some(error);
                let _ = slint::quit_event_loop();
            }
        }
    };
    // The first run's window (UX7, `ux-spec.md` 5.1, `D105`), ALONE until it
    // goes: two windows at once, with nothing saying which is which, was the
    // owner's first complaint (`D109`).
    let welcome = if welcome_due {
        let welcome = Welcome::new(
            WelcomeWindow::new()?,
            Box::new(SystemKeyboard::default()),
            choose.clone(),
            &bindings,
            say_in(&palette),
            Box::new(appear),
        );
        welcome.open_on_start(Arc::clone(&in_use), std::time::Instant::now());
        Some(welcome)
    } else {
        appear();
        None
    };
    // Closing the palette closes the pack window too - otherwise the event
    // loop would wait for it, and the process would outlive the palette.
    // Where it stood is read here, while the window is still up (UX7).
    let closed_at = std::rc::Rc::new(std::cell::Cell::new(None));
    palette.window().on_close_requested({
        let packs = std::rc::Rc::clone(&packs);
        let shortcuts = std::rc::Rc::clone(&shortcuts);
        let closed_at = std::rc::Rc::clone(&closed_at);
        let palette = palette.as_weak();
        let welcome = welcome.clone();
        move || {
            if let Some(palette) = palette.upgrade() {
                let at = palette.window().position();
                closed_at.set(Some(Point { x: at.x, y: at.y }));
            }
            packs.close();
            shortcuts.close();
            if let Some(welcome) = &welcome {
                welcome.dismiss();
            }
            slint::CloseRequestResponse::HideWindow
        }
    });
    let remembers = store.clone();
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
            // To the shortcuts window, through the main thread's event loop.
            tell: nkb_gui::shortcuts::tell(),
        };
        // Measured: work handed to the event loop before `run()` is delivered
        // once it starts (`slint.md` 1.9), so this thread may say something
        // before the window is running and nothing is lost.
        move || live::drive(&palette, &stop, &commands, &in_use, start, &standing)
    });

    // The loop rather than `palette.run()`, which would show the palette at
    // once: on the first run the welcome stands alone until it goes.
    let ran = slint::run_event_loop().and_then(|()| failed.take().map_or(Ok(()), Err));
    stop.store(true, Ordering::Relaxed);
    // A worker that panicked has already lost its shortcuts to its own unwind,
    // and the window is closing either way. Nothing is swallowed that anyone
    // could act on: the workspace denies `unwrap`, `expect` and `panic` in
    // product code, so a panic here is a bug rather than a path.
    drop(worker.join());
    // The place is remembered here, with the worker joined: the worker reads
    // the stop flag before any command, so one sent on closing would be lost,
    // and two writers at once is the race `W4` names. On the layout of THIS
    // moment - a screen unplugged while the palette ran changed it. Read again,
    // so a change the worker saved in this run is not written back over, and
    // a file that cannot be used is still left alone (`D84`). A failure has
    // nobody left to hear it: the window is gone.
    if let (Some(at), Some(layout)) = (closed_at.take(), screens::layout()) {
        let (mut kept, _) = KeptSettings::open(&remembers);
        let _ = kept.keep_position(&remembers, &layout, at);
    }
    ran
}

/// Shows the palette and makes it refuse the keyboard - the two belong
/// together, because the handle `focus` waits for exists only once the window
/// is shown, and a palette that took the keyboard would take it from the field
/// under test (`D62`). Called once per run, never after a hide (`OBS-80`).
fn show_palette(
    palette: &Palette,
    kept: KeptFocus,
    standing: &focus::Standing,
) -> Result<(), slint::PlatformError> {
    palette.show()?;
    focus::refuse_focus(palette, kept, standing, || {});
    Ok(())
}

/// What a click on a Copy button does: reads the key the palette holds beside
/// that button's band and asks the worker, which holds the pack and the
/// clipboard (`D98`).
fn copy_on_click(
    palette: &Palette,
    asks: &std::sync::mpsc::Sender<live::Command>,
    key: fn(&Palette) -> nkb_gui::ValueKey,
) -> impl Fn() + 'static {
    let palette = palette.as_weak();
    let asks = asks.clone();
    move || {
        if let Some(command) = palette
            .upgrade()
            .and_then(|palette| live::copy_command(&key(&palette)))
        {
            // A send that fails is a palette already closing - nobody is left
            // to tell.
            let _ = asks.send(command);
        }
    }
}

/// What a click on a button that needs nothing from the palette does: asks the
/// worker for `command`.
fn ask_on_click(
    asks: &std::sync::mpsc::Sender<live::Command>,
    command: live::Command,
) -> impl Fn() + 'static {
    let asks = asks.clone();
    // A send that fails is a palette already closing - nobody is left to tell.
    move || drop(asks.send(command.clone()))
}

/// What a click on way `index` of a switch does: asks the worker for what
/// `meaning` says that way means. An index the switch does not show asks for
/// nothing.
fn ask_for_way(
    asks: &std::sync::mpsc::Sender<live::Command>,
    meaning: fn(i32) -> Option<live::Command>,
) -> impl Fn(i32) + 'static {
    let asks = asks.clone();
    // A send that fails is a palette already closing - nobody is left to tell.
    move |index| {
        if let Some(command) = meaning(index) {
            let _ = asks.send(command);
        }
    }
}

/// A line into the palette's message band, for a window that has something to
/// say after it closed - the pack window and the shortcuts window alike.
fn say_in(palette: &Palette) -> Box<dyn Fn(String)> {
    let palette = palette.as_weak();
    Box::new(move |line| {
        if let Some(palette) = palette.upgrade() {
            live::say_now(&palette, line);
        }
    })
}
