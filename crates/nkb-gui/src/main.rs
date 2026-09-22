//! The nkb palette.
//!
//! A full product in its own right, not a windowless CLI with a window bolted
//! on - D25. It shares `core` and `app` with `nkb` and shares the same catalogue
//! on disk, and neither executable can start the other.
//!
//! # What it does today, and what it does not
//!
//! It opens the palette on a pack and shows where the sequence stands. What it
//! does NOT do yet is listen: the global shortcuts, the worker thread and
//! `drive_sequence` are piece C5, and the pattern for that is already written
//! and measured in `crates/nkb-adapters/examples/drive_loop.rs`. So the window
//! is real and the data in it is real, and nothing yet moves it.
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
//! (`OBS-103`). Before this change that error reached a console; after it, with
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

use nkb_adapters::i18n::PaletteLabel;
use nkb_adapters::{BuiltInCatalogue, TomlPackFormat, i18n, report_window_failure};
use nkb_app::AdvanceSequence;
use nkb_core::hotkeys::{DEFAULT_BINDINGS, HotkeyAction};
use nkb_gui::{Gallery, HintRow, Palette};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

/// The pack the palette opens on when the command line names none.
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
/// ⚠️ `OpenPacks` is here although the pack search is step 7 and does not exist.
/// It stays because the shortcut IS registered - `nkb_core::hotkeys` reserves
/// all ten so another application cannot take them - and an unwired one answers
/// with `Message::Unhandled` rather than with silence. A tester who presses it
/// learns something true either way.
const HINTED: [HotkeyAction; 4] = [
    HotkeyAction::NextValue,
    HotkeyAction::PreviousValue,
    HotkeyAction::CopyReport,
    HotkeyAction::OpenPacks,
];

fn main() -> ExitCode {
    match start(&request()) {
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
/// Deliberately tiny: two shapes, no flags to combine, no parser. `nkb` is the
/// executable with a command surface and `ux-spec.md` 10 is its contract; this
/// one is a window, and a window that grows an option grammar has started to
/// become the other binary.
enum Request {
    /// The palette, on this pack.
    Palette(String),
    /// The component catalogue - document 13 section 3.
    Gallery,
}

fn request() -> Request {
    let mut arguments = std::env::args().skip(1);
    match arguments.next() {
        Some(first) if first == "--gallery" => Request::Gallery,
        Some(pack) => Request::Palette(pack),
        None => Request::Palette(DEFAULT_PACK.to_owned()),
    }
}

/// Everything that can fail before there is a window to fail in.
fn start(request: &Request) -> Result<(), slint::PlatformError> {
    match request {
        Request::Gallery => Gallery::new()?.run(),
        Request::Palette(pack) => palette(pack)?.run(),
    }
}

/// The palette, filled from a real pack.
///
/// 🔴 Every string handed to the window comes from `nkb_adapters::i18n`. Writing
/// one here would move the sentence out of the dictionary into the wiring, which
/// is the same violation as writing it into the `.slint` file and is caught by
/// the same guard - untouchable rule 9, D58.
fn palette(pack: &str) -> Result<Palette, slint::PlatformError> {
    let mut sequence = AdvanceSequence::new();
    let mut messages: Vec<SharedString> = Vec::new();

    if let Err(error) = sequence.choose_pack(&BuiltInCatalogue::new(), &TomlPackFormat, pack) {
        messages.push(i18n::choose_error(&error, pack).into());
    }

    let palette = Palette::new()?;
    palette.set_window_title(i18n::label(PaletteLabel::Title).into());
    palette.set_degraded_label(i18n::label(PaletteLabel::DirectInputRefused).into());

    if let Some(name) = sequence.pack_name() {
        palette.set_pack(name.into());
    }
    if let Some((done, total)) = sequence.counter() {
        palette.set_counter(i18n::counter(done, total).into());
    }

    palette.set_messages(ModelRc::new(VecModel::from(messages)));
    palette.set_hints(ModelRc::new(VecModel::from(hints())));

    // Awake at first run, with the hints up and nothing sent yet - `ux-spec.md`
    // 5.1. The resting state arrives with the loop that can leave it again.
    palette.set_showing(true);
    palette.set_has_value(false);

    Ok(palette)
}

/// The hint bar's rows, built from the bindings rather than written out.
fn hints() -> Vec<HintRow> {
    DEFAULT_BINDINGS
        .iter()
        .filter(|(action, _)| HINTED.contains(action))
        .map(|(action, chord)| HintRow {
            key: i18n::chord(*chord).into(),
            action: i18n::action_name(*action).into(),
        })
        .collect()
}
