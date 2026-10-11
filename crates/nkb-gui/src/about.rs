//! The About window on the main thread: opened by a click on the palette's
//! link, closed by Esc, its Close button, the window's close button or by
//! leaving it (the owner's point 4 of 2026-10-10).
//!
//! # The shortcuts window's ways, without the pause
//!
//! Nothing here records a key, so the palette keeps its shortcuts while the
//! window is open. The keyboard is taken as it opens, so Esc closes it, and
//! handed back when it closes, to the window the tester was in - the pack
//! window's reason: the next press of a shortcut belongs in the field under
//! test. Leaving for another window closes it without handing the keyboard
//! back, because it is where the tester put it (`OBS-147`).
//!
//! # Where the facts come from
//!
//! The package manifest - the version, the author, the licence, the two
//! addresses - and the build, which says the year the copyright names
//! (`crates/nkb-cli/file_details.rs`). The same build puts the same words into
//! the file's details, and `tests/file_details.rs` holds [`facts`] and
//! [`copyright`] to what the built `nkb-gui.exe` carries.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Instant;

use nkb_adapters::i18n::{self, AboutLabel, Environment, Startup};
use slint::{ComponentHandle, ModelRc, VecModel};

use crate::focus::{HANDLE_WAIT, POLL, window_handle};
use crate::packs::{Keyboard, build_before_the_loop, forget_held_modifiers};
use crate::shortcut_list::{Key, key_of};
use crate::{AboutWindow, Fact};

/// The facts the window lists, each a label and its value, in their order.
#[must_use]
pub fn facts() -> Vec<(&'static str, String)> {
    vec![
        (
            i18n::about_label(AboutLabel::Version),
            env!("CARGO_PKG_VERSION").to_owned(),
        ),
        (i18n::about_label(AboutLabel::Author), author()),
        (
            i18n::about_label(AboutLabel::Licence),
            env!("CARGO_PKG_LICENSE").to_owned(),
        ),
        (
            i18n::about_label(AboutLabel::Website),
            shown_address(env!("CARGO_PKG_HOMEPAGE")),
        ),
        (
            i18n::about_label(AboutLabel::Source),
            shown_address(env!("CARGO_PKG_REPOSITORY")),
        ),
    ]
}

/// The copyright line: the year the build names and the author.
#[must_use]
pub fn copyright() -> String {
    i18n::about_copyright(env!("NKB_FIRST_YEAR"), &author())
}

/// The manifest's authors, as one line.
fn author() -> String {
    env!("CARGO_PKG_AUTHORS").replace(':', ", ")
}

/// An address as a tester types it: without the scheme, which a browser adds
/// by itself, and without the closing slash.
#[must_use]
pub fn shown_address(address: &str) -> String {
    let bare = address
        .strip_prefix("https://")
        .or_else(|| address.strip_prefix("http://"))
        .unwrap_or(address);
    bare.strip_suffix('/').unwrap_or(bare).to_owned()
}

/// The About window, labelled once.
pub struct About {
    window: AboutWindow,
    open: Cell<bool>,
    keyboard: Box<dyn Keyboard>,
    /// Puts a line in the palette's message band.
    say: Box<dyn Fn(String)>,
}

impl About {
    /// The window, labelled and wired, not yet shown.
    #[must_use]
    pub fn new(
        window: AboutWindow,
        keyboard: Box<dyn Keyboard>,
        say: Box<dyn Fn(String)>,
    ) -> Rc<Self> {
        label(&window);
        // After the words, which are part of what the layout measures - the
        // pack window's reason (`OBS-154`).
        build_before_the_loop(window.window());
        let about = Rc::new(Self {
            window,
            open: Cell::new(false),
            keyboard,
            say,
        });
        let weak = Rc::downgrade(&about);
        about
            .window
            .on_pressed(move |text, _control, _alt, _shift, _meta| {
                weak.upgrade()
                    .is_some_and(|about| about.press(text.as_str()))
            });
        let weak = Rc::downgrade(&about);
        about.window.on_close_clicked(move || {
            if let Some(about) = weak.upgrade() {
                about.close();
            }
        });
        let weak = Rc::downgrade(&about);
        about.window.on_deactivated(move || {
            if let Some(about) = weak.upgrade() {
                about.left();
            }
        });
        let weak = Rc::downgrade(&about);
        about.window.on_activated(move || {
            if let Some(about) = weak.upgrade() {
                forget_held_modifiers(about.window.window());
            }
        });
        let weak = Rc::downgrade(&about);
        about.window.window().on_close_requested(move || {
            if let Some(about) = weak.upgrade() {
                about.close();
            }
            // Hidden already, after the keyboard went back - see `close`.
            slint::CloseRequestResponse::KeepWindowShown
        });
        about
    }

    /// Whether the window is open.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.open.get()
    }

    /// The window, for a test that reads what it shows.
    #[must_use]
    pub fn window(&self) -> &AboutWindow {
        &self.window
    }

    /// Opens the window with the keyboard in it. Asked again while open, it
    /// asks for the keyboard again. A window that does not open says so in
    /// the palette.
    pub fn open(self: &Rc<Self>) {
        if !self.is_open() {
            // Before the window shows: afterwards the answer is the window.
            self.keyboard.remember();
            if let Err(error) = self.window.show() {
                (self.say)(i18n::startup_failure(
                    Startup::WindowFailed,
                    &error.to_string(),
                ));
                return;
            }
            self.open.set(true);
        }
        self.window.invoke_focus_keys();
        self.take_keyboard(Instant::now());
    }

    /// Hands the keyboard back and hides the window, in that order, for the
    /// reason `Packs::close` gives: once hidden, the system has already given
    /// the foreground away.
    pub fn close(&self) {
        if !self.open.replace(false) {
            return;
        }
        if let Err(reason) = self.keyboard.give_back(window_handle(self.window.window())) {
            (self.say)(i18n::environment(Environment::FocusNotReturned, &reason));
        }
        let _ = self.window.hide();
    }

    /// The window stopped being the active one: the tester is elsewhere, and
    /// the keyboard stays where they put it.
    fn left(&self) {
        if self.open.replace(false) {
            let _ = self.window.hide();
        }
    }

    /// Asks for the keyboard as soon as the window has a handle to ask with -
    /// the pack window's poll, for the same measured reason.
    fn take_keyboard(self: &Rc<Self>, started: Instant) {
        let handle = window_handle(self.window.window());
        if handle.is_none() && started.elapsed() < HANDLE_WAIT {
            let weak = Rc::downgrade(self);
            slint::Timer::single_shot(POLL, move || {
                if let Some(about) = weak.upgrade()
                    && about.is_open()
                {
                    about.take_keyboard(started);
                }
            });
            return;
        }
        self.take_with(handle);
    }

    /// What the poll ends in. Public so a test with no handle to wait for
    /// reaches the same code.
    pub fn take_with(&self, handle: Option<u64>) {
        if let Err(reason) = self.keyboard.take(handle) {
            (self.say)(i18n::environment(Environment::AboutFocusNotTaken, &reason));
        }
    }

    /// Esc closes, and every other key travels on.
    fn press(&self, text: &str) -> bool {
        if key_of(text) != Key::Escape {
            return false;
        }
        self.close();
        true
    }
}

/// Every word and every fact - none of them changes while the program runs.
fn label(window: &AboutWindow) {
    window.set_window_title(i18n::about_label(AboutLabel::Title).into());
    window.set_heading(i18n::about_label(AboutLabel::Heading).into());
    window.set_summary(i18n::about_label(AboutLabel::Summary).into());
    let facts: Vec<Fact> = facts()
        .into_iter()
        .map(|(label, value)| Fact {
            label: label.into(),
            value: value.into(),
        })
        .collect();
    window.set_facts(ModelRc::new(VecModel::from(facts)));
    window.set_copyright(copyright().into());
    window.set_warranty(i18n::about_label(AboutLabel::Warranty).into());
    window.set_licence_text(i18n::about_label(AboutLabel::LicenceText).into());
    window.set_close_label(i18n::about_label(AboutLabel::Close).into());
    window.set_close_action(i18n::about_label(AboutLabel::CloseAction).into());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_address_is_shown_the_way_a_tester_types_it() {
        assert_eq!(
            shown_address("https://naughtykeyboard.donislawdev.com/"),
            "naughtykeyboard.donislawdev.com"
        );
        assert_eq!(
            shown_address("https://github.com/donislawdev/Naughty-Keyboard"),
            "github.com/donislawdev/Naughty-Keyboard"
        );
        assert_eq!(shown_address("http://example.test/"), "example.test");
        assert_eq!(shown_address("example.test"), "example.test");
    }

    /// The five facts the owner asked for, in their order, each with a value -
    /// an empty field in the manifest would show a label beside nothing.
    #[test]
    fn the_window_lists_the_version_the_author_the_licence_and_both_addresses() {
        let facts = facts();
        let labels: Vec<&str> = facts.iter().map(|(label, _)| *label).collect();
        assert_eq!(
            labels,
            ["Version", "Author", "Licence", "Website", "Source code"]
        );
        for (label, value) in &facts {
            assert!(!value.is_empty(), "{label} has no value");
        }
        assert_eq!(facts[0].1, env!("CARGO_PKG_VERSION"));
        assert_eq!(copyright(), "Copyright \u{a9} 2026 DonislawDev");
    }
}
