//! The shortcuts window on the main thread: asked for with a click, opened
//! once the worker has given the palette's shortcuts back, closed with the
//! keyboard handed back and the shortcuts taken again (K5.5).
//!
//! # The order of opening, and why it waits for the worker
//!
//! A registered shortcut reaches no window at all, this one included, and one
//! pressed here would send a value into it (`ux-spec.md` 5.4). So a click
//! first asks the worker to PAUSE, and the window shows when the worker
//! answers with the table it edits (`live::Told::Paused`). Shown earlier, it
//! would say "paused" while the shortcuts were still held, and show a table
//! the worker may have changed since (`D90`). The wait is at most one tick of
//! the worker, or the end of a value being typed.
//!
//! # What runs where
//!
//! All of it on the MAIN thread, like `packs`. The worker's answers arrive
//! through [`tell`], which hands each one to this thread's event loop, where
//! the window of this thread is found again. What a key does and what a row
//! shows is [`ShortcutList`]'s. Nothing here needs a screen to be tested: the
//! keyboard and the system's answers are traits, the worker a channel.
//!
//! # Closing
//!
//! Esc, the close button or `Alt+F4` hand the keyboard back, hide the window,
//! and ask the worker to take the shortcuts again - with the table the
//! settings give now. Leaving for another window closes too, without handing
//! the keyboard back, for the reason `packs` gives (`OBS-147`): a window that
//! stayed open would keep the palette's shortcuts paused where nobody sees it.

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::sync::mpsc::Sender;
use std::time::Instant;

use nkb_adapters::i18n::{self, Environment, ShortcutsLabel, Startup};
use nkb_adapters::{ChordHeld, default_bindings};
use nkb_app::{Ended, ShortcutChange};
use nkb_core::hotkeys::HotkeyAction;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::focus::{HANDLE_WAIT, POLL, window_handle};
use crate::live::{Command, ShortcutsNow, Tell, Told};
use crate::packs::{Keyboard, forget_held_modifiers};
use crate::shortcut_list::{Act, ShortcutList, ShortcutRow, key_of};
use crate::{HintRow, PickRow, ShortcutsWindow};

/// What the window asks of the system besides the keyboard focus: which chord
/// is held (`D91`), and keeping its menu shut on `Alt+Space`. A trait, so what
/// the window does with the answers is tested without a system.
pub trait SystemKeys {
    /// The chord held now - asked while a key press is being handled.
    fn chord_held(&self) -> ChordHeld;
    /// Keeps the window's menu from opening from the keyboard.
    ///
    /// # Errors
    ///
    /// Why it could not, in the system layer's words.
    fn keep_menu_shut(&self, window: Option<u64>) -> Result<(), String>;
}

/// The system, through the adapter.
#[derive(Debug, Default, Clone, Copy)]
pub struct System;

impl SystemKeys for System {
    fn chord_held(&self) -> ChordHeld {
        nkb_adapters::chord_held()
    }

    fn keep_menu_shut(&self, window: Option<u64>) -> Result<(), String> {
        nkb_adapters::no_keyboard_menu(window).map_err(|error| error.to_string())
    }
}

thread_local! {
    /// The shortcuts window of THIS thread - the main one - for the worker's
    /// answers to find. The worker holds only a closure that can cross threads,
    /// and the window cannot: it lives in an `Rc`, like every Slint handle.
    static WINDOW: RefCell<Weak<Shortcuts>> = const { RefCell::new(Weak::new()) };
}

/// How the worker tells the shortcuts window its answers: each is handed to
/// the main thread's event loop, where the window registered by
/// [`Shortcuts::new`] takes it.
///
/// The result of the hand-over is dropped for the reason `live::show` gives:
/// after the loop has quit it reports success for work nobody does, and there
/// is no window left to tell.
#[must_use]
pub fn tell() -> Tell {
    Box::new(|told| {
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(window) = WINDOW.with(|slot| slot.borrow().upgrade()) {
                window.told(told);
            }
        });
    })
}

/// The shortcuts window and what it shows while it is open.
pub struct Shortcuts {
    window: ShortcutsWindow,
    /// `Some` while the window is open, built from the table the worker told
    /// on pausing.
    list: RefCell<Option<ShortcutList>>,
    /// A pause asked for and not answered yet: the window shows on the answer.
    opening: Cell<bool>,
    keyboard: Box<dyn Keyboard>,
    keys: Box<dyn SystemKeys>,
    commands: Sender<Command>,
    /// Puts a line in the palette's message band.
    say: Box<dyn Fn(String)>,
}

impl Shortcuts {
    /// The window, labelled and wired, not yet shown - and registered as the
    /// one [`tell`] delivers to.
    #[must_use]
    pub fn new(
        window: ShortcutsWindow,
        keyboard: Box<dyn Keyboard>,
        keys: Box<dyn SystemKeys>,
        commands: Sender<Command>,
        say: Box<dyn Fn(String)>,
    ) -> Rc<Self> {
        label(&window);
        let shortcuts = Rc::new(Self {
            window,
            list: RefCell::new(None),
            opening: Cell::new(false),
            keyboard,
            keys,
            commands,
            say,
        });
        WINDOW.with(|slot| *slot.borrow_mut() = Rc::downgrade(&shortcuts));
        let weak = Rc::downgrade(&shortcuts);
        shortcuts
            .window
            .on_pressed(move |text, _control, _alt, _shift, _meta| {
                // The flags are ignored on purpose: they go stale after the
                // window menu swallowed a release, and a chord is read from
                // the system instead (`D91`).
                weak.upgrade()
                    .is_some_and(|shortcuts| shortcuts.press(text.as_str()))
            });
        // A release means nothing here: the chord is read at the press.
        shortcuts.window.on_released(|_| false);
        let weak = Rc::downgrade(&shortcuts);
        shortcuts.window.on_clicked(move |row| {
            if let Some(shortcuts) = weak.upgrade()
                && let Ok(row) = usize::try_from(row)
            {
                let act = shortcuts.with_list(|list| list.click(row));
                shortcuts.act(act.unwrap_or(Act::Handled));
            }
        });
        let weak = Rc::downgrade(&shortcuts);
        shortcuts.window.on_deactivated(move || {
            if let Some(shortcuts) = weak.upgrade() {
                shortcuts.left();
            }
        });
        let weak = Rc::downgrade(&shortcuts);
        shortcuts.window.on_activated(move || {
            if let Some(shortcuts) = weak.upgrade() {
                forget_held_modifiers(shortcuts.window.window());
            }
        });
        let weak = Rc::downgrade(&shortcuts);
        shortcuts.window.window().on_close_requested(move || {
            if let Some(shortcuts) = weak.upgrade() {
                shortcuts.close();
            }
            // Hidden already, after the keyboard went back - see `close`.
            slint::CloseRequestResponse::KeepWindowShown
        });
        shortcuts
    }

    /// Whether the window is open.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.list.borrow().is_some()
    }

    /// Asks for the window: the worker pauses first, and the window shows on
    /// its answer. Asked again while open, it asks for the keyboard again.
    pub fn open(self: &Rc<Self>) {
        if self.is_open() {
            self.window.invoke_focus_keys();
            self.take_keyboard(Instant::now());
            return;
        }
        if self.opening.get() {
            return;
        }
        // Before the window exists: afterwards the answer is the window.
        self.keyboard.remember();
        self.opening.set(true);
        if self.commands.send(Command::Pause).is_err() {
            self.opening.set(false);
            self.worker_gone();
        }
    }

    /// Hands the keyboard back, hides the window and asks the worker to take
    /// the shortcuts again. In that order, for the reason `Packs::close`
    /// gives: once hidden, the system has already given the foreground away.
    pub fn close(&self) {
        if self.list.borrow().is_none() {
            return;
        }
        if let Err(reason) = self.keyboard.give_back(window_handle(self.window.window())) {
            (self.say)(i18n::environment(Environment::FocusNotReturned, &reason));
        }
        self.shut();
    }

    /// The window stopped being the active one: the tester is elsewhere, and
    /// the keyboard stays where they put it.
    fn left(&self) {
        if self.list.borrow().is_none() {
            return;
        }
        self.shut();
    }

    /// Hides the window and takes the shortcuts back - every way out.
    fn shut(&self) {
        let _ = self.window.hide();
        *self.list.borrow_mut() = None;
        if self.commands.send(Command::Resume).is_err() {
            self.worker_gone();
        }
    }

    /// One answer from the worker, on the main thread.
    pub fn told(self: &Rc<Self>, told: Told) {
        match told {
            Told::Paused(now) => self.paused(now),
            Told::Shortcut {
                action,
                change,
                not_saved,
            } => {
                let open = self.with_list(|list| list.answer(action, &change, not_saved.clone()));
                if open.is_some() {
                    self.show_all();
                } else {
                    self.told_after_closing(action, &change, not_saved);
                }
            }
        }
    }

    /// An answer that arrived after the window closed - Esc pressed right
    /// after the chord. What it came to goes where the tester looks next, the
    /// palette: a change kept, or not kept, is news either way. "Already so"
    /// is not, and says nothing.
    fn told_after_closing(
        &self,
        action: HotkeyAction,
        change: &ShortcutChange,
        not_saved: Option<String>,
    ) {
        let (asked, answers) = match change {
            ShortcutChange::AlreadySo => return,
            // The window is gone with what it asked, so a change to the
            // default reads as a restore and any other as a recording.
            ShortcutChange::Changed { bindings, .. } => {
                let now = bindings.chord(action);
                (
                    (now != default_bindings().chord(action)).then_some(now),
                    now,
                )
            }
            ShortcutChange::Refused(_)
            | ShortcutChange::Taken { .. }
            | ShortcutChange::NotRegistered { .. } => (None, default_bindings().chord(action)),
        };
        for line in i18n::shortcut_answer(action, asked, change, answers)
            .into_iter()
            .chain(not_saved)
        {
            (self.say)(line);
        }
    }

    /// The worker paused: the window shows, with the table it told.
    fn paused(self: &Rc<Self>, now: ShortcutsNow) {
        if !self.opening.replace(false) {
            // Nobody is opening - the window closed before the answer came, or
            // this answer is stale. The shortcuts must not stay paused behind
            // a window nobody sees.
            if !self.is_open() && self.commands.send(Command::Resume).is_err() {
                self.worker_gone();
            }
            return;
        }
        *self.list.borrow_mut() = Some(ShortcutList::new(default_bindings(), now));
        self.show_all();
        if let Err(error) = self.window.show() {
            *self.list.borrow_mut() = None;
            (self.say)(i18n::startup_failure(
                Startup::WindowFailed,
                &error.to_string(),
            ));
            if self.commands.send(Command::Resume).is_err() {
                self.worker_gone();
            }
            return;
        }
        self.window.invoke_focus_keys();
        self.take_keyboard(Instant::now());
    }

    /// Asks for the keyboard - and keeps the menu shut - as soon as the window
    /// has a handle, the same poll as the pack window's.
    fn take_keyboard(self: &Rc<Self>, started: Instant) {
        let handle = window_handle(self.window.window());
        if handle.is_none() && started.elapsed() < HANDLE_WAIT {
            let weak = Rc::downgrade(self);
            slint::Timer::single_shot(POLL, move || {
                if let Some(shortcuts) = weak.upgrade()
                    && shortcuts.is_open()
                {
                    shortcuts.take_keyboard(started);
                }
            });
            return;
        }
        self.take_with(handle);
    }

    /// What the poll ends in. Public so a test with no handle to wait for
    /// reaches the same code.
    ///
    /// The menu first: it is shut on every opening, because a window shown
    /// again is a new system window (`OBS-80`).
    pub fn take_with(&self, handle: Option<u64>) {
        if let Err(reason) = self.keys.keep_menu_shut(handle) {
            (self.say)(i18n::environment(Environment::ShortcutsMenuOpen, &reason));
        }
        if let Err(reason) = self.keyboard.take(handle) {
            (self.say)(i18n::environment(
                Environment::ShortcutsFocusNotTaken,
                &reason,
            ));
        }
    }

    fn press(&self, text: &str) -> bool {
        let key = key_of(text);
        let Some(act) = self.with_list(|list| list.press(key, &|| self.keys.chord_held())) else {
            return false;
        };
        let ours = act != Act::NotOurs;
        self.act(act);
        ours
    }

    fn act(&self, act: Act) {
        match act {
            Act::NotOurs => {}
            Act::Handled => self.show_all(),
            Act::Ask { action, chord } => {
                if self
                    .commands
                    .send(Command::Shortcut { action, chord })
                    .is_err()
                {
                    self.worker_gone();
                }
                self.show_all();
            }
            Act::Close => self.close(),
        }
    }

    /// The worker is gone only when the process is ending - said anyway, so
    /// nothing asked for is lost without a word.
    fn worker_gone(&self) {
        if let Some(line) = i18n::ended(Ended::ShortcutsGone) {
            (self.say)(line);
        }
    }

    fn with_list<T>(&self, act: impl FnOnce(&mut ShortcutList) -> T) -> Option<T> {
        self.list.borrow_mut().as_mut().map(act)
    }

    /// Everything the list decides, onto the window.
    fn show_all(&self) {
        let list = self.list.borrow();
        let Some(list) = list.as_ref() else {
            return;
        };
        self.window.set_rows(ModelRc::new(VecModel::from(
            list.rows().iter().map(pick_row).collect::<Vec<_>>(),
        )));
        self.window
            .set_selected(i32::try_from(list.selected()).unwrap_or(-1));
        self.window.set_summary(list.summary().into());
        self.window.set_messages(ModelRc::new(VecModel::from(
            list.messages()
                .iter()
                .map(|line| SharedString::from(line.as_str()))
                .collect::<Vec<_>>(),
        )));
    }

    /// The window, for a test that reads what it shows.
    #[must_use]
    pub fn window(&self) -> &ShortcutsWindow {
        &self.window
    }
}

/// Every word that does not change while the window is open.
fn label(window: &ShortcutsWindow) {
    window.set_window_title(i18n::shortcuts_label(ShortcutsLabel::Title).into());
    window.set_heading(i18n::shortcuts_label(ShortcutsLabel::Heading).into());
    window.set_intro(i18n::shortcuts_label(ShortcutsLabel::Intro).into());
    window.set_current_label(i18n::shortcuts_label(ShortcutsLabel::Recording).into());
    let hints: Vec<HintRow> = [
        (ShortcutsLabel::KeyEnter, ShortcutsLabel::Record),
        (ShortcutsLabel::KeyDelete, ShortcutsLabel::Restore),
        (ShortcutsLabel::KeyArrows, ShortcutsLabel::Move),
        (ShortcutsLabel::KeyEscape, ShortcutsLabel::Close),
    ]
    .into_iter()
    .map(|(key, action)| HintRow {
        key: i18n::shortcuts_label(key).into(),
        action: i18n::shortcuts_label(action).into(),
    })
    .collect();
    window.set_hints(ModelRc::new(VecModel::from(hints)));
}

/// One row as the window draws it. The only place a [`ShortcutRow`] becomes a
/// [`PickRow`].
fn pick_row(row: &ShortcutRow) -> PickRow {
    PickRow {
        title: row.title.as_str().into(),
        detail: SharedString::new(),
        badge: row
            .badge
            .as_ref()
            .map_or_else(SharedString::new, |badge| badge.text.as_str().into()),
        has_badge: row.badge.is_some(),
        badge_risky: row.badge.as_ref().is_some_and(|badge| badge.risky),
        current: row.current,
        // Every action can be recorded.
        enabled: true,
        // A shortcut is a name and a key combination, on one line.
        single_line: true,
        key: row.key.as_str().into(),
        has_key: true,
    }
}
