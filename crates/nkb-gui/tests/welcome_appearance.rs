//! The welcome window, rendered without a screen, and what its design promises
//! MEASURED rather than trusted (GUI rule 10).
//!
//! # What the design promises (`ux-spec.md` 5.1, UX7, `D105`)
//!
//! - it fits a small laptop screen: the drawn window - down to the end of the
//!   footer band - stays under 720 logical pixels, the usable height of a
//!   1366 x 768 screen with a task bar, at the shipped width and with the
//!   product's longest sentences filled in. Measured on the picture, because
//!   the offscreen surface has the size the test gives it, not the window's.
//! - the main action is the ONE thing on a ground of the accent (GUI rule 17):
//!   accent pixels as a ground stand in the footer and nowhere above it, where
//!   the accent appears only as the box's edge and its caret. A ground, not a
//!   frame: the button's frame is in the accent too, and alone it makes two
//!   rows where the ground makes a dozen (`M553` passed before this was asked).
//! - the box draws the text after the caret: a box with text after the caret
//!   holds more ink than the same box with that text gone.
//!
//! Its own binary for the reason `palette_appearance.rs` gives: one platform
//! per process. The specimen words are the test's own, like the gallery's
//! labels - the product fills them from `nkb-adapters::i18n`.

#![allow(clippy::panic, clippy::expect_used)]

#[allow(dead_code)]
mod offscreen;

use nkb_gui::{HintRow, WelcomeWindow};
use slint::{ComponentHandle, ModelRc, VecModel};

const WIDTH: u32 = 480;
const TALL: u32 = 900;
/// The usable height of a 1366 x 768 screen with a task bar, in logical
/// pixels at 100 % - the smallest screen the tool is used on.
const FITS: f32 = 720.0;

/// Copied rather than read, for the reason the palette test gives.
const ACCENT: (u8, u8, u8) = (0x7A, 0xA2, 0xF7);
const SURFACE: (u8, u8, u8) = (0x14, 0x16, 0x1A);

fn fill(window: &WelcomeWindow) {
    window.set_window_title("Naughty Keyboard - welcome".into());
    window.set_heading("Welcome to Naughty Keyboard".into());
    window.set_try_heading("Try it here first".into());
    window.set_try_it("Click the box below and press Alt+Shift+N.".into());
    window.set_box_name("The box for the first try".into());
    window.set_box_before("Kowalski\u{2423}".into());
    window.set_box_after(String::new().into());
    window.set_box_facts("graphemes: 9, code points: 9, bytes: 9, UTF-16 units: 9".into());
    window.set_does_heading("What it does".into());
    window.set_does(
        "Naughty Keyboard types into the window in front, the way a keyboard does. It does not \
         read what the window shows - only the name of its program and the kind of control that \
         has the keyboard."
            .into(),
    );
    window.set_allowed("Use it only on systems you are allowed to test.".into());
    window.set_ready_heading("Ready".into());
    window.set_ready(
        "The palette starts on the pack Whitespace. Press Alt+Shift+Space to choose another pack \
         or value."
            .into(),
    );
    window.set_hints(ModelRc::new(VecModel::from(
        [
            ("Alt+Shift+N", "Next value"),
            ("Alt+Shift+P", "Previous value"),
            ("Alt+Shift+Space", "Find a value"),
        ]
        .iter()
        .map(|(key, action)| HintRow {
            key: (*key).into(),
            action: (*action).into(),
        })
        .collect::<Vec<_>>(),
    )));
    window.set_start_label("Start testing".into());
    window.set_start_action("Close the welcome and start testing".into());
}

fn pixel(buffer: &[offscreen::Pixel], x: u32, y: u32) -> (u8, u8, u8) {
    let p = buffer[(y * WIDTH + x) as usize];
    (p.r, p.g, p.b)
}

/// Rows where a run of at least `run` accent pixels stands side by side - a
/// GROUND of the accent, which an edge or a caret two pixels wide is not.
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
fn the_welcome_window_keeps_its_promises() {
    let surface = offscreen::start(WIDTH, TALL);
    let window = WelcomeWindow::new().expect("the welcome window must build");
    fill(&window);
    window.show().expect("the welcome window must show");

    let picture = offscreen::draw(&surface, WIDTH, TALL);
    let path = offscreen::save(&picture, WIDTH, TALL, "welcome.png");

    // ---- it fits a small laptop screen -------------------------------------
    // The footer band stands on its own ground, so the last row whose left edge
    // is not the plain surface is where the window ends.
    let height = (0..TALL)
        .rev()
        .find(|&y| pixel(&picture, 1, y) != SURFACE)
        .map_or(0, |y| y + 1);
    assert!(
        height > 0 && (height as f32) <= FITS,
        "the welcome window is drawn {height} logical pixels tall, more than {FITS}. Look at {}",
        path.display()
    );

    // ---- the main action is the one accent ground ---------------------------
    let grounds = accent_ground_rows(&picture, height, 24);
    // A frame in the accent makes two such rows, its top edge and its bottom
    // one - and the primary button HAS a frame in the accent. A ground makes
    // every row above and below the word (twelve, measured at the shipped
    // tokens), so fewer than eight is a frame with nothing inside it.
    assert!(
        grounds.len() >= 8,
        "only {} rows of an accent ground ({grounds:?}) - that is the frame of a button, not \
         its ground. Look at {}",
        grounds.len(),
        path.display()
    );
    let first = *grounds.first().unwrap_or_else(|| {
        panic!(
            "no ground of the accent anywhere - the main action does not stand out. Look at {}",
            path.display()
        )
    });
    assert!(
        first > height * 3 / 4,
        "an accent ground starts at y={first} of {height}, above the footer: the main action \
         is not the only one. Look at {}",
        path.display()
    );

    // ---- the box draws the text after the caret -----------------------------
    window.set_box_before("Kow".into());
    window.set_box_after("alski".into());
    let split = offscreen::draw(&surface, WIDTH, TALL);
    window.set_box_after(String::new().into());
    let cut = offscreen::draw(&surface, WIDTH, TALL);
    let differ = split
        .iter()
        .zip(cut.iter())
        .filter(|(a, b)| (a.r, a.g, a.b) != (b.r, b.g, b.b))
        .count();
    assert!(
        differ > 50,
        "the box draws nothing after the caret: {differ} pixels differ. Look at {}",
        offscreen::save(&split, WIDTH, TALL, "welcome-split.png").display()
    );
}
