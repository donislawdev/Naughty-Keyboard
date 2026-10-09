//! `nkb` links nothing that reaches off the machine: no network library, and
//! no call that starts a program or hands an address to the shell.
//!
//! The command line tool runs in other people's CI, next to their secrets. It
//! reads pack files and writes text, and nothing in it has a reason to open a
//! connection or start anything. This reads the import table of the `nkb` this
//! test run built and holds it to the register in `imports/mod.rs`, which is
//! where the reasons and the limits are written.

#![allow(clippy::panic, clippy::expect_used)]

mod imports;

#[test]
fn nkb_links_nothing_that_reaches_off_the_machine() {
    imports::assert_links_nothing_off_the_machine(imports::Program::Cli, env!("CARGO_BIN_EXE_nkb"));
}
