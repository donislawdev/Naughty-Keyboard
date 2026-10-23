//! The palette's own frame (`D145` point 38): the header moves the window and a button
//! at its end closes it, on a system that gives the window no frame of its own - MEASURED
//! on the rendered palette, with the pointer followed to the callbacks.
//!
//! # What this proves that reading `palette.slint` cannot
//!
//! A button that exists in the source can be drawn nowhere, or under another element, or
//! be out of the pointer's reach. So the button is FOUND by clicking: the right end of
//! the header is walked from its edge, and the click that asks to close is where the
//! button is. Without the frame the same walk finds nothing, which is its positive
//! control - a walk that found a close button in a palette that has none would prove
//! nothing about the one that has it.
//!
//! The header moves the window only where nothing else is: a press on the pack's name
//! belongs to the name, which opens the value window, and a press on the empty stretch
//! belongs to the drag. The drag is also off where the system draws the frame, because a
//! second handle on a title bar that moves the window already would move it twice.
//!
//! The hairline at the edge is found as a difference between the two renders at the
//! leftmost pixel of a row inside the header, where only the outline can differ.
//!
//! Its own binary for the reason `palette_appearance.rs` gives: one platform per process.

#![allow(clippy::panic, clippy::expect_used)]

#[allow(dead_code)]
mod offscreen;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use nkb_gui::Palette;
use offscreen::{click, point};
use slint::platform::{PointerEventButton, WindowEvent};
use slint::{ComponentHandle, LogicalPosition};

const WIDTH: u32 = 480;
const HEIGHT: u32 = 460;

/// How far from the right edge the header's last button is looked for: well past its
/// width and the gutter beside it, and short of the button before it.
const RIGHT_END: u32 = 56;

/// The rows of the header that are walked: it is one row of text, a few dozen pixels.
const HEADER_ROWS: [f32; 5] = [8.0, 14.0, 20.0, 26.0, 32.0];

/// The empty stretch of the header, with no links to stand in it.
const EMPTY_HEADER: LogicalPosition = LogicalPosition::new(240.0, 20.0);

/// What the header reports of a drag: where the pointer pressed and where it is now.
type Drag = (f32, f32, f32, f32);

struct Held {
    palette: Palette,
    closes: Rc<Cell<u32>>,
    toggles: Rc<Cell<u32>>,
    opens: Rc<Cell<u32>>,
    drags: Rc<RefCell<Vec<Drag>>>,
}

fn palette(own_frame: bool) -> Held {
    let palette = Palette::new().expect("the palette must build");
    palette.set_pack("whitespace".into());
    palette.set_counter("3 / 12".into());
    palette.set_collapse_label("Collapse".into());
    palette.set_expand_label("Expand".into());
    palette.set_no_value("Nothing sent yet.".into());
    palette.set_own_frame(own_frame);
    palette.set_close_label("Close the palette".into());
    let held = Held {
        palette,
        closes: Rc::default(),
        toggles: Rc::default(),
        opens: Rc::default(),
        drags: Rc::default(),
    };
    let count = Rc::clone(&held.closes);
    held.palette
        .on_close_palette(move || count.set(count.get() + 1));
    let count = Rc::clone(&held.toggles);
    held.palette
        .on_toggle_compact(move || count.set(count.get() + 1));
    let count = Rc::clone(&held.opens);
    held.palette
        .on_open_packs(move || count.set(count.get() + 1));
    let seen = Rc::clone(&held.drags);
    held.palette
        .on_drag_header(move |a, b, c, d| seen.borrow_mut().push((a, b, c, d)));
    held.palette.show().expect("the palette must show");
    held
}

/// Where a click asks to close the palette: the right end of the header, walked from its
/// edge, or `None` when no click there asks for it.
fn close_button(held: &Held) -> Option<(f32, f32)> {
    for x in (WIDTH - RIGHT_END..WIDTH).rev() {
        for y in HEADER_ROWS {
            let before = held.closes.get();
            click(held.palette.window(), LogicalPosition::new(x as f32, y));
            if held.closes.get() > before {
                return Some((x as f32, y));
            }
        }
    }
    None
}

/// Presses at `from`, moves to `to` and lets go.
fn drag(held: &Held, from: LogicalPosition, to: LogicalPosition) {
    let window = held.palette.window();
    let button = PointerEventButton::Left;
    point(window, from);
    window.dispatch_event(WindowEvent::PointerPressed {
        position: from,
        button,
    });
    point(window, to);
    window.dispatch_event(WindowEvent::PointerReleased {
        position: to,
        button,
    });
}

#[test]
fn the_header_of_a_palette_with_a_frame_of_its_own_moves_it_and_a_button_closes_it() {
    let surface = offscreen::start(WIDTH, HEIGHT);
    let own = palette(true);
    let drawn = offscreen::draw(&surface, WIDTH, HEIGHT);
    let path = offscreen::save_cropped(&drawn, WIDTH, HEIGHT, "palette-own-frame.png");

    // ---- the button that closes ---------------------------------------------
    close_button(&own).unwrap_or_else(|| {
        panic!(
            "no click at the right end of the header asks to close the palette. Look at {}",
            path.display()
        )
    });
    assert_eq!(
        (own.toggles.get(), own.opens.get()),
        (0, 0),
        "the click on the cross asked for something else as well"
    );

    // ---- the drag: the empty stretch only -----------------------------------
    drag(
        &own,
        EMPTY_HEADER,
        LogicalPosition::new(EMPTY_HEADER.x + 20.0, EMPTY_HEADER.y + 7.0),
    );
    let seen = own.drags.borrow().clone();
    assert!(
        !seen.is_empty(),
        "dragging the empty stretch of the header reports nothing"
    );
    let last = *seen.last().expect("one report at least");
    assert_eq!(
        last,
        (240.0, 20.0, 260.0, 27.0),
        "a drag reports where the press was and where the pointer is, in the header's own units"
    );
    own.drags.borrow_mut().clear();
    // The pack's name is its own: a press there opens the value window and moves nothing.
    drag(
        &own,
        LogicalPosition::new(24.0, EMPTY_HEADER.y),
        LogicalPosition::new(44.0, EMPTY_HEADER.y + 7.0),
    );
    assert!(
        own.drags.borrow().is_empty(),
        "a press on the pack's name started a drag: {:?}",
        own.drags.borrow()
    );
    assert_eq!(
        own.opens.get(),
        1,
        "the pack's name stopped opening the list"
    );
}

#[test]
fn a_palette_with_the_frame_of_the_system_has_neither_the_button_nor_the_drag() {
    let surface = offscreen::start(WIDTH, HEIGHT);
    let framed = palette(false);
    let _ = offscreen::draw(&surface, WIDTH, HEIGHT);

    assert_eq!(
        close_button(&framed),
        None,
        "a close button stands in a palette whose window has a frame already"
    );
    drag(
        &framed,
        EMPTY_HEADER,
        LogicalPosition::new(EMPTY_HEADER.x + 20.0, EMPTY_HEADER.y + 7.0),
    );
    assert!(
        framed.drags.borrow().is_empty(),
        "the header moved a window that the system moves: {:?}",
        framed.drags.borrow()
    );
}

#[test]
fn only_the_palette_with_a_frame_of_its_own_has_an_outline() {
    // Two renders in one process need two threads, as the platform is one per thread.
    let outlined = std::thread::spawn(|| edge_pixel(true))
        .join()
        .expect("the render of the outlined palette");
    let plain = std::thread::spawn(|| edge_pixel(false))
        .join()
        .expect("the render of the plain palette");
    assert_ne!(
        outlined, plain,
        "the edge of the palette looks the same with and without the outline"
    );
}

/// The colour of the leftmost pixel of a row inside the header, where the ground of the
/// header and the outline are all there is.
fn edge_pixel(own_frame: bool) -> (u8, u8, u8) {
    let surface = offscreen::start(WIDTH, HEIGHT);
    let _held = palette(own_frame);
    let drawn = offscreen::draw(&surface, WIDTH, HEIGHT);
    let pixel = drawn[(20 * WIDTH) as usize];
    (pixel.r, pixel.g, pixel.b)
}
