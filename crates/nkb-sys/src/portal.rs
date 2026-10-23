//! Global shortcuts asked of the portal of the desktop, on a Linux session that has no X server
//! to grab keys on (`D145` points 39 and 53, `OBS-217`, `OBS-222`).
//!
//! # Why this is not `hotkey`
//!
//! A grab (`hotkey`) is a chord the program names and the server holds or refuses. Here it is
//! the other way round. The program asks the desktop for shortcuts and offers a chord for
//! each, the person chooses in a window of the system, and the desktop tells the program what
//! it did bind, in words: `Press <Control><Alt>n`. The program never learns that a chord is
//! taken, and a chord the portal accepted can belong to the system and never fire
//! (`OBS-217` point 5). So the answer of this route is the shortcuts the person was given and
//! the presses that arrive, and it claims nothing else.
//!
//! The desktop also wants to know who is asking. A program that is no application of it has no
//! identifier, and the identifier comes from an entry (`<id>.desktop`) in a directory of
//! applications. Making that entry is a change to the person's home that nothing in this
//! program does by itself: [`write_entry`] is called when the person asks for it, and
//! [`start`] without an entry says so ([`Unavailable::NeedsEntry`]) and does nothing else.
//!
//! The route is a bus of this machine and nothing else (`linux/bus.rs`).

use std::sync::mpsc::Receiver;

use crate::hotkey::{Hotkey, HotkeyId};

/// A shortcut the program asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wish {
    /// The chord offered. The person may choose another, and `id` is what comes back with a
    /// press of whatever they chose.
    pub hotkey: Hotkey,
    /// What the shortcut does, in the words the window of the system and the settings of the
    /// desktop show beside it.
    pub description: String,
}

/// A shortcut the portal says it holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bound {
    pub id: HotkeyId,
    /// The words the desktop gave for the chord, as they came, in the language of the person.
    pub words: String,
    /// The chord in those words, when they are one of the vocabulary of the palette. `None`
    /// is a chord the palette has no name for, and `words` are then what can be shown.
    pub chord: Option<Hotkey>,
}

/// Why no shortcut was bound. Named, never a shrug (untouchable rule 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unavailable {
    /// This system has no portal of this kind.
    NotHere,
    /// There is no session bus to ask: no address, or one that names something other than a
    /// file socket of this machine.
    NoBus,
    /// The bus did not accept the connection or stopped answering.
    Unreachable,
    /// The desktop has no portal for global shortcuts.
    NoPortal,
    /// The program has no entry that gives it an identifier, so the desktop does not know it.
    NeedsEntry,
    /// The person closed the window of the system without agreeing.
    Cancelled,
    /// The portal ended the request, with the code it gave.
    Ended(u32),
    /// The portal answered in a way its own specification does not allow.
    Odd,
}

impl core::fmt::Display for Unavailable {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NotHere => f.write_str("this system has no portal for global shortcuts"),
            Self::NoBus => f.write_str("there is no session bus of this machine to ask"),
            Self::Unreachable => f.write_str("the session bus did not answer"),
            Self::NoPortal => f.write_str("the desktop has no portal for global shortcuts"),
            Self::NeedsEntry => f.write_str("the desktop does not know this program yet"),
            Self::Cancelled => f.write_str("the window of the system was closed without agreeing"),
            Self::Ended(code) => write!(f, "the portal ended the request with code {code}"),
            Self::Odd => {
                f.write_str("the portal answered in a way its specification does not allow")
            }
        }
    }
}

/// The shortcuts the person was given, the presses of them, and the handle that keeps it so.
pub struct Started {
    /// The shortcuts the portal holds. A wish that is not here was not given.
    pub bound: Vec<Bound>,
    /// The identifier of each shortcut as it is pressed.
    pub fired: Receiver<HotkeyId>,
    pub listener: PortalListener,
}

/// Owns the connection and the thread that hears the presses. Closes the session of
/// shortcuts when it goes, so that none stays bound to a program that has ended.
pub struct PortalListener {
    inner: platform::Listener,
}

impl PortalListener {
    /// Ends the session and the thread, now.
    pub fn stop(self) {
        self.inner.stop();
    }
}

/// Where an entry for this program was made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Written {
    /// It was made now, at this path.
    Made(String),
    /// An entry was there already, this program's or another's, and was left as it is.
    AlreadyThere,
}

/// What taking the entry away came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Removed {
    /// The entry this program made was removed.
    Removed,
    /// There was no entry in the person's directory.
    NotThere,
    /// There is a file of that name that this program did not make, and it was left.
    NotOurs,
}

/// Why no entry was made or taken away.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryError {
    /// This system has no such entries.
    NotHere,
    /// The environment names no directory for the person's applications.
    NoPlace,
    /// The program cannot say where it is, as a path that can be written down.
    NoProgram,
    /// The directory or the file could not be written.
    Io,
}

impl core::fmt::Display for EntryError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NotHere => f.write_str("this system has no entries of applications"),
            Self::NoPlace => f.write_str("there is no directory for applications of this person"),
            Self::NoProgram => f.write_str("the place of this program cannot be written down"),
            Self::Io => f.write_str("the entry could not be written"),
        }
    }
}

/// Asks the portal for `wishes` and listens for them.
///
/// Blocks until the portal has answered the bind, which is at once for a program the person
/// has agreed to before and as long as the person takes to read the window of the system for
/// one they have not (at most five minutes).
///
/// # Errors
///
/// [`Unavailable`] with the reason nothing was bound.
pub fn start(wishes: &[Wish]) -> Result<Started, Unavailable> {
    platform::start(wishes)
}

/// Whether the desktop can find an entry that gives this program an identifier.
#[must_use]
pub fn entry_present() -> bool {
    platform::entry_present()
}

/// Makes the entry that gives this program an identifier, in the person's own directory of
/// applications, unless one is there. Written whole to a file of its own and put in place
/// without replacing anything, so an interrupted write leaves nothing and a file somebody else
/// made is never touched.
///
/// # Errors
///
/// [`EntryError`] with the reason no entry was made.
pub fn write_entry() -> Result<Written, EntryError> {
    platform::write_entry()
}

/// Takes away the entry [`write_entry`] made, and only that one. The desktop forgets the
/// program the next time it looks, and the shortcuts the person chose stay with the desktop
/// until they remove them in its settings.
///
/// # Errors
///
/// [`EntryError`] with the reason nothing was done.
pub fn remove_entry() -> Result<Removed, EntryError> {
    platform::remove_entry()
}

#[cfg(target_os = "linux")]
mod platform {
    use super::{EntryError, PortalListener, Removed, Started, Unavailable, Wish, Written};
    use crate::linux::portal;

    pub use crate::linux::portal::Listener;

    pub fn start(wishes: &[Wish]) -> Result<Started, Unavailable> {
        let (bound, fired, inner) = portal::start(wishes)?;
        Ok(Started {
            bound,
            fired,
            listener: PortalListener { inner },
        })
    }

    pub fn entry_present() -> bool {
        portal::entry::present()
    }

    pub fn write_entry() -> Result<Written, EntryError> {
        portal::entry::write()
    }

    pub fn remove_entry() -> Result<Removed, EntryError> {
        portal::entry::remove()
    }
}

#[cfg(not(target_os = "linux"))]
mod platform {
    use super::{EntryError, Removed, Started, Unavailable, Wish, Written};

    pub struct Listener;

    impl Listener {
        pub fn stop(self) {}
    }

    pub fn start(_wishes: &[Wish]) -> Result<Started, Unavailable> {
        Err(Unavailable::NotHere)
    }

    pub fn entry_present() -> bool {
        false
    }

    pub fn write_entry() -> Result<Written, EntryError> {
        Err(EntryError::NotHere)
    }

    pub fn remove_entry() -> Result<Removed, EntryError> {
        Err(EntryError::NotHere)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(not(target_os = "linux"))]
    fn a_system_without_the_portal_says_so_and_makes_nothing() {
        assert_eq!(start(&[]).err(), Some(Unavailable::NotHere));
        assert!(!entry_present());
        assert_eq!(write_entry(), Err(EntryError::NotHere));
        assert_eq!(remove_entry(), Err(EntryError::NotHere));
    }

    #[test]
    fn every_reason_has_words_of_its_own() {
        let reasons = [
            Unavailable::NotHere,
            Unavailable::NoBus,
            Unavailable::Unreachable,
            Unavailable::NoPortal,
            Unavailable::NeedsEntry,
            Unavailable::Cancelled,
            Unavailable::Ended(2),
            Unavailable::Odd,
        ];
        let words: Vec<String> = reasons.iter().map(ToString::to_string).collect();
        for (index, one) in words.iter().enumerate() {
            assert!(!one.is_empty());
            for other in &words[index + 1..] {
                assert_ne!(one, other, "two reasons in the same words");
            }
        }
        assert!(Unavailable::Ended(77).to_string().contains("77"));
    }

    #[test]
    fn every_entry_error_has_words_of_its_own() {
        let words: Vec<String> = [
            EntryError::NotHere,
            EntryError::NoPlace,
            EntryError::NoProgram,
            EntryError::Io,
        ]
        .iter()
        .map(ToString::to_string)
        .collect();
        for (index, one) in words.iter().enumerate() {
            for other in &words[index + 1..] {
                assert_ne!(one, other);
            }
        }
    }
}
