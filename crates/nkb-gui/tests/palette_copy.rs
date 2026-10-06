//! The palette's two Copy buttons (`UX-GUI-003`, `D98`) - their pointer states
//! MEASURED on the rendered palette, and each click followed to its own
//! callback.
//!
//! # What this proves that reading `button.slint` cannot
//!
//! GUI rule 10: a property can be ignored by the toolkit and look exactly like
//! one that works. The button has three things no still picture shows: the
//! accent under the pointer, a click reaching the callback of ITS band and not
//! the other one's, and no click at all while a value goes - the value band
//! then shows the value on its way, and its button still holds the key of the
//! last one sent. Each button is found as the ink its word adds to the render,
//! so the pointer goes where the button IS.
//!
//! Its own binary for the reason `palette_appearance.rs` gives: one platform
//! per process.

#![allow(clippy::panic, clippy::expect_used)]

#[allow(dead_code)]
mod offscreen;

use std::cell::Cell;
use std::rc::Rc;

use nkb_gui::Palette;
use slint::platform::software_renderer::MinimalSoftwareWindow;
use slint::platform::{PointerEventButton, WindowEvent};
use slint::{ComponentHandle, LogicalPosition};

const WIDTH: u32 = 420;
const HEIGHT: u32 = 460;

/// How much bluer than red a pixel must be to count as the accent's ink - the
/// threshold `palette_opener.rs` measured for the same two text roles.
const BLUER_THAN_RED: u8 = 40;

#[derive(Debug, Clone, Copy)]
struct Ink {
    left: u32,
    right: u32,
    top: u32,
    bottom: u32,
}

impl Ink {
    fn centre(self) -> LogicalPosition {
        LogicalPosition::new(
            ((self.left + self.right) / 2) as f32,
            ((self.top + self.bottom) / 2) as f32,
        )
    }
}

/// The box of the pixels that differ between two renders of the same palette.
fn added(before: &[offscreen::Pixel], after: &[offscreen::Pixel]) -> Ink {
    let hits: Vec<(u32, u32)> = (0..HEIGHT)
        .flat_map(|y| (0..WIDTH).map(move |x| (x, y)))
        .filter(|&(x, y)| {
            let (a, b) = (
                before[(y * WIDTH + x) as usize],
                after[(y * WIDTH + x) as usize],
            );
            (a.r, a.g, a.b) != (b.r, b.g, b.b)
        })
        .collect();
    let xs = hits.iter().map(|&(x, _)| x);
    let ys = hits.iter().map(|&(_, y)| y);
    Ink {
        left: xs.clone().min().expect("the button's word is drawn"),
        right: xs.max().expect("the button's word is drawn"),
        top: ys.clone().min().expect("the button's word is drawn"),
        bottom: ys.max().expect("the button's word is drawn"),
    }
}

/// Pixels of the accent's hue inside a box - the word's own ink.
fn accent_in(buffer: &[offscreen::Pixel], at: Ink) -> usize {
    (at.top..=at.bottom)
        .flat_map(|y| (at.left..=at.right).map(move |x| (x, y)))
        .filter(|&(x, y)| {
            let p = buffer[(y * WIDTH + x) as usize];
            p.b > p.r.saturating_add(BLUER_THAN_RED)
        })
        .count()
}

fn pointer(palette: &Palette, event: WindowEvent) {
    palette.window().dispatch_event(event);
}

fn press(palette: &Palette, at: LogicalPosition) {
    pointer(palette, WindowEvent::PointerMoved { position: at });
    pointer(
        palette,
        WindowEvent::PointerPressed {
            position: at,
            button: PointerEventButton::Left,
        },
    );
    pointer(
        palette,
        WindowEvent::PointerReleased {
            position: at,
            button: PointerEventButton::Left,
        },
    );
}

fn away(palette: &Palette) {
    pointer(
        palette,
        WindowEvent::PointerMoved {
            position: LogicalPosition::new((WIDTH - 4) as f32, (HEIGHT - 4) as f32),
        },
    );
}

/// The button as the pixels its word changes: the same render without the word
/// and with it, so the pointer goes where the button IS. The box is the whole
/// frame rather than the word alone, because an empty word narrows the button -
/// measured on the first run, 21 pixels tall.
fn find_button(palette: &Palette, surface: &Rc<MinimalSoftwareWindow>) -> Ink {
    palette.set_copy_label("".into());
    let without = offscreen::draw(surface, WIDTH, HEIGHT);
    palette.set_copy_label("Copy".into());
    let with = offscreen::draw(surface, WIDTH, HEIGHT);
    added(&without, &with)
}

#[test]
fn each_copy_button_answers_the_pointer_and_asks_for_its_own_band() {
    let surface = offscreen::start(WIDTH, HEIGHT);
    let palette = Palette::new().expect("the palette must build");
    palette.set_pack("whitespace".into());
    palette.set_counter("3 / 12".into());
    palette.set_last_sent_label("Last sent".into());
    palette.set_next_heading("Next: value 4 of 12".into());
    palette.set_next_name("Leading space".into());
    palette.set_value_name("Trailing space".into());
    let next = Rc::new(Cell::new(0));
    let last = Rc::new(Cell::new(0));
    let packs = Rc::new(Cell::new(0));
    let count = Rc::clone(&next);
    palette.on_copy_next(move || count.set(count.get() + 1));
    let count = Rc::clone(&last);
    palette.on_copy_last(move || count.set(count.get() + 1));
    let count = Rc::clone(&packs);
    palette.on_open_packs(move || count.set(count.get() + 1));
    palette.show().expect("the palette must show");

    // ---- the button beside the next value -------------------------------------
    palette.set_has_value(false);
    palette.set_has_next(true);
    palette.set_next_has_value(true);
    let button = find_button(&palette, &surface);
    assert!(
        button.left > WIDTH / 2 && button.bottom - button.top < 30,
        "the word of the next band's button is not one short word at the right end of \
         its row: {button:?}"
    );
    let rest = offscreen::draw(&surface, WIDTH, HEIGHT);
    let before = accent_in(&rest, button);
    pointer(
        &palette,
        WindowEvent::PointerMoved {
            position: button.centre(),
        },
    );
    let hovered = offscreen::draw(&surface, WIDTH, HEIGHT);
    let path = offscreen::save(&hovered, WIDTH, HEIGHT, "palette-copy-hover.png");
    let under = accent_in(&hovered, button);
    assert!(
        under > before + 10,
        "the pointer over Copy changes nothing: {before} accent pixels at rest, {under} \
         under the pointer, over the word {button:?}. Look at {}",
        path.display()
    );
    away(&palette);
    let left = offscreen::draw(&surface, WIDTH, HEIGHT);
    assert!(
        accent_in(&left, button) <= before,
        "the accent stayed after the pointer left Copy"
    );
    assert_eq!(next.get(), 0, "moving the pointer copied");

    press(&palette, button.centre());
    assert_eq!(
        (next.get(), last.get(), packs.get()),
        (1, 0, 0),
        "a click on the next band's Copy must ask for the next value, once, and nothing else"
    );

    // ---- not at the end of a pack: nothing to copy -----------------------------
    palette.set_next_has_value(false);
    palette.set_next_heading("Next: end of pack (12/12)".into());
    press(&palette, button.centre());
    assert_eq!(
        next.get(),
        1,
        "a Copy button answered at the end of the pack, where there is no value"
    );

    // ---- the button beside the value sent last ----------------------------------
    palette.set_has_next(false);
    palette.set_has_value(true);
    let button = find_button(&palette, &surface);
    assert!(
        button.left > WIDTH / 2 && button.bottom - button.top < 30,
        "the word of the value band's button is not one short word at the right end of \
         its row: {button:?}"
    );
    press(&palette, button.centre());
    assert_eq!(
        (next.get(), last.get(), packs.get()),
        (1, 1, 0),
        "a click on the value band's Copy must ask for the last value, once, and nothing else"
    );

    // ---- while a value goes: faded, and deaf ------------------------------------
    // The band shows the value on its way then, and the button's key still
    // names the last one sent - a click would copy a value no longer on screen.
    palette.set_sending(true);
    let sending = offscreen::draw(&surface, WIDTH, HEIGHT);
    let path = offscreen::save(&sending, WIDTH, HEIGHT, "palette-copy-sending.png");
    // The send band above pushes the value band down - the button is found again.
    palette.set_copy_label("".into());
    let without = offscreen::draw(&surface, WIDTH, HEIGHT);
    let faded = added(&without, &sending);
    // 🔴 The word back BEFORE the press: without it the button is only its
    // padding, narrower than the box measured with the word, and a press at
    // the box's centre lands beside it - the first run of mutation M435 passed
    // for exactly that reason.
    palette.set_copy_label("Copy".into());
    press(&palette, faded.centre());
    pointer(
        &palette,
        WindowEvent::PointerMoved {
            position: faded.centre(),
        },
    );
    let hovered = offscreen::draw(&surface, WIDTH, HEIGHT);
    assert_eq!(
        last.get(),
        1,
        "the value band's Copy answered while a value was on its way. Look at {}",
        path.display()
    );
    assert!(
        accent_in(&hovered, faded) <= accent_in(&sending, faded),
        "a Copy that cannot be used still lit up under the pointer"
    );
}
