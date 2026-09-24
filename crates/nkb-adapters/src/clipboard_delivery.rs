//! Delivering a value by putting it on the clipboard, for the tester to paste.
//!
//! The second implementation of `ValueDelivery`, beside `DirectInjection`
//! (`architektura.md` 3, `D71`). The sequence picks one of the two by its
//! delivery axis, in one place, and everything past that choice is shared - so
//! this file holds only what is true of the clipboard ROUTE and nothing about
//! the clipboard library, which stays in the one adapter that `nkb-gui` owns.
//!
//! # It presses nothing
//!
//! Not even the paste shortcut. Every reason that leads to clipboard mode - no
//! direct route on the system, a window that input does not reach, an
//! application that ignores synthetic input - also blocks the channel a
//! synthetic paste would travel. So the tester presses paste, and
//! `product-spec.md` 9.2 counts that as the whole price of the mode: two
//! presses instead of one. An automatic paste is a different route, chosen by
//! the tester when it exists, with its own door.
//!
//! # The one character it refuses
//!
//! `U+0000`. Windows documents that a NUL ends the data of `CF_UNICODETEXT`, so
//! a paste would stop there without a sign - the silent truncation this tool is
//! built to find in other software. On macOS and Linux it is unmeasured, and a
//! receiver written in C would cut it anyway. Refused by name, on every system,
//! rather than delivered short (`D71`, `OBS-130`).
//!
//! # Out of the history
//!
//! Values go on the clipboard with [`History::Skip`]: walking a pack would
//! otherwise push the tester's own items out of `Win+V`, one value at a time.
//! The report block takes the other door and stays in the history on purpose.

use nkb_app::ports::{
    Availability, Clipboard, ClipboardError, Delivered, DeliveryError, History, TargetRef,
    ValueDelivery,
};

/// What [`ClipboardDelivery::target`] answers: the clipboard, which is always
/// there. Zero, which no window handle is, because the value does not go to the
/// focused window at all - the tester carries it there.
const THE_CLIPBOARD: TargetRef = TargetRef(0);

/// Puts each value on the system clipboard, replacing what was there, and
/// presses nothing.
///
/// Holds a reference to the clipboard port rather than a clipboard of its own:
/// the report block writes through the same one, and on Linux the process that
/// set the clipboard is the one serving it, so one handle for the palette's
/// lifetime is the right number.
pub struct ClipboardDelivery<'a> {
    clipboard: &'a dyn Clipboard,
}

impl<'a> ClipboardDelivery<'a> {
    #[must_use]
    pub fn new(clipboard: &'a dyn Clipboard) -> Self {
        Self { clipboard }
    }
}

/// The first character this route cannot carry whole, if the text has one.
fn first_uncarried(text: &str) -> Option<char> {
    text.chars().find(|character| *character == '\0')
}

impl ValueDelivery for ClipboardDelivery<'_> {
    fn availability(&self) -> Availability {
        // Whether the clipboard takes the text is known only by trying - on
        // Linux it is a connection to a display server that may have gone - and
        // `deliver` names the refusal when there is one.
        Availability::Ready
    }

    fn target(&self) -> Option<TargetRef> {
        Some(THE_CLIPBOARD)
    }

    fn deliver(&self, text: &str) -> Result<Delivered, DeliveryError> {
        // Checked BEFORE the clipboard is touched: a refused value must leave
        // the tester's clipboard exactly as it was.
        if let Some(character) = first_uncarried(text) {
            return Err(DeliveryError::CannotCarry { character });
        }
        match self.clipboard.put_text(text, History::Skip) {
            Ok(()) => Ok(Delivered {
                utf16_units: text.encode_utf16().count(),
            }),
            Err(ClipboardError::Busy) => Err(DeliveryError::Busy),
            Err(ClipboardError::Failed { detail }) => Err(DeliveryError::Refused { detail }),
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    // 🔴 The real clipboard is never touched here: `cargo test` runs while the
    // owner works on this machine. What the real one does is measured by
    // `tools/petla-palety.ps1`, run with a warning first.

    /// Keeps what it was handed and how, or refuses as told.
    struct Recording {
        refuse: Option<ClipboardError>,
        puts: RefCell<Vec<(String, History)>>,
    }

    impl Recording {
        fn working() -> Self {
            Self {
                refuse: None,
                puts: RefCell::new(Vec::new()),
            }
        }
        fn refusing(error: ClipboardError) -> Self {
            Self {
                refuse: Some(error),
                puts: RefCell::new(Vec::new()),
            }
        }
    }

    impl Clipboard for Recording {
        fn put_text(&self, text: &str, history: History) -> Result<(), ClipboardError> {
            match &self.refuse {
                Some(error) => Err(error.clone()),
                None => {
                    self.puts.borrow_mut().push((text.to_owned(), history));
                    Ok(())
                }
            }
        }
    }

    #[test]
    fn a_value_goes_on_the_clipboard_whole_and_out_of_the_history() {
        let clipboard = Recording::working();
        let delivered = ClipboardDelivery::new(&clipboard)
            .deliver("ab\u{200B}cd \u{1F600}")
            .expect("the clipboard took it");
        assert_eq!(
            *clipboard.puts.borrow(),
            vec![("ab\u{200B}cd \u{1F600}".to_owned(), History::Skip)]
        );
        assert_eq!(
            delivered.utf16_units, 8,
            "the emoji crosses as a surrogate pair, so eight units, not seven"
        );
    }

    #[test]
    fn a_nul_is_refused_by_name_and_the_clipboard_is_left_alone() {
        let clipboard = Recording::working();
        assert_eq!(
            ClipboardDelivery::new(&clipboard).deliver("before\0after"),
            Err(DeliveryError::CannotCarry { character: '\0' })
        );
        assert!(
            clipboard.puts.borrow().is_empty(),
            "a refused value must not replace what the tester had copied"
        );
    }

    #[test]
    fn the_empty_value_is_put_as_it_is() {
        // `len-0` is a legal test value. What an application pastes for an
        // empty clipboard text is unmeasured (`D71`). This route does not guess.
        let clipboard = Recording::working();
        let delivered = ClipboardDelivery::new(&clipboard)
            .deliver("")
            .expect("an empty value is not refused");
        assert_eq!(delivered.utf16_units, 0);
        assert_eq!(clipboard.puts.borrow().len(), 1);
    }

    #[test]
    fn a_held_clipboard_is_passing_and_a_refusal_keeps_its_words() {
        let busy = Recording::refusing(ClipboardError::Busy);
        assert_eq!(
            ClipboardDelivery::new(&busy).deliver("x"),
            Err(DeliveryError::Busy)
        );
        let failed = Recording::refusing(ClipboardError::Failed {
            detail: "no display".to_owned(),
        });
        assert_eq!(
            ClipboardDelivery::new(&failed).deliver("x"),
            Err(DeliveryError::Refused {
                detail: "no display".to_owned()
            })
        );
    }

    #[test]
    fn the_route_is_always_there_and_is_not_a_window() {
        let clipboard = Recording::working();
        let route = ClipboardDelivery::new(&clipboard);
        assert_eq!(route.availability(), Availability::Ready);
        assert_eq!(route.target(), Some(TargetRef(0)));
    }
}
