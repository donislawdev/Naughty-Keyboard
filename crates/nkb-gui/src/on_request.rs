//! How a window opened on request stands on the screen - the pack window, the
//! shortcuts window and About alike - and how it goes away.
//!
//! # Not minimized, ever
//!
//! 🔴 Measured, not read (`OBS-183`, `tools/sondy/minimalizacja-okna-paczek.ps1`,
//! 2026-10-19, Slint 1.18.1, winit 0.30.13, Windows 11). Such a window closes
//! when it stops being the active one (`OBS-147`), and minimizing it makes it
//! stop. Three things went wrong from there, each answered here:
//!
//! 1. The hide asked for in that moment was lost when the minimizing came from
//!    the window's own button, its menu or `Win+Down`: the report that the
//!    window went inactive arrives while the system is still minimizing it,
//!    and the window stayed in the taskbar while its owner took it for closed.
//!    Restored from there it answered no key, and neither Esc nor its close
//!    button closed it. So a window that was left hides once that event is
//!    over: `hide_once_left`.
//! 2. Opened again, it came up minimized, in each of the three ways tried -
//!    `show()` shows a window in the state it was hidden in - and it took the
//!    keyboard there. The tester saw nothing, and the letters of the search
//!    went into a window nobody could see. So every opening brings it back:
//!    `bring_up`.
//! 3. A button that closes the window under another name is one too many (the
//!    owner's choice, `D137`): its title bar keeps Close alone.
//!
//! What stays possible is named: without the button the window can still be
//! minimized another way - by a program of somebody else's at least - and that
//! is leaving like any other, which 1 and 2 make safe.
//!
//! # Made at the first opening
//!
//! About and the shortcuts window are made the first time the tester opens
//! them, not at start: [`Later`] says why, and why the pack window is not.

use std::cell::{Cell, OnceCell};
use std::rc::Rc;
use std::time::Duration;

use nkb_adapters::i18n::{self, Startup};
use slint::winit_030::WinitWindowAccessor;
use slint::winit_030::winit::window::WindowButtons;

/// Shows `window` - or, already shown, brings it back - as a window opened on
/// request stands: on the screen rather than in the taskbar, with Close as its
/// only button.
///
/// # Errors
///
/// The toolkit's own, when the window cannot be shown.
pub(crate) fn bring_up(window: &slint::Window) -> Result<(), slint::PlatformError> {
    window.show()?;
    // After `show()`, never before: bringing a hidden window back from the
    // taskbar shows it, and a window that is not minimized is left as it is.
    window.set_minimized(false);
    // Last: winit does not write the buttons of a minimized window (its
    // `apply_diff`), and asked again for the same buttons it has no change
    // to write.
    keep_only_close(window);
    take_the_front(window);
    Ok(())
}

/// Brings a window opened on request, and the keyboard, in front.
///
/// On macOS every window of the palette is created not active, so that the
/// palette itself does not take the front (`focus::select_backend`), and a window
/// that is meant to take it has to be asked. Through winit's own call, which
/// activates the application and makes the window the key one: measured, the three
/// letters typed after it reached the window, where `SetFrontProcess` alone left
/// the keyboard with the application that had it (`nkb-sys`, `macos/window.rs`).
/// Elsewhere the window manager does it on show, and the caller's `LentFocus`
/// makes sure. Needs the event loop to be running, which every opening is.
#[cfg(target_os = "macos")]
pub(crate) fn take_the_front(window: &slint::Window) {
    let _ = window.with_winit_window(|window| window.focus_window());
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn take_the_front(_window: &slint::Window) {}

/// Leaves Close as the title bar's only button - minimize and maximize go.
///
/// Both, because on Windows a minimize button leaves the title bar only
/// together with maximize: alone it stays there, greyed. The window still
/// resizes by its edges.
///
/// Through winit rather than the window's style in `nkb-sys`: winit writes the
/// style from its own flags whenever any of them changes - every show and every
/// hide - and would put the buttons back. A window that winit does not back,
/// the tests' own, has no title bar to change.
fn keep_only_close(window: &slint::Window) {
    let _ = window.with_winit_window(|window| window.set_enabled_buttons(WindowButtons::CLOSE));
}

/// A window opened on request that is made at its first opening and kept from
/// then on - About and the shortcuts window.
///
/// 🔴 Measured, not read (R23, `D167`, `tools/sonda-okna-pamiec`, 2026-11-03,
/// Slint 1.18.1 with femtovg): a window holds a graphics context of its own from
/// the moment it is made, shown or not. On Windows with an AMD graphics driver
/// one such window cost about 55 MB of private memory and 20 MB of the working
/// set, and the three the palette used to make at start 150 MB and 60 MB - as
/// much as the palette's own window. On Linux in a virtual machine it was
/// 17 MB a window, on macOS under 2 MB. Made at its first opening, a window the
/// tester never opens costs nothing, and that opening waits for the context
/// instead: 114-165 ms on the same Windows machine.
///
/// The pack window is not made this way. Its shortcut is pressed by a tester
/// about to type, and until it has the keyboard the letters reach the field
/// under test (`OBS-148`).
///
/// 🔴 The first opening waits until the window is ready (`when_backed`).
/// Opened at once, the window made while the loop runs has no native window
/// yet, so `bring_up` had nothing to take the buttons off, and About came up
/// with minimize and maximize beside Close - measured,
/// `tools/sondy/okna-na-zadanie.ps1`.
pub struct Later<T> {
    made: OnceCell<Rc<T>>,
    /// A first opening that waits for the window to be ready. A close in that
    /// time - the palette's - takes it back: the window must not open after
    /// the palette is gone, or the event loop would wait for it.
    waiting: Cell<bool>,
    /// The palette's message band: a window that cannot be made says so there,
    /// and the window made gets its own way to it.
    say: Rc<dyn Fn(String)>,
    make: Maker<T>,
}

/// How [`Later`] makes its window, handed the way to the palette's message band.
type Maker<T> = Box<dyn Fn(Box<dyn Fn(String)>) -> Result<Rc<T>, slint::PlatformError>>;

/// What a window is told once it is ready to open, or why it cannot be.
pub type Ready = Box<dyn FnOnce(Result<(), slint::PlatformError>)>;

/// What [`Later`] asks of the window it makes.
pub trait Requested {
    /// Opens the window - or, open already, brings it back.
    fn open(self: &Rc<Self>);
    /// Closes the window.
    fn close(&self);
    /// Runs `then` once the window can open as one made at start does - for a
    /// window on the winit backend, `when_backed`.
    fn when_ready(self: &Rc<Self>, then: Ready);
}

impl<T: Requested + 'static> Later<T> {
    /// Nothing made yet: `make` runs at the first opening.
    #[must_use]
    pub fn new(
        say: Box<dyn Fn(String)>,
        make: impl Fn(Box<dyn Fn(String)>) -> Result<Rc<T>, slint::PlatformError> + 'static,
    ) -> Rc<Self> {
        Rc::new(Self {
            made: OnceCell::new(),
            waiting: Cell::new(false),
            say: Rc::from(say),
            make: Box::new(make),
        })
    }

    /// Opens the window - made now, and opened once it is ready, if this is its
    /// first opening. Asked again while that opening waits, it does nothing
    /// more. A window that cannot be made says so in the palette, the way one
    /// that cannot be shown does, and the next opening tries again.
    pub fn open(self: &Rc<Self>) {
        if self.waiting.get() {
            return;
        }
        if let Some(window) = self.made.get() {
            window.open();
            return;
        }
        let say = Rc::clone(&self.say);
        let window = match (self.make)(Box::new(move |line| say(line))) {
            Ok(window) => Rc::clone(self.made.get_or_init(|| window)),
            Err(error) => {
                self.cannot(&error);
                return;
            }
        };
        self.waiting.set(true);
        let later = Rc::downgrade(self);
        window.when_ready(Box::new(move |ready| {
            let Some(later) = later.upgrade() else {
                return;
            };
            if !later.waiting.replace(false) {
                return;
            }
            match (ready, later.made.get()) {
                (Ok(()), Some(window)) => window.open(),
                (Err(error), _) => later.cannot(&error),
                (Ok(()), None) => {}
            }
        }));
    }

    /// Closes the window, and takes back a first opening still waiting. One
    /// never made has nothing to close.
    pub fn close(&self) {
        self.waiting.set(false);
        if let Some(window) = self.made.get() {
            window.close();
        }
    }

    fn cannot(&self, error: &slint::PlatformError) {
        (self.say)(i18n::startup_failure(
            Startup::WindowFailed,
            &error.to_string(),
        ));
    }
}

/// Runs `then` once the backend has made the native window of `owner`'s
/// window - at once when it has.
///
/// Read in i-slint-backend-winit 1.18.1 (`set_visibility`, `ensure_window`)
/// and measured with `tools/sondy/okna-na-zadanie.ps1`: a window made while the
/// loop runs has no native window until the backend's next turn, and showing
/// it only asks for one. So it is shown and hidden in one turn, which leaves
/// nothing on the screen and has the backend make its native window hidden,
/// the way it makes those made before the loop at its start. On Wayland it
/// could not - a native window there is made visible - and the palette's
/// windows are X11 ones on Linux (`D145` point 49).
pub(crate) fn when_backed<T: 'static>(
    owner: &Rc<T>,
    window: fn(&T) -> &slint::Window,
    then: Ready,
) {
    if window(owner).has_winit_window() {
        then(Ok(()));
        return;
    }
    if let Err(error) = window(owner).show().and_then(|()| window(owner).hide()) {
        then(Err(error));
        return;
    }
    // Shared with the task, so that a task that cannot be started still tells.
    let then = Rc::new(Cell::new(Some(then)));
    let spawned = slint::spawn_local({
        let owner = Rc::clone(owner);
        let then = Rc::clone(&then);
        async move {
            let backed = window(&owner).winit_window().await.map(drop);
            if let Some(then) = then.take() {
                then(backed);
            }
        }
    });
    if let Err(error) = spawned
        && let Some(then) = then.take()
    {
        then(Err(slint::PlatformError::Other(error.to_string())));
    }
}

/// Hides the window of `owner` once the event that made it inactive is over -
/// if `closed` still says so then, because a window opened again in between
/// stays where it is.
///
/// One turn of the event loop, not a delay: a timer of zero runs once the
/// system has finished with the message it was handling, the minimizing
/// included. Asked for inside it, the hide was lost (point 1 of the header).
pub(crate) fn hide_once_left<T: 'static>(
    owner: &Rc<T>,
    closed: fn(&T) -> bool,
    window: fn(&T) -> &slint::Window,
) {
    let owner = Rc::downgrade(owner);
    slint::Timer::single_shot(Duration::ZERO, move || {
        if let Some(owner) = owner.upgrade()
            && closed(&owner)
        {
            let _ = window(&owner).hide();
        }
    });
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use std::cell::{Cell, RefCell};

    use super::*;

    /// A window that counts what was asked of it, and is ready when the test
    /// says so.
    #[derive(Default)]
    struct Counted {
        opened: Cell<u32>,
        closed: Cell<u32>,
        ready: RefCell<Option<Ready>>,
    }

    impl Requested for Counted {
        fn open(self: &Rc<Self>) {
            self.opened.set(self.opened.get() + 1);
        }
        fn close(&self) {
            self.closed.set(self.closed.get() + 1);
        }
        fn when_ready(self: &Rc<Self>, then: Ready) {
            *self.ready.borrow_mut() = Some(then);
        }
    }

    impl Counted {
        /// The window is ready, or cannot be.
        fn ready(&self, result: Result<(), slint::PlatformError>) {
            let then = self.ready.borrow_mut().take().expect("nobody waits");
            then(result);
        }
    }

    /// What a test reads: the lines said in the palette and how many times a
    /// window was made, failed tries included.
    struct Seen {
        said: Rc<RefCell<Vec<String>>>,
        made: Rc<Cell<u32>>,
    }

    /// A `Later` over counted windows whose first `fails` makings fail.
    fn later(fails: u32) -> (Seen, Rc<Later<Counted>>) {
        let said = Rc::new(RefCell::new(Vec::new()));
        let made = Rc::new(Cell::new(0));
        let later = Later::new(
            {
                let said = Rc::clone(&said);
                Box::new(move |line| said.borrow_mut().push(line))
            },
            {
                let made = Rc::clone(&made);
                move |_say| {
                    made.set(made.get() + 1);
                    if made.get() <= fails {
                        return Err(slint::PlatformError::Other("no graphics context".into()));
                    }
                    Ok(Rc::new(Counted::default()))
                }
            },
        );
        (Seen { said, made }, later)
    }

    fn failure(reason: &str) -> Vec<String> {
        vec![i18n::startup_failure(Startup::WindowFailed, reason)]
    }

    #[test]
    fn nothing_is_made_until_the_first_opening_and_closing_before_it_makes_nothing() {
        let (seen, later) = later(0);
        assert_eq!(seen.made.get(), 0, "made at start");
        later.close();
        assert_eq!(seen.made.get(), 0, "closing made the window");
        assert!(seen.said.borrow().is_empty());
    }

    #[test]
    fn the_first_opening_waits_for_the_window_and_the_window_is_made_once() {
        let (seen, later) = later(0);
        later.open();
        let window = Rc::clone(later.made.get().expect("made at the first opening"));
        assert_eq!(window.opened.get(), 0, "opened before it was ready");
        later.open();
        assert_eq!(window.opened.get(), 0, "a second click opened it early");
        window.ready(Ok(()));
        assert_eq!(window.opened.get(), 1);
        later.open();
        later.close();
        assert_eq!(seen.made.get(), 1, "made again");
        assert_eq!((window.opened.get(), window.closed.get()), (2, 1));
        assert!(seen.said.borrow().is_empty());
    }

    #[test]
    fn a_close_while_the_first_opening_waits_keeps_the_window_shut() {
        let (_seen, later) = later(0);
        later.open();
        later.close();
        let window = Rc::clone(later.made.get().expect("made at the first opening"));
        window.ready(Ok(()));
        assert_eq!(window.opened.get(), 0, "opened after the palette closed it");
        later.open();
        assert_eq!(window.opened.get(), 1, "the next opening stayed shut");
    }

    #[test]
    fn a_window_that_cannot_be_made_or_readied_says_so() {
        let (seen, later) = later(1);
        later.open();
        assert!(later.made.get().is_none());
        assert_eq!(*seen.said.borrow(), failure("no graphics context"));
        later.open();
        assert_eq!(seen.made.get(), 2, "the next opening did not try again");
        let window = Rc::clone(later.made.get().expect("made at the second try"));
        window.ready(Err(slint::PlatformError::Other("no native window".into())));
        assert_eq!(window.opened.get(), 0);
        assert_eq!(
            seen.said.borrow()[1..],
            failure("no native window")[..],
            "a window never ready said nothing"
        );
    }
}
