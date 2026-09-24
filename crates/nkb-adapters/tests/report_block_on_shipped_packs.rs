//! The report block, built for every value the product ships.
//!
//! # Why the shipped packs and not a pattern
//!
//! C4b-4 learned it the hard way: the palette's appearance test used a pattern
//! value with `7 graphemes`, and never tried `len-100000` - where the counters
//! ran off the window. So this renders the block for EVERY shipped value, through
//! the real format adapter and the real catalogue, and checks what must hold for
//! all of them.
//!
//! # The two surfaces must not disagree
//!
//! A tester reads the size in the palette and pastes the block into a ticket. If
//! the two numbers ever differed, the ticket would contradict what the tester
//! saw. So the block's counts are compared with the ones `deliver_value` hands
//! the palette, value by value.
//!
//! The rendered blocks are written to `target/tmp/report/<pack>.txt`, to be read
//! by a person: a test proves the invariants, not that the block reads well.

// A failed expectation in a test is a failed test.
#![allow(clippy::panic, clippy::expect_used)]

use std::path::Path;

use nkb_adapters::report_text::report_text;
use nkb_adapters::{BuiltInCatalogue, TomlPackFormat};
use nkb_app::ports::{
    Availability, Delivered, DeliveryError, KeystrokeError, KeystrokeSender, PackCatalogue,
    PackFormat, PackSource, TargetRef, ValueDelivery,
};
use nkb_app::{Clearing, SendOutcome, ValueFacts, deliver_value};
use nkb_core::keys::KeyChord;
use nkb_core::pack::Pack;
use nkb_core::report::{Arrival, ReportBlock};

/// A field that takes everything, so `deliver_value` reports what it measured.
struct TakesEverything;

impl ValueDelivery for TakesEverything {
    fn availability(&self) -> Availability {
        Availability::Ready
    }
    fn target(&self) -> Option<TargetRef> {
        Some(TargetRef(1))
    }
    fn deliver(&self, text: &str) -> Result<Delivered, DeliveryError> {
        Ok(Delivered {
            utf16_units: text.encode_utf16().count(),
        })
    }
}

struct NoKeys;

impl KeystrokeSender for NoKeys {
    fn send_keystrokes(&self, _chords: &[KeyChord]) -> Result<(), KeystrokeError> {
        Ok(())
    }
}

/// The shipped values whose `Typed:` line a normalizing tracker could rewrite.
///
/// 🔴 Pinned from outside this code. On 2026-09-23 Python's `unicodedata`
/// (Unicode 16.0) normalized the `Typed:` text of every shipped value to NFC
/// and to NFKC, and these four came back different - these four and no other.
/// A different implementation, so a mistake in `nkb_core::normalization` cannot
/// stand on both sides of the comparison (`D70`).
const REWRITTEN_BY_NORMALIZATION: [&str; 4] = [
    "unicode-text/combining-acute",
    "unicode-text/zalgo",
    "unicode-text/fullwidth-latin",
    "unicode-text/math-bold",
];

fn shipped_packs() -> Vec<Pack> {
    let catalogue = BuiltInCatalogue::new();
    let ids = catalogue.list().expect("the built-in catalogue lists");
    assert!(!ids.is_empty(), "the product ships packs");
    ids.iter()
        .map(|id| {
            let text = catalogue.read(id).expect("a shipped pack reads");
            TomlPackFormat
                .parse(&text)
                .unwrap_or_else(|| panic!("shipped pack {id} parses"))
        })
        .collect()
}

#[test]
fn every_shipped_value_has_a_block_that_agrees_with_the_palette() {
    let out = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("target")
        .join("tmp")
        .join("report");
    std::fs::create_dir_all(&out).expect("target/tmp/report can be created");

    let mut values = 0usize;
    let mut spelled_out = 0usize;
    for pack in shipped_packs() {
        let mut rendered = Vec::new();
        for value in &pack.values {
            let block = ReportBlock::describe(&pack, value, Arrival::Whole)
                .unwrap_or_else(|e| panic!("{}/{}: {e:?}", pack.id, value.id));

            let SendOutcome::Sent {
                facts:
                    ValueFacts {
                        graphemes,
                        code_points,
                        bytes,
                        shape,
                        reference,
                        ..
                    },
                ..
            } = deliver_value(&pack, value, &TakesEverything, &NoKeys, Clearing::Keep, 0)
            else {
                panic!(
                    "{}/{}: a field that takes everything took it",
                    pack.id, value.id
                );
            };
            assert_eq!(block.reference, reference);
            assert_eq!(
                (block.graphemes, block.code_points, block.bytes),
                (graphemes, code_points, bytes),
                "{reference}: the block and the palette count differently"
            );
            assert_eq!(block.shape, shape, "{reference}: a different shape line");

            let text = report_text(&block);
            let lines: Vec<&str> = text.lines().collect();
            if REWRITTEN_BY_NORMALIZATION.contains(&reference.as_str()) {
                spelled_out += 1;
                assert_eq!(
                    lines.len(),
                    11,
                    "{reference}: normalization rewrites its Typed: line, so it is \
                     spelled out as well\n{text}"
                );
                assert!(
                    lines[3].starts_with("Unicode:  U+"),
                    "{reference}: {}",
                    lines[3]
                );
                assert_eq!(
                    lines[3].matches("U+").count(),
                    code_points,
                    "{reference}: the Unicode: line lists every code point Size: counts"
                );
            } else {
                assert_eq!(
                    lines.len(),
                    10,
                    "{reference}: nothing in its Typed: line is rewritten, so ten \
                     lines\n{text}"
                );
            }
            assert!(
                lines[0].starts_with(&format!("Value:    {reference} @ pack {}", pack.version)),
                "{reference}: {}",
                lines[0]
            );
            assert!(
                !text
                    .chars()
                    .any(|c| c.is_control() && c != '\n' && c != '\r'),
                "{reference}: a control character reached the block"
            );
            rendered.push(text);
            values += 1;
        }
        std::fs::write(
            out.join(format!("{}.txt", pack.id)),
            rendered.join("\n\n----\n\n"),
        )
        .expect("the rendered blocks are written");
    }
    assert!(values > 0, "no value was checked - the loop saw nothing");
    assert_eq!(
        spelled_out,
        REWRITTEN_BY_NORMALIZATION.len(),
        "a pinned reference no longer names a shipped value - the list went stale"
    );
}
