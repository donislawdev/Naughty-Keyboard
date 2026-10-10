//! The palette's Copy buttons (`D98`) and the report block's (`D121`): what a
//! click asks the worker, what the worker answers, and the word "Copied" a
//! button says for two seconds after its copy went through - the palette's one
//! clock, the owner's point 7. Apart from the loop in `live.rs`, which stands
//! at the ceiling of `D113`.

use slint::ComponentHandle;

use super::*;
use crate::CopiedButton;

/// Which Copy button a click came from: the word "Copied" goes back to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyButton {
    /// Beside the next value.
    Next,
    /// Beside the value sent last.
    Last,
    /// Beside the report block.
    Report,
}

/// How long a button says "Copied" (the owner's point 7): long enough to be
/// read after the click, short enough to be about this click.
pub(super) const COPIED_FOR: Duration = Duration::from_secs(2);

/// The words of the three Copy buttons and of the report block around the
/// last one (`D98`, `D121`), set once: the same word on every button, and what
/// each does in UI Automation's words.
pub fn label_copies(palette: &Palette) {
    palette.set_copy_label(i18n::label(PaletteLabel::Copy).into());
    palette.set_copy_next_action(i18n::label(PaletteLabel::CopyNext).into());
    palette.set_copy_last_action(i18n::label(PaletteLabel::CopyLast).into());
    palette.set_copied_label(i18n::label(PaletteLabel::Copied).into());
    palette.set_report_label(i18n::label(PaletteLabel::ReportBlock).into());
    palette.set_report_action(i18n::label(PaletteLabel::ReportBlockAction).into());
    palette.set_copy_report_action(i18n::label(PaletteLabel::CopyReport).into());
}

/// What a click on a value's Copy button asks the worker for, from the key the
/// palette holds beside the band it stands in (`D98`). On the MAIN thread.
///
/// `None` for a key never set - the button stands only beside a value, so it
/// cannot be reached then, and an empty identifier is said as nothing rather
/// than sent to be refused.
#[must_use]
pub fn copy_command(shown: &ShownKey, button: CopyButton) -> Option<Command> {
    if shown.pack.is_empty() || shown.value.is_empty() {
        return None;
    }
    Some(Command::Copy(
        ValueKey {
            pack: shown.pack.to_string(),
            value: shown.value.to_string(),
        },
        button,
    ))
}

/// The button that says "Copied" after the worker's answer: the one clicked,
/// and only when the clipboard took what it was asked for. A value gone, a
/// clipboard held by somebody else or a copy refused keeps the word, and the
/// message band says why (untouchable rule 1).
#[must_use]
pub(super) fn copied_by(said: &[Message], button: CopyButton) -> Option<CopyButton> {
    said.iter()
        .any(|message| {
            matches!(
                message,
                Message::ValueCopied { .. } | Message::ReportCopied { .. }
            )
        })
        .then_some(button)
}

impl Worker<'_> {
    /// A value's Copy (`D98`): the value by its identifiers, through the
    /// clipboard route, answered in the message band - and with "Copied" on
    /// the button when the clipboard took it.
    pub(super) fn copy(&self, key: &ValueKey, button: CopyButton) -> Carried {
        let said = self.sequence.copy_value(&key.pack, &key.value, self.ports);
        let copied = copied_by(std::slice::from_ref(&said), button);
        let line = i18n::message(&said, &key.pack, &self.memory.bindings.get());
        Carried {
            view: Some(View {
                copied,
                ..self.view(vec![line], ValueBand::Keep, None)
            }),
            told: None,
            toggled: None,
        }
    }

    /// The report block's Copy (`D121`): the action the report shortcut is, so
    /// the block reaches the clipboard through its one door, stays in the
    /// clipboard history (`D68`) and is answered in the shortcut's sentences.
    pub(super) fn copy_report(&mut self) -> Carried {
        let outcome = self
            .sequence
            .on_action(HotkeyAction::CopyReport, self.ports);
        let mut carried = self.saying(&outcome.messages);
        if let Some(view) = carried.view.as_mut() {
            view.copied = copied_by(&outcome.messages, CopyButton::Report);
        }
        carried
    }
}

thread_local! {
    /// The clock that takes "Copied" back - on the MAIN thread, the one that
    /// touches the window, and one for the whole palette, so a second copy
    /// starts it over and only one button says the word.
    static COPIED: slint::Timer = slint::Timer::default();
}

/// Runs on the MAIN thread, with every view of the worker's: the button it
/// names says "Copied" for [`COPIED_FOR`], and a view that names none takes
/// the word back at once - a newer turn of the palette, a send or a copy that
/// failed, is what it is about now (`D121`).
pub(super) fn show_copied(palette: &Palette, copied: Option<CopyButton>) {
    show_copied_for(palette, copied, COPIED_FOR);
}

/// [`show_copied`] with the clock's length given, so a test waits a moment
/// rather than two seconds.
pub(super) fn show_copied_for(palette: &Palette, copied: Option<CopyButton>, how_long: Duration) {
    palette.set_copied(match copied {
        None => CopiedButton::None,
        Some(CopyButton::Next) => CopiedButton::Next,
        Some(CopyButton::Last) => CopiedButton::Last,
        Some(CopyButton::Report) => CopiedButton::Report,
    });
    COPIED.with(|clock| match copied {
        Some(_) => {
            let palette = palette.as_weak();
            clock.start(slint::TimerMode::SingleShot, how_long, move || {
                if let Some(palette) = palette.upgrade() {
                    palette.set_copied(CopiedButton::None);
                }
            });
        }
        None => clock.stop(),
    });
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;
    use crate::live::tests::{Registrar, a_palette, a_store, an_outcome, quiet, with_worker};

    fn key(pack: &str, value: &str) -> ValueKey {
        ValueKey {
            pack: pack.to_owned(),
            value: value.to_owned(),
        }
    }

    #[test]
    fn a_click_on_a_copy_button_asks_for_the_key_it_holds_and_an_empty_one_asks_nothing() {
        assert_eq!(copy_command(&ShownKey::default(), CopyButton::Next), None);
        assert_eq!(
            copy_command(
                &ShownKey {
                    pack: "whitespace".into(),
                    value: "".into(),
                },
                CopyButton::Last
            ),
            None
        );
        assert_eq!(
            copy_command(
                &ShownKey {
                    pack: "whitespace".into(),
                    value: "nbsp".into(),
                },
                CopyButton::Last
            ),
            Some(Command::Copy(key("whitespace", "nbsp"), CopyButton::Last))
        );
    }

    /// `D121`: "Copied" only when the clipboard took the value or the block,
    /// and on the button clicked.
    #[test]
    fn copied_is_said_on_the_button_clicked_and_only_when_the_clipboard_took_it() {
        let reference = String::from("whitespace/nbsp");
        let took = [
            Message::ValueCopied {
                reference: reference.clone(),
            },
            Message::ReportCopied {
                reference: reference.clone(),
            },
        ];
        for message in took {
            for button in [CopyButton::Next, CopyButton::Last, CopyButton::Report] {
                assert_eq!(
                    copied_by(std::slice::from_ref(&message), button),
                    Some(button)
                );
            }
        }
        let refused = [
            Message::CopyGone { reference },
            Message::CopyBusy,
            Message::CopyFailed {
                detail: String::from("no"),
            },
            Message::NothingToReport,
            Message::ReportBusy,
        ];
        for message in refused {
            assert_eq!(
                copied_by(std::slice::from_ref(&message), CopyButton::Next),
                None,
                "{message:?}"
            );
        }
        assert_eq!(copied_by(&[], CopyButton::Report), None);
    }

    /// Nothing here reaches a clipboard: the tests carry the REAL one, which no
    /// test may write while the owner works on this machine. The copy that
    /// writes is tested with fakes in `nkb-app` and on the live palette by
    /// `tools/petla-palety.ps1`.
    #[test]
    fn a_copy_is_answered_in_the_message_band_and_moves_nothing() {
        let store = a_store(false);
        let registrar = Registrar::default();
        with_worker(&store, &registrar, false, |worker, _| {
            let before = worker.sequence.sequence();
            let carried =
                worker.carry_out(Command::Copy(key("unicode-text", "nbsp"), CopyButton::Next));
            let view = carried.view.expect("a copy is answered");
            assert_eq!(
                view.messages,
                vec![String::from(
                    "unicode-text/nbsp is not in the pack in use any more, so nothing was copied. \
                     Click Copy beside the value on screen now."
                )]
            );
            assert_eq!(view.copied, None, "a copy that did not happen said Copied");
            assert!(matches!(view.value, ValueBand::Keep));
            assert!(carried.told.is_none());
            assert_eq!(worker.sequence.sequence(), before);
        });
    }

    /// The report block's Copy goes the shortcut's way: before a value went
    /// out it writes nothing and says so in the shortcut's words.
    #[test]
    fn the_report_block_copy_answers_as_the_report_shortcut() {
        let store = a_store(false);
        let registrar = Registrar::default();
        with_worker(&store, &registrar, false, |worker, _| {
            let carried = worker.carry_out(Command::CopyReport);
            let view = carried.view.expect("a report is answered");
            assert_eq!(view.messages.len(), 1);
            assert!(
                view.messages[0].starts_with("Nothing has been sent yet"),
                "{:?}",
                view.messages
            );
            assert_eq!(view.copied, None);
        });
    }

    /// The view carries the word to the window: `apply` puts it on the button
    /// the view names, and the next view, which names none, takes it back.
    #[test]
    fn a_view_from_the_worker_puts_copied_on_its_button() {
        let palette = a_palette();
        let bindings = nkb_adapters::default_bindings();
        let outcome = an_outcome(None, Vec::new());
        let mut view = view_of(&outcome, "whitespace", "whitespace", &quiet(), &bindings);
        view.copied = Some(CopyButton::Last);
        apply(&palette, view);
        assert_eq!(palette.get_copied(), CopiedButton::Last);
        let next = view_of(&outcome, "whitespace", "whitespace", &quiet(), &bindings);
        apply(&palette, next);
        assert_eq!(palette.get_copied(), CopiedButton::None);
    }

    /// The word comes on the button named and goes when the clock runs out, a
    /// second copy moves it, and a view that names no button takes it back.
    #[test]
    fn copied_stands_on_one_button_until_the_clock_or_a_newer_view_takes_it() {
        let palette = a_palette();
        let moment = Duration::from_millis(30);
        show_copied_for(&palette, Some(CopyButton::Next), moment);
        assert_eq!(palette.get_copied(), CopiedButton::Next);
        show_copied_for(&palette, Some(CopyButton::Report), moment);
        assert_eq!(
            palette.get_copied(),
            CopiedButton::Report,
            "one button at a time"
        );
        slint::platform::update_timers_and_animations();
        assert_eq!(
            palette.get_copied(),
            CopiedButton::Report,
            "before the clock"
        );
        std::thread::sleep(moment * 3);
        slint::platform::update_timers_and_animations();
        assert_eq!(palette.get_copied(), CopiedButton::None, "after the clock");

        show_copied_for(&palette, Some(CopyButton::Last), Duration::from_secs(60));
        show_copied(&palette, None);
        assert_eq!(palette.get_copied(), CopiedButton::None, "a newer view");
        assert_eq!(
            COPIED_FOR,
            Duration::from_secs(2),
            "the owner's two seconds"
        );
    }
}
