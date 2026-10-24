//! The file socket of a bus of this machine, and the watch that keeps a peer which stopped
//! answering from holding a call for ever (`D145` points 37, 41 and 53).
//!
//! Two routes talk to a bus: typing on Wayland talks to the input method daemon (`ibus`), and
//! the shortcuts on Wayland talk to the portal of the session bus (`portal`). Both open the
//! stream HERE, from a path their own rules have taken out of an address that was accepted
//! only if it named a file socket, and hand the open stream to the bus library. The library
//! therefore never reads an address and never follows one to a host, which is what an
//! address of `tcp:` would otherwise make it do. This is the one place of the program that
//! opens such a socket, which is what `nothing_reaches_the_network.rs` grants.

use core::time::Duration;
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread::JoinHandle;

/// Opens the file socket at `path`.
pub(super) fn connect(path: &str) -> std::io::Result<UnixStream> {
    UnixStream::connect(path)
}

/// A second handle on a socket that was given to the bus library. It can watch the socket and
/// shut it, which is all a route needs of it, so no route names the type of the socket.
pub(super) struct Handle(UnixStream);

impl Handle {
    /// Shuts the socket when `deadline` has passed, unless the watch is dropped first.
    pub(super) fn watch(&self, deadline: Duration) -> std::io::Result<Watch> {
        Watch::start(&self.0, deadline)
    }

    /// Shuts the socket, so that every call and wait on it returns at once.
    pub(super) fn shut_down(&self) {
        shut_down(&self.0);
    }
}

/// Opens the socket at `path`: the stream for the bus library, and the handle to watch and
/// shut it with.
pub(super) fn open(path: &str) -> std::io::Result<(UnixStream, Handle)> {
    let stream = connect(path)?;
    let kept = stream.try_clone()?;
    Ok((stream, Handle(kept)))
}

/// Whether something accepts connections at `path`. A connection made and dropped: the
/// peer sees a client that never speaks, which it takes to be one that went away.
pub(super) fn reachable(path: &str) -> bool {
    UnixStream::connect(path).is_ok()
}

/// Shuts the connection, so that every call and every wait on it returns at once. Shutting
/// a socket that is shut already is not a failure worth a word.
pub(super) fn shut_down(stream: &UnixStream) {
    let _ = stream.shutdown(Shutdown::Both);
}

/// Shuts the connection when `deadline` has passed, so that every call waiting on it
/// returns an error, and does nothing when it is dropped first.
pub(super) struct Watch {
    done: Option<Sender<()>>,
    thread: Option<JoinHandle<()>>,
    /// Set by the thread BEFORE it shuts the socket, so whoever the shut socket wakes finds
    /// it set: a call that ended because the deadline passed can tell itself from one that
    /// ended because the peer went.
    fired: Arc<AtomicBool>,
}

impl Watch {
    pub(super) fn start(stream: &UnixStream, deadline: Duration) -> std::io::Result<Self> {
        let shut = stream.try_clone()?;
        let (done, wait) = mpsc::channel::<()>();
        let fired = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&fired);
        let thread = std::thread::Builder::new()
            .name("nkb-bus-watch".to_owned())
            .spawn(move || {
                if wait.recv_timeout(deadline) == Err(RecvTimeoutError::Timeout) {
                    flag.store(true, Ordering::SeqCst);
                    shut_down(&shut);
                }
            })?;
        Ok(Self {
            done: Some(done),
            thread: Some(thread),
            fired,
        })
    }

    /// Whether the deadline passed and the socket was shut because of it.
    pub(super) fn fired(&self) -> bool {
        self.fired.load(Ordering::SeqCst)
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        // The thread wakes when the sender goes, and ends without having shut anything.
        drop(self.done.take());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;

    #[test]
    fn a_watch_whose_deadline_passed_says_so() {
        let (stream, _peer) = UnixStream::pair().expect("a pair of sockets");
        let watch = Watch::start(&stream, Duration::from_millis(20)).expect("a watch");
        let end = std::time::Instant::now() + Duration::from_secs(5);
        while !watch.fired() && std::time::Instant::now() < end {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(
            watch.fired(),
            "the deadline passed and the watch did not say so"
        );
    }

    #[test]
    fn a_watch_dropped_before_its_deadline_never_says_so() {
        let (stream, _peer) = UnixStream::pair().expect("a pair of sockets");
        let watch = Watch::start(&stream, Duration::from_secs(60)).expect("a watch");
        assert!(!watch.fired());
        let fired = Arc::clone(&watch.fired);
        drop(watch);
        assert!(
            !fired.load(Ordering::SeqCst),
            "a watch dropped in time fired"
        );
    }
}
