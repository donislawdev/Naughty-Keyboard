//! A sample value for the tests of the palette's worker, kept apart from
//! `live.rs` so that the file stays under the ceiling of `D113`. Compiled for
//! tests only: `live.rs` declares the module under `cfg(test)`.

use nkb_app::UpcomingValue;
use nkb_core::identity::ValueKey;
use nkb_core::preview::ValuePreview;

/// The value after the one that went out, with its own name, reference and
/// preview, so the next band cannot borrow a line from the value band and
/// still pass.
pub(in crate::live) fn an_upcoming() -> UpcomingValue {
    UpcomingValue::Value {
        index: 8,
        total: 34,
        key: ValueKey {
            pack: "unicode-text".to_owned(),
            value: "trailing-nbsp".to_owned(),
        },
        name: "Trailing no-break space".to_owned(),
        reference: "unicode-text/trailing-nbsp".to_owned(),
        preview: ValuePreview::Text(nkb_core::preview::preview("Kowalski\u{A0}")),
        offensive: true,
        halves: Vec::new(),
        within: None,
    }
}
