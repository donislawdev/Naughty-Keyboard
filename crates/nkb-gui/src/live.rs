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
//! # Choosing another pack while the palette runs
//!
//! The pack window runs on the main thread and the sequence lives here, so a
//! choice crosses as a [`Command`] on a channel. It is carried out BETWEEN
//! presses: the loop's `keep_going` answers no when a command waits,
//! `drive_sequence` returns, the pack is chosen and the loop is entered again
//! with the same shortcuts still registered. `keep_going` is asked only
//! between turns - never between a send and the drain after it - so a choice
//! made during a send waits for its end, and `W1` holds.
//!
//! ⚠️ A command is noticed at the end of the current wait, up to one `TICK`
//! later. A press that arrives after the command within that tick is still
//! answered with the pack it was pressed in, and the palette shows that value
//! under that pack's name before it shows the new one - late, never false.
//!
//! # The shortcuts window: pause, change, resume (K5.3, `D90`)
//!
//! While the shortcuts window is open the palette's shortcuts are given back
//! to the system (`ux-spec.md` 5.4): a registered shortcut reaches no window at
//! all, that one included, and pressed there it would send a value into it.
//! Three more commands carry that, on the same channel as a pack choice and
//! for the same reason - between presses, never inside a send:
//!
//! - [`Command::Pause`] releases them and answers the window with the table it
//!   edits ([`Told::Paused`]). The window gets the table in the answer rather
//!   than from a shared slot, so a window closed and opened again at once can
//!   never read the table from before its own last change.
//! - [`Command::Shortcut`] is one whole change, carried out HERE because the
//!   settings of the run live here (`D84`: one [`KeptSettings`] per run) and a
//!   save may take seconds: consider, a trial registration, save
//!   ([`KeptSettings::change_shortcut`]).
//! - [`Command::Resume`] takes the shortcuts again, computing the table from the
//!   settings with the same function the start uses - never from a table
//!   carried in the command - so what is registered is what the next start
//!   would register. The hint bar and the empty value band follow it.
//!
//! While nothing is held this thread waits on the channel instead of on the
//! shortcuts. That is also where it goes when the shortcuts could not be
//! registered or their thread ended: a pack can still be chosen, and the next
//! `Resume` tries again. Until K5.3 the worker simply ended there, and a choice
//! made in the pack window afterwards went nowhere, in silence.
//!
//! # What this module deliberately does not do
//!
//! It does not make the palette refuse the keyboard focus. That runs on the main
//! thread, because only the main thread may touch the window, and it lives in
//! `focus`. What crosses between them is [`crate::focus::Standing`]: a sentence
//! saying the promise could not be kept, which this module reads again on every
//! view because it rebuilds the message band from scratch each time.

use std::cell::{Cell, RefCell};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use nkb_adapters::i18n::PaletteLabel;
use nkb_adapters::{
    BuiltInCatalogue, ClipboardDelivery, DirectInjection, EnglishReport, GlobalShortcuts,
    SettingsFile, TomlPackFormat, altgr_character, default_bindings, i18n,
};
use nkb_app::advance_sequence::{InFlight, Ports, RouteRequest, Sent};
use nkb_app::browse_packs::{Direction, list_packs};
use nkb_app::ports::{
    Clearing, HotkeyRegistrar, LiveShortcuts, Progress, SettingChange, Settings, SettingsStore,
    ShortcutRegistration, Wait,
};
use nkb_app::{
    AdvanceSequence, Ended, KeptSettings, Message, Opening, Outcome, SettingsMessage,
    ShortcutChange, UpcomingValue, ValueFacts, ValueKey, drive_sequence, note_used,
};
use nkb_core::hotkeys::{Bindings, HotkeyAction, HotkeyChord};
use nkb_core::preview::ValuePreview;
use nkb_core::report::Arrival;
use nkb_core::sequence::Delivery;
use nkb_core::typeface::outside_guarantee;
use slint::{ModelRc, SharedString, VecModel, Weak};

use crate::clipboard::SystemClipboard;
use crate::focus::{Standing, standing_line};
use crate::typeface::SHIPPED;
// The view's own copy of a key: the same two identifiers, as Slint holds them.
use crate::{HintRow, Marker, Palette, ValueKey as ShownKey};

mod copy;
mod presses;
mod recent;
mod view;

use copy::show_copied;
pub use copy::{CopyButton, copy_command, label_copies};
use presses::{PaletteShortcuts, between_presses};
use recent::{after_press, carry_out_noting, keep_recent};
pub(crate) use view::preview_line;
use view::*;
pub use view::{
    View, clearing_command, label_arrows, label_switches, route_command, set_compact,
    set_last_sent_open, show_clearing, starts_clearing, wire_folds,
};

/// How long one wait for a press lasts before the stop flag is read again.
///
/// It bounds how long the window can be closed before this thread notices, and
/// nothing else: a press arriving mid-tick is delivered immediately, because the
/// wait returns on the press rather than on the timeout.
const TICK: Duration = Duration::from_millis(100);

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
    /// The shortcuts of this run: the defaults with the tester's own on top
    /// (`KeptSettings::bindings`). Read once, on the main thread, because the
    /// hint bar there shows the same ones the worker registers.
    pub bindings: Bindings,
    /// How the shortcuts window hears the answers to its commands. Called on
    /// THIS thread, so the main thread hands the answer on through its event
    /// loop - the worker never touches a window.
    pub tell: Tell,
}

/// What the main thread asks the worker to do between presses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Open this pack, by identifier, in place of the one in use.
    Choose(String),
    /// Make this value the one the next press sends, opening its pack first
    /// when that is another one - the value window (UX2). By identifiers: the
    /// value is looked up in the pack the worker holds.
    ChooseValue { pack: String, value: String },
    /// Start `pack` again from `value`, its first value - the restart row of
    /// the value window. Carried out as [`Command::ChooseValue`] is, but the
    /// value does not become recent: the tester chose a place in the pack,
    /// not that value (`D106`).
    Restart { pack: String, value: String },
    /// Give the palette's shortcuts back to the system - the shortcuts window
    /// is opening, and a press there must reach it rather than send a value.
    /// Answered with [`Told::Paused`].
    Pause,
    /// Take the shortcuts again, as the settings now give them - the shortcuts
    /// window closed. The table is computed here, never carried (`D90`).
    Resume,
    /// Give `action` this chord, or its default back for `None`. Answered with
    /// [`Told::Shortcut`].
    Shortcut {
        action: HotkeyAction,
        chord: Option<HotkeyChord>,
    },
    /// Put this value on the clipboard for the tester to paste - a Copy button
    /// (`UX-GUI-003`, `D98`). By identifiers, looked up in the pack the worker
    /// holds, so the value copied is the one drawn where the tester clicked -
    /// and which button it was, which says "Copied" when it went (`D121`).
    Copy(ValueKey, CopyButton),
    /// Put the report block of the value sent last on the clipboard - the
    /// block's Copy (`D121`), the action the report shortcut is.
    CopyReport,
    /// Turn clipboard mode on - the route switch under the hint bar
    /// (`UX-GUI-010`, `D99`).
    UseClipboard,
    /// Turn clipboard mode off - the same switch, or the button on the
    /// standing clipboard bar.
    TurnOffClipboard,
    /// Collapse the palette, or expand it - the button at the end of the pack
    /// band (`UX-GUI-004`). What `ToggleVisibility` does, through the same
    /// switch, so a click and a press cannot disagree about the state.
    ToggleCompact,
    /// Type values this way from the next press: the line cleared first, or
    /// the value at the cursor - the clearing switch under the hint bar
    /// (`UX-GUI-005`, `D101`). The way itself rather than "the other one",
    /// which the button before the switch sent: two quick clicks on one half
    /// of a switch that has not redrawn yet then ask twice for the same way,
    /// where two toggles would have undone each other.
    SetClearing(Clearing),
    /// Move the next value one forward without typing - the arrow beside it
    /// (`D120`). What `SkipValue` does, through the same sequence.
    Skip,
    /// Move it one back without typing - the other arrow.
    Back,
    /// Open the pack one step along the value window's list, from its first
    /// value - `NextPack` and `PreviousPack` (`D120`), taken in front of the
    /// sequence and carried out here, where the pack in use and the settings
    /// that remember it live.
    Pack(Direction),
    /// The tester closed the welcome window - remember it, so it does not open
    /// again (`[welcome] done`, UX7, `D103`, `D105`), and start the pack over
    /// from its first value (`D109`). Here, because the worker owns the
    /// settings and the sequence for the run.
    WelcomeDone,
}

/// What the worker tells the shortcuts window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Told {
    /// The palette's shortcuts are given back, and this is the table the
    /// window edits, as it stands now.
    Paused(ShortcutsNow),
    /// What came of one [`Command::Shortcut`], and a line when saving it
    /// failed for a reason not said before.
    Shortcut {
        action: HotkeyAction,
        change: ShortcutChange,
        not_saved: Option<String>,
    },
}

/// How the shortcuts window hears what the worker has to tell it.
pub type Tell = Box<dyn Fn(Told) + Send>;

/// The shortcuts in effect, and how each fared the last time the palette
/// registered them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShortcutsNow {
    /// The table in effect - what the palette registers when the window closes.
    pub bindings: Bindings,
    /// Each action with the chord registered for it last time and what the
    /// system said. Empty when nothing could be registered. A chord here that
    /// differs from `bindings` has not been registered since it changed, so
    /// nothing is known about it yet.
    pub registered: Vec<(HotkeyAction, HotkeyChord, ShortcutRegistration)>,
}

/// The palette's own actions: about windows rather than the pack in use, so
/// [`PaletteShortcuts`] takes them before the sequence sees them.
///
/// `OpenPacks` opens the value window (step 7, K3.2c) through the palette's
/// `open-packs` callback, and `ToggleVisibility` collapses or expands the
/// palette (`D83`). `NextPack` and `PreviousPack` open another pack the way
/// the value window does (`D120`), so the settings remember it - a choice of
/// the palette's, not a step of the sequence.
const PALETTE_OWN: [HotkeyAction; 4] = [
    HotkeyAction::OpenPacks,
    HotkeyAction::ToggleVisibility,
    HotkeyAction::NextPack,
    HotkeyAction::PreviousPack,
];

/// Whether a press of `action` does something in this build: the sequence
/// carries it out, or the palette does.
///
/// The hint bar names exactly these (`UX-GUI-007`). Until UX4 it named four
/// chosen ones, and two that work - restarting the pack and collapsing the
/// palette - were found only by reading the shortcuts window or by accident.
/// The four that do nothing yet stay registered and stay off the bar.
#[must_use]
pub fn wired(action: HotkeyAction) -> bool {
    AdvanceSequence::handles(action) || PALETTE_OWN.contains(&action)
}

/// The words on the palette that name a shortcut: the hint bar, and the value
/// band before anything is sent. Built from the table in effect rather than
/// written out, so a shortcut the tester changed is the one they show - at
/// start, and again whenever the table changes (K5.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Legend {
    /// The key and the action of each hinted shortcut, in the table's order.
    hints: Vec<(String, String)>,
    no_value: String,
}

/// The words naming the shortcuts of `bindings`. One function for the start
/// and for every change, so the two cannot word it differently.
#[must_use]
pub fn legend(bindings: &Bindings) -> Legend {
    Legend {
        // In the order a person reads them, not the table's (`D120`).
        hints: HotkeyAction::READING_ORDER
            .iter()
            .filter(|action| wired(**action))
            .map(|action| {
                (
                    i18n::chord(bindings.chord(*action)),
                    i18n::action_name(*action).to_owned(),
                )
            })
            .collect(),
        no_value: i18n::no_value_yet(bindings.chord(HotkeyAction::NextValue)),
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
}

/// The pack in use and the value its next press sends, by identifier,
/// published by the worker for the value window, which marks both and opens on
/// the second (UX2).
///
/// A shared slot rather than a message, for the reason [`Standing`] gives: the
/// window reads it whenever it opens, which may be long after the last change.
/// The worker is the one writer, because the sequence that knows the answer
/// lives there.
pub type InUse = Arc<Mutex<InUseNow>>;

/// What the slot holds.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InUseNow {
    pub pack: Option<String>,
    /// `None` when the next press sends no value: the end of a pack, or no pack.
    pub next: Option<String>,
    /// The restart shortcut of the table in effect, which the window's restart
    /// row names at its end (`UX-GUI-007`). Here because the table lives with
    /// the worker and changes when the shortcuts window closes.
    pub restart: Option<HotkeyChord>,
    /// The values chosen last in the value window, from any pack, the most
    /// recent first (`UX-GUI-016`, `D106`) - the list itself, not a copy:
    /// the worker keeps it here, for the window to read when it opens, and
    /// saves it from here when the palette closes.
    pub recent: Vec<ValueKey>,
}

/// A fresh slot, empty until the worker opens a pack.
#[must_use]
pub fn in_use() -> InUse {
    Arc::new(Mutex::new(InUseNow::default()))
}

/// The pack in use and the next value, as last published. A poisoned lock
/// answers nothing - the window then marks no row, which is a smaller failure
/// than a crash.
#[must_use]
pub fn in_use_now(in_use: &InUse) -> InUseNow {
    in_use.lock().map(|held| held.clone()).unwrap_or_default()
}

fn publish(in_use: &InUse, sequence: &AdvanceSequence, bindings: &Bindings) {
    if let Ok(mut held) = in_use.lock() {
        held.pack = sequence.pack_id().map(str::to_owned);
        held.next = next_id(sequence.upcoming().as_ref());
        held.restart = Some(bindings.chord(HotkeyAction::RestartPack));
    }
}

/// After a press: the pack is the same, the next value may have moved.
fn publish_next(in_use: &InUse, upcoming: Option<&UpcomingValue>) {
    if let Ok(mut held) = in_use.lock() {
        held.next = next_id(upcoming);
    }
}

fn next_id(upcoming: Option<&UpcomingValue>) -> Option<String> {
    match upcoming {
        Some(UpcomingValue::Value { key, .. }) => Some(key.value.clone()),
        Some(UpcomingValue::EndOfPack { .. }) | None => None,
    }
}

/// Whether the palette starts collapsed. One function for both threads, so
/// the window and the worker cannot start from two different answers.
#[must_use]
pub fn starts_compact(settings: &Settings) -> bool {
    settings.compact.unwrap_or(false)
}

/// Runs until the flag is set or the shortcuts die, carrying out whatever
/// `commands` brings between presses.
///
/// Takes the pack by name rather than a loaded sequence, because the sequence
/// must live on this thread: it is the one gate over the sequence state
/// (`architektura.md` 6a) and it never crosses back.
pub fn drive(
    palette: &Weak<Palette>,
    stop: &Arc<AtomicBool>,
    commands: &Receiver<Command>,
    in_use: &InUse,
    start: Start,
    standing: &Standing,
) {
    let Start {
        asked,
        default_pack,
        route,
        store,
        kept,
        said,
        bindings,
        tell,
    } = start;
    // Where the settings live, for the sentences that send the tester there.
    let file = store
        .path()
        .map(|path| path.display().to_string())
        .unwrap_or_default();
    let mut sequence = AdvanceSequence::new();
    let mut opening: Vec<String> = said
        .iter()
        .filter_map(|message| i18n::settings_message(message, &file, &bindings))
        .collect();
    // Created here, on the thread that uses it, and dropped with it - which is
    // when a Linux clipboard stops serving what it holds (`clipboard` says why).
    // Nothing connects until the first write, so a palette that never copies
    // never touches the clipboard at all.
    let clipboard = SystemClipboard::new();
    let by_clipboard = ClipboardDelivery::new(&clipboard);
    // `OBS-160`: a send that runs past the route's first report is drawn while
    // it goes. Called on THIS thread between two keys, so it only hands a
    // finished view to the main thread and returns.
    let on_the_way = |in_flight: InFlight| {
        let view = in_flight_view(&in_flight, standing);
        let _ = palette.upgrade_in_event_loop(move |palette| apply_in_flight(&palette, view));
    };
    let ports = Ports {
        direct: &DirectInjection,
        by_clipboard: &by_clipboard,
        keys: &DirectInjection,
        inspector: &DirectInjection,
        clipboard: &clipboard,
        report_text: &EnglishReport,
        progress: &on_the_way,
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
            Opening::Settings(message) => {
                opening.extend(i18n::settings_message(message, &file, &bindings));
            }
        }
    }
    // Before the first view, which says how values meet the field (`D101`).
    sequence.set_clearing(starts_clearing(kept.settings()));
    publish(in_use, &sequence, &bindings);
    if let Ok(mut held) = in_use.lock() {
        held.recent.clone_from(&kept.settings().recent);
    }

    let memory = Memory {
        kept: RefCell::new(kept),
        store: &store,
        file,
        bindings: Cell::new(bindings),
    };
    let mut worker = Worker {
        sequence,
        // The pack the messages are about - the one opened, or the last one
        // tried. It changes when the tester chooses another one that opens.
        pack: opened.pack,
        memory: &memory,
        hold: Hold::new(&GlobalShortcuts),
        ports: &ports,
        standing,
        defaults: default_bindings(),
        types: &altgr_character,
        route: Some(route),
        collapse: Collapse::new(&memory),
    };
    // With nothing registered the palette says why and stays up, and the
    // worker keeps serving the pack window. Untouchable rule 1: a run that did
    // less than it promised says so, rather than looking alive.
    opening.extend(worker.take(bindings));
    // No legend: the main thread drew it from the same table before the
    // window was shown.
    show(palette, worker.view(opening, ValueBand::Keep, None));

    let on_open = || open_packs_later(palette);
    let pending = Cell::new(None);
    loop {
        let next = match worker.hold.live() {
            Some(live) => {
                // The worker's own switch, the one a click reaches as a
                // command between presses - built here because the loop below
                // borrows the sequence beside it.
                let collapse = &worker.collapse;
                let on_toggle = || draw_toggle(palette, collapse.toggle());
                // A press for another pack waits as a command does, and the
                // loop below carries it out as soon as the wait ends.
                let on_pack = |direction| pending.set(Some(Command::Pack(direction)));
                let shortcuts = PaletteShortcuts {
                    inner: live,
                    on_toggle: &on_toggle,
                    on_open: &on_open,
                    on_pack: &on_pack,
                };
                // Captured on every way in, because `drive_sequence` borrows
                // the sequence for the whole loop and the closure that builds
                // each view cannot reach it - and the pack or the table may
                // have changed since the last one.
                let pack_shown = shown(&worker.sequence, &worker.pack);
                let pack = &worker.pack;
                let bindings = memory.bindings.get();
                let mut keep_going = || between_presses(stop, commands, &pending);
                let mut present = |outcome: Outcome| {
                    // The value window opens on the next value (UX2), and a
                    // press moves it.
                    after_press(in_use, &outcome);
                    show(
                        palette,
                        view_of(&outcome, &pack_shown, pack, standing, &bindings),
                    );
                };
                let ended = drive_sequence(
                    &shortcuts,
                    &mut worker.sequence,
                    &ports,
                    TICK,
                    &mut keep_going,
                    &mut present,
                );
                // A command waits only when `keep_going` took one, which ends
                // the loop as `Stopped` - so the window is still up.
                match (pending.take(), ended) {
                    (Some(command), _) => Next::Command(command),
                    (None, Ended::Stopped) => Next::Stop,
                    (None, Ended::ShortcutsGone) => Next::Gone,
                }
            }
            None => next_command(stop, commands),
        };
        match next {
            Next::Stop => break,
            Next::Gone => {
                worker.hold.lose();
                // `ShortcutsGone` always has a sentence, and the window is up:
                // the stop flag was not set, or this would be `Stop`.
                let said = i18n::ended(Ended::ShortcutsGone).into_iter().collect();
                show(palette, worker.view(said, ValueBand::Keep, None));
            }
            Next::Command(command) => {
                let carried = carry_out_noting(&mut worker, in_use, command);
                if let Some(view) = carried.view {
                    show(palette, view);
                }
                if let Some(toggled) = carried.toggled {
                    draw_toggle(palette, toggled);
                }
                if let Some(told) = carried.told {
                    tell(told);
                }
                // With the table in effect: a command may have changed it.
                publish(in_use, &worker.sequence, &memory.bindings.get());
            }
        }
    }
    // Before anything else: the shortcuts go back to the system. Holding them
    // after the palette is gone would take `Alt+Shift+N` away from whoever
    // wants it next.
    drop(worker);
    keep_recent(&memory, in_use);
}

/// What the loop in [`drive`] does next.
enum Next {
    /// The window is closing.
    Stop,
    /// The thread behind the shortcuts ended, so no press can arrive.
    Gone,
    /// Something the main thread asked for.
    Command(Command),
}

/// Waits for a command while no shortcuts are held - nothing else can happen
/// then. The stop is read first, as in [`between_presses`].
fn next_command(stop: &AtomicBool, commands: &Receiver<Command>) -> Next {
    loop {
        if stop.load(Ordering::Relaxed) {
            return Next::Stop;
        }
        match commands.recv_timeout(TICK) {
            Ok(command) => return Next::Command(command),
            Err(RecvTimeoutError::Timeout) => {}
            // Nobody left to send, but the window is still up: the stop flag,
            // not this channel, says when it closes. Slept rather than asked
            // again at once, because a closed channel answers at once and the
            // wait would spin.
            Err(RecvTimeoutError::Disconnected) => std::thread::sleep(TICK),
        }
    }
}

/// What carrying out one command came to: what the palette draws now and
/// what the shortcuts window is told. Either may be nothing.
struct Carried {
    view: Option<View>,
    told: Option<Told>,
    /// The compact state to draw after a click on the collapse button, and a
    /// line when saving it failed for a reason not said before - what
    /// [`Collapse::toggle`] answers, handed on as the shortcut hands it on.
    toggled: Option<(bool, Option<String>)>,
}

/// Everything a command changes, owned by this thread.
///
/// The ports come in as references, so a test builds one with a registrar
/// that answers what the test dictates and a store that keeps every save.
struct Worker<'a> {
    sequence: AdvanceSequence,
    /// The pack the messages are about - see [`drive`].
    pack: String,
    memory: &'a Memory<'a>,
    hold: Hold<'a>,
    ports: &'a Ports<'a>,
    standing: &'a Standing,
    /// The default table of this system - what a restore gives back.
    defaults: Bindings,
    /// Which character a `Ctrl+Alt` chord types as `AltGr` (`D88`).
    types: &'a dyn Fn(HotkeyChord) -> Option<char>,
    /// How the palette was asked to deliver, until the first time the
    /// shortcuts are held. Only then: a sentence telling the tester to paste
    /// each value would promise a flow that a palette with no shortcuts does
    /// not have (`D71`).
    route: Option<RouteRequest>,
    /// Whether the palette is compact. Here, beside the commands, because two
    /// ways reach it - the shortcut, taken in front of the sequence, and the
    /// button in the pack band, which arrives as a command (`UX-GUI-004`) -
    /// and one switch is what keeps them from disagreeing.
    collapse: Collapse<'a>,
}

impl Worker<'_> {
    /// A step through the pack without typing, and what the palette says of it.
    fn step(&mut self, action: HotkeyAction) -> Carried {
        let outcome = self.sequence.on_action(action, self.ports);
        self.saying(&outcome.messages)
    }

    /// The pack one step along the value window's list, opened as a choice
    /// there opens it - from its first value, and remembered (`D120`). With no
    /// other pack that opens, the palette says so rather than doing nothing.
    fn step_pack(&mut self, direction: Direction) -> Carried {
        let listing = list_packs(&BuiltInCatalogue::new(), &TomlPackFormat).ok();
        let other = listing
            .as_ref()
            .and_then(|listing| listing.neighbour(&self.pack, direction))
            .map(str::to_owned);
        let view = match other {
            Some(pack) => choose(
                &mut self.sequence,
                self.memory,
                &mut self.pack,
                &pack,
                self.standing,
                self.hold.paused(),
            ),
            None => Some(self.view(
                vec![i18n::label(PaletteLabel::NoOtherPack).to_owned()],
                ValueBand::Keep,
                None,
            )),
        };
        Carried {
            view,
            told: None,
            toggled: None,
        }
    }

    fn carry_out(&mut self, command: Command) -> Carried {
        match command {
            Command::Choose(asked) => Carried {
                view: choose(
                    &mut self.sequence,
                    self.memory,
                    &mut self.pack,
                    &asked,
                    self.standing,
                    self.hold.paused(),
                ),
                told: None,
                toggled: None,
            },
            Command::ChooseValue { pack, value } | Command::Restart { pack, value } => Carried {
                view: Some(choose_value(
                    &mut self.sequence,
                    self.memory,
                    &mut self.pack,
                    &pack,
                    &value,
                    self.standing,
                    self.hold.paused(),
                )),
                told: None,
                toggled: None,
            },
            Command::Pause => {
                self.hold.pause();
                Carried {
                    view: Some(self.view(Vec::new(), ValueBand::Keep, None)),
                    told: Some(Told::Paused(ShortcutsNow {
                        bindings: self.in_effect().0,
                        registered: self.hold.registered.clone(),
                    })),
                    toggled: None,
                }
            }
            // Held already: the window did not pause, or a second close came.
            // Taking them again would say every line about them twice.
            Command::Resume if self.hold.live().is_some() => Carried {
                view: None,
                told: None,
                toggled: None,
            },
            Command::Resume => Carried {
                view: Some(self.resume()),
                told: None,
                toggled: None,
            },
            // The arrows beside the next value (`D120`): through the sequence,
            // as the shortcuts go, so a click and a press cannot walk the pack
            // two ways. Nothing is sent, so the value band keeps what it shows.
            Command::Skip => self.step(HotkeyAction::SkipValue),
            Command::Back => self.step(HotkeyAction::BackOneValue),
            Command::Pack(direction) => self.step_pack(direction),
            // Here, on the thread that holds the pack and the clipboard: the
            // value is built from the pack in memory (`W5`), and on Linux the
            // process that set the clipboard is the one serving it.
            Command::Copy(key, button) => self.copy(&key, button),
            Command::CopyReport => self.copy_report(),
            // The tester's clicks (`D99`), between presses like every command,
            // so the route never changes under a value in flight. On is the
            // request `--clipboard` makes at start, with the same warning that
            // the clipboard is replaced.
            //
            // 🔴 And under the same rule as that request (`D71`): while nothing
            // drives the palette, a sentence telling the tester to paste each
            // value would promise a flow there is none of. So the click waits
            // where `--clipboard` waits, and `take` keeps it the moment the
            // shortcuts are held. Paused is not that: the window that paused
            // them gives them back when it closes.
            Command::UseClipboard if self.hold.live().is_none() && !self.hold.paused() => {
                self.route = Some(RouteRequest::Clipboard);
                Carried {
                    view: None,
                    told: None,
                    toggled: None,
                }
            }
            Command::UseClipboard => {
                let said = self
                    .sequence
                    .choose_route(RouteRequest::Clipboard, self.ports);
                self.saying(&said)
            }
            Command::TurnOffClipboard => {
                let said = self.sequence.leave_clipboard(self.ports);
                self.saying(&said)
            }
            // The window's state, not the sequence's: nothing is redrawn but
            // the bands the state shows, and the message band keeps what it
            // says - as after the shortcut.
            Command::ToggleCompact => Carried {
                view: None,
                told: None,
                toggled: Some(self.collapse.toggle()),
            },
            // `D101`: the way asked for, in effect from the next press and kept
            // for the next run. Between presses like every command, so a value
            // in flight keeps the way it started with. The band is rebuilt as
            // the route switch rebuilds it, with the line about saving when the
            // save failed for a reason not said before. Asked for the way in
            // effect already, it changes nothing and saves nothing.
            Command::SetClearing(clearing) if clearing == self.sequence.clearing() => Carried {
                view: None,
                told: None,
                toggled: None,
            },
            Command::SetClearing(clearing) => {
                self.sequence.set_clearing(clearing);
                let lines = self
                    .memory
                    .keep(SettingChange::Clearing(clearing))
                    .into_iter()
                    .collect();
                Carried {
                    view: Some(self.view(lines, ValueBand::Keep, None)),
                    told: None,
                    toggled: None,
                }
            }
            // Testing starts at the first value (`D109`): what the tester sent
            // into the box to try it is not where the real field starts. The
            // palette shows it as it appears, so the view is drawn. A save that
            // failed for a reason not said before is said, like every other
            // change.
            Command::WelcomeDone => {
                let moved = self.sequence.start_over();
                let lines: Vec<String> = self
                    .memory
                    .keep(SettingChange::WelcomeDone)
                    .into_iter()
                    .collect();
                Carried {
                    view: (moved || !lines.is_empty())
                        .then(|| self.view(lines, ValueBand::Keep, None)),
                    told: None,
                    toggled: None,
                }
            }
            Command::Shortcut { action, chord } => {
                let (change, said) = self.memory.kept.borrow_mut().change_shortcut(
                    self.memory.store,
                    self.hold.registrar,
                    self.defaults,
                    self.types,
                    action,
                    chord,
                );
                let not_saved = said.and_then(|message| {
                    i18n::settings_message(&message, &self.memory.file, &self.memory.bindings.get())
                });
                // The window pauses before it changes anything, so this is
                // never reached in the product. If it ever were, what is held
                // would no longer be the table in effect - so it is taken
                // again at once rather than left to disagree.
                let view = (matches!(change, ShortcutChange::Changed { .. })
                    && self.hold.live().is_some())
                .then(|| self.resume());
                Carried {
                    view,
                    told: Some(Told::Shortcut {
                        action,
                        change,
                        not_saved,
                    }),
                    toggled: None,
                }
            }
        }
    }

    /// The table the settings give now, and what cannot be used of them.
    fn in_effect(&self) -> (Bindings, Vec<SettingsMessage>) {
        self.memory
            .kept
            .borrow()
            .bindings(self.defaults, self.types)
    }

    /// Takes the shortcuts the settings give now, and the palette's view of
    /// it - with the words naming them, which may have changed.
    ///
    /// What the file wishes and cannot have is said again: it is still true,
    /// and the tester may just have looked at it in the window.
    fn resume(&mut self) -> View {
        let (bindings, refused) = self.in_effect();
        self.memory.bindings.set(bindings);
        let mut lines: Vec<String> = refused
            .iter()
            .filter_map(|message| i18n::settings_message(message, &self.memory.file, &bindings))
            .collect();
        lines.extend(self.take(bindings));
        self.view(lines, ValueBand::Keep, Some(legend(&bindings)))
    }

    /// Registers `bindings` in place of whatever is held, and what is worth
    /// saying about it - with the route asked for at start, the first time
    /// the shortcuts are held.
    fn take(&mut self, bindings: Bindings) -> Vec<String> {
        let mut lines = self.hold.take(&bindings);
        if self.hold.live().is_some()
            && let Some(route) = self.route.take()
        {
            // A system with no direct route starts in clipboard mode here,
            // rather than failing the first press.
            lines.extend(
                self.sequence
                    .choose_route(route, self.ports)
                    .iter()
                    .map(|message| i18n::message(message, &self.pack, &bindings)),
            );
        }
        lines
    }

    /// A view of the palette as it stands, saying what the sequence said.
    fn saying(&self, said: &[Message]) -> Carried {
        let bindings = self.memory.bindings.get();
        let lines = said
            .iter()
            .map(|message| i18n::message(message, &self.pack, &bindings))
            .collect();
        Carried {
            view: Some(self.view(lines, ValueBand::Keep, None)),
            told: None,
            toggled: None,
        }
    }

    /// A view made between presses, carrying the sentence about the pause
    /// while it lasts.
    fn view(&self, messages: Vec<String>, value: ValueBand, legend: Option<Legend>) -> View {
        View {
            legend,
            ..view_between(
                &self.sequence,
                &self.pack,
                messages,
                value,
                self.standing,
                self.hold.paused(),
            )
        }
    }
}

/// Whether the palette's shortcuts are held by this process now.
enum Holding {
    /// Registered, and driving the sequence.
    Held(Box<dyn LiveShortcuts + Send>),
    /// Given back to the system while the shortcuts window is open.
    Paused,
    /// Not held, and not because of the window: nothing could be registered,
    /// or the thread behind them ended. Said when it happened. A
    /// [`Command::Resume`] tries again.
    Lost,
}

/// The palette's shortcuts as this thread holds them, and the registrar that
/// takes them.
struct Hold<'a> {
    registrar: &'a dyn HotkeyRegistrar,
    holding: Holding,
    /// What the last registration said, chord by chord - see [`ShortcutsNow`].
    registered: Vec<(HotkeyAction, HotkeyChord, ShortcutRegistration)>,
}

impl<'a> Hold<'a> {
    const fn new(registrar: &'a dyn HotkeyRegistrar) -> Self {
        Self {
            registrar,
            holding: Holding::Lost,
            registered: Vec::new(),
        }
    }

    fn live(&self) -> Option<&dyn LiveShortcuts> {
        match &self.holding {
            Holding::Held(live) => Some(live.as_ref()),
            Holding::Paused | Holding::Lost => None,
        }
    }

    const fn paused(&self) -> bool {
        matches!(self.holding, Holding::Paused)
    }

    /// Gives the shortcuts back. Dropping the handle is the release, and it
    /// returns only once the system has let them go.
    fn pause(&mut self) {
        self.holding = Holding::Paused;
    }

    /// The thread behind the shortcuts has ended.
    fn lose(&mut self) {
        self.holding = Holding::Lost;
    }

    /// Registers `bindings` in place of whatever is held, and the lines worth
    /// saying about it.
    ///
    /// 🔴 What is held is released FIRST. The old set holds the very chords the
    /// new one asks for, so the other order would come back `Taken`, against
    /// ourselves, for every chord that did not move.
    ///
    /// A registration that worked says nothing - `i18n::registration` answers
    /// `None` for it, so the silence is the dictionary's decision and not this
    /// module's.
    fn take(&mut self, bindings: &Bindings) -> Vec<String> {
        self.holding = Holding::Lost;
        match self.registrar.register(bindings.as_slice()) {
            Ok(live) => {
                self.registered = live
                    .outcomes()
                    .iter()
                    .map(|(action, outcome)| (*action, bindings.chord(*action), *outcome))
                    .collect();
                self.holding = Holding::Held(live);
                self.registered
                    .iter()
                    .filter_map(|(action, chord, outcome)| {
                        i18n::registration(outcome, *action, *chord)
                    })
                    .collect()
            }
            Err(error) => {
                self.registered.clear();
                vec![i18n::shortcuts_unavailable(&error)]
            }
        }
    }
}

/// Opens `asked` in place of the pack in use, and the view that says what came
/// of it. `pack` follows the pack that opened.
///
/// `None` when `asked` is the pack in use. The pack window never asks for it -
/// Enter on that row just closes - but the window works from a snapshot, and
/// the pack in use is decided here: choosing it again must not send `7 / 34`
/// back to the start.
///
/// A pack that cannot be opened changes nothing - `choose_pack` leaves the pack
/// in use, its place and the last value as they were - and says why, naming
/// the pack that was asked for.
fn choose(
    sequence: &mut AdvanceSequence,
    memory: &Memory,
    pack: &mut String,
    asked: &str,
    standing: &Standing,
    paused: bool,
) -> Option<View> {
    let (said, value) = match open_pack(sequence, memory, pack, asked) {
        PackOpening::Same => return None,
        PackOpening::New(said) => (said, ValueBand::Clear),
        PackOpening::Failed(line) => (vec![line], ValueBand::Keep),
    };
    Some(view_between(sequence, pack, said, value, standing, paused))
}

/// Opens `asked` when it is another pack, then makes `value` the one the next
/// press sends - the value window (UX2). The view says what came of it.
///
/// A pack that cannot be opened stops here, and says why, exactly as a pack
/// row does. A value the pack in use does not hold changes nothing and is said:
/// the window offered it from its own reading of the catalogue, and the palette
/// sends from the pack it holds (`W5`).
fn choose_value(
    sequence: &mut AdvanceSequence,
    memory: &Memory,
    pack: &mut String,
    asked: &str,
    value: &str,
    standing: &Standing,
    paused: bool,
) -> View {
    let (mut said, band) = match open_pack(sequence, memory, pack, asked) {
        PackOpening::Same => (Vec::new(), ValueBand::Keep),
        PackOpening::New(said) => (said, ValueBand::Clear),
        PackOpening::Failed(line) => {
            return view_between(
                sequence,
                pack,
                vec![line],
                ValueBand::Keep,
                standing,
                paused,
            );
        }
    };
    if !sequence.choose_next(value) {
        said.push(i18n::value_gone(value, &shown(sequence, pack)));
    }
    view_between(sequence, pack, said, band, standing, paused)
}

/// What opening a pack came to.
enum PackOpening {
    /// It is the pack in use - the window works from a snapshot, and choosing
    /// it again must not send `7 / 34` back to the start.
    Same,
    /// Opened, with what saving it as the remembered pack had to say.
    New(Vec<String>),
    /// It could not be opened, and nothing changed - `choose_pack` leaves the
    /// pack in use, its place and the last value as they were. The line says
    /// why, naming the pack that was asked for.
    Failed(String),
}

/// Opens `asked` in place of the pack in use. One path for a pack row and a
/// value row of the window. `pack` follows the pack that opened.
fn open_pack(
    sequence: &mut AdvanceSequence,
    memory: &Memory,
    pack: &mut String,
    asked: &str,
) -> PackOpening {
    if sequence.pack_id() == Some(asked) {
        return PackOpening::Same;
    }
    match sequence.choose_pack(&BuiltInCatalogue::new(), &TomlPackFormat, asked) {
        Ok(()) => {
            asked.clone_into(pack);
            PackOpening::New(
                memory
                    .keep(SettingChange::Pack(asked.to_owned()))
                    .into_iter()
                    .collect(),
            )
        }
        Err(error) => PackOpening::Failed(i18n::choose_error(&error, asked)),
    }
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
    memory: &'a Memory<'a>,
}

impl<'a> Collapse<'a> {
    fn new(memory: &'a Memory<'a>) -> Self {
        Self {
            compact: Cell::new(starts_compact(memory.kept.borrow().settings())),
            memory,
        }
    }

    /// Flips the switch and remembers it. The new state, and a line to say
    /// when saving failed for a reason not said before.
    fn toggle(&self) -> (bool, Option<String>) {
        let compact = !self.compact.get();
        self.compact.set(compact);
        (compact, self.memory.keep(SettingChange::Compact(compact)))
    }
}

/// The settings of this run and where they are saved, shared by everything the
/// worker remembers: collapsing the palette and choosing a pack.
///
/// One [`KeptSettings`] for both, never two: each save rewrites one key of the
/// file (`D84`), but a failure is said once PER RUN, and two copies would say
/// it twice and disagree about what is in effect.
struct Memory<'a> {
    kept: RefCell<KeptSettings>,
    store: &'a dyn SettingsStore,
    /// Where the settings live, for a sentence saying they were not saved.
    file: String,
    /// The shortcuts registered last, for the sentences that name one. A
    /// cell, because the shortcuts window changes them while the palette runs
    /// (K5.3).
    bindings: Cell<Bindings>,
}

impl Memory<'_> {
    /// Remembers one change, in effect at once and saved when it can be. A
    /// line to say when saving failed for a reason not said before.
    fn keep(&self, change: SettingChange) -> Option<String> {
        self.kept
            .borrow_mut()
            .keep(self.store, change)
            .and_then(|message| i18n::settings_message(&message, &self.file, &self.bindings.get()))
    }
}

/// Draws what a collapse came to - the press and the click alike - and says
/// the line about saving it, when there is one.
fn draw_toggle(palette: &Weak<Palette>, (compact, line): (bool, Option<String>)) {
    set_compact_later(palette, compact);
    if let Some(line) = line {
        say_later(palette, line);
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
    let _ = palette.upgrade_in_event_loop(move |palette| say_now(&palette, line));
}

/// Adds one line to the message band, on the main thread - what `say_later`
/// hands over, and what the pack window says about the keyboard focus.
pub fn say_now(palette: &Palette, line: String) {
    let mut lines: Vec<SharedString> = slint::Model::iter(&palette.get_messages()).collect();
    lines.push(line.into());
    palette.set_messages(ModelRc::new(VecModel::from(lines)));
}

/// Asks the main thread to open the pack window.
///
/// Through the palette's own callback, the one a click on the pack's name will
/// use as well, so both ways in reach the same code on the main thread. Dropped
/// for the reason `show` gives.
fn open_packs_later(palette: &Weak<Palette>) {
    let _ = palette.upgrade_in_event_loop(|palette| palette.invoke_open_packs());
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
    use nkb_app::{UpcomingValue, ValueFacts};
    use nkb_core::report::{Arrival, ControlKind, Target};
    use nkb_core::sequence::{Position, Sequence};
    use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
    use slint::platform::{Platform, PlatformError, WindowAdapter};
    use slint::{Model, ModelRc, SharedString, VecModel};

    use std::cell::{Cell, RefCell};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc;

    use nkb_adapters::{BuiltInCatalogue, TomlPackFormat, i18n};
    use nkb_app::advance_sequence::ChooseError;
    use nkb_app::ports::SettingChange;
    use nkb_app::{AdvanceSequence, KeptSettings};

    use super::{
        Command, CopyButton, Delivery, Duration, HotkeyAction, InFlight, LiveShortcuts, Memory,
        Outcome, Palette, Progress, ShortcutRegistration, ShownKey, Standing, ValueBand, ValueKey,
        ValuePreview, Wait, apply, apply_in_flight, choose, clipboard_bar, copy_command,
        in_flight_view, markers_in_flight, markers_of, set_compact, share, view_between, view_of,
        with_standing,
    };

    /// No standing sentence: the ordinary case, and the one the field-by-field
    /// test is about.
    pub(super) fn quiet() -> Standing {
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

    /// A palette on the headless platform. Slint keeps the platform per THREAD
    /// and each test runs on a thread of its own, so the platform goes in on
    /// the first call on each. Until 2026-10-10 a global `Once` put it on one
    /// thread only: the others fell to the system's backend, and the second of
    /// them failed to build a palette - measured when two more tests came in.
    pub(super) fn a_palette() -> Palette {
        thread_local! {
            static INSTALLED: Cell<bool> = const { Cell::new(false) };
        }
        if !INSTALLED.get() {
            let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
            slint::platform::set_platform(Box::new(Headless { window }))
                .expect("no other platform may be installed on this thread");
            INSTALLED.set(true);
        }
        Palette::new().expect("the palette must build")
    }

    pub(super) fn a_sent() -> Sent {
        Sent {
            key: ValueKey {
                pack: String::from("unicode-text"),
                value: String::from("zero-width"),
            },
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
            target: Some(Target {
                program: Some(String::from("notepad.exe")),
                field: ControlKind::TextField,
            }),
            report: Some(String::from("Value:    unicode-text/zero-width @ pack 1.0")),
        }
    }

    pub(super) fn an_outcome(sent: Option<Sent>, messages: Vec<Message>) -> Outcome {
        Outcome {
            sequence: Sequence {
                position: Position::Running { done: 7, total: 34 },
                delivery: Delivery::Direct,
            },
            sent,
            messages,
            attempted_send: true,
            clipboard_for_window: false,
            upcoming: Some(an_upcoming()),
        }
    }

    /// The value after the one that went out, with its own name, reference and
    /// preview, so the next band cannot borrow a line from the value band and
    /// still pass.
    fn an_upcoming() -> UpcomingValue {
        UpcomingValue::Value {
            index: 8,
            total: 34,
            key: ValueKey {
                pack: "unicode-text".to_owned(),
                value: "trailing-nbsp".to_owned(),
            },
            name: "Trailing no-break space".to_owned(),
            reference: "unicode-text/trailing-nbsp".to_owned(),
            preview: ValuePreview::Text(nkb_core::preview::preview("Kowalski\u{A0}")),
            offensive: true,
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
                &nkb_adapters::default_bindings(),
            ),
        );
        assert_eq!(palette.get_pack(), "Unicode & text");
        assert_eq!(palette.get_counter(), "7 / 34");
        assert_eq!(palette.get_value_name(), "Three zero-width spaces");
        // Where it was typed, read at the press (UX8, `D104`).
        assert!(palette.get_has_sent_to());
        assert_eq!(palette.get_value_sent_to(), "to notepad.exe, a text field");
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
        // The next value (UX-GUI-001) - its own lines, not the value band's.
        assert!(palette.get_has_next());
        assert!(palette.get_next_has_value());
        assert_eq!(palette.get_next_heading(), "Next: value 8 of 34");
        assert_eq!(palette.get_next_name(), "Trailing no-break space");
        assert_eq!(palette.get_next_preview(), "Kowalski\u{2423}");
        assert_eq!(
            slint::Model::row_count(&palette.get_next_markers()),
            1,
            "the one risk known before a send - offensive - and nothing about a delivery"
        );
        // `D98`: each Copy button's key arrives with its band, and a click
        // asks for the value drawn there - the next one beside the next band,
        // the one sent beside the value band, never the other way round.
        assert_eq!(
            copy_command(&palette.get_next_key(), CopyButton::Next),
            Some(Command::Copy(
                value_key("unicode-text", "trailing-nbsp"),
                CopyButton::Next
            ))
        );
        assert_eq!(
            copy_command(&palette.get_last_key(), CopyButton::Last),
            Some(Command::Copy(
                value_key("unicode-text", "zero-width"),
                CopyButton::Last
            ))
        );

        // ---- the next press only says the pack is finished ----------------
        let mut ending = an_outcome(None, Vec::new());
        ending.upcoming = Some(UpcomingValue::EndOfPack { total: 34 });
        apply(
            &palette,
            view_of(
                &ending,
                "p",
                "p",
                &quiet(),
                &nkb_adapters::default_bindings(),
            ),
        );
        assert!(palette.get_has_next());
        assert!(
            !palette.get_next_has_value(),
            "no value goes out at the end"
        );
        assert_eq!(palette.get_next_heading(), "Next: end of pack (34/34)");

        // ---- nothing upcoming: the band goes ------------------------------
        let mut nothing = an_outcome(None, Vec::new());
        nothing.upcoming = None;
        apply(
            &palette,
            view_of(
                &nothing,
                "p",
                "p",
                &quiet(),
                &nkb_adapters::default_bindings(),
            ),
        );
        assert!(!palette.get_has_next());

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
                &nkb_adapters::default_bindings(),
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
                &nkb_adapters::default_bindings(),
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
                &nkb_adapters::default_bindings(),
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
            view_of(
                &an_outcome(Some(generated), Vec::new()),
                "p",
                "p",
                &quiet(),
                &nkb_adapters::default_bindings(),
            ),
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
        apply(
            &palette,
            view_of(
                &by_clipboard,
                "p",
                "p",
                &quiet(),
                &nkb_adapters::default_bindings(),
            ),
        );
        assert!(palette.get_clipboard_mode());
        assert_eq!(palette.get_clipboard_mode_label(), "clipboard mode");
        assert!(
            palette.get_clipboard_mode_on(),
            "the mode is the tester's, so the bar carries its way out (D99)"
        );
        assert_eq!(
            palette.get_route_selected(),
            1,
            "the route switch does not fill Clipboard in clipboard mode"
        );

        // ---- and so does the window in front (`D72`), in its own words ---
        let for_window = Outcome {
            clipboard_for_window: true,
            ..an_outcome(Some(a_sent()), Vec::new())
        };
        apply(
            &palette,
            view_of(
                &for_window,
                "p",
                "p",
                &quiet(),
                &nkb_adapters::default_bindings(),
            ),
        );
        assert!(palette.get_clipboard_mode());
        assert_eq!(
            palette.get_clipboard_mode_label(),
            "clipboard mode for this window"
        );
        assert!(
            !palette.get_clipboard_mode_on(),
            "a window the tester is looking at is not a mode a click could end (D99)"
        );
        assert_eq!(
            palette.get_route_selected(),
            0,
            "the route switch fills Clipboard for a window the tester did not choose it for"
        );
        apply(
            &palette,
            view_of(
                &an_outcome(None, Vec::new()),
                "p",
                "p",
                &quiet(),
                &nkb_adapters::default_bindings(),
            ),
        );
        assert!(
            !palette.get_clipboard_mode(),
            "the window left the front, so the bar goes"
        );

        // ---- how a typed value meets the field (`D101`) ----------------------
        // A view between presses carries the way, a press never changes it.
        let mut at_cursor = on("whitespace");
        at_cursor.set_clearing(super::Clearing::Keep);
        apply(
            &palette,
            view_between(
                &at_cursor,
                "whitespace",
                Vec::new(),
                ValueBand::Keep,
                &quiet(),
                false,
            ),
        );
        // The switch's words, once, and the way in effect filled: the second
        // way is the cursor (the owner's point 2).
        super::label_switches(&palette);
        super::label_arrows(&palette);
        let words = |model: slint::ModelRc<slint::SharedString>| {
            model
                .iter()
                .map(|word| word.to_string())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            words(palette.get_clearing_options()),
            ["Clear line first", "Insert at cursor"]
        );
        assert_eq!(
            words(palette.get_route_options()),
            ["Keyboard", "Clipboard"]
        );
        assert_eq!(
            palette.get_clearing_selected(),
            1,
            "the cursor way is not filled"
        );
        apply(
            &palette,
            view_of(
                &an_outcome(None, Vec::new()),
                "p",
                "p",
                &quiet(),
                &nkb_adapters::default_bindings(),
            ),
        );
        assert_eq!(
            palette.get_clearing_selected(),
            1,
            "a press changed the way of clearing the switch shows"
        );
        super::show_clearing(&palette, super::Clearing::Line);
        assert_eq!(palette.get_clearing_selected(), 0);
        // A click on each way asks for that way, never for "the other one" -
        // and an index the switch does not show asks for nothing.
        assert_eq!(
            [0, 1, 2, -1].map(super::clearing_command),
            [
                Some(Command::SetClearing(super::Clearing::Line)),
                Some(Command::SetClearing(super::Clearing::Keep)),
                None,
                None,
            ]
        );
        assert_eq!(
            [0, 1, 2, -1].map(super::route_command),
            [
                Some(Command::TurnOffClipboard),
                Some(Command::UseClipboard),
                None,
                None,
            ]
        );

        // ---- the tester collapses it, and only the tester expands it -------
        set_compact(&palette, true);
        assert!(palette.get_compact());
        apply(
            &palette,
            view_of(
                &an_outcome(Some(sent_of("b")), Vec::new()),
                "p",
                "p",
                &quiet(),
                &nkb_adapters::default_bindings(),
            ),
        );
        assert!(
            palette.get_compact(),
            "a value expanded what the tester collapsed"
        );
        set_compact(&palette, false);
        assert!(!palette.get_compact());

        // ---- another pack takes the value away -----------------------------
        // The value on screen came from the pack before. Under the new pack's
        // name it would say something false, so the band goes back to "nothing
        // sent yet" - and a view with nothing to show keeps that, too.
        assert!(palette.get_has_value(), "the band holds a value before");
        let another = on("unicode-text");
        apply(
            &palette,
            view_between(
                &another,
                "unicode-text",
                Vec::new(),
                ValueBand::Clear,
                &quiet(),
                false,
            ),
        );
        assert_eq!(palette.get_pack(), "Unicode and text");
        assert!(
            !palette.get_has_value(),
            "the old value stayed under the new pack"
        );
        apply(
            &palette,
            view_between(
                &another,
                "unicode-text",
                Vec::new(),
                ValueBand::Keep,
                &quiet(),
                false,
            ),
        );
        assert!(!palette.get_has_value(), "keeping nothing showed something");

        // ---- a value on its way, then its outcome (`OBS-160`) ---------------
        // The send band and the value band from one report, every field where
        // it belongs. Then any view of the worker's takes the band away.
        // Any sentence of the product's will do: what matters is that it goes.
        palette.set_messages(ModelRc::new(VecModel::from(vec![SharedString::from(
            i18n::label(nkb_adapters::i18n::PaletteLabel::ShortcutsPaused),
        )])));
        let in_flight = InFlight {
            facts: a_sent().facts,
            offensive: true,
            progress: Progress {
                units_arrived: 3,
                units_total: 12,
            },
        };
        let sent_before = ShownKey {
            pack: "unicode-text".into(),
            value: "nbsp".into(),
        };
        palette.set_last_key(sent_before.clone());
        apply_in_flight(&palette, in_flight_view(&in_flight, &quiet()));
        assert!(palette.get_sending(), "the send band is up");
        // `D98`: the value on its way has not been sent, so the Copy button of
        // the value band keeps naming the last one that was - and the band
        // fades it while the send runs.
        assert_eq!(palette.get_last_key(), sent_before);
        assert_eq!(palette.get_sending_counter(), "3 / 12 units");
        assert_eq!(palette.get_sending_label(), "Typing");
        assert_eq!(palette.get_sending_hint(), "Press Esc to stop.");
        assert!((palette.get_sending_fraction() - 0.25).abs() < f32::EPSILON);
        assert!(palette.get_has_value());
        assert_eq!(palette.get_value_name(), "Three zero-width spaces");
        assert_eq!(palette.get_value_reference(), "unicode-text/zero-width");
        assert!(
            palette.get_value_counts().contains("UTF-16 units: 12"),
            "the counts name the whole value's size: {}",
            palette.get_value_counts()
        );
        assert_eq!(
            palette.get_messages().row_count(),
            0,
            "the sentence about the value before is stale once another goes"
        );
        apply(
            &palette,
            view_of(
                &an_outcome(Some(a_sent()), Vec::new()),
                "Unicode & text",
                "u",
                &quiet(),
                &nkb_adapters::default_bindings(),
            ),
        );
        assert!(
            !palette.get_sending(),
            "the outcome took the send band away"
        );
        assert_eq!(
            palette.get_last_key().value,
            "zero-width",
            "and the value that went out is the one its Copy button names now"
        );
    }

    #[test]
    fn the_share_of_a_send_is_clamped_and_a_value_on_its_way_earns_only_its_own_markers() {
        assert!((share(1, 4) - 0.25).abs() < f32::EPSILON);
        assert!(share(0, 0).abs() < f32::EPSILON, "no total, no share");
        assert!(
            (share(9, 4) - 1.0).abs() < f32::EPSILON,
            "never past the end"
        );
        let mut in_flight = InFlight {
            facts: a_sent().facts,
            offensive: true,
            progress: Progress {
                units_arrived: 1,
                units_total: 7,
            },
        };
        // Offensive and warnings are facts about the value. `cleared first`,
        // `interrupted` and `on the clipboard` are about a delivery not over.
        assert_eq!(
            markers_in_flight(&in_flight),
            vec![
                ("offensive".to_owned(), true),
                ("pack warnings: 2".to_owned(), true)
            ]
        );
        in_flight.offensive = false;
        in_flight.facts.warnings = 0;
        assert!(markers_in_flight(&in_flight).is_empty());
    }

    /// A sequence on a pack that ships, the way the palette opens one.
    fn on(pack: &str) -> AdvanceSequence {
        let mut sequence = AdvanceSequence::new();
        sequence
            .choose_pack(&BuiltInCatalogue::new(), &TomlPackFormat, pack)
            .expect("a shipped pack opens");
        sequence
    }

    /// The settings of one run, over `store`.
    fn memory_over(store: &Remembered) -> Memory<'_> {
        let (kept, _) = KeptSettings::open(store);
        Memory {
            kept: RefCell::new(kept),
            store,
            file: String::from("f"),
            bindings: Cell::new(nkb_adapters::default_bindings()),
        }
    }

    pub(super) fn a_store(fail: bool) -> Remembered {
        Remembered {
            saved: RefCell::new(Vec::new()),
            fail,
        }
    }

    /// 🔴 The pack the tester picked opens, is remembered, and the value from
    /// the pack before is taken off the screen.
    #[test]
    fn choosing_another_pack_opens_it_remembers_it_and_clears_the_value() {
        let store = a_store(false);
        let memory = memory_over(&store);
        let mut sequence = on("whitespace");
        let mut pack = String::from("whitespace");

        let view = choose(
            &mut sequence,
            &memory,
            &mut pack,
            "unicode-text",
            &quiet(),
            false,
        )
        .expect("another pack is drawn");

        assert_eq!(pack, "unicode-text", "the messages are about the new pack");
        assert_eq!(sequence.pack_id(), Some("unicode-text"));
        assert_eq!(view.pack, "Unicode and text");
        let (done, total) = sequence.counter().expect("a pack is open");
        assert_eq!(done, 0, "a new pack starts at its beginning");
        assert_eq!(view.counter, i18n::counter(0, total));
        assert!(matches!(view.value, ValueBand::Clear));
        assert!(view.messages.is_empty(), "{:?}", view.messages);
        assert_eq!(
            *store.saved.borrow(),
            vec![SettingChange::Pack(String::from("unicode-text"))]
        );
    }

    /// A pack that will not open changes nothing, and the sentence names the
    /// pack that was asked for - not the one still in use.
    #[test]
    fn a_pack_that_cannot_be_opened_changes_nothing_and_says_which() {
        let store = a_store(false);
        let memory = memory_over(&store);
        let mut sequence = on("whitespace");
        let mut pack = String::from("whitespace");

        let view = choose(
            &mut sequence,
            &memory,
            &mut pack,
            "no-such-pack",
            &quiet(),
            false,
        )
        .expect("the refusal is drawn");

        assert_eq!(pack, "whitespace");
        assert_eq!(sequence.pack_id(), Some("whitespace"));
        assert_eq!(view.pack, "Whitespace");
        assert!(
            matches!(view.value, ValueBand::Keep),
            "the value on screen still belongs to the pack in use"
        );
        assert_eq!(
            view.messages,
            vec![i18n::choose_error(&ChooseError::NotFound, "no-such-pack")]
        );
        assert!(
            store.saved.borrow().is_empty(),
            "a pack that did not open was remembered"
        );
    }

    /// The pack in use, asked for again, is left where it is - its place in
    /// it included. The pack window never asks, but it works from a snapshot.
    #[test]
    fn choosing_the_pack_in_use_changes_nothing_and_draws_nothing() {
        let store = a_store(false);
        let memory = memory_over(&store);
        let mut sequence = on("whitespace");
        let mut pack = String::from("whitespace");

        assert!(
            choose(
                &mut sequence,
                &memory,
                &mut pack,
                "whitespace",
                &quiet(),
                false
            )
            .is_none()
        );
        assert!(
            store.saved.borrow().is_empty(),
            "nothing changed, nothing is saved"
        );
    }

    /// The new pack opens even when it cannot be remembered, and the reason
    /// stands beside it.
    #[test]
    fn a_pack_that_cannot_be_remembered_still_opens_and_says_so() {
        let store = a_store(true);
        let memory = memory_over(&store);
        let mut sequence = on("whitespace");
        let mut pack = String::from("whitespace");

        let view = choose(
            &mut sequence,
            &memory,
            &mut pack,
            "unicode-text",
            &quiet(),
            false,
        )
        .expect("another pack is drawn");

        assert_eq!(view.pack, "Unicode and text");
        assert!(
            view.messages
                .iter()
                .any(|line| line.contains("could not be saved to f")),
            "{:?}",
            view.messages
        );
    }

    /// The button in the pack band reaches the same switch as the shortcut
    /// (`UX-GUI-004`): each click flips it, hands the state on to draw and
    /// saves exactly that state - and changes nothing else on the palette.
    #[test]
    fn a_click_on_the_collapse_button_flips_the_one_switch_and_saves_it() {
        let store = a_store(false);
        let registrar = Registrar::default();
        with_worker(&store, &registrar, true, |worker, _| {
            let first = worker.carry_out(Command::ToggleCompact);
            assert_eq!(first.toggled, Some((true, None)));
            assert!(
                first.view.is_none() && first.told.is_none(),
                "a collapse redrew the palette's bands or told the shortcuts window"
            );
            let second = worker.carry_out(Command::ToggleCompact);
            assert_eq!(second.toggled, Some((false, None)));
        });
        assert_eq!(
            *store.saved.borrow(),
            vec![SettingChange::Compact(true), SettingChange::Compact(false)]
        );
    }

    /// `D109`: closing the welcome window starts the pack over - values tried
    /// in its box do not eat the first ones of the real field - and draws the
    /// palette that now appears, and remembers the welcome as done.
    #[test]
    fn closing_the_welcome_starts_the_pack_over_and_remembers_it() {
        let store = a_store(false);
        let registrar = Registrar::default();
        with_worker(&store, &registrar, true, |worker, _| {
            // Where two values tried in the box leave the sequence.
            assert!(worker.sequence.choose_next("space-only"));
            assert!(matches!(
                worker.sequence.upcoming(),
                Some(UpcomingValue::Value { index: 3, .. })
            ));
            let carried = worker.carry_out(Command::WelcomeDone);
            assert!(
                matches!(
                    worker.sequence.upcoming(),
                    Some(UpcomingValue::Value { index: 1, .. })
                ),
                "testing starts at the first value"
            );
            let view = carried.view.expect("the palette appearing is drawn");
            assert!(view.messages.is_empty(), "{:?}", view.messages);
        });
        assert_eq!(*store.saved.borrow(), vec![SettingChange::WelcomeDone]);
    }

    /// `D101`: a click on a way of the clearing switch sets how typed values
    /// meet the field - the sequence's own choice, so the next press uses it -
    /// draws the switch with that way filled and saves exactly that way, and
    /// moves nothing in the pack. A click on the way in effect already, which
    /// two quick clicks before a redraw produce, changes and saves nothing -
    /// where the toggle before the switch would have undone the first click.
    #[test]
    fn a_click_on_a_way_of_clearing_sets_that_way_and_saves_it_once() {
        let store = a_store(false);
        let registrar = Registrar::default();
        with_worker(&store, &registrar, true, |worker, _| {
            let before = worker.sequence.upcoming();
            assert_eq!(worker.sequence.clearing(), super::Clearing::Line);
            let carried = worker.carry_out(Command::SetClearing(super::Clearing::Keep));
            assert_eq!(worker.sequence.clearing(), super::Clearing::Keep);
            let view = carried.view.expect("the palette shows the new way");
            assert_eq!(view.clearing, Some(super::Clearing::Keep));
            assert!(view.messages.is_empty(), "{:?}", view.messages);
            assert_eq!(worker.sequence.upcoming(), before, "the pack moved");
            let again = worker.carry_out(Command::SetClearing(super::Clearing::Keep));
            assert_eq!(worker.sequence.clearing(), super::Clearing::Keep);
            assert!(
                again.view.is_none() && again.told.is_none(),
                "asking for the way in effect redrew the palette"
            );
            let back = worker.carry_out(Command::SetClearing(super::Clearing::Line));
            assert_eq!(worker.sequence.clearing(), super::Clearing::Line);
            assert_eq!(
                back.view.expect("drawn").clearing,
                Some(super::Clearing::Line)
            );
        });
        assert_eq!(
            *store.saved.borrow(),
            vec![
                SettingChange::Clearing(super::Clearing::Keep),
                SettingChange::Clearing(super::Clearing::Line)
            ]
        );
    }

    /// The palette starts with the way the tester last chose, and with the
    /// line when the file says nothing.
    #[test]
    fn the_palette_starts_with_the_remembered_way_of_clearing_or_the_line() {
        assert_eq!(
            super::starts_clearing(&super::Settings::default()),
            super::Clearing::Line
        );
        assert_eq!(
            super::starts_clearing(&super::Settings {
                clearing: Some(super::Clearing::Keep),
                ..super::Settings::default()
            }),
            super::Clearing::Keep
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
    pub(super) struct Remembered {
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
        let store = a_store(false);
        let memory = memory_over(&store);
        let collapse = super::Collapse::new(&memory);
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
        let store = a_store(true);
        let memory = memory_over(&store);
        let collapse = super::Collapse::new(&memory);
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

    // ---- the shortcuts window: pause, change, resume (K5.3) -------------------

    use std::sync::Arc;
    use std::sync::atomic::AtomicUsize;

    use nkb_adapters::i18n::PaletteLabel;
    use nkb_adapters::{ClipboardDelivery, DirectInjection, EnglishReport};
    use nkb_app::ShortcutChange;
    use nkb_app::advance_sequence::Ports;
    use nkb_app::ports::{HotkeyRegistrar, ShortcutsUnavailable};
    use nkb_core::hotkeys::HotkeyChord;

    use super::{Hold, Next, ShortcutsNow, Told, Worker, legend, next_command};
    use crate::clipboard::SystemClipboard;

    fn chord(text: &str) -> HotkeyChord {
        HotkeyChord::parse(text).unwrap_or_else(|error| panic!("{text}: {error:?}"))
    }

    fn no_layout(_: HotkeyChord) -> Option<char> {
        None
    }

    /// A registrar standing in for the system: a chord in `taken` is held by
    /// another program, and `unavailable` answers that nothing can be
    /// registered at all. It keeps every set it was asked for and counts the
    /// handles alive - how many were alive when each set was asked for, too.
    #[derive(Default)]
    pub(super) struct Registrar {
        taken: RefCell<Vec<HotkeyChord>>,
        unavailable: Cell<bool>,
        asked: RefCell<Vec<Vec<(HotkeyAction, HotkeyChord)>>>,
        alive_when_asked: RefCell<Vec<usize>>,
        alive: Arc<AtomicUsize>,
    }

    impl Registrar {
        fn alive(&self) -> usize {
            self.alive.load(Ordering::SeqCst)
        }
        fn last_asked(&self) -> Vec<(HotkeyAction, HotkeyChord)> {
            self.asked.borrow().last().cloned().unwrap_or_default()
        }
    }

    struct Held {
        outcomes: Vec<(HotkeyAction, ShortcutRegistration)>,
        alive: Arc<AtomicUsize>,
    }

    impl Drop for Held {
        fn drop(&mut self) {
            self.alive.fetch_sub(1, Ordering::SeqCst);
        }
    }

    impl LiveShortcuts for Held {
        fn outcomes(&self) -> &[(HotkeyAction, ShortcutRegistration)] {
            &self.outcomes
        }
        fn next(&self, _: Duration) -> Wait {
            Wait::Nothing
        }
    }

    impl HotkeyRegistrar for Registrar {
        fn register(
            &self,
            bindings: &[(HotkeyAction, nkb_core::hotkeys::HotkeyChord)],
        ) -> Result<Box<dyn LiveShortcuts + Send>, ShortcutsUnavailable> {
            self.asked.borrow_mut().push(bindings.to_vec());
            self.alive_when_asked.borrow_mut().push(self.alive());
            if self.unavailable.get() {
                return Err(ShortcutsUnavailable::CouldNotStart);
            }
            self.alive.fetch_add(1, Ordering::SeqCst);
            let taken = self.taken.borrow();
            Ok(Box::new(Held {
                outcomes: bindings
                    .iter()
                    .map(|(action, chord)| {
                        let outcome = if taken.contains(chord) {
                            ShortcutRegistration::Taken
                        } else {
                            ShortcutRegistration::Registered
                        };
                        (*action, outcome)
                    })
                    .collect(),
                alive: Arc::clone(&self.alive),
            }))
        }
    }

    /// A worker on `whitespace` over `store` and `registrar`, holding the
    /// shortcuts of the defaults when `held`, handed to `test`.
    pub(super) fn with_worker<R>(
        store: &Remembered,
        registrar: &Registrar,
        held: bool,
        test: impl FnOnce(&mut Worker<'_>, &Memory<'_>) -> R,
    ) -> R {
        let memory = memory_over(store);
        let clipboard = SystemClipboard::new();
        let by_clipboard = ClipboardDelivery::new(&clipboard);
        let ports = Ports {
            direct: &DirectInjection,
            by_clipboard: &by_clipboard,
            keys: &DirectInjection,
            inspector: &DirectInjection,
            clipboard: &clipboard,
            report_text: &EnglishReport,
            progress: &|_| {},
        };
        let standing = quiet();
        let mut worker = Worker {
            sequence: on("whitespace"),
            pack: String::from("whitespace"),
            memory: &memory,
            hold: Hold::new(registrar),
            ports: &ports,
            standing: &standing,
            defaults: nkb_adapters::default_bindings(),
            types: &no_layout,
            route: None,
            collapse: super::Collapse::new(&memory),
        };
        if held {
            let lines = worker.take(memory.bindings.get());
            assert!(lines.is_empty(), "{lines:?}");
        }
        test(&mut worker, &memory)
    }

    fn paused_line() -> String {
        i18n::label(PaletteLabel::ShortcutsPaused).to_owned()
    }

    /// 🔴 The window opens on a palette that has let its shortcuts go - a
    /// press there must reach the window, not send a value into it - and
    /// hears the table it edits in the answer.
    #[test]
    fn a_pause_gives_the_shortcuts_back_says_so_and_tells_the_window_the_table() {
        let store = a_store(false);
        let registrar = Registrar::default();
        with_worker(&store, &registrar, true, |worker, _| {
            assert_eq!(registrar.alive(), 1);

            let carried = worker.carry_out(Command::Pause);

            assert_eq!(registrar.alive(), 0, "the shortcuts were not given back");
            assert!(worker.hold.paused());
            let view = carried.view.expect("the palette says it is paused");
            assert_eq!(view.messages, vec![paused_line()]);
            let defaults = nkb_adapters::default_bindings();
            assert_eq!(
                carried.told,
                Some(Told::Paused(ShortcutsNow {
                    bindings: defaults,
                    registered: defaults
                        .as_slice()
                        .iter()
                        .map(|(action, chord)| (*action, *chord, ShortcutRegistration::Registered))
                        .collect(),
                }))
            );
        });
    }

    /// `D99`: the two clicks reach the sequence and come back as the bar, the
    /// button that stands and the sentence - with the value band kept and the
    /// place in the pack unmoved. Neither writes the clipboard: only a press in
    /// the mode does, and nothing here presses.
    #[test]
    fn clipboard_mode_turns_on_and_off_from_the_palette_and_says_so() {
        let store = a_store(false);
        let registrar = Registrar::default();
        with_worker(&store, &registrar, true, |worker, _| {
            let before = worker.sequence.sequence();
            assert_eq!(before.delivery, Delivery::Direct);

            let on = worker.carry_out(Command::UseClipboard);
            let view = on.view.expect("turning on is answered");
            assert_eq!(
                view.messages,
                vec![String::from(
                    "Clipboard mode: each value goes to your clipboard, replacing what you had \
                     copied. Press your paste shortcut to insert each one."
                )]
            );
            assert_eq!(view.clipboard_bar, Some("clipboard mode"));
            assert!(view.clipboard_mode_on);
            assert!(matches!(view.value, ValueBand::Keep));
            assert!(on.told.is_none());
            assert_eq!(worker.sequence.sequence().position, before.position);

            let off = worker
                .carry_out(Command::TurnOffClipboard)
                .view
                .expect("turning off is answered");
            // Only where this build types into other applications - Windows
            // today. Elsewhere (`OBS-70`) there is no way to leave the mode
            // for, and the palette says why rather than pretending: measured
            // on macOS 2026-10-07, the first full run there since this test
            // was written assuming Windows.
            if !cfg!(windows) {
                assert_eq!(off.messages.len(), 1, "{:?}", off.messages);
                assert!(
                    off.messages[0]
                        .starts_with("This build cannot type into other applications on "),
                    "{:?}",
                    off.messages
                );
                assert_eq!(off.clipboard_bar, Some("clipboard mode"));
                assert!(off.clipboard_mode_on);
                return;
            }
            assert_eq!(
                off.messages,
                vec![String::from(
                    "Clipboard mode is off: from the next press, each value is typed into the field."
                )]
            );
            assert_eq!(off.clipboard_bar, None);
            assert!(!off.clipboard_mode_on);
            assert_eq!(worker.sequence.sequence(), before);

            let again = worker
                .carry_out(Command::TurnOffClipboard)
                .view
                .expect("a second click is answered with the palette as it is");
            assert!(
                again.messages.is_empty(),
                "a click on a mode already off said something: {:?}",
                again.messages
            );
        });
    }

    /// `D99`: paused is not lost. The shortcuts window gives the shortcuts back
    /// when it closes, so a click while it is open turns the mode on at once,
    /// and the palette shows it at once.
    #[test]
    fn clipboard_mode_asked_for_while_the_shortcuts_are_paused_is_on_at_once() {
        let store = a_store(false);
        let registrar = Registrar::default();
        with_worker(&store, &registrar, true, |worker, _| {
            worker.carry_out(Command::Pause);
            let view = worker
                .carry_out(Command::UseClipboard)
                .view
                .expect("a paused palette still answers the click");
            assert_eq!(worker.sequence.sequence().delivery, Delivery::ClipboardMode);
            assert!(view.clipboard_mode_on);
            assert_eq!(
                view.messages.first(),
                Some(&paused_line()),
                "the pause is still said, first"
            );
        });
    }

    /// `D99` under the rule of `D71`: with no shortcuts held, nothing drives
    /// the palette, so turning clipboard mode on waits - like `--clipboard` at
    /// start - and is kept, with its warning, the moment they are held.
    #[test]
    fn clipboard_mode_asked_for_with_no_shortcuts_waits_for_them() {
        let store = a_store(false);
        let registrar = Registrar::default();
        with_worker(&store, &registrar, false, |worker, _| {
            assert!(
                worker.hold.live().is_none(),
                "the test needs no shortcuts held"
            );
            let asked = worker.carry_out(Command::UseClipboard);
            assert!(
                asked.view.is_none(),
                "a palette nothing drives was told to paste each value"
            );
            assert_eq!(worker.sequence.sequence().delivery, Delivery::Direct);

            let held = worker
                .carry_out(Command::Resume)
                .view
                .expect("taking the shortcuts is drawn");
            assert_eq!(registrar.alive(), 1);
            assert_eq!(
                worker.sequence.sequence().delivery,
                Delivery::ClipboardMode,
                "the click was forgotten when the shortcuts came"
            );
            assert!(
                held.messages
                    .iter()
                    .any(|line| line.starts_with("Clipboard mode: each value goes")),
                "the mode came without its warning: {:?}",
                held.messages
            );
            assert!(held.clipboard_mode_on);
        });
    }

    /// The value window opens on what this slot says (UX2): the pack in use and
    /// its next value once a pack opens, the next value moved by a press with
    /// the pack kept, and no value named at the end of a pack - and the restart
    /// shortcut of the table it was given, which its restart row names (UX4).
    #[test]
    fn the_slot_says_the_pack_in_use_and_the_value_its_next_press_sends() {
        let slot = super::in_use();
        let (mine, refused) = nkb_adapters::default_bindings().with(
            &[(HotkeyAction::RestartPack, chord("Alt+Shift+F9"))],
            &no_layout,
        );
        assert!(refused.is_empty(), "{refused:?}");
        super::publish(&slot, &on("whitespace"), &mine);
        assert_eq!(
            super::in_use_now(&slot),
            super::InUseNow {
                pack: Some(String::from("whitespace")),
                next: Some(String::from("trailing-space")),
                restart: Some(chord("Alt+Shift+F9")),
                recent: Vec::new(),
            }
        );
        let mut later = on("whitespace");
        assert!(later.choose_next("leading-space"));
        super::publish_next(&slot, later.upcoming().as_ref());
        let now = super::in_use_now(&slot);
        assert_eq!(now.next.as_deref(), Some("leading-space"));
        assert_eq!(
            now.pack.as_deref(),
            Some("whitespace"),
            "a press keeps the pack"
        );
        super::publish_next(
            &slot,
            Some(&nkb_app::UpcomingValue::EndOfPack { total: 12 }),
        );
        assert_eq!(super::in_use_now(&slot).next, None);
    }

    fn value_key(pack: &str, value: &str) -> ValueKey {
        ValueKey {
            pack: pack.to_owned(),
            value: value.to_owned(),
        }
    }

    /// `D106`: a value that went out is not recent - a press moves the next
    /// value and leaves the list as the window left it - and a publish after
    /// a command keeps the list, which only the worker's notes change.
    #[test]
    fn a_value_that_went_out_is_not_recent_and_a_publish_keeps_the_list() {
        let slot = super::in_use();
        let chosen = value_key("locale-pl", "pesel-valid");
        slot.lock()
            .expect("the slot is not poisoned")
            .recent
            .push(chosen.clone());

        super::after_press(&slot, &an_outcome(Some(a_sent()), Vec::new()));
        let now = super::in_use_now(&slot);
        assert_eq!(
            now.recent,
            vec![chosen.clone()],
            "a press made the value it sent recent"
        );
        assert_eq!(
            now.next,
            super::next_id(Some(&an_upcoming())),
            "the press also moved the next value"
        );
        assert!(now.next.is_some());

        super::publish(&slot, &on("whitespace"), &nkb_adapters::default_bindings());
        assert_eq!(super::in_use_now(&slot).recent, vec![chosen]);
    }

    /// A value chosen in the window is recent once it is the next one, and a
    /// choice that did not happen is not - nor the restart row, which puts
    /// the first value next without the tester choosing it.
    #[test]
    fn a_value_chosen_in_the_window_is_recent_only_when_it_became_the_next() {
        let store = a_store(false);
        let registrar = Registrar::default();
        with_worker(&store, &registrar, false, |worker, _| {
            let slot = super::in_use();
            let choose = |worker: &mut Worker<'_>, pack: &str, value: &str| {
                let _ = super::carry_out_noting(
                    worker,
                    &slot,
                    Command::ChooseValue {
                        pack: pack.to_owned(),
                        value: value.to_owned(),
                    },
                );
            };
            choose(worker, "whitespace", "no-such-value");
            choose(worker, "no-such-pack", "trailing-space");
            assert!(super::in_use_now(&slot).recent.is_empty());

            choose(worker, "whitespace", "leading-space");
            choose(worker, "locale-pl", "pesel-valid");
            let _ = super::carry_out_noting(worker, &slot, Command::ToggleCompact);
            let _ = super::carry_out_noting(
                worker,
                &slot,
                Command::Restart {
                    pack: String::from("whitespace"),
                    value: String::from("trailing-space"),
                },
            );
            assert_eq!(
                super::next_id(worker.sequence.upcoming().as_ref()).as_deref(),
                Some("trailing-space"),
                "the restart row put the first value next"
            );
            assert_eq!(
                super::in_use_now(&slot).recent,
                vec![
                    value_key("locale-pl", "pesel-valid"),
                    value_key("whitespace", "leading-space"),
                ]
            );
        });
    }

    /// The list goes to the settings once, as the palette closes - and not
    /// at all when it is the one the palette started with.
    #[test]
    fn the_recent_values_are_saved_once_when_the_palette_closes() {
        let store = a_store(false);
        let memory = memory_over(&store);
        let slot = super::in_use();
        super::keep_recent(&memory, &slot);
        assert_eq!(
            *store.saved.borrow(),
            Vec::new(),
            "the list it started with"
        );

        let chosen = value_key("locale-pl", "pesel-valid");
        slot.lock()
            .expect("the slot is not poisoned")
            .recent
            .push(chosen.clone());
        super::keep_recent(&memory, &slot);
        assert_eq!(
            *store.saved.borrow(),
            vec![SettingChange::Recent(vec![chosen])]
        );
    }

    /// UX2: a value chosen in the value window becomes the next one - in the
    /// pack in use as it is, in another pack after that pack opens and is
    /// remembered - and an identifier the pack does not hold changes nothing
    /// and is said rather than dropped.
    #[test]
    fn a_value_chosen_in_the_window_becomes_the_next_and_its_pack_opens_first() {
        let store = a_store(false);
        let registrar = Registrar::default();
        with_worker(&store, &registrar, false, |worker, _| {
            let heading = |view: &super::View| view.next.as_ref().map(|next| next.heading.clone());

            let same = worker
                .carry_out(Command::ChooseValue {
                    pack: String::from("whitespace"),
                    value: String::from("leading-space"),
                })
                .view
                .expect("the palette shows the new next value");
            assert_eq!(heading(&same).as_deref(), Some("Next: value 2 of 12"));
            assert!(same.messages.is_empty(), "{:?}", same.messages);
            assert!(matches!(same.value, super::ValueBand::Keep));
            assert!(
                store.saved.borrow().is_empty(),
                "the same pack is not saved again"
            );

            let other = worker
                .carry_out(Command::ChooseValue {
                    pack: String::from("locale-pl"),
                    value: String::from("pesel-bad-checksum"),
                })
                .view
                .expect("the palette shows the other pack");
            assert_eq!(worker.sequence.pack_id(), Some("locale-pl"));
            assert!(
                heading(&other).is_some_and(|line| line.starts_with("Next: value 2 of ")),
                "{:?}",
                heading(&other)
            );
            assert!(
                matches!(other.value, super::ValueBand::Clear),
                "the value of the old pack leaves the screen"
            );
            assert_eq!(
                *store.saved.borrow(),
                vec![SettingChange::Pack(String::from("locale-pl"))]
            );

            let gone = worker
                .carry_out(Command::ChooseValue {
                    pack: String::from("locale-pl"),
                    value: String::from("no-such-value"),
                })
                .view
                .expect("the palette says why nothing changed");
            assert_eq!(
                gone.messages,
                vec![i18n::value_gone("no-such-value", "Polish locale")]
            );
            assert_eq!(heading(&gone), heading(&other), "the next value stayed");
        });
    }

    /// The pause is said in every view while it lasts - a pack chosen in the
    /// pack window meanwhile must not take the sentence away - and not after.
    #[test]
    fn while_paused_every_view_says_so_and_after_a_resume_none_does() {
        let store = a_store(false);
        let registrar = Registrar::default();
        with_worker(&store, &registrar, true, |worker, _| {
            worker.carry_out(Command::Pause);

            let chosen = worker
                .carry_out(Command::Choose(String::from("unicode-text")))
                .view
                .expect("another pack is drawn");
            assert_eq!(chosen.messages.first(), Some(&paused_line()));

            let resumed = worker
                .carry_out(Command::Resume)
                .view
                .expect("the palette says what came of taking them again");
            assert!(
                !resumed.messages.contains(&paused_line()),
                "{:?}",
                resumed.messages
            );
            assert!(!worker.hold.paused());
            assert_eq!(registrar.alive(), 1);
        });
    }

    /// 🔴 The whole change: tried alone and released, saved, told to the
    /// window - and registered, with the words naming it, when the window
    /// closes.
    #[test]
    fn a_change_while_paused_is_tried_saved_told_and_taken_on_resume() {
        let store = a_store(false);
        let registrar = Registrar::default();
        with_worker(&store, &registrar, true, |worker, memory| {
            worker.carry_out(Command::Pause);
            let wanted = chord("Alt+Shift+M");

            let carried = worker.carry_out(Command::Shortcut {
                action: HotkeyAction::NextValue,
                chord: Some(wanted),
            });

            assert!(
                carried.view.is_none(),
                "the palette stays as it was until the resume"
            );
            let Some(Told::Shortcut {
                action,
                change: ShortcutChange::Changed { bindings, .. },
                not_saved: None,
            }) = carried.told
            else {
                panic!("the change was not told: {:?}", carried.told);
            };
            assert_eq!(action, HotkeyAction::NextValue);
            assert_eq!(bindings.chord(HotkeyAction::NextValue), wanted);
            assert_eq!(
                registrar.last_asked(),
                vec![(HotkeyAction::NextValue, wanted)],
                "the chord was tried alone"
            );
            assert_eq!(registrar.alive(), 0, "the trial kept the chord");
            assert_eq!(
                *store.saved.borrow(),
                vec![SettingChange::Shortcut {
                    action: HotkeyAction::NextValue,
                    chord: Some(wanted),
                }]
            );

            let resumed = worker
                .carry_out(Command::Resume)
                .view
                .expect("the palette is drawn again");
            assert_eq!(registrar.last_asked(), bindings.as_slice());
            assert_eq!(registrar.alive(), 1);
            assert_eq!(
                memory.bindings.get(),
                bindings,
                "sentences would name the old table"
            );
            assert_eq!(
                resumed.legend,
                Some(legend(&bindings)),
                "the hint bar and the empty value band would name the old shortcut"
            );
            assert!(
                resumed
                    .legend
                    .as_ref()
                    .is_some_and(|words| words.no_value.contains(&i18n::chord(wanted))),
                "{:?}",
                resumed.legend
            );
        });
    }

    /// A chord another program holds is refused, nothing is saved, and the
    /// palette takes back the table it had.
    #[test]
    fn a_chord_another_program_holds_is_refused_and_the_table_stays() {
        let store = a_store(false);
        let registrar = Registrar::default();
        registrar.taken.borrow_mut().push(chord("Alt+Shift+M"));
        with_worker(&store, &registrar, true, |worker, _| {
            worker.carry_out(Command::Pause);

            let told = worker
                .carry_out(Command::Shortcut {
                    action: HotkeyAction::NextValue,
                    chord: Some(chord("Alt+Shift+M")),
                })
                .told;

            assert_eq!(
                told,
                Some(Told::Shortcut {
                    action: HotkeyAction::NextValue,
                    change: ShortcutChange::Taken {
                        chord: chord("Alt+Shift+M")
                    },
                    not_saved: None,
                })
            );
            assert!(store.saved.borrow().is_empty());
            worker.carry_out(Command::Resume);
            assert_eq!(
                registrar.last_asked(),
                nkb_adapters::default_bindings().as_slice()
            );
        });
    }

    /// A resume says what the system would not give, chord by chord - and
    /// remembers it for the next time the window opens.
    #[test]
    fn a_resume_says_what_is_taken_and_the_next_pause_tells_the_window() {
        let store = a_store(false);
        let registrar = Registrar::default();
        let previous = nkb_adapters::default_bindings().chord(HotkeyAction::PreviousValue);
        with_worker(&store, &registrar, true, |worker, _| {
            worker.carry_out(Command::Pause);
            registrar.taken.borrow_mut().push(previous);

            let resumed = worker.carry_out(Command::Resume).view.expect("drawn");
            assert_eq!(
                resumed.messages,
                vec![
                    i18n::registration(
                        &ShortcutRegistration::Taken,
                        HotkeyAction::PreviousValue,
                        previous
                    )
                    .expect("a taken shortcut is said")
                ]
            );

            let Some(Told::Paused(now)) = worker.carry_out(Command::Pause).told else {
                panic!("a pause tells the window the table");
            };
            assert!(now.registered.contains(&(
                HotkeyAction::PreviousValue,
                previous,
                ShortcutRegistration::Taken
            )));
        });
    }

    /// Nothing is registered twice: a resume while held changes nothing and
    /// says nothing.
    #[test]
    fn a_resume_while_the_shortcuts_are_held_does_nothing() {
        let store = a_store(false);
        let registrar = Registrar::default();
        with_worker(&store, &registrar, true, |worker, _| {
            let asked = registrar.asked.borrow().len();
            let carried = worker.carry_out(Command::Resume);
            assert!(carried.view.is_none() && carried.told.is_none());
            assert_eq!(registrar.asked.borrow().len(), asked);
            assert_eq!(registrar.alive(), 1);
        });
    }

    /// Not held because nothing could be registered is a state the worker
    /// stays in, saying why - and the next resume tries again. Until K5.3 the
    /// worker ended there.
    #[test]
    fn with_nothing_registrable_the_worker_says_why_and_a_resume_tries_again() {
        let store = a_store(false);
        let registrar = Registrar::default();
        registrar.unavailable.set(true);
        with_worker(&store, &registrar, false, |worker, memory| {
            let lines = worker.take(memory.bindings.get());
            assert_eq!(
                lines,
                vec![i18n::shortcuts_unavailable(
                    &ShortcutsUnavailable::CouldNotStart
                )]
            );
            assert!(worker.hold.live().is_none());
            assert!(
                !worker.hold.paused(),
                "lost is not paused, and must not say it is"
            );

            let chosen = worker
                .carry_out(Command::Choose(String::from("unicode-text")))
                .view
                .expect("a pack is still chosen");
            assert_eq!(chosen.pack, "Unicode and text");

            registrar.unavailable.set(false);
            worker.carry_out(Command::Resume);
            assert!(worker.hold.live().is_some(), "the resume did not try again");
        });
    }

    /// The window pauses before it changes anything. If a change ever came
    /// while held, what is held would stop being the table in effect - so it
    /// is taken again at once, the old set released first.
    #[test]
    fn a_change_while_held_is_taken_at_once_releasing_the_old_set_first() {
        let store = a_store(false);
        let registrar = Registrar::default();
        with_worker(&store, &registrar, true, |worker, _| {
            let carried = worker.carry_out(Command::Shortcut {
                action: HotkeyAction::NextValue,
                chord: Some(chord("Alt+Shift+M")),
            });
            let view = carried.view.expect("the palette follows the new table");
            assert!(view.legend.is_some());
            assert_eq!(
                registrar
                    .last_asked()
                    .iter()
                    .find(|(action, _)| *action == HotkeyAction::NextValue)
                    .map(|(_, chord)| *chord),
                Some(chord("Alt+Shift+M"))
            );
            assert_eq!(
                registrar.alive_when_asked.borrow().last(),
                Some(&0),
                "the old set still held its chords when the new one was asked for"
            );
            assert_eq!(registrar.alive(), 1);
        });
    }

    /// The hint bar names every shortcut that does something, in the order a
    /// person reads them (`D120`), and none that does not (`UX-GUI-007`) - and
    /// the empty value band
    /// names the one that sends a value.
    #[test]
    fn the_legend_names_every_working_shortcut_of_the_table_in_effect() {
        let defaults = nkb_adapters::default_bindings();
        let words = legend(&defaults);
        // Worded through `i18n::chord`, because the table is this system's
        // own and macOS names its modifiers differently.
        let hint = |action| {
            (
                i18n::chord(defaults.chord(action)),
                i18n::action_name(action).to_owned(),
            )
        };
        assert_eq!(
            words.hints,
            vec![
                hint(HotkeyAction::NextValue),
                hint(HotkeyAction::PreviousValue),
                hint(HotkeyAction::SkipValue),
                hint(HotkeyAction::BackOneValue),
                hint(HotkeyAction::RestartPack),
                hint(HotkeyAction::NextPack),
                hint(HotkeyAction::PreviousPack),
                hint(HotkeyAction::OpenPacks),
                hint(HotkeyAction::CopyReport),
                hint(HotkeyAction::ToggleVisibility),
            ]
        );
        assert_eq!(
            words.no_value,
            i18n::no_value_yet(defaults.chord(HotkeyAction::NextValue))
        );
    }

    /// While nothing is held the worker waits on the channel: a command
    /// wakes it, a closing window comes first, and a channel nobody sends on
    /// is not a reason to stop.
    #[test]
    fn with_nothing_held_a_command_wakes_the_worker_and_the_stop_comes_first() {
        let (send, commands) = mpsc::channel();
        let stop = AtomicBool::new(false);
        send.send(Command::Resume).expect("the receiver is alive");
        assert!(matches!(
            next_command(&stop, &commands),
            Next::Command(Command::Resume)
        ));

        send.send(Command::Resume).expect("the receiver is alive");
        stop.store(true, Ordering::Relaxed);
        assert!(matches!(next_command(&stop, &commands), Next::Stop));
        assert_eq!(
            commands.try_recv(),
            Ok(Command::Resume),
            "a closing palette read a command"
        );

        drop(send);
        assert!(matches!(next_command(&stop, &commands), Next::Stop));
    }
}
