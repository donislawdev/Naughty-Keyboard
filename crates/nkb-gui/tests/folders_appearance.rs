//! The pack folders window, rendered without a screen, and what its design
//! promises MEASURED rather than trusted (GUI rule 10).
//!
//! # What the design promises (`D175`)
//!
//! - it fits a small laptop screen: the drawn window stays under 720 logical
//!   pixels, the welcome window's measure, with a fault, a note and long words.
//! - nothing runs off the right edge: a folder's path has no space to wrap at,
//!   and the gutter at the right stays the ground of its row on every row - with
//!   a path far longer than its line.
//! - the main action, choosing the team folder, is the ONE thing on a ground of
//!   the accent (GUI rule 17).
//!
//! Filled with specimen words here: the window's own code fills it in
//! `folders_window.rs`. Its own binary for the reason `palette_appearance.rs`
//! gives: one platform per process.

#![allow(clippy::panic, clippy::expect_used)]

#[allow(dead_code)]
mod offscreen;

use nkb_gui::{FolderAction, FolderPlace, FoldersWindow};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

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

fn pixel(buffer: &[offscreen::Pixel], x: u32, y: u32) -> (u8, u8, u8) {
    let p = buffer[(y * WIDTH + x) as usize];
    (p.r, p.g, p.b)
}

fn action(text: &str, enabled: bool, primary: bool) -> FolderAction {
    FolderAction {
        text: text.into(),
        action: text.into(),
        enabled,
        primary,
    }
}

fn place(
    heading: &str,
    path: &str,
    state: &str,
    fault: bool,
    actions: Vec<FolderAction>,
) -> FolderPlace {
    FolderPlace {
        heading: heading.into(),
        path: path.into(),
        state: state.into(),
        fault,
        targeted: false,
        actions: ModelRc::new(VecModel::from(actions)),
    }
}

/// Rows where a run of at least `run` accent pixels stands side by side.
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

/// Rows on which something is drawn in the right gutter.
fn inked_gutter(picture: &[offscreen::Pixel], height: u32) -> Vec<u32> {
    (0..height)
        .filter(|&y| {
            let ground = pixel(picture, WIDTH - 1, y);
            (WIDTH - GUTTER / 2..WIDTH).any(|x| pixel(picture, x, y) != ground)
        })
        .collect()
}

#[test]
fn the_folders_window_keeps_its_promises() {
    let surface = offscreen::start(WIDTH, TALL);
    let window = FoldersWindow::new().expect("the folders window must build");
    window.set_heading("Pack folders".into());
    window.set_summary(
        "Packs come from three places: the 22 built into the program, the team folder and \
         your own folder. A pack in your own folder is used instead of one with the same name \
         in the team folder, and one in the team folder instead of a built-in pack."
            .into(),
    );
    window.set_team(place(
        "Team folder",
        r"\\a-server-with-a-long-name\a-share\testing\packs-for-every-project-of-the-whole-team",
        "The team folder did not answer within 2 seconds, so its packs are not offered.",
        true,
        vec![
            action("Choose folder", true, true),
            action("Open", false, false),
            action("Stop using", true, false),
        ],
    ));
    window.set_drop_hint(
        "You can also drop a folder on this window to use it as the team folder.".into(),
    );
    window.set_own(place(
        "Your own folder",
        r"C:\Users\tester\AppData\Roaming\Naughty Keyboard\packs",
        "Not made yet. It is made when you open it.",
        false,
        vec![action("Make and open", true, false)],
    ));
    window.set_close_label("Close".into());
    window.set_notes(ModelRc::new(VecModel::from(vec![SharedString::from(
        "The team folder is saved. Its packs are offered in the palette and in the value window.",
    )])));
    window.show().expect("shown");

    let picture = offscreen::draw(&surface, WIDTH, TALL);
    let path = offscreen::save(&picture, WIDTH, TALL, "folders.png");

    let height = (0..TALL)
        .rev()
        .find(|&y| pixel(&picture, 1, y) != SURFACE)
        .map_or(0, |y| y + 1);
    assert!(
        height > 0 && (height as f32) <= FITS,
        "the folders window is drawn {height} logical pixels tall, more than {FITS}. Look at {}",
        path.display()
    );
    let inked = inked_gutter(&picture, height);
    assert!(
        inked.is_empty(),
        "something is drawn in the right gutter on rows {inked:?}. Look at {}",
        path.display()
    );
    let grounds = accent_ground_rows(&picture, height, 24);
    assert!(
        grounds.len() >= 8,
        "only {} rows of an accent ground - the main action is not on the accent. Look at {}",
        grounds.len(),
        path.display()
    );
}
