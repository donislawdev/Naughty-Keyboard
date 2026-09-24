//! Generated from the Unicode Character Database. Do not edit by hand.
//!
//! Source file, its exact bytes and the reason it is vendored:
//! `crates/nkb-core/unicode/README.md`. Unicode 17.0.0.
//!
//! Every code point with `Default_Ignorable_Code_Point` in
//! `DerivedCoreProperties.txt`, as inclusive ranges in code point order.
//!
//! What keeps this honest is `tests/unicode_data.rs`, which rebuilds the table
//! from that file and compares all 1 114 112 code points. It also writes the
//! source the file implies to `target/tmp/unicode/ignorable_table.rs` on every
//! run, which is how this file was made and how it is remade for the next
//! version of the standard.

pub(super) static DEFAULT_IGNORABLE: [(u32, u32); 17] = [
    (0x00AD, 0x00AD),
    (0x034F, 0x034F),
    (0x061C, 0x061C),
    (0x115F, 0x1160),
    (0x17B4, 0x17B5),
    (0x180B, 0x180F),
    (0x200B, 0x200F),
    (0x202A, 0x202E),
    (0x2060, 0x206F),
    (0x3164, 0x3164),
    (0xFE00, 0xFE0F),
    (0xFEFF, 0xFEFF),
    (0xFFA0, 0xFFA0),
    (0xFFF0, 0xFFF8),
    (0x1BCA0, 0x1BCA3),
    (0x1D173, 0x1D17A),
    (0xE0000, 0xE0FFF),
];
