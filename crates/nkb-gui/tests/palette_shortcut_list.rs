//! The list of shortcuts at the palette's foot folds under its heading
//! (`D123`) - MEASURED on the rendered palette, and the heading's click
//! followed to its callback.
//!
//! # What this proves that reading `palette.slint` cannot
//!
//! Folded, the rows of the list are not drawn and the palette is at least that
//! much shorter, while the heading is still drawn, on the row it had - so the
//! tester opens the list again where they folded it. The heading is found as
//! the ink its words add, the rows as the ink their words add, and a click on
//! the heading as drawn must reach the callback the window answers.
//!
//! Its own binary for the reason `palette_appearance.rs` gives: one platform
//! per process.

#![allow(clippy::panic, clippy::expect_used)]

#[allow(dead_code)]
mod offscreen;

use std::cell::Cell;
use std::rc::Rc;

use nkb_gui::{HintRow, Palette};
use offscreen::{Ink, added, click};
use slint::platform::software_renderer::MinimalSoftwareWindow;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

const WIDTH: u32 = 420;
const HEIGHT: u32 = 900;

const HEADING: &str = "Shortcuts";

fn rows(rows: &[(&str, &str)]) -> ModelRc<HintRow> {
    ModelRc::new(VecModel::from(
        rows.iter()
            .map(|(key, action)| HintRow {
                key: (*key).into(),
                action: (*action).into(),
            })
            .collect::<Vec<_>>(),
    ))
}

/// The palette's list of shortcuts, the longest key included.
fn the_list() -> ModelRc<HintRow> {
    rows(&[
        ("Alt+Shift+N", "Type next value"),
        ("Alt+Shift+P", "Type previous value"),
        ("Alt+Shift+Space", "Find a value"),
        ("Alt+Shift+H", "Collapse or expand"),
    ])
}

/// What `words` adds to the render, or `None` when it adds nothing. The words
/// are back in place on return, so a click aims at what the tester sees.
fn find<T: Clone>(
    palette: &Palette,
    surface: &Rc<MinimalSoftwareWindow>,
    words: fn(&Palette, T),
    empty: T,
    full: T,
) -> Option<Ink> {
    words(palette, empty);
    let without = offscreen::draw(surface, WIDTH, HEIGHT);
    words(palette, full);
    let with = offscreen::draw(surface, WIDTH, HEIGHT);
    added(&without, &with, WIDTH, HEIGHT)
}

/// The heading's letters: what they add over other letters of the same count.
/// Not over no letters - an empty heading is a shorter row, the mark beside it
/// moves up with the row, and the mark is a different shape open and folded,
/// so the box would move between the two states though nothing did (the first
/// run of this test: four pixels across, one down).
fn heading(palette: &Palette, surface: &Rc<MinimalSoftwareWindow>) -> Option<Ink> {
    find(
        palette,
        surface,
        Palette::set_hints_label,
        SharedString::from("x".repeat(HEADING.len())),
        SharedString::from(HEADING),
    )
}

fn list(palette: &Palette, surface: &Rc<MinimalSoftwareWindow>) -> Option<Ink> {
    find(palette, surface, Palette::set_hints, rows(&[]), the_list())
}

/// The last row with letters on it from `from` down to the end of the band
/// it stands in - the rows of the list, without the room the window grew by
/// to hold them and without the status line under the footer. The band ends
/// where the ground at the right edge, where no text stands, changes colour.
fn last_letters(buffer: &[offscreen::Pixel], from: u32) -> u32 {
    let brightness = |p: offscreen::Pixel| u32::from(p.r) + u32::from(p.g) + u32::from(p.b);
    let gutter = |y: u32| buffer[(y * WIDTH + WIDTH - 6) as usize];
    let ground = gutter(from);
    (from..HEIGHT)
        .take_while(|&y| gutter(y) == ground)
        .filter(|&y| {
            (0..WIDTH / 2)
                .any(|x| brightness(buffer[(y * WIDTH + x) as usize]) > brightness(ground) + 60)
        })
        .last()
        .unwrap_or(from)
}

/// The last row the palette draws anything on - the buffer keeps its initial
/// value below the window.
fn bottom_edge(buffer: &[offscreen::Pixel]) -> u32 {
    let empty = offscreen::Pixel::default();
    buffer
        .chunks(WIDTH as usize)
        .rposition(|row| row.iter().any(|pixel| *pixel != empty))
        .map_or(0, |row| row as u32)
}

#[test]
fn the_heading_folds_the_list_of_shortcuts_and_stays_where_it_was() {
    let surface = offscreen::start(WIDTH, HEIGHT);
    let palette = Palette::new().expect("the palette must build");
    palette.set_pack("whitespace".into());
    palette.set_counter("".into());
    palette.set_no_value("Nothing sent yet.".into());
    palette.set_hints(the_list());
    palette.set_hints_label(HEADING.into());
    palette.set_hints_action("Show or hide the list of shortcuts".into());
    let toggles = Rc::new(Cell::new(0));
    let count = Rc::clone(&toggles);
    palette.on_toggle_hints(move || count.set(count.get() + 1));
    palette.show().expect("the palette must show");

    // ---- open: the heading, and the rows under it -----------------------------
    palette.set_hints_open(true);
    let open = offscreen::draw(&surface, WIDTH, HEIGHT);
    let open_path = offscreen::save_cropped(&open, WIDTH, HEIGHT, "palette-shortcut-list-open.png");
    let over_open = heading(&palette, &surface).unwrap_or_else(|| {
        panic!(
            "no heading over the open list. Look at {}",
            open_path.display()
        )
    });
    let drawn = list(&palette, &surface).unwrap_or_else(|| {
        panic!(
            "the open list draws no rows. Look at {}",
            open_path.display()
        )
    });
    assert!(
        drawn.top > over_open.bottom,
        "the rows {drawn:?} do not stand under the heading {over_open:?}. Look at {}",
        open_path.display()
    );

    // ---- folded: the heading where it was, no rows, a shorter palette ---------
    palette.set_hints_open(false);
    let folded = offscreen::draw(&surface, WIDTH, HEIGHT);
    let folded_path =
        offscreen::save_cropped(&folded, WIDTH, HEIGHT, "palette-shortcut-list-folded.png");
    let over_folded = heading(&palette, &surface).unwrap_or_else(|| {
        panic!(
            "no heading over the folded list. Look at {}",
            folded_path.display()
        )
    });
    assert_eq!(
        (over_folded.left, over_folded.top),
        (over_open.left, over_open.top),
        "folding the list moved its heading. Look at {} and {}",
        open_path.display(),
        folded_path.display()
    );
    assert!(
        list(&palette, &surface).is_none(),
        "the folded list still draws its rows. Look at {}",
        folded_path.display()
    );
    let taken_back = bottom_edge(&open).saturating_sub(bottom_edge(&folded));
    // The letters of the rows, first to last: the rows take at least this
    // much, so folding them must give back at least this much.
    let rows_tall = last_letters(&open, drawn.top) - drawn.top;
    assert!(
        taken_back >= rows_tall,
        "folding took back {taken_back} px, and the letters of the rows spanned {rows_tall} px. \
         Look at {} and {}",
        open_path.display(),
        folded_path.display()
    );

    // ---- the heading as drawn is what a click reaches -------------------------
    click(palette.window(), over_folded.centre());
    assert_eq!(
        toggles.get(),
        1,
        "a click on the heading did not ask to open the list"
    );
}
