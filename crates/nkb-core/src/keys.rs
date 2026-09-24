//! The keys the tool may press that are NOT content, and the one recipe built
//! from them.
//!
//! `ux-spec.md` 4 makes clearing the field the only place where the tool sends
//! keystrokes other than the value itself, and it spells out how: select within
//! the LINE, never "select all", because in a good many applications "select
//! all" reaches past the field - the whole page, the whole list, the whole
//! document - and the `Delete` that follows it lands on something the tester
//! never meant to touch. That promise ("clearing does not reach beyond the
//! field", untouchable rule 17) has no static guard in architektura.md 5, and
//! this module is the closest thing to one: the vocabulary below cannot SAY
//! "select all".
//!
//! # Why the type is this narrow
//!
//! [`Key`] has three variants and [`KeyChord`] has one modifier, `shift`. There
//! is no `Ctrl`, no `A`, no way to write `Ctrl+A` or `Ctrl+Home`. A future
//! session cannot slip the dangerous chord in without first widening a type
//! whose documentation says why it is narrow - which is the shape every
//! negative promise in this project takes: an absence, not a rule.
//!
//! # What the recipe does in each kind of field
//!
//! - single-line field: `Home`, `Shift+End`, `Delete` selects and removes the
//!   whole content.
//! - multi-line field: the same keys act on the CURRENT LINE, so the recipe
//!   under-clears. That is the safe direction, and `ux-spec.md` 4 says a
//!   multi-line field gets a question before any stronger clearing.
//! - wrapped editors treat `Home`/`End` as the visual line, which under-clears
//!   further. Still the safe direction.
//!
//! What this module does NOT know: whether a held modifier on the physical
//! keyboard would turn `Home` into `Ctrl+Home`. That is a property of the
//! machine at the moment of sending, and the system layer refuses to send
//! while a modifier is held - see `nkb-sys`.

/// A key the tool is allowed to press on its own account.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// Start of the line.
    Home,
    /// End of the line.
    End,
    /// Removes the selection.
    Delete,
}

impl Key {
    /// Every key this vocabulary has. Exists so a test can state the size of
    /// the vocabulary, since Rust has no reflection to count variants with.
    pub const ALL: [Key; 3] = [Key::Home, Key::End, Key::Delete];
}

/// One press: a key, optionally with `Shift` held.
///
/// `Shift` is the only modifier this type can express, because extending a
/// selection is the only thing the recipe needs a modifier for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyChord {
    pub key: Key,
    pub shift: bool,
}

impl KeyChord {
    pub const fn plain(key: Key) -> Self {
        Self { key, shift: false }
    }

    pub const fn shifted(key: Key) -> Self {
        Self { key, shift: true }
    }
}

/// The three presses that clear the current line of a field, in order.
///
/// `ux-spec.md` 4, answer 1: "move the caret to the start and to the end of
/// the field's content while selecting" - then remove the selection.
pub const fn line_clearing_recipe() -> [KeyChord; 3] {
    [
        KeyChord::plain(Key::Home),
        KeyChord::shifted(Key::End),
        KeyChord::plain(Key::Delete),
    ]
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;

    #[test]
    fn the_recipe_is_home_shift_end_delete_in_that_order() {
        // The order is the behaviour: `Delete` before the selection exists
        // would remove one character, and `Shift+End` before `Home` would
        // select nothing from a caret already at the end.
        assert_eq!(
            line_clearing_recipe(),
            [
                KeyChord {
                    key: Key::Home,
                    shift: false
                },
                KeyChord {
                    key: Key::End,
                    shift: true
                },
                KeyChord {
                    key: Key::Delete,
                    shift: false
                },
            ]
        );
    }

    #[test]
    fn only_the_selection_step_holds_shift() {
        let shifted: Vec<Key> = line_clearing_recipe()
            .iter()
            .filter(|chord| chord.shift)
            .map(|chord| chord.key)
            .collect();
        assert_eq!(
            shifted,
            vec![Key::End],
            "Shift extends the selection to the end of the line, nowhere else"
        );
    }

    #[test]
    fn the_vocabulary_has_exactly_three_keys_and_no_letter() {
        // A fourth variant is how "select all" would get in. Widening this
        // vocabulary is a decision, and this test is where it becomes visible.
        assert_eq!(Key::ALL.len(), 3);
        for key in Key::ALL {
            assert!(
                matches!(key, Key::Home | Key::End | Key::Delete),
                "unexpected key in the vocabulary: {key:?}"
            );
        }
    }

    #[test]
    fn the_recipe_uses_every_key_exactly_once() {
        // Three keys, three presses: nothing is pressed twice, nothing is
        // missing. A recipe that repeated `Delete` would remove a character
        // beyond the selection in a field that has no selection.
        let recipe = line_clearing_recipe();
        for key in Key::ALL {
            let presses = recipe.iter().filter(|chord| chord.key == key).count();
            assert_eq!(presses, 1, "{key:?} must be pressed exactly once");
        }
    }
}
