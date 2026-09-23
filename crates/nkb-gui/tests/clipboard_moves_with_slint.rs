//! The clipboard library is the one Slint already carries - one copy, not two.
//!
//! The workspace manifest says `arboard` adds no crate to the product, because
//! Slint's winit backend links it for its own text fields. That sentence stays
//! true only while both ask for the SAME version. When Slint moves to a release
//! outside our caret, Cargo keeps both, the palette ships two clipboard
//! libraries, and nothing else would say so.

// A failed expectation in a test is a failed test.
#![allow(clippy::panic)]

use std::path::Path;

#[test]
fn the_lock_file_holds_one_clipboard_library() {
    let lock_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("Cargo.lock");
    let lock = std::fs::read_to_string(&lock_path)
        .unwrap_or_else(|e| panic!("{}: {e}", lock_path.display()));
    let versions = lock.matches("\nname = \"arboard\"\n").count();
    assert_eq!(
        versions, 1,
        "Cargo.lock holds {versions} versions of arboard. The workspace declaration \
         has to move together with the one Slint locks."
    );
}
