//! The palette's Copy buttons (`D98`): a value on the clipboard for the tester
//! to paste by hand. Apart from the loop in `advance_sequence.rs`, which stands
//! at the edge of the band of `D113`.
//!
//! # Which day a copied date is of (`D133` 5)
//!
//! A value of type `next-date` names another date once its rule has moved on.
//! The button beside the next value copies what a press would type now, so the
//! date of today. The one beside the value sent last copies what went out, so
//! the date of the day it went out - the date its report block quotes, also
//! after midnight. Until 2026-10-15 both copied the date of today.
//!
//! Identifiers alone cannot tell the two apart: one value is the next one and
//! the last one sent at once in a pack of one value, or once it is chosen again
//! in the value window. So the button says which band it stands beside.

use std::sync::atomic::AtomicBool;

use nkb_core::calendar::Day;

use super::{AdvanceSequence, Message, Ports};
use crate::ports::{DeliveryError, TargetRef};

/// Which band a Copy button stands beside - what decides the day a value of
/// type `next-date` names when it is copied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyFrom {
    /// The next value: what a press would type now.
    Upcoming,
    /// The value sent last: what went out, on the day it went out.
    LastSent,
}

impl AdvanceSequence {
    /// Puts value `value` of pack `pack` on the clipboard, for the tester to
    /// paste by hand - the Copy buttons beside the next value and the last one
    /// sent (`UX-GUI-003`, `D98`). What the palette says about it comes back.
    ///
    /// Moves nothing: the counter, the next value and the value a report block
    /// describes stay as they were, because nothing reached a field.
    ///
    /// By identifiers, as [`Self::choose_next`] takes them, and for a sharper
    /// reason here: the button copies what the tester SAW, and a press handled
    /// between the click and this call may have moved the sequence since. An
    /// identifier names that value or none. A pack that is not the one in use
    /// any more names none, and nothing is copied rather than another value.
    ///
    /// 🔴 Through the clipboard ROUTE, never a door of its own (untouchable rule
    /// 17, `tests/clipboard_has_named_doors.rs`): the value stays out of the
    /// system's clipboard history and off the cloud like every value of
    /// clipboard mode (`D71`), and a value the clipboard cannot carry whole is
    /// refused by name before the clipboard is touched.
    ///
    /// `from` names the band the button stands beside, which decides the day
    /// a value of type `next-date` names (`D133` 5, the module's text).
    #[must_use]
    pub fn copy_value(
        &self,
        pack: &str,
        value: &str,
        from: CopyFrom,
        ports: &Ports<'_>,
    ) -> Message {
        let reference = format!("{pack}/{value}");
        let Some(held) = self
            .loaded
            .as_ref()
            .filter(|loaded| loaded.pack.id == pack)
            .and_then(|loaded| loaded.pack.values.iter().find(|held| held.id == value))
        else {
            return Message::CopyGone { reference };
        };
        let sent_on = match from {
            CopyFrom::LastSent => self.sent_on(value),
            CopyFrom::Upcoming => None,
        };
        // Measured from the recipe before anything is built, as a send measures
        // it. Unreachable for a pack that loaded, and the send's own sentence if
        // it ever is: the same recipe could not be sent either.
        let body = held
            .definition
            .on(sent_on.unwrap_or_else(|| self.clock.today()));
        let built = body.metrics().map(|_| body.materialise());
        let Some(Ok(text)) = built else {
            return Message::ValueTooLarge {
                id: held.id.clone(),
            };
        };
        // Meant for no window: the clipboard takes the value whatever the
        // target says, and the tester pastes it wherever they choose.
        match ports.by_clipboard.deliver(
            &text,
            TargetRef::UNNAMED,
            &AtomicBool::new(false),
            &mut |_| {},
        ) {
            Ok(_) => Message::ValueCopied { reference },
            Err(DeliveryError::Busy) => Message::CopyBusy,
            Err(DeliveryError::CannotCarry { character }) => Message::NotForClipboard {
                id: held.id.clone(),
                character,
            },
            Err(DeliveryError::Refused { detail }) => Message::CopyFailed { detail },
            // The clipboard route answers none of the others. Said in its own
            // words rather than swallowed, if it ever does.
            Err(other) => Message::CopyFailed {
                detail: other.to_string(),
            },
        }
    }

    /// The day value `value` of the pack in use went out on, while it is the
    /// value sent last. `None` once another one went out since - a press
    /// handled between the click and the copy - and the copy then names the
    /// date of today, as for any other value.
    fn sent_on(&self, value: &str) -> Option<Day> {
        let last = self.last.as_ref()?;
        let values = &self.loaded.as_ref()?.pack.values;
        let sent = values.get(last.index.checked_sub(1)?)?;
        (sent.id == value).then_some(last.on)
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
    use crate::advance_sequence::tests::{a_clock_change, on_day};
    use crate::test_support::*;
    use nkb_core::hotkeys::HotkeyAction;
    use nkb_core::pack::Risk;

    #[test]
    fn a_copy_takes_the_clipboard_route_with_the_value_named_and_moves_nothing() {
        let mut advance = chosen(Risk::Normal);
        let kit = Kit::ready();
        let _ = advance.on_action(HotkeyAction::NextValue, &kit.ports());
        let before = advance.sequence();
        let upcoming = advance.upcoming();

        // Value three, while value two is the next one: a copy names its value
        // and does not follow the sequence.
        let said = advance.copy_value("sample", "three", CopyFrom::Upcoming, &kit.ports());

        assert_eq!(
            said,
            Message::ValueCopied {
                reference: "sample/three".to_owned()
            }
        );
        assert_eq!(
            *kit.by_clipboard.handed.borrow(),
            vec!["gamma"],
            "the clipboard ROUTE took it - out of the history, like clipboard mode (D71)"
        );
        assert!(
            kit.clipboard.puts.borrow().is_empty(),
            "no door of its own to the clipboard"
        );
        assert_eq!(
            *kit.direct.handed.borrow(),
            vec!["alpha"],
            "nothing typed but the press before"
        );
        assert_eq!(
            *kit.keys.requests.borrow(),
            1,
            "only the clearing of the press before"
        );
        assert_eq!(advance.sequence(), before, "the counter did not move");
        assert_eq!(advance.upcoming(), upcoming, "nor did the next value");
        let report = advance.on_action(HotkeyAction::CopyReport, &kit.ports());
        assert_eq!(
            report.messages,
            vec![Message::ReportCopied {
                reference: "sample/one".to_owned()
            }],
            "the report block is still about the value that went out"
        );
    }

    #[test]
    fn a_copy_is_not_a_send_so_there_is_still_nothing_to_report() {
        // The next value is on screen before the first press (UX1), so it can
        // be copied then - and that does not make it a value that went out.
        let mut advance = chosen(Risk::Normal);
        let kit = Kit::ready();
        assert_eq!(
            advance.copy_value("sample", "one", CopyFrom::Upcoming, &kit.ports()),
            Message::ValueCopied {
                reference: "sample/one".to_owned()
            }
        );
        let outcome = advance.on_action(HotkeyAction::CopyReport, &kit.ports());
        assert_eq!(outcome.messages, vec![Message::NothingToReport]);
        assert_eq!(advance.counter(), Some((0, 3)));
    }

    #[test]
    fn a_value_not_in_the_pack_in_use_is_not_copied_and_no_other_is() {
        let advance = chosen(Risk::Normal);
        let kit = Kit::ready();
        assert_eq!(
            advance.copy_value("another", "one", CopyFrom::Upcoming, &kit.ports()),
            Message::CopyGone {
                reference: "another/one".to_owned()
            },
            "a value of the same name in the pack in use is not the value on screen"
        );
        assert_eq!(
            advance.copy_value("sample", "four", CopyFrom::Upcoming, &kit.ports()),
            Message::CopyGone {
                reference: "sample/four".to_owned()
            }
        );
        assert_eq!(
            AdvanceSequence::new(Box::new(StoppedClock::default())).copy_value(
                "sample",
                "one",
                CopyFrom::Upcoming,
                &kit.ports()
            ),
            Message::CopyGone {
                reference: "sample/one".to_owned()
            },
            "no pack at all"
        );
        assert!(kit.by_clipboard.handed.borrow().is_empty());
        assert!(kit.clipboard.puts.borrow().is_empty());
    }

    #[test]
    fn a_copy_the_clipboard_refuses_is_said_in_the_words_of_a_copy() {
        let advance = chosen(Risk::Normal);
        for (error, said) in [
            (DeliveryError::Busy, Message::CopyBusy),
            (
                DeliveryError::Refused {
                    detail: "no display".to_owned(),
                },
                Message::CopyFailed {
                    detail: "no display".to_owned(),
                },
            ),
            (
                DeliveryError::CannotCarry { character: '\0' },
                Message::NotForClipboard {
                    id: "two".to_owned(),
                    character: '\0',
                },
            ),
            (
                DeliveryError::NoTarget,
                Message::CopyFailed {
                    detail: "no-target".to_owned(),
                },
            ),
        ] {
            let kit = Kit::with_by_clipboard(FakeDelivery::failing(error));
            assert_eq!(
                advance.copy_value("sample", "two", CopyFrom::Upcoming, &kit.ports()),
                said
            );
        }
    }

    #[test]
    fn a_next_date_value_copied_beside_the_next_is_the_date_of_the_day_it_is_copied() {
        let mut advance = AdvanceSequence::new(Box::new(StoppedClock(on_day(2027, 1, 4))));
        let pack = a_pack(vec![a_clock_change("dst-ambiguous")], Risk::Normal);
        advance
            .choose_pack(&FakeSource, &FakeFormat::of(pack), "sample")
            .expect("a scripted pack with no errors loads");
        let kit = Kit::ready();
        let _ = advance.copy_value("sample", "dst-ambiguous", CopyFrom::Upcoming, &kit.ports());
        assert_eq!(*kit.by_clipboard.handed.borrow(), vec!["2027-10-31 02:30"]);
    }

    /// `D133` 5: one value of type `next-date`, copied beside both bands on the
    /// day its rule moved on - beside the value sent last the date that went
    /// out, beside the next value the date a press would type now.
    #[test]
    fn a_next_date_value_copied_beside_the_last_sent_is_the_date_that_went_out() {
        let clock = MovingClock::on(on_day(2026, 10, 15));
        let mut advance = AdvanceSequence::new(Box::new(clock.clone()));
        let pack = a_pack(vec![a_clock_change("dst-ambiguous")], Risk::Normal);
        advance
            .choose_pack(&FakeSource, &FakeFormat::of(pack), "sample")
            .expect("a scripted pack with no errors loads");
        let kit = Kit::ready();
        let _ = advance.on_action(HotkeyAction::NextValue, &kit.ports());
        assert_eq!(*kit.direct.handed.borrow(), vec!["2026-10-25 02:30"]);

        // From 24 October in UTC the change of 2026 is today somewhere, and
        // the date ahead is next year's.
        clock.set(on_day(2026, 10, 24));
        for from in [CopyFrom::LastSent, CopyFrom::Upcoming] {
            let _ = advance.copy_value("sample", "dst-ambiguous", from, &kit.ports());
        }
        assert_eq!(
            *kit.by_clipboard.handed.borrow(),
            vec!["2026-10-25 02:30", "2027-10-31 02:30"],
            "beside the last sent what went out, beside the next what goes out now"
        );
    }

    /// A press handled between the click and the copy sent another value: the
    /// day of the one clicked is not held any more, and it is copied as of
    /// today rather than with the day of the value that went out since.
    #[test]
    fn a_value_no_longer_the_last_sent_is_copied_as_of_today() {
        let clock = MovingClock::on(on_day(2026, 10, 15));
        let mut advance = AdvanceSequence::new(Box::new(clock.clone()));
        let pack = a_pack(
            vec![
                a_clock_change("dst-ambiguous"),
                a_value("plain", "text", None),
            ],
            Risk::Normal,
        );
        advance
            .choose_pack(&FakeSource, &FakeFormat::of(pack), "sample")
            .expect("a scripted pack with no errors loads");
        let kit = Kit::ready();
        let _ = advance.on_action(HotkeyAction::NextValue, &kit.ports());
        let _ = advance.on_action(HotkeyAction::NextValue, &kit.ports());

        clock.set(on_day(2026, 10, 24));
        let _ = advance.copy_value("sample", "dst-ambiguous", CopyFrom::LastSent, &kit.ports());
        let _ = advance.copy_value("sample", "plain", CopyFrom::LastSent, &kit.ports());
        assert_eq!(
            *kit.by_clipboard.handed.borrow(),
            vec!["2027-10-31 02:30", "text"]
        );
    }
}
