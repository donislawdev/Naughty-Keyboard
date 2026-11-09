//! What the worker remembers between commands: the settings of the run, where
//! they are saved, the shortcuts registered last and the pack folders. Out of
//! `live.rs`, which stands at the ceiling of `D113`.

use std::cell::{Cell, RefCell};

use nkb_adapters::i18n;
use nkb_app::KeptSettings;
use nkb_app::ports::{SettingChange, SettingsStore};
use nkb_core::hotkeys::Bindings;

use super::folders::Folders;

/// The settings of this run and where they are saved, shared by everything the
/// worker remembers: collapsing the palette and choosing a pack.
///
/// One [`KeptSettings`] for both, never two: each save rewrites one key of the
/// file (`D84`), but a failure is said once PER RUN, and two copies would say
/// it twice and disagree about what is in effect.
pub(super) struct Memory<'a> {
    pub(super) kept: RefCell<KeptSettings>,
    pub(super) store: &'a dyn SettingsStore,
    /// Where the settings live, for a sentence saying they were not saved.
    pub(super) file: String,
    /// The shortcuts registered last, for the sentences that name one. A
    /// cell, because the shortcuts window changes them while the palette runs
    /// (K5.3).
    pub(super) bindings: Cell<Bindings>,
    /// The pack folders, read again at every choice of a pack (`D173`).
    pub(super) folders: Folders,
}

impl Memory<'_> {
    /// Remembers one change, in effect at once and saved when it can be. A
    /// line to say when saving failed for a reason not said before.
    pub(super) fn keep(&self, change: SettingChange) -> Option<String> {
        self.kept
            .borrow_mut()
            .keep(self.store, change)
            .and_then(|message| i18n::settings_message(&message, &self.file, &self.bindings.get()))
    }
}
