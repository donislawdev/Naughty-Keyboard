//! The pack's name in the palette as a thing to click - its pointer states
//! MEASURED on the rendered palette, and its click followed to the callback.
//! The same for the link under the hint bar, which asks for the shortcuts
//! window (K5.5).
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

use nkb_gui::{HintRow, Palette};
use slint::platform::{PointerEventButton, WindowEvent};
use slint::{ComponentHandle, LogicalPosition, ModelRc, VecModel};

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

/// The box the name's ink occupies: the first and last column and row
/// BRIGHTER than the ground, in the left half of the first band.
///
/// Brighter, not merely different: outside the palette's rounded corner the
/// buffer is black, and a box that took the corner in started at (0, 0) and
/// put its "centre" beside the name. Windows happened to land on the name
/// anyway, macOS 27.0 did not - the first run there hovered nothing.
#[derive(Debug)]
struct Ink {
    left: u32,
    right: u32,
    top: u32,
    bottom: u32,
}

fn brightness(p: offscreen::Pixel) -> u32 {
    u32::from(p.r) + u32::from(p.g) + u32::from(p.b)
}

fn ink(buffer: &[offscreen::Pixel]) -> Ink {
    let ground = brightness(buffer[(4 * WIDTH + WIDTH / 2) as usize]);
    let hits: Vec<(u32, u32)> = (0..40)
        .flat_map(|y| (0..WIDTH / 2).map(move |x| (x, y)))
        .filter(|&(x, y)| brightness(buffer[(y * WIDTH + x) as usize]) > ground + 60)
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
fn the_palette_links_answer_the_pointer_and_open_their_windows() {
    let surface = offscreen::start(WIDTH, HEIGHT);
    let palette = Palette::new().expect("the palette must build");
    palette.set_pack("unicode-text".into());
    palette.set_counter("7 / 34".into());
    palette.set_no_value("Nothing sent yet.".into());
    palette.set_open_packs_label("Find a value".into());
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
         {under} under the pointer, pointer at {centre:?} over the ink {name:?}. Look at {}",
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

    // ---- a click beside it does not --------------------------------------------
    // In the middle of the band, between the name and the counter: since UX4
    // the right end holds the collapse button, which asks for something else.
    press(&palette, LogicalPosition::new((WIDTH / 2) as f32, centre.y));
    assert_eq!(
        opened.get(),
        1,
        "a click beside the name opened the pack window"
    );

    // ---- the heading of the next band opens nothing (the owner's point 3) ---
    // Until 2026-10-07 it opened the value window too, and two ways in one
    // window for one window was the complaint. Found as the ink its words add,
    // like the link below: the band shows only its heading at the end of a
    // pack, so the words are the only difference. The pointer over it adds no
    // accent either - a heading that lit up would still invite the click.
    palette.set_next_has_value(false);
    palette.set_has_next(true);
    palette.set_next_heading("".into());
    let no_words = offscreen::draw(&surface, WIDTH, HEIGHT);
    palette.set_next_heading("Next: end of pack (34/34)".into());
    let words = offscreen::draw(&surface, WIDTH, HEIGHT);
    let heading = added(&no_words, &words);
    assert!(
        heading.bottom - heading.top < 30,
        "the heading's words changed more than one line of the palette: {heading:?}"
    );
    let on_heading = LogicalPosition::new(
        ((heading.left + heading.right) / 2) as f32,
        ((heading.top + heading.bottom) / 2) as f32,
    );
    pointer(
        &palette,
        WindowEvent::PointerMoved {
            position: on_heading,
        },
    );
    assert_eq!(
        accent_in(&offscreen::draw(&surface, WIDTH, HEIGHT), &heading),
        accent_in(&words, &heading),
        "the next band's heading lights up under the pointer, so it still invites a click"
    );
    press(&palette, on_heading);
    assert_eq!(
        opened.get(),
        1,
        "a click on the next band's heading opened the value window - the pack's name is \
         the one way in by the pointer"
    );
    palette.set_has_next(false);

    // ---- the heading of the last value folds it (the owner's point 7) -------
    // Found as the ink its words add, like the link below. Its own callback,
    // once per click, and the value window stays shut: the fold shows more
    // of this window, it does not open another. The pointer over it lights
    // the heading in the accent, as it does a link, so the click is invited.
    let folds = Rc::new(Cell::new(0));
    let count = Rc::clone(&folds);
    palette.on_toggle_last_sent(move || count.set(count.get() + 1));
    palette.set_has_value(true);
    palette.set_value_name("Three zero-width spaces".into());
    palette.set_last_sent_label("".into());
    let no_words = offscreen::draw(&surface, WIDTH, HEIGHT);
    palette.set_last_sent_label("Last sent".into());
    let words = offscreen::draw(&surface, WIDTH, HEIGHT);
    let fold = added(&no_words, &words);
    assert!(
        fold.bottom - fold.top < 30,
        "the heading's words changed more than one line of the palette: {fold:?}"
    );
    let on_fold = LogicalPosition::new(
        ((fold.left + fold.right) / 2) as f32,
        ((fold.top + fold.bottom) / 2) as f32,
    );
    pointer(&palette, WindowEvent::PointerMoved { position: on_fold });
    assert!(
        accent_in(&offscreen::draw(&surface, WIDTH, HEIGHT), &fold) > accent_in(&words, &fold) + 10,
        "the heading of the last value does not answer the pointer, so nothing says it folds"
    );
    press(&palette, on_fold);
    assert_eq!(
        (folds.get(), opened.get()),
        (1, 1),
        "a click on the heading of the last value did not ask to fold it once, or opened the \
         value window"
    );
    palette.set_has_value(false);

    // ---- the link under the hint bar: the same states, its own callback ------
    // In this test rather than a second one, because the platform may be
    // installed once per process. The link is found as the ink its words add
    // to the palette - the same render with the words and without them - so the
    // pointer goes where the link IS.
    let asked = Rc::new(Cell::new(0));
    let count = Rc::clone(&asked);
    palette.on_open_shortcuts(move || count.set(count.get() + 1));
    palette.set_hints(ModelRc::new(VecModel::from(vec![
        HintRow {
            key: "Alt+Shift+N".into(),
            action: "Next value".into(),
        },
        HintRow {
            key: "Alt+Shift+Space".into(),
            action: "Find a value".into(),
        },
    ])));
    let without = offscreen::draw(&surface, WIDTH, HEIGHT);
    palette.set_shortcuts_link("Change shortcuts".into());
    let with = offscreen::draw(&surface, WIDTH, HEIGHT);
    let path = offscreen::save(&with, WIDTH, HEIGHT, "palette-link.png");
    let link = added(&without, &with);
    assert!(
        link.bottom - link.top < 30 && link.right - link.left < WIDTH / 2,
        "the link's words changed more than one line of the palette: {link:?}. Look at {}",
        path.display()
    );
    let centre = LogicalPosition::new(
        ((link.left + link.right) / 2) as f32,
        ((link.top + link.bottom) / 2) as f32,
    );
    let before = accent_in(&with, &link);
    pointer(&palette, WindowEvent::PointerMoved { position: centre });
    let hovered = offscreen::draw(&surface, WIDTH, HEIGHT);
    let hover_path = offscreen::save(&hovered, WIDTH, HEIGHT, "palette-link-hover.png");
    let under = accent_in(&hovered, &link);
    assert!(
        under > before + 20,
        "the pointer over the link changes nothing: {before} accent pixels at rest, {under} \
         under the pointer, pointer at {centre:?} over the ink {link:?}. Look at {}",
        hover_path.display()
    );
    press(&palette, centre);
    // One opening of the value window so far - the pack's name - and the link
    // must not add a second.
    assert_eq!(
        (asked.get(), opened.get()),
        (1, 1),
        "a click on the link did not ask for the shortcuts window once, or opened the pack \
         window. Look at {}",
        path.display()
    );
    // A click on the hint bar above it asks for nothing.
    press(
        &palette,
        LogicalPosition::new(centre.x, (link.top - 12) as f32),
    );
    assert_eq!(
        (asked.get(), opened.get()),
        (1, 1),
        "a click on the hint bar above the link asked for a window"
    );
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
        left: xs.clone().min().expect("the link's words are drawn"),
        right: xs.max().expect("the link's words are drawn"),
        top: ys.clone().min().expect("the link's words are drawn"),
        bottom: ys.max().expect("the link's words are drawn"),
    }
}
