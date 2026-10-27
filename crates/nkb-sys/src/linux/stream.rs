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

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use std::io::{Read as _, Write as _};

    use super::super::connect::socket_pair;
    use super::*;

    const SHORT: Duration = Duration::from_millis(50);

    #[test]
    fn a_silent_server_is_given_up_after_the_limit_and_then_at_once() {
        let (inner, _server) = socket_pair().expect("a socket pair");
        let stream = LocalStream::new(inner, Wait::Bounded(SHORT));
        let started = Instant::now();
        let first = stream
            .poll(PollMode::Readable)
            .expect_err("nothing was written");
        let took = started.elapsed();
        assert_eq!(first.kind(), io::ErrorKind::TimedOut);
        assert!(took >= SHORT, "waited the limit: {took:?}");
        assert!(took < SHORT * 20, "and not much longer: {took:?}");

        let again = Instant::now();
        let second = stream.poll(PollMode::Readable).expect_err("given up");
        assert_eq!(second.kind(), io::ErrorKind::TimedOut);
        assert!(
            again.elapsed() < SHORT,
            "not waited twice: {:?}",
            again.elapsed()
        );
    }

    #[test]
    fn an_answer_ends_the_wait() {
        let (inner, mut server) = socket_pair().expect("a socket pair");
        let stream = LocalStream::new(inner, Wait::Bounded(Duration::from_secs(10)));
        server.write_all(b"x").expect("the answer");
        stream
            .poll(PollMode::Readable)
            .expect("a readable socket is not a silent server");
        let mut fds = Vec::new();
        let mut buf = [0u8; 4];
        assert_eq!(stream.read(&mut buf, &mut fds).expect("the byte"), 1);
    }

    #[test]
    fn writes_still_go_once_the_server_is_given_up() {
        // Giving up refuses waiting, not writing: the requests that give the spare
        // keys back are still written after the wait that ran out.
        let (inner, mut server) = socket_pair().expect("a socket pair");
        let stream = LocalStream::new(inner, Wait::Bounded(SHORT));
        let _ = stream.poll(PollMode::Readable);
        let mut fds = Vec::new();
        assert_eq!(stream.write(b"give back", &mut fds).expect("written"), 9);
        let mut buf = [0u8; 9];
        server.read_exact(&mut buf).expect("what was written");
        assert_eq!(&buf, b"give back");
    }

    #[test]
    fn a_connection_to_a_silent_server_ends_within_the_limit() {
        // The handshake waits for the server's setup like any answer: a socket that
        // accepts and never speaks is a refusal, not a hang.
        let (inner, _server) = socket_pair().expect("a socket pair");
        let stream = LocalStream::new(inner, Wait::Bounded(SHORT));
        let started = Instant::now();
        let refused = RustConnection::connect_to_stream(stream, 0);
        assert!(refused.is_err(), "no setup was ever sent");
        assert!(
            started.elapsed() < SHORT * 20,
            "ended within the limit: {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn the_listener_of_the_shortcuts_waits_as_long_as_it_takes() {
        // The thread of the shortcuts waits for the person, and an answer that comes
        // after a bounded wait would have run out still ends its wait.
        let (inner, mut server) = socket_pair().expect("a socket pair");
        let stream = LocalStream::new(inner, Wait::Forever);
        let writer = std::thread::spawn(move || {
            std::thread::sleep(SHORT * 2);
            server.write_all(b"x").expect("the answer");
            server
        });
        stream
            .poll(PollMode::Readable)
            .expect("waited until the answer");
        let _server = writer.join().expect("the writer");
    }
}
