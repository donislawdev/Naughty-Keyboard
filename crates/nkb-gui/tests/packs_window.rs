//! The pack window as the product wires it - keys in, a choice out, the
//! keyboard handed back - with no screen and no system focus to move.
//!
//! # What this proves that `picker.rs` and `packs_keys.rs` cannot
//!
//! `picker.rs` decides what Enter comes to, with no window. `packs_keys.rs`
//! sends keys to a window wired by hand. Neither sees the product's own wiring
//! in `nkb_gui::packs`: which callback moves the selection, whether Enter on
//! another pack reaches the worker as a command, whether Escape closes without
//! one, whether the keyboard is handed back on EVERY way out the window offers -
//! and NOT when the tester leaves for another window. Here the keys
//! go to the window as the system would send them, and the answers are read
//! from the channel, the window and a keyboard that records what it was asked.
//!
//! # What it cannot
//!
//! Move a real focus. The keyboard here is a stand-in, and whether the system
//! actually hands the foreground over is measured on live windows
//! (`tools/sonda-okno-paczek`, and the level 1 proof of K3.2d).
//!
//! One test in its own binary for the reason `palette_appearance.rs` gives:
//! one platform per process.

#![allow(clippy::panic, clippy::expect_used)]

#[allow(dead_code)]
mod offscreen;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;

use nkb_core::hotkeys::HotkeyChord;
use nkb_gui::PacksWindow;
use nkb_gui::live::{self, Command};
use nkb_gui::packs::{Keyboard, Packs};
use slint::platform::WindowEvent;
use slint::{ComponentHandle, Model, SharedString};

const RETURN: &str = "\n";
const ESCAPE: &str = "\u{1b}";
const DOWN: &str = "\u{F701}";

fn tap(window: &PacksWindow, text: &str) {
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

fn type_text(window: &PacksWindow, text: &str) {
    for c in text.chars() {
        tap(window, &c.to_string());
    }
}

/// A keyboard that records what it was asked, and fails when told to.
#[derive(Default)]
struct Recorded {
    calls: RefCell<Vec<&'static str>>,
    take_fails: RefCell<bool>,
    give_back_fails: RefCell<bool>,
    /// Whether the window was still shown at each give-back. The order is
    /// measured (`slint.md` 2.30): once hidden, the system has already given
    /// the foreground to somebody, and ours is no longer ours to give.
    shown_at_give_back: RefCell<Vec<bool>>,
    window_shown: RefCell<Option<Box<dyn Fn() -> bool>>>,
}

/// The stand-in the window gets, sharing its record with the test.
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
        if *self.0.give_back_fails.borrow() {
            Err(String::from("stayed here"))
        } else {
            Ok(())
        }
    }
}

fn calls(keyboard: &Rc<Recorded>) -> Vec<&'static str> {
    std::mem::take(&mut *keyboard.calls.borrow_mut())
}

fn titles(window: &PacksWindow) -> Vec<String> {
    window
        .get_rows()
        .iter()
        .map(|row| row.title.to_string())
        .collect()
}

fn selected_title(window: &PacksWindow) -> String {
    let at = usize::try_from(window.get_selected()).expect("a row is selected");
    titles(window)[at].clone()
}

#[test]
fn the_pack_window_chooses_closes_and_always_hands_the_keyboard_back() {
    let _surface = offscreen::start(440, 560);
    let keyboard = Rc::new(Recorded::default());
    let (choose, commands) = mpsc::channel();
    let in_use = live::in_use();
    // No next value yet - the window then opens on the pack in use (UX2).
    *in_use.lock().expect("a fresh lock") = live::InUseNow {
        pack: Some(String::from("whitespace")),
        next: None,
        restart: HotkeyChord::parse("Alt+Shift+F9").ok(),
    };
    let in_use_slot = std::sync::Arc::clone(&in_use);
    let said: Rc<RefCell<Vec<String>>> = Rc::default();
    let heard = Rc::clone(&said);
    let packs = Packs::new(
        PacksWindow::new().expect("the pack window must build"),
        Box::new(Shared(Rc::clone(&keyboard))),
        choose,
        in_use,
        Box::new(move |line| heard.borrow_mut().push(line)),
    );
    let window = packs.window();
    let weak = std::rc::Rc::downgrade(&packs);
    *keyboard.window_shown.borrow_mut() = Some(Box::new(move || {
        weak.upgrade()
            .is_some_and(|packs| packs.window().window().is_visible())
    }));

    // ---- labelled once, from the dictionary --------------------------------
    assert_eq!(window.get_window_title(), "Naughty Keyboard - find a value");
    assert_eq!(
        window.get_hints().row_count(),
        3,
        "Enter, Esc and the arrows are named in the footer"
    );

    // ---- opening on the pack in use ----------------------------------------
    packs.open();
    assert!(packs.is_open());
    assert_eq!(
        calls(&keyboard),
        vec!["remember"],
        "remembered before showing - the keyboard is taken once the handle exists"
    );
    assert_eq!(
        titles(window).len(),
        1 + 1 + 12 + 1 + 9,
        "the restart row and the values of the pack in use, then every shipped pack, each \
         section under a heading: {:?}",
        titles(window)
    );
    assert_eq!(
        window.get_rows().iter().filter(|row| row.heading).count(),
        2
    );
    assert_eq!(titles(window)[0], "Values in Whitespace");
    // UX-GUI-007: the way back to the start, first in the section, as a row
    // of the shortcuts window's shape - one line, the shortcut of the table in
    // effect at its end, choosable.
    let restart = window.get_rows().row_data(1).expect("a second row");
    assert_eq!(restart.title, "Restart pack");
    assert!(restart.enabled && !restart.heading && restart.single_line);
    assert!(restart.has_key);
    assert_eq!(restart.key, "Alt+Shift+F9");
    assert_eq!(
        selected_title(window),
        "Whitespace",
        "with no next value known it opens on the pack in use"
    );
    assert!(
        window.get_notes().row_count() >= 1,
        "the sources it did not read are said below the list"
    );

    // ---- asked again while open: the same window, the same place ------------
    packs.open();
    assert!(
        calls(&keyboard).is_empty(),
        "reopening remembered the pack window itself as the place to go back to"
    );

    // ---- typing filters, Enter on another pack reaches the worker -----------
    // The id, because "unicode" alone also matches the whitespace pack, whose
    // description is about Unicode spaces - the query searches descriptions.
    type_text(window, "unicode-text");
    assert_eq!(window.get_query(), "unicode-text");
    assert_eq!(
        titles(window),
        vec![String::from("Packs"), String::from("Unicode and text")],
        "the pack, and none of its values - a value matches on its own fields"
    );
    tap(window, RETURN);
    assert_eq!(
        commands.try_recv(),
        Ok(Command::Choose(String::from("unicode-text")))
    );
    assert!(!packs.is_open(), "a choice closes the window");
    assert_eq!(calls(&keyboard), vec!["give back"]);
    assert!(said.borrow().is_empty(), "{:?}", said.borrow());

    // ---- a value found in another pack: Enter makes it the next one ---------
    // The owner's case (UX-GUI-002): PESEL is a value of `locale-pl`.
    packs.open();
    let _ = calls(&keyboard);
    type_text(window, "pesel");
    assert_eq!(titles(window)[0], "Values");
    tap(window, RETURN);
    assert_eq!(
        commands.try_recv(),
        Ok(Command::ChooseValue {
            pack: String::from("locale-pl"),
            value: String::from("pesel-valid"),
        })
    );
    assert!(!packs.is_open());
    assert_eq!(calls(&keyboard), vec!["give back"]);

    // ---- with a next value known, the window opens on it --------------------
    in_use_slot.lock().expect("the slot").next = Some(String::from("leading-space"));
    packs.open();
    let _ = calls(&keyboard);
    assert_eq!(selected_title(window), "Leading space");
    tap(window, ESCAPE);
    in_use_slot.lock().expect("the slot").next = None;
    let _ = calls(&keyboard);

    // ---- Escape closes and chooses nothing ---------------------------------
    packs.open();
    assert_eq!(
        window.get_query(),
        "",
        "every opening starts a fresh search"
    );
    let _ = calls(&keyboard);
    tap(window, DOWN);
    tap(window, ESCAPE);
    assert!(!packs.is_open());
    assert_eq!(commands.try_recv(), Err(mpsc::TryRecvError::Empty));
    assert_eq!(calls(&keyboard), vec!["give back"]);

    // ---- Enter on the pack in use only closes ------------------------------
    // `7 / 34` must not go back to the start because the tester confirmed
    // what was already open.
    packs.open();
    let _ = calls(&keyboard);
    tap(window, RETURN);
    assert!(!packs.is_open());
    assert_eq!(commands.try_recv(), Err(mpsc::TryRecvError::Empty));
    assert_eq!(calls(&keyboard), vec!["give back"]);

    // ---- nothing matches: Enter keeps the window open -----------------------
    packs.open();
    type_text(window, "qqqq");
    assert!(titles(window).is_empty());
    tap(window, RETURN);
    assert!(
        packs.is_open(),
        "closing on no match would look like a choice"
    );
    assert_eq!(commands.try_recv(), Err(mpsc::TryRecvError::Empty));

    // ---- the window's own close button hands the keyboard back as well -----
    let _ = calls(&keyboard);
    window.window().dispatch_event(WindowEvent::CloseRequested);
    assert!(!packs.is_open());
    assert_eq!(calls(&keyboard), vec!["give back"]);

    // ---- the tester clicks another window: closed, and NOT handed back ------
    // The keyboard is where the tester put it, and handing it back would take
    // it from there (`OBS-147`). What the system sends is the window going
    // inactive - Slint's `WindowActiveChanged`, reaching the query line as
    // focus lost for `window-activation`.
    packs.open();
    type_text(window, "uni");
    let _ = calls(&keyboard);
    window
        .window()
        .dispatch_event(WindowEvent::WindowActiveChanged(true));
    assert!(
        packs.is_open(),
        "becoming active is not leaving - the window must survive its own opening"
    );
    window
        .window()
        .dispatch_event(WindowEvent::WindowActiveChanged(false));
    assert!(
        !packs.is_open(),
        "a window the tester left stays on top without the keyboard (OBS-147)"
    );
    assert!(!window.window().is_visible(), "closed means hidden");
    assert!(
        calls(&keyboard).is_empty(),
        "leaving hands nothing back - the keyboard stays where the tester put it"
    );
    assert_eq!(
        commands.try_recv(),
        Err(mpsc::TryRecvError::Empty),
        "leaving is not a choice"
    );
    assert!(said.borrow().is_empty(), "{:?}", said.borrow());

    // ---- the next opening after leaving is a fresh one ----------------------
    packs.open();
    assert_eq!(
        calls(&keyboard),
        vec!["remember"],
        "remembered anew - the old place to go back to is gone"
    );
    assert_eq!(window.get_query(), "", "the search starts empty again");

    // ---- our own hand-back makes the window inactive too: nothing more ------
    // Enter and Escape give the keyboard back, the system then reports the
    // window inactive - after the window is closed. That report must not do
    // anything a second time.
    tap(window, ESCAPE);
    assert_eq!(calls(&keyboard), vec!["give back"]);
    window
        .window()
        .dispatch_event(WindowEvent::WindowActiveChanged(false));
    assert!(!packs.is_open());
    assert!(calls(&keyboard).is_empty());
    assert!(said.borrow().is_empty(), "{:?}", said.borrow());

    // ---- a shortcut still held as the window comes in is not left held ------
    // The window can get its thread's keyboard while the shortcut is down:
    // winit reports Alt and Shift as pressed then, and the tester's release goes
    // to the application under test (`OBS-151`). The window becoming active
    // must forget them, or every letter of the search arrives with Alt and is
    // taken for a shortcut - the tester types and nothing happens.
    packs.open();
    for key in [slint::platform::Key::Alt, slint::platform::Key::Shift] {
        window
            .window()
            .dispatch_event(WindowEvent::KeyPressed { text: key.into() });
    }
    window
        .window()
        .dispatch_event(WindowEvent::WindowActiveChanged(true));
    type_text(window, "uni");
    assert_eq!(
        window.get_query(),
        "uni",
        "letters after the window came in with the shortcut held (OBS-151)"
    );
    assert!(packs.is_open(), "coming in is not leaving");
    tap(window, ESCAPE);
    assert!(!packs.is_open());
    let _ = calls(&keyboard);
    assert!(said.borrow().is_empty(), "{:?}", said.borrow());

    // ---- a keyboard that stays elsewhere is said, in both directions --------
    *keyboard.take_fails.borrow_mut() = true;
    packs.open();
    packs.take_with(Some(1));
    *keyboard.give_back_fails.borrow_mut() = true;
    tap(window, ESCAPE);
    let lines = said.borrow().clone();
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert!(
        lines[0].starts_with("The pack window could not take the keyboard focus: kept elsewhere."),
        "{lines:?}"
    );
    assert!(
        lines[1].starts_with("The keyboard focus could not be returned")
            && lines[1].contains("stayed here"),
        "{lines:?}"
    );
    said.borrow_mut().clear();
    *keyboard.take_fails.borrow_mut() = false;
    *keyboard.give_back_fails.borrow_mut() = false;

    // ---- a choice with nobody left to carry it out is not lost in silence ---
    drop(commands);
    packs.open();
    type_text(window, "unicode-text");
    tap(window, RETURN);
    assert!(!packs.is_open());
    assert_eq!(
        said.borrow().as_slice(),
        [String::from(
            "The shortcut listener has stopped, so no shortcut will respond. Restart the palette."
        )]
    );

    // ---- every give-back happened BEFORE the window went away ---------------
    let order = keyboard.shown_at_give_back.borrow().clone();
    assert!(
        order.len() >= 6 && order.iter().all(|&shown| shown),
        "the keyboard was handed back after the window was hidden: {order:?}"
    );
}
