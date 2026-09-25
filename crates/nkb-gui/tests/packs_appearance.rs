//! The pack window, rendered without a screen, and its two keyboard signs
//! MEASURED rather than trusted.
//!
//! # What this proves that reading `packs.slint` cannot
//!
//! GUI rule 10: a property written in a view can be ignored by the toolkit and
//! look exactly like one that works. The pack window has two things a tester
//! steers by without a mouse - the search field holding the keyboard, and the
//! row the arrow keys have reached - and both are drawn in the accent. So this
//! counts accent pixels in the field and in the list, each with a control that
//! must come out the other way: the field before it is focused, and a list
//! with no rows.
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

/// Accent pixels inside a horizontal band of the buffer.
fn accent_in(buffer: &[offscreen::Pixel], rows: std::ops::Range<u32>) -> usize {
    rows.flat_map(|y| (0..WIDTH).map(move |x| (x, y)))
        .filter(|(x, y)| {
            let p = buffer[(y * WIDTH + x) as usize];
            (p.r, p.g, p.b) == ACCENT
        })
        .count()
}

#[test]
fn the_pack_window_shows_where_the_keyboard_is_and_which_row_it_reached() {
    let surface = offscreen::start(WIDTH, HEIGHT);
    let window = PacksWindow::new().expect("the pack window must build");
    fill(&window, specimen());
    window.show().expect("the pack window must show");

    // The header and the search field take the top of the window, the list
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
    let after = accent_in(&focused, field_band);
    assert!(
        after > before + 100,
        "the search field holding the keyboard is not drawn: {before} accent pixels before \
         the focus, {after} after. Look at {}",
        path.display()
    );

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
