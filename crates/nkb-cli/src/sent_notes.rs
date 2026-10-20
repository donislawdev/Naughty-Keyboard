//! What `nkb send` adds under the news that a value went in: how far the
//! application could be followed, and which text services of the system could
//! have changed what was typed. Out of `main.rs` under the ceiling of `D113`,
//! with the second note (`D139`).

use std::io::Write;

use nkb_core::substitutions::Substitutions;

/// Whether the application took every key, said plainly, because the shorter
/// sentence reads as a stronger claim than the tool can make. Paced (`D95`), the
/// application's queue took every key - which is still not what the field KEPT.
/// Unpaced, only the system took them, and a busy application may have lost some
/// (`OBS-158`).
pub(crate) fn say_pace(err: &mut impl Write, paced: bool) {
    let _ = if paced {
        writeln!(
            err,
            "  the application took every keystroke - check the field to see what it kept"
        )
    } else {
        writeln!(
            err,
            "  this application's pace could not be followed, so characters may be missing if it was busy - check the field"
        )
    };
}

/// The text services that are on and could have acted on this value (`D139`):
/// said only when there is one, naming it in words the person finds in the
/// keyboard settings.
pub(crate) fn say_text_substitutions(err: &mut impl Write, may_alter: Substitutions) {
    if may_alter.is_empty() {
        return;
    }
    let _ = writeln!(
        err,
        "  the system's text substitutions may have changed what went in: {} - to type exactly what is shown, switch them off in System Settings, Keyboard, Text Input, Edit",
        nkb_adapters::i18n::substitution_names(may_alter)
    );
}
