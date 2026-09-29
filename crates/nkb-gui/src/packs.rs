//! The pack window on the main thread: opened on request, filled from
//! [`PackPicker`], closed with the keyboard handed back.
//!
//! # The one moment the tool takes the keyboard
//!
//! `ux-spec.md` 5.2: the pack search is the only moment the tool holds the
//! keyboard focus, and on closing the focus goes back to the field it was taken
//! from - or the tool says it could not. The palette refuses the focus for the
//! whole run (`D62`), so this is a window of its own.
//!
//! 🔴 Showing the window does not give it the keyboard, and that is measured
//! rather than read: `tools/sonda-okno-paczek`, 2026-09-29, the shortcut sent
//! from a third process. Shown alone the window stayed behind the application
//! under test 3 times out of 3 and the letters meant for the search went into
//! the field under test. Asking for the foreground on this thread after the
//! shortcut worked 3 out of 3, and handing it back BEFORE hiding put the next
//! letter in the field again 3 out of 3. So opening asks and reads back, and
//! closing hands back and then hides - in that order.
//!
//! # What runs where
//!
//! All of it on the MAIN thread, like `focus`. The worker asks for the window
//! through the palette's `open-packs` callback and learns the choice through
//! [`Command`]. The pack in use comes the other way, through [`InUse`].
//!
//! # What is decided here, and what is not
//!
//! Which packs match, which row is selected and what Enter comes to are
//! [`PackPicker`]'s. This module moves its answers onto the window and acts on
//! them - and nothing in it needs a screen to be tested: the keyboard is a
//! trait, the worker a channel, the palette's message band a closure.
//!
//! # Leaving it for another window
//!
//! A tester who clicks another window, or switches away, has put the keyboard
//! there. The window closes and does NOT hand the keyboard back - handing it
//! back would take it from where the tester just put it (`OBS-147`). Before
//! this it stayed on top without the keyboard, and once, after a real
//! shortcut, without a word either (`OBS-149`). Slint reports the change as the
//! query line losing focus for `window-activation`, the only reason the view
//! passes on.
//!
//! # Coming in while the shortcut is still held
//!
//! 🔴 The window can get the keyboard of its own thread before it gets the
//! foreground: `show()` activates it there, and winit then reports every key
//! held at that moment as pressed - the shortcut's Alt and Shift. The tester
//! lets go while the application under test is still in front, so the
//! releases go there, and Slint keeps believing the keys held. It forgets
//! modifiers when a window goes inactive, never when one becomes active, and
//! it keeps them for the whole process. Once the foreground arrives, every
//! letter of the search came with Alt and the query line took it for a
//! shortcut - the tester typed and nothing happened, without a word
//! (`OBS-151`, `slint.md` 2.34, read in winit 0.30.13 and i-slint-core
//! 1.18.1). So the window forgets them itself when it becomes active: see
//! [`forget_held_modifiers`].

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::mpsc::Sender;
use std::time::Instant;

use nkb_adapters::i18n::{self, Environment, PacksLabel, Startup};
use nkb_adapters::{BuiltInCatalogue, LentFocus, TomlPackFormat};
use nkb_app::{Ended, list_packs};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::focus::{HANDLE_WAIT, POLL, window_handle};
use crate::live::{Command, InUse, in_use_now};
use crate::picker::{Chosen, PackPicker, Row};
use crate::query::{KeyPress, Pressed};
use crate::{HintRow, PacksWindow, PickRow};

/// Moving the keyboard to the pack window and back.
///
/// A trait so what the window does with the answers is tested without a
/// system that has windows to move between. The errors are the system layer's
/// own words, because they go into a sentence as its reason.
pub trait Keyboard {
    /// Remembers who holds the keyboard - before the window shows, because
    /// afterwards the answer is the window itself.
    fn remember(&self);
    /// Takes the keyboard for `window`, the raw handle of the pack window.
    ///
    /// # Errors
    ///
    /// Why it stayed elsewhere.
    fn take(&self, window: Option<u64>) -> Result<(), String>;
    /// Gives it back to whoever was remembered.
    ///
    /// # Errors
    ///
    /// Why it could not be given back.
    fn give_back(&self, window: Option<u64>) -> Result<(), String>;
}

/// The system's keyboard, through [`LentFocus`].
#[derive(Debug, Default)]
pub struct SystemKeyboard {
    lent: Cell<Option<LentFocus>>,
}

impl Keyboard for SystemKeyboard {
    fn remember(&self) {
        self.lent.set(Some(LentFocus::remember()));
    }

    fn take(&self, window: Option<u64>) -> Result<(), String> {
        self.lent
            .get()
            .unwrap_or_else(LentFocus::remember)
            .take_for(window)
            .map_err(|error| error.to_string())
    }

    fn give_back(&self, window: Option<u64>) -> Result<(), String> {
        // Nothing remembered means nothing was lent: the window never opened.
        self.lent.take().map_or(Ok(()), |lent| {
            lent.give_back(window).map_err(|error| error.to_string())
        })
    }
}

/// The pack window and what it shows while it is open.
pub struct Packs {
    window: PacksWindow,
    /// `Some` while the window is open. Built fresh on every opening, so the
    /// list says what is on disk now and the query starts empty.
    picker: RefCell<Option<PackPicker>>,
    keyboard: Box<dyn Keyboard>,
    commands: Sender<Command>,
    in_use: InUse,
    /// Puts a line in the palette's message band - where every other sentence
    /// about the tool goes, and where the tester looks after the window closes.
    say: Box<dyn Fn(String)>,
}

impl Packs {
    /// The window, labelled and wired, not yet shown.
    ///
    /// The callbacks hold a weak handle: the window belongs to this value, and
    /// a strong one would keep both alive forever.
    #[must_use]
    pub fn new(
        window: PacksWindow,
        keyboard: Box<dyn Keyboard>,
        commands: Sender<Command>,
        in_use: InUse,
        say: Box<dyn Fn(String)>,
    ) -> Rc<Self> {
        label(&window);
        let packs = Rc::new(Self {
            window,
            picker: RefCell::new(None),
            keyboard,
            commands,
            in_use,
            say,
        });
        let weak = Rc::downgrade(&packs);
        packs
            .window
            .on_query_pressed(move |text, control, alt, meta| {
                weak.upgrade().is_some_and(|packs| {
                    packs.press(KeyPress {
                        text: text.as_str(),
                        control,
                        alt,
                        meta,
                    })
                })
            });
        let weak = Rc::downgrade(&packs);
        packs.window.on_next(move || {
            if let Some(packs) = weak.upgrade() {
                packs.move_selection(PackPicker::next);
            }
        });
        let weak = Rc::downgrade(&packs);
        packs.window.on_previous(move || {
            if let Some(packs) = weak.upgrade() {
                packs.move_selection(PackPicker::previous);
            }
        });
        let weak = Rc::downgrade(&packs);
        packs.window.on_choose(move || {
            if let Some(packs) = weak.upgrade() {
                let chosen = packs.with_picker(|picker| picker.chosen());
                packs.act(chosen.unwrap_or(Chosen::Nothing));
            }
        });
        let weak = Rc::downgrade(&packs);
        packs.window.on_clicked(move |row| {
            if let Some(packs) = weak.upgrade() {
                packs.click(row);
            }
        });
        let weak = Rc::downgrade(&packs);
        packs.window.on_cancel(move || {
            if let Some(packs) = weak.upgrade() {
                packs.close();
            }
        });
        let weak = Rc::downgrade(&packs);
        packs.window.on_deactivated(move || {
            if let Some(packs) = weak.upgrade() {
                packs.left();
            }
        });
        let weak = Rc::downgrade(&packs);
        packs.window.on_activated(move || {
            if let Some(packs) = weak.upgrade() {
                forget_held_modifiers(packs.window.window());
            }
        });
        let weak = Rc::downgrade(&packs);
        packs.window.window().on_close_requested(move || {
            if let Some(packs) = weak.upgrade() {
                packs.close();
            }
            // Hidden already, after the keyboard went back - see `close`.
            slint::CloseRequestResponse::KeepWindowShown
        });
        packs
    }

    /// Whether the window is open.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.picker.borrow().is_some()
    }

    /// Opens the window on the pack in use, or brings it back to the front.
    ///
    /// Asked again while open - the shortcut, or a click on the pack's name in
    /// the palette, with the window already up - it keeps the list, the query
    /// and the window it will hand the keyboard back to, and asks for the
    /// keyboard again. A tester who clicked away finds it closed instead (the
    /// module header, "Leaving it for another window").
    pub fn open(self: &Rc<Self>) {
        if !self.is_open() {
            self.keyboard.remember();
            let picker = PackPicker::new(
                list_packs(&BuiltInCatalogue::new(), &TomlPackFormat),
                in_use_now(&self.in_use).as_deref(),
            );
            self.window.set_notes(strings(picker.notes()));
            *self.picker.borrow_mut() = Some(picker);
            self.show_list();
            if let Err(error) = self.window.show() {
                *self.picker.borrow_mut() = None;
                (self.say)(i18n::startup_failure(
                    Startup::WindowFailed,
                    &error.to_string(),
                ));
                return;
            }
        }
        self.window.invoke_focus_search();
        self.take_keyboard(Instant::now());
    }

    /// Hands the keyboard back, then hides the window.
    ///
    /// 🔴 In THAT order: while the window is in front the tool may move the
    /// foreground, and once it is hidden the system has already given it to
    /// somebody - measured, see the module header.
    pub fn close(&self) {
        if !self.is_open() {
            return;
        }
        if let Err(reason) = self.keyboard.give_back(window_handle(self.window.window())) {
            (self.say)(i18n::environment(Environment::FocusNotReturned, &reason));
        }
        let _ = self.window.hide();
        *self.picker.borrow_mut() = None;
    }

    /// The window stopped being the active one: the tester is elsewhere.
    ///
    /// Closes without a word and without handing the keyboard back - it is
    /// where the tester put it. Our own hand-back in [`Packs::close`] makes the
    /// window inactive too, and whenever that report arrives, this finds the
    /// window already closed or closes what `close` closes anyway: the first
    /// line is the whole guard.
    fn left(&self) {
        if self.picker.borrow_mut().take().is_none() {
            return;
        }
        let _ = self.window.hide();
    }

    /// Asks for the keyboard as soon as the window has a handle to ask with.
    ///
    /// The handle comes from the window manager a moment after `show()` - 15
    /// to 17 ms in the probe, 258 to 489 ms for the palette at startup - so this
    /// asks again every [`POLL`] and gives up after [`HANDLE_WAIT`], saying so.
    fn take_keyboard(self: &Rc<Self>, started: Instant) {
        let handle = window_handle(self.window.window());
        if handle.is_none() && started.elapsed() < HANDLE_WAIT {
            let weak = Rc::downgrade(self);
            slint::Timer::single_shot(POLL, move || {
                if let Some(packs) = weak.upgrade()
                    && packs.is_open()
                {
                    packs.take_keyboard(started);
                }
            });
            return;
        }
        self.take_with(handle);
    }

    /// Takes the keyboard for `handle`, and says so when the system keeps it
    /// elsewhere - the letters of the search would go to the application under
    /// test, so silence here would be the worst answer.
    ///
    /// What the poll in `open` ends in. Public so a test with no window handle
    /// to wait for can reach the same code.
    pub fn take_with(&self, handle: Option<u64>) {
        if let Err(reason) = self.keyboard.take(handle) {
            (self.say)(i18n::environment(Environment::PacksFocusNotTaken, &reason));
        }
    }

    fn press(&self, press: KeyPress<'_>) -> bool {
        let Some(pressed) = self.with_picker(|picker| picker.press(press)) else {
            return false;
        };
        if pressed == Pressed::Changed {
            self.show_list();
        }
        pressed != Pressed::NotOurs
    }

    fn move_selection(&self, step: fn(&mut PackPicker)) {
        if self.with_picker(step).is_some() {
            self.show_selected();
        }
    }

    fn click(&self, row: i32) {
        let Ok(row) = usize::try_from(row) else {
            return;
        };
        let Some(chosen) = self.with_picker(|picker| picker.click(row)) else {
            return;
        };
        self.show_selected();
        self.act(chosen);
    }

    /// What Enter or a click comes to.
    fn act(&self, chosen: Chosen) {
        match chosen {
            // Nothing can be chosen - nothing matches, or nothing loads. The
            // window stays, because closing it would look like a choice.
            Chosen::Nothing => {}
            Chosen::InUse => self.close(),
            Chosen::Pack(pack) => {
                // The worker is gone only when the shortcuts died, and the
                // palette said so then. Said again, so the choice is not lost
                // without a word.
                if self.commands.send(Command::Choose(pack)).is_err()
                    && let Some(line) = i18n::ended(Ended::ShortcutsGone)
                {
                    (self.say)(line);
                }
                self.close();
            }
        }
    }

    fn with_picker<T>(&self, act: impl FnOnce(&mut PackPicker) -> T) -> Option<T> {
        self.picker.borrow_mut().as_mut().map(act)
    }

    /// The query, the rows and everything that follows them.
    fn show_list(&self) {
        let picker = self.picker.borrow();
        let Some(picker) = picker.as_ref() else {
            return;
        };
        self.window.set_query(picker.query().into());
        self.window.set_rows(ModelRc::new(VecModel::from(
            picker.rows().iter().map(pick_row).collect::<Vec<_>>(),
        )));
        self.window.set_selected(selected(picker));
        self.window.set_summary(picker.summary().into());
        self.window.set_empty_text(picker.empty_text().into());
    }

    /// The selection alone - the rows stay as they are, and so does the
    /// list's scroll position.
    fn show_selected(&self) {
        if let Some(picker) = self.picker.borrow().as_ref() {
            self.window.set_selected(selected(picker));
        }
    }

    /// The window, for a test that reads what it shows.
    #[must_use]
    pub fn window(&self) -> &PacksWindow {
        &self.window
    }
}

/// Every modifier key Slint keeps a pressed state for - `InternalKeyboardModifierState`
/// in i-slint-core 1.18.1, read, not guessed. A key missing here would stay
/// stuck, so the list is all of them rather than the shortcut's own.
const MODIFIERS: [slint::platform::Key; 8] = [
    slint::platform::Key::Alt,
    slint::platform::Key::AltGr,
    slint::platform::Key::Control,
    slint::platform::Key::ControlR,
    slint::platform::Key::Shift,
    slint::platform::Key::ShiftR,
    slint::platform::Key::Meta,
    slint::platform::Key::MetaR,
];

/// Tells `window` that no modifier is held - on the moment the window becomes
/// active, see the module header.
///
/// Releases, not a reset: Slint has no public way to clear its modifier state,
/// and a release of each key is the one event that clears it. A key still
/// physically held at that moment is forgotten too, and the cost is named: a
/// letter typed while the tester keeps holding the shortcut's Alt goes into
/// the query instead of being passed on as `Alt+letter`. The pack window has
/// no such shortcut, and the tool's own shortcuts are taken by the system
/// before any window sees them. The reverse error - every letter lost - is
/// what `OBS-151` measured.
///
/// Public so the probe `tools/sonda-klawisze` (`akord naprawa`) runs this very
/// code on a live window.
pub fn forget_held_modifiers(window: &slint::Window) {
    for key in MODIFIERS {
        window.dispatch_event(slint::platform::WindowEvent::KeyReleased { text: key.into() });
    }
}

/// Every word that does not change while the window is open.
fn label(window: &PacksWindow) {
    window.set_window_title(i18n::packs_label(PacksLabel::Title).into());
    window.set_heading(i18n::packs_label(PacksLabel::Heading).into());
    window.set_search_label(i18n::packs_label(PacksLabel::Search).into());
    window.set_current_label(i18n::packs_label(PacksLabel::InUse).into());
    let hints: Vec<HintRow> = [
        (PacksLabel::KeyEnter, PacksLabel::UsePack),
        (PacksLabel::KeyEscape, PacksLabel::Close),
        (PacksLabel::KeyArrows, PacksLabel::Move),
    ]
    .into_iter()
    .map(|(key, action)| HintRow {
        key: i18n::packs_label(key).into(),
        action: i18n::packs_label(action).into(),
    })
    .collect();
    window.set_hints(ModelRc::new(VecModel::from(hints)));
}

/// One row as the window draws it. The only place a [`Row`] becomes a
/// [`PickRow`], so the two cannot drift apart in two conversions.
fn pick_row(row: &Row) -> PickRow {
    PickRow {
        title: row.title.as_str().into(),
        detail: row.detail.as_str().into(),
        badge: row
            .badge
            .as_ref()
            .map_or_else(SharedString::new, |badge| badge.text.as_str().into()),
        has_badge: row.badge.is_some(),
        badge_risky: row.badge.as_ref().is_some_and(|badge| badge.risky),
        current: row.current,
        enabled: row.enabled,
    }
}

/// The selected row for the view, or `-1` - which no row's index equals - when
/// nothing can be selected.
fn selected(picker: &PackPicker) -> i32 {
    picker
        .selected()
        .and_then(|at| i32::try_from(at).ok())
        .unwrap_or(-1)
}

fn strings(lines: &[String]) -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from(
        lines
            .iter()
            .map(|line| SharedString::from(line.as_str()))
            .collect::<Vec<_>>(),
    ))
}
