//! `nkb.exe` says what it is: the details a file manager's Properties and the
//! task manager read, held against the package manifest. Read from the file
//! this test run built, so a build script that stopped writing them, or wrote
//! the wrong ones, is red - the effect measured, not the script trusted.
//!
//! Windows only: the other systems have no such block in a program, and the
//! macOS bundle's words are the release's (`release.py`).

#![cfg(windows)]
#![allow(clippy::panic, clippy::expect_used)]

mod details;

#[test]
fn nkb_says_what_it_is_in_its_file() {
    let read = details::read(env!("CARGO_BIN_EXE_nkb"));
    details::assert_describes(
        &read,
        &details::Program {
            name: "nkb",
            description: "Naughty Keyboard command line tool",
        },
    );
}

/// The guard for the guard: a real program with no version block - this test
/// itself, which the build script leaves alone, because the resource goes to
/// the program and not to its tests - is refused rather than passed by
/// finding nothing.
#[test]
#[should_panic(expected = "has no version block")]
fn a_program_without_details_is_refused() {
    let me = std::env::current_exe().expect("a test knows its own file");
    details::read(&me.to_string_lossy());
}
