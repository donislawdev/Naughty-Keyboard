//! The length limit window as the product wires it (`D178`), with no screen:
//! what its keys do, driven through the toolkit as the system would send them,
//! what it hands the worker, where the keyboard goes, and what it draws.
//!
//! # What this proves that `limit_form.rs` cannot
//!
//! The form decides what a digit means, and its own tests hand it presses
//! directly. Whether the WINDOW hands it the same presses is another question:
//! the keyboard has to stand in the first number, Tab has to reach the second,
//! and the arrows, Enter and Esc have to be caught before a number sees them.
//!
//! # What it cannot
//!
//! Move a real focus between the value window and this one - that is the
//! porcja okien, like every window's.

#![allow(clippy::panic, clippy::expect_used)]

#[allow(dead_code)]
mod offscreen;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver};

use nkb_core::boundary::{self, Limit, Unit};
use nkb_gui::LimitWindow;
use nkb_gui::length_limit::{LengthLimit, Suggested, Wiring};
use nkb_gui::live::{self, Command, InUse};
use nkb_gui::packs::Keyboard;
use slint::platform::WindowEvent;
use slint::{ComponentHandle, Model, SharedString};

const WIDTH: u32 = 520;
const HEIGHT: u32 = 640;

// The toolkit's codes for keys that type nothing, as `i-slint-common` 1.18.1
// `key_codes.rs` names them.
const BACKSPACE: &str = "\u{8}";
const TAB: &str = "\t";
const RETURN: &str = "\n";
const ESCAPE: &str = "\u{1b}";
const UP: &str = "\u{F700}";
const DOWN: &str = "\u{F701}";
const LEFT: &str = "\u{F702}";
const RIGHT: &str = "\u{F703}";

/// Copied rather than read, for the reason the palette test gives.
const ACCENT: (u8, u8, u8) = (0x7A, 0xA2, 0xF7);

fn tap(window: &LimitWindow, text: &str) {
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

fn type_text(window: &LimitWindow, text: &str) {
    for character in text.chars() {
        tap(window, &character.to_string());
    }
}

/// What the keyboard and the value window were asked, in order.
type Record = Rc<RefCell<Vec<&'static str>>>;

struct Keys(Record);

impl Keyboard for Keys {
    fn remember(&self) {
        self.0.borrow_mut().push("remember");
    }
    fn take(&self, _window: Option<u64>) -> Result<(), String> {
        self.0.borrow_mut().push("take");
        Ok(())
    }
    fn give_back(&self, _window: Option<u64>) -> Result<(), String> {
        self.0.borrow_mut().push("give back");
        Ok(())
    }
}

struct Opened {
    limit: Rc<LengthLimit>,
    record: Record,
    commands: Receiver<Command>,
    in_use: InUse,
    suggested: Suggested,
}

impl Opened {
    fn asked(&self) -> Vec<&'static str> {
        std::mem::take(&mut *self.record.borrow_mut())
    }

    fn window(&self) -> &LimitWindow {
        self.limit.window()
    }
}

/// The window, wired as `main.rs` wires it, not opened yet.
fn wired() -> Opened {
    let record = Record::default();
    let (sent, commands) = mpsc::channel();
    let in_use = live::in_use();
    let suggested = Suggested::default();
    let limit = LengthLimit::new(
        LimitWindow::new().expect("the length limit window must build"),
        Box::new(Keys(Rc::clone(&record))),
        Wiring {
            commands: sent,
            in_use: std::sync::Arc::clone(&in_use),
            handed_over: Rc::default(),
            suggested: Rc::clone(&suggested),
            give_way: Box::new({
                let record = Rc::clone(&record);
                move || record.borrow_mut().push("give way")
            }),
        },
        Box::new(|line| panic!("the window said {line:?} in the palette")),
    );
    Opened {
        limit,
        record,
        commands,
        in_use,
        suggested,
    }
}

fn titles(window: &LimitWindow) -> Vec<String> {
    window
        .get_rows()
        .iter()
        .map(|row| row.title.to_string())
        .collect()
}

fn pixel(buffer: &[offscreen::Pixel], x: u32, y: u32) -> (u8, u8, u8) {
    let p = buffer[(y * WIDTH + x) as usize];
    (p.r, p.g, p.b)
}

/// Rows of the picture between `from` and `to` on which at least `run` pixels
/// of the accent stand side by side: a ground, not an edge.
fn accent_ground_rows(buffer: &[offscreen::Pixel], from: u32, to: u32, run: u32) -> usize {
    (from..to)
        .filter(|&y| {
            let mut longest = 0;
            let mut now = 0;
            for x in 0..WIDTH {
                if pixel(buffer, x, y) == ACCENT {
                    now += 1;
                    longest = longest.max(now);
                } else {
                    now = 0;
                }
            }
            longest >= run
        })
        .count()
}

#[test]
fn a_limit_is_typed_with_the_keyboard_alone_and_its_values_go_to_the_palette() {
    let surface = offscreen::start(WIDTH, HEIGHT);
    let opened = wired();
    let window = opened.window();

    opened.limit.open();
    assert_eq!(
        opened.asked(),
        ["give way", "remember"],
        "the value window gives way before the keyboard is remembered"
    );
    opened.limit.take_with(None);
    assert_eq!(opened.asked(), ["take"]);
    assert!(
        window.get_most_has_focus(),
        "the keyboard is not in the first number"
    );

    // Nothing typed: an invitation where the values will be, and no action.
    assert_eq!(window.get_rows().row_count(), 0);
    assert_eq!(
        window.get_empty_text(),
        "Type the limit the field states. The values worth trying are listed here: nothing, then one under, exactly at and one over each bound."
    );
    assert!(!window.get_can_use());
    assert!(!window.get_most_at_fault() && !window.get_least_at_fault());
    tap(window, RETURN);
    assert!(
        opened.commands.try_recv().is_err(),
        "Enter used the values of no limit"
    );
    assert!(
        opened.limit.is_open(),
        "Enter closed a window that did nothing"
    );

    // Digits are the number, letters are not, Backspace takes the last away.
    type_text(window, "5x09");
    tap(window, BACKSPACE);
    assert_eq!(window.get_most(), "50");
    assert_eq!(window.get_least(), "");
    assert!(window.get_can_use());
    let at_most = Limit::new(Unit::Graphemes, None, Some(50)).expect("a limit");
    assert_eq!(
        window.get_rows().row_count(),
        boundary::values(at_most).len()
    );
    assert_eq!(window.get_selected(), 0);
    assert_eq!(titles(window)[0], "Empty value");
    let first = window.get_rows().row_data(1).expect("a second row");
    assert_eq!(
        (
            first.title.as_str(),
            first.detail.as_str(),
            first.badge.as_str()
        ),
        ("49 x a", "graphemes: 49", "inside the limit")
    );
    assert!(window.get_account().starts_with("values: "));

    // Tab is the toolkit's own, to the second number and back.
    tap(window, TAB);
    assert!(
        window.get_least_has_focus() && !window.get_most_has_focus(),
        "Tab did not move the keyboard to the second number"
    );
    type_text(window, "8");
    assert_eq!(
        (window.get_most(), window.get_least()),
        ("50".into(), "8".into())
    );
    tap(window, TAB);
    assert!(window.get_most_has_focus(), "Tab did not come back");

    // The arrows are caught before the number sees them: left and right step
    // the unit and stop at its ends, up and down move the start.
    tap(window, LEFT);
    assert_eq!(window.get_unit(), 0, "stepped before the first unit");
    for _ in 0..5 {
        tap(window, RIGHT);
    }
    assert_eq!(window.get_unit(), 3, "the last unit, and no further");
    assert_eq!(window.get_most(), "50", "an arrow reached the number");
    let limit = Limit::new(Unit::Bytes, Some(8), Some(50)).expect("a limit");
    let values = boundary::values(limit);
    assert_eq!(window.get_rows().row_count(), values.len());
    tap(window, UP);
    assert_eq!(window.get_selected(), 0, "moved above the first value");
    tap(window, DOWN);
    tap(window, DOWN);
    assert_eq!(window.get_selected(), 2);

    // As it stands before Enter: the main action is the one ground of the
    // accent in the footer, and the unit in effect the one in the form.
    window.show().expect("shown");
    let picture = offscreen::draw(&surface, WIDTH, HEIGHT);
    let path = offscreen::save(&picture, WIDTH, HEIGHT, "limit.png");
    assert!(
        accent_ground_rows(&picture, HEIGHT - 60, HEIGHT, 60) >= 8,
        "the main action is not on a ground of the accent. Look at {}",
        path.display()
    );
    assert!(
        accent_ground_rows(&picture, 0, HEIGHT / 2, 24) >= 8,
        "the unit in effect is not filled. Look at {}",
        path.display()
    );

    tap(window, RETURN);
    assert_eq!(
        opened.commands.try_recv(),
        Ok(Command::UseLimit {
            limit,
            start: Some(values[2].id()),
        })
    );
    assert_eq!(opened.asked(), ["give back"]);
    assert!(!opened.limit.is_open());
    assert!(!window.window().is_visible());
}

#[test]
fn the_number_at_fault_is_marked_and_the_list_says_why() {
    let surface = offscreen::start(WIDTH, HEIGHT);
    let opened = wired();
    let window = opened.window();
    opened.limit.open();
    type_text(window, "8");
    tap(window, TAB);
    type_text(window, "50");
    assert!(window.get_least_at_fault() && !window.get_most_at_fault());
    assert!(!window.get_can_use());
    assert_eq!(window.get_rows().row_count(), 0);
    assert_eq!(window.get_account(), "");
    assert_eq!(
        window.get_empty_text(),
        "The least is more than the most, so no length fits both."
    );
    window.show().expect("shown");
    let picture = offscreen::draw(&surface, WIDTH, HEIGHT);
    let path = offscreen::save(&picture, WIDTH, HEIGHT, "limit-at-fault.png");
    assert!(path.exists());

    // A click on the switch and on a row, as a mouse sends them.
    tap(window, BACKSPACE);
    assert!(window.get_can_use(), "5 to 8 is a limit");
    window.invoke_unit_chosen(2);
    assert_eq!(window.get_unit(), 2);
    window.invoke_clicked(3);
    assert_eq!(window.get_selected(), 3);
    window.invoke_clicked(999);
    assert_eq!(window.get_selected(), 3, "a row the list does not have");
    window.invoke_use();
    let limit = Limit::new(Unit::Utf16, Some(5), Some(8)).expect("a limit");
    assert_eq!(
        opened.commands.try_recv(),
        Ok(Command::UseLimit {
            limit,
            start: Some(boundary::values(limit)[3].id()),
        })
    );
}

#[test]
fn it_opens_on_the_number_from_the_search_or_on_the_limit_in_use() {
    let _surface = offscreen::start(WIDTH, HEIGHT);
    let opened = wired();
    let window = opened.window();

    opened.suggested.set(Some(255));
    opened.limit.open();
    assert_eq!(
        (window.get_most(), window.get_least()),
        ("255".into(), "".into())
    );
    assert_eq!(window.get_unit(), 0);
    assert!(window.get_can_use());
    assert_eq!(
        opened.suggested.get(),
        None,
        "the number outlived its opening"
    );

    // Esc closes, the keyboard goes back, nothing goes to the palette.
    opened.asked();
    tap(window, ESCAPE);
    assert_eq!(opened.asked(), ["give back"]);
    assert!(opened.commands.try_recv().is_err());

    // Nothing is remembered: the next opening is empty.
    opened.limit.open();
    assert_eq!(
        (window.get_most(), window.get_least()),
        ("".into(), "".into())
    );
    opened.limit.close();

    // A limit in use is where the window opens, to change it from there.
    let limit = Limit::new(Unit::Utf16, Some(8), Some(50)).expect("a limit");
    opened.in_use.lock().expect("a fresh lock").made = Some(limit);
    opened.limit.open();
    assert_eq!(
        (window.get_most(), window.get_least()),
        ("50".into(), "8".into())
    );
    assert_eq!(window.get_unit(), 2);

    // Asked again while open, it keeps what is typed.
    type_text(window, "0");
    opened.asked();
    opened.limit.open();
    assert_eq!(window.get_most(), "500");
    assert_eq!(
        opened.asked(),
        Vec::<&str>::new(),
        "an open window asked again"
    );

    // The tester went to another window: closed, and the keyboard stays there.
    window.invoke_deactivated();
    assert!(!opened.limit.is_open());
    assert_eq!(
        opened.asked(),
        Vec::<&str>::new(),
        "the keyboard was taken back"
    );
}
