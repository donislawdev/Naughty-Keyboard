//! Every value the product ships can go through clipboard mode whole.
//!
//! # A tripwire, not a feature test
//!
//! Clipboard mode refuses a value holding `U+0000` by name, because the
//! clipboard ends its text there (`D71`). A refusal does not move the counter,
//! so the next press asks for the same value and is refused again: the sequence
//! stands still until the tester steps back (`OBS-130`). Nothing shipped today
//! holds such a value - measured with `nkb emit` over all nine packs on
//! 2026-09-25 - and the target catalogue holds two (`null-byte`, `nul-in-text`).
//!
//! This goes red on the day one of them ships, which is the day `OBS-130` has
//! to be decided: skip it, or say how to get past it. It pushes every shipped
//! value through the real `ClipboardDelivery` over a clipboard that only
//! records, so it proves the route, not a copy of its rule.

// A failed expectation in a test is a failed test.
#![allow(clippy::panic, clippy::expect_used)]

use std::cell::RefCell;

use nkb_adapters::{BuiltInCatalogue, ClipboardDelivery, TomlPackFormat};
use nkb_app::ports::{
    Clipboard, ClipboardError, History, PackCatalogue, PackFormat, PackSource, ValueDelivery,
};

/// Keeps what it was handed. Never the real clipboard: `cargo test` runs while
/// the owner works on this machine.
struct Recording(RefCell<Vec<String>>);

impl Clipboard for Recording {
    fn put_text(&self, text: &str, _history: History) -> Result<(), ClipboardError> {
        self.0.borrow_mut().push(text.to_owned());
        Ok(())
    }
}

#[test]
fn every_shipped_value_goes_through_clipboard_mode_whole() {
    let catalogue = BuiltInCatalogue::new();
    let ids = catalogue.list().expect("the built-in catalogue lists");
    let mut values = 0usize;
    for id in &ids {
        let text = catalogue.read(id).expect("a shipped pack reads");
        let pack = TomlPackFormat
            .parse(&text)
            .unwrap_or_else(|| panic!("shipped pack {id} parses"));
        for value in &pack.values {
            let literal = value
                .body
                .materialise()
                .unwrap_or_else(|e| panic!("{id}/{}: {e:?}", value.id));
            let clipboard = Recording(RefCell::new(Vec::new()));
            let delivered = ClipboardDelivery::new(&clipboard)
                .deliver(&literal)
                .unwrap_or_else(|refused| {
                    panic!(
                        "{id}/{}: clipboard mode refuses it ({refused}). The sequence would \
                         stand still on this value - decide OBS-130 before shipping it",
                        value.id
                    )
                });
            assert_eq!(
                *clipboard.0.borrow(),
                vec![literal.clone()],
                "{id}/{}: the clipboard got something other than the value",
                value.id
            );
            assert_eq!(delivered.utf16_units, literal.encode_utf16().count());
            values += 1;
        }
    }
    // Anti-vacuity: a catalogue that listed nothing would pass the loop above.
    assert_eq!(
        values, 102,
        "the shipped catalogue holds 102 values (CLAUDE.md, `shipped_packs.rs`) - a \
         different count means this walked something else"
    );
}
