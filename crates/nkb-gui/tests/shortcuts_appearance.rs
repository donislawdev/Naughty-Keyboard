//! The shortcuts window, rendered without a screen, and the four things its
//! design promises MEASURED rather than trusted (GUI rule 10).
//!
//! # What the design promises
//!
//! - one line per shortcut: `single-line` really takes the detail line away,
//!   measured against the same rows drawn with it.
//! - the combinations end in ONE column at the right, whatever pills stand
//!   before them and however long the combination is - the way a menu shows
//!   its shortcuts.
//! - at the height its ten rows ask for, nothing scrolls. The height the
//!   window asks for itself is `shortcuts_size.rs`.
//! - the row being recorded says so in a pill, beside the accent edge of the
//!   selected row - a second sign on the same row, not a replacement.
//!
//! Its own binary for the reason `palette_appearance.rs` gives: one platform
//! per process. The specimen words are the test's own, like the gallery's
//! labels - the product fills them from `nkb-adapters::i18n`.

#![allow(clippy::panic, clippy::expect_used)]

// The shared harness carries helpers other render tests use and this one does not.
#[allow(dead_code)]
mod offscreen;

use nkb_gui::{HintRow, PickRow, ShortcutsWindow};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

const WIDTH: u32 = 480;
/// Taller than the ten rows need, so a list that still scrolls is a list
/// whose rows are taller than the design says.
const TALL: u32 = 900;

/// Colours from the dictionary, copied rather than read for the reason the
/// palette test gives: a test that reads the value it checks cannot fail.
const SURFACE: (u8, u8, u8) = (0x14, 0x16, 0x1A);
const ACCENT: (u8, u8, u8) = (0x7A, 0xA2, 0xF7);
const THUMB: (u8, u8, u8) = (0x45, 0x4C, 0x5C);

/// The ten actions of `ux-spec.md` 3 with the default combinations, one of
/// them the tester's own and one taken by another application.
fn specimen(single_line: bool) -> Vec<PickRow> {
    let row = |title: &str, key: &str| PickRow {
        title: title.into(),
        detail: "default".into(),
        badge: SharedString::new(),
        has_badge: false,
        badge_risky: false,
        current: false,
        enabled: true,
        single_line,
        key: key.into(),
        has_key: true,
    };
    vec![
        row("Next value", "Alt+Shift+N"),
        PickRow {
            badge: "changed".into(),
            has_badge: true,
            ..row("Previous value", "Ctrl+Alt+Shift+Win+F24")
        },
        row("Repeat last value", "Alt+Shift+R"),
        row("Restart pack", "Alt+Shift+0"),
        row("Copy report block", "Alt+Shift+B"),
        row("Mark as working", "Alt+Shift+1"),
        row("Mark as a problem", "Alt+Shift+2"),
        row("Mark as suspect", "Alt+Shift+3"),
        PickRow {
            badge: "taken".into(),
            has_badge: true,
            badge_risky: true,
            ..row("Open pack search", "Alt+Shift+Space")
        },
        row("Collapse or expand the palette", "Alt+Shift+H"),
    ]
}

fn hints(rows: &[(&str, &str)]) -> ModelRc<HintRow> {
    ModelRc::new(VecModel::from(
        rows.iter()
            .map(|(key, action)| HintRow {
                key: (*key).into(),
                action: (*action).into(),
            })
            .collect::<Vec<_>>(),
    ))
}

fn lines(lines: &[&str]) -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from(
        lines
            .iter()
            .map(|line| SharedString::from(*line))
            .collect::<Vec<_>>(),
    ))
}

fn fill(window: &ShortcutsWindow) {
    window.set_window_title("Naughty Keyboard - shortcuts".into());
    window.set_heading("Shortcuts".into());
    window.set_summary("changed: 1 of 10".into());
    window.set_intro(
        "The palette answers these in every application. They are paused while this window \
         is open."
            .into(),
    );
    window.set_rows(ModelRc::new(VecModel::from(specimen(true))));
    window.set_selected(2);
    window.set_current_label("recording".into());
    window.set_hints(hints(&[
        ("Enter", "Record a new shortcut"),
        ("Delete", "Restore the default"),
        ("\u{2191} \u{2193}", "Move through the list"),
        ("Esc", "Close"),
    ]));
    window.set_messages(lines(&[
        "\"Previous value\" now answers Ctrl+Alt+Shift+Win+F24.",
    ]));
}

fn pixel(buffer: &[offscreen::Pixel], width: u32, x: u32, y: u32) -> (u8, u8, u8) {
    let p = buffer[(y * width + x) as usize];
    (p.r, p.g, p.b)
}

/// The rows of the buffer between the header and the footer - where the ground
/// at the window's left edge is the plain surface. The chrome bands stand on a
/// ground of their own, so this finds the content without knowing its offsets.
fn content(buffer: &[offscreen::Pixel], width: u32, height: u32) -> std::ops::Range<u32> {
    let plain = |y: u32| pixel(buffer, width, 1, y) == SURFACE;
    let top = (0..height)
        .find(|&y| plain(y))
        .expect("the window has a content area");
    let bottom = (top..height)
        .find(|&y| !plain(y))
        .expect("the footer stands below the content");
    top..bottom
}

/// Bands of rows holding anything but the surface, left to right inside the
/// content's padding - one band per drawn line of the list when the intro and
/// the messages are empty.
fn ink_bands(
    buffer: &[offscreen::Pixel],
    width: u32,
    rows: std::ops::Range<u32>,
) -> Vec<std::ops::Range<u32>> {
    let inked = |y: u32| (2..width - 2).any(|x| pixel(buffer, width, x, y) != SURFACE);
    let mut bands = Vec::new();
    let mut start = None;
    for y in rows.clone() {
        match (inked(y), start) {
            (true, None) => start = Some(y),
            (false, Some(from)) => {
                bands.push(from..y);
                start = None;
            }
            _ => {}
        }
    }
    if let Some(from) = start {
        bands.push(from..rows.end);
    }
    bands
}

fn rightmost_ink(buffer: &[offscreen::Pixel], width: u32, band: &std::ops::Range<u32>) -> u32 {
    band.clone()
        .flat_map(|y| (0..width).map(move |x| (x, y)))
        .filter(|&(x, y)| pixel(buffer, width, x, y) != SURFACE)
        .map(|(x, _)| x)
        .max()
        .expect("an ink band holds ink")
}

fn count(
    buffer: &[offscreen::Pixel],
    width: u32,
    rows: std::ops::Range<u32>,
    columns: std::ops::Range<u32>,
    colour: (u8, u8, u8),
) -> usize {
    rows.flat_map(|y| columns.clone().map(move |x| (x, y)))
        .filter(|&(x, y)| pixel(buffer, width, x, y) == colour)
        .count()
}

#[test]
fn the_shortcuts_window_keeps_its_promises() {
    let surface = offscreen::start(WIDTH, TALL);
    let window = ShortcutsWindow::new().expect("the shortcuts window must build");
    fill(&window);
    window.show().expect("the shortcuts window must show");

    // The picture a person looks at: the window as the product will fill it.
    let idle = offscreen::draw(&surface, WIDTH, TALL);
    let idle_path = offscreen::save(&idle, WIDTH, TALL, "shortcuts.png");

    // ---- one line per shortcut, and every combination in one column ---------
    // Nothing selected and nothing said, so every band of ink in the content
    // is a row of the list.
    window.set_intro(SharedString::new());
    window.set_messages(lines(&[]));
    window.set_selected(-1);
    let bare = offscreen::draw(&surface, WIDTH, TALL);
    let bare_path = offscreen::save(&bare, WIDTH, TALL, "shortcuts-bare.png");
    let area = content(&bare, WIDTH, TALL);
    let single = ink_bands(&bare, WIDTH, area.clone());
    assert_eq!(
        single.len(),
        10,
        "at a height taller than ten rows need, {} rows are drawn instead of ten. Look at {}",
        single.len(),
        bare_path.display()
    );
    let ends: Vec<u32> = single
        .iter()
        .map(|band| rightmost_ink(&bare, WIDTH, band))
        .collect();
    let (first, last) = (
        *ends.iter().min().expect("ten rows"),
        *ends.iter().max().expect("ten rows"),
    );
    // Three pixels, because the last glyph's own side bearing differs - `1`
    // ends before `N` in the same cell. Combinations aligned to their START
    // would end at least a cell apart per extra character: 31 px between
    // `Alt+Shift+N` and `Alt+Shift+Space`, measured by the same test.
    assert!(
        last - first <= 3,
        "the combinations do not end in one column: the rows end between x={first} and \
         x={last}. Look at {}",
        bare_path.display()
    );
    // The column is the COMBINATION's, not whatever ends a row: in the two rows
    // with a pill, nothing in the pill's colours may stand in the last 40
    // pixels, where the combination is.
    for (at, tint) in [(1, (0x7E, 0x87, 0x97)), (8, (0xE0, 0xAF, 0x68))] {
        let pill_end = single[at]
            .clone()
            .flat_map(|y| (0..WIDTH).map(move |x| (x, y)))
            .filter(|&(x, y)| pixel(&bare, WIDTH, x, y) == tint)
            .map(|(x, _)| x)
            .max()
            .unwrap_or(0);
        assert!(
            pill_end > 0 && pill_end + 40 < ends[at],
            "row {at}: the pill ends at x={pill_end} and the row at x={} - the pill is not \
             before the combination. Look at {}",
            ends[at],
            bare_path.display()
        );
    }
    assert_eq!(
        count(&bare, WIDTH, area.clone(), (WIDTH - 24)..WIDTH, THUMB),
        0,
        "the list shows a scroll thumb although the window is taller than its rows. Look at {}",
        bare_path.display()
    );

    // The negative control for `single-line`: the same rows with their detail
    // line. Taller bands, or the flag decides nothing.
    window.set_rows(ModelRc::new(VecModel::from(specimen(false))));
    let double = offscreen::draw(&surface, WIDTH, TALL);
    let double_path = offscreen::save(&double, WIDTH, TALL, "shortcuts-two-lines.png");
    // Measured by where the list ENDS, not by bands: with a typeface whose
    // lines stand further apart, a title and its detail are two bands of ink
    // with the ground between them - measured under WSL, 18 bands for ten
    // rows. Ten detail lines push the end down by at least eight pixels each.
    let two = ink_bands(&double, WIDTH, content(&double, WIDTH, TALL));
    let single_end = single.last().map_or(0, |band| band.end);
    let double_end = two.last().map_or(0, |band| band.end);
    assert!(
        two.len() >= 10 && double_end > single_end + 10 * 8,
        "rows with a detail line are not taller than rows without one: the list ends at \
         y={double_end} with them and y={single_end} without. Look at {}",
        double_path.display()
    );
    window.set_rows(ModelRc::new(VecModel::from(specimen(true))));

    // ---- the row being recorded says so, beside the selection edge -----------
    window.set_selected(0);
    let selected = offscreen::draw(&surface, WIDTH, TALL);
    let mut rows = specimen(true);
    rows[0].current = true;
    rows[0].key = "Alt+Shift+\u{2026}".into();
    window.set_rows(ModelRc::new(VecModel::from(rows)));
    let recording = offscreen::draw(&surface, WIDTH, TALL);
    let recording_path = offscreen::save(&recording, WIDTH, TALL, "shortcuts-recording.png");
    // The selection edge is drawn on the row's BOX, which reaches past its ink
    // by the padding and by whatever the typeface leaves above and below its
    // letters. So the band runs from the top of the content to halfway to the
    // second row's ink - a fixed margin around the letters held on Windows
    // and missed the edge under WSL, where the lines stand further apart.
    let first_row = area.start..(single[0].end + single[1].start) / 2;
    let edge_only = count(&selected, WIDTH, first_row.clone(), 0..WIDTH, ACCENT);
    let with_pill = count(&recording, WIDTH, first_row, 0..WIDTH, ACCENT);
    assert!(
        edge_only > 200,
        "the selected row carries {edge_only} accent pixels - the row the arrow keys reached \
         is not visible. Look at {}",
        idle_path.display()
    );
    assert!(
        with_pill > edge_only + 60,
        "recording adds {} accent pixels to the selected row - the pill that says so is not \
         drawn. Look at {}",
        with_pill.saturating_sub(edge_only),
        recording_path.display()
    );
    // A list must hold still: the pill that appears when recording starts
    // leaves every row below exactly where it was. Compared on renders with
    // nothing else said, so only the rows are in the content.
    let settled = ink_bands(&selected, WIDTH, content(&selected, WIDTH, TALL));
    let moved = ink_bands(&recording, WIDTH, content(&recording, WIDTH, TALL));
    assert!(
        settled.len() == moved.len() && settled[1..] == moved[1..],
        "the recording pill moved the rows below it: {:?} became {:?}. Look at {}",
        settled.iter().skip(1).map(|b| b.start).collect::<Vec<_>>(),
        moved.iter().skip(1).map(|b| b.start).collect::<Vec<_>>(),
        recording_path.display()
    );

    // Repeatable, or comparing two renders means nothing.
    assert!(
        recording == offscreen::draw(&surface, WIDTH, TALL),
        "two renders of an unchanged window differ"
    );

    // Until K5.7 a third section drew the window at its minimum size, where the
    // list scrolled. The window has no minimum since then: its size is bound to
    // its content, because the live window ignored a preferred size and opened
    // at the minimum (`slint.md` 2.38). What size it asks for, and what it
    // shows at that size, is `shortcuts_size.rs`.
}
