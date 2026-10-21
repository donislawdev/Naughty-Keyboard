//! `Escape` during a send on X11: ours for as long as one send lasts and not a
//! moment longer (`D96`, `OBS-202` point 4).
//!
//! The order of the reservation is `escape.rs`, shared with the other systems. Here
//! the key is taken with `XGrabKey` on the root window, in all four combinations
//! with Caps Lock and Num Lock because the server matches the grab against the
//! exact modifier state. Measured 2026-10-20: with the grab, a press of `Escape`
//! does not reach the application under test (0 extra events against 2 without),
//! and a second client asking for the same key is refused with `BadAccess`.
//!
//! The grab lives on a connection of its own, so the questions of the send and the
//! key presses of the person never wait for each other, and it is gone with the
//! connection even if the program is killed in the middle.

use core::time::Duration;

use x11rb::connection::Connection;
use x11rb::protocol::Event;
use x11rb::protocol::xproto::{ConnectionExt as _, GrabMode, ModMask};

use super::connect::{Server, connect};
use crate::escape::{ESCAPE_RELEASE_WAIT, EscapeKey, Reserved};
use crate::linux_rules::{DisplayAddress, key_is_down};

/// The server asks the grab for the state of the locks the key is pressed in, so
/// each combination is a grab of its own.
const LOCK_COMBINATIONS: [u16; 4] = [0, 0x02, 0x10, 0x12];

/// `Escape` on a connection of its own. Not usable - no second connection, no key
/// of that name - is a key that is never registered, and then a press is the
/// key going down after it was seen up (`escape.rs`), which here is nothing.
pub(super) struct XEscape {
    server: Option<Server>,
    keycode: u8,
}

pub(super) type ReservedEscape = Reserved<XEscape>;

impl XEscape {
    /// Reserves `Escape`, which is on `keycode`, for as long as the value lives.
    pub(super) fn reserve(address: DisplayAddress, keycode: Option<u8>) -> ReservedEscape {
        let usable =
            keycode.and_then(|keycode| connect(address).ok().map(|server| (server, keycode)));
        let key = match usable {
            Some((server, keycode)) => Self {
                server: Some(server),
                keycode,
            },
            None => Self {
                server: None,
                keycode: 0,
            },
        };
        Reserved::reserve(key, ESCAPE_RELEASE_WAIT)
    }

    fn ungrab(&self, server: &Server) {
        let _ = server
            .conn
            .ungrab_key(self.keycode, server.root, ModMask::ANY);
        let _ = server.conn.flush();
    }
}

impl EscapeKey for XEscape {
    fn register(&mut self) -> bool {
        let Some(server) = &self.server else {
            return false;
        };
        for locks in LOCK_COMBINATIONS {
            let granted = server
                .conn
                .grab_key(
                    false,
                    server.root,
                    ModMask::from(locks),
                    self.keycode,
                    GrabMode::ASYNC,
                    GrabMode::ASYNC,
                )
                .is_ok_and(|cookie| cookie.check().is_ok());
            if !granted {
                // Another program holds the key. Whatever was taken is given
                // back, so that half a reservation never stays behind.
                self.ungrab(server);
                return false;
            }
        }
        true
    }

    fn unregister(&mut self) {
        if let Some(server) = &self.server {
            self.ungrab(server);
        }
    }

    fn fired(&mut self) -> bool {
        let Some(server) = &self.server else {
            return false;
        };
        let mut fired = false;
        while let Ok(Some(event)) = server.conn.poll_for_event() {
            if let Event::KeyPress(press) = event {
                fired |= press.detail == self.keycode;
            }
        }
        fired
    }

    fn down(&mut self) -> bool {
        let Some(server) = &self.server else {
            return false;
        };
        server
            .conn
            .query_keymap()
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .is_some_and(|reply| key_is_down(&reply.keys, self.keycode))
    }

    fn pause(&mut self, how_long: Duration) {
        std::thread::sleep(how_long);
    }
}
