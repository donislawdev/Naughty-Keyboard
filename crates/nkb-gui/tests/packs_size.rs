//! The size the pack window asks its backend for BEFORE it is ever shown - the
//! moment the winit backend creates it.
//!
//! # Why this exists
//!
//! The live pack window opened 360 x 560 instead of 440 x 560, every time
//! (`OBS-154`). The backend creates every window, hidden, as the event loop
//! starts, from the layout constraints the window reports at that moment, and
//! keeps that size on a later `show()` (`slint.md` 2.38). A window that has not
//! built its element tree yet reports a maximum width no larger than its
//! content's minimum, and the backend clamped the preferred 440 to it
//! (`slint.md` 2.40). The product builds the tree in the constructor
//! (`nkb_gui::packs::build_before_the_loop`).
//!
//! Here a window adapter of the test's own records the constraints each window
//! hands its backend (`WindowAdapter::update_window_properties`). The FIRST
//! record is the one that counts: the core sends it on a zero-length timer
//! after the window is made, before anything is drawn - exactly when the live
//! backend creates the window.
//!
//! The same is asked of a bare `PacksWindow`, built without the constructor.
//! That is the positive control: it shows the test can see the fault. If it
//! fails, Slint now builds the tree before answering for the layout, and the
//! workaround may go.
//!
//! One test in its own binary: one platform per process.

#![allow(clippy::panic, clippy::expect_used)]

use std::cell::RefCell;
use std::rc::{Rc, Weak};
use std::sync::mpsc;

use nkb_gui::PacksWindow;
use nkb_gui::live::{Command, in_use};
use nkb_gui::packs::{Keyboard, Packs};
use slint::platform::software_renderer::{RepaintBufferType, SoftwareRenderer};
use slint::platform::{
    LayoutConstraints, Platform, PlatformError, Renderer, WindowAdapter, WindowProperties,
};
use slint::{LogicalSize, PhysicalSize};

/// The dictionary's values, copied rather than read for the reason the other
/// render tests give: a test that reads the value it checks cannot fail.
const PREFERRED: LogicalSize = LogicalSize::new(440.0, 560.0);
const MINIMUM: LogicalSize = LogicalSize::new(360.0, 360.0);

/// A window adapter that remembers every set of constraints its window sent.
struct Asking {
    window: slint::Window,
    renderer: SoftwareRenderer,
    asked: RefCell<Vec<LayoutConstraints>>,
}

impl WindowAdapter for Asking {
    fn window(&self) -> &slint::Window {
        &self.window
    }

    fn size(&self) -> PhysicalSize {
        // Nothing yet: the window does not exist on any screen, as with the
        // live backend before the event loop starts.
        PhysicalSize::new(0, 0)
    }

    fn renderer(&self) -> &dyn Renderer {
        &self.renderer
    }

    fn update_window_properties(&self, properties: WindowProperties<'_>) {
        self.asked
            .borrow_mut()
            .push(properties.layout_constraints());
    }
}

/// A fresh adapter for every window, kept so the test can read each one.
struct Headless(Rc<RefCell<Vec<Rc<Asking>>>>);

impl Platform for Headless {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        let adapter = Rc::new_cyclic(|weak: &Weak<Asking>| Asking {
            window: slint::Window::new(weak.clone() as Weak<dyn WindowAdapter>),
            renderer: SoftwareRenderer::new_with_repaint_buffer_type(RepaintBufferType::NewBuffer),
            asked: RefCell::new(Vec::new()),
        });
        self.0.borrow_mut().push(Rc::clone(&adapter));
        Ok(adapter)
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

/// The first constraints the newest window sent - after the zero-length timer
/// the core starts when a window is made, and before any drawing.
fn first_asked(adapters: &RefCell<Vec<Rc<Asking>>>) -> LayoutConstraints {
    slint::platform::update_timers_and_animations();
    let adapters = adapters.borrow();
    let newest = adapters.last().expect("making a window made an adapter");
    newest
        .asked
        .borrow()
        .first()
        .copied()
        .expect("the window told its backend nothing before it was drawn")
}

fn about(size: LogicalSize, expected: LogicalSize) -> bool {
    (size.width - expected.width).abs() < 0.5 && (size.height - expected.height).abs() < 0.5
}

#[test]
fn the_pack_window_asks_for_its_preferred_size_before_it_is_ever_shown() {
    let adapters = Rc::new(RefCell::new(Vec::new()));
    slint::platform::set_platform(Box::new(Headless(Rc::clone(&adapters))))
        .expect("no other platform may be installed in this process");

    // ---- the window as the product makes it ---------------------------------
    let (commands, _worker) = mpsc::channel::<Command>();
    let packs = Packs::new(
        PacksWindow::new().expect("the pack window must build"),
        Box::new(Quiet),
        commands,
        in_use(),
        Box::new(|_| {}),
    );
    let asked = first_asked(&adapters);
    assert!(
        about(asked.preferred, PREFERRED),
        "before it is ever shown the pack window asks its backend for {:?}, not {PREFERRED:?} - \
         the live window is created at that size and keeps it (OBS-154). Min {:?}, max {:?}",
        asked.preferred,
        asked.min,
        asked.max
    );
    assert!(
        asked.min.is_some_and(|min| about(min, MINIMUM)),
        "before it is ever shown the pack window's minimum is {:?}, not {MINIMUM:?}",
        asked.min
    );
    // No maximum at all: the tester may make the window as large as they like.
    assert!(
        asked.max.is_none(),
        "before it is ever shown the pack window reports a maximum of {:?} - the tester could not \
         widen it, and the backend clamps the opening size to it",
        asked.max
    );
    drop(packs);

    // ---- positive control: the same window, never built ---------------------
    let bare = PacksWindow::new().expect("the pack window must build");
    let bare_asked = first_asked(&adapters);
    assert!(
        !about(bare_asked.preferred, PREFERRED),
        "a pack window that never built its element tree asks for {:?} - the size it was designed \
         for. Slint now builds the tree before answering for the layout, so \
         `build_before_the_loop` in packs.rs may go (OBS-154). Until it went, this test could see \
         the fault: min {:?}, max {:?}",
        bare_asked.preferred,
        bare_asked.min,
        bare_asked.max
    );
    drop(bare);
}
