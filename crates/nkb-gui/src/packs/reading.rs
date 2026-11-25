//! Reading the packs for the pack window, on a helper thread.
//!
//! Every opening reads the catalogue afresh - every pack file of both folders,
//! checked and parsed - so a pack fixed on disk shows at the next one. A team
//! folder on a share that answers slowly makes that seconds, and read on this
//! thread it froze the window and the palette with it (the urgent report's
//! point 9). So the packs are read on a helper thread, and opening waits for
//! them [`LIST_WAIT`] at most: a list read in time opens with the window, as
//! it always did, and a later one comes when it is read, the window saying
//! meanwhile in the place of its list that it reads. Letters typed then go into
//! the query, which the list takes when it comes. A list read for an opening
//! that has ended since is dropped.
//!
//! Out of `packs.rs`, which the reading took past the band of `D113`.

use std::rc::Rc;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, TryRecvError};
use std::time::Duration;

use nkb_adapters::i18n;
use nkb_adapters::{PackFolders, SystemClock, TomlPackFormat};
use nkb_app::browse_packs::Listing;
use nkb_app::ports::SourceError;
use nkb_app::{Clock, list_packs};

use super::{Packs, strings};
use crate::focus::POLL;
use crate::live::{InUseNow, in_use_now};
use crate::picker::PackPicker;
use crate::query::Query;

/// How long opening waits for the packs before the window shows without its
/// list (the module header). The catalogue the tool carries reads in about
/// 7.5 ms in release (`BL-2`, measured 2026-11-03), and folders on a local
/// disk add little to it, so a window that waits this long opens with its list
/// as before. A tenth of a second is about where a delay starts to be noticed -
/// an estimate, not a measurement.
pub const LIST_WAIT: Duration = Duration::from_millis(100);

/// What the window holds while it is open.
pub(super) enum Shown {
    /// The packs are still being read (the module header). `opening` tells
    /// this reading from one of an earlier opening, `query` is what the tester
    /// typed meanwhile, and `now` is what a list that never comes is made of.
    Reading {
        opening: u64,
        query: Query,
        now: InUseNow,
    },
    /// The list over what was read.
    Picking(PackPicker),
}

impl Packs {
    /// Whether the window is open and its packs are still being read - for a
    /// test, which waits for the list as the tester does.
    #[must_use]
    pub fn is_reading(&self) -> bool {
        matches!(*self.shown.borrow(), Some(Shown::Reading { .. }))
    }

    /// Makes every opening wait `wait` for its list instead of [`LIST_WAIT`] -
    /// for a test: zero shows the window while it reads, and a long wait
    /// opens it with its list however slow the build that runs the test.
    pub fn set_list_wait(&self, wait: Duration) {
        self.list_wait.set(wait);
    }

    /// Reads the packs for an opening on a helper thread and waits for them
    /// [`LIST_WAIT`] at most (the module header).
    pub(super) fn read(self: &Rc<Self>) {
        let now = in_use_now(&self.in_use);
        let folders = self.folders.borrow().clone();
        let (send, read) = mpsc::channel();
        let asked = now.clone();
        let reader = std::thread::Builder::new()
            .name("nkb-pack-list".to_owned())
            .spawn(move || {
                // Nobody waits any more when the window closed meanwhile.
                let _ = send.send(picker_over(&folders, &asked));
            });
        if reader.is_err() {
            // No thread to read on: read here, as the window did before.
            let folders = self.folders.borrow().clone();
            self.show_picker(picker_over(&folders, &now));
            return;
        }
        match read.recv_timeout(self.list_wait.get()) {
            Ok(picker) => self.show_picker(picker),
            Err(RecvTimeoutError::Disconnected) => self.show_picker(unread(&now)),
            Err(RecvTimeoutError::Timeout) => {
                let opening = self.openings.get().wrapping_add(1);
                self.openings.set(opening);
                *self.shown.borrow_mut() = Some(Shown::Reading {
                    opening,
                    query: Query::default(),
                    now,
                });
                self.window.set_notes(strings(&[]));
                self.show_list();
                self.wait_for_list(opening, read);
            }
        }
    }

    /// Takes the list once it is read, while the window is still open on the
    /// opening it was read for.
    fn wait_for_list(self: &Rc<Self>, opening: u64, read: Receiver<PackPicker>) {
        let weak = Rc::downgrade(self);
        slint::Timer::single_shot(POLL, move || {
            let Some(packs) = weak.upgrade() else {
                return;
            };
            if !packs.reads(opening) {
                return;
            }
            match read.try_recv() {
                Ok(picker) => packs.arrived(Some(picker)),
                Err(TryRecvError::Empty) => packs.wait_for_list(opening, read),
                // The reading thread ended without a list: it panicked.
                Err(TryRecvError::Disconnected) => packs.arrived(None),
            }
        });
    }

    /// Whether the window still waits for the list of `opening`.
    fn reads(&self, opening: u64) -> bool {
        matches!(
            *self.shown.borrow(),
            Some(Shown::Reading { opening: waiting, .. }) if waiting == opening
        )
    }

    /// The list read while the window was open - or, when the reading came to
    /// nothing, the list that says it could not be read - with what the tester
    /// typed meanwhile.
    fn arrived(&self, picker: Option<PackPicker>) {
        let taken = self.shown.borrow_mut().take();
        let (query, now) = match taken {
            Some(Shown::Reading { query, now, .. }) => (query, now),
            other => {
                *self.shown.borrow_mut() = other;
                return;
            }
        };
        let picker = picker.unwrap_or_else(|| unread(&now)).with_query(query);
        self.show_picker(picker);
    }

    /// Makes `picker` what the window shows.
    fn show_picker(&self, picker: PackPicker) {
        self.window.set_notes(strings(picker.notes()));
        *self.shown.borrow_mut() = Some(Shown::Picking(picker));
        self.show_list();
    }
}

/// The list for one opening, from a fresh look at the folders - what the
/// helper thread reads (the module header).
fn picker_over(folders: &PackFolders, now: &InUseNow) -> PackPicker {
    let listing = list_packs(&folders.catalogue(), &TomlPackFormat);
    let notes = listing.as_ref().map_or_else(
        |_| Vec::new(),
        |listing| i18n::catalogue_notes(&listing.coverage, folders),
    );
    picker_of(listing, now).with_notes(notes)
}

/// The list that says the packs could not be read - what an opening shows
/// when the thread reading them ended without a list.
fn unread(now: &InUseNow) -> PackPicker {
    picker_of(Err(SourceError::Unreadable), now)
}

/// The list over `listing`, opening on the pack and value in use.
fn picker_of(listing: Result<Listing, SourceError>, now: &InUseNow) -> PackPicker {
    PackPicker::new(
        listing,
        SystemClock.today(),
        now.pack.as_deref(),
        now.next.as_deref(),
    )
    .with_limit(now.made)
    .with_restart_key(now.restart.map(i18n::chord))
    .with_recent(&now.recent)
}
