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
//! # Left, as the system says it
//!
//! 🔴 Measured, not read (`OBS-268`, `tools/sondy/linux/okno-wartosci-x11.py`,
//! 2026-11-18, Xorg with openbox, winit 0.30.13). The toolkit's report that a
//! window went inactive is not always the keyboard moving away. On X11 a
//! global shortcut is a grab of the keyboard, and its start and end reach the
//! window holding the focus as a focus out and a focus in, which winit passes
//! on without saying they came from a grab. The value window can open while
//! its shortcut is still held - some 65 ms after the key went down - and then
//! the tester letting go of the shortcut was the window "left": it closed as
//! the keys came up, every time, with the focus of the server never leaving
//! it. So a window asks the system, at the report, whether the tester really
//! went elsewhere: `has_left`. Where the system is not asked, they did.
//!
//! # Made at the first opening
//!
//! About and the shortcuts window are made the first time the tester opens
//! them, not at start: [`Later`] says why, and why the pack window is not.
//!
//! # Beside the palette
//!
//! A window that is not on the screen opens beside the palette, not where the
//! system puts a new window - at its first opening that was often the other
//! end of the screen (the owner's point 3, 2026-11-16). Where exactly is the
//! core's rule, `nkb_core::screens::beside`. A window already open stays where
//! the tester has it, and one the tester moved is put back beside the palette
//! at its next opening: it is a window opened for one choice, and it closes
//! when it is left (`OBS-147`).

use std::cell::{Cell, OnceCell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use nkb_adapters::i18n::{self, Startup};
use nkb_core::screens::{Rect, beside};
use slint::ComponentHandle;
use slint::winit_030::WinitWindowAccessor;
use slint::winit_030::winit::window::WindowButtons;

/// The room between the palette and a window opened beside it, in logical
/// pixels - the space between two sections of a window (`Tokens.space-2`), so
/// the two read as two windows and not as one.
const GAP: f32 = 8.0;

thread_local! {
    /// The palette the windows opened on request stand beside, on the main
    /// thread, where every window lives. Named once at start (`main.rs`).
    static PALETTE: RefCell<Option<slint::Weak<crate::Palette>>> = const { RefCell::new(None) };
}

/// Names the palette the windows opened on request open beside.
pub fn open_beside(palette: &crate::Palette) {
    PALETTE.with(|named| *named.borrow_mut() = Some(palette.as_weak()));
}

/// Where `window` goes beside the palette, as tall as winit reports it now.
///
/// In physical pixels, the unit the toolkit reports the palette and its screen
/// in, and the screen is the one winit says the palette stands on - on every
/// system, which `nkb_sys::screens` is not yet (Linux answers it with `None`).
/// Without a palette, a screen or a native window - the tests' windows have
/// none - the window opens where the system puts it, as it did before.
///
/// The OUTER frames, title bar and borders included: a window is placed by the
/// corner of its frame, and its inner size alone would let a window pushed to
/// the bottom of the screen hang off it by its title bar.
fn beside_palette(window: &slint::Window) -> Option<slint::PhysicalPosition> {
    let palette = PALETTE.with(|named| named.borrow().as_ref().and_then(slint::Weak::upgrade))?;
    let anchor = palette.window();
    // The part of the screen windows may use, where the system says it, so a
    // window pushed down stays above the taskbar (`D190`) - else the screen.
    let screen = crate::room::work_area_of(anchor).or_else(|| {
        anchor
            .with_winit_window(|window| window.current_monitor())
            .flatten()
            .map(|monitor| {
                let (corner, extent) = (monitor.position(), monitor.size());
                Rect {
                    x: corner.x,
                    y: corner.y,
                    width: extent.width,
                    height: extent.height,
                }
            })
    })?;
    let (at, size) = (anchor.position(), outer_size(anchor));
    let gap = (GAP * anchor.scale_factor()).round();
    let place = beside(
        Rect {
            x: at.x,
            y: at.y,
            width: size.0,
            height: size.1,
        },
        outer_size(window),
        screen,
        // A scale is a small positive number, so the gap is a few pixels.
        if gap.is_finite() && gap >= 0.0 {
            gap as u32
        } else {
            0
        },
    );
    Some(slint::PhysicalPosition::new(place.x, place.y))
}

/// The height a window opened beside the palette may have whole (`D190`): the
/// work area of the palette's screen less the window's own frame, in the
/// window's logical pixels. Zero - "not known" to the view - without a palette,
/// or where the system does not say.
pub(crate) fn room_beside(window: &slint::Window) -> f32 {
    PALETTE
        .with(|named| named.borrow().as_ref().and_then(slint::Weak::upgrade))
        .and_then(|palette| crate::room::beside(palette.window(), window))
        .unwrap_or(0.0)
}

/// Tells a window opened beside the palette its room again - through `tell` -
/// whenever winit reports it moved, resized or took the keyboard: on Linux,
/// where a window's title bar and frame are known only once the window manager
/// has framed it, after its first showing, and the room told before counted
/// none. Measured 2026-11-17 on GNOME 50 at 1280 x 800
/// (`tools/sondy/linux/niski-ekran-wayland.py`): the shortcuts window got
/// 768 px of content in a work area 768 px tall, and its title bar ran 37 px
/// past the bottom. A move and a resize alone fitted it in one run of two -
/// in the other both came before the frame did. The keyboard comes to the
/// window once it is mapped, framed by then, and this window takes it at every
/// opening.
///
/// The window's one filter of winit events on Linux (`slint.md` 2.81).
#[cfg(target_os = "linux")]
pub(crate) fn keep_room_beside(window: &slint::Window, tell: impl Fn(f32) + 'static) {
    use slint::winit_030::EventResult;
    use slint::winit_030::winit::event::WindowEvent;
    window.on_winit_window_event(move |window, event| {
        if matches!(
            event,
            WindowEvent::Moved(_) | WindowEvent::Resized(_) | WindowEvent::Focused(true)
        ) {
            tell(room_beside(window));
        }
        EventResult::Propagate
    });
}

/// A window's size with its frame, in physical pixels - its inner size where
/// winit does not back it or cannot say (`room::content_and_frame`).
fn outer_size(window: &slint::Window) -> (u32, u32) {
    crate::room::content_and_frame(window).map_or_else(
        || (window.size().width, window.size().height),
        |(content, frame)| {
            (
                content.0.saturating_add(frame.0),
                content.1.saturating_add(frame.1),
            )
        },
    )
}

/// Shows `window` - or, already shown, brings it back - as a window opened on
/// request stands: on the screen rather than in the taskbar, with Close as its
/// only button.
///
/// # Errors
///
/// The toolkit's own, when the window cannot be shown.
pub(crate) fn bring_up(window: &slint::Window) -> Result<(), slint::PlatformError> {
    // Before `show()`, while it is still off the screen: shown first, it would
    // flash where the system put it. A window on the screen already stays
    // where the tester has it.
    let placed = if window.is_visible() {
        None
    } else {
        beside_palette(window).map(|at| (at, outer_size(window)))
    };
    if let Some((at, _)) = placed {
        window.set_position(at);
    }
    window.show()?;
    // And again once it is on the screen, where it is not the size it was
    // placed at: the toolkit lays out rows made a moment before, and gives the
    // window the height of its view, only when it shows it (i-slint-core
    // 1.18.1, `WindowInner::show`). Measured 2026-11-17
    // (`tools/sondy/niski-ekran.ps1`, Windows Server 2025): the shortcuts
    // window, which takes its rows and its room just before it opens, read
    // 316 px of content before its first showing and showed 869 px, and with
    // the palette low on a 975 px screen it was placed level with it and ran
    // to 1387 px. It may stand at the first place for a moment before it
    // moves - at its first opening, and only where the two places differ.
    //
    // And on Linux always: a window manager places a window it maps by its own
    // rule and keeps only a move of a window already mapped. Measured
    // 2026-11-16 under openbox (`tools/sondy/linux/dymek-x11.py`): the value
    // window stood in the middle of the free part of the screen, the place
    // given before `show()` ignored. The window may show at the manager's
    // place for a moment before it moves. Its size is asked for here while the
    // manager may still be framing it, so a frame winit cannot know yet counts
    // as none (`room::content_and_frame`, `OBS-262`).
    if let Some((first, size)) = placed {
        let again = if outer_size(window) == size {
            Some(first)
        } else {
            beside_palette(window)
        };
        if let Some(at) = again
            && (cfg!(target_os = "linux") || at != first)
        {
            window.set_position(at);
        }
    }
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

/// Whether the tester left `window`, which the toolkit has just reported
/// inactive: unless the system says the window holds the keyboard still (the
/// module header, "Left, as the system says it").
///
/// Asked at the report, not later: by the time the report arrives, a manager
/// that gave the keyboard to another window has moved the server's focus
/// already, and a grab never moved it.
pub(crate) fn has_left(window: &slint::Window) -> bool {
    nkb_adapters::holds_keyboard(crate::focus::window_handle(window)) != Some(true)
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
