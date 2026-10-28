//! Why a send stopped, as the clause that stands inside the sentences of a value
//! cut short (`Message::Interrupted`, `Message::InFlight`, `Message::NothingArrived`
//! and `Message::ClearedThenNothingArrived`, `D95`). One reason, one clause, on
//! every system, with the name of a key filled in where the reason has one.

use nkb_app::ports::StopReason;

use super::fill;

/// The pattern for one reason a send stopped. `ux-spec.md` 6, section A, byte
/// for byte. A clause, not a sentence: it stands after a colon, the way
/// `{detail}` does in `Message::ClipboardFailed`.
fn pattern_stop_reason(reason: StopReason) -> &'static str {
    match reason {
        StopReason::Dropped => {
            "the system did not deliver the keys - another program may be blocking input"
        }
        StopReason::FocusMoved => "another window came to the front",
        StopReason::NotTaking => "the application stopped taking keys",
        StopReason::Escape => "Escape was pressed",
        StopReason::KeyPressed { .. } => "{key} was pressed",
        StopReason::Halted => "the palette was closed",
        // `D159`. Wayland, where the input method says when the field loses the focus.
        StopReason::FieldLeft => "the keyboard focus left the field",
        StopReason::NoField => "no field that takes typed text had the keyboard focus",
        // `D160`. X11, where a key of the layout types what the group in effect gives it.
        StopReason::LayoutChanged => "the keyboard layout changed",
    }
}

/// Why a send stopped, ready to stand inside a sentence.
#[must_use]
pub fn stop_reason(reason: StopReason) -> String {
    let pattern = pattern_stop_reason(reason);
    match reason {
        // The key's name on the system the route runs on (`D149`).
        StopReason::KeyPressed { key } => fill(pattern, &[("key", key)]),
        StopReason::Dropped
        | StopReason::FocusMoved
        | StopReason::NotTaking
        | StopReason::Escape
        | StopReason::Halted
        | StopReason::FieldLeft
        | StopReason::NoField
        | StopReason::LayoutChanged => pattern.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_reason_a_send_stopped_reads_as_its_own_clause() {
        // `D95`, `D96`: the clause stands between a colon and the full stop its
        // sentence brings, so it carries no full stop of its own, and two
        // reasons that read the same would tell the tester nothing.
        let clauses: Vec<String> = [
            StopReason::Dropped,
            StopReason::FocusMoved,
            StopReason::NotTaking,
            StopReason::Escape,
            StopReason::KeyPressed { key: "Alt" },
            StopReason::Halted,
            StopReason::FieldLeft,
            StopReason::NoField,
            StopReason::LayoutChanged,
        ]
        .into_iter()
        .map(stop_reason)
        .collect();
        assert_eq!(clauses[4], "Alt was pressed", "the key is named, D149");
        assert_eq!(
            stop_reason(StopReason::KeyPressed { key: "Caps Lock" }),
            "Caps Lock was pressed",
            "a name of two words stands whole, D160"
        );
        for (i, clause) in clauses.iter().enumerate() {
            assert!(!clause.is_empty() && !clause.ends_with('.'), "{clause}");
            assert!(!clauses[i + 1..].contains(clause), "\"{clause}\" twice");
        }
    }
}
