//! The About window, rendered without a screen with the product's own words and
//! facts, and what its design promises MEASURED rather than trusted (GUI
//! rule 10).
//!
//! # What the design promises (the owner's point 4 of 2026-10-10)
//!
//! - it fits a small laptop screen: the drawn window stays under 720 logical
//!   pixels, the welcome window's measure, with every sentence the product
//!   gives it.
//! - the main action is the ONE thing on a ground of the accent (GUI rule 17),
//!   and it stands in the footer.
//! - nothing runs off the right edge: an address has no space to wrap at, and
//!   the gutter at the right stays the ground of its row on every row - with
//!   the product's addresses, and with one far longer than its line.
//! - the Close button the tester sees closes the window: clicked where it is
//!   drawn, not invoked by name.
//!
//! Filled by the window's own code (`nkb_gui::about::About`) rather than with
//! specimen words, because what has to fit is what the product says - the
//! version and the addresses come from the package manifest. Its own binary for
//! the reason `palette_appearance.rs` gives: one platform per process.

#![allow(clippy::panic, clippy::expect_used)]

#[allow(dead_code)]
mod offscreen;

use nkb_gui::about::About;
use nkb_gui::packs::Keyboard;
use nkb_gui::{AboutWindow, Fact};
use slint::{ComponentHandle, ModelRc, VecModel};

const WIDTH: u32 = 480;
const TALL: u32 = 900;
/// The usable height of a 1366 x 768 screen with a task bar - the welcome
/// window's measure.
const FITS: f32 = 720.0;
/// The window's gutter, `Tokens.gutter` - copied for the reason the palette
/// test gives.
const GUTTER: u32 = 16;

/// Copied rather than read, for the reason the palette test gives.
const ACCENT: (u8, u8, u8) = (0x7A, 0xA2, 0xF7);
const SURFACE: (u8, u8, u8) = (0x14, 0x16, 0x1A);

/// A keyboard that is always there.
struct Quiet;

impl Keyboard for Quiet {
    fn remember(&self) {}
    fn take(&self, _window: Option<u64>) -> Result<(), String> {
        Ok(())
    }
    fn give_back(&self, _window: Option<u64>) -> Result<(), String> {
        Ok(())
    }
}

fn pixel(buffer: &[offscreen::Pixel], x: u32, y: u32) -> (u8, u8, u8) {
    let p = buffer[(y * WIDTH + x) as usize];
    (p.r, p.g, p.b)
}

/// Rows where a run of at least `run` accent pixels stands side by side - a
/// ground of the accent, which a frame or an edge is not.
fn accent_ground_rows(buffer: &[offscreen::Pixel], height: u32, run: u32) -> Vec<u32> {
    (0..height)
        .filter(|&y| {
            let mut longest = 0;
            let mut now = 0;
            for x in 0..WIDTH {
                if pixel(buffer, x, y) == ACCENT {
                    now += 1;
                    longest = longest.max(now);
                } else {
                    now = 0;
                }
            }
            longest >= run
        })
        .collect()
}

#[test]
fn the_about_window_keeps_its_promises() {
    let surface = offscreen::start(WIDTH, TALL);
    let about = About::new(
        AboutWindow::new().expect("the About window must build"),
        Box::new(Quiet),
        Box::new(|line| panic!("the window said {line:?}")),
    );
    about.open();

    let picture = offscreen::draw(&surface, WIDTH, TALL);
    let path = offscreen::save(&picture, WIDTH, TALL, "about.png");

    // ---- it fits a small laptop screen -------------------------------------
    let height = (0..TALL)
        .rev()
        .find(|&y| pixel(&picture, 1, y) != SURFACE)
        .map_or(0, |y| y + 1);
    assert!(
        height > 0 && (height as f32) <= FITS,
        "the About window is drawn {height} logical pixels tall, more than {FITS}. Look at {}",
        path.display()
    );

    // ---- the main action is the one accent ground ---------------------------
    let grounds = accent_ground_rows(&picture, height, 24);
    assert!(
        grounds.len() >= 8,
        "only {} rows of an accent ground ({grounds:?}) - the frame of a button, not its \
         ground. Look at {}",
        grounds.len(),
        path.display()
    );
    let first = grounds[0];
    assert!(
        first > height * 3 / 4,
        "an accent ground starts at y={first} of {height}, above the footer. Look at {}",
        path.display()
    );

    // ---- nothing runs off the right edge ------------------------------------
    let inked = inked_gutter(&picture, height);
    assert!(
        inked.is_empty(),
        "something is drawn in the right gutter on rows {inked:?}. Look at {}",
        path.display()
    );

    // ---- not even an address far longer than its line -----------------------
    // The product's addresses fit on one line today. A longer one - another
    // repository, a translation of the label beside it - must break inside
    // itself, where it allows, rather than run past the edge.
    let window = about.window();
    let shown = window.get_facts();
    window.set_facts(ModelRc::new(VecModel::from(vec![Fact {
        label: "Source code".into(),
        value: "github.com/an-organisation-with-a-long-name/a-repository-with-a-long-name".into(),
    }])));
    let long = offscreen::draw(&surface, WIDTH, TALL);
    let long_path = offscreen::save(&long, WIDTH, TALL, "about-long-address.png");
    let inked = inked_gutter(&long, TALL);
    assert!(
        inked.is_empty(),
        "a long address runs into the right gutter on rows {inked:?}. Look at {}",
        long_path.display()
    );
    window.set_facts(shown);

    // ---- the Close button the tester sees closes the window -----------------
    let middle = grounds[grounds.len() / 2];
    let across: Vec<u32> = (0..WIDTH)
        .filter(|&x| pixel(&picture, x, middle) == ACCENT)
        .collect();
    let (left, right) = (across[0], across[across.len() - 1]);
    offscreen::click(
        window.window(),
        slint::LogicalPosition::new(((left + right) / 2) as f32, middle as f32),
    );
    assert!(
        !about.is_open(),
        "a click on the Close button at x={left}..{right}, y={middle} left the window open. \
         Look at {}",
        path.display()
    );
}

/// Rows on which something is drawn in the right gutter. Within it every row
/// is one colour, its ground: a letter there is a word that did not wrap.
fn inked_gutter(picture: &[offscreen::Pixel], height: u32) -> Vec<u32> {
    (0..height)
        .filter(|&y| {
            let ground = pixel(picture, WIDTH - 1, y);
            (WIDTH - GUTTER / 2..WIDTH).any(|x| pixel(picture, x, y) != ground)
        })
        .collect()
}
