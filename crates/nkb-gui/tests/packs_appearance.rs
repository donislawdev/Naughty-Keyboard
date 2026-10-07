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

const WIDTH: u32 = 520;
const HEIGHT: u32 = 600;

/// `accent` from the dictionary, copied rather than read for the reason the
/// palette test gives: a test that reads the value it checks cannot fail.
const ACCENT: (u8, u8, u8) = (0x7A, 0xA2, 0xF7);

/// `border-strong`, the scroll thumb's colour - copied for the same reason.
const THUMB: (u8, u8, u8) = (0x45, 0x4C, 0x5C);

/// The row of the next value in [`specimen`], selected for the pictures.
const NEXT_ROW: usize = 4;

/// The row of a pack with a description long enough for two lines in
/// [`specimen`] - the pack in use, so it is in view without scrolling.
const LONG_ROW: usize = 2;

/// Specimen data - the test's own, like the gallery's labels. The product fills
/// these from `nkb-adapters::i18n` and the catalogue.
fn fill(window: &PacksWindow, rows: Vec<PickRow>) {
    window.set_window_title("Naughty Keyboard - find a value".into());
    window.set_heading("Find a value".into());
    window.set_summary("values: 102, packs: 9".into());
    window.set_search_label("Search values and packs by name, tag or id".into());
    window.set_query("unicode".into());
    window.set_current_label("in use".into());
    window.set_empty_text("Nothing matches \"zzz\". Press Backspace to widen the search.".into());
    window.set_rows(ModelRc::new(VecModel::from(rows)));
    // The next value inside the opened pack in use - so the picture shows the
    // selection, the pill and the fold marks at once.
    window.set_selected(NEXT_ROW as i32);
    window.set_hints(ModelRc::new(VecModel::from(vec![
        HintRow {
            key: "Enter".into(),
            action: "Use the selected value or pack".into(),
        },
        HintRow {
            key: "Esc".into(),
            action: "Close".into(),
        },
        HintRow {
            key: "\u{2191} \u{2193}".into(),
            action: "Move through the list".into(),
        },
        HintRow {
            key: "\u{2190} \u{2192}".into(),
            action: "Show or hide values".into(),
        },
    ])));
}

/// A pack in the tree: its name, and what it is for, wrapped.
fn pack(title: &str, detail: &str) -> PickRow {
    PickRow {
        title: title.into(),
        detail: detail.into(),
        enabled: true,
        tree: true,
        foldable: true,
        detail_wraps: true,
        ..PickRow::default()
    }
}

/// A value under its pack: its name, and what it types.
fn value(title: &str, detail: &str) -> PickRow {
    PickRow {
        title: title.into(),
        detail: detail.into(),
        enabled: true,
        tree: true,
        child: true,
        detail_mono: true,
        ..PickRow::default()
    }
}

/// A section heading: one line, muted, never chosen.
fn heading(title: &str) -> PickRow {
    PickRow {
        title: title.into(),
        heading: true,
        single_line: true,
        tree: true,
        ..PickRow::default()
    }
}

/// The window as the tree opens it (the owner's points 4 to 6): the fold of
/// the values used last, closed, then the packs, each with what it is for - the pack
/// in use opened by the tester, with the row that starts it again (UX4) and
/// its values saying what each types, the next one marked - and the rest closed.
fn specimen() -> Vec<PickRow> {
    vec![
        PickRow {
            enabled: true,
            foldable: true,
            ..heading("Recent (2)")
        },
        heading("Packs"),
        PickRow {
            current: true,
            open: true,
            ..pack(
                "Whitespace",
                "Characters that take up space, or claim to, and are impossible to see in a form.",
            )
        },
        PickRow {
            single_line: true,
            key: "Alt+Shift+0".into(),
            has_key: true,
            detail_mono: false,
            ..value("Restart pack", "")
        },
        PickRow {
            badge: "next".into(),
            has_badge: true,
            badge_current: true,
            ..value("Trailing space", "Kowalski\u{2423}")
        },
        value(
            "A value whose name is long enough to need eliding here",
            "255 \u{D7} \"a\"",
        ),
        PickRow {
            badge: "offensive".into(),
            has_badge: true,
            badge_risky: true,
            ..pack("Injections", "Strings that a careless backend executes.")
        },
        pack(
            "Dates that cannot be",
            "Dates that do not exist, that are ambiguous, or that are written differently than \
             the field assumes.",
        ),
        pack(
            "Numbers at the extremes",
            "Numbers at the edges of their types, in foreign notations, and where precision runs \
             out.",
        ),
    ]
}

/// The window with a query typed: the values found, each with its pack and
/// where the query hit beside it, then a pack found by a tag.
fn search() -> Vec<PickRow> {
    vec![
        PickRow {
            tree: false,
            ..heading("Values")
        },
        PickRow {
            tree: false,
            child: false,
            aside: "Polish locale - in its id".into(),
            has_aside: true,
            ..value("PESEL with a valid checksum", "\u{2423}Kowalski")
        },
        PickRow {
            tree: false,
            ..heading("Packs")
        },
        PickRow {
            tree: false,
            foldable: false,
            aside: "in its tags".into(),
            has_aside: true,
            ..pack(
                "Whitespace",
                "Characters that take up space, or claim to, and are impossible to see in a form.",
            )
        },
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
    let list_band = 140..(HEIGHT - 140);

    let unfocused = offscreen::draw(&surface, WIDTH, HEIGHT);
    offscreen::save(&unfocused, WIDTH, HEIGHT, "packs.png");
    let before = accent_in(&unfocused, field_band.clone());

    // ---- the list says there is more below it ----------------------------------
    // The specimen is taller than the room the window leaves the list, so the
    // thumb at the list's right edge must be drawn - and with three rows it
    // must not. The pack window is the one whose list scrolls: the shortcuts
    // window is as tall as its rows since K5.7, so its own test can only see
    // the thumb absent, and mutation M308 walked past it (2026-10-07).
    let thumb_column = (WIDTH - 20)..(WIDTH - 12);
    let thumb_in = |picture: &[offscreen::Pixel]| {
        list_band
            .clone()
            .flat_map(|y| thumb_column.clone().map(move |x| (y * WIDTH + x) as usize))
            .filter(|&at| {
                let p = picture[at];
                (p.r, p.g, p.b) == THUMB
            })
            .count()
    };
    assert!(
        thumb_in(&unfocused) > 20,
        "the list is longer than its room and draws no scroll thumb - the rows below the \
         fold have no sign. Look at packs.png"
    );
    window.set_rows(ModelRc::new(VecModel::from(specimen()[..3].to_vec())));
    assert_eq!(
        thumb_in(&offscreen::draw(&surface, WIDTH, HEIGHT)),
        0,
        "three rows fit, and the list still draws a scroll thumb"
    );
    window.set_rows(ModelRc::new(VecModel::from(specimen())));

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

    // ---- the next value's pill wears the accent (UX2) -----------------------
    // A difference of two renders, the pill marked and not: the colour is the
    // whole difference between "next" and any other pill.
    let mut unmarked = specimen();
    unmarked[NEXT_ROW].badge_current = false;
    window.set_rows(ModelRc::new(VecModel::from(unmarked)));
    let plain = offscreen::draw(&surface, WIDTH, HEIGHT);
    window.set_rows(ModelRc::new(VecModel::from(specimen())));
    let marked = offscreen::draw(&surface, WIDTH, HEIGHT);
    let marked_path = offscreen::save(&marked, WIDTH, HEIGHT, "packs-next.png");
    assert!(
        accent_in(&marked, list_band.clone()) > accent_in(&plain, list_band.clone()) + 20,
        "the next value's pill does not wear the accent, so it looks like any other pill. \
         Look at {}",
        marked_path.display()
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

    // ---- the notes in the status line (K3.2c, D110) --------------------------
    // The sentence the product says today, and one twice as long. Drawn in the
    // status line, the last band of the window, where every window of the
    // product says what the tool has to say - so the list gives up the room
    // and the footer stands above it, while the header and the query line stay
    // exactly where they were. And WRAPPED: a text that does not wrap may not
    // be narrower than itself (`slint.md` 2.29), so it would run into the
    // right margin, which holds nothing but the grounds and the rules.
    window.set_rows(ModelRc::new(VecModel::from(specimen())));
    let bare = offscreen::draw(&surface, WIDTH, HEIGHT);
    window.set_notes(ModelRc::new(VecModel::from(vec![
        slint::SharedString::from(
            "Not read: team folder, own folder - cannot be set in this version.",
        ),
        slint::SharedString::from(
            "Not read: a source with a very long name that nobody would choose - the folder \
             could not be read, and this sentence is long enough to need a second line.",
        ),
    ])));
    let noted = offscreen::draw(&surface, WIDTH, HEIGHT);
    let noted_path = offscreen::save(&noted, WIDTH, HEIGHT, "packs-notes.png");
    let differ = |rows: std::ops::Range<u32>, columns: std::ops::Range<u32>| {
        rows.flat_map(|y| columns.clone().map(move |x| (y * WIDTH + x) as usize))
            .filter(|&at| bare[at] != noted[at])
            .count()
    };
    // The header and the query line: the title band and the line under it,
    // well short of the first row of the list.
    assert_eq!(
        differ(0..100, 0..WIDTH),
        0,
        "the notes moved the header or the query line. Look at {}",
        noted_path.display()
    );
    assert!(
        differ((HEIGHT - 80)..HEIGHT, 0..WIDTH) > 300,
        "the notes are not drawn in the status line at the bottom. Look at {}",
        noted_path.display()
    );
    // The right margin is narrower than the gutter, so no glyph and no control
    // ever reaches it: only the content's ground, the raised ground of the
    // chrome and the rules between them. Checked on both renders - on the bare
    // one it is the control that the predicate itself is not too strict.
    let grounds = [(0x14, 0x16, 0x1A), (0x1C, 0x1F, 0x26), (0x2A, 0x2F, 0x3A)];
    let in_margin = |buffer: &[offscreen::Pixel]| {
        (0..HEIGHT)
            .flat_map(|y| ((WIDTH - 12)..WIDTH).map(move |x| (y * WIDTH + x) as usize))
            .filter(|&at| {
                let pixel = buffer[at];
                !grounds.contains(&(pixel.r, pixel.g, pixel.b))
            })
            .count()
    };
    assert_eq!(
        in_margin(&bare),
        0,
        "the bare window draws something other than its grounds in the right margin, so the \
         check below cannot tell a wrapped note from one that ran over"
    );
    assert_eq!(
        in_margin(&noted),
        0,
        "a note ran into the right margin instead of wrapping. Look at {}",
        noted_path.display()
    );

    // ---- what a pack is for is read whole (the owner's point 6) -------------
    // A long description WRAPS: the same list with it cut to a word, and the
    // rows under it must move down - a description elided at the end of one
    // line would change one line and move nothing. And it wraps inside the
    // window: the right margin stays as it was.
    window.set_notes(ModelRc::new(VecModel::from(
        Vec::<slint::SharedString>::new(),
    )));
    let mut short = specimen();
    short[LONG_ROW].detail = "Short.".into();
    window.set_rows(ModelRc::new(VecModel::from(short)));
    let one_line = offscreen::draw(&surface, WIDTH, HEIGHT);
    window.set_rows(ModelRc::new(VecModel::from(specimen())));
    let wrapped = offscreen::draw(&surface, WIDTH, HEIGHT);
    let tree_path = offscreen::save(&wrapped, WIDTH, HEIGHT, "packs-tree.png");
    let moved = offscreen::added(&one_line, &wrapped, WIDTH, HEIGHT)
        .expect("a longer description changed nothing on screen");
    assert!(
        moved.bottom - moved.top > 40,
        "a long description changed one line only ({moved:?}), so it is cut rather than \
         wrapped and the tester cannot read what the pack is for. Look at {}",
        tree_path.display()
    );
    let margin = |picture: &[offscreen::Pixel]| {
        (0..HEIGHT)
            .flat_map(|y| ((WIDTH - 12)..WIDTH).map(move |x| (y * WIDTH + x) as usize))
            .map(|at| picture[at])
            .collect::<Vec<_>>()
    };
    assert!(
        margin(&one_line) == margin(&wrapped),
        "a description ran into the right margin. Look at {}",
        tree_path.display()
    );

    // ---- a value is one line, one step in from its pack (the owner's point 3) --
    // Until 2026-10-07 a value stood two whole columns in from its pack's name -
    // the second one empty, since a value never folds - over two short lines,
    // and the owner found the room left and right of it "fatal". Now its name
    // stands ONE step in from the pack's name, and what it types stands on the
    // same line at the row's right edge (`D110`). Each found as the pixels its
    // words change, so the measurement is where the words ARE.
    let emptied = |at: usize, title: bool| {
        let mut rows = specimen();
        if title {
            rows[at].title = "".into();
        } else {
            rows[at].detail = "".into();
        }
        window.set_rows(ModelRc::new(VecModel::from(rows)));
        let picture = offscreen::draw(&surface, WIDTH, HEIGHT);
        window.set_rows(ModelRc::new(VecModel::from(specimen())));
        picture
    };
    let pack_name = offscreen::added(&emptied(LONG_ROW, true), &wrapped, WIDTH, HEIGHT)
        .expect("the pack's name is not drawn");
    let value_name = offscreen::added(&emptied(NEXT_ROW, true), &wrapped, WIDTH, HEIGHT)
        .expect("the value's name is not drawn");
    let value_types = offscreen::added(&emptied(NEXT_ROW, false), &wrapped, WIDTH, HEIGHT)
        .expect("what the value types is not drawn");
    let step = value_name.left.saturating_sub(pack_name.left);
    assert!(
        value_name.left > pack_name.left && step <= 24,
        "a value's name stands {step} px in from its pack's name ({value_name:?} against \
         {pack_name:?}) - one step of the tree is the fold column at most, and two columns \
         left the room the owner called fatal. Look at {}",
        tree_path.display()
    );
    assert!(
        value_types.top < value_name.bottom && value_types.bottom > value_name.top,
        "what the value types is not on its name's line ({value_types:?} against \
         {value_name:?}) - two short lines leave the row's right side empty. Look at {}",
        tree_path.display()
    );
    assert!(
        value_types.right + 40 > WIDTH,
        "what the value types does not end at the row's right edge ({value_types:?}), so the \
         previews of the list do not stand in one column. Look at {}",
        tree_path.display()
    );

    // ---- a click on a fold mark asks for that row's fold (point 4) ----------
    // The mark found as the pixels it changes between open and closed - the
    // only difference between the two renders - and clicked there: the list
    // must say which row, and choose nothing.
    let toggled = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let chosen = std::rc::Rc::new(std::cell::Cell::new(0));
    let log = std::rc::Rc::clone(&toggled);
    window.on_toggled(move |row| log.borrow_mut().push(row));
    let count = std::rc::Rc::clone(&chosen);
    window.on_clicked(move |_| count.set(count.get() + 1));
    let mut closed = specimen();
    closed[LONG_ROW].open = false;
    window.set_rows(ModelRc::new(VecModel::from(closed)));
    let mark_closed = offscreen::draw(&surface, WIDTH, HEIGHT);
    window.set_rows(ModelRc::new(VecModel::from(specimen())));
    let mark_open = offscreen::draw(&surface, WIDTH, HEIGHT);
    let mark = offscreen::added(&mark_closed, &mark_open, WIDTH, HEIGHT)
        .expect("the fold mark does not change between open and closed");
    assert!(
        mark.right - mark.left < 20 && mark.bottom - mark.top < 20 && mark.left < WIDTH / 4,
        "open and closed differ in more than the mark at the row's left: {mark:?}. Look at {}",
        tree_path.display()
    );
    offscreen::click(window.window(), mark.centre());
    assert_eq!(
        (toggled.borrow().clone(), chosen.get()),
        (vec![LONG_ROW as i32], 0),
        "a click on the fold mark did not ask for that row's fold alone"
    );

    // ---- a search: each found value names its pack beside it (point 5) -----
    // The aside is drawn - a difference of two renders - and on the right of
    // the row, beside what the value types, not over it.
    window.set_query("pesel".into());
    let mut found = search();
    found[1].has_aside = false;
    window.set_rows(ModelRc::new(VecModel::from(found)));
    window.set_selected(1);
    let without_aside = offscreen::draw(&surface, WIDTH, HEIGHT);
    window.set_rows(ModelRc::new(VecModel::from(search())));
    let with_aside = offscreen::draw(&surface, WIDTH, HEIGHT);
    let search_path = offscreen::save(&with_aside, WIDTH, HEIGHT, "packs-search.png");
    let aside = offscreen::added(&without_aside, &with_aside, WIDTH, HEIGHT)
        .expect("the value's pack is not drawn beside it");
    assert!(
        aside.left > WIDTH / 2 && aside.bottom - aside.top < 30,
        "the pack beside a found value is not one line at the right of its row: {aside:?}. \
         Look at {}",
        search_path.display()
    );
}
