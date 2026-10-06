//! How a typed value meets the field (`UX-GUI-005`, `D101`) - the line that
//! says it and the button for the other way, MEASURED on the rendered palette,
//! and the click followed to its callback.
//!
//! # What this proves that reading `palette.slint` cannot
//!
//! The line stands under the hint bar from the first frame, so the tester
//! knows the line is cleared before a value goes - the audit's complaint was
//! learning it from the marker afterwards. Its button stands at the right end
//! of the same row, above the clipboard button. Neither stands in clipboard
//! mode, where nothing is cleared whatever the choice, nor in the compact
//! palette, which shows no hint bar. While a value goes the button stays,
//! faded and deaf: the worker answers between presses. Each piece is found as
//! the ink its words add to the render, its absence as no ink at all.
//!
//! Its own binary for the reason `palette_appearance.rs` gives: one platform
//! per process.

#![allow(clippy::panic, clippy::expect_used)]

#[allow(dead_code)]
mod offscreen;

use std::cell::Cell;
use std::rc::Rc;

use nkb_gui::Palette;
use offscreen::{Ink, added, click};
use slint::platform::software_renderer::MinimalSoftwareWindow;
use slint::{ComponentHandle, SharedString};

const WIDTH: u32 = 420;
const HEIGHT: u32 = 460;

const STATE: &str = "The line is cleared before each value";
const SWITCH: &str = "Insert at cursor";
const WAY_IN: &str = "Use clipboard mode";

/// A piece as the pixels its words change, or `None` when it is not drawn.
/// The words are back in place on return, so a click aims at the piece as the
/// tester sees it - the lesson of mutation M435 in `palette_copy.rs`.
fn find(
    palette: &Palette,
    surface: &Rc<MinimalSoftwareWindow>,
    words: fn(&Palette, SharedString),
    text: &str,
) -> Option<Ink> {
    words(palette, "".into());
    let without = offscreen::draw(surface, WIDTH, HEIGHT);
    words(palette, text.into());
    let with = offscreen::draw(surface, WIDTH, HEIGHT);
    added(&without, &with, WIDTH, HEIGHT)
}

fn state(palette: &Palette, surface: &Rc<MinimalSoftwareWindow>) -> Option<Ink> {
    find(palette, surface, Palette::set_clearing_state, STATE)
}

fn switch(palette: &Palette, surface: &Rc<MinimalSoftwareWindow>) -> Option<Ink> {
    find(palette, surface, Palette::set_clearing_switch, SWITCH)
}

#[test]
fn the_way_a_value_meets_the_field_is_said_under_the_hints_with_a_button_for_the_other() {
    let surface = offscreen::start(WIDTH, HEIGHT);
    let palette = Palette::new().expect("the palette must build");
    palette.set_pack("whitespace".into());
    palette.set_counter("3 / 12".into());
    palette.set_shortcuts_link("Change shortcuts".into());
    palette.set_use_clipboard_label(WAY_IN.into());
    palette.set_clearing_state(STATE.into());
    palette.set_clearing_switch(SWITCH.into());
    let switches = Rc::new(Cell::new(0));
    let others = Rc::new(Cell::new(0));
    let count = Rc::clone(&switches);
    palette.on_switch_clearing(move || count.set(count.get() + 1));
    let count = Rc::clone(&others);
    palette.on_use_clipboard(move || count.set(count.get() + 1));
    let count = Rc::clone(&others);
    palette.on_open_shortcuts(move || count.set(count.get() + 1));
    palette.show().expect("the palette must show");
    let clicks = || (switches.get(), others.get());

    // ---- values typed: the line and its button, above the clipboard row ------
    let typed = offscreen::draw(&surface, WIDTH, HEIGHT);
    let path = offscreen::save_cropped(&typed, WIDTH, HEIGHT, "palette-clearing.png");
    let line = state(&palette, &surface).expect("no line says how a value meets the field");
    let button = switch(&palette, &surface).expect("no button for the other way");
    let way_in = find(&palette, &surface, Palette::set_use_clipboard_label, WAY_IN)
        .expect("the clipboard button is drawn");
    assert!(
        line.left < WIDTH / 2 && button.left > WIDTH / 2,
        "the line is not on the left with its button on the right: {line:?}, {button:?}. \
         Look at {}",
        path.display()
    );
    assert!(
        button.bottom < way_in.top && button.bottom - button.top < 30,
        "the button is not one row above the clipboard button: {button:?}, {way_in:?}. \
         Look at {}",
        path.display()
    );
    click(palette.window(), button.centre());
    assert_eq!(
        clicks(),
        (1, 0),
        "a click on the button must ask for the other way, once, and nothing else"
    );

    // ---- clipboard mode: nothing is cleared, so nothing is said --------------
    palette.set_clipboard_mode(true);
    palette.set_clipboard_mode_on(true);
    assert!(
        state(&palette, &surface).is_none() && switch(&palette, &surface).is_none(),
        "the clipboard route clears nothing, and the palette still offers a way of clearing"
    );
    palette.set_clipboard_mode(false);
    palette.set_clipboard_mode_on(false);

    // ---- compact: no hint bar, no line ---------------------------------------
    palette.set_compact(true);
    assert!(
        state(&palette, &surface).is_none() && switch(&palette, &surface).is_none(),
        "the compact palette shows the line about clearing"
    );
    palette.set_compact(false);

    // ---- while a value goes: in place, faded, and deaf -------------------------
    palette.set_sending(true);
    let faded = switch(&palette, &surface).expect("the button vanished while a value goes");
    click(palette.window(), faded.centre());
    assert_eq!(
        clicks(),
        (1, 0),
        "the button asked for something while a value was on its way"
    );
}
