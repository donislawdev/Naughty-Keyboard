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
//! The screens, each as its rectangle on the virtual desktop, in the unit the
//! window system takes a window's position in: PHYSICAL pixels on Windows and
//! POINTS on macOS (`nkb_sys::screens` says why). The scale a screen is set to
//! is not part of it. On Windows changing the scale keeps the physical
//! rectangle, and a place in physical pixels stays where it was. On macOS a
//! display's size in points is the resolution it is set to, so a new
//! resolution is a new layout and gets no remembered place.
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
/// screen, in the unit of the layout: right and down.
///
/// Not the corner itself, because the outer frame of a window on Windows 10
/// and 11 has invisible borders - a window pushed against the left edge of a
/// screen stands at a NEGATIVE x of a few pixels and is fully visible. The
/// point is inside the title bar of any window at any scale this tool meets.
pub const PROBE: Point = Point { x: 32, y: 16 };

/// One screen: its rectangle on the virtual desktop, in the unit of the layout.
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

/// A point on the virtual desktop in the unit of the layout - the top left
/// corner of a window's outer frame, when it is a place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

/// A rectangle on the virtual desktop: a window's frame, or the screen it
/// stands on - in one unit, whichever the caller measures both in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Where a window opened from the palette stands - the value window, the
/// shortcuts window, About and the rest: beside the palette rather than where
/// the system puts a new window, which is often the other end of the screen
/// (the owner's point 3, 2026-11-16).
///
/// To the palette's right, `gap` away, where the screen has room for the whole
/// window, else to its left, else over the palette, as far left as it fits.
/// Its top level with the palette's, moved up as far as the screen's bottom
/// needs. A window larger than the screen keeps its top left corner on it,
/// because that is where its title bar is.
///
/// `screen` is where on the palette's screen the window may stand: the part
/// windows may use, where the system says it (`D190`), else the whole screen -
/// where a taskbar along its bottom edge may still cover the bottom of a window
/// pushed down there.
#[must_use]
pub fn beside(palette: Rect, window: (u32, u32), screen: Rect, gap: u32) -> Point {
    let (width, height) = (i64::from(window.0), i64::from(window.1));
    let gap = i64::from(gap);
    let (left, top) = (i64::from(screen.x), i64::from(screen.y));
    let right = left + i64::from(screen.width);
    let bottom = top + i64::from(screen.height);
    let palette_left = i64::from(palette.x);
    let palette_right = palette_left + i64::from(palette.width);
    let after = palette_right + gap;
    let before = palette_left - gap - width;
    let x = if after + width <= right {
        after
    } else if before >= left {
        before
    } else {
        palette_left
    };
    // Into the screen: the right edge first and the left one last, so a window
    // wider than the screen keeps its left edge - and its title - on it. The
    // same for the bottom and the top.
    let x = x.min(right - width).max(left);
    let y = i64::from(palette.y).min(bottom - height).max(top);
    Point {
        x: saturate(x),
        y: saturate(y),
    }
}

/// How tall a window may be on a screen too short for its content: from the top
/// of its content down to the bottom of `area`, the part of the screen windows
/// may use - the screen less the taskbar, the Dock, the menu bar or a panel.
/// In the unit of both.
///
/// Zero when that top stands at or below the area's bottom - a window put that
/// low has no room, keeps what never scrolls and stands past the edge, as any
/// window does there. A top above the area's top (a window pushed up past it)
/// counts from where the window is, so the bottom edge stays on the area.
#[must_use]
pub fn room_below(top: i32, area: Rect) -> u32 {
    let bottom = i64::from(area.y) + i64::from(area.height);
    let room = (bottom - i64::from(top)).max(0);
    u32::try_from(room).unwrap_or(u32::MAX)
}

impl Rect {
    /// The part two rectangles share, or `None` where they do not overlap -
    /// edges that only touch share nothing, as with every rectangle the window
    /// system reports. The work area one system gives for the whole desktop,
    /// met with the screen a window stands on, is that screen's work area.
    #[must_use]
    pub fn meet(self, other: Self) -> Option<Self> {
        let left = i64::from(self.x).max(i64::from(other.x));
        let top = i64::from(self.y).max(i64::from(other.y));
        let right = (i64::from(self.x) + i64::from(self.width))
            .min(i64::from(other.x) + i64::from(other.width));
        let bottom = (i64::from(self.y) + i64::from(self.height))
            .min(i64::from(other.y) + i64::from(other.height));
        Some(Self {
            x: i32::try_from(left).ok()?,
            y: i32::try_from(top).ok()?,
            width: u32::try_from(right - left)
                .ok()
                .filter(|width| *width > 0)?,
            height: u32::try_from(bottom - top)
                .ok()
                .filter(|height| *height > 0)?,
        })
    }
}

/// A coordinate worked out in a wider type, back in the type of a place. Every
/// value [`beside`] works out lies between two coordinates that fit, so this
/// only keeps the arithmetic free of a cast that could wrap.
fn saturate(value: i64) -> i32 {
    i32::try_from(value).unwrap_or(if value < 0 { i32::MIN } else { i32::MAX })
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

    const fn rect(x: i32, y: i32, width: u32, height: u32) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    /// A full HD screen, a palette 480 wide and a window 520 by 600 - the
    /// value window's default size.
    const SCREEN: Rect = rect(0, 0, 1920, 1080);
    const WINDOW: (u32, u32) = (520, 600);

    /// A laptop at 1366 x 768 with a taskbar 48 high along its bottom.
    const WORK_AREA: Rect = rect(0, 0, 1366, 720);

    #[test]
    fn the_room_runs_from_the_top_of_the_content_to_the_bottom_of_the_work_area() {
        assert_eq!(room_below(100, WORK_AREA), 620);
        assert_eq!(room_below(0, WORK_AREA), 720);
        // A screen below and left of the main one: the same arithmetic.
        assert_eq!(room_below(1200, rect(-1366, 1080, 1366, 720)), 600);
        // Pushed up past the area's top, the window's top still counts.
        assert_eq!(room_below(-40, WORK_AREA), 760);
    }

    #[test]
    fn a_window_put_at_or_below_the_bottom_of_the_work_area_has_no_room() {
        assert_eq!(room_below(720, WORK_AREA), 0);
        assert_eq!(room_below(900, WORK_AREA), 0);
        // The extremes of a coordinate do not wrap.
        assert_eq!(room_below(i32::MAX, WORK_AREA), 0);
        assert_eq!(
            room_below(i32::MIN, rect(i32::MAX, i32::MAX, u32::MAX, u32::MAX)),
            u32::MAX
        );
    }

    #[test]
    fn a_work_area_of_the_whole_desktop_met_with_one_screen_is_that_screens_part() {
        // A desktop of two screens side by side, a panel 32 high along the
        // top of both: the work area the desktop gives, met with each screen.
        let desktop = rect(0, 32, 3200, 1048);
        assert_eq!(
            desktop.meet(rect(0, 0, 1920, 1080)),
            Some(rect(0, 32, 1920, 1048))
        );
        assert_eq!(
            desktop.meet(rect(1920, 0, 1280, 800)),
            Some(rect(1920, 32, 1280, 768))
        );
        // Order does not matter.
        assert_eq!(
            rect(1920, 0, 1280, 800).meet(desktop),
            Some(rect(1920, 32, 1280, 768))
        );
    }

    #[test]
    fn rectangles_that_only_touch_or_lie_apart_share_nothing() {
        assert_eq!(rect(0, 0, 100, 100).meet(rect(100, 0, 100, 100)), None);
        assert_eq!(rect(0, 0, 100, 100).meet(rect(0, 100, 100, 100)), None);
        assert_eq!(rect(0, 0, 100, 100).meet(rect(300, 300, 10, 10)), None);
        // The widest rectangles do not wrap.
        let all = rect(i32::MIN, i32::MIN, u32::MAX, u32::MAX);
        assert_eq!(all.meet(rect(5, 5, 10, 10)), Some(rect(5, 5, 10, 10)));
    }

    #[test]
    fn a_window_opens_to_the_right_of_the_palette_level_with_its_top() {
        let palette = rect(300, 120, 480, 700);
        assert_eq!(beside(palette, WINDOW, SCREEN, 8), Point { x: 788, y: 120 });
    }

    #[test]
    fn with_no_room_on_the_right_it_opens_to_the_left() {
        let palette = rect(1400, 120, 480, 700);
        assert_eq!(beside(palette, WINDOW, SCREEN, 8), Point { x: 872, y: 120 });
        // Exactly the room it needs on the right is room.
        let snug = rect(1920 - 480 - 8 - 520, 0, 480, 300);
        assert_eq!(beside(snug, WINDOW, SCREEN, 8).x, 1920 - 520);
    }

    #[test]
    fn with_room_on_neither_side_it_opens_over_the_palette_and_on_the_screen() {
        let narrow = rect(0, 0, 1280, 720);
        let palette = rect(400, 50, 480, 600);
        assert_eq!(beside(palette, WINDOW, narrow, 8), Point { x: 400, y: 50 });
        // A wide window and a palette near the right edge: over it, pulled in
        // so the window ends at the screen's edge.
        let right = rect(700, 50, 480, 600);
        assert_eq!(
            beside(right, (700, 600), narrow, 8),
            Point { x: 580, y: 50 }
        );
    }

    #[test]
    fn a_palette_low_on_the_screen_lifts_the_window_onto_it() {
        let palette = rect(300, 900, 480, 150);
        assert_eq!(beside(palette, WINDOW, SCREEN, 8), Point { x: 788, y: 480 });
    }

    #[test]
    fn a_window_larger_than_the_screen_keeps_its_top_left_corner_on_it() {
        let small = rect(0, 0, 400, 300);
        let palette = rect(100, 100, 200, 100);
        assert_eq!(beside(palette, WINDOW, small, 8), Point { x: 0, y: 0 });
    }

    #[test]
    fn a_screen_left_of_and_above_the_main_one_is_kept_to() {
        let left = rect(-1920, -200, 1920, 1080);
        let palette = rect(-600, -150, 480, 700);
        assert_eq!(
            beside(palette, WINDOW, left, 8),
            Point { x: -1128, y: -150 }
        );
        // A palette half off its screen still opens the window on the screen.
        let off = rect(-2100, 700, 480, 400);
        let at = beside(off, WINDOW, left, 8);
        assert!(at.x >= -1920 && at.y + 600 <= 880, "{at:?}");
    }

    #[test]
    fn the_extremes_of_a_coordinate_do_not_wrap() {
        let far = rect(i32::MAX - 10, i32::MAX - 10, u32::MAX, u32::MAX);
        let at = beside(far, (u32::MAX, u32::MAX), far, u32::MAX);
        assert_eq!(at, Point { x: far.x, y: far.y });
        let low = rect(i32::MIN, i32::MIN, 10, 10);
        let at = beside(low, WINDOW, low, 8);
        assert_eq!(
            at,
            Point {
                x: i32::MIN,
                y: i32::MIN
            }
        );
    }

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
