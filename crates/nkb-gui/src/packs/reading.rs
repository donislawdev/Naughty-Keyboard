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
//! One read at a time ([`InFlight`]): an opening that finds a read still under
//! way waits for that one rather than start another beside it, so a folder on
//! a share that stopped answering holds one thread however often the window
//! opens, not one per opening. A read that ended while no window waited for it
//! is dropped, since an opening wants a fresh look at the folders. A read
//! brings the packs as the window offers them, and the opening makes its list
//! of them with the pack and value in use when the list comes - which a read
//! begun for an earlier opening cannot know.
//!
//! Out of `packs.rs`, which the reading took past the band of `D113`.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::time::Duration;

use nkb_adapters::i18n;
use nkb_adapters::{PackFolders, SystemClock, TomlPackFormat};
use nkb_app::ports::SourceError;
use nkb_app::{Clock, list_packs};

use super::{Packs, strings};
use crate::focus::POLL;
use crate::live::{InUseNow, in_use_now};
use crate::picker::{PackChoices, PackPicker};
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
    /// typed meanwhile, and `now` is the pack and value in use, which the list
    /// opens on when it comes.
    Reading {
        opening: u64,
        query: Query,
        now: InUseNow,
    },
    /// The list over what was read.
    Picking(PackPicker),
}

/// How the window reads its packs, kept from one opening to the next.
pub(super) struct Reader {
    /// How many openings had to wait for their list - the number of the last.
    openings: Cell<u64>,
    /// How long an opening waits for its list: [`LIST_WAIT`], or what a test
    /// sets.
    wait: Cell<Duration>,
    /// The read under way, which outlives the opening it began for.
    in_flight: RefCell<InFlight<Read>>,
}

impl Default for Reader {
    fn default() -> Self {
        Self {
            openings: Cell::new(0),
            wait: Cell::new(LIST_WAIT),
            in_flight: RefCell::default(),
        }
    }
}

/// What one read of the packs brings: the packs as the window offers them, and
/// a note for each source that was not read.
pub(super) struct Read {
    choices: PackChoices,
    notes: Vec<String>,
}

/// The one read that may be under way (the module header): what it will send
/// comes in here.
pub(super) struct InFlight<T>(Option<Receiver<T>>);

impl<T> Default for InFlight<T> {
    fn default() -> Self {
        Self(None)
    }
}

/// What came of waiting for the read under way.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Came<T> {
    /// The read ended and brought this.
    Read(T),
    /// Nothing will come: the thread reading ended without a list - it
    /// panicked - or no read was under way.
    Lost,
    /// The read is still under way.
    NotYet,
}

impl<T> InFlight<T> {
    /// Makes sure a read is under way for an opening: the one that still is,
    /// or a new one that `start` begins, handed where to send what it reads.
    /// A read that ended while nobody waited for it is dropped first. `false`
    /// when no read is under way and `start` could not begin one.
    pub(super) fn join_or_start(&mut self, start: impl FnOnce(Sender<T>) -> bool) -> bool {
        let under_way = self
            .0
            .as_ref()
            .is_some_and(|read| matches!(read.try_recv(), Err(TryRecvError::Empty)));
        if !under_way {
            let (send, read) = mpsc::channel();
            self.0 = start(send).then_some(read);
        }
        self.0.is_some()
    }

    /// What came of the read under way, waiting `wait` for it at most. A read
    /// that came, or that nothing will come of, is no longer under way.
    pub(super) fn wait(&mut self, wait: Duration) -> Came<T> {
        let Some(read) = &self.0 else {
            return Came::Lost;
        };
        let came = match read.recv_timeout(wait) {
            Ok(read) => Came::Read(read),
            Err(RecvTimeoutError::Disconnected) => Came::Lost,
            Err(RecvTimeoutError::Timeout) => return Came::NotYet,
        };
        self.0 = None;
        came
    }
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
        self.reader.wait.set(wait);
    }

    /// Reads the packs for an opening on a helper thread, or joins the read
    /// still under way, and waits for them [`LIST_WAIT`] at most (the module
    /// header).
    pub(super) fn read(self: &Rc<Self>) {
        let now = in_use_now(&self.in_use);
        let folders = self.folders.borrow().clone();
        let under_way = self.reader.in_flight.borrow_mut().join_or_start(|send| {
            std::thread::Builder::new()
                .name("nkb-pack-list".to_owned())
                .spawn(move || {
                    // What no opening waits for is dropped (the module header).
                    let _ = send.send(read_over(&folders));
                })
                .is_ok()
        });
        if !under_way {
            // No thread to read on: read here, as the window did before.
            let folders = self.folders.borrow().clone();
            self.show_picker(picker_of(read_over(&folders), &now));
            return;
        }
        let came = self
            .reader
            .in_flight
            .borrow_mut()
            .wait(self.reader.wait.get());
        match came {
            Came::Read(read) => self.show_picker(picker_of(read, &now)),
            Came::Lost => self.show_picker(unread(&now)),
            Came::NotYet => {
                let opening = self.reader.openings.get().wrapping_add(1);
                self.reader.openings.set(opening);
                *self.shown.borrow_mut() = Some(Shown::Reading {
                    opening,
                    query: Query::default(),
                    now,
                });
                self.window.set_notes(strings(&[]));
                self.show_list();
                self.wait_for_list(opening);
            }
        }
    }

    /// Takes the list once it is read, while the window is still open on the
    /// opening it waits for. When the window closes first, the read stays
    /// under way for the next opening to join.
    fn wait_for_list(self: &Rc<Self>, opening: u64) {
        let weak = Rc::downgrade(self);
        slint::Timer::single_shot(POLL, move || {
            let Some(packs) = weak.upgrade() else {
                return;
            };
            if !packs.reads(opening) {
                return;
            }
            let came = packs.reader.in_flight.borrow_mut().wait(Duration::ZERO);
            match came {
                Came::Read(read) => packs.arrived(Some(read)),
                Came::Lost => packs.arrived(None),
                Came::NotYet => packs.wait_for_list(opening),
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

    /// The list over what was read while the window was open - or, when the
    /// reading came to nothing, the list that says it could not be read - with
    /// what the tester typed meanwhile.
    fn arrived(&self, read: Option<Read>) {
        let taken = self.shown.borrow_mut().take();
        let (query, now) = match taken {
            Some(Shown::Reading { query, now, .. }) => (query, now),
            other => {
                *self.shown.borrow_mut() = other;
                return;
            }
        };
        let picker = read.map_or_else(|| unread(&now), |read| picker_of(read, &now));
        self.show_picker(picker.with_query(query));
    }

    /// Makes `picker` what the window shows.
    fn show_picker(&self, picker: PackPicker) {
        self.window.set_notes(strings(picker.notes()));
        *self.shown.borrow_mut() = Some(Shown::Picking(picker));
        self.show_list();
    }
}

/// What one read brings, from a fresh look at the folders - what the helper
/// thread does (the module header).
fn read_over(folders: &PackFolders) -> Read {
    let listing = list_packs(&folders.catalogue(), &TomlPackFormat);
    let notes = listing.as_ref().map_or_else(
        |_| Vec::new(),
        |listing| i18n::catalogue_notes(&listing.coverage, folders),
    );
    Read {
        choices: PackChoices::of(listing, SystemClock.today()),
        notes,
    }
}

/// The list that says the packs could not be read - what an opening shows
/// when the thread reading them ended without a list.
fn unread(now: &InUseNow) -> PackPicker {
    let read = Read {
        choices: PackChoices::of(Err(SourceError::Unreadable), SystemClock.today()),
        notes: Vec::new(),
    };
    picker_of(read, now)
}

/// The list over what a read brought, opening on the pack and value in use.
fn picker_of(read: Read, now: &InUseNow) -> PackPicker {
    PackPicker::over(read.choices, now.pack.as_deref(), now.next.as_deref())
        .with_limit(now.made)
        .with_restart_key(now.restart.map(i18n::chord))
        .with_recent(&now.recent)
        .with_notes(read.notes)
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::sync::mpsc::Sender;
    use std::time::Duration;

    use super::{Came, InFlight};

    /// A read begun and still under way: the sender it would send through,
    /// kept until the test sends - and the number of reads begun.
    #[derive(Default)]
    struct Reads {
        begun: Cell<u32>,
        sender: RefCell<Option<Sender<u32>>>,
    }

    impl Reads {
        fn begin(&self, send: Sender<u32>) -> bool {
            self.begun.set(self.begun.get() + 1);
            *self.sender.borrow_mut() = Some(send);
            true
        }

        /// Ends the read under way with `value` - whether one was under way
        /// and its value could go.
        fn end_with(&self, value: u32) -> bool {
            let sender = self.sender.borrow_mut().take();
            sender.is_some_and(|sender| sender.send(value).is_ok())
        }
    }

    #[test]
    fn an_opening_joins_the_read_under_way_rather_than_begin_another() {
        let reads = Reads::default();
        let mut in_flight = InFlight::default();
        assert!(in_flight.join_or_start(|send| reads.begin(send)));
        assert_eq!(in_flight.wait(Duration::ZERO), Came::NotYet);

        // The window closed and opened again while the folder still answers.
        assert!(in_flight.join_or_start(|send| reads.begin(send)));
        assert_eq!(reads.begun.get(), 1, "a second read beside the first");

        assert!(reads.end_with(7));
        assert_eq!(in_flight.wait(Duration::ZERO), Came::Read(7));
        assert_eq!(
            in_flight.wait(Duration::ZERO),
            Came::Lost,
            "a read taken is no longer under way"
        );
    }

    #[test]
    fn a_read_that_ended_while_nobody_waited_is_dropped_for_a_fresh_one() {
        let reads = Reads::default();
        let mut in_flight = InFlight::default();
        assert!(in_flight.join_or_start(|send| reads.begin(send)));
        // Read to the end after its window closed.
        assert!(reads.end_with(1));

        assert!(in_flight.join_or_start(|send| reads.begin(send)));
        assert_eq!(reads.begun.get(), 2, "the next opening looks afresh");
        assert!(reads.end_with(2));
        assert_eq!(in_flight.wait(Duration::ZERO), Came::Read(2));
    }

    #[test]
    fn a_read_that_brings_nothing_is_lost_and_the_next_opening_begins_anew() {
        let reads = Reads::default();
        let mut in_flight = InFlight::default();
        // The thread ended without sending: it panicked.
        assert!(in_flight.join_or_start(|send| {
            drop(send);
            true
        }));
        assert_eq!(in_flight.wait(Duration::ZERO), Came::Lost);

        assert!(in_flight.join_or_start(|send| reads.begin(send)));
        assert_eq!(reads.begun.get(), 1);

        // No thread to read on: nothing is under way.
        let mut none_begun = InFlight::<u32>::default();
        assert!(!none_begun.join_or_start(|_| false));
        assert_eq!(none_begun.wait(Duration::ZERO), Came::Lost);
    }
}
