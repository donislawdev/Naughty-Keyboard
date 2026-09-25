//! Use cases, and the ports they need.
//!
//! Nothing here knows where data came from or where it will go. A use case
//! takes its ports as parameters and returns a value - it does not print, does
//! not touch paths, and does not decide what a person will read.
//!
//! Ports are declared **here**, by the layer that needs them, and implemented
//! further out. That is the direction that keeps the arrow pointing one way.

#![forbid(unsafe_code)]

pub mod advance_sequence;
pub mod browse_packs;
pub mod drive_sequence;
pub mod emit_values;
pub mod format_pack;
pub mod keep_settings;
pub mod lint_pack;
pub mod load_pack;
pub mod new_pack;
pub mod ports;
pub mod send_value;
#[cfg(test)]
pub(crate) mod test_support;

pub use advance_sequence::{AdvanceSequence, ChooseError, Message, Outcome, RouteRequest, Sent};
pub use browse_packs::{Listing, PackEntry, ShowOutcome, list_packs, show_pack};
pub use drive_sequence::{Ended, drive_sequence};
pub use emit_values::{Emission, EmitOutcome, EmittedValue, emit_values};
pub use format_pack::{FormatOutcome, format_pack};
pub use keep_settings::{KeptSettings, Opened, Opening, SettingsMessage};
pub use lint_pack::{LintOutcome, lint_pack};
pub use load_pack::{LoadedPack, Refused, load};
pub use new_pack::{NewPackOutcome, new_pack};
pub use ports::{
    Availability, CatalogueCoverage, CatalogueSource, Clock, Date, Delivered, DeliveryError,
    HotkeyRegistrar, KeystrokeError, KeystrokeSender, LiveShortcuts, PackCatalogue, PackFormat,
    PackSink, PackSource, SaveError, SettingChange, Settings, SettingsLoad, SettingsNote,
    SettingsStore, SettingsUnusable, ShortcutRegistration, ShortcutsUnavailable, SinkError,
    SourceError, SourceSkipped, TargetRef, TranslationCheck, TranslationTarget, ValueDelivery,
    Wait,
};
pub use send_value::{
    Clearing, ClearingOutcome, SendOutcome, SendRequest, SkipReason, ValueFacts, deliver_value,
    send_value,
};
