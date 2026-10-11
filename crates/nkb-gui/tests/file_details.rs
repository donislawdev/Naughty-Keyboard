//! `nkb-gui.exe` says what it is - and says it the way its About window does.
//!
//! The details a file manager's Properties and the task manager read, held
//! against the package manifest by the reader `nkb-cli` shares (see
//! `nkb-cli/tests/details/mod.rs`), and then against the window: the version,
//! the author and the copyright the window shows are the ones the file
//! carries. Two places that say one thing, built by two different paths - the
//! window's from the code, the file's from the build script - and this is the
//! one place they meet.
//!
//! Windows only, for the reason the `nkb` test gives.

#![cfg(windows)]
#![allow(clippy::panic, clippy::expect_used)]

#[path = "../../nkb-cli/tests/details/mod.rs"]
mod details;

use nkb_gui::about::{copyright, facts, shown_address};

#[test]
fn nkb_gui_says_what_it_is_in_its_file() {
    let read = details::read(env!("CARGO_BIN_EXE_nkb-gui"));
    details::assert_describes(
        &read,
        &details::Program {
            name: "nkb-gui",
            description: "Naughty Keyboard",
        },
    );
}

#[test]
fn the_about_window_says_what_the_file_says() {
    let read = details::read(env!("CARGO_BIN_EXE_nkb-gui"));
    let facts = facts();
    let value = |label: &str| -> &str {
        facts
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.as_str())
            .unwrap_or_else(|| panic!("the About window lists no {label}"))
    };
    assert_eq!(value("Version"), read.string("FileVersion"));
    assert_eq!(value("Author"), read.string("CompanyName"));
    assert_eq!(value("Website"), shown_address(read.string("Comments")));
    // The file's line goes on to name the licence, which the window lists as
    // a fact of its own.
    assert_eq!(
        read.string("LegalCopyright"),
        format!("{}. {}", copyright(), value("Licence"))
    );
}
