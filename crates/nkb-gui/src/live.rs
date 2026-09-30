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
//! ⚠️ A command is noticed at the end of the current wait, up to one [`TICK`]
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
use nkb_app::advance_sequence::{Ports, RouteRequest, Sent};
use nkb_app::ports::{
    HotkeyRegistrar, LiveShortcuts, SettingChange, Settings, SettingsStore, ShortcutRegistration,
    Wait,
};
use nkb_app::{
    AdvanceSequence, Ended, KeptSettings, Opening, Outcome, SettingsMessage, ShortcutChange,
    drive_sequence,
};
use nkb_core::hotkeys::{Bindings, HotkeyAction, HotkeyChord};
use nkb_core::preview::ValuePreview;
use nkb_core::report::Arrival;
use nkb_core::sequence::Delivery;
use nkb_core::typeface::outside_guarantee;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel, Weak};

use crate::clipboard::SystemClipboard;
use crate::focus::{Standing, standing_line};
use crate::typeface::SHIPPED;
use crate::{HintRow, Marker, Palette};

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
    value: ValueBand,
    messages: Vec<String>,
    /// The standing clipboard bar and its words, or `None` when values are
    /// typed. Two conditions share the one bar: the mode the sequence is in,
    /// and the window in front taking no typing (`D72`). The words differ, so
    /// the view carries them rather than a flag.
    clipboard_bar: Option<&'static str>,
    /// The words naming the shortcuts, when the table in effect changed with
    /// this view - `None` leaves the ones on screen.
    legend: Option<Legend>,
}

/// What the value band does with this view.
enum ValueBand {
    /// This turn produced no value - a warning, a refusal, a press that
    /// arrived while busy. The palette KEEPS THE PREVIOUS VALUE on screen:
    /// blanking it would take away the thing the message is about.
    Keep,
    /// Another pack is in use now. The value on screen came from the one
    /// before, and under the new pack's name it would be a false statement.
    Clear,
    /// A value went out.
    Show(ValueView),
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

/// Which shortcuts the hint bar names, and in this order.
///
/// Four of the ten, because `ux-spec.md` 2 gives the hint bar four and because a
/// list of ten stops being a hint. These four are the ones the first five
/// minutes need: move through the pack, and get the report out.
///
/// `OpenPacks` opens the pack window (step 7, K3.2c): the worker takes the
/// press before the sequence sees it and asks the main thread through the
/// palette's `open-packs` callback.
const HINTED: [HotkeyAction; 4] = [
    HotkeyAction::NextValue,
    HotkeyAction::PreviousValue,
    HotkeyAction::CopyReport,
    HotkeyAction::OpenPacks,
];

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
        hints: bindings
            .as_slice()
            .iter()
            .filter(|(action, _)| HINTED.contains(action))
            .map(|(action, chord)| (i18n::chord(*chord), i18n::action_name(*action).to_owned()))
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

/// The identifier of the pack in use, published by the worker for the pack
/// window, which marks it and opens on it.
///
/// A shared slot rather than a message, for the reason [`Standing`] gives: the
/// window reads it whenever it opens, which may be long after the last change.
/// The worker is the one writer, because the sequence that knows the answer
/// lives there.
pub type InUse = Arc<Mutex<Option<String>>>;

/// A fresh slot, empty until the worker opens a pack.
#[must_use]
pub fn in_use() -> InUse {
    Arc::new(Mutex::new(None))
}

/// The pack in use, as last published. A poisoned lock answers `None` - the
/// window then marks no row, which is a smaller failure than a crash.
#[must_use]
pub fn in_use_now(in_use: &InUse) -> Option<String> {
    in_use.lock().ok().and_then(|held| held.clone())
}

fn publish(in_use: &InUse, sequence: &AdvanceSequence) {
    if let Ok(mut held) = in_use.lock() {
        *held = sequence.pack_id().map(str::to_owned);
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
            Opening::Settings(message) => {
                opening.extend(i18n::settings_message(message, &file, &bindings));
            }
        }
    }
    publish(in_use, &sequence);

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
    };
    // With nothing registered the palette says why and stays up, and the
    // worker keeps serving the pack window. Untouchable rule 1: a run that did
    // less than it promised says so, rather than looking alive.
    opening.extend(worker.take(bindings));
    // No legend: the main thread drew it from the same table before the
    // window was shown.
    show(palette, worker.view(opening, ValueBand::Keep, None));

    let collapse = Collapse::new(&memory);
    let on_toggle = || {
        let (compact, line) = collapse.toggle();
        set_compact_later(palette, compact);
        if let Some(line) = line {
            say_later(palette, line);
        }
    };
    let on_open = || open_packs_later(palette);
    let pending = Cell::new(None);
    loop {
        let next = match worker.hold.live() {
            Some(live) => {
                let shortcuts = PaletteShortcuts {
                    inner: live,
                    on_toggle: &on_toggle,
                    on_open: &on_open,
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
                let carried = worker.carry_out(command);
                if let Some(view) = carried.view {
                    show(palette, view);
                }
                if let Some(told) = carried.told {
                    tell(told);
                }
                publish(in_use, &worker.sequence);
            }
        }
    }
    // Before anything else: the shortcuts go back to the system. Holding them
    // after the palette is gone would take `Alt+Shift+N` away from whoever
    // wants it next.
    drop(worker);
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
}

impl Worker<'_> {
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
            },
            Command::Pause => {
                self.hold.pause();
                Carried {
                    view: Some(self.view(Vec::new(), ValueBand::Keep, None)),
                    told: Some(Told::Paused(ShortcutsNow {
                        bindings: self.in_effect().0,
                        registered: self.hold.registered.clone(),
                    })),
                }
            }
            // Held already: the window did not pause, or a second close came.
            // Taking them again would say every line about them twice.
            Command::Resume if self.hold.live().is_some() => Carried {
                view: None,
                told: None,
            },
            Command::Resume => Carried {
                view: Some(self.resume()),
                told: None,
            },
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

/// Whether the loop goes on: no while the window is closing, and no while a
/// command waits - which is then kept in `pending` for [`drive`] to carry out.
///
/// The stop is read FIRST, so a palette that is closing leaves a command
/// unread rather than choosing a pack nobody will see.
fn between_presses(
    stop: &AtomicBool,
    commands: &Receiver<Command>,
    pending: &Cell<Option<Command>>,
) -> bool {
    if stop.load(Ordering::Relaxed) {
        return false;
    }
    match commands.try_recv() {
        Ok(command) => {
            pending.set(Some(command));
            false
        }
        // Nothing waiting, or nobody left to send: the stop flag, not this
        // channel, says when the palette is closing.
        Err(_) => true,
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
    if sequence.pack_id() == Some(asked) {
        return None;
    }
    let view = match sequence.choose_pack(&BuiltInCatalogue::new(), &TomlPackFormat, asked) {
        Ok(()) => {
            asked.clone_into(pack);
            let said = memory.keep(SettingChange::Pack(asked.to_owned()));
            view_between(
                sequence,
                pack,
                said.into_iter().collect(),
                ValueBand::Clear,
                standing,
                paused,
            )
        }
        Err(error) => view_between(
            sequence,
            pack,
            vec![i18n::choose_error(&error, asked)],
            ValueBand::Keep,
            standing,
            paused,
        ),
    };
    Some(view)
}

/// The name the palette shows for the pack: its own name when it opened, the
/// identifier that was tried when nothing did.
fn shown(sequence: &AdvanceSequence, pack: &str) -> String {
    sequence.pack_name().unwrap_or(pack).to_owned()
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

/// A view made between presses, with no outcome behind it: the palette
/// opening, a pack chosen, the shortcuts paused, taken again or gone.
///
/// While the shortcuts are paused every such view says so, first among its
/// own lines and behind the standing sentence: the band is rebuilt from
/// scratch each time, so a line said once would last exactly one view - and a
/// pack chosen in the pack window during the pause would take it away.
fn view_between(
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
        value,
        messages: with_standing(messages, standing),
        clipboard_bar: clipboard_bar(
            sequence.sequence().delivery,
            sequence.clipboard_for_window(),
        ),
        legend: None,
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

fn view_of(
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
        value: outcome
            .sent
            .as_ref()
            .map_or(ValueBand::Keep, |sent| ValueBand::Show(value_view(sent))),
        messages: with_standing(
            outcome
                .messages
                .iter()
                .map(|message| i18n::message(message, pack, bindings))
                .collect(),
            standing,
        ),
        clipboard_bar: clipboard_bar(outcome.sequence.delivery, outcome.clipboard_for_window),
        // An outcome comes from a press, and a press never changes the table.
        legend: None,
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
    if let Some(legend) = view.legend {
        show_legend(palette, legend);
    }

    match view.value {
        ValueBand::Keep => {}
        // The band goes back to what it said before anything was sent, which
        // is true again: nothing has gone out of this pack yet.
        ValueBand::Clear => palette.set_has_value(false),
        ValueBand::Show(value) => show_value(palette, value),
    }

    // 🔴 No timer and no state change here, and that is `D83`: until then every
    // view woke the palette and a timer put it back to rest four seconds later,
    // taking the value with it. Whether the palette is compact is the tester's
    // choice alone - see `Collapse`.
}

/// The value band, on the main thread.
fn show_value(palette: &Palette, value: ValueView) {
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

/// The palette's own shortcuts in front of the sequence.
///
/// `ToggleVisibility` and `OpenPacks` are about windows, not about the pack in
/// use, so they never reach `AdvanceSequence` - which would answer them with
/// `Unhandled`. Every other press passes through untouched, in order.
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
    /// What an `OpenPacks` press does - asks the main thread to open the pack
    /// window. A press during a send is answered after it, like a toggle: the
    /// window opens once the value is in, and no value is replayed.
    on_open: &'a dyn Fn(),
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
                Wait::Pressed(HotkeyAction::OpenPacks) => (self.on_open)(),
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

    use nkb_app::ValueFacts;
    use nkb_app::advance_sequence::{Message, Sent};
    use nkb_core::report::Arrival;
    use nkb_core::sequence::{Position, Sequence};
    use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
    use slint::platform::{Platform, PlatformError, WindowAdapter};

    use std::cell::{Cell, RefCell};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc;

    use nkb_adapters::{BuiltInCatalogue, TomlPackFormat, i18n};
    use nkb_app::advance_sequence::ChooseError;
    use nkb_app::ports::SettingChange;
    use nkb_app::{AdvanceSequence, KeptSettings};

    use super::{
        Command, Delivery, Duration, HotkeyAction, LiveShortcuts, Memory, Outcome, Palette,
        PaletteShortcuts, ShortcutRegistration, Standing, ValueBand, ValuePreview, Wait, apply,
        between_presses, choose, clipboard_bar, hold_height, markers_of, set_compact, view_between,
        view_of, with_standing,
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
                &nkb_adapters::default_bindings(),
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
                &nkb_adapters::default_bindings(),
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

    fn a_store(fail: bool) -> Remembered {
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

    /// A waiting command ends the turn and is kept for `drive`. A closing
    /// window comes first and leaves the command unread. A channel nobody
    /// sends on any more is not a reason to stop.
    #[test]
    fn a_waiting_command_ends_the_turn_but_a_closing_window_comes_first() {
        let (send, commands) = mpsc::channel();
        let stop = AtomicBool::new(false);
        let pending = Cell::new(None);
        assert!(between_presses(&stop, &commands, &pending), "nothing waits");

        send.send(Command::Choose(String::from("x")))
            .expect("the receiver is alive");
        assert!(!between_presses(&stop, &commands, &pending));
        assert_eq!(pending.take(), Some(Command::Choose(String::from("x"))));

        send.send(Command::Choose(String::from("y")))
            .expect("the receiver is alive");
        stop.store(true, Ordering::Relaxed);
        assert!(!between_presses(&stop, &commands, &pending));
        assert_eq!(pending.take(), None, "a closing palette chose a pack");
        assert_eq!(commands.try_recv(), Ok(Command::Choose(String::from("y"))));

        stop.store(false, Ordering::Relaxed);
        drop(send);
        assert!(
            between_presses(&stop, &commands, &pending),
            "a channel nobody sends on stopped the palette"
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
                HotkeyAction::OpenPacks,
                HotkeyAction::NextValue,
                HotkeyAction::ToggleVisibility,
            ]
            .into(),
        ));
        let toggles = std::cell::Cell::new(0);
        let on_toggle = || toggles.set(toggles.get() + 1);
        let opens = std::cell::Cell::new(0);
        let on_open = || opens.set(opens.get() + 1);
        let shortcuts = PaletteShortcuts {
            inner: &queued,
            on_toggle: &on_toggle,
            on_open: &on_open,
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
        assert_eq!(opens.get(), 1, "the pack window was asked for, once");
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
    struct Registrar {
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
    fn with_worker<R>(
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
            clipboard: &clipboard,
            report_text: &EnglishReport,
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

    /// The hint bar names four shortcuts in the table's order, and the empty
    /// value band names the one that sends a value.
    #[test]
    fn the_legend_names_the_hinted_shortcuts_of_the_table_in_effect() {
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
                hint(HotkeyAction::CopyReport),
                hint(HotkeyAction::OpenPacks),
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
