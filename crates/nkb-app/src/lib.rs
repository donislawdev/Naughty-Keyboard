//! Use cases, and the ports they need.
//!
//! Nothing here knows where data came from or where it will go. A use case
//! takes its ports as parameters and returns a value - it does not print, does
//! not touch paths, and does not decide what a person will read.
//!
//! Ports are declared **here**, by the layer that needs them, and implemented
//! further out. That is the direction that keeps the arrow pointing one way.

#![forbid(unsafe_code)]

pub mod lint_pack;
pub mod ports;

pub use lint_pack::{LintOutcome, lint_pack};
pub use ports::{PackFormat, PackSource, SourceError, TranslationCheck, TranslationTarget};
