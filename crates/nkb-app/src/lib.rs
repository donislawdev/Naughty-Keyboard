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
pub mod emit_values;
pub mod format_pack;
pub mod lint_pack;
pub mod load_pack;
pub mod new_pack;
pub mod ports;
pub mod send_value;

pub use browse_packs::{Listing, PackEntry, ShowOutcome, list_packs, show_pack};
pub use emit_values::{Emission, EmitOutcome, EmittedValue, emit_values};
pub use format_pack::{FormatOutcome, format_pack};
pub use lint_pack::{LintOutcome, lint_pack};
pub use load_pack::{LoadedPack, Refused, load};
pub use new_pack::{NewPackOutcome, new_pack};
pub use ports::{
    Availability, CatalogueCoverage, CatalogueSource, Clock, Date, Delivered, DeliveryError,
    PackCatalogue, PackFormat, PackSink, PackSource, SinkError, SourceError, SourceSkipped,
    TargetRef, TranslationCheck, TranslationTarget, ValueDelivery,
};
pub use send_value::{SendOutcome, send_value};
