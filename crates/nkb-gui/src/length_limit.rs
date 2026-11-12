//! The length limit window on the main thread (`D178`): opened from the value
//! window's first row, filled from [`LimitForm`], closed with the keyboard
//! handed back.
//!
//! # The second moment the tool takes the keyboard
//!
//! Two numbers have to be typed, and the palette refuses the keyboard for the
//! whole run (`D62`). So this is a window of its own, like the value window,
//! and it treats the keyboard the way that one does (`packs.rs` says why, with
//! the measurements): it asks for it and reads back, hands it back BEFORE
//! hiding, and a tester who goes to another window finds it closed, with the
//! keyboard left where they put it (`OBS-147`).
//!
//! # The value window gives way once this one holds the keyboard, not before
//!
//! The tester has just pressed Enter and is about to type a number. Wherever
//! the keyboard is meanwhile, the digits go - and a hidden window that held the
//! foreground has the system give it to the next window in line, which is the
//! application under test. So the value window stays, holding the keyboard,
//! through both waits of the opening:
//!
//! 1. The window is made at its first opening (R23, `D167`), and that opening
//!    waits for the window's graphics context - a tenth of a second and more.
//!    Had the value window closed at the Enter, the digits typed in that wait
//!    went into the field under test (`OBS-148`).
//! 2. Showing this window and taking the keyboard come after it. 🔴 Had the
//!    value window hidden first, the application under test held the foreground
//!    until this window took it: measured with `tools/okno-paczek.ps1`, scene
//!    `pospiech`, 2026-11-12. On a debug build from 120-160 ms after the Enter
//!    to 300 ms, and five digits of about fifty in the field. On a release
//!    build from 27 ms to 78 ms, and one digit (`OBS-248`).
//!
//! So the value window hands over the keyboard it lent just before this one
//! shows ([`Wiring::hand_keyboard_over`]) - showing it makes the value window
//! inactive, and an inactive one closes - and hides only once this one has
//! taken the keyboard, or could not ([`Wiring::give_way`]). Hidden then, it is
//! not the active window, and the foreground stays where it is.
//!
//! # What is decided here, and what is not
//!
//! Which numbers are a limit and what the list holds are [`LimitForm`]'s. The
//! values become the palette's on the worker, which holds the sequence
//! ([`Command::UseLimit`]). This module moves answers onto the window, and
//! nothing in it needs a screen to be tested: the keyboard is a trait, the
//! worker a channel, the palette's message band a closure.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::mpsc::Sender;
use std::time::Instant;

use nkb_adapters::i18n::{self, Environment, LimitLabel, PacksLabel, Startup};
use nkb_app::Ended;
use nkb_core::boundary::Unit;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::focus::{HANDLE_WAIT, POLL, window_handle};
use crate::limit_form::{Bound, LimitForm, Opening, ValueLine};
use crate::live::{Command, InUse, in_use_now};
use crate::on_request::{self, Later, Ready, Requested};
use crate::pack_folders::HandedOver;
use crate::packs::{Keyboard, SystemKeyboard, build_before_the_loop, forget_held_modifiers};
use crate::query::{KeyPress, Pressed};
use crate::{HintRow, LimitWindow, PickRow};

/// The number the tester typed in the value window's search, for this window
/// to open with as the most (`D178`). Taken at the opening it was left for.
pub type Suggested = Rc<Cell<Option<u32>>>;

/// What this window needs from the rest of the palette.
pub struct Wiring {
    pub commands: Sender<Command>,
    /// What the palette walks now: a limit in use is what the window opens on.
    pub in_use: InUse,
    /// The keyboard the value window lent, handed over just before this window
    /// shows.
    pub handed_over: HandedOver,
    pub suggested: Suggested,
    /// Has the value window hand over the keyboard it lent, just before this
    /// one shows - see the module header.
    pub hand_keyboard_over: Box<dyn Fn()>,
    /// Has the value window close, once this one has taken the keyboard or
    /// could not - see the module header.
    pub give_way: Box<dyn Fn()>,
}

/// The length limit window and what it shows while it is open.
pub struct LengthLimit {
    window: LimitWindow,
    /// `Some` while the window is open. Built fresh on every opening.
    form: RefCell<Option<LimitForm>>,
    keyboard: Box<dyn Keyboard>,
    wiring: Wiring,
    /// Set by an opening the value window stays up for, until it gives way:
    /// once, whatever ends the opening.
    value_window_up: Cell<bool>,
    /// Puts a line in the palette's message band.
    say: Box<dyn Fn(String)>,
}

impl LengthLimit {
    /// The window, labelled and wired, not yet shown.
    #[must_use]
    pub fn new(
        window: LimitWindow,
        keyboard: Box<dyn Keyboard>,
        wiring: Wiring,
        say: Box<dyn Fn(String)>,
    ) -> Rc<Self> {
        label(&window);
        // After the words, which are part of what the layout measures.
        build_before_the_loop(window.window());
        let limit = Rc::new(Self {
            window,
            form: RefCell::new(None),
            keyboard,
            wiring,
            value_window_up: Cell::new(false),
            say,
        });
        wire(&limit);
        limit
    }

    /// The window as the palette has it: made at its first opening, with the
    /// system's keyboard, and kept from then on.
    #[must_use]
    pub fn later(wiring: Wiring, say: Box<dyn Fn(String)>) -> Rc<Later<Self>> {
        let wiring = RefCell::new(Some(wiring));
        Later::new(say, move |say| {
            let Some(wiring) = wiring.borrow_mut().take() else {
                return Err(slint::PlatformError::Other(String::from(
                    "the length limit window was made twice",
                )));
            };
            Ok(Self::new(
                LimitWindow::new()?,
                Box::new(SystemKeyboard::default()),
                wiring,
                say,
            ))
        })
    }

    /// Whether the window is open.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.form.borrow().is_some()
    }

    /// The window, for a test that reads what it shows.
    #[must_use]
    pub fn window(&self) -> &LimitWindow {
        &self.window
    }

    /// Opens the window with the keyboard in its first number, or brings it
    /// back to the front.
    ///
    /// It opens on the number the value window's search suggested, else on the
    /// limit the palette walks now, else empty: nothing is remembered from one
    /// opening to the next (`D178`).
    pub fn open(self: &Rc<Self>) {
        if !self.is_open() {
            (self.wiring.hand_keyboard_over)();
            self.value_window_up.set(true);
            match self.wiring.handed_over.take() {
                Some(lent) => self.keyboard.take_over(lent),
                None => self.keyboard.remember(),
            }
            let opening = self.wiring.suggested.take().map_or_else(
                || {
                    in_use_now(&self.wiring.in_use)
                        .made
                        .map_or(Opening::EMPTY, Opening::of)
                },
                Opening::at_most,
            );
            *self.form.borrow_mut() = Some(LimitForm::new(opening));
            self.show_all();
        }
        if let Err(error) = on_request::bring_up(self.window.window()) {
            *self.form.borrow_mut() = None;
            self.value_window_gives_way();
            (self.say)(i18n::startup_failure(
                Startup::WindowFailed,
                &error.to_string(),
            ));
            return;
        }
        self.window.invoke_focus_most();
        self.take_keyboard(Instant::now());
    }

    /// Hands the keyboard back, then hides the window - in that order, for the
    /// reason `Packs::close` gives.
    pub fn close(&self) {
        if self.is_open()
            && let Err(reason) = self.keyboard.give_back(window_handle(self.window.window()))
        {
            (self.say)(i18n::environment(Environment::FocusNotReturned, &reason));
        }
        let _ = self.window.hide();
        *self.form.borrow_mut() = None;
        self.value_window_gives_way();
    }

    /// The window stopped being the active one: the tester is elsewhere. It
    /// closes without handing the keyboard back, as the value window does.
    fn left(self: &Rc<Self>) {
        *self.form.borrow_mut() = None;
        self.value_window_gives_way();
        on_request::hide_once_left(
            self,
            |limit| !limit.is_open(),
            |limit| limit.window.window(),
        );
    }

    fn take_keyboard(self: &Rc<Self>, started: Instant) {
        let handle = window_handle(self.window.window());
        if handle.is_none() && started.elapsed() < HANDLE_WAIT {
            let weak = Rc::downgrade(self);
            slint::Timer::single_shot(POLL, move || {
                if let Some(limit) = weak.upgrade() {
                    limit.look_again(started);
                }
            });
            return;
        }
        self.take_with(handle);
    }

    /// The handle looked for once more - or, the window closed meanwhile, the
    /// value window gives way without it.
    fn look_again(self: &Rc<Self>, started: Instant) {
        if self.is_open() {
            self.take_keyboard(started);
        } else {
            self.value_window_gives_way();
        }
    }

    /// Takes the keyboard for `handle`, and says so when the system keeps it
    /// elsewhere - the digits would go to the application under test. Then,
    /// and not before, the value window gives way (the module header). Public
    /// so a test with no window handle to wait for reaches the same code.
    pub fn take_with(&self, handle: Option<u64>) {
        if let Err(reason) = self.keyboard.take(handle) {
            (self.say)(i18n::environment(Environment::LimitFocusNotTaken, &reason));
        }
        self.value_window_gives_way();
    }

    /// Has the value window close, if this opening left it up: once, and
    /// whatever ended the opening - the keyboard taken, the window that could
    /// not show, or closed before it got a handle - so it never stays behind.
    fn value_window_gives_way(&self) {
        if self.value_window_up.replace(false) {
            (self.wiring.give_way)();
        }
    }

    fn with_form<T>(&self, act: impl FnOnce(&mut LimitForm) -> T) -> Option<T> {
        self.form.borrow_mut().as_mut().map(act)
    }

    /// One key press in the line of `bound`.
    fn press(&self, bound: Bound, press: KeyPress<'_>) -> bool {
        let Some(pressed) = self.with_form(|form| form.press(bound, press)) else {
            return false;
        };
        if pressed == Pressed::Changed {
            self.show_all();
        }
        pressed != Pressed::NotOurs
    }

    /// A change of the unit, and the list made again when it was one.
    fn change_unit(&self, change: impl FnOnce(&mut LimitForm) -> bool) {
        if self.with_form(change) == Some(true) {
            self.show_all();
        }
    }

    /// A move of the row the walk starts from. The rows stay as they are, and
    /// so does the list's place.
    fn move_start(&self, step: impl FnOnce(&mut LimitForm)) {
        if self.with_form(step).is_some() {
            self.show_selected();
        }
    }

    /// The main action: the values go to the worker, and the window closes.
    /// While the numbers make no limit nothing happens - the button is off,
    /// and Enter must not close a window that did nothing.
    fn use_values(&self) {
        let Some(chosen) = self.with_form(|form| form.chosen()).flatten() else {
            return;
        };
        // The worker is gone only when the shortcuts died, and the palette
        // said so then. Said again, so the choice is not lost without a word.
        let command = Command::UseLimit {
            limit: chosen.limit,
            start: chosen.start,
        };
        if self.wiring.commands.send(command).is_err()
            && let Some(line) = i18n::ended(Ended::ShortcutsGone)
        {
            (self.say)(line);
        }
        self.close();
    }

    /// Everything the form decides: the digits, the unit, the list.
    fn show_all(&self) {
        let form = self.form.borrow();
        let Some(form) = form.as_ref() else {
            return;
        };
        self.window.set_most(form.digits(Bound::Most).into());
        self.window.set_least(form.digits(Bound::Least).into());
        self.window.set_most_at_fault(form.at_fault(Bound::Most));
        self.window.set_least_at_fault(form.at_fault(Bound::Least));
        self.window
            .set_unit(i32::try_from(form.unit_index()).unwrap_or(0));
        self.window.set_account(form.account().into());
        self.window.set_empty_text(form.empty_text().into());
        self.window.set_rows(ModelRc::new(VecModel::from(
            form.lines().iter().map(pick_row).collect::<Vec<_>>(),
        )));
        self.window.set_selected(selected(form));
        self.window.set_can_use(form.chosen().is_some());
    }

    fn show_selected(&self) {
        if let Some(form) = self.form.borrow().as_ref() {
            self.window.set_selected(selected(form));
        }
    }
}

/// The callbacks of the window.
fn wire(limit: &Rc<LengthLimit>) {
    for bound in [Bound::Most, Bound::Least] {
        let weak = Rc::downgrade(limit);
        let pressed = move |text: SharedString, control, alt, _shift, meta| {
            weak.upgrade().is_some_and(|limit| {
                limit.press(
                    bound,
                    KeyPress {
                        text: text.as_str(),
                        control,
                        alt,
                        meta,
                    },
                )
            })
        };
        match bound {
            Bound::Most => limit.window.on_most_pressed(pressed),
            Bound::Least => limit.window.on_least_pressed(pressed),
        }
    }
    let weak = Rc::downgrade(limit);
    limit.window.on_unit_chosen(move |index| {
        if let (Some(limit), Ok(index)) = (weak.upgrade(), usize::try_from(index)) {
            limit.change_unit(|form| form.set_unit(index));
        }
    });
    for forward in [false, true] {
        let weak = Rc::downgrade(limit);
        let step = move || {
            if let Some(limit) = weak.upgrade() {
                limit.change_unit(|form| form.step_unit(forward));
            }
        };
        if forward {
            limit.window.on_unit_forward(step);
        } else {
            limit.window.on_unit_back(step);
        }
    }
    let weak = Rc::downgrade(limit);
    limit.window.on_next(move || {
        if let Some(limit) = weak.upgrade() {
            limit.move_start(LimitForm::next);
        }
    });
    let weak = Rc::downgrade(limit);
    limit.window.on_previous(move || {
        if let Some(limit) = weak.upgrade() {
            limit.move_start(LimitForm::previous);
        }
    });
    let weak = Rc::downgrade(limit);
    limit.window.on_clicked(move |row| {
        if let (Some(limit), Ok(row)) = (weak.upgrade(), usize::try_from(row)) {
            limit.move_start(|form| form.select(row));
        }
    });
    let weak = Rc::downgrade(limit);
    limit.window.on_use(move || {
        if let Some(limit) = weak.upgrade() {
            limit.use_values();
        }
    });
    let weak = Rc::downgrade(limit);
    limit.window.on_cancel(move || {
        if let Some(limit) = weak.upgrade() {
            limit.close();
        }
    });
    let weak = Rc::downgrade(limit);
    limit.window.on_deactivated(move || {
        if let Some(limit) = weak.upgrade() {
            limit.left();
        }
    });
    let weak = Rc::downgrade(limit);
    limit.window.on_activated(move || {
        if let Some(limit) = weak.upgrade() {
            forget_held_modifiers(limit.window.window());
        }
    });
    let weak = Rc::downgrade(limit);
    limit.window.window().on_close_requested(move || {
        if let Some(limit) = weak.upgrade() {
            limit.close();
        }
        // Hidden already, after the keyboard went back - see `close`.
        slint::CloseRequestResponse::KeepWindowShown
    });
}

/// Every word that does not change while the window is open.
fn label(window: &LimitWindow) {
    window.set_window_title(i18n::limit_label(LimitLabel::Title).into());
    window.set_heading(i18n::limit_label(LimitLabel::Heading).into());
    window.set_most_label(i18n::limit_label(LimitLabel::Most).into());
    window.set_least_label(i18n::limit_label(LimitLabel::Least).into());
    window.set_unit_label(i18n::limit_label(LimitLabel::Unit).into());
    window.set_use_label(i18n::limit_label(LimitLabel::Use).into());
    window.set_use_action(i18n::limit_label(LimitLabel::UseAction).into());
    window.set_pill_label(i18n::limit_side(false).into());
    let units: Vec<SharedString> = Unit::ALL
        .into_iter()
        .map(|unit| i18n::limit_unit(unit).into())
        .collect();
    window.set_units(ModelRc::new(VecModel::from(units)));
    // The keys the value window names are named in its words, so one key has
    // one name in both windows.
    let hints: Vec<HintRow> = [
        (
            i18n::packs_label(PacksLabel::KeyEnter),
            i18n::limit_label(LimitLabel::Use),
        ),
        (
            i18n::packs_label(PacksLabel::KeyEscape),
            i18n::packs_label(PacksLabel::Close),
        ),
        (
            i18n::limit_label(LimitLabel::KeyTab),
            i18n::limit_label(LimitLabel::OtherNumber),
        ),
        (
            i18n::packs_label(PacksLabel::KeyFold),
            i18n::limit_label(LimitLabel::ChangeUnit),
        ),
        (
            i18n::packs_label(PacksLabel::KeyArrows),
            i18n::limit_label(LimitLabel::StartAt),
        ),
    ]
    .into_iter()
    .map(|(key, action)| HintRow {
        key: key.into(),
        action: action.into(),
    })
    .collect();
    window.set_hints(ModelRc::new(VecModel::from(hints)));
}

/// One value as the window draws it: the value window's row for a value - a
/// name, and at the right edge its length where that row has what the value
/// types - with the side of the limit as its pill.
fn pick_row(line: &ValueLine) -> PickRow {
    PickRow {
        title: line.name.as_str().into(),
        detail: line.length.as_str().into(),
        badge: line.side.as_str().into(),
        has_badge: true,
        enabled: true,
        ..PickRow::default()
    }
}

/// The row the walk starts from, or `-1` - which no row's index equals - while
/// the list is empty.
fn selected(form: &LimitForm) -> i32 {
    form.selected()
        .and_then(|at| i32::try_from(at).ok())
        .unwrap_or(-1)
}

impl Requested for LengthLimit {
    fn open(self: &Rc<Self>) {
        Self::open(self);
    }
    fn close(&self) {
        Self::close(self);
    }
    fn when_ready(self: &Rc<Self>, then: Ready) {
        on_request::when_backed(self, |limit| limit.window.window(), then);
    }
}
