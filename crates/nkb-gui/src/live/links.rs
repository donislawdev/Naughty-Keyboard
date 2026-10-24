//! The links under the two switches: the way into the shortcuts window and, where
//! the desktop gives the shortcuts, the entry it wants before it will (`D145`
//! point 68). Kept apart from the loop so that `live.rs` stays under the ceiling
//! of `D113`. The public names are still `nkb_gui::live::<name>`.
//!
//! # A list, and the click is an index
//!
//! The palette used to have one link. It has one, two or three now, depending on
//! what the desktop said, so the view holds a list of words and hands Rust the place
//! of the one clicked - as the switches do for the way chosen. Which link stands at
//! a place is the worker's to say and is kept here, on the main thread, with the
//! words it drew: the two are set together, so a click can never mean a link that
//! is not on the screen.

use std::cell::RefCell;
use std::sync::mpsc::Sender;

use slint::{ModelRc, SharedString, VecModel};

use super::*;

/// What a link under the switches does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ShortcutLink {
    /// Opens the shortcuts window.
    Window,
    /// Makes the entry the desktop wants before it gives the program shortcuts.
    EnableEntry,
    /// Takes that entry away again.
    RemoveEntry,
}

impl ShortcutLink {
    /// The link's words, which are also what a click does.
    fn words(self) -> &'static str {
        i18n::label(match self {
            Self::Window => PaletteLabel::ShortcutsLink,
            Self::EnableEntry => PaletteLabel::EnableShortcuts,
            Self::RemoveEntry => PaletteLabel::RemoveEntry,
        })
    }
}

thread_local! {
    /// The links on the screen, in the order they are drawn, on the MAIN thread.
    static SHOWN: RefCell<Vec<ShortcutLink>> = const { RefCell::new(Vec::new()) };
}

/// Puts the links on the palette. On the MAIN thread.
pub(super) fn show_links(palette: &Palette, links: &[ShortcutLink]) {
    SHOWN.with_borrow_mut(|shown| {
        shown.clear();
        shown.extend_from_slice(links);
    });
    palette.set_shortcuts_links(ModelRc::new(VecModel::from(
        links
            .iter()
            .map(|link| SharedString::from(link.words()))
            .collect::<Vec<_>>(),
    )));
}

/// The link at `place` of the list on the screen, or `None` for a place nothing is at.
fn link_at(place: i32) -> Option<ShortcutLink> {
    SHOWN.with_borrow(|shown| {
        usize::try_from(place)
            .ok()
            .and_then(|at| shown.get(at).copied())
    })
}

/// The clicks on the links. A link that opens the window is the window's to open, on
/// this thread, and the other two only ask the worker, which holds the entry.
pub fn wire_shortcut_links(
    palette: &Palette,
    open_window: impl Fn() + 'static,
    asks: &Sender<Command>,
) {
    let asks = asks.clone();
    palette.on_open_shortcuts(move |place| {
        let ask = |command| {
            // A send that fails is a palette already closing - nobody is left to ask.
            let _ = asks.send(command);
        };
        match link_at(place) {
            Some(ShortcutLink::Window) => open_window(),
            Some(ShortcutLink::EnableEntry) => ask(Command::EnableEntry),
            Some(ShortcutLink::RemoveEntry) => ask(Command::RemoveEntry),
            None => {}
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every link has words of its own, and they are the words of the labels.
    #[test]
    fn each_link_says_its_own_words() {
        let words = [
            ShortcutLink::Window,
            ShortcutLink::EnableEntry,
            ShortcutLink::RemoveEntry,
        ]
        .map(ShortcutLink::words);
        assert_eq!(words[0], i18n::label(PaletteLabel::ShortcutsLink));
        assert_eq!(words[1], i18n::label(PaletteLabel::EnableShortcuts));
        assert_eq!(words[2], i18n::label(PaletteLabel::RemoveEntry));
        assert!(words[0] != words[1] && words[1] != words[2] && words[0] != words[2]);
    }

    /// A place nothing is at is no link, and a negative one is none either.
    #[test]
    fn a_place_with_no_link_is_no_link() {
        SHOWN.with_borrow_mut(|shown| *shown = vec![ShortcutLink::Window]);
        assert_eq!(link_at(0), Some(ShortcutLink::Window));
        assert_eq!(link_at(1), None);
        assert_eq!(link_at(-1), None);
    }
}
