//! The shortcuts window as the product wires it - a pause asked for, the
//! window shown on the worker's answer, a chord recorded and asked for, the
//! shortcuts taken again on every way out - with no screen and no system.
//!
//! # What this proves that `shortcut_list.rs` cannot
//!
//! `shortcut_list.rs` decides what a key does, with no window. Here the keys go
//! to the window as the system would send them, through the product's own
//! wiring in `nkb_gui::shortcuts`, and the answers are read from the channel
//! to the worker, the window, and stand-ins that record what they were asked.
//!
//! # What it cannot
//!
//! Move a real focus, read a real chord or keep a real menu shut - measured on
//! live windows (`tools/sonda-klawisze`, `ktory`, and the level 1 proof of K5.7).
//!
//! One test in its own binary for the reason `palette_appearance.rs` gives:
//! one platform per process.

#![allow(clippy::panic, clippy::expect_used)]

#[allow(dead_code)]
mod offscreen;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;

use nkb_adapters::{ChordHeld, default_bindings, i18n};
use nkb_app::ShortcutChange;
use nkb_app::ports::ShortcutRegistration;
use nkb_core::hotkeys::{HotkeyAction, HotkeyChord};
use nkb_gui::ShortcutsWindow;
use nkb_gui::live::{Command, ShortcutsNow, Told};
use nkb_gui::packs::Keyboard;
use nkb_gui::shortcuts::{Shortcuts, SystemKeys};
use slint::platform::{Key, WindowEvent};
use slint::{ComponentHandle, Model, SharedString};

fn tap(window: &ShortcutsWindow, text: &str) {
    for event in [
        WindowEvent::KeyPressed {
            text: SharedString::from(text),
        },
        WindowEvent::KeyReleased {
            text: SharedString::from(text),
        },
    ] {
        window.window().dispatch_event(event);
    }
}

fn tap_key(window: &ShortcutsWindow, key: Key) {
    tap(window, SharedString::from(key).as_str());
}

/// A keyboard that records what it was asked, and whether the window was
/// still shown when the keyboard went back.
#[derive(Default)]
struct Recorded {
    calls: RefCell<Vec<&'static str>>,
    take_fails: RefCell<bool>,
    window_shown: RefCell<Option<Box<dyn Fn() -> bool>>>,
    shown_at_give_back: RefCell<Vec<bool>>,
}

struct Shared(Rc<Recorded>);

impl Keyboard for Shared {
    fn remember(&self) {
        self.0.calls.borrow_mut().push("remember");
    }
    fn take(&self, _window: Option<u64>) -> Result<(), String> {
        self.0.calls.borrow_mut().push("take");
        if *self.0.take_fails.borrow() {
            Err(String::from("kept elsewhere"))
        } else {
            Ok(())
        }
    }
    fn give_back(&self, _window: Option<u64>) -> Result<(), String> {
        self.0.calls.borrow_mut().push("give back");
        let shown = self
            .0
            .window_shown
            .borrow()
            .as_ref()
            .is_some_and(|shown| shown());
        self.0.shown_at_give_back.borrow_mut().push(shown);
        Ok(())
    }
}

/// The system's answers, as a test dictates them.
struct Dictated {
    held: Rc<RefCell<ChordHeld>>,
    menu_fails: Rc<RefCell<bool>>,
    menus: Rc<RefCell<usize>>,
}

impl SystemKeys for Dictated {
    fn chord_held(&self) -> ChordHeld {
        *self.held.borrow()
    }
    fn keep_menu_shut(&self, _window: Option<u64>) -> Result<(), String> {
        *self.menus.borrow_mut() += 1;
        if *self.menu_fails.borrow() {
            Err(String::from("refused"))
        } else {
            Ok(())
        }
    }
}

fn calls(keyboard: &Rc<Recorded>) -> Vec<&'static str> {
    std::mem::take(&mut *keyboard.calls.borrow_mut())
}

fn chord(text: &str) -> HotkeyChord {
    HotkeyChord::parse(text).expect("a chord the test writes reads")
}

fn now() -> ShortcutsNow {
    let defaults = default_bindings();
    ShortcutsNow {
        bindings: defaults,
        registered: defaults
            .as_slice()
            .iter()
            .map(|(action, held)| (*action, *held, ShortcutRegistration::Registered))
            .collect(),
    }
}

fn keys_of(window: &ShortcutsWindow) -> Vec<String> {
    window
        .get_rows()
        .iter()
        .map(|row| row.key.to_string())
        .collect()
}

fn messages(window: &ShortcutsWindow) -> Vec<String> {
    window
        .get_messages()
        .iter()
        .map(|line| line.to_string())
        .collect()
}

#[test]
fn the_shortcuts_window_pauses_records_and_always_takes_the_shortcuts_back() {
    let _surface = offscreen::start(520, 640);
    let keyboard = Rc::new(Recorded::default());
    let held = Rc::new(RefCell::new(ChordHeld::NoKey));
    let menu_fails = Rc::new(RefCell::new(false));
    let menus = Rc::new(RefCell::new(0));
    let (ask, commands) = mpsc::channel();
    let said: Rc<RefCell<Vec<String>>> = Rc::default();
    let heard = Rc::clone(&said);
    let shortcuts = Shortcuts::new(
        ShortcutsWindow::new().expect("the shortcuts window must build"),
        Box::new(Shared(Rc::clone(&keyboard))),
        Box::new(Dictated {
            held: Rc::clone(&held),
            menu_fails: Rc::clone(&menu_fails),
            menus: Rc::clone(&menus),
        }),
        ask,
        Box::new(move |line| heard.borrow_mut().push(line)),
    );
    let window = shortcuts.window();
    let weak = Rc::downgrade(&shortcuts);
    *keyboard.window_shown.borrow_mut() = Some(Box::new(move || {
        weak.upgrade()
            .is_some_and(|shortcuts| shortcuts.window().window().is_visible())
    }));

    // ---- labelled once, from the dictionary --------------------------------
    assert_eq!(window.get_window_title(), "Naughty Keyboard - shortcuts");
    assert_eq!(window.get_hints().row_count(), 4);
    assert_eq!(window.get_current_label(), "recording");

    // ---- a click asks the worker to pause, the window waits for it ---------
    shortcuts.open();
    assert_eq!(commands.try_recv(), Ok(Command::Pause));
    assert!(
        !shortcuts.is_open(),
        "shown before the worker paused, it would say paused while the shortcuts were held"
    );
    assert_eq!(calls(&keyboard), vec!["remember"]);
    shortcuts.open();
    assert_eq!(
        commands.try_recv(),
        Err(mpsc::TryRecvError::Empty),
        "a second click while waiting asked twice"
    );

    shortcuts.told(Told::Paused(now()));
    assert!(shortcuts.is_open());
    assert!(window.window().is_visible());
    assert_eq!(
        window.get_rows().row_count(),
        nkb_core::hotkeys::HotkeyAction::COUNT
    );
    assert_eq!(window.get_summary(), "changed: 0 of 14");
    assert_eq!(
        keys_of(window)[0],
        i18n::chord(default_bindings().chord(HotkeyAction::NextValue))
    );

    // ---- the keyboard and the menu, once the handle exists ------------------
    shortcuts.take_with(None);
    assert_eq!(calls(&keyboard), vec!["take"]);
    assert_eq!(*menus.borrow(), 1, "the menu is shut on every opening");
    assert!(said.borrow().is_empty(), "{:?}", said.borrow());
    *menu_fails.borrow_mut() = true;
    *keyboard.take_fails.borrow_mut() = true;
    shortcuts.take_with(None);
    assert_eq!(
        *said.borrow(),
        vec![
            i18n::environment(i18n::Environment::ShortcutsMenuOpen, "refused"),
            i18n::environment(i18n::Environment::ShortcutsFocusNotTaken, "kept elsewhere"),
        ]
    );
    said.borrow_mut().clear();
    let _ = calls(&keyboard);

    // ---- Down, Enter, a chord: the change goes to the worker ---------------
    tap_key(window, Key::DownArrow);
    assert_eq!(window.get_selected(), 1);
    tap_key(window, Key::Return);
    assert_eq!(
        messages(window),
        vec![i18n::shortcut_invite(HotkeyAction::PreviousValue)]
    );
    assert!(window.get_rows().row_data(1).expect("row 1").current);
    tap_key(window, Key::Alt);
    assert_eq!(
        commands.try_recv(),
        Err(mpsc::TryRecvError::Empty),
        "a modifier alone finished the chord"
    );
    let wanted = chord("Alt+Shift+M");
    *held.borrow_mut() = ChordHeld::Chord(wanted);
    // The text Slint gives is not what is read - the system is (`D91`).
    tap(window, "!");
    assert_eq!(
        commands.try_recv(),
        Ok(Command::Shortcut {
            action: HotkeyAction::PreviousValue,
            chord: Some(wanted),
        })
    );

    // ---- the answer moves the row ------------------------------------------
    let bindings = default_bindings()
        .with(&[(HotkeyAction::PreviousValue, wanted)], &|_| None)
        .0;
    let change = ShortcutChange::Changed {
        bindings,
        also: Vec::new(),
        unchecked: None,
    };
    shortcuts.told(Told::Shortcut {
        action: HotkeyAction::PreviousValue,
        change: change.clone(),
        not_saved: None,
    });
    assert_eq!(keys_of(window)[1], i18n::chord(wanted));
    assert_eq!(window.get_summary(), "changed: 1 of 14");
    assert_eq!(
        messages(window),
        i18n::shortcut_answer(
            HotkeyAction::PreviousValue,
            Some(wanted),
            &change,
            default_bindings().chord(HotkeyAction::PreviousValue)
        )
    );

    // ---- Esc closes: keyboard back while shown, shortcuts taken again ------
    tap_key(window, Key::Escape);
    assert!(!shortcuts.is_open());
    assert!(!window.window().is_visible());
    assert_eq!(calls(&keyboard), vec!["give back"]);
    assert_eq!(*keyboard.shown_at_give_back.borrow(), vec![true]);
    assert_eq!(commands.try_recv(), Ok(Command::Resume));

    // ---- leaving for another window: closed, taken again, NOT handed back --
    shortcuts.open();
    assert_eq!(commands.try_recv(), Ok(Command::Pause));
    shortcuts.told(Told::Paused(now()));
    let _ = calls(&keyboard);
    window
        .window()
        .dispatch_event(WindowEvent::WindowActiveChanged(true));
    window
        .window()
        .dispatch_event(WindowEvent::WindowActiveChanged(false));
    assert!(!shortcuts.is_open());
    assert_eq!(commands.try_recv(), Ok(Command::Resume));
    assert!(
        calls(&keyboard).is_empty(),
        "the keyboard was taken from where the tester put it"
    );

    // ---- the close button does what Esc does --------------------------------
    shortcuts.open();
    let _ = commands.try_recv();
    shortcuts.told(Told::Paused(now()));
    let _ = calls(&keyboard);
    window.window().dispatch_event(WindowEvent::CloseRequested);
    assert!(!shortcuts.is_open());
    assert_eq!(calls(&keyboard), vec!["give back"]);
    assert_eq!(commands.try_recv(), Ok(Command::Resume));

    // ---- a pause nobody waits for does not leave the shortcuts paused ------
    shortcuts.told(Told::Paused(now()));
    assert!(!shortcuts.is_open());
    assert_eq!(commands.try_recv(), Ok(Command::Resume));

    // ---- an answer after closing goes to the palette -----------------------
    shortcuts.told(Told::Shortcut {
        action: HotkeyAction::PreviousValue,
        change: ShortcutChange::Taken { chord: wanted },
        not_saved: Some(String::from("not saved")),
    });
    let lines = said.borrow().clone();
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert!(
        lines[0].contains("taken by another application"),
        "{lines:?}"
    );
    assert_eq!(lines[1], "not saved");
}
