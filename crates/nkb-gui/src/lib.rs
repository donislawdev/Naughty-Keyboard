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
//!
//! The same holds for the shape ceilings. `clippy::all` and `clippy::pedantic`
//! cover three of them, and `cognitive_complexity` is named because it belongs
//! to neither group - measured 2026-10-08, the generated code gave it 2794
//! reports. The count of these names in the workspace is frozen in
//! crates/nkb-cli/tests/shape.rs, so a second allowance cannot appear quietly.

#[allow(
    clippy::all,
    clippy::pedantic,
    clippy::cognitive_complexity,
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

// The bridge between the shortcut thread and the window. It lives in the LIBRARY
// rather than beside `main.rs` for one reason: in the binary it would be
// unreachable from a test, and `apply` maps six fields onto six properties -
// a pair swapped there shows up as two labels trading places, which nothing else
// in this crate would notice.
pub mod live;

// Making the window refuse the keyboard focus. Beside `live` rather than inside
// it for one reason: this runs on the MAIN thread and `live` is the worker, and
// mixing the two in one file is how a session ends up calling a Slint method
// from the wrong side.
pub mod focus;

// What the shipped typeface draws by itself (`D52`, `D66`). Here rather than in
// the core because it is a fact about a file this crate carries, and the core
// must not learn where its tables come from.
pub mod typeface;
// The system clipboard, for the report block. Here rather than in nkb-adapters
// because the library behind it must never reach `nkb` - the module says why.
pub mod clipboard;
// The pack search's query, typed without a text field (`OBS-145`). No toolkit
// in it, so what a key press means is tested without a window (GUI rule 15).
pub mod query;
// Choosing a pack: the list, the query and the selected row, tested without a
// window. The pack window draws what this decides (step 7, K3.2).
pub mod picker;
// The shortcuts window's list: rows, recording and what a change came to,
// tested without a window like `picker` (step 7, K5.5). What a change IS is
// decided on the worker (`D90`).
pub mod shortcut_list;
// The shortcuts window itself, on the main thread - what `shortcut_list`
// decides moved onto the window, opened once the worker paused the palette's
// shortcuts and closed with them taken again (K5.5).
pub mod shortcuts;
// The chord held, read from the key events of the shortcuts window itself - how
// macOS records a shortcut, where the system cannot be asked (`D142`). No
// window in it, so the tracking is tested without one.
pub mod window_keys;
// The pack window itself, on the main thread: what `picker` decides moved onto
// the window, the keyboard taken on opening and handed back on closing. Beside
// `focus` for the reason `focus` gives - it is main-thread code.
pub mod packs;
// The welcome window's box for the first try: its text, caret and selection,
// tested without a window like `query` (UX7, `D105`).
pub mod trial;
// The welcome window itself, on the main thread: opened once at the first run,
// the keyboard taken for the box, closing remembered through the worker.
pub mod welcome;
// The About window, on the main thread: what the program is, which version,
// whose, under which licence - opened by the palette's link (the owner's
// point 4 of 2026-10-10).
pub mod about;
// How the three windows above that open on request stand on the screen and
// go away - never minimized, Close as the only button (`OBS-183`, `D137`).
// One place, because the three got it wrong the same way.
mod on_request;

// The links in the palette's header, and the two pages - the project on
// GitHub and the support page - that two of them open in the browser (the
// owner's points of 2026-10-14). The one place that hands an address to
// anything, and why: the module's own header.
pub mod header_links;
