//! The size the shortcuts window ASKS the system for, read through the call a
//! backend receives - and the window drawn at exactly that size.
//!
//! # Why this exists
//!
//! The live window opened at its minimum, 400 x 360, with four of its ten rows
//! showing (K5.7, `tools/okno-skrotow.ps1`, `slint.md` 2.38). The winit backend
//! creates every window, hidden, when the event loop starts and keeps that
//! size on a later `show()`, so a PREFERRED size never reached the screen. Every
//! render test drew the window into a surface of a size the TEST chose - 900
//! pixels tall in `shortcuts_appearance.rs` - so none of them could see it.
//!
//! Here a window adapter of the test's own records the layout constraints the
//! window hands its backend (`WindowAdapter::update_window_properties`), the
//! numbers winit sizes the window from: the size must be BOUND (min = max), the
//! one kind the live backend follows, as the palette showed. The window is then
//! drawn at that size: ten rows with no scroll thumb, and the last row standing
//! where it stood after the invitation, the answer and a refusal appear under
//! the list.
//!
//! One test in its own binary: one platform per process.

#![allow(clippy::panic, clippy::expect_used)]

// The shared harness carries helpers this test does not use.
#[allow(dead_code)]
mod offscreen;

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::sync::mpsc;

use nkb_adapters::{ChordHeld, default_bindings};
use nkb_app::ShortcutChange;
use nkb_app::ports::ShortcutRegistration;
use nkb_core::hotkeys::{HotkeyAction, HotkeyChord, Refusal, Refused};
use nkb_gui::ShortcutsWindow;
use nkb_gui::live::{ShortcutsNow, Told};
use nkb_gui::packs::Keyboard;
use nkb_gui::shortcuts::{Shortcuts, SystemKeys};
use slint::platform::software_renderer::{RepaintBufferType, SoftwareRenderer};
use slint::platform::{
    Key, LayoutConstraints, Platform, PlatformError, Renderer, WindowAdapter, WindowEvent,
    WindowProperties,
};
use slint::{ComponentHandle, LogicalSize, PhysicalSize, SharedString};

/// The dictionary's values, copied rather than read for the reason the other
/// render tests give: a test that reads the value it checks cannot fail.
const WIDTH: f32 = 480.0;
/// The height the live window opened at before K5.7 - its old minimum.
const LIVE_MINIMUM: f32 = 360.0;
const SURFACE: (u8, u8, u8) = (0x14, 0x16, 0x1A);
const THUMB: (u8, u8, u8) = (0x45, 0x4C, 0x5C);

/// A window adapter that remembers what the window asked of it.
struct Asking {
    window: slint::Window,
    renderer: SoftwareRenderer,
    size: Cell<PhysicalSize>,
    asked: Cell<Option<LayoutConstraints>>,
}

impl WindowAdapter for Asking {
    fn window(&self) -> &slint::Window {
        &self.window
    }

    fn size(&self) -> PhysicalSize {
        self.size.get()
    }

    fn renderer(&self) -> &dyn Renderer {
        &self.renderer
    }

    fn update_window_properties(&self, properties: WindowProperties<'_>) {
        self.asked.set(Some(properties.layout_constraints()));
    }
}

impl Asking {
    /// Gives the window this size, the way a backend does after the system
    /// resized it.
    fn resize(&self, width: u32, height: u32) {
        self.size.set(PhysicalSize::new(width, height));
        self.window.dispatch_event(WindowEvent::Resized {
            size: LogicalSize::new(width as f32, height as f32),
        });
    }

    fn draw(&self) -> Vec<offscreen::Pixel> {
        let size = self.size.get();
        let mut buffer = vec![offscreen::Pixel::default(); (size.width * size.height) as usize];
        slint::platform::update_timers_and_animations();
        self.renderer.render(&mut buffer, size.width as usize);
        buffer
    }
}

struct Headless(Rc<Asking>);

impl Platform for Headless {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        Ok(self.0.clone())
    }
}

/// A keyboard that is always there.
struct Quiet;

impl Keyboard for Quiet {
    fn remember(&self) {}
    fn take(&self, _window: Option<u64>) -> Result<(), String> {
        Ok(())
    }
    fn give_back(&self, _window: Option<u64>) -> Result<(), String> {
        Ok(())
    }
}

/// The system's answer about the chord held, as the test dictates it.
struct Dictated(Rc<RefCell<ChordHeld>>);

impl SystemKeys for Dictated {
    fn chord_held(&self) -> ChordHeld {
        *self.0.borrow()
    }
    fn keep_menu_shut(&self, _window: Option<u64>) -> Result<(), String> {
        Ok(())
    }
}

fn tap(window: &ShortcutsWindow, text: &str) {
    for event in [
        WindowEvent::KeyPressed {
            text: SharedString::from(text),
        },
        WindowEvent::KeyReleased {
            text: SharedString::from(text),
        },
    ] {
        window.window().dispatch_event(event);
    }
}

fn tap_key(window: &ShortcutsWindow, key: Key) {
    tap(window, SharedString::from(key).as_str());
}

fn chord(text: &str) -> HotkeyChord {
    HotkeyChord::parse(text).expect("a chord the test writes reads")
}

fn pixel(buffer: &[offscreen::Pixel], width: u32, x: u32, y: u32) -> (u8, u8, u8) {
    let p = buffer[(y * width + x) as usize];
    (p.r, p.g, p.b)
}

/// The rows between the header and the footer, found by their ground - the
/// same measure as `shortcuts_appearance.rs`.
fn content(buffer: &[offscreen::Pixel], width: u32, height: u32) -> std::ops::Range<u32> {
    let plain = |y: u32| pixel(buffer, width, 1, y) == SURFACE;
    let top = (0..height)
        .find(|&y| plain(y))
        .expect("the window has a content area");
    let bottom = (top..height)
        .find(|&y| !plain(y))
        .expect("the footer stands below the content");
    top..bottom
}

/// Bands of rows holding anything but the surface, inside the content.
fn ink_bands(
    buffer: &[offscreen::Pixel],
    width: u32,
    rows: std::ops::Range<u32>,
) -> Vec<std::ops::Range<u32>> {
    let inked = |y: u32| (2..width - 2).any(|x| pixel(buffer, width, x, y) != SURFACE);
    let mut bands = Vec::new();
    let mut start = None;
    for y in rows.clone() {
        match (inked(y), start) {
            (true, None) => start = Some(y),
            (false, Some(from)) => {
                bands.push(from..y);
                start = None;
            }
            _ => {}
        }
    }
    if let Some(from) = start {
        bands.push(from..rows.end);
    }
    bands
}

/// Pixels of the scroll thumb in the strip at the content's right edge.
fn thumb(buffer: &[offscreen::Pixel], width: u32, rows: std::ops::Range<u32>) -> usize {
    rows.flat_map(|y| ((width - 24)..width).map(move |x| (x, y)))
        .filter(|&(x, y)| pixel(buffer, width, x, y) == THUMB)
        .count()
}

#[test]
fn the_shortcuts_window_asks_for_the_room_its_rows_and_sentences_need() {
    let adapter = Rc::new_cyclic(|weak: &Weak<Asking>| Asking {
        window: slint::Window::new(weak.clone() as Weak<dyn WindowAdapter>),
        renderer: SoftwareRenderer::new_with_repaint_buffer_type(RepaintBufferType::NewBuffer),
        size: Cell::new(PhysicalSize::new(1, 1)),
        asked: Cell::new(None),
    });
    slint::platform::set_platform(Box::new(Headless(Rc::clone(&adapter))))
        .expect("no other platform may be installed in this process");

    let held = Rc::new(RefCell::new(ChordHeld::NoKey));
    let (ask, _commands) = mpsc::channel();
    let shortcuts = Shortcuts::new(
        ShortcutsWindow::new().expect("the shortcuts window must build"),
        Box::new(Quiet),
        Box::new(Dictated(Rc::clone(&held))),
        ask,
        Box::new(|_| {}),
    );
    let window = shortcuts.window();

    // ---- opened the way the product opens it: on the worker's answer ---------
    let defaults = default_bindings();
    shortcuts.open();
    shortcuts.told(Told::Paused(ShortcutsNow {
        bindings: defaults,
        registered: defaults
            .as_slice()
            .iter()
            .map(|(action, held)| (*action, *held, ShortcutRegistration::Registered))
            .collect(),
    }));
    let asked = adapter
        .asked
        .get()
        .expect("showing the window told its backend nothing");
    let preferred = asked.preferred;
    assert!(
        (preferred.width - WIDTH).abs() < 0.5 && preferred.height > LIVE_MINIMUM,
        "the window asks its backend for {preferred:?} - no taller than the {LIVE_MINIMUM} it \
         opened at live, with four of ten rows showing"
    );
    // 🔴 BOUND, not merely preferred: the backend creates the window hidden
    // when the event loop starts and keeps that size on `show()`, so a
    // preferred size alone was ignored live - even one bound to the content
    // (`slint.md` 2.38). A size the backend cannot leave is min = max.
    assert!(
        asked.min == Some(preferred) && asked.max == Some(preferred),
        "the window's size is not bound: min {:?}, max {:?}, preferred {preferred:?} - the live \
         window would keep the size it was created with, before it had rows",
        asked.min,
        asked.max
    );

    // ---- drawn at the size it asked for: ten rows, nothing scrolls -----------
    let (width, height) = (
        preferred.width.ceil() as u32,
        preferred.height.ceil() as u32,
    );
    adapter.resize(width, height);
    let open = adapter.draw();
    let open_path = offscreen::save(&open, width, height, "shortcuts-preferred.png");
    let area = content(&open, width, height);
    assert_eq!(
        thumb(&open, width, area.clone()),
        0,
        "at the size it asks for, the list scrolls. Look at {}",
        open_path.display()
    );
    let last_row = ink_bands(&open, width, area)
        .last()
        .cloned()
        .expect("the list draws rows");

    // ---- what the window says under the list moves nothing -------------------
    // The invitation, the answer about a recording and a refusal: the sentences
    // of every recording, the refusal the level 1 proof presses.
    // What the window asks for, read after each drawing: the core re-sends its
    // properties on a zero-length timer and on every draw.
    let asks = || {
        adapter
            .asked
            .get()
            .expect("the window told its backend nothing")
            .preferred
    };
    tap_key(window, Key::DownArrow);
    tap_key(window, Key::Return);
    let invited = adapter.draw();
    let invited_asks = asks();
    *held.borrow_mut() = ChordHeld::Chord(chord("Alt+Shift+M"));
    tap(window, "m");
    let bindings = defaults
        .with(
            &[(HotkeyAction::PreviousValue, chord("Alt+Shift+M"))],
            &|_| None,
        )
        .0;
    shortcuts.told(Told::Shortcut {
        action: HotkeyAction::PreviousValue,
        change: ShortcutChange::Changed {
            bindings,
            also: Vec::new(),
            unchecked: None,
        },
        not_saved: None,
    });
    let answered = adapter.draw();
    let answered_asks = asks();
    tap_key(window, Key::Return);
    *held.borrow_mut() = ChordHeld::Chord(chord("Alt+Shift+Space"));
    tap(window, " ");
    shortcuts.told(Told::Shortcut {
        action: HotkeyAction::PreviousValue,
        change: ShortcutChange::Refused(Refused {
            action: HotkeyAction::PreviousValue,
            chord: chord("Alt+Shift+Space"),
            why: Refusal::SameAs(HotkeyAction::OpenPacks),
        }),
        not_saved: None,
    });
    let refused = adapter.draw();
    let refused_asks = asks();
    for (name, buffer, now) in [
        ("invitation", &invited, invited_asks),
        ("answer", &answered, answered_asks),
        ("refusal", &refused, refused_asks),
    ] {
        // The room under the list is kept from the start, so a sentence that
        // fits it changes nothing - not the rows, not the window's edge.
        assert_eq!(
            now, preferred,
            "with the {name} under the list the window asks for {now:?} instead of \
             {preferred:?} - the sentence did not fit the room kept for it"
        );
        let path = offscreen::save(
            buffer,
            width,
            height,
            &format!("shortcuts-preferred-{name}.png"),
        );
        let area = content(buffer, width, height);
        let messages = window.get_messages();
        assert_eq!(
            thumb(buffer, width, area.clone()),
            0,
            "with the {name} under the list, the list scrolls. Look at {}",
            path.display()
        );
        assert!(
            ink_bands(buffer, width, area).contains(&last_row),
            "with the {name} under the list, the last row no longer stands at {last_row:?} - \
             the list moved ({} lines said). Look at {}",
            slint::Model::row_count(&messages),
            path.display()
        );
    }
}
