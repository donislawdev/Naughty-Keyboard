//! Whether the palette's shortcuts are held by this process now, and the
//! registrar that takes them.
//!
//! # A set in which nothing took is not held (`OBS-176`)
//!
//! A second palette finds every shortcut held by the first. Until 2026-10-14
//! that set was held anyway and the sequence waited on it - and the Windows
//! listener, which does not pump for a set where nothing registered, had
//! already ended its thread. The wait answered `Wait::Gone`, and the palette
//! wrote "the shortcut listener has stopped, restart the palette" over the
//! sentences saying the shortcuts were taken. A restart changed nothing while
//! the first palette ran.
//!
//! Nothing here knows about that listener: a set in which no shortcut took can
//! never be pressed, whatever stands behind it
//! ([`LiveShortcuts::holds_any`]), and that alone is reason not to wait on it.
//! It is [`Holding::Lost`], the state that already meant "nothing could be
//! registered", and a resume tries again as it does there.

use nkb_adapters::i18n;
use nkb_app::ports::{HotkeyRegistrar, LiveShortcuts, ShortcutRegistration};
use nkb_core::hotkeys::{Bindings, HotkeyAction, HotkeyChord};

/// Whether the palette's shortcuts are held by this process now.
enum Holding {
    /// Registered, and driving the sequence.
    Held(Box<dyn LiveShortcuts + Send>),
    /// Given back to the system while the shortcuts window is open.
    Paused,
    /// Not held, and not because of the window: nothing could be registered,
    /// none of the shortcuts took, or the thread behind them ended. Said when
    /// it happened. A [`Command::Resume`](super::Command::Resume) tries again.
    Lost,
}

/// The palette's shortcuts as this thread holds them, and the registrar that
/// takes them.
pub(super) struct Hold<'a> {
    pub(super) registrar: &'a dyn HotkeyRegistrar,
    holding: Holding,
    /// What the last registration said, chord by chord - see
    /// [`ShortcutsNow`](super::ShortcutsNow).
    pub(super) registered: Vec<(HotkeyAction, HotkeyChord, ShortcutRegistration)>,
}

impl<'a> Hold<'a> {
    pub(super) const fn new(registrar: &'a dyn HotkeyRegistrar) -> Self {
        Self {
            registrar,
            holding: Holding::Lost,
            registered: Vec::new(),
        }
    }

    pub(super) fn live(&self) -> Option<&dyn LiveShortcuts> {
        match &self.holding {
            Holding::Held(live) => Some(live.as_ref()),
            Holding::Paused | Holding::Lost => None,
        }
    }

    pub(super) const fn paused(&self) -> bool {
        matches!(self.holding, Holding::Paused)
    }

    /// Gives the shortcuts back. Dropping the handle is the release, and it
    /// returns only once the system has let them go.
    pub(super) fn pause(&mut self) {
        self.holding = Holding::Paused;
    }

    /// The thread behind the shortcuts has ended.
    pub(super) fn lose(&mut self) {
        self.holding = Holding::Lost;
    }

    /// Registers `bindings` in place of whatever is held, and the lines worth
    /// saying about it.
    ///
    /// 🔴 What is held is released FIRST. The old set holds the very chords the
    /// new one asks for, so the other order would come back `Taken`, against
    /// ourselves, for every chord that did not move.
    ///
    /// A registration that worked says nothing, and every shortcut taken is
    /// one sentence - both are `i18n::registrations` deciding, not this
    /// module. A set in which nothing took is let go here and stays
    /// [`Holding::Lost`]: see the module.
    pub(super) fn take(&mut self, bindings: &Bindings) -> Vec<String> {
        self.holding = Holding::Lost;
        match self.registrar.register(bindings.as_slice()) {
            Ok(live) => {
                self.registered = live
                    .outcomes()
                    .iter()
                    .map(|(action, outcome)| (*action, bindings.chord(*action), *outcome))
                    .collect();
                if live.holds_any() {
                    self.holding = Holding::Held(live);
                }
                i18n::registrations(&self.registered)
            }
            Err(error) => {
                self.registered.clear();
                vec![i18n::shortcuts_unavailable(&error)]
            }
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
    use nkb_adapters::i18n;
    use nkb_app::ports::ShortcutsUnavailable;

    use crate::live::Command;
    use crate::live::tests::{Registrar, a_store, with_worker};

    /// Not held because nothing could be registered is a state the worker
    /// stays in, saying why - and the next resume tries again. Until K5.3 the
    /// worker ended there.
    #[test]
    fn with_nothing_registrable_the_worker_says_why_and_a_resume_tries_again() {
        let store = a_store(false);
        let registrar = Registrar::default();
        registrar.unavailable.set(true);
        with_worker(&store, &registrar, false, |worker, memory| {
            let lines = worker.take(memory.bindings.get());
            assert_eq!(
                lines,
                vec![i18n::shortcuts_unavailable(
                    &ShortcutsUnavailable::CouldNotStart
                )]
            );
            assert!(worker.hold.live().is_none());
            assert!(
                !worker.hold.paused(),
                "lost is not paused, and must not say it is"
            );

            let chosen = worker
                .carry_out(Command::Choose(String::from("unicode-text")))
                .view
                .expect("a pack is still chosen");
            assert_eq!(chosen.pack, "Unicode and text");

            registrar.unavailable.set(false);
            worker.carry_out(Command::Resume);
            assert!(worker.hold.live().is_some(), "the resume did not try again");
        });
    }

    /// `OBS-176`: a second palette, every shortcut held by the first. Nothing
    /// is held, so the sequence never waits on a set that cannot be pressed,
    /// and one sentence says why. The window still learns which chords are
    /// taken, and once they come free a resume holds them.
    #[test]
    fn with_every_shortcut_taken_nothing_is_held_and_one_sentence_says_why() {
        let store = a_store(false);
        let registrar = Registrar::default();
        let chords = nkb_adapters::default_bindings();
        registrar
            .taken
            .borrow_mut()
            .extend(chords.as_slice().iter().map(|(_, chord)| *chord));
        with_worker(&store, &registrar, false, |worker, memory| {
            let lines = worker.take(memory.bindings.get());
            assert_eq!(lines.len(), 1, "{lines:?}");
            assert!(
                lines[0].starts_with("Every shortcut is already taken"),
                "{lines:?}"
            );
            assert!(worker.hold.live().is_none(), "a set where nothing took");
            assert!(!worker.hold.paused());
            assert_eq!(worker.hold.registered.len(), chords.as_slice().len());

            registrar.taken.borrow_mut().clear();
            worker.carry_out(Command::Resume);
            assert!(worker.hold.live().is_some(), "the resume did not try again");
        });
    }
}
