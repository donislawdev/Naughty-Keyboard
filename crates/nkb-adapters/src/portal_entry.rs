//! The entry that tells the desktop which program is asking for shortcuts, through
//! `nkb_sys::portal` (`D145` points 54 and 68).
//!
//! A thin adapter, as `PortalShortcuts` next door is: one port becomes one call into
//! `nkb-sys`, and the two vocabularies meet in the two functions that map an answer.
//! What the entry is, where it goes and why a file somebody else made is never replaced
//! are decided there (`linux/portal/entry.rs`). Nothing here runs by itself: a call is
//! the answer to a click.

use nkb_app::ports::{EntryNews, EntryRefusal, ProgramEntry};
use nkb_sys::portal::{self, EntryError, Removed, Written};

/// Looks after the entry of this program in the person's own folder of applications.
///
/// Stateless: the state is the file.
#[derive(Debug, Default, Clone, Copy)]
pub struct PortalEntry;

impl ProgramEntry for PortalEntry {
    fn ours(&self) -> bool {
        portal::entry_ours()
    }

    fn make(&self) -> EntryNews {
        made(portal::write_entry())
    }

    fn take_away(&self) -> EntryNews {
        taken(portal::remove_entry())
    }
}

/// What making the entry came to, in the port's words.
fn made(result: Result<Written, EntryError>) -> EntryNews {
    match result {
        Ok(Written::Made(place)) => EntryNews::Made(place),
        Ok(Written::AlreadyThere) => EntryNews::AlreadyThere,
        Err(error) => EntryNews::Refused(refusal(error)),
    }
}

/// What taking the entry away came to, in the port's words.
fn taken(result: Result<Removed, EntryError>) -> EntryNews {
    match result {
        Ok(Removed::Removed) => EntryNews::TakenAway,
        Ok(Removed::NotThere) => EntryNews::NotThere,
        Ok(Removed::NotOurs) => EntryNews::NotOurs,
        Err(error) => EntryNews::Refused(refusal(error)),
    }
}

/// Why nothing was done. A system with no such entries and an account with no folder
/// for them are one thing to the person, who can do nothing about either.
const fn refusal(error: EntryError) -> EntryRefusal {
    match error {
        EntryError::NotHere | EntryError::NoPlace => EntryRefusal::NoPlace,
        EntryError::NoProgram => EntryRefusal::NoProgram,
        EntryError::Io => EntryRefusal::Io,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every answer of `nkb-sys` has an answer of the port, and a file somebody else
    /// made is told apart from one that was not there.
    #[test]
    fn what_the_system_answered_to_a_making_is_told_in_the_ports_words() {
        assert_eq!(
            made(Ok(Written::Made(String::from("/home/a/x.desktop")))),
            EntryNews::Made(String::from("/home/a/x.desktop"))
        );
        assert_eq!(made(Ok(Written::AlreadyThere)), EntryNews::AlreadyThere);
        assert_eq!(
            made(Err(EntryError::Io)),
            EntryNews::Refused(EntryRefusal::Io)
        );
    }

    #[test]
    fn what_the_system_answered_to_a_taking_away_is_told_in_the_ports_words() {
        assert_eq!(taken(Ok(Removed::Removed)), EntryNews::TakenAway);
        assert_eq!(taken(Ok(Removed::NotThere)), EntryNews::NotThere);
        assert_eq!(taken(Ok(Removed::NotOurs)), EntryNews::NotOurs);
        assert_eq!(
            taken(Err(EntryError::NoProgram)),
            EntryNews::Refused(EntryRefusal::NoProgram)
        );
    }

    /// A system without such entries and an account without a folder for them are
    /// said as one, and the two other reasons keep their own.
    #[test]
    fn the_reasons_nothing_was_done_stay_apart_where_the_person_can_act_on_them() {
        assert_eq!(refusal(EntryError::NotHere), EntryRefusal::NoPlace);
        assert_eq!(refusal(EntryError::NoPlace), EntryRefusal::NoPlace);
        assert_eq!(refusal(EntryError::NoProgram), EntryRefusal::NoProgram);
        assert_eq!(refusal(EntryError::Io), EntryRefusal::Io);
    }
}
