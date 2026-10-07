//! How a typed value meets the field (`UX-GUI-005`, `D101`) - the switch that
//! shows both ways and fills the one in effect, MEASURED on the rendered
//! palette, and each click followed to its callback.
//!
//! # What this proves that reading `palette.slint` cannot
//!
//! The owner's point 2 of 2026-10-07: a sentence naming the way in effect
//! beside a button naming the other did not say which was which. So the switch
//! stands under the hint bar from the first frame, below the route switch, with
//! BOTH ways drawn and the one in effect filled in the accent. A click on the
//! other way asks for that way by its index, a click on the way in effect asks
//! for nothing. In clipboard mode it stays where it is, faded and deaf -
//! nothing is cleared there, and a row that vanished would move the link below
//! it. The compact palette shows no switch at all, and while a value goes it
//! is faded and deaf as well: the worker answers between presses. Each way is
//! found as the ink its word adds to the render, its absence as no ink at all.
//!
//! Its own binary for the reason `palette_appearance.rs` gives: one platform
//! per process.

#![allow(clippy::panic, clippy::expect_used)]

#[allow(dead_code)]
mod offscreen;

use std::cell::RefCell;
use std::rc::Rc;

use nkb_gui::Palette;
use offscreen::{Ink, click};
use slint::platform::software_renderer::MinimalSoftwareWindow;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

const WIDTH: u32 = 420;
const HEIGHT: u32 = 460;

const ROUTE: [&str; 2] = ["Keyboard", "Clipboard"];
const CLEARING: [&str; 2] = ["Clear line first", "Insert at cursor"];

/// `accent` from the dictionary - the ground of the way in effect. The test's
/// own copy, for the reason `palette_appearance.rs` gives for its colours.
const ACCENT: (u8, u8, u8) = (0x7A, 0xA2, 0xF7);

fn ways(words: &[&str]) -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from(
        words
            .iter()
            .map(|word| SharedString::from(*word))
            .collect::<Vec<_>>(),
    ))
}

/// Way `at` of the clearing switch as the pixels its word changes, or `None`
/// when it is not drawn - `offscreen::two_ways`, which puts the words back,
/// so a click aims at the way as the tester sees it (the lesson of mutation
/// M435 in `palette_copy.rs`).
fn way(palette: &Palette, surface: &Rc<MinimalSoftwareWindow>, at: usize) -> Option<Ink> {
    offscreen::two_ways(surface, WIDTH, HEIGHT, CLEARING, &|words| {
        palette.set_clearing_options(ways(&words));
    })
    .map(|both| both[at])
}

/// Accent pixels inside `at` - the fill of the way in effect.
fn accent_in(buffer: &[offscreen::Pixel], at: &Ink) -> usize {
    (at.top..=at.bottom)
        .flat_map(|y| (at.left..=at.right).map(move |x| (x, y)))
        .filter(|&(x, y)| {
            let p = buffer[(y * WIDTH + x) as usize];
            (p.r, p.g, p.b) == ACCENT
        })
        .count()
}

#[test]
fn both_ways_of_meeting_the_field_stand_on_one_switch_and_the_one_in_effect_is_filled() {
    let surface = offscreen::start(WIDTH, HEIGHT);
    let palette = Palette::new().expect("the palette must build");
    palette.set_pack("whitespace".into());
    palette.set_counter("3 / 12".into());
    palette.set_shortcuts_link("Change shortcuts".into());
    palette.set_route_label("Send by".into());
    palette.set_route_options(ways(&ROUTE));
    palette.set_clearing_label("Each value".into());
    palette.set_clearing_options(ways(&CLEARING));
    palette.set_clearing_selected(0);
    let asked = Rc::new(RefCell::new(Vec::new()));
    let others = Rc::new(RefCell::new(0));
    let log = Rc::clone(&asked);
    palette.on_choose_clearing(move |index| log.borrow_mut().push(index));
    let count = Rc::clone(&others);
    palette.on_choose_route(move |_| *count.borrow_mut() += 1);
    let count = Rc::clone(&others);
    palette.on_open_shortcuts(move || *count.borrow_mut() += 1);
    palette.show().expect("the palette must show");
    let clicks = || (asked.borrow().clone(), *others.borrow());

    // ---- values typed: both ways, side by side, the line filled --------------
    let typed = offscreen::draw(&surface, WIDTH, HEIGHT);
    let path = offscreen::save_cropped(&typed, WIDTH, HEIGHT, "palette-clearing.png");
    let line = way(&palette, &surface, 0).expect("the way of clearing the line is not drawn");
    let cursor = way(&palette, &surface, 1).expect("the way at the cursor is not drawn");
    assert!(
        line.right < cursor.left && line.bottom - line.top < 30 && cursor.top < line.bottom,
        "the two ways are not side by side on one row: {line:?}, {cursor:?}. Look at {}",
        path.display()
    );
    assert!(
        accent_in(&typed, &line) > accent_in(&typed, &cursor) + 50,
        "the way in effect is not the filled one: {} accent pixels on the line, {} at the \
         cursor. Look at {}",
        accent_in(&typed, &line),
        accent_in(&typed, &cursor),
        path.display()
    );
    // The fill follows the state, not the order: the other way filled now.
    palette.set_clearing_selected(1);
    let other = offscreen::draw(&surface, WIDTH, HEIGHT);
    assert!(
        accent_in(&other, &cursor) > accent_in(&other, &line) + 50,
        "the fill did not move with the way in effect. Look at {}",
        path.display()
    );
    palette.set_clearing_selected(0);
    click(palette.window(), cursor.centre());
    assert_eq!(
        clicks(),
        (vec![1], 0),
        "a click on the other way must ask for that way, once, and nothing else"
    );
    click(palette.window(), line.centre());
    assert_eq!(
        clicks(),
        (vec![1], 0),
        "a click on the way in effect asked for something"
    );

    // ---- clipboard mode: in place, faded, and deaf ---------------------------
    palette.set_clipboard_mode(true);
    palette.set_clipboard_mode_on(true);
    // Across, the same place. Down, the standing bar the mode adds at the top
    // moves every band below it, the switch included - that is the bar's
    // place (D71), not the switch leaving its own.
    let faded = way(&palette, &surface, 1).expect("the switch left its place in clipboard mode");
    assert_eq!(
        (faded.left, faded.right),
        (cursor.left, cursor.right),
        "the switch moved across, or changed its width, in clipboard mode"
    );
    click(palette.window(), faded.centre());
    assert_eq!(
        clicks(),
        (vec![1], 0),
        "the clearing switch answered in clipboard mode, where nothing is cleared"
    );
    palette.set_clipboard_mode(false);
    palette.set_clipboard_mode_on(false);

    // ---- compact: no hint bar, no switch -------------------------------------
    palette.set_compact(true);
    assert!(
        way(&palette, &surface, 0).is_none() && way(&palette, &surface, 1).is_none(),
        "the compact palette shows the clearing switch"
    );
    palette.set_compact(false);

    // ---- while a value goes: in place, faded, and deaf -------------------------
    palette.set_sending(true);
    let faded = way(&palette, &surface, 1).expect("the switch vanished while a value goes");
    click(palette.window(), faded.centre());
    assert_eq!(
        clicks(),
        (vec![1], 0),
        "the switch asked for something while a value was on its way"
    );
}
