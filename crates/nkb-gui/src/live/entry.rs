//! What the palette does when the tester clicks a link that makes the entry the
//! desktop wants, or takes it away (`D145` point 68). Kept apart from the loop so that
//! `live.rs` stays under the ceiling of `D113`.
//!
//! # Only a click does this
//!
//! The entry is a file in the tester's own folder, so nothing here runs by itself
//! (untouchable rule 2): the palette offers it in a sentence and a link, and the
//! worker carries out the click between presses like every command. A click that
//! comes late - the offer is gone, the entry is not ours - changes nothing and draws
//! nothing, because the view the tester is looking at has already moved on.

use nkb_app::ports::EntryNews;

use super::*;

impl Worker<'_> {
    /// The view that says what a click on a link came to.
    ///
    /// A made entry is only as good as the desktop's answer to it, so the shortcuts are
    /// asked for again at once, and the palette says it waits ([`Worker::resume_after`]).
    /// Anything else leaves the hold as the hold made it: a refusal changed nothing, and
    /// an entry taken away has let the shortcuts go (`Hold::remove_entry`).
    pub(super) fn entry_news(&mut self, news: Option<EntryNews>, made: bool) -> Carried {
        let view = news.map(|news| {
            let said = vec![i18n::entry_news(&news)];
            if made && matches!(news, EntryNews::Made(_) | EntryNews::AlreadyThere) {
                self.hold.say_on_answer(said.clone());
                self.resume_after(said)
            } else {
                self.answered(said)
            }
        });
        Carried {
            view,
            told: None,
            toggled: None,
        }
    }
}
