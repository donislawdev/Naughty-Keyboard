//! Clipboard mode turned on and off from the palette (`UX-GUI-010`, `D99`) -
//! the route switch and the bar's way out MEASURED on the rendered palette,
//! and each click followed to its own callback.
//!
//! # What this proves that reading `palette.slint` cannot
//!
//! The route switch under the hint bar shows both ways, `Keyboard` and
//! `Clipboard`, and fills the one in effect (the owner's point 2 of
//! 2026-10-07). A click on the other way asks for it by its index, a click on
//! the way in effect asks for nothing. While the tester's mode is on, the
//! standing bar carries "Turn off" as well - the one way back the compact
//! palette shows, where the switch is not drawn. On the bar of a window with
//! higher privileges (`D72`) there is no way out, because that is not a mode a
//! click could end, and the switch shows `Keyboard`: the tester did not choose
//! the clipboard. While a value goes neither answers: the route cannot change
//! under a value in flight. Each piece is found as the ink its words add to
//! the render, and its absence as no ink at all.
//!
//! Its own binary for the reason `palette_appearance.rs` gives: one platform
//! per process.

#![allow(clippy::panic, clippy::expect_used)]

#[allow(dead_code)]
mod offscreen;

use std::cell::RefCell;
use std::rc::Rc;

use nkb_gui::Palette;
use offscreen::{Ink, added, click};
use slint::platform::software_renderer::MinimalSoftwareWindow;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

const WIDTH: u32 = 420;
const HEIGHT: u32 = 460;

const ROUTE: [&str; 2] = ["Keyboard", "Clipboard"];
const WAY_OUT: &str = "Turn off";

fn ways(words: &[&str]) -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from(
        words
            .iter()
            .map(|word| SharedString::from(*word))
            .collect::<Vec<_>>(),
    ))
}

/// Way `at` of the route switch as the pixels its word changes, or `None` when
/// it is not drawn - `offscreen::two_ways`, which puts the words back, so a
/// click aims at the way as the tester sees it (the lesson of mutation M435 in
/// `palette_copy.rs`).
fn way(palette: &Palette, surface: &Rc<MinimalSoftwareWindow>, at: usize) -> Option<Ink> {
    offscreen::two_ways(surface, WIDTH, HEIGHT, ROUTE, &|words| {
        palette.set_route_options(ways(&words));
    })
    .map(|both| both[at])
}

/// The bar's Turn off, the same way.
fn way_out(palette: &Palette, surface: &Rc<MinimalSoftwareWindow>) -> Option<Ink> {
    palette.set_turn_off_label("".into());
    let without = offscreen::draw(surface, WIDTH, HEIGHT);
    palette.set_turn_off_label(WAY_OUT.into());
    let with = offscreen::draw(surface, WIDTH, HEIGHT);
    added(&without, &with, WIDTH, HEIGHT)
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct Clicks {
    /// The indices the route switch asked for, in order.
    route: Vec<i32>,
    /// Clicks on the bar's Turn off.
    off: usize,
    /// Anything else - the link to the shortcuts window.
    other: usize,
}

#[test]
fn the_route_switch_turns_clipboard_mode_on_and_off_and_its_bar_turns_it_off() {
    let surface = offscreen::start(WIDTH, HEIGHT);
    let palette = Palette::new().expect("the palette must build");
    palette.set_pack("whitespace".into());
    palette.set_counter("3 / 12".into());
    palette.set_shortcuts_link("Change shortcuts".into());
    palette.set_clipboard_mode_label("clipboard mode".into());
    palette.set_route_label("Send by".into());
    palette.set_route_options(ways(&ROUTE));
    palette.set_clearing_label("Each value".into());
    palette.set_clearing_options(ways(&["Clear line first", "Insert at cursor"]));
    palette.set_turn_off_label(WAY_OUT.into());
    let clicks = Rc::new(RefCell::new(Clicks::default()));
    let log = Rc::clone(&clicks);
    palette.on_choose_route(move |index| log.borrow_mut().route.push(index));
    let log = Rc::clone(&clicks);
    palette.on_turn_off_clipboard(move || log.borrow_mut().off += 1);
    let log = Rc::clone(&clicks);
    palette.on_open_shortcuts(move || log.borrow_mut().other += 1);
    palette.show().expect("the palette must show");
    let seen = |route: &[i32], off: usize| Clicks {
        route: route.to_vec(),
        off,
        other: 0,
    };

    // ---- values typed: the switch on Keyboard, no Turn off ---------------------
    palette.set_clipboard_mode(false);
    palette.set_clipboard_mode_on(false);
    palette.set_route_selected(0);
    let typed = offscreen::draw(&surface, WIDTH, HEIGHT);
    let path = offscreen::save_cropped(&typed, WIDTH, HEIGHT, "palette-clipboard-off.png");
    let keyboard = way(&palette, &surface, 0).expect("the switch has no Keyboard way");
    let clipboard =
        way(&palette, &surface, 1).expect("no way into clipboard mode while values are typed");
    assert!(
        keyboard.right < clipboard.left && clipboard.bottom - clipboard.top < 30,
        "the two ways are not side by side on one row: {keyboard:?}, {clipboard:?}. Look at {}",
        path.display()
    );
    assert!(
        way_out(&palette, &surface).is_none(),
        "a Turn off stands while the mode is off"
    );
    click(palette.window(), clipboard.centre());
    assert_eq!(
        *clicks.borrow(),
        seen(&[1], 0),
        "a click on Clipboard must ask for clipboard mode, once, and nothing else"
    );
    click(palette.window(), keyboard.centre());
    assert_eq!(
        *clicks.borrow(),
        seen(&[1], 0),
        "a click on the way in effect asked for something"
    );

    // ---- the tester's mode: Clipboard filled, Turn off on the bar ---------------
    palette.set_clipboard_mode(true);
    palette.set_clipboard_mode_on(true);
    palette.set_route_selected(1);
    let in_mode = offscreen::draw(&surface, WIDTH, HEIGHT);
    let path = offscreen::save_cropped(&in_mode, WIDTH, HEIGHT, "palette-clipboard-on.png");
    let out = way_out(&palette, &surface).expect("no way out of clipboard mode on its bar");
    assert!(
        out.left > WIDTH / 2 && out.bottom < clipboard.top && out.bottom - out.top < 30,
        "Turn off is not one button at the right end of the bar, above the hints: {out:?}. \
         Look at {}",
        path.display()
    );
    click(palette.window(), out.centre());
    assert_eq!(
        *clicks.borrow(),
        seen(&[1], 1),
        "a click on Turn off must ask to leave the mode, once, and nothing else"
    );
    let keyboard = way(&palette, &surface, 0).expect("the switch left in clipboard mode");
    click(palette.window(), keyboard.centre());
    assert_eq!(
        *clicks.borrow(),
        seen(&[1, 0], 1),
        "a click on Keyboard in clipboard mode did not ask to leave the mode"
    );

    // ---- compact: the bar stays, and its way out with it -----------------------
    palette.set_compact(true);
    assert!(
        way(&palette, &surface, 1).is_none(),
        "the compact palette shows the route switch"
    );
    let small = way_out(&palette, &surface).expect("Turn off left the compact palette");
    click(palette.window(), small.centre());
    assert_eq!(
        *clicks.borrow(),
        seen(&[1, 0], 2),
        "Turn off in the compact palette did not answer"
    );
    palette.set_compact(false);

    // ---- a window with higher privileges (D72): a bar, no way out ---------------
    palette.set_clipboard_mode_label("clipboard mode for this window".into());
    palette.set_clipboard_mode_on(false);
    palette.set_route_selected(0);
    assert!(
        way_out(&palette, &surface).is_none(),
        "the bar of a window the tester is looking at offers to turn off a mode"
    );
    assert!(
        way(&palette, &surface, 1).is_some(),
        "the route switch went with a window's bar"
    );

    // ---- while a value goes: where they were, faded, and deaf -------------------
    palette.set_sending(true);
    let faded = way(&palette, &surface, 1).expect("the switch vanished while a value goes");
    click(palette.window(), faded.centre());
    palette.set_clipboard_mode_label("clipboard mode".into());
    palette.set_clipboard_mode_on(true);
    let faded_out = way_out(&palette, &surface).expect("Turn off vanished while a value goes");
    click(palette.window(), faded_out.centre());
    assert_eq!(
        *clicks.borrow(),
        seen(&[1, 0], 2),
        "the switch or the bar changed the route while a value was on its way"
    );
}
