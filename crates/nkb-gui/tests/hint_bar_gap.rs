//! The hint bar keeps its key and its action apart: the gap between the longest
//! key and the action beside it, MEASURED on a render (GUI rule 10).
//!
//! Until 2026-10-10 the bar had one spacing for its rows and its columns, four
//! pixels, and on the palette's render `Alt+Shift+Space` stood four pixels
//! from `Find a value` - close enough to read as one word. The columns now
//! stand a step further apart (`Tokens.space-3`), and this holds it.
//!
//! Rendered in the shortcuts window, whose footer is a hint bar and nothing
//! else. The action's box is the ink its words add, and the key's right edge is the
//! last ink before it on the same rows.

#![allow(clippy::panic, clippy::expect_used)]

#[allow(dead_code)]
mod offscreen;

use nkb_gui::{HintRow, ShortcutsWindow};
use slint::{ComponentHandle, ModelRc, VecModel};

const WIDTH: u32 = 480;
const TALL: u32 = 700;
/// The narrowest gap that still reads as two columns: `Tokens.space-3` is 12,
/// less what the glyphs' own side bearings take.
const APART: u32 = 9;

fn hints(window: &ShortcutsWindow, action: &str) {
    window.set_hints(ModelRc::new(VecModel::from(vec![HintRow {
        key: "Alt+Shift+Space".into(),
        action: action.into(),
    }])));
}

#[test]
fn the_longest_key_stands_apart_from_its_action() {
    let surface = offscreen::start(WIDTH, TALL);
    let window = ShortcutsWindow::new().expect("the shortcuts window must build");
    window.set_heading("Shortcuts".into());
    window.show().expect("the shortcuts window must show");

    hints(&window, "");
    let bare = offscreen::draw(&surface, WIDTH, TALL);
    hints(&window, "Find a value");
    let full = offscreen::draw(&surface, WIDTH, TALL);
    let path = offscreen::save(&full, WIDTH, TALL, "hint-bar-gap.png");
    let action = offscreen::added(&bare, &full, WIDTH, TALL)
        .unwrap_or_else(|| panic!("the action is not drawn. Look at {}", path.display()));

    let at = |x: u32, y: u32| full[(y * WIDTH + x) as usize];
    let key_right = (0..action.left)
        .rev()
        .find(|&x| (action.top..=action.bottom).any(|y| at(x, y) != at(WIDTH - 1, y)))
        .unwrap_or_else(|| panic!("no key before the action. Look at {}", path.display()));
    let gap = action.left - key_right - 1;
    assert!(
        gap >= APART,
        "the key ends at x={key_right} and its action starts at x={}: {gap} pixels apart, \
         fewer than {APART}. Look at {}",
        action.left,
        path.display()
    );
}
