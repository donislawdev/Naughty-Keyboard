//! The welcome window as the product wires it - keys into the box, the
//! palette's clearing recipe understood, closing remembered through the worker
//! and the keyboard handed back - with no screen and no system focus to move.
//!
//! # What this proves that `trial.rs` cannot
//!
//! `trial.rs` decides what a key does to the box, with no window. Here the keys
//! go to the window as the system would send them - Shift held as a key of its
//! own, the way the palette's `SendInput` holds it - and the answers are read
//! from the window, the channel to the worker and a keyboard that records what
//! it was asked.
//!
//! # What it cannot
//!
//! Move a real focus, or send a value from the palette into a window of the
//! palette's own process - that is the porcja okien (`D105`, unmeasured).

#![allow(clippy::panic, clippy::expect_used)]

#[allow(dead_code)]
mod offscreen;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;

use nkb_core::hotkeys::HotkeyAction;
use nkb_gui::WelcomeWindow;
use nkb_gui::live::Command;
use nkb_gui::packs::Keyboard;
use nkb_gui::welcome::Welcome;
use slint::platform::WindowEvent;
use slint::{ComponentHandle, Model, SharedString};

const SHIFT: &str = "\u{10}";
const HOME: &str = "\u{F729}";
const END: &str = "\u{F72B}";
const DELETE: &str = "\u{7f}";

fn press(window: &WelcomeWindow, text: &str) {
    window.window().dispatch_event(WindowEvent::KeyPressed {
        text: SharedString::from(text),
    });
}

fn release(window: &WelcomeWindow, text: &str) {
    window.window().dispatch_event(WindowEvent::KeyReleased {
        text: SharedString::from(text),
    });
}

fn tap(window: &WelcomeWindow, text: &str) {
    press(window, text);
    release(window, text);
}

fn type_text(window: &WelcomeWindow, text: &str) {
    for c in text.chars() {
        tap(window, &c.to_string());
    }
}

#[derive(Default)]
struct Recorded {
    calls: RefCell<Vec<&'static str>>,
}

struct Shared(Rc<Recorded>);

impl Keyboard for Shared {
    fn remember(&self) {
        self.0.calls.borrow_mut().push("remember");
    }
    fn take(&self, _window: Option<u64>) -> Result<(), String> {
        self.0.calls.borrow_mut().push("take");
        Ok(())
    }
    fn give_back(&self, _window: Option<u64>) -> Result<(), String> {
        self.0.calls.borrow_mut().push("give back");
        Ok(())
    }
}

fn calls(keyboard: &Rc<Recorded>) -> Vec<&'static str> {
    std::mem::take(&mut *keyboard.calls.borrow_mut())
}

/// The chord of `action` in the default table of the system the test runs on -
/// `Alt+Shift` on Windows and Linux, `Alt+Win` on macOS (`D82`). The sentences
/// are composed from it, so the test holds on every system rather than on the
/// one it was written on: measured on macOS 2026-10-07, the first full run there
/// since this test was written with the Windows chords spelled out.
fn default_chord(action: HotkeyAction) -> String {
    nkb_adapters::default_bindings().chord(action).text()
}

#[test]
fn the_welcome_window_takes_a_value_and_remembers_it_was_closed() {
    let _surface = offscreen::start(480, 900);
    let keyboard = Rc::new(Recorded::default());
    let (asks, worker) = mpsc::channel();
    let said: Rc<RefCell<Vec<String>>> = Rc::default();
    let heard = Rc::clone(&said);
    let bindings = nkb_adapters::default_bindings();
    // The palette appearing (`D109`): recorded with whether the welcome was
    // still shown at that moment - it must be, or the event loop ends.
    let built = WelcomeWindow::new().expect("the welcome window must build");
    let seen = built.as_weak();
    let appeared: Rc<RefCell<Vec<bool>>> = Rc::default();
    let record = Rc::clone(&appeared);
    let welcome = Welcome::new(
        built,
        Box::new(Shared(Rc::clone(&keyboard))),
        asks,
        &bindings,
        Box::new(move |line| heard.borrow_mut().push(line)),
        Box::new(move || {
            record
                .borrow_mut()
                .push(seen.upgrade().is_some_and(|w| w.window().is_visible()));
        }),
    );
    let window = welcome.window();

    // ---- labelled once, from the dictionary, with the shortcuts in effect ----
    assert_eq!(window.get_window_title(), "Naughty Keyboard - welcome");
    assert_eq!(
        window.get_try_it(),
        format!(
            "Click the box below and press {}.",
            default_chord(HotkeyAction::NextValue)
        )
    );
    assert_eq!(window.get_hints().row_count(), 3);
    assert_eq!(window.get_box_facts(), "Nothing has arrived yet.");

    // ---- opening names the pack in use and takes the keyboard ---------------
    welcome.open(Some("whitespace"));
    assert!(welcome.is_open());
    assert_eq!(
        window.get_ready(),
        format!(
            "Start testing opens the palette: a small window that stays on top and shows what \
             each press sends. It starts on the pack whitespace. Press {} to choose another \
             pack or value.",
            default_chord(HotkeyAction::OpenPacks)
        )
    );
    assert_eq!(
        calls(&keyboard),
        vec!["remember"],
        "remembered before showing - taken once the window has a handle"
    );

    // ---- a value arrives, counted, invisible characters marked --------------
    type_text(window, "Kowalski ");
    assert_eq!(welcome.box_text(), "Kowalski ");
    assert_eq!(window.get_box_before(), "Kowalski\u{2423}");
    assert_eq!(window.get_box_after(), "");
    assert_eq!(
        window.get_box_facts(),
        "graphemes: 9, code points: 9, bytes: 9, UTF-16 units: 9"
    );

    // ---- the palette's clearing recipe: Home, Shift+End, Delete -------------
    tap(window, HOME);
    assert_eq!(window.get_box_before(), "");
    assert_eq!(window.get_box_after(), "Kowalski\u{2423}");
    press(window, SHIFT);
    tap(window, END);
    release(window, SHIFT);
    tap(window, DELETE);
    assert_eq!(welcome.box_text(), "", "the line was cleared");
    type_text(window, " Kowalski");
    assert_eq!(welcome.box_text(), " Kowalski");
    assert!(said.borrow().is_empty(), "nothing went wrong to say");
    assert!(
        appeared.borrow().is_empty(),
        "the palette appeared while the welcome was open (the owner's point 1)"
    );

    // ---- closing: keyboard back, the palette shown BEFORE the welcome goes,
    // ---- remembered by the worker ----------------------------------------------
    window.invoke_start();
    assert!(!welcome.is_open());
    assert!(!window.window().is_visible());
    assert_eq!(calls(&keyboard), vec!["give back"]);
    assert_eq!(
        *appeared.borrow(),
        vec![true],
        "the palette appears once, while the welcome is still shown"
    );
    assert_eq!(worker.try_recv(), Ok(Command::WelcomeDone));
    // A second close is nothing at all.
    welcome.close();
    assert!(calls(&keyboard).is_empty());
    assert!(worker.try_recv().is_err());
    assert_eq!(appeared.borrow().len(), 1, "the palette appears once");
}

#[test]
fn closing_the_palette_hides_the_welcome_without_remembering_it() {
    let _surface = offscreen::start(480, 900);
    let keyboard = Rc::new(Recorded::default());
    let (asks, worker) = mpsc::channel();
    let welcome = Welcome::new(
        WelcomeWindow::new().expect("the welcome window must build"),
        Box::new(Shared(Rc::clone(&keyboard))),
        asks,
        &nkb_adapters::default_bindings(),
        Box::new(|_| {}),
        Box::new(|| panic!("closing the palette made the palette appear")),
    );
    welcome.open(None);
    assert_eq!(
        welcome.window().get_ready(),
        format!(
            "Start testing opens the palette: a small window that stays on top and shows what \
             each press sends. Press {} to choose a pack or a value.",
            default_chord(HotkeyAction::OpenPacks)
        )
    );
    let _ = calls(&keyboard);

    welcome.dismiss();
    assert!(!welcome.is_open());
    assert!(!welcome.window().window().is_visible());
    assert!(
        worker.try_recv().is_err(),
        "a tester who closed the palette has not closed the welcome"
    );
    assert!(calls(&keyboard).is_empty());
}
