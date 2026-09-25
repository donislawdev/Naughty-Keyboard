//! The pack window, rendered without a screen, and its two keyboard signs
//! MEASURED rather than trusted.
//!
//! # What this proves that reading `packs.slint` cannot
//!
//! GUI rule 10: a property written in a view can be ignored by the toolkit and
//! look exactly like one that works. The pack window has two things a tester
//! steers by without a mouse - the query line holding the keyboard, and the
//! row the arrow keys have reached - and both are drawn in the accent. So this
//! counts accent pixels in the line and in the list, each with a control that
//! must come out the other way: the line before it is focused, and a list
//! with no rows.
//!
//! The query line has no text input (`OBS-145`), so its mark after the text is
//! drawn by us rather than by the toolkit, and a query longer than the line is
//! scrolled by us too. Both are measured here: the mark exists only with the
//! keyboard, and a long query keeps the mark inside the line without making
//! the line wider.
//!
//! Its own binary for the reason `palette_appearance.rs` gives: one platform
//! per process.

#![allow(clippy::panic, clippy::expect_used)]

// The shared harness carries helpers other render tests use and this one does not.
#[allow(dead_code)]
mod offscreen;

use nkb_gui::{HintRow, PacksWindow, PickRow};
use slint::{ComponentHandle, ModelRc, VecModel};

const WIDTH: u32 = 440;
const HEIGHT: u32 = 560;

/// `accent` from the dictionary, copied rather than read for the reason the
/// palette test gives: a test that reads the value it checks cannot fail.
const ACCENT: (u8, u8, u8) = (0x7A, 0xA2, 0xF7);

/// Specimen data - the test's own, like the gallery's labels. The product fills
/// these from `nkb-adapters::i18n` and the catalogue.
fn fill(window: &PacksWindow, rows: Vec<PickRow>) {
    window.set_window_title("Naughty Keyboard - packs".into());
    window.set_heading("Packs".into());
    window.set_summary("packs: 9".into());
    window.set_search_label("Search by name, tag or description".into());
    window.set_query("unicode".into());
    window.set_current_label("in use".into());
    window.set_empty_text("No pack matches \"zzz\". Clear the search to see every pack.".into());
    window.set_rows(ModelRc::new(VecModel::from(rows)));
    window.set_selected(1);
    window.set_hints(ModelRc::new(VecModel::from(vec![
        HintRow {
            key: "Enter".into(),
            action: "Open the pack".into(),
        },
        HintRow {
            key: "Esc".into(),
            action: "Close".into(),
        },
        HintRow {
            key: "\u{2191} \u{2193}".into(),
            action: "Move through the list".into(),
        },
    ])));
}

fn row(title: &str, detail: &str, current: bool) -> PickRow {
    PickRow {
        title: title.into(),
        detail: detail.into(),
        badge: "".into(),
        has_badge: false,
        badge_risky: false,
        current,
        enabled: true,
    }
}

fn specimen() -> Vec<PickRow> {
    vec![
        row("Whitespace", "whitespace . values: 12", false),
        row("Unicode & text", "unicode-text . values: 34", true),
        PickRow {
            badge: "offensive".into(),
            has_badge: true,
            badge_risky: true,
            ..row("Injections", "injections . values: 40", false)
        },
        row(
            "Numbers at the extremes",
            "numbers-extreme . values: 12",
            false,
        ),
    ]
}

fn is_accent(buffer: &[offscreen::Pixel], x: u32, y: u32) -> bool {
    let p = buffer[(y * WIDTH + x) as usize];
    (p.r, p.g, p.b) == ACCENT
}

/// Accent pixels inside a horizontal band of the buffer.
fn accent_in(buffer: &[offscreen::Pixel], rows: std::ops::Range<u32>) -> usize {
    rows.flat_map(|y| (0..WIDTH).map(move |x| (x, y)))
        .filter(|&(x, y)| is_accent(buffer, x, y))
        .count()
}

/// The box the accent edge draws around the focused line: the first and last
/// column and row holding an accent pixel in the band.
struct Edge {
    left: u32,
    right: u32,
    top: u32,
    bottom: u32,
}

fn accent_edge(buffer: &[offscreen::Pixel], rows: std::ops::Range<u32>) -> Edge {
    let hits: Vec<(u32, u32)> = rows
        .flat_map(|y| (0..WIDTH).map(move |x| (x, y)))
        .filter(|&(x, y)| is_accent(buffer, x, y))
        .collect();
    let xs = hits.iter().map(|&(x, _)| x);
    let ys = hits.iter().map(|&(_, y)| y);
    Edge {
        left: xs
            .clone()
            .min()
            .expect("the focused line draws an accent edge"),
        right: xs.max().expect("the focused line draws an accent edge"),
        top: ys
            .clone()
            .min()
            .expect("the focused line draws an accent edge"),
        bottom: ys.max().expect("the focused line draws an accent edge"),
    }
}

/// Accent pixels INSIDE the edge, three pixels in from it so the rounded
/// corners and the edge's own antialiasing never count - that leaves the mark.
/// Returns how many, and the rightmost column that holds one.
fn mark_inside(buffer: &[offscreen::Pixel], edge: &Edge) -> (usize, Option<u32>) {
    let inset = 3;
    let mut count = 0;
    let mut rightmost = None;
    for y in (edge.top + inset)..=(edge.bottom - inset) {
        for x in (edge.left + inset)..=(edge.right - inset) {
            if is_accent(buffer, x, y) {
                count += 1;
                rightmost = Some(rightmost.map_or(x, |r: u32| r.max(x)));
            }
        }
    }
    (count, rightmost)
}

#[test]
fn the_pack_window_shows_where_the_keyboard_is_and_which_row_it_reached() {
    let surface = offscreen::start(WIDTH, HEIGHT);
    let window = PacksWindow::new().expect("the pack window must build");
    fill(&window, specimen());
    window.show().expect("the pack window must show");

    // The header and the query line take the top of the window, the list
    // starts below them. Bands rather than coordinates of one element, so a
    // spacing change does not break the test - only a state that stops being
    // drawn does.
    let field_band = 0..140;
    let list_band = 140..(HEIGHT - 120);

    let unfocused = offscreen::draw(&surface, WIDTH, HEIGHT);
    offscreen::save(&unfocused, WIDTH, HEIGHT, "packs.png");
    let before = accent_in(&unfocused, field_band.clone());

    window.invoke_focus_search();
    let focused = offscreen::draw(&surface, WIDTH, HEIGHT);
    let path = offscreen::save(&focused, WIDTH, HEIGHT, "packs-focused.png");
    let after = accent_in(&focused, field_band.clone());
    assert!(
        after > before + 100,
        "the query line holding the keyboard is not drawn: {before} accent pixels before \
         the focus, {after} after. Look at {}",
        path.display()
    );

    // The mark after the text: there with the keyboard, and absent without it -
    // the unfocused render has no accent edge to measure inside, so the control
    // is that the whole band had almost none.
    let edge = accent_edge(&focused, field_band.clone());
    let (mark, _) = mark_inside(&focused, &edge);
    assert!(
        mark >= 10,
        "the line holds the keyboard but draws no mark after the text ({mark} accent pixels \
         inside its edge). Look at {}",
        path.display()
    );
    assert!(
        before < 10,
        "the line draws {before} accent pixels before it has the keyboard"
    );

    // A query far longer than the line: the line keeps its width, and the mark
    // stays inside it, near the right edge - the end of the query is in view.
    // Measured: sixty characters still fit, so this is well over a hundred.
    let long_query = [
        "a query typed on and on and on, well past the point where any pack name",
        "could still match it, until it runs far beyond the right edge of the line",
    ]
    .join(" ");
    window.set_query(long_query.into());
    let long = offscreen::draw(&surface, WIDTH, HEIGHT);
    let long_path = offscreen::save(&long, WIDTH, HEIGHT, "packs-long-query.png");
    let long_edge = accent_edge(&long, field_band.clone());
    assert!(
        (long_edge.left, long_edge.right) == (edge.left, edge.right),
        "a long query changed the line's width: {}..{} became {}..{}. Look at {}",
        edge.left,
        edge.right,
        long_edge.left,
        long_edge.right,
        long_path.display()
    );
    let (long_mark, rightmost) = mark_inside(&long, &long_edge);
    let near_the_end = rightmost.is_some_and(|x| x + 20 > long_edge.right);
    assert!(
        long_mark >= 10 && near_the_end,
        "a long query pushed the mark out of the line ({long_mark} pixels, rightmost at \
         {rightmost:?}, edge at {}): the end of the query is not in view. Look at {}",
        long_edge.right,
        long_path.display()
    );
    window.set_query("unicode".into());

    let selected = accent_in(&focused, list_band.clone());
    assert!(
        selected > 200,
        "the selected row carries {selected} accent pixels - the row the arrow keys reached \
         is not visible. Look at {}",
        path.display()
    );

    // The negative control: no rows, no selected row, and the empty sentence
    // in their place. The accent that was there came from the row, not from
    // something else standing in the same band.
    window.set_rows(ModelRc::new(VecModel::from(Vec::<PickRow>::new())));
    let empty = offscreen::draw(&surface, WIDTH, HEIGHT);
    let empty_path = offscreen::save(&empty, WIDTH, HEIGHT, "packs-empty.png");
    assert!(
        accent_in(&empty, list_band) < selected / 4,
        "an empty list still draws the selection edge. Look at {}",
        empty_path.display()
    );

    // Repeatable, or comparing two renders means nothing.
    assert!(
        empty == offscreen::draw(&surface, WIDTH, HEIGHT),
        "two renders of an unchanged window differ"
    );
}
