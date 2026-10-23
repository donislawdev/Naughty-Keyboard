//! What `nkb send` adds under the news that a value went in: how far the
//! application could be followed, and which text services of the system could
//! have changed what was typed. Out of `main.rs` under the ceiling of `D113`,
//! with the second note (`D139`).

use std::io::Write;

use nkb_app::Following;
use nkb_core::substitutions::Substitutions;

/// How far the value could be followed after it left, said plainly, because the
/// shorter sentence reads as a stronger claim than the tool can make. Followed by
/// the application (`D95`), its queue took every key - which is still not what the
/// field KEPT. Followed by the system only, a busy application may have lost some
/// (`OBS-158`). Handed over, no key was pressed: the desktop's input method has the
/// value, and not even whether a field had the focus is known (`D145`).
pub(crate) fn say_pace(err: &mut impl Write, following: Following) {
    let _ = match following {
        Following::Application => writeln!(
            err,
            "  the application took every keystroke - check the field to see what it kept"
        ),
        Following::System => writeln!(
            err,
            "  this application's pace could not be followed, so characters may be missing if it was busy - check the field"
        ),
        Following::HandedOver => writeln!(
            err,
            "  the value was handed to the desktop's input method, which gives it to whatever has the keyboard focus - check the field to see what it kept"
        ),
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

#[cfg(test)]
#[allow(
    clippy::panic,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;

    fn said(following: Following) -> String {
        let mut out = Vec::new();
        say_pace(&mut out, following);
        String::from_utf8(out).unwrap_or_else(|error| panic!("not UTF-8: {error}"))
    }

    #[test]
    fn each_way_a_value_can_be_followed_has_its_own_sentence() {
        let application = said(Following::Application);
        let system = said(Following::System);
        let handed = said(Following::HandedOver);
        assert!(
            application.contains("took every keystroke"),
            "{application}"
        );
        assert!(system.contains("pace could not be followed"), "{system}");
        assert!(
            handed.contains("handed to the desktop's input method"),
            "{handed}"
        );
        assert_ne!(application, system);
        assert_ne!(system, handed);
        assert_ne!(application, handed);
    }

    #[test]
    fn a_value_handed_over_is_never_said_to_have_reached_a_field() {
        // `D145`: the input method gives it to whatever has the focus, and not even
        // whether that was a field is known, so the sentence says neither "typed" nor
        // "took every keystroke", and sends the person to the field.
        let handed = said(Following::HandedOver);
        assert!(!handed.contains("keystroke"), "{handed}");
        assert!(!handed.contains("typed"), "{handed}");
        assert!(handed.contains("check the field"), "{handed}");
    }
}
