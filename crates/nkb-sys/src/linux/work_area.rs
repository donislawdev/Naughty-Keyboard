//! The call behind `nkb_sys::screens::work_area` on Linux: `_NET_WORKAREA` of the
//! current desktop, which the window manager keeps on the root window of the X
//! server the windows of this program stand on - XWayland's on a Wayland session,
//! where the palette and every window opened from it are its clients
//! (`connect_windows`).
//!
//! One rectangle for the whole desktop, which is what the extended hints define:
//! the desktop less what the panels reserve. The screen a window stands on is
//! known to the window toolkit, not here, so the caller meets the two.
//!
//! # One connection, kept
//!
//! The palette asks at every move the window manager reports (`room.rs` in the
//! window layer), dozens of times a second while it is dragged, on the thread
//! that draws it. So the connection is opened at the first question and kept,
//! with the two names of the hint already looked up: a question is then two
//! properties read, not a new connection, its checks and two more lookups. A
//! kept connection that fails - the server went silent for `SERVER_WAIT`, which
//! gives the connection up for good (`stream`), or went away - is replaced once,
//! in the same question.

use std::sync::{Mutex, PoisonError};

use x11rb::protocol::xproto::{AtomEnum, ConnectionExt};

use super::Lost;
use super::connect::{Server, connect_windows};
use crate::screens::Monitor;

/// The work area of the current desktop, in the pixels of the server - or `None`
/// when there is no server for the windows, no manager that writes the hint, or a
/// hint this does not read.
pub(crate) fn work_area() -> Option<Monitor> {
    let mut kept = KEPT.lock().unwrap_or_else(PoisonError::into_inner);
    ask_kept(&mut kept, Asker::open, Asker::read).flatten()
}

/// The connection the questions go through, once one was opened.
static KEPT: Mutex<Option<Asker>> = Mutex::new(None);

/// A connection to the server of the windows, with the names of the hint.
struct Asker {
    server: Server,
    current: u32,
    area: u32,
}

impl Asker {
    fn open() -> Option<Self> {
        let server = connect_windows().ok()?.server;
        let atom = |name: &[u8]| -> Result<u32, Lost> {
            Ok(server.conn.intern_atom(false, name)?.reply()?.atom)
        };
        let (current, area) = (
            atom(b"_NET_CURRENT_DESKTOP").ok()?,
            atom(b"_NET_WORKAREA").ok()?,
        );
        Some(Self {
            server,
            current,
            area,
        })
    }

    fn read(&self) -> Result<Option<Monitor>, Lost> {
        let (conn, root) = (&self.server.conn, self.server.root);
        // Not written is the first desktop, the one every manager has.
        let desktop = conn
            .get_property(false, root, self.current, AtomEnum::CARDINAL, 0, 1)?
            .reply()?
            .value32()
            .and_then(|mut values| values.next())
            .unwrap_or(0);
        let values: Vec<u32> = conn
            .get_property(
                false,
                root,
                self.area,
                AtomEnum::CARDINAL,
                0,
                WORK_AREA_LONGEST,
            )?
            .reply()?
            .value32()
            .map(Iterator::collect)
            .unwrap_or_default();
        Ok(crate::screens::work_area_of_desktop(&values, desktop))
    }
}

/// The answer of `ask` through the connection in `kept`, which `open` makes when
/// there is none. A kept connection that fails is dropped and a new one asked
/// once - the answer is then the new one's, and it is kept. A new connection
/// that fails is not kept: the next question opens again. `None` when there is
/// no connection to be had or the one just opened fails.
fn ask_kept<S, T>(
    kept: &mut Option<S>,
    open: impl FnOnce() -> Option<S>,
    ask: impl Fn(&S) -> Result<T, Lost>,
) -> Option<T> {
    if let Some(server) = kept.as_ref() {
        match ask(server) {
            Ok(answer) => return Some(answer),
            Err(Lost) => *kept = None,
        }
    }
    let server = open()?;
    let answer = ask(&server).ok()?;
    *kept = Some(server);
    Some(answer)
}

/// How many numbers of the hint are read: four for each of far more desktops
/// than any manager keeps, so the current one is never cut off.
const WORK_AREA_LONGEST: u32 = 4 * 64;

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use core::cell::Cell;

    use super::{Lost, ask_kept};

    /// A connection that answers with its number, or fails when told to.
    struct Fake {
        number: u32,
        fails: bool,
    }

    /// Opens connections numbered from one, the `failing` ones failing, and
    /// counts them.
    struct Opener {
        opened: Cell<u32>,
        failing: Vec<u32>,
        none: bool,
    }

    impl Opener {
        fn new(failing: &[u32]) -> Self {
            Self {
                opened: Cell::new(0),
                failing: failing.to_vec(),
                none: false,
            }
        }

        fn open(&self) -> Option<Fake> {
            if self.none {
                return None;
            }
            let number = self.opened.get() + 1;
            self.opened.set(number);
            Some(Fake {
                number,
                fails: self.failing.contains(&number),
            })
        }
    }

    fn ask(fake: &Fake) -> Result<u32, Lost> {
        if fake.fails {
            Err(Lost)
        } else {
            Ok(fake.number)
        }
    }

    #[test]
    fn questions_go_through_one_connection() {
        let opener = Opener::new(&[]);
        let mut kept = None;
        let answers: Vec<_> = (0..3)
            .map(|_| ask_kept(&mut kept, || opener.open(), ask))
            .collect();
        assert_eq!(answers, [Some(1), Some(1), Some(1)]);
        assert_eq!(
            opener.opened.get(),
            1,
            "a question opened a connection again"
        );
    }

    #[test]
    fn a_kept_connection_that_fails_is_replaced_in_the_same_question() {
        let opener = Opener::new(&[]);
        let mut kept = None;
        assert_eq!(ask_kept(&mut kept, || opener.open(), ask), Some(1));
        kept.as_mut().expect("the first connection is kept").fails = true;
        assert_eq!(
            ask_kept(&mut kept, || opener.open(), ask),
            Some(2),
            "the question failed with its connection instead of asking a new one"
        );
        assert_eq!(kept.as_ref().map(|fake| fake.number), Some(2));
        assert_eq!(ask_kept(&mut kept, || opener.open(), ask), Some(2));
        assert_eq!(opener.opened.get(), 2);
    }

    #[test]
    fn a_new_connection_that_fails_is_not_kept_or_asked_again() {
        let opener = Opener::new(&[1]);
        let mut kept = None;
        assert_eq!(ask_kept(&mut kept, || opener.open(), ask), None);
        assert!(kept.is_none(), "a connection that failed was kept");
        assert_eq!(opener.opened.get(), 1, "a new connection was asked twice");
        assert_eq!(
            ask_kept(&mut kept, || opener.open(), ask),
            Some(2),
            "the next question did not open again"
        );
    }

    #[test]
    fn a_kept_connection_and_its_replacement_failing_give_nothing_and_keep_nothing() {
        let opener = Opener::new(&[2]);
        let mut kept = None;
        assert_eq!(ask_kept(&mut kept, || opener.open(), ask), Some(1));
        kept.as_mut().expect("the first connection is kept").fails = true;
        assert_eq!(ask_kept(&mut kept, || opener.open(), ask), None);
        assert!(kept.is_none());
        assert_eq!(opener.opened.get(), 2, "one replacement, not more");
    }

    #[test]
    fn no_server_gives_nothing_and_keeps_nothing() {
        let opener = Opener {
            none: true,
            ..Opener::new(&[])
        };
        let mut kept: Option<Fake> = None;
        assert_eq!(ask_kept(&mut kept, || opener.open(), ask), None);
        assert!(kept.is_none());
    }
}
