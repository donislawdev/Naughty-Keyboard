//! The presses the palette takes for itself, in front of the sequence, and the
//! turn between two presses - where a command from the window, or a press the
//! palette took, ends the wait. Apart from the loop in `live.rs`, so that the
//! two are found apart and neither file grows past the ceiling of `D113`.

use nkb_app::ports::GivenShortcut;

use super::*;
use crate::on_request::MadeWay;
use nkb_adapters::i18n::OwnWindow;

/// Whether the loop goes on: no while the window is closing, and no while a
/// command waits - which is then kept in `pending` for [`drive`] to carry out.
///
/// The stop is read FIRST, so a palette that is closing leaves a command
/// unread rather than choosing a pack nobody will see - and drops a press
/// for another pack taken in the last wait, for the same reason.
pub(super) fn between_presses(
    stop: &AtomicBool,
    commands: &Receiver<Command>,
    pending: &Cell<Option<Command>>,
) -> bool {
    if stop.load(Ordering::Relaxed) {
        pending.set(None);
        return false;
    }
    // A press the palette took for itself may be waiting already (`D120`): it
    // goes first, and the window's commands wait their turn in the channel.
    let waiting = pending.take();
    if waiting.is_some() {
        pending.set(waiting);
        return false;
    }
    match commands.try_recv() {
        Ok(command) => {
            pending.set(Some(command));
            false
        }
        // Nothing waiting, or nobody left to send: the stop flag, not this
        // channel, says when the palette is closing.
        Err(_) => true,
    }
}

/// The palette's own shortcuts in front of the sequence.
///
/// `ToggleVisibility` and `OpenPacks` are about windows, not about the pack in
/// use, so they never reach `AdvanceSequence` - which would answer them with
/// `Unhandled`. Every other press passes through untouched, in order.
///
/// ⚠️ An intercepted press does not end the wait. The wait goes on for what is
/// left of it, and a zero wait - the drain after a send, `W1` - keeps draining.
/// Returning `Nothing` early would end that drain and leave a `NextValue`
/// pressed during the send in the queue, to be acted on afterwards: exactly the
/// queueing `ux-spec.md` 3 rejects.
pub(super) struct PaletteShortcuts<'a> {
    pub(super) inner: &'a dyn LiveShortcuts,
    /// What a `ToggleVisibility` press does. A closure rather than the window
    /// handle, so the interception can be tested without a window.
    pub(super) on_toggle: &'a dyn Fn(),
    /// What an `OpenPacks` press does - asks the main thread to open the pack
    /// window. A press during a send is answered after it, like a toggle: the
    /// window opens once the value is in, and no value is replayed.
    pub(super) on_open: &'a dyn Fn(),
    /// What a press taken here comes to later, as a command the worker carries
    /// out between presses: a `NextPack` or `PreviousPack` press leaves the
    /// step for the worker, which holds the pack in use (`D120`), answered
    /// after a send like the two above - and a press that types a value and
    /// may not go on leaves the sentence that says why ([`Command::NotSent`]).
    pub(super) later: &'a dyn Fn(Command),
    /// Asked before a press that types a value goes on: closes a window of
    /// the palette that holds the keyboard, so the value goes to the field the
    /// tester was in and not into that window (`OBS-269`), and answers whether
    /// the press may go on. Through the main thread, where the windows live.
    pub(super) make_way: &'a dyn Fn() -> MadeWay,
}

impl LiveShortcuts for PaletteShortcuts<'_> {
    fn outcomes(&self) -> &[(HotkeyAction, ShortcutRegistration)] {
        self.inner.outcomes()
    }

    fn given(&self) -> &[(HotkeyAction, GivenShortcut)] {
        self.inner.given()
    }

    fn next(&self, wait: Duration) -> Wait {
        let deadline = Instant::now() + wait;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.inner.next(left) {
                Wait::Pressed(HotkeyAction::ToggleVisibility) => (self.on_toggle)(),
                Wait::Pressed(HotkeyAction::OpenPacks) => (self.on_open)(),
                Wait::Pressed(HotkeyAction::NextPack) => {
                    (self.later)(Command::Pack(Direction::Next))
                }
                Wait::Pressed(HotkeyAction::PreviousPack) => {
                    (self.later)(Command::Pack(Direction::Previous));
                }
                Wait::Pressed(action) if action.types_a_value() => match (self.make_way)() {
                    MadeWay::Clear => return Wait::Pressed(action),
                    refused => (self.later)(Command::NotSent(refusal(refused))),
                },
                other => return other,
            }
        }
    }
}

/// How long a press that types a value waits for the main thread to close a
/// window of the palette that holds the keyboard (`OBS-269`): five times the
/// longest a hand-back reads the system for the field's window to come to the
/// front (400 ms on X11, `nkb-sys`).
pub(super) const MAKE_WAY_WAIT: Duration = Duration::from_secs(2);

/// Asks the main thread to close the windows of the palette that hold the
/// keyboard (`on_request::make_way_now`) and waits for its answer. No answer
/// in [`MAKE_WAY_WAIT`] - the event loop gone, or held up - sends nothing:
/// a window of ours may still have the keyboard.
pub(super) fn make_way_for_a_value() -> MadeWay {
    let (answer, answered) = std::sync::mpsc::channel();
    let asked = slint::invoke_from_event_loop(move || {
        let _ = answer.send(crate::on_request::make_way_now());
    });
    if asked.is_err() {
        return MadeWay::NoAnswer;
    }
    answered
        .recv_timeout(MAKE_WAY_WAIT)
        .unwrap_or(MadeWay::NoAnswer)
}

/// What the palette says for a press that may not go on.
const fn refusal(why: MadeWay) -> OwnWindow {
    match why {
        MadeWay::Choosing => OwnWindow::ChoosingFolder,
        MadeWay::Clear | MadeWay::NoAnswer => OwnWindow::DidNotClose,
    }
}

impl Worker<'_> {
    /// A press that types a value sent nothing (`OBS-269`): the palette says why.
    pub(super) fn not_sent(&self, why: OwnWindow) -> Carried {
        let line = nkb_adapters::i18n::nothing_sent_for(why).to_owned();
        Carried {
            view: Some(self.view(vec![line], ValueBand::Keep, None)),
            told: None,
            toggled: None,
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;
    use nkb_app::browse_packs::Direction;
    use std::cell::Cell;
    use std::sync::atomic::AtomicBool;
    use std::sync::mpsc;

    /// A waiting command ends the turn and is kept for `drive`. A closing
    /// window comes first and leaves the command unread. A channel nobody
    /// sends on any more is not a reason to stop.
    #[test]
    fn a_waiting_command_ends_the_turn_but_a_closing_window_comes_first() {
        let (send, commands) = mpsc::channel();
        let stop = AtomicBool::new(false);
        let pending = Cell::new(None);
        assert!(between_presses(&stop, &commands, &pending), "nothing waits");

        send.send(Command::Choose(String::from("x")))
            .expect("the receiver is alive");
        assert!(!between_presses(&stop, &commands, &pending));
        assert_eq!(pending.take(), Some(Command::Choose(String::from("x"))));

        send.send(Command::Choose(String::from("y")))
            .expect("the receiver is alive");
        stop.store(true, Ordering::Relaxed);
        assert!(!between_presses(&stop, &commands, &pending));
        assert_eq!(pending.take(), None, "a closing palette chose a pack");
        assert_eq!(commands.try_recv(), Ok(Command::Choose(String::from("y"))));

        stop.store(false, Ordering::Relaxed);
        drop(send);
        assert!(
            between_presses(&stop, &commands, &pending),
            "a channel nobody sends on stopped the palette"
        );
    }

    /// A press the palette took for another pack waits in `pending` (`D120`):
    /// the turn ends at once, the press is still there for `drive` to carry
    /// out, and a command from the window waits in the channel behind it. A
    /// closing palette drops it, as it leaves a command unread.
    #[test]
    fn a_press_for_another_pack_ends_the_turn_and_a_closing_window_drops_it() {
        let (send, commands) = mpsc::channel();
        let stop = AtomicBool::new(false);
        let pending = Cell::new(Some(Command::Pack(Direction::Next)));
        send.send(Command::ToggleCompact)
            .expect("the receiver is alive");
        assert!(
            !between_presses(&stop, &commands, &pending),
            "the turn went on"
        );
        assert_eq!(
            pending.take(),
            Some(Command::Pack(Direction::Next)),
            "the press for another pack was lost"
        );
        assert_eq!(
            commands.try_recv(),
            Ok(Command::ToggleCompact),
            "the window's command was taken in its place"
        );

        pending.set(Some(Command::Pack(Direction::Previous)));
        stop.store(true, Ordering::Relaxed);
        assert!(!between_presses(&stop, &commands, &pending));
        assert_eq!(pending.take(), None, "a closing palette stepped to a pack");
    }

    /// A queue of presses standing in for the system, for the interception test.
    struct Queued(std::cell::RefCell<std::collections::VecDeque<HotkeyAction>>);

    impl LiveShortcuts for Queued {
        fn outcomes(&self) -> &[(HotkeyAction, ShortcutRegistration)] {
            &[]
        }

        fn next(&self, _wait: Duration) -> Wait {
            self.0
                .borrow_mut()
                .pop_front()
                .map_or(Wait::Nothing, Wait::Pressed)
        }
    }

    /// `wired` is the sequence's answer plus the palette's own, and the
    /// palette's own are exactly what [`PaletteShortcuts`] takes in front of
    /// the sequence - so the hint bar can neither name a press the palette
    /// ignores nor miss one it answers.
    #[test]
    fn a_shortcut_is_wired_when_the_sequence_or_the_palette_answers_it() {
        for action in HotkeyAction::ALL {
            let queued = Queued(std::cell::RefCell::new([action].into()));
            let taken = std::cell::Cell::new(false);
            let on_own = || taken.set(true);
            let later = |_| taken.set(true);
            let shortcuts = PaletteShortcuts {
                inner: &queued,
                on_toggle: &on_own,
                on_open: &on_own,
                later: &later,
                make_way: &|| MadeWay::Clear,
            };
            let passed_on = shortcuts.next(Duration::ZERO) == Wait::Pressed(action);
            assert_eq!(
                taken.get(),
                crate::live::PALETTE_OWN.contains(&action),
                "{action:?}: the palette takes it in front of the sequence, or not"
            );
            assert_eq!(passed_on, !taken.get(), "{action:?}");
            assert_eq!(
                crate::live::wired(action),
                taken.get() || AdvanceSequence::handles(action),
                "{action:?}"
            );
        }
    }

    /// The palette's own shortcut never reaches the sequence, and taking it out
    /// does not end the drain after a send (`W1`).
    #[test]
    fn toggling_is_answered_by_the_palette_and_does_not_cut_the_drain_short() {
        let queued = Queued(std::cell::RefCell::new(
            [
                HotkeyAction::ToggleVisibility,
                HotkeyAction::OpenPacks,
                HotkeyAction::NextPack,
                HotkeyAction::NextValue,
                HotkeyAction::ToggleVisibility,
                HotkeyAction::PreviousPack,
            ]
            .into(),
        ));
        let toggles = std::cell::Cell::new(0);
        let on_toggle = || toggles.set(toggles.get() + 1);
        let opens = std::cell::Cell::new(0);
        let on_open = || opens.set(opens.get() + 1);
        let packs = std::cell::RefCell::new(Vec::new());
        let later = |command| packs.borrow_mut().push(command);
        let shortcuts = PaletteShortcuts {
            inner: &queued,
            on_toggle: &on_toggle,
            on_open: &on_open,
            later: &later,
            make_way: &|| MadeWay::Clear,
        };
        // A zero wait, as the drain after a send asks: the toggle in front must
        // not hide the NextValue behind it.
        assert_eq!(
            shortcuts.next(Duration::ZERO),
            Wait::Pressed(HotkeyAction::NextValue)
        );
        assert_eq!(shortcuts.next(Duration::ZERO), Wait::Nothing);
        assert_eq!(
            toggles.get(),
            2,
            "both toggles were answered, none was dropped"
        );
        assert_eq!(opens.get(), 1, "the pack window was asked for, once");
        assert_eq!(
            *packs.borrow(),
            vec![
                Command::Pack(Direction::Next),
                Command::Pack(Direction::Previous)
            ],
            "each press for another pack was answered, in its own direction"
        );
    }

    /// A press that types a value asks for the way first and goes on only
    /// when it is clear (`OBS-269`). One that could not be cleared sends
    /// nothing, says why, and the presses behind it are still answered. A
    /// press that types nothing never asks: walking the pack must not close
    /// the value window the tester walks it with.
    #[test]
    fn a_press_that_types_goes_on_only_once_no_window_of_ours_holds_the_keyboard() {
        let queued = Queued(std::cell::RefCell::new(
            [
                HotkeyAction::NextValue,
                HotkeyAction::SkipValue,
                HotkeyAction::PreviousValue,
                HotkeyAction::RepeatLast,
                HotkeyAction::CopyReport,
            ]
            .into(),
        ));
        let answers = std::cell::RefCell::new(std::collections::VecDeque::from([
            MadeWay::Clear,
            MadeWay::Choosing,
            MadeWay::NoAnswer,
        ]));
        let make_way = || {
            answers
                .borrow_mut()
                .pop_front()
                .expect("asked once for each press that types, and for no other")
        };
        let refused = std::cell::RefCell::new(Vec::new());
        let later = |command| refused.borrow_mut().push(command);
        let nothing = || {};
        let shortcuts = PaletteShortcuts {
            inner: &queued,
            on_toggle: &nothing,
            on_open: &nothing,
            later: &later,
            make_way: &make_way,
        };
        assert_eq!(
            shortcuts.next(Duration::ZERO),
            Wait::Pressed(HotkeyAction::NextValue)
        );
        assert_eq!(
            shortcuts.next(Duration::ZERO),
            Wait::Pressed(HotkeyAction::SkipValue)
        );
        assert_eq!(
            shortcuts.next(Duration::ZERO),
            Wait::Pressed(HotkeyAction::CopyReport),
            "a refused press reached the sequence, or cut the drain short"
        );
        assert_eq!(shortcuts.next(Duration::ZERO), Wait::Nothing);
        assert!(
            answers.borrow().is_empty(),
            "a press that types did not ask"
        );
        assert_eq!(
            *refused.borrow(),
            vec![
                Command::NotSent(OwnWindow::ChoosingFolder),
                Command::NotSent(OwnWindow::DidNotClose)
            ]
        );
    }
}
