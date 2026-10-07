//! Rendering a screen WITHOUT A SCREEN, shared by the tests that look at one.
//!
//! This lives in a subdirectory so that Cargo treats it as a module rather than
//! as a test binary of its own. It is a module and not a copy in each file for
//! the plainest of reasons: `slint::platform::set_platform` may be called ONCE
//! per process, so every test that renders has to be its own binary, and a
//! harness copied into each of those binaries is a harness that drifts.

use std::rc::Rc;

use slint::platform::software_renderer::{
    MinimalSoftwareWindow, PremultipliedRgbaColor, RepaintBufferType, TargetPixel,
};
use slint::platform::{Platform, PlatformError, WindowAdapter};

/// One pixel of the target buffer. Slint blends into it, we read it back.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Pixel {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl TargetPixel for Pixel {
    fn blend(&mut self, colour: PremultipliedRgbaColor) {
        let inverse = 255 - colour.alpha;
        self.r = (u32::from(self.r) * u32::from(inverse) / 255) as u8 + colour.red;
        self.g = (u32::from(self.g) * u32::from(inverse) / 255) as u8 + colour.green;
        self.b = (u32::from(self.b) * u32::from(inverse) / 255) as u8 + colour.blue;
    }

    fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
}

struct Headless {
    window: Rc<MinimalSoftwareWindow>,
}

impl Platform for Headless {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        Ok(self.window.clone())
    }
}

/// Installs the headless platform and returns the window everything draws into.
///
/// Panics if called twice, and that is the useful behaviour: a second call means
/// two rendering tests ended up in one binary, where the second would silently
/// draw into the first one's window.
pub fn start(width: u32, height: u32) -> Rc<MinimalSoftwareWindow> {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(Headless {
        window: window.clone(),
    }))
    .expect("no other platform may be installed in this process");
    window.set_size(slint::PhysicalSize::new(width, height));
    window
}

pub fn draw(window: &Rc<MinimalSoftwareWindow>, width: u32, height: u32) -> Vec<Pixel> {
    let mut buffer = vec![Pixel::default(); (width * height) as usize];
    window.request_redraw();
    slint::platform::update_timers_and_animations();
    let drew = window.draw_if_needed(|renderer| {
        renderer.render(&mut buffer, width as usize);
    });
    assert!(drew, "the renderer reported that it had nothing to draw");
    buffer
}

/// Writes the buffer where a person - or a session with no eyes - can look at
/// it. Inside `target/`, which git ignores, because a test may not leave
/// artefacts in the working tree.
pub fn save(buffer: &[Pixel], width: u32, height: u32, name: &str) -> std::path::PathBuf {
    let out_dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("appearance");
    std::fs::create_dir_all(&out_dir).expect("the output folder must be creatable");
    let path = out_dir.join(name);

    let mut bytes = Vec::with_capacity(buffer.len() * 3);
    for pixel in buffer {
        bytes.push(pixel.r);
        bytes.push(pixel.g);
        bytes.push(pixel.b);
    }
    let image = image::RgbImage::from_raw(width, height, bytes)
        .expect("the buffer must match the declared size");
    image.save(&path).expect("the picture must be writable");
    path
}

/// The same, cropped to what the window actually covers.
///
/// ⚠️ For a window whose own background is transparent, the buffer is bigger
/// than the window: everything the window does not cover keeps the buffer's
/// initial value. Saving the whole buffer therefore shows a tall empty box under
/// a short palette, and a session judging the picture would be judging the
/// buffer rather than the window - the failure class of `OBS-83`, where the
/// thing looked at is not the thing shipped.
///
/// Cropping is only ever applied to the PICTURE. Every measurement in this
/// project's tests runs over the full buffer, so a stray pixel outside the crop
/// still counts.
pub fn save_cropped(buffer: &[Pixel], width: u32, height: u32, name: &str) -> std::path::PathBuf {
    let empty = Pixel::default();
    let mut top = height;
    let mut bottom = 0_u32;
    let mut left = width;
    let mut right = 0_u32;
    for y in 0..height {
        for x in 0..width {
            if buffer[(y * width + x) as usize] != empty {
                top = top.min(y);
                bottom = bottom.max(y);
                left = left.min(x);
                right = right.max(x);
            }
        }
    }
    if top > bottom || left > right {
        // Nothing was drawn. Save the buffer whole rather than guess a crop -
        // the empty picture is itself the report.
        return save(buffer, width, height, name);
    }

    let (w, h) = (right - left + 1, bottom - top + 1);
    let mut cropped = Vec::with_capacity((w * h) as usize);
    for y in top..=bottom {
        for x in left..=right {
            cropped.push(buffer[(y * width + x) as usize]);
        }
    }
    save(&cropped, w, h, name)
}

/// How many pixels carry exactly this colour.
///
/// Exact rather than near, and that is the point: the software renderer blends a
/// glyph with its background by coverage, so the CORE of a stroke comes out at
/// the colour itself. Measured on the gallery on 2026-09-22: 307 pixels exactly
/// `text-muted`, 643 exactly `text-primary`. A near-match would also count the
/// antialiased edges of a neighbouring role and stop meaning one thing.
pub fn count_exactly(buffer: &[Pixel], rgb: (u8, u8, u8)) -> usize {
    let wanted = Pixel {
        r: rgb.0,
        g: rgb.1,
        b: rgb.2,
    };
    buffer.iter().filter(|p| **p == wanted).count()
}

/// A box on a render, in pixels.
#[derive(Debug, Clone, Copy)]
pub struct Ink {
    pub left: u32,
    pub right: u32,
    pub top: u32,
    pub bottom: u32,
}

impl Ink {
    /// Where a pointer aims at it.
    pub fn centre(self) -> slint::LogicalPosition {
        slint::LogicalPosition::new(
            ((self.left + self.right) / 2) as f32,
            ((self.top + self.bottom) / 2) as f32,
        )
    }
}

/// The box of the pixels that differ between two renders of one size, or
/// `None` when they are the same picture.
///
/// This is how the pointer tests find a control: render without its words and
/// with them, and the difference is where it IS - not where the source says it
/// should be. `None` is how they say a control is NOT drawn.
pub fn added(before: &[Pixel], after: &[Pixel], width: u32, height: u32) -> Option<Ink> {
    let hits: Vec<(u32, u32)> = (0..height)
        .flat_map(|y| (0..width).map(move |x| (x, y)))
        .filter(|&(x, y)| before[(y * width + x) as usize] != after[(y * width + x) as usize])
        .collect();
    let xs = hits.iter().map(|&(x, _)| x);
    let ys = hits.iter().map(|&(_, y)| y);
    Some(Ink {
        left: xs.clone().min()?,
        right: xs.max()?,
        top: ys.clone().min()?,
        bottom: ys.max()?,
    })
}

/// The two ways of a two-way switch (`Segmented`) as boxes, or `None` when the
/// switch is not drawn. `set` puts a pair of words on the switch, and the
/// words are back in place on return, so a click aims at the switch as the
/// tester sees it.
///
/// Found by the ink the words add, like every control here - with one twist
/// the first try measured (2026-10-07): blanking the FIRST word shrinks its
/// half and moves the second to the left, so that difference covers both. The
/// LAST word moves nothing before it, so its box is exact, and the first runs
/// from where its own ink starts to where the second half begins.
pub fn two_ways(
    window: &Rc<MinimalSoftwareWindow>,
    width: u32,
    height: u32,
    words: [&str; 2],
    set: &dyn Fn([&str; 2]),
) -> Option<[Ink; 2]> {
    let found = |blank: [&str; 2]| {
        set(blank);
        let without = draw(window, width, height);
        set(words);
        let with = draw(window, width, height);
        added(&without, &with, width, height)
    };
    let second = found([words[0], ""])?;
    let both = found(["", words[1]])?;
    Some([
        Ink {
            left: both.left,
            right: second.left.saturating_sub(1),
            top: both.top,
            bottom: both.bottom,
        },
        second,
    ])
}

/// Moves the pointer to `at`.
pub fn point(window: &slint::Window, at: slint::LogicalPosition) {
    window.dispatch_event(slint::platform::WindowEvent::PointerMoved { position: at });
}

/// Moves the pointer to `at` and clicks there with the left button.
pub fn click(window: &slint::Window, at: slint::LogicalPosition) {
    point(window, at);
    let button = slint::platform::PointerEventButton::Left;
    window.dispatch_event(slint::platform::WindowEvent::PointerPressed {
        position: at,
        button,
    });
    window.dispatch_event(slint::platform::WindowEvent::PointerReleased {
        position: at,
        button,
    });
}
