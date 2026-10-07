//! The welcome window on the main thread (`ux-spec.md` 5.1, UX7, `UX-GUI-011`,
//! `D105`): opened once, at the first run, with the keyboard taken for its box
//! and handed back when it closes - and remembered as closed.
//!
//! # When it opens - and the palette only after it (the owner's point 1)
//!
//! When `KeptSettings::welcome_due` says so: the settings file has no
//! `[welcome] done = true` and a closing could be remembered (`D103`). It opens
//! ALONE, as the event loop starts, once the worker has said which pack the
//! palette starts on. Until 2026-10-07 the palette opened first and the
//! welcome beside it, and the owner could not tell what the two windows were
//! for. Now the palette appears when the welcome goes - by "Start testing",
//! the close button or `Alt+F4` alike, and also when the welcome could not
//! open at all - through the one callback the window is given (`D109`).
//!
//! 🔴 The palette is shown BEFORE the welcome is hidden. Slint keeps its event
//! loop alive by counting the windows shown and quits when the count falls to
//! nothing (`release_keepalive` in i-slint-core 1.18.1, read), so hiding the
//! only window first would end the run. The palette is never hidden after
//! that (`OBS-80`): it is shown for the first time here, and refuses the
//! keyboard then, as at any start.
//!
//! # The keyboard
//!
//! The pack window's way (`packs`): asked for on this thread once the window
//! has a handle, and handed back BEFORE hiding, because once hidden the system
//! has already given the foreground to somebody. A tester who clicks another
//! window keeps the welcome open - unlike the pack window, it is not in the
//! way, and the box is still there to try.
//!
//! # Closing is remembered by the worker
//!
//! `[welcome] done` is one key of the settings file, and the worker owns the
//! settings for the run: two writers in one process would share the temporary
//! file name a save writes through (`settings_file`). So closing asks the
//! worker, like every other change, with [`Command::WelcomeDone`].

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::mpsc::Sender;
use std::time::Instant;

use nkb_adapters::i18n::{self, Environment, Startup, WelcomeLabel};
use nkb_core::hotkeys::{Bindings, HotkeyAction};
use nkb_core::preview::preview;
use slint::{ComponentHandle, ModelRc, VecModel};

use crate::focus::{HANDLE_WAIT, POLL, window_handle};
use crate::live::{Command, InUse, in_use_now};
use crate::packs::{Keyboard, build_before_the_loop, forget_held_modifiers};
use crate::query::Pressed;
use crate::trial::{Trial, TrialKey};
use crate::{HintRow, WelcomeWindow};

/// The welcome window and the box in it.
pub struct Welcome {
    window: WelcomeWindow,
    trial: RefCell<Trial>,
    keyboard: Box<dyn Keyboard>,
    commands: Sender<Command>,
    /// The shortcut that opens the value window, for the third step.
    open_packs: nkb_core::hotkeys::HotkeyChord,
    open: Cell<bool>,
    /// Puts a line in the palette's message band.
    say: Box<dyn Fn(String)>,
    /// Shows the palette - once, when the welcome goes or cannot open.
    appear: RefCell<Option<Box<dyn FnOnce()>>>,
}

impl Welcome {
    /// The window, labelled and wired, not yet shown. `bindings` are the
    /// shortcuts of this run, so the steps name the ones the palette answers.
    /// `appear` shows the palette, and runs once: when the welcome goes, or
    /// when it could not open (`D109`).
    #[must_use]
    pub fn new(
        window: WelcomeWindow,
        keyboard: Box<dyn Keyboard>,
        commands: Sender<Command>,
        bindings: &Bindings,
        say: Box<dyn Fn(String)>,
        appear: Box<dyn FnOnce()>,
    ) -> Rc<Self> {
        label(&window, bindings);
        // After the words, which are part of what the layout measures - the
        // pack window's reason (`OBS-154`).
        build_before_the_loop(window.window());
        let welcome = Rc::new(Self {
            window,
            trial: RefCell::new(Trial::default()),
            keyboard,
            commands,
            open_packs: bindings.chord(HotkeyAction::OpenPacks),
            open: Cell::new(false),
            say,
            appear: RefCell::new(Some(appear)),
        });
        welcome.show_box();
        let weak = Rc::downgrade(&welcome);
        welcome
            .window
            .on_pressed(move |text, control, alt, shift, meta| {
                weak.upgrade().is_some_and(|welcome| {
                    welcome.press(TrialKey {
                        text: text.as_str(),
                        control,
                        alt,
                        shift,
                        meta,
                    })
                })
            });
        let weak = Rc::downgrade(&welcome);
        welcome.window.on_start(move || {
            if let Some(welcome) = weak.upgrade() {
                welcome.close();
            }
        });
        let weak = Rc::downgrade(&welcome);
        welcome.window.on_activated(move || {
            if let Some(welcome) = weak.upgrade() {
                forget_held_modifiers(welcome.window.window());
            }
        });
        let weak = Rc::downgrade(&welcome);
        welcome.window.window().on_close_requested(move || {
            if let Some(welcome) = weak.upgrade() {
                welcome.close();
            }
            // Hidden already, after the keyboard went back - see `close`.
            slint::CloseRequestResponse::KeepWindowShown
        });
        welcome
    }

    /// Whether the window is open.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.open.get()
    }

    /// The window, for a test that sends it keys and reads it back.
    #[must_use]
    pub fn window(&self) -> &WelcomeWindow {
        &self.window
    }

    /// The box's text, as it stands - what a test reads back.
    #[must_use]
    pub fn box_text(&self) -> String {
        self.trial.borrow().text().to_owned()
    }

    /// Opens the window once the worker has said which pack the palette
    /// starts on, so the third step can name it - asked again every [`POLL`],
    /// and opened without the name after [`HANDLE_WAIT`] rather than not at
    /// all. Call it before the event loop runs: the poll is delivered once it
    /// starts (`slint.md` 1.9).
    pub fn open_on_start(self: &Rc<Self>, in_use: InUse, started: Instant) {
        let now = in_use_now(&in_use);
        if now.pack.is_none() && started.elapsed() < HANDLE_WAIT {
            let weak = Rc::downgrade(self);
            slint::Timer::single_shot(POLL, move || {
                if let Some(welcome) = weak.upgrade() {
                    welcome.open_on_start(in_use, started);
                }
            });
            return;
        }
        self.open(now.pack.as_deref());
    }

    /// Opens the window with the keyboard in the box. `pack` is the pack the
    /// palette is on, when the worker has said so already. A window that does
    /// not open says so in the palette - and the palette appears, or the run
    /// would show nothing at all.
    pub fn open(self: &Rc<Self>, pack: Option<&str>) {
        if self.is_open() {
            return;
        }
        self.window
            .set_ready(i18n::welcome_ready(pack, self.open_packs).into());
        self.keyboard.remember();
        if let Err(error) = self.window.show() {
            (self.say)(i18n::startup_failure(
                Startup::WindowFailed,
                &error.to_string(),
            ));
            self.appear();
            return;
        }
        self.open.set(true);
        self.window.invoke_focus_box();
        self.take_keyboard(Instant::now());
    }

    /// Hides the window without remembering it closed - the palette is
    /// closing, and a tester who closed the palette has not closed the
    /// welcome. The next run welcomes again. The event loop would otherwise
    /// wait for this window, and the process outlive the palette.
    pub fn dismiss(&self) {
        if self.open.replace(false) {
            let _ = self.window.hide();
        }
    }

    /// Hands the keyboard back, shows the palette, hides the window and asks
    /// the worker to remember it closed and start the pack over. In THAT
    /// order: the keyboard for the pack window's reason, the palette before
    /// the hiding for the event loop's (the module header).
    pub fn close(&self) {
        if !self.open.replace(false) {
            return;
        }
        if let Err(reason) = self.keyboard.give_back(window_handle(self.window.window())) {
            (self.say)(i18n::environment(Environment::FocusNotReturned, &reason));
        }
        self.appear();
        let _ = self.window.hide();
        // A worker already gone is a palette already closing - nobody is left
        // to welcome, and the next run asks again.
        let _ = self.commands.send(Command::WelcomeDone);
    }

    /// Shows the palette, the first time only.
    fn appear(&self) {
        let appear = self.appear.borrow_mut().take();
        if let Some(appear) = appear {
            appear();
        }
    }

    /// Asks for the keyboard as soon as the window has a handle to ask with -
    /// the pack window's poll, for the same measured reason.
    fn take_keyboard(self: &Rc<Self>, started: Instant) {
        let handle = window_handle(self.window.window());
        if handle.is_none() && started.elapsed() < HANDLE_WAIT {
            let weak = Rc::downgrade(self);
            slint::Timer::single_shot(POLL, move || {
                if let Some(welcome) = weak.upgrade()
                    && welcome.is_open()
                {
                    welcome.take_keyboard(started);
                }
            });
            return;
        }
        if let Err(reason) = self.keyboard.take(handle) {
            (self.say)(i18n::environment(
                Environment::WelcomeFocusNotTaken,
                &reason,
            ));
        }
    }

    fn press(&self, key: TrialKey<'_>) -> bool {
        let pressed = self.trial.borrow_mut().press(key);
        if pressed == Pressed::Changed {
            self.show_box();
        }
        pressed != Pressed::NotOurs
    }

    /// The box as the window draws it: the text before and after the caret
    /// with the invisible characters as the palette's marker, and what
    /// arrived, counted the way the palette counts.
    fn show_box(&self) {
        let trial = self.trial.borrow();
        self.window
            .set_box_before(preview(trial.before()).shown.into());
        self.window
            .set_box_after(preview(trial.after()).shown.into());
        let text = trial.text();
        let facts = if text.is_empty() {
            i18n::welcome_label(WelcomeLabel::Empty).to_owned()
        } else {
            i18n::counts(
                nkb_core::graphemes::count(text),
                text.chars().count(),
                text.len(),
                text.encode_utf16().count(),
            )
        };
        self.window.set_box_facts(facts.into());
    }
}

/// Every word that does not change while the window is open.
fn label(window: &WelcomeWindow, bindings: &Bindings) {
    let word = |label| i18n::welcome_label(label).into();
    window.set_window_title(word(WelcomeLabel::Title));
    window.set_heading(word(WelcomeLabel::Heading));
    window.set_try_heading(word(WelcomeLabel::TryHeading));
    window.set_try_it(i18n::welcome_try_it(bindings.chord(HotkeyAction::NextValue)).into());
    window.set_box_name(word(WelcomeLabel::BoxName));
    window.set_does_heading(word(WelcomeLabel::DoesHeading));
    window.set_does(word(WelcomeLabel::Does));
    window.set_allowed(word(WelcomeLabel::Allowed));
    window.set_ready_heading(word(WelcomeLabel::ReadyHeading));
    window.set_ready(i18n::welcome_ready(None, bindings.chord(HotkeyAction::OpenPacks)).into());
    window.set_start_label(word(WelcomeLabel::Start));
    window.set_start_action(word(WelcomeLabel::StartAction));
    // The three shortcuts `ux-spec.md` 5.1 asks for: the next value, the one
    // before it, and the window to choose another.
    let hints: Vec<HintRow> = [
        HotkeyAction::NextValue,
        HotkeyAction::PreviousValue,
        HotkeyAction::OpenPacks,
    ]
    .into_iter()
    .map(|action| HintRow {
        key: i18n::chord(bindings.chord(action)).into(),
        action: i18n::action_name(action).into(),
    })
    .collect();
    window.set_hints(ModelRc::new(VecModel::from(hints)));
}
