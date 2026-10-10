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
use offscreen::{Ink, added, click, point};
use slint::platform::software_renderer::MinimalSoftwareWindow;
use slint::{ComponentHandle, LogicalPosition};

const WIDTH: u32 = 420;
const HEIGHT: u32 = 460;

/// How much bluer than red a pixel must be to count as the accent's ink - the
/// threshold `palette_opener.rs` measured for the same two text roles.
const BLUER_THAN_RED: u8 = 40;

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

fn away(palette: &Palette) {
    point(
        palette.window(),
        LogicalPosition::new((WIDTH - 4) as f32, (HEIGHT - 4) as f32),
    );
}

/// Rows with no change between two buttons before they count as two boxes.
const BETWEEN_BUTTONS: u32 = 4;

/// Every button as the pixels its word changes, top to bottom: the same render
/// without the word and with it, so the pointer goes where a button IS. A box
/// is the whole frame rather than the word alone, because an empty word
/// narrows the button - measured on the first run, 21 pixels tall.
///
/// Boxes rather than one: the two buttons share their word, and since `D110`
/// the band of the last value stands before the first one is sent, its Copy
/// faded in place - so the word changes the render in two places at once.
fn find_buttons(palette: &Palette, surface: &Rc<MinimalSoftwareWindow>) -> Vec<Ink> {
    palette.set_copy_label("".into());
    let without = offscreen::draw(surface, WIDTH, HEIGHT);
    palette.set_copy_label("Copy".into());
    let with = offscreen::draw(surface, WIDTH, HEIGHT);
    let mut boxes: Vec<Ink> = Vec::new();
    for y in 0..HEIGHT {
        let changed: Vec<u32> = (0..WIDTH)
            .filter(|&x| without[(y * WIDTH + x) as usize] != with[(y * WIDTH + x) as usize])
            .collect();
        let (Some(&left), Some(&right)) = (changed.iter().min(), changed.iter().max()) else {
            continue;
        };
        match boxes.last_mut() {
            Some(open) if y <= open.bottom + BETWEEN_BUTTONS => {
                open.left = open.left.min(left);
                open.right = open.right.max(right);
                open.bottom = y;
            }
            _ => boxes.push(Ink {
                left,
                right,
                top: y,
                bottom: y,
            }),
        }
    }
    boxes
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
    // The arrows stand in the next band's row with their words, as on screen.
    palette.set_back_label("Back".into());
    palette.set_skip_label("Skip".into());
    palette.set_can_go_back(true);
    let steps = Rc::new(Cell::new(0));
    let count = Rc::clone(&steps);
    palette.on_back_one_value(move || count.set(count.get() + 1));
    let count = Rc::clone(&steps);
    palette.on_skip_value(move || count.set(count.get() + 1));
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
    let both = find_buttons(&palette, &surface);
    assert_eq!(
        both.len(),
        2,
        "before the first value the next band's Copy and the value band's faded one must \
         both stand, in two places: {both:?}"
    );
    // The arrows that walk the pack stand left of Copy in the same row (`D120`),
    // and a word of another width moves them too, so the box measured there
    // reaches over them. Copy ends the row, so its right edge is its own: the
    // box is cut to the width of the button in the band below, which stands
    // alone in its row.
    let width = both[1].right - both[1].left;
    let button = Ink {
        left: both[0].right - width,
        ..both[0]
    };
    assert!(
        button.left > WIDTH / 2 && button.bottom - button.top < 30,
        "the word of the next band's button is not one short word at the right end of \
         its row: {button:?}"
    );
    let rest = offscreen::draw(&surface, WIDTH, HEIGHT);
    let before = accent_in(&rest, button);
    point(palette.window(), button.centre());
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

    click(palette.window(), button.centre());
    assert_eq!(
        (next.get(), last.get(), packs.get()),
        (1, 0, 0),
        "a click on the next band's Copy must ask for the next value, once, and nothing else"
    );
    assert_eq!(steps.get(), 0, "a click on Copy walked the pack");

    // ---- not at the end of a pack: nothing to copy -----------------------------
    palette.set_next_has_value(false);
    palette.set_next_heading("Next: end of pack (12/12)".into());
    click(palette.window(), button.centre());
    assert_eq!(
        next.get(),
        1,
        "a Copy button answered at the end of the pack, where there is no value"
    );

    // ---- the button beside the value sent last ----------------------------------
    palette.set_has_next(false);
    palette.set_has_value(true);
    let only = find_buttons(&palette, &surface);
    assert_eq!(only.len(), 1, "one band, one button: {only:?}");
    let button = only[0];
    assert!(
        button.left > WIDTH / 2 && button.bottom - button.top < 30,
        "the word of the value band's button is not one short word at the right end of \
         its row: {button:?}"
    );
    click(palette.window(), button.centre());
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
    // Found again rather than reused: the faded word is a different picture.
    palette.set_copy_label("".into());
    let without = offscreen::draw(&surface, WIDTH, HEIGHT);
    let faded = added(&without, &sending, WIDTH, HEIGHT).expect("the faded word is drawn");
    // 🔴 The word back BEFORE the press: without it the button is only its
    // padding, narrower than the box measured with the word, and a press at
    // the box's centre lands beside it - the first run of mutation M435 passed
    // for exactly that reason.
    palette.set_copy_label("Copy".into());
    click(palette.window(), faded.centre());
    point(palette.window(), faded.centre());
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

/// The arrows beside the next value (`D120`): each asks for its own step and
/// nothing else, Back is faded and deaf on the first value, where it would
/// move nothing, and both are deaf while a value goes.
#[test]
fn each_arrow_asks_for_its_own_step_and_back_rests_on_the_first_value() {
    let surface = offscreen::start(WIDTH, HEIGHT);
    let palette = Palette::new().expect("the palette must build");
    palette.set_pack("whitespace".into());
    palette.set_counter("3 / 12".into());
    palette.set_last_sent_label("Last sent".into());
    palette.set_next_heading("Next: value 4 of 12".into());
    palette.set_next_name("Leading space".into());
    palette.set_copy_label("Copy".into());
    palette.set_skip_label("Skip".into());
    palette.set_has_next(true);
    palette.set_next_has_value(true);
    palette.set_can_go_back(true);
    let back = Rc::new(Cell::new(0));
    let skip = Rc::new(Cell::new(0));
    let copies = Rc::new(Cell::new(0));
    let count = Rc::clone(&back);
    palette.on_back_one_value(move || count.set(count.get() + 1));
    let count = Rc::clone(&skip);
    palette.on_skip_value(move || count.set(count.get() + 1));
    let count = Rc::clone(&copies);
    palette.on_copy_next(move || count.set(count.get() + 1));
    palette.show().expect("the palette must show");

    // Back is found by its word: it stands first after the gap, so its width
    // moves nothing beside it.
    palette.set_back_label("".into());
    let without = offscreen::draw(&surface, WIDTH, HEIGHT);
    palette.set_back_label("Back".into());
    let with = offscreen::draw(&surface, WIDTH, HEIGHT);
    let back_box = added(&without, &with, WIDTH, HEIGHT).expect("the word Back is drawn");
    assert!(
        back_box.bottom - back_box.top < 30 && back_box.left > WIDTH / 3,
        "Back is not one short word in the next band's row: {back_box:?}"
    );
    click(palette.window(), back_box.centre());
    assert_eq!(
        (back.get(), skip.get(), copies.get()),
        (1, 0, 0),
        "a click on Back"
    );

    // Skip moves Back when its word changes, so its box reaches over Back - and
    // its right edge, beside Copy, is its own.
    palette.set_skip_label("".into());
    let without = offscreen::draw(&surface, WIDTH, HEIGHT);
    palette.set_skip_label("Skip".into());
    let with = offscreen::draw(&surface, WIDTH, HEIGHT);
    let reach = added(&without, &with, WIDTH, HEIGHT).expect("the word Skip is drawn");
    let on_skip = LogicalPosition::new(
        (reach.right - 6) as f32,
        ((reach.top + reach.bottom) / 2) as f32,
    );
    click(palette.window(), on_skip);
    assert_eq!(
        (back.get(), skip.get(), copies.get()),
        (1, 1, 0),
        "a click on Skip"
    );

    // On the first value a step back moves nothing, so Back neither lights up
    // nor answers.
    palette.set_can_go_back(false);
    let rest = offscreen::draw(&surface, WIDTH, HEIGHT);
    point(palette.window(), back_box.centre());
    let hovered = offscreen::draw(&surface, WIDTH, HEIGHT);
    assert!(
        accent_in(&hovered, back_box) <= accent_in(&rest, back_box),
        "Back lit up under the pointer on the first value"
    );
    click(palette.window(), back_box.centre());
    assert_eq!(back.get(), 1, "Back answered on the first value");

    // While a value goes, neither walks.
    palette.set_can_go_back(true);
    palette.set_sending(true);
    click(palette.window(), back_box.centre());
    click(palette.window(), on_skip);
    assert_eq!(
        (back.get(), skip.get()),
        (1, 1),
        "an arrow answered while a value was going"
    );
}
