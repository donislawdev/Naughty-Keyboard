//! Adapters: everything that knows about a file format or a disk.
//!
//! Each adapter knows everything on its own side and nothing on the other.
//! They implement the ports declared by `nkb-app`, which is why the dependency
//! points this way and not the other.

#![forbid(unsafe_code)]

pub mod built_in;
pub mod canonical;
pub mod catalogue;
pub mod clipboard_delivery;
pub mod focus;
pub mod fs;
pub mod i18n;
pub mod keyboard;
pub(crate) mod read_pack;
pub mod report_text;
pub mod shortcuts;
pub mod skeleton;
pub mod startup;
pub mod toml_pack;

pub use built_in::BUILT_IN_PACKS;
pub use catalogue::BuiltInCatalogue;
pub use clipboard_delivery::ClipboardDelivery;
pub use focus::KeptFocus;
pub use fs::{DirectoryPackSink, DirectoryPackSource, SystemClock};
pub use keyboard::DirectInjection;
pub use report_text::EnglishReport;
pub use shortcuts::{GlobalShortcuts, default_bindings};
pub use startup::report_window_failure;
pub use toml_pack::TomlPackFormat;
