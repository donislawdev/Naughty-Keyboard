//! `nkb-gui` links nothing that reaches off the machine, beyond what its
//! register names with a reason.
//!
//! The window is where most of the lock file goes: Slint and its backends
//! bring several hundred packages, and a new release of any of them could link
//! a network library without a line of ours changing. This reads the import
//! table of the `nkb-gui` this test run built and holds it to the register in
//! `nkb-cli/tests/imports/mod.rs`, shared with the command line tool so that
//! both programs answer to one list.

#![allow(clippy::panic, clippy::expect_used)]

#[path = "../../nkb-cli/tests/imports/mod.rs"]
mod imports;

#[test]
fn nkb_gui_links_nothing_that_reaches_off_the_machine() {
    imports::assert_links_nothing_off_the_machine(
        imports::Program::Gui,
        env!("CARGO_BIN_EXE_nkb-gui"),
    );
}
