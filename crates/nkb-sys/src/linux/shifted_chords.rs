//! The shortcuts of the palette, where the engine of a send finds them when the `Shift` of a
//! clearing has changed one (`OBS-260`). `linux_shifted_chord_rules.rs` has the measurement
//! and what is decided.
//!
//! The thread that hears the presses of the portal (`portal.rs`) puts its shortcuts here when
//! it starts and takes them away when it ends, and the engine of a send, which runs on a
//! thread of its bus connection, reads them. Both are in the process of the palette, which
//! has one session of shortcuts at a time. What the engine says goes through the same `Held`
//! as what the portal says, so a repeat is one press whoever announced it. The sender of the
//! presses is held here only while that thread runs: a sender kept after it would leave the
//! palette waiting on a channel that never says the shortcuts are gone.

use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;

use crate::hotkey::HotkeyId;
use crate::linux_portal_rules::Held;
use crate::linux_shifted_chord_rules::{ShiftedChord, ShiftedChords, Word};

/// The shortcuts of one session of the portal, the names the desktop knows them under, what
/// is held of them, and where their presses go.
struct Watched {
    session: String,
    chords: Vec<ShiftedChord>,
    names: Vec<(String, HotkeyId)>,
    held: Arc<Mutex<Held>>,
    presses: Sender<HotkeyId>,
}

/// Empty outside a palette, so `nkb send` takes nothing.
static WATCHED: Mutex<Option<Watched>> = Mutex::new(None);

/// The shortcuts of `session` stand here until this is dropped.
pub(super) struct Watching {
    session: String,
}

impl Drop for Watching {
    fn drop(&mut self) {
        let mut watched = WATCHED.lock().unwrap_or_else(PoisonError::into_inner);
        // A session that began after this one has put its own shortcuts here, and they stay.
        if watched
            .as_ref()
            .is_some_and(|watched| watched.session == self.session)
        {
            *watched = None;
        }
    }
}

/// Puts `chords`, the shortcuts of `session` known by `names`, here, with what is held of them
/// and the sender their presses go to.
pub(super) fn watch(
    session: &str,
    chords: Vec<ShiftedChord>,
    names: Vec<(String, HotkeyId)>,
    held: Arc<Mutex<Held>>,
    presses: Sender<HotkeyId>,
) -> Watching {
    *WATCHED.lock().unwrap_or_else(PoisonError::into_inner) = Some(Watched {
        session: session.to_owned(),
        chords,
        names,
        held,
        presses,
    });
    Watching {
        session: session.to_owned(),
    }
}

/// Whether the engine takes the event of key `keyval` on key `keycode` with `state`. What the
/// portal would have said of it goes through `Held`, and a press is told to the palette.
/// `ours` is whether the recipe holds `Shift` now, and `down` is what this engine took and
/// has not seen let go.
pub(super) fn meet(
    down: &mut ShiftedChords,
    keyval: u32,
    keycode: u32,
    state: u32,
    ours: bool,
) -> bool {
    let watched = WATCHED.lock().unwrap_or_else(PoisonError::into_inner);
    let chords = watched
        .as_ref()
        .map_or(&[][..], |watched| watched.chords.as_slice());
    let met = down.handle(keyval, keycode, state, ours, chords);
    if let (Some(word), Some(watched)) = (met.said, watched.as_ref()) {
        say(watched, word);
    }
    met.taken
}

/// `word` told as the portal's words are told (`portal::hear`). A palette that has stopped
/// listening has nobody to tell, and the key is still not the application's: the person
/// pressed the palette's shortcut.
fn say(watched: &Watched, word: Word) {
    let (Word::Activated(id) | Word::Deactivated(id)) = word;
    let Some((name, _)) = watched.names.iter().find(|(_, named)| *named == id) else {
        return;
    };
    let mut held = watched.held.lock().unwrap_or_else(PoisonError::into_inner);
    match word {
        Word::Activated(_) => {
            if held.pressed(name, Instant::now()) {
                let _ = watched.presses.send(id);
            }
        }
        Word::Deactivated(_) => held.released(name),
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use std::sync::mpsc::{self, TryRecvError};

    use super::*;
    use crate::hotkey::Hotkey;
    use crate::linux_ibus_rules::IBUS_RELEASE_MASK;

    const SHIFT_CTRL_ALT: u32 = (1 << 0) | (1 << 2) | (1 << 3);
    const KEY_N: u32 = 49;

    fn next_value() -> ShiftedChord {
        ShiftedChord::of(&Hotkey {
            id: HotkeyId(7),
            ctrl: true,
            alt: true,
            shift: false,
            win: false,
            vk: 0x4e,
        })
        .expect("Ctrl+Alt+N is a chord the Shift changes")
    }

    // One test, because the shortcuts stand in one place for the whole process and tests run
    // side by side.
    #[test]
    fn the_engine_tells_a_press_once_and_only_while_its_session_stands() {
        let (presses, heard) = mpsc::channel();
        let held = Arc::new(Mutex::new(Held::default()));
        let names = vec![("next-value".to_owned(), HotkeyId(7))];
        let watching = watch(
            "/s/1",
            vec![next_value()],
            names.clone(),
            Arc::clone(&held),
            presses.clone(),
        );
        let mut down = ShiftedChords::default();

        // The press under the Shift of the recipe is taken and told.
        assert!(meet(&mut down, 0x4e, KEY_N, SHIFT_CTRL_ALT, true));
        assert_eq!(heard.try_recv(), Ok(HotkeyId(7)));
        // Its repeat is taken and not told again, as the portal's repeats are not.
        assert!(meet(&mut down, 0x4e, KEY_N, SHIFT_CTRL_ALT, true));
        assert_eq!(heard.try_recv(), Err(TryRecvError::Empty));
        // Let go, and pressed again: a new press.
        assert!(meet(&mut down, 0x6e, KEY_N, IBUS_RELEASE_MASK, false));
        assert!(meet(&mut down, 0x4e, KEY_N, SHIFT_CTRL_ALT, true));
        assert_eq!(heard.try_recv(), Ok(HotkeyId(7)));
        assert!(meet(&mut down, 0x6e, KEY_N, IBUS_RELEASE_MASK, false));

        // A chord the portal announced is held for the engine too: the repeat it hands on while
        // the Shift of the recipe is down is not a press.
        assert!(
            held.lock()
                .expect("not poisoned")
                .pressed("next-value", Instant::now())
        );
        assert!(meet(&mut down, 0x4e, KEY_N, SHIFT_CTRL_ALT, true));
        assert_eq!(heard.try_recv(), Err(TryRecvError::Empty));
        assert!(meet(&mut down, 0x6e, KEY_N, IBUS_RELEASE_MASK, false));

        // A session that began after this one keeps its shortcuts when this one ends.
        let later = watch("/s/2", vec![next_value()], names, held, presses);
        drop(watching);
        assert!(meet(&mut down, 0x4e, KEY_N, SHIFT_CTRL_ALT, true));
        assert_eq!(heard.try_recv(), Ok(HotkeyId(7)));
        assert!(meet(&mut down, 0x6e, KEY_N, IBUS_RELEASE_MASK, false));

        // With no session the key is the application's, and nothing is told.
        drop(later);
        assert!(!meet(&mut down, 0x4e, KEY_N, SHIFT_CTRL_ALT, true));
        assert_eq!(heard.try_recv(), Err(TryRecvError::Disconnected));
    }
}
