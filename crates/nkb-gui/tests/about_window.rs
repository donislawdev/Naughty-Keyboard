//! The About window as the product wires it - opened with the keyboard taken,
//! closed by Esc, by its Close button and by the window's close button with the
//! keyboard handed back, and closed WITHOUT handing it back when the tester
//! leaves for another window - with no screen and no system focus to move.
//!
//! # What it cannot
//!
//! Move a real focus: whether the keyboard goes back to the field under test
//! on a live window is the porcja okien, like every window's (`D105`).

#![allow(clippy::panic, clippy::expect_used)]

#[allow(dead_code)]
mod offscreen;

use std::cell::RefCell;
use std::rc::Rc;

use nkb_adapters::i18n::{self, Environment};
use nkb_gui::AboutWindow;
use nkb_gui::about::{About, copyright, facts};
use nkb_gui::packs::Keyboard;
use slint::platform::{Key, WindowEvent};
use slint::{ComponentHandle, Model, SharedString};

#[derive(Default)]
struct Recorded {
    calls: RefCell<Vec<&'static str>>,
    take_fails: RefCell<bool>,
}

struct Shared(Rc<Recorded>);

impl Keyboard for Shared {
    fn remember(&self) {
        self.0.calls.borrow_mut().push("remember");
    }
    fn take(&self, _window: Option<u64>) -> Result<(), String> {
        self.0.calls.borrow_mut().push("take");
        if *self.0.take_fails.borrow() {
            return Err("kept elsewhere".to_owned());
        }
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

fn tap(window: &AboutWindow, key: Key) {
    let text = SharedString::from(key);
    window
        .window()
        .dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
    window
        .window()
        .dispatch_event(WindowEvent::KeyReleased { text });
}

#[test]
fn the_about_window_opens_with_the_keyboard_and_gives_it_back_on_every_way_out() {
    let _surface = offscreen::start(480, 900);
    let keyboard = Rc::new(Recorded::default());
    let said: Rc<RefCell<Vec<String>>> = Rc::default();
    let heard = Rc::clone(&said);
    let about = About::new(
        AboutWindow::new().expect("the About window must build"),
        Box::new(Shared(Rc::clone(&keyboard))),
        Box::new(move |line| heard.borrow_mut().push(line)),
    );
    let window = about.window();

    // ---- labelled once: the dictionary's words and the manifest's facts -----
    assert_eq!(window.get_window_title(), "Naughty Keyboard - about");
    assert_eq!(window.get_heading(), "About Naughty Keyboard");
    let shown: Vec<(String, String)> = window
        .get_facts()
        .iter()
        .map(|fact| (fact.label.to_string(), fact.value.to_string()))
        .collect();
    let wanted: Vec<(String, String)> = facts()
        .into_iter()
        .map(|(label, value)| (label.to_owned(), value))
        .collect();
    assert_eq!(shown, wanted);
    assert_eq!(window.get_copyright(), copyright());
    assert_eq!(window.get_close_label(), "Close");
    assert!(!about.is_open());
    assert!(!window.window().is_visible());

    // ---- opening remembers who held the keyboard, BEFORE showing -----------
    about.open();
    assert!(about.is_open());
    assert!(window.window().is_visible());
    let opening = calls(&keyboard);
    assert_eq!(opening.first(), Some(&"remember"), "{opening:?}");
    // No handle offscreen: the poll asks again, and the test reaches the end
    // of it directly.
    about.take_with(None);
    assert_eq!(calls(&keyboard).last(), Some(&"take"));
    assert!(said.borrow().is_empty(), "{:?}", said.borrow());

    // ---- a keyboard kept elsewhere is said, not swallowed ------------------
    *keyboard.take_fails.borrow_mut() = true;
    about.take_with(None);
    assert_eq!(
        *said.borrow(),
        vec![i18n::environment(
            Environment::AboutFocusNotTaken,
            "kept elsewhere"
        )]
    );
    said.borrow_mut().clear();
    *keyboard.take_fails.borrow_mut() = false;
    let _ = calls(&keyboard);

    // ---- any other key travels on, Esc closes and hands the keyboard back --
    tap(window, Key::Return);
    assert!(about.is_open(), "a key other than Esc closed the window");
    tap(window, Key::Escape);
    assert!(!about.is_open());
    assert!(!window.window().is_visible());
    assert_eq!(calls(&keyboard), vec!["give back"]);
    // A second close is nothing at all.
    about.close();
    assert!(calls(&keyboard).is_empty());

    // ---- the Close button: the same way out ---------------------------------
    about.open();
    let _ = calls(&keyboard);
    window.invoke_close_clicked();
    assert!(!about.is_open());
    assert_eq!(calls(&keyboard), vec!["give back"]);

    // ---- the window's close button: the same, and the window stays built ---
    about.open();
    let _ = calls(&keyboard);
    window.window().dispatch_event(WindowEvent::CloseRequested);
    assert!(!about.is_open());
    assert!(!window.window().is_visible());
    assert_eq!(calls(&keyboard), vec!["give back"]);

    // ---- leaving for another window closes it and leaves the keyboard -------
    about.open();
    window
        .window()
        .dispatch_event(WindowEvent::WindowActiveChanged(true));
    let _ = calls(&keyboard);
    window
        .window()
        .dispatch_event(WindowEvent::WindowActiveChanged(false));
    assert!(
        !about.is_open(),
        "a window the tester left stays on top (OBS-147)"
    );
    assert!(
        calls(&keyboard).is_empty(),
        "the keyboard was taken back from where the tester put it"
    );
    assert!(said.borrow().is_empty(), "{:?}", said.borrow());
}
