//! Layouts of screens, and whether a remembered place is on one (`UX-GUI-012`).
//!
//! # Why a place is remembered per layout
//!
//! The palette stays where the tester put it, separately for each layout of
//! screens (`ux-spec.md` 2). A laptop on its dock and the same laptop alone
//! are two layouts, and a place on the dock's second monitor means nothing on
//! the laptop alone - restoring it there would open the only window of the
//! tool off every screen. So the key of a remembered place is the layout it
//! was remembered on, and a layout never seen before gets no place at all:
//! the system puts the window where it puts any new one.
//!
//! # What a layout is
//!
//! The screens, each as its rectangle on the virtual desktop in PHYSICAL
//! pixels - the unit the window system reports and takes a window's position
//! in. The scale a screen is set to is not part of it: changing the scale
//! keeps the physical rectangle, and a place in physical pixels stays where it
//! was.
//!
//! # The text form is a public name
//!
//! It is the key of `[palette.position]` in the settings file
//! (`settings-format.md` 2), read by every later version, so it is fixed here
//! and never changed (untouchable rule 3). One screen is `WIDTHxHEIGHT@X,Y`,
//! the screens stand ordered by `X`, then by `Y`, one space apart:
//!
//! ```text
//! 2560x1440@0,0 1920x1080@2560,0
//! ```
//!
//! A screen left of or above the main one has a negative coordinate. Reading
//! is exact: only the form [`Layout::text`] writes reads back, so one layout
//! cannot hide under two spellings as two keys.

/// The most screens a layout may hold. A machine with more is not one this
/// tool has met, and a longer key is more likely a file edited by mistake.
pub const MAX_SCREENS: usize = 16;

/// How far inside a window's outer corner the point lies that has to be on a
/// screen, in physical pixels: right and down.
///
/// Not the corner itself, because the outer frame of a window on Windows 10
/// and 11 has invisible borders - a window pushed against the left edge of a
/// screen stands at a NEGATIVE x of a few pixels and is fully visible. The
/// point is inside the title bar of any window at any scale this tool meets.
pub const PROBE: Point = Point { x: 32, y: 16 };

/// One screen: its rectangle on the virtual desktop, in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Screen {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Screen {
    /// Whether the point lies on this screen. The right and bottom edges are
    /// outside, as with every rectangle the window system reports.
    #[must_use]
    pub fn holds(&self, point: Point) -> bool {
        let (x, y) = (i64::from(point.x), i64::from(point.y));
        let (left, top) = (i64::from(self.x), i64::from(self.y));
        x >= left
            && y >= top
            && x < left + i64::from(self.width)
            && y < top + i64::from(self.height)
    }
}

/// A point on the virtual desktop in physical pixels - the top left corner of
/// a window's outer frame, when it is a place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

/// The screens of one machine at one moment, in the order of the text form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    screens: Vec<Screen>,
}

impl Layout {
    /// The layout of these screens, in any order - or `None` for no screen,
    /// more than [`MAX_SCREENS`], or a screen with no area, none of which is a
    /// layout a window can stand on.
    ///
    /// The same screen reported twice counts once: a mirrored pair of monitors
    /// is one rectangle to stand on.
    #[must_use]
    pub fn new(screens: impl IntoIterator<Item = Screen>) -> Option<Self> {
        let mut screens: Vec<Screen> = screens.into_iter().collect();
        screens.sort_unstable();
        screens.dedup();
        let usable = !screens.is_empty()
            && screens.len() <= MAX_SCREENS
            && screens
                .iter()
                .all(|screen| screen.width > 0 && screen.height > 0);
        usable.then_some(Self { screens })
    }

    /// The screens, in the order of the text form.
    #[must_use]
    pub fn screens(&self) -> &[Screen] {
        &self.screens
    }

    /// The text form - the module header gives it.
    #[must_use]
    pub fn text(&self) -> String {
        let mut text = String::new();
        for (at, screen) in self.screens.iter().enumerate() {
            if at > 0 {
                text.push(' ');
            }
            // `format!` rather than `write!` into the string: writing into a
            // `String` cannot fail, but its `Result` would still have to be
            // thrown away, and this layer never throws a result away.
            text.push_str(&format!(
                "{}x{}@{},{}",
                screen.width, screen.height, screen.x, screen.y
            ));
        }
        text
    }

    /// The layout a text form names - or `None` for text that is not EXACTLY
    /// the form [`Layout::text`] writes: other spacing, other order, a plus
    /// sign, a leading zero, a screen twice.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let mut screens = Vec::new();
        for part in text.split(' ') {
            let (size, corner) = part.split_once('@')?;
            let (width, height) = size.split_once('x')?;
            let (x, y) = corner.split_once(',')?;
            screens.push(Screen {
                x: x.parse().ok()?,
                y: y.parse().ok()?,
                width: width.parse().ok()?,
                height: height.parse().ok()?,
            });
        }
        let layout = Self::new(screens)?;
        (layout.text() == text).then_some(layout)
    }

    /// Whether a window whose outer frame starts at `place` stands on one of
    /// these screens - its [`PROBE`] point lies on one.
    #[must_use]
    pub fn shows(&self, place: Point) -> bool {
        let probe = Point {
            x: place.x.saturating_add(PROBE.x),
            y: place.y.saturating_add(PROBE.y),
        };
        self.screens.iter().any(|screen| screen.holds(probe))
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;

    fn screen(width: u32, height: u32, x: i32, y: i32) -> Screen {
        Screen {
            x,
            y,
            width,
            height,
        }
    }

    fn dock() -> Layout {
        Layout::new([screen(1920, 1080, 2560, 0), screen(2560, 1440, 0, 0)])
            .expect("two screens are a layout")
    }

    #[test]
    fn the_text_form_orders_the_screens_and_reads_back_as_the_same_layout() {
        let layout = dock();
        assert_eq!(layout.text(), "2560x1440@0,0 1920x1080@2560,0");
        assert_eq!(Layout::parse(&layout.text()), Some(layout));
    }

    #[test]
    fn a_screen_left_of_the_main_one_has_a_negative_corner_and_comes_first() {
        let layout = Layout::new([screen(2560, 1440, 0, 0), screen(1920, 1080, -1920, 180)])
            .expect("a layout");
        assert_eq!(layout.text(), "1920x1080@-1920,180 2560x1440@0,0");
        assert_eq!(Layout::parse(&layout.text()), Some(layout));
    }

    #[test]
    fn only_the_written_form_reads() {
        for other in [
            "",
            " 2560x1440@0,0",
            "2560x1440@0,0 ",
            "2560x1440@0,0  1920x1080@2560,0",
            "1920x1080@2560,0 2560x1440@0,0",
            "2560x1440@+0,0",
            "2560x1440@00,0",
            "02560x1440@0,0",
            "2560X1440@0,0",
            "2560x1440 @0,0",
            "2560x1440@0,0 2560x1440@0,0",
            "0x1440@0,0",
            "2560x1440@0",
            "2560x1440",
            "-2560x1440@0,0",
            "2560x1440@0,0,0",
            "99999999999x1440@0,0",
        ] {
            assert_eq!(Layout::parse(other), None, "{other:?}");
        }
    }

    #[test]
    fn a_layout_holds_one_to_sixteen_screens_with_area() {
        assert_eq!(Layout::new([]), None);
        assert_eq!(Layout::new([screen(0, 1080, 0, 0)]), None);
        assert_eq!(Layout::new([screen(1920, 0, 0, 0)]), None);
        let sixteen = (0..16).map(|at| screen(100, 100, at * 100, 0));
        assert!(Layout::new(sixteen).is_some());
        let seventeen = (0..17).map(|at| screen(100, 100, at * 100, 0));
        assert_eq!(Layout::new(seventeen), None);
    }

    #[test]
    fn a_mirrored_screen_counts_once() {
        let mirrored =
            Layout::new([screen(1920, 1080, 0, 0), screen(1920, 1080, 0, 0)]).expect("a layout");
        assert_eq!(mirrored.text(), "1920x1080@0,0");
    }

    #[test]
    fn a_place_shows_when_its_probe_point_is_on_a_screen() {
        let layout = dock();
        // Pushed against the left edge: the frame starts a few pixels off it.
        assert!(layout.shows(Point { x: -7, y: 0 }));
        // On the second screen.
        assert!(layout.shows(Point { x: 3000, y: 500 }));
        // Below the second screen, which is shorter than the first.
        assert!(!layout.shows(Point { x: 3000, y: 1200 }));
        // Left of everything, and far away.
        assert!(!layout.shows(Point { x: -100, y: 0 }));
        assert!(!layout.shows(Point {
            x: i32::MAX,
            y: i32::MAX
        }));
        assert!(!layout.shows(Point {
            x: i32::MIN,
            y: i32::MIN
        }));
    }

    #[test]
    fn the_right_and_bottom_edges_are_outside() {
        let one = Layout::new([screen(100, 100, 0, 0)]).expect("a layout");
        let probe = |x, y| {
            one.shows(Point {
                x: x - PROBE.x,
                y: y - PROBE.y,
            })
        };
        assert!(probe(99, 99));
        assert!(!probe(100, 99));
        assert!(!probe(99, 100));
        assert!(probe(0, 0));
        assert!(!probe(-1, 0));
    }
}
