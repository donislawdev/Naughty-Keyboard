//! The socket under a connection to the X server, with a bound on how long it waits.
//!
//! The library waits for the server in `poll` with no timeout, and it makes the socket
//! non-blocking first, so a read timeout on the socket would change nothing. A server
//! that stops answering - stopped, hung, or held by another client's grab - would keep
//! the waiting thread for ever: `nkb send`, the worker of the palette, and the cleanup
//! that gives the spare keys back. So the wait is ours: the library's own stream, with
//! `poll` given a limit.
//!
//! The first wait that runs out gives the connection up, and every later wait on it
//! fails at once. A server that did not answer for the limit is not waited for again by
//! the same send, so getting out costs one limit, not one per question. Writes are not
//! waits, so the requests that give the spare keys back still go into the socket. That
//! does not give them back: a server stopped until this program has ended closes the
//! connection without carrying them out, and the keys stay rewritten (measured
//! 2026-10-27 on Xvfb, `petla-ofiary-linux.py` scene `serwer-czeka`), as they do after
//! a program killed in the middle of a send.
//!
//! Its tests are in `connect.rs`, which makes the socket a test needs.

use core::sync::atomic::{AtomicBool, Ordering};
use core::time::Duration;
use std::io;
use std::io::IoSlice;
use std::time::Instant;

use rustix::event::{PollFd, PollFlags, Timespec, poll};
use rustix::fd::AsFd as _;
use rustix::io::Errno;
use x11rb::rust_connection::{DefaultStream, PollMode, RustConnection, Stream};
use x11rb::utils::RawFdContainer;

/// A connection to the X server over [`LocalStream`].
pub(crate) type Conn = RustConnection<LocalStream>;

/// How long a wait for the server may last.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Wait {
    /// Questions and answers: a server that is silent this long is given up.
    Bounded(Duration),
    /// A thread whose work is to wait for the person, the shortcuts of the palette.
    Forever,
}

/// The local socket of the server, waited on for at most what `wait` says.
#[derive(Debug)]
pub(crate) struct LocalStream {
    inner: DefaultStream,
    wait: Wait,
    gave_up: AtomicBool,
}

impl LocalStream {
    pub(crate) const fn new(inner: DefaultStream, wait: Wait) -> Self {
        Self {
            inner,
            wait,
            gave_up: AtomicBool::new(false),
        }
    }

    /// Waits for the socket as `mode` says, for at most `limit` in all.
    fn poll_within(&self, mode: PollMode, limit: Duration) -> io::Result<()> {
        if self.gave_up.load(Ordering::Relaxed) {
            return Err(silent());
        }
        let mut flags = PollFlags::empty();
        if mode.readable() {
            flags |= PollFlags::IN;
        }
        if mode.writable() {
            flags |= PollFlags::OUT;
        }
        let deadline = Instant::now() + limit;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let timeout = Timespec::try_from(left).map_err(|_| silent())?;
            let mut fds = [PollFd::from_borrowed_fd(self.inner.as_fd(), flags)];
            match poll(&mut fds, Some(&timeout)) {
                Ok(0) => {
                    self.gave_up.store(true, Ordering::Relaxed);
                    return Err(silent());
                }
                Ok(_) => return Ok(()),
                // A signal is not an answer. What is left of the limit is waited.
                Err(Errno::INTR) => {}
                Err(error) => return Err(error.into()),
            }
        }
    }
}

/// The error of a server that did not answer in time.
fn silent() -> io::Error {
    io::Error::new(io::ErrorKind::TimedOut, "the X server did not answer")
}

impl Stream for LocalStream {
    fn poll(&self, mode: PollMode) -> io::Result<()> {
        match self.wait {
            Wait::Bounded(limit) => self.poll_within(mode, limit),
            Wait::Forever => self.inner.poll(mode),
        }
    }

    fn read(&self, buf: &mut [u8], fd_storage: &mut Vec<RawFdContainer>) -> io::Result<usize> {
        self.inner.read(buf, fd_storage)
    }

    fn write(&self, buf: &[u8], fds: &mut Vec<RawFdContainer>) -> io::Result<usize> {
        self.inner.write(buf, fds)
    }

    fn write_vectored(
        &self,
        bufs: &[IoSlice<'_>],
        fds: &mut Vec<RawFdContainer>,
    ) -> io::Result<usize> {
        self.inner.write_vectored(bufs, fds)
    }
}
