//! The pack window's keyboard, driven through the toolkit rather than around it.
//!
//! # What this proves that `query.rs` cannot
//!
//! `nkb_gui::query` decides what a press means, and its own tests hand it
//! presses directly. Whether the WINDOW hands it the same presses is a
//! different question: the focus has to be on the query line, the picker's
//! keys have to be caught before the line sees them, and the modifiers have to
//! arrive as the toolkit tracks them from the modifier keys' own events. So
//! this sends key events to the window, as the system would, and reads back
//! the query the window draws.
//!
//! 🔴 The promise it holds up: `Ctrl+V`, `Ctrl+C`, `Ctrl+X`, `Shift+Insert`
//! and `Ctrl+Insert` do nothing to the query - there is no clipboard behind
//! the line to read or write (`OBS-145`). The positive control is the letters
//! around them, which do arrive.
//!
//! # What it cannot
//!
//! `AltGr`. The toolkit tells `AltGr` from `Ctrl+Alt` with the text the system
//! produced WITHOUT the modifiers, and only the system's own keyboard event
//! carries that - a headless window cannot be handed it. That is measured on a
//! live window with a real layout (`slint.md` 2.29).
//!
//! Its own binary for the reason `palette_appearance.rs` gives: one platform
//! per process.

#![allow(clippy::panic, clippy::expect_used)]

#[allow(dead_code)]
mod offscreen;

use std::cell::RefCell;
use std::rc::Rc;

use nkb_gui::PacksWindow;
use nkb_gui::query::{KeyPress, Pressed, Query};
use slint::platform::WindowEvent;
use slint::{ComponentHandle, SharedString};

// The toolkit's codes for keys that type nothing, as `i-slint-common` 1.18.1
// `key_codes.rs` names them.
const CONTROL: &str = "\u{11}";
const SHIFT: &str = "\u{10}";
const BACKSPACE: &str = "\u{8}";
const INSERT: &str = "\u{F727}";
const UP: &str = "\u{F700}";
const DOWN: &str = "\u{F701}";
const RETURN: &str = "\n";
const ESCAPE: &str = "\u{1b}";

fn down(window: &PacksWindow, text: &str) {
    window.window().dispatch_event(WindowEvent::KeyPressed {
        text: SharedString::from(text),
    });
}

fn up(window: &PacksWindow, text: &str) {
    window.window().dispatch_event(WindowEvent::KeyReleased {
        text: SharedString::from(text),
    });
}

fn tap(window: &PacksWindow, text: &str) {
    down(window, text);
    up(window, text);
}

fn chord(window: &PacksWindow, modifier: &str, key: &str) {
    down(window, modifier);
    tap(window, key);
    up(window, modifier);
}

#[test]
fn the_query_line_types_letters_and_nothing_from_the_clipboard_shortcuts() {
    let _surface = offscreen::start(440, 560);
    let window = PacksWindow::new().expect("the pack window must build");

    // The product's wiring in miniature: Rust keeps the query and sends it
    // back. K3.2 puts the same three lines into the worker.
    let query = Rc::new(RefCell::new(Query::default()));
    let weak = window.as_weak();
    let kept = query.clone();
    window.on_query_pressed(move |text, control, alt, meta| {
        let mut query = kept.borrow_mut();
        let pressed = query.press(KeyPress {
            text: text.as_str(),
            control,
            alt,
            meta,
        });
        if pressed == Pressed::Changed
            && let Some(window) = weak.upgrade()
        {
            window.set_query(query.as_str().into());
        }
        pressed != Pressed::NotOurs
    });

    // What the picker's own keys fired, in order.
    let picker_keys: Rc<RefCell<Vec<&str>>> = Rc::default();
    let heard = picker_keys.clone();
    window.on_previous(move || heard.borrow_mut().push("previous"));
    let heard = picker_keys.clone();
    window.on_next(move || heard.borrow_mut().push("next"));
    let heard = picker_keys.clone();
    window.on_choose(move || heard.borrow_mut().push("choose"));
    let heard = picker_keys.clone();
    window.on_cancel(move || heard.borrow_mut().push("cancel"));

    window.show().expect("the pack window must show");

    // Before the focus: letters go nowhere. The negative control for the rest.
    tap(&window, "z");
    assert_eq!(
        window.get_query(),
        "",
        "a letter reached the line without the focus"
    );

    window.invoke_focus_search();
    for letter in ["u", "n", "i"] {
        tap(&window, letter);
    }
    assert_eq!(
        window.get_query(),
        "uni",
        "typed letters must reach the query"
    );

    // The clipboard shortcuts, each between letters that DO arrive, so a key
    // event that stopped arriving at all cannot pass for a refused shortcut.
    chord(&window, CONTROL, "v");
    tap(&window, "c");
    chord(&window, CONTROL, "c");
    tap(&window, "o");
    chord(&window, CONTROL, "x");
    chord(&window, SHIFT, INSERT);
    chord(&window, CONTROL, INSERT);
    tap(&window, "d");
    assert_eq!(
        window.get_query(),
        "unicod",
        "a clipboard shortcut changed the query"
    );

    // Shift is a modifier, not a shortcut: it types the capital.
    chord(&window, SHIFT, "E");
    assert_eq!(window.get_query(), "unicodE");

    tap(&window, BACKSPACE);
    tap(&window, BACKSPACE);
    assert_eq!(window.get_query(), "unico", "Backspace must erase");

    // The picker's keys are caught before the line: each fires its callback
    // once and none of them reaches the query.
    for key in [UP, DOWN, RETURN, ESCAPE] {
        tap(&window, key);
    }
    assert_eq!(
        *picker_keys.borrow(),
        vec!["previous", "next", "choose", "cancel"]
    );
    assert_eq!(
        window.get_query(),
        "unico",
        "a picker key reached the query"
    );
}
