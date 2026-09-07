//! The compiled interface, and nothing else.
//!
//! # Why this file exists at all
//!
//! `slint::include_modules!()` pastes machine-generated Rust into whatever
//! module invokes it. That code is full of `unwrap`, `expect` and `panic`, which
//! this workspace denies - so clippy reported two dozen errors in a file nobody
//! wrote and nobody can edit.
//!
//! The tempting fix is to relax the lints for the whole package. That would also
//! relax them for every line we write by hand, which is the half that actually
//! needs them: a panic in the palette is a crash in front of a tester.
//!
//! So the allowance is scoped to exactly the generated module and no further.
//! Hand-written code in this crate still answers to the full set.

#[allow(
    clippy::all,
    clippy::pedantic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::missing_const_for_fn,
    missing_docs
)]
mod generated {
    slint::include_modules!();
}

pub use generated::*;
