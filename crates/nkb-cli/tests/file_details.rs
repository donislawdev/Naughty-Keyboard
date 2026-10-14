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

/// The command line tool wears the palette's icon (the owner's choice,
/// 2026-10-14): it has no window, but its file is seen in the file manager, a
/// terminal's tab and the task manager, and the two files read as one product.
#[test]
fn nkb_wears_the_palettes_icon() {
    details::assert_wears_the_icon(env!("CARGO_BIN_EXE_nkb"));
}

/// The resource script adds the file's details and the icon, and nothing else,
/// and it names the code page before the details. The older resource compiler
/// a build may pick (CI's does) honours it only in the script it compiles, and
/// wrote the copyright sign as two letters (`crates/nkb-gui/assets/nkb-gui.rc`).
/// The compiler on a newer machine would not show that, so the line is pinned.
#[test]
fn the_resource_script_is_the_details_and_the_icon() {
    let dir = env!("CARGO_MANIFEST_DIR");
    let script = std::fs::read_to_string(format!("{dir}/assets/nkb.rc"))
        .expect("assets/nkb.rc is the script build.rs compiles");
    let statements: Vec<&str> = script
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("//"))
        .collect();
    assert_eq!(
        statements,
        [
            "#pragma code_page(65001)",
            "#include \"details.rc\"",
            "1 ICON \"../../nkb-gui/assets/edamame.ico\""
        ],
        "the resource script adds the file's details and the icon, and nothing else"
    );
    let build = std::fs::read_to_string(format!("{dir}/build.rs")).expect("build.rs is read");
    assert!(
        build.contains("\"assets/nkb.rc\""),
        "build.rs does not compile assets/nkb.rc"
    );
}

/// The guard for the icon's guard: this test's own file, which carries no
/// icon, is refused rather than passed by finding nothing to compare.
#[test]
#[should_panic(expected = "carries no icon")]
fn a_program_without_an_icon_is_refused() {
    let me = std::env::current_exe().expect("a test knows its own file");
    details::assert_wears_the_icon(&me.to_string_lossy());
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
