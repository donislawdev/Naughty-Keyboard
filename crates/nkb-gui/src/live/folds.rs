//! What the palette folds and remembers between runs: the compact switch, which
//! the worker owns, and the list of shortcuts at its foot, which the window
//! owns and the worker only saves. Kept apart from the loop so that `live.rs`
//! stays under the ceiling of `D113`. The public names are still
//! `nkb_gui::live::<name>`.

use std::sync::mpsc::Sender;

use slint::ComponentHandle;

use super::*;

/// The compact switch and the memory of it, both owned by the worker.
///
/// # Why the worker and not the window
///
/// Until `D84` the main thread flipped the window's own property. With the
/// state remembered, two owners would mean two answers to "is it collapsed":
/// the one drawn and the one saved. So the worker holds the switch from the
/// start, tells the window what to draw, and saves the same value - on this
/// thread, where a slow settings folder costs a press nothing.
pub(super) struct Collapse<'a> {
    compact: Cell<bool>,
    memory: &'a Memory<'a>,
}

impl<'a> Collapse<'a> {
    pub(super) fn new(memory: &'a Memory<'a>) -> Self {
        Self {
            compact: Cell::new(starts_compact(memory.kept.borrow().settings())),
            memory,
        }
    }

    /// Flips the switch and remembers it. The new state, and a line to say
    /// when saving failed for a reason not said before.
    pub(super) fn toggle(&self) -> (bool, Option<String>) {
        let compact = !self.compact.get();
        self.compact.set(compact);
        (compact, self.memory.keep(SettingChange::Compact(compact)))
    }
}

/// Draws what a collapse came to - the press and the click alike - and says
/// the line about saving it, when there is one.
pub(super) fn draw_toggle(palette: &Weak<Palette>, (compact, line): (bool, Option<String>)) {
    set_compact_later(palette, compact);
    if let Some(line) = line {
        say_later(palette, line);
    }
}

/// Hands the compact state to the thread that owns the window.
fn set_compact_later(palette: &Weak<Palette>, compact: bool) {
    // Dropped for the reason `show` gives.
    let _ = palette.upgrade_in_event_loop(move |palette| set_compact(&palette, compact));
}

/// Whether the list of shortcuts starts open: as the tester last left it, and
/// open on a first run, when the list is needed most (`D123`).
#[must_use]
pub fn starts_with_shortcut_list(settings: &Settings) -> bool {
    settings.shortcut_list.unwrap_or(true)
}

/// The heading of the list of shortcuts - its words and the state the list
/// starts in (`D123`). On the MAIN thread, before the window is shown.
pub fn label_shortcut_list(palette: &Palette, settings: &Settings) {
    palette.set_hints_label(i18n::label(PaletteLabel::ShortcutList).into());
    palette.set_hints_action(i18n::label(PaletteLabel::ShortcutListAction).into());
    palette.set_hints_open(starts_with_shortcut_list(settings));
}

/// A click on the heading of the list of shortcuts: folds the list or opens
/// it at once, here, and asks the worker to remember the state drawn
/// (`D123`). On the MAIN thread.
///
/// # Why the window and not the worker
///
/// The compact switch belongs to the worker because a shortcut reaches it
/// there too (`Collapse`). Nothing but this click reaches the list, so the
/// window owns it, as it owns the folds of the value sent last - and the
/// worker only saves, because the one [`KeptSettings`] of the run lives there.
/// It is told the state rather than "the other one", so two quick clicks save
/// the second state drawn, never a pair that undid itself.
pub fn wire_shortcut_list(palette: &Palette, asks: &Sender<Command>) {
    palette.on_toggle_hints({
        let palette = palette.as_weak();
        let asks = asks.clone();
        move || {
            if let Some(palette) = palette.upgrade() {
                let open = !palette.get_hints_open();
                palette.set_hints_open(open);
                // A send that fails is a palette already closing - nobody is
                // left to remember it for.
                let _ = asks.send(Command::ShortcutList(open));
            }
        }
    });
}

impl Worker<'_> {
    /// Remembers whether the list of shortcuts stands open (`D123`). Nothing
    /// is redrawn when the save goes through: the window drew the state before
    /// it asked, and the message band keeps what it says. A save that failed
    /// for a reason not said before is said, as after a change of the clearing.
    pub(super) fn remember_list(&self, open: bool) -> Carried {
        let view = self
            .memory
            .keep(SettingChange::ShortcutList(open))
            .map(|line| self.view(vec![line], ValueBand::Keep, None));
        Carried {
            view,
            told: None,
            toggled: None,
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;
    use crate::live::tests::{Registrar, a_palette, a_store, with_worker};

    /// `D123`: a first run opens the list, and every run after that opens it
    /// as the tester left it.
    #[test]
    fn the_list_opens_on_a_first_run_and_as_it_was_left_after_that() {
        let left = |open| Settings {
            shortcut_list: open,
            ..Settings::default()
        };
        assert!(starts_with_shortcut_list(&left(None)));
        assert!(starts_with_shortcut_list(&left(Some(true))));
        assert!(!starts_with_shortcut_list(&left(Some(false))));

        let palette = a_palette();
        label_shortcut_list(&palette, &left(Some(false)));
        assert!(
            !palette.get_hints_open(),
            "the list opened although it was left folded"
        );
        assert_eq!(palette.get_hints_label(), "Shortcuts");
    }

    /// The click folds the list in the window at once and asks the worker to
    /// remember exactly the state drawn - two clicks, two states, in order.
    #[test]
    fn a_click_on_the_heading_folds_the_list_and_asks_to_keep_the_state_drawn() {
        let palette = a_palette();
        let (asks, asked) = std::sync::mpsc::channel();
        palette.set_hints_open(true);
        wire_shortcut_list(&palette, &asks);
        palette.invoke_toggle_hints();
        assert!(!palette.get_hints_open(), "the click did not fold the list");
        palette.invoke_toggle_hints();
        assert!(palette.get_hints_open(), "the second click did not open it");
        assert_eq!(
            asked.try_iter().collect::<Vec<_>>(),
            vec![Command::ShortcutList(false), Command::ShortcutList(true)]
        );
    }

    /// The worker saves the state it is told and draws nothing: the window
    /// drew it already, and a redrawn message band would lose what it says.
    #[test]
    fn the_worker_keeps_the_state_of_the_list_and_draws_nothing() {
        let store = a_store(false);
        let registrar = Registrar::default();
        with_worker(&store, &registrar, true, |worker, _| {
            let folded = worker.carry_out(Command::ShortcutList(false));
            assert!(
                folded.view.is_none() && folded.told.is_none() && folded.toggled.is_none(),
                "remembering the list redrew the palette or told the shortcuts window"
            );
            let _ = worker.carry_out(Command::ShortcutList(true));
        });
        assert_eq!(
            *store.saved.borrow(),
            vec![
                SettingChange::ShortcutList(false),
                SettingChange::ShortcutList(true)
            ]
        );
    }

    /// A save that fails is said in the message band, once per run.
    #[test]
    fn a_list_that_cannot_be_kept_is_said_once() {
        let store = a_store(true);
        let registrar = Registrar::default();
        with_worker(&store, &registrar, true, |worker, _| {
            let first = worker.carry_out(Command::ShortcutList(false));
            let view = first.view.expect("the failed save is said");
            assert!(
                view.messages
                    .iter()
                    .any(|line| line.contains("could not be saved to f")),
                "{:?}",
                view.messages
            );
            let second = worker.carry_out(Command::ShortcutList(true));
            assert!(second.view.is_none(), "the same failure was said twice");
        });
    }
}
