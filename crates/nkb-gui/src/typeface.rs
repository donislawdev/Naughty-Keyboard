//! What the typeface this binary carries draws on its own - `D52`, point 1.
//!
//! The table is DATA about a file under `ui/fonts/`, which is why it lives in
//! the crate that ships that file. The rule that reads it is `nkb_core::typeface`,
//! which never learns where the table came from.
//!
//! # Why a table in the repository and not one built at compile time
//!
//! `D52` sketched a build script parsing the font on every build. `D66` chose a
//! generated file instead, for three reasons that are about the next decade
//! rather than about today:
//!
//! - a build script needs a font parser, and a parser needs an oracle to be
//!   trusted. The oracle is `skrifa`, the library the toolkit's own text layout
//!   asks - so the parser would be a second answer checked against the first,
//!   and the first one is enough.
//! - the table is visible in review. A font swapped for another one changes
//!   this file line by line, where a build script would change nothing anybody
//!   reads.
//! - it is the same shape as the grapheme table in `nkb-core` (`D65`), so one
//!   pattern covers both.
//!
//! What stops it drifting is `tests/typeface_guarantee.rs`, for every code point.

use nkb_core::typeface::TypefaceGuarantee;

mod table;

/// Everything DejaVu Sans Mono 2.37 draws by itself, on any machine.
///
/// Checked while compiling: an unsorted or overlapping table stops the build
/// here rather than answering wrongly for part of the standard at run time.
#[allow(
    clippy::panic,
    reason = "evaluated by the compiler. A malformed table is a build error, never a crash"
)]
pub static SHIPPED: TypefaceGuarantee = match TypefaceGuarantee::new(&table::RANGES) {
    Some(guarantee) => guarantee,
    None => panic!("the typeface table must be sorted, disjoint and inside the standard"),
};
