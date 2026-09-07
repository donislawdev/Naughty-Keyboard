//! Use cases, and the ports they need.
//!
//! Nothing here knows where data came from or where it will go. A use case
//! takes its ports as parameters and returns a value - it does not print, does
//! not touch paths, and does not decide what a person will read.
//!
//! Ports are declared **here**, by the layer that needs them, and implemented
//! further out. That is the direction that keeps the arrow pointing one way.

#![forbid(unsafe_code)]

pub mod browse_packs;
pub mod format_pack;
pub mod lint_pack;
pub mod new_pack;
pub mod ports;

pub use browse_packs::{Listing, PackEntry, ShowOutcome, list_packs, show_pack};
pub use format_pack::{FormatOutcome, format_pack};
pub use lint_pack::{LintOutcome, lint_pack};
pub use new_pack::{NewPackOutcome, new_pack};
pub use ports::{
    CatalogueCoverage, CatalogueSource, Clock, Date, PackCatalogue, PackFormat, PackSink,
    PackSource, SinkError, SourceError, SourceSkipped, TranslationCheck, TranslationTarget,
};
