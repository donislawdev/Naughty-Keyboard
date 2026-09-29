//! The pack's name in the palette as a thing to click - its pointer states
//! MEASURED on the rendered palette, and its click followed to the callback.
//!
//! # What this proves that reading `list.slint` cannot
//!
//! GUI rule 10: a property can be ignored by the toolkit and look exactly like
//! one that works. The name has two pointer states and one job, and none of
//! them shows in a still picture: the accent under the pointer, the accent
//! going away when the pointer leaves, and a click reaching `open-packs` - the
//! callback the shortcut uses too. The name's own ink is found in the render,
//! so the pointer goes where the name IS, not where a coordinate says it was.
//!
//! Its own binary for the reason `palette_appearance.rs` gives: one platform
//! per process.

#![allow(clippy::panic, clippy::expect_used)]

#[allow(dead_code)]
mod offscreen;

use std::cell::Cell;
use std::rc::Rc;

use nkb_gui::Palette;
use slint::platform::{PointerEventButton, WindowEvent};
use slint::{ComponentHandle, LogicalPosition};

const WIDTH: u32 = 420;
const HEIGHT: u32 = 460;

/// How much bluer than red a pixel must be to count as the accent's ink.
///
/// Measured by hue rather than by the exact colour, because the name is a thin
/// face at body size and only 7 pixels of it came out exactly `accent`
/// (`0x7A 0xA2 0xF7`, blue 125 over red) - the rest are its strokes blended
/// with the ground. `text-secondary` (`0x9A 0xA4 0xB4`) is 26 bluer than red at
/// full ink and less when blended, so 40 separates the two roles at any
/// coverage above a third.
const BLUER_THAN_RED: u8 = 40;

/// The box the name's ink occupies: the first and last column and row that
/// differ from the ground, in the left half of the first band.
struct Ink {
    left: u32,
    right: u32,
    top: u32,
    bottom: u32,
}

fn ink(buffer: &[offscreen::Pixel]) -> Ink {
    let ground = buffer[(4 * WIDTH + WIDTH / 2) as usize];
    let hits: Vec<(u32, u32)> = (0..40)
        .flat_map(|y| (0..WIDTH / 2).map(move |x| (x, y)))
        .filter(|&(x, y)| buffer[(y * WIDTH + x) as usize] != ground)
        .collect();
    let xs = hits.iter().map(|&(x, _)| x);
    let ys = hits.iter().map(|&(_, y)| y);
    Ink {
        left: xs.clone().min().expect("the pack's name is drawn"),
        right: xs.max().expect("the pack's name is drawn"),
        top: ys.clone().min().expect("the pack's name is drawn"),
        bottom: ys.max().expect("the pack's name is drawn"),
    }
}

/// Pixels of the accent's hue inside the name's box - the letters' own ink.
fn accent_in(buffer: &[offscreen::Pixel], at: &Ink) -> usize {
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

#[test]
fn the_pack_name_answers_the_pointer_and_opens_the_pack_window() {
    let surface = offscreen::start(WIDTH, HEIGHT);
    let palette = Palette::new().expect("the palette must build");
    palette.set_pack("unicode-text".into());
    palette.set_counter("7 / 34".into());
    palette.set_no_value("Nothing sent yet.".into());
    palette.set_open_packs_label("Open pack search".into());
    let opened = Rc::new(Cell::new(0));
    let count = Rc::clone(&opened);
    palette.on_open_packs(move || count.set(count.get() + 1));
    palette.show().expect("the palette must show");

    let rest = offscreen::draw(&surface, WIDTH, HEIGHT);
    let name = ink(&rest);
    let centre = LogicalPosition::new(
        ((name.left + name.right) / 2) as f32,
        ((name.top + name.bottom) / 2) as f32,
    );
    let before = accent_in(&rest, &name);

    // ---- the pointer over the name turns it to the accent --------------------
    pointer(&palette, WindowEvent::PointerMoved { position: centre });
    let hovered = offscreen::draw(&surface, WIDTH, HEIGHT);
    let path = offscreen::save(&hovered, WIDTH, HEIGHT, "palette-opener-hover.png");
    let under = accent_in(&hovered, &name);
    assert!(
        under > before + 20,
        "the pointer over the pack's name changes nothing: {before} accent pixels at rest, \
         {under} under the pointer. Look at {}",
        path.display()
    );

    // ---- and it goes away with the pointer ----------------------------------
    pointer(
        &palette,
        WindowEvent::PointerMoved {
            position: LogicalPosition::new((WIDTH - 10) as f32, (HEIGHT - 10) as f32),
        },
    );
    let left = offscreen::draw(&surface, WIDTH, HEIGHT);
    assert!(
        accent_in(&left, &name) <= before,
        "the accent stayed after the pointer left the name"
    );
    assert_eq!(opened.get(), 0, "moving the pointer opened the pack window");

    // ---- a click on the name opens the pack window, once --------------------
    let press = |palette: &Palette, at: LogicalPosition| {
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
    };
    press(&palette, centre);
    assert_eq!(
        opened.get(),
        1,
        "a click on the pack's name did not open the pack window"
    );

    // ---- a click on the counter beside it does not ---------------------------
    press(
        &palette,
        LogicalPosition::new((WIDTH - 30) as f32, centre.y),
    );
    assert_eq!(
        opened.get(),
        1,
        "a click beside the name opened the pack window"
    );
}
