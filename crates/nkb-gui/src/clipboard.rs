//! The system clipboard, behind `nkb_app::ports::Clipboard`.
//!
//! # Why this adapter lives in the GUI package
//!
//! Every other adapter sits in `nkb-adapters`, which both executables link. This
//! one cannot: `arboard` brings AppKit on macOS and an X11 client on Linux, and
//! a Cargo feature gating it there would be switched on for the CLI too by any
//! workspace-wide build - feature unification does not ask which binary is being
//! linked. `D25` says `nkb` carries no graphical library, so the adapter lives in
//! the one package that already carries one. It adds no crate: Slint links this
//! very library for its own text fields.
//!
//! # Created on first use, kept for the palette's lifetime
//!
//! On Linux a clipboard is a connection to the display server, and the content
//! is SERVED by the process that set it for as long as it is alive - there is no
//! system copy to hand it to unless a clipboard manager runs. So the handle is
//! opened the first time a report is copied (a palette that never copies never
//! connects) and held until the worker thread ends, which is when the palette
//! closes. `arboard` offers the content to a clipboard manager on that drop; with
//! none running, the block leaves with the palette. Named in `ux-spec.md` 7.
//!
//! # 🔴 Kept off the cloud; in the history or out of it, as asked
//!
//! Windows can upload the clipboard to the owner's account and sync it to their
//! other devices. The tool promises to send nothing over the network, and a copy
//! it triggers must not become an upload it caused - so on Windows EVERYTHING
//! this adapter writes carries the documented `CanUploadToCloudClipboard = 0`.
//! macOS has no such switch in this library - Universal Clipboard may carry the
//! text to a nearby Apple device, and that is recorded in `D68` rather than
//! hidden.
//!
//! The LOCAL history is the caller's choice ([`History`]). The report block
//! stays in it on purpose: a tester who copied three blocks and pastes them
//! into three tickets reaches for exactly that history (`D68`). The values of
//! clipboard mode stay out of it (`D71`), through the one switch the library
//! has on each system: `CanIncludeInClipboardHistory = 0` on Windows, the
//! `org.nspasteboard.ConcealedType` convention on macOS, and KDE's password
//! manager hint on Linux - read from the library's source, 3.6.1. What the
//! managers of other desktops do with it is theirs.

use std::cell::RefCell;

use nkb_app::ports::{Clipboard, ClipboardError, History};

/// The clipboard of the session the palette runs in.
#[derive(Default)]
pub struct SystemClipboard {
    handle: RefCell<Option<arboard::Clipboard>>,
}

impl SystemClipboard {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl Clipboard for SystemClipboard {
    fn put_text(&self, text: &str, history: History) -> Result<(), ClipboardError> {
        let mut slot = self.handle.borrow_mut();
        if slot.is_none() {
            *slot = Some(arboard::Clipboard::new().map_err(error)?);
        }
        let Some(clipboard) = slot.as_mut() else {
            // Just filled above. Said rather than unwrapped if it ever is not.
            return Err(ClipboardError::Failed {
                detail: "the clipboard handle vanished".to_owned(),
            });
        };
        let set = clipboard.set();
        #[cfg(windows)]
        let set = {
            use arboard::SetExtWindows;
            let set = set.exclude_from_cloud();
            match history {
                History::Keep => set,
                History::Skip => set.exclude_from_history(),
            }
        };
        #[cfg(target_os = "macos")]
        let set = {
            use arboard::SetExtApple;
            match history {
                History::Keep => set,
                History::Skip => set.exclude_from_history(),
            }
        };
        #[cfg(all(unix, not(target_os = "macos")))]
        let set = {
            use arboard::SetExtLinux;
            match history {
                History::Keep => set,
                History::Skip => set.exclude_from_history(),
            }
        };
        set.text(text).map_err(error)
    }
}

/// The library's error, sorted onto the two answers the palette gives.
///
/// Only "held by somebody else" is passing - `arboard` has already retried it
/// (six attempts, five milliseconds apart, on Windows). Everything else keeps
/// the library's own sentence, without its full stop, because it is dropped into
/// the middle of one of ours.
fn error(error: arboard::Error) -> ClipboardError {
    match error {
        arboard::Error::ClipboardOccupied => ClipboardError::Busy,
        other => ClipboardError::Failed {
            detail: other.to_string().trim_end_matches('.').to_owned(),
        },
    }
}

#[cfg(test)]
#[allow(
    clippy::panic,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;

    // 🔴 No test here writes to the real clipboard, and that is a rule, not an
    // omission: `cargo test` runs while the owner works on this machine, and a
    // test that replaced their clipboard would destroy whatever they had copied.
    // What the real clipboard does is measured by `tools/petla-palety.ps1`, run
    // by hand with a warning first: the text, the history and cloud flags, and
    // the block. What CAN be checked without touching it is the sorting below.

    #[test]
    fn only_a_held_clipboard_is_passing() {
        assert_eq!(
            error(arboard::Error::ClipboardOccupied),
            ClipboardError::Busy
        );
        assert!(matches!(
            error(arboard::Error::ClipboardNotSupported),
            ClipboardError::Failed { .. }
        ));
    }

    #[test]
    fn the_library_sentence_loses_its_full_stop_and_nothing_else() {
        let ClipboardError::Failed { detail } = error(arboard::Error::Unknown {
            description: "the display went away.".to_owned(),
        }) else {
            panic!("an unknown error is not passing");
        };
        assert!(!detail.ends_with('.'), "{detail}");
        assert!(detail.contains("the display went away"), "{detail}");
    }
}
