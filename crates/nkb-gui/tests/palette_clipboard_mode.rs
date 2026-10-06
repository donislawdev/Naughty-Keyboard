//! Clipboard mode turned on and off from the palette (`UX-GUI-010`, `D99`) -
//! which button stands in which state MEASURED on the rendered palette, and
//! each click followed to its own callback.
//!
//! # What this proves that reading `palette.slint` cannot
//!
//! Two buttons, never both: "Use clipboard mode" under the hint bar while
//! values are typed, "Turn off" on the standing bar while the tester's mode is
//! on. On the bar of a window with higher privileges (`D72`) there is no way
//! out, because that is not a mode a click could end. Each button is found as
//! the ink its words add to the render, and its absence as no ink at all. Turn
//! off stays in the compact palette, like the bar it stands on, and while a
//! value goes neither button answers: the route cannot change under a value in
//! flight.
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

const WAY_IN: &str = "Use clipboard mode";
const WAY_OUT: &str = "Turn off";

/// A button as the pixels its words change, or `None` when it is not drawn.
/// The words are back in place on return, so a click aims at the button as
/// the tester sees it - the lesson of mutation M435 in `palette_copy.rs`.
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

fn way_in(palette: &Palette, surface: &Rc<MinimalSoftwareWindow>) -> Option<Ink> {
    find(palette, surface, Palette::set_use_clipboard_label, WAY_IN)
}

fn way_out(palette: &Palette, surface: &Rc<MinimalSoftwareWindow>) -> Option<Ink> {
    find(palette, surface, Palette::set_turn_off_label, WAY_OUT)
}

#[test]
fn one_button_turns_clipboard_mode_on_and_one_on_its_bar_turns_it_off() {
    let surface = offscreen::start(WIDTH, HEIGHT);
    let palette = Palette::new().expect("the palette must build");
    palette.set_pack("whitespace".into());
    palette.set_counter("3 / 12".into());
    palette.set_shortcuts_link("Change shortcuts".into());
    palette.set_clipboard_mode_label("clipboard mode".into());
    palette.set_use_clipboard_label(WAY_IN.into());
    palette.set_turn_off_label(WAY_OUT.into());
    let on = Rc::new(Cell::new(0));
    let off = Rc::new(Cell::new(0));
    let shortcuts = Rc::new(Cell::new(0));
    let count = Rc::clone(&on);
    palette.on_use_clipboard(move || count.set(count.get() + 1));
    let count = Rc::clone(&off);
    palette.on_turn_off_clipboard(move || count.set(count.get() + 1));
    let count = Rc::clone(&shortcuts);
    palette.on_open_shortcuts(move || count.set(count.get() + 1));
    palette.show().expect("the palette must show");
    let clicks = || (on.get(), off.get(), shortcuts.get());

    // ---- values typed: the way in, under the hint bar --------------------------
    palette.set_clipboard_mode(false);
    palette.set_clipboard_mode_on(false);
    let typed = offscreen::draw(&surface, WIDTH, HEIGHT);
    let path = offscreen::save_cropped(&typed, WIDTH, HEIGHT, "palette-clipboard-off.png");
    let button =
        way_in(&palette, &surface).expect("no way into clipboard mode while values are typed");
    assert!(
        button.left > WIDTH / 2 && button.bottom - button.top < 30,
        "the way in is not one button at the right end of the link's row: {button:?}. \
         Look at {}",
        path.display()
    );
    assert!(
        way_out(&palette, &surface).is_none(),
        "a Turn off stands while the mode is off"
    );
    click(palette.window(), button.centre());
    assert_eq!(
        clicks(),
        (1, 0, 0),
        "a click on the way in must ask for clipboard mode, once, and nothing else"
    );

    // ---- the tester's mode: the way out, on the bar ----------------------------
    palette.set_clipboard_mode(true);
    palette.set_clipboard_mode_on(true);
    let in_mode = offscreen::draw(&surface, WIDTH, HEIGHT);
    let path = offscreen::save_cropped(&in_mode, WIDTH, HEIGHT, "palette-clipboard-on.png");
    let out = way_out(&palette, &surface).expect("no way out of clipboard mode on its bar");
    assert!(
        out.left > WIDTH / 2 && out.bottom < button.top && out.bottom - out.top < 30,
        "Turn off is not one button at the right end of the bar, above the hints: {out:?}. \
         Look at {}",
        path.display()
    );
    assert!(
        way_in(&palette, &surface).is_none(),
        "the way in stands beside the way out"
    );
    click(palette.window(), out.centre());
    assert_eq!(
        clicks(),
        (1, 1, 0),
        "a click on Turn off must ask to leave the mode, once, and nothing else"
    );

    // ---- compact: the bar stays, and its way out with it -----------------------
    palette.set_compact(true);
    let small = way_out(&palette, &surface).expect("Turn off left the compact palette");
    click(palette.window(), small.centre());
    assert_eq!(
        clicks(),
        (1, 2, 0),
        "Turn off in the compact palette did not answer"
    );
    palette.set_compact(false);

    // ---- a window with higher privileges (D72): a bar, no way out ---------------
    palette.set_clipboard_mode_label("clipboard mode for this window".into());
    palette.set_clipboard_mode_on(false);
    assert!(
        way_out(&palette, &surface).is_none(),
        "the bar of a window the tester is looking at offers to turn off a mode"
    );
    assert!(
        way_in(&palette, &surface).is_some(),
        "the way into the mode went with a window's bar"
    );

    // ---- while a value goes: where they were, faded, and deaf -------------------
    palette.set_sending(true);
    let faded_in = way_in(&palette, &surface).expect("the way in vanished while a value goes");
    click(palette.window(), faded_in.centre());
    palette.set_clipboard_mode_label("clipboard mode".into());
    palette.set_clipboard_mode_on(true);
    let faded_out = way_out(&palette, &surface).expect("Turn off vanished while a value goes");
    click(palette.window(), faded_out.centre());
    assert_eq!(
        clicks(),
        (1, 2, 0),
        "a button changed the route while a value was on its way"
    );
}
