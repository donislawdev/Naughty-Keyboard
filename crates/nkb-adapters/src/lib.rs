//! Adapters: everything that knows about a file format or a disk.
//!
//! Each adapter knows everything on its own side and nothing on the other.
//! They implement the ports declared by `nkb-app`, which is why the dependency
//! points this way and not the other.

#![forbid(unsafe_code)]

pub mod fs;
pub mod skeleton;
pub mod toml_pack;

pub use fs::{DirectoryPackSink, DirectoryPackSource, SystemClock};
pub use toml_pack::TomlPackFormat;
