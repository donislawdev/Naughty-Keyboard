//! The way back from the compact palette (`UX-GUI-004`) and the mark that says
//! a name opens the value window (`UX-GUI-015`) - both MEASURED on the
//! rendered palette, and the button's click followed to its callback.
//!
//! # What this proves that reading `palette.slint` cannot
//!
//! The button at the end of the pack band says what a click does now:
//! "Collapse" while the palette is expanded, "Expand" while it is compact - and
//! the second is the one way back the compact state shows, so it must be drawn
//! THERE, not only exist in the source. Each word is found as the ink it adds to
//! the render, its absence as no ink at all. While a value goes the button
//! stays where it was, faded, and a click asks for nothing: the worker answers
//! a click between presses, so it would land after the send.
//!
//! The mark after the pack's name and after the next band's heading is a
//! shape, not a word, so it is found the other way round: with the words
//! emptied, whatever ink is left at the start of the band is the mark.
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

const COLLAPSE: &str = "Collapse";
const EXPAND: &str = "Expand";

/// The first band's height, generously: the pack band is one row of text and
/// a button, well inside this.
const PACK_BAND: u32 = 48;

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

fn collapse(palette: &Palette, surface: &Rc<MinimalSoftwareWindow>) -> Option<Ink> {
    find(palette, surface, Palette::set_collapse_label, COLLAPSE)
}

fn expand(palette: &Palette, surface: &Rc<MinimalSoftwareWindow>) -> Option<Ink> {
    find(palette, surface, Palette::set_expand_label, EXPAND)
}

/// Pixels clearly brighter than the palette's ground inside `columns` of the
/// rows `rows` - letters or a mark, whatever their colour.
fn ink_in(
    buffer: &[offscreen::Pixel],
    columns: std::ops::Range<u32>,
    rows: std::ops::Range<u32>,
) -> usize {
    let brightness = |p: offscreen::Pixel| u32::from(p.r) + u32::from(p.g) + u32::from(p.b);
    let ground = brightness(buffer[(4 * WIDTH + WIDTH / 2) as usize]);
    rows.flat_map(|y| columns.clone().map(move |x| (x, y)))
        .filter(|&(x, y)| brightness(buffer[(y * WIDTH + x) as usize]) > ground + 60)
        .count()
}

#[test]
fn the_pack_band_has_a_way_back_from_compact_and_its_name_says_it_opens_a_list() {
    let surface = offscreen::start(WIDTH, HEIGHT);
    let palette = Palette::new().expect("the palette must build");
    palette.set_pack("whitespace".into());
    // No counter, on purpose: the button's words change its width, the counter
    // beside it moves with them, and the box of what changed would then take
    // the counter in - the first run aimed its click between the two. The
    // button stands at the band's end whatever the counter says.
    palette.set_counter("".into());
    palette.set_no_value("Nothing sent yet.".into());
    palette.set_collapse_label(COLLAPSE.into());
    palette.set_expand_label(EXPAND.into());
    let toggles = Rc::new(Cell::new(0));
    let opens = Rc::new(Cell::new(0));
    let count = Rc::clone(&toggles);
    palette.on_toggle_compact(move || count.set(count.get() + 1));
    let count = Rc::clone(&opens);
    palette.on_open_packs(move || count.set(count.get() + 1));
    palette.show().expect("the palette must show");
    let clicks = || (toggles.get(), opens.get());

    // ---- expanded: Collapse, at the right end of the pack band ----------------
    palette.set_compact(false);
    let expanded = offscreen::draw(&surface, WIDTH, HEIGHT);
    let path = offscreen::save_cropped(&expanded, WIDTH, HEIGHT, "palette-collapse.png");
    let button = collapse(&palette, &surface).expect("no Collapse in the expanded palette");
    assert!(
        button.left > WIDTH / 2 && button.bottom < PACK_BAND,
        "Collapse is not at the right end of the pack band: {button:?}. Look at {}",
        path.display()
    );
    assert!(
        expand(&palette, &surface).is_none(),
        "Expand stands in the expanded palette"
    );
    click(palette.window(), button.centre());
    assert_eq!(
        clicks(),
        (1, 0),
        "a click on Collapse must ask to collapse, once, and nothing else"
    );

    // ---- compact: Expand, in the same place ----------------------------------
    palette.set_compact(true);
    let compact = offscreen::draw(&surface, WIDTH, HEIGHT);
    let path = offscreen::save_cropped(&compact, WIDTH, HEIGHT, "palette-expand.png");
    let way_back = expand(&palette, &surface).expect("the compact palette shows no way back");
    assert!(
        way_back.left > WIDTH / 2 && way_back.bottom < PACK_BAND,
        "Expand is not at the right end of the pack band: {way_back:?}. Look at {}",
        path.display()
    );
    assert!(
        collapse(&palette, &surface).is_none(),
        "Collapse stands in the compact palette"
    );
    click(palette.window(), way_back.centre());
    assert_eq!(clicks(), (2, 0), "a click on Expand did not ask to expand");

    // ---- while a value goes: in place, faded, and deaf -------------------------
    palette.set_sending(true);
    let faded = expand(&palette, &surface).expect("the button vanished while a value goes");
    click(palette.window(), faded.centre());
    assert_eq!(
        clicks(),
        (2, 0),
        "the button asked for something while a value was on its way"
    );
    palette.set_sending(false);
    palette.set_compact(false);

    // ---- the mark after the pack's name ---------------------------------------
    // The line the name stands on is where its words change the render. With
    // the words gone, the only ink left at the start of that line is the mark.
    let name_line = find(&palette, &surface, Palette::set_pack, "whitespace")
        .expect("the pack's name is drawn");
    palette.set_pack("".into());
    let no_name = offscreen::draw(&surface, WIDTH, HEIGHT);
    let path = offscreen::save_cropped(&no_name, WIDTH, HEIGHT, "palette-mark.png");
    let mark = ink_in(&no_name, 0..WIDTH / 4, name_line.top..name_line.bottom + 1);
    assert!(
        mark > 4,
        "no mark after the pack's name says it opens the value window ({mark} pixels on \
         the rows of {name_line:?}). Look at {}",
        path.display()
    );
    palette.set_pack("whitespace".into());

    // ---- and after the next band's heading ------------------------------------
    // The same way: at the end of a pack the band is its heading alone.
    palette.set_has_next(true);
    palette.set_next_has_value(false);
    let heading_line = find(
        &palette,
        &surface,
        Palette::set_next_heading,
        "Next: end of pack (12/12)",
    )
    .expect("the next band's heading is drawn");
    palette.set_next_heading("".into());
    let no_heading = offscreen::draw(&surface, WIDTH, HEIGHT);
    let heading_mark = ink_in(
        &no_heading,
        0..WIDTH / 4,
        heading_line.top..heading_line.bottom + 1,
    );
    assert!(
        heading_mark > 4,
        "no mark after the next band's heading ({heading_mark} pixels on the rows of \
         {heading_line:?}) - it opens the value window too"
    );
}
