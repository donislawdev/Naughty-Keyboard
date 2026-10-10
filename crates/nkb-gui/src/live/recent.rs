//! The recent values of the value window (`D106`): what a press leaves them
//! (nothing), what a choice in the window notes, and their save as the
//! palette closes. Apart from the loop in `live.rs`, so that the list is
//! found in one place and the file stays under the ceiling of `D113`.

use super::*;

/// What a press changes in the slot: the next value moves. The value that
/// went out does not become recent - the list holds what the tester chose
/// in the value window (`D106`), and a list that turned over at every
/// press would be gone before the tester came back to it.
pub(super) fn after_press(in_use: &InUse, outcome: &Outcome) {
    publish_next(in_use, outcome.upcoming.as_ref());
}

/// Carries out `command`, and notes a value chosen in the value window as
/// recent once it IS the next one - a pack that would not open, or a value
/// it no longer holds, left the sequence where it was and was not chosen.
/// The restart row is no choice of a value, so it is not noted.
pub(super) fn carry_out_noting(
    worker: &mut Worker<'_>,
    in_use: &InUse,
    command: Command,
) -> Carried {
    let chosen = match &command {
        Command::ChooseValue { pack, value } => Some(ValueKey {
            pack: pack.clone(),
            value: value.clone(),
        }),
        _ => None,
    };
    let carried = worker.carry_out(command);
    if let Some(chosen) = chosen {
        let sequence = &worker.sequence;
        let is_next = sequence.pack_id() == Some(chosen.pack.as_str())
            && next_id(sequence.upcoming().as_ref()).as_deref() == Some(chosen.value.as_str());
        if let (true, Ok(mut held)) = (is_next, in_use.lock()) {
            note_used(&mut held.recent, chosen);
        }
    }
    carried
}

/// Saves the recent values, once, as the palette closes - not after every
/// press, where a write between two presses could cost the next one (`W1`)
/// and the file would be rewritten all day (`settings-format.md` 4). A
/// process that is killed keeps the list it started with, as it keeps the
/// palette's place.
pub(super) fn keep_recent(memory: &Memory, in_use: &InUse) {
    // The palette is closing: a line about a failed save has nowhere to go.
    let _ = memory.keep(SettingChange::Recent(in_use_now(in_use).recent));
}
