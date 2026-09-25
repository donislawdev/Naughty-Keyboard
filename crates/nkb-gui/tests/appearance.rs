//! Renders the interface WITHOUT A SCREEN, and proves the result is repeatable.
//!
//! # Why this is a test and not a command
//!
//! Two reasons, and the second is the one that matters.
//!
//! The first: D25 froze the number and the names of the executables as a public
//! contract. A third binary would be a breaking change for the sake of a
//! developer convenience.
//!
//! The second: as a command, repeatable rendering would be something a session
//! *could* use. As a test, it is something that *cannot silently stop working* -
//! and the whole value of rendering off-screen rests on the result being stable,
//! because a picture that differs run to run tells you nothing about the change
//! you just made.
//!
//! # What it is for
//!
//! A session writing an interface has no eyes. It picks a spacing, a colour and
//! a size, and it never finds out whether the result looks like anything. This
//! test writes `target/tmp/appearance/gallery.png`, which the session can then
//! actually look at - the loop being render, look, fix. Document 13 section 4
//! marked repeatable off-screen rendering as NOT MEASURED for any toolkit. ADR-4
//! measured it, and this is where the measurement lives from now on.
//!
//! # What this CANNOT see, and it is not a small hole
//!
//! 🔴 The software renderer and the hardware renderer disagree, and OBS-67 used
//! to record ONE of the ways. A capability sheet of 27 effects, rendered both
//! ways on 2026-09-08, measured five - OBS-83:
//!
//! | | software, ie this picture | hardware, ie the product |
//! |---|---|---|
//! | `drop-shadow-blur` / `-spread` | draws NOTHING AT ALL | a visible glow |
//! | `stroke` on text | barely there | a clear outline |
//! | emoji | monochrome | in colour |
//! | a ZWJ sequence | falls apart into pieces | joined into one glyph |
//! | a glyph the face lacks | nothing | a replacement box |
//!
//! 🔴 The first row is the dangerous one, because it points the wrong way. A
//! screen designed with a shadow looks FLAT here, so a session either deletes a
//! shadow that works or piles on more of one it cannot see. Both mistakes are
//! invisible to anyone reading this picture alone. Before trusting it about a
//! shadow, a glow or an emoji: open a real window.
//!
//! So this picture is not what the user sees. It catches a broken layout, a
//! wrong colour, a clipped label and a drifted spacing - which is what it was
//! built for. It is BLIND to a missing typeface, which is OBS-66, and no amount
//! of staring at the output will reveal one.

// A failed expectation in a test is a failed test.
#![allow(clippy::panic, clippy::expect_used)]

use std::rc::Rc;

use slint::platform::software_renderer::{
    MinimalSoftwareWindow, PremultipliedRgbaColor, RepaintBufferType, TargetPixel,
};
use slint::platform::{Platform, PlatformError, WindowAdapter};

use nkb_gui::Gallery;
use slint::ComponentHandle;

/// One pixel of the target buffer. Slint blends into it, we read it back.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct Pixel {
    r: u8,
    g: u8,
    b: u8,
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

const WIDTH: u32 = 420;
const HEIGHT: u32 = 1280;

fn draw(window: &Rc<MinimalSoftwareWindow>) -> Vec<Pixel> {
    let mut buffer = vec![Pixel::default(); (WIDTH * HEIGHT) as usize];
    window.request_redraw();
    slint::platform::update_timers_and_animations();
    let drew = window.draw_if_needed(|renderer| {
        renderer.render(&mut buffer, WIDTH as usize);
    });
    assert!(drew, "the renderer reported that it had nothing to draw");
    buffer
}

#[test]
fn the_gallery_renders_off_screen_and_renders_the_same_way_twice() {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(Headless {
        window: window.clone(),
    }))
    .expect("no other platform may be installed in this process");

    let gallery = Gallery::new().expect("the gallery must build");
    window.set_size(slint::PhysicalSize::new(WIDTH, HEIGHT));
    gallery.show().expect("the gallery must show");

    let first = draw(&window);
    let second = draw(&window);

    // 🔴 The ink count is not a nicety. An all-background buffer compares equal
    // to itself perfectly well, so without this the determinism assertion above
    // would pass just as happily on a screen that drew absolutely nothing.
    let background = first[0];
    let ink = first.iter().filter(|p| **p != background).count();
    assert!(
        ink > (WIDTH * HEIGHT) as usize / 100,
        "only {ink} of {} pixels differ from the corner colour - the gallery rendered \
         (almost) nothing, so every other assertion here is passing for the wrong reason",
        WIDTH * HEIGHT
    );

    assert!(
        first == second,
        "two renders of an unchanged screen produced different pixels, so comparing \
         appearance between runs is not possible and the loop this test exists to \
         support does not work"
    );

    // Written where a person - or a session - can look at it. Inside target/,
    // which is ignored by git, because a test may not leave artefacts in the
    // working tree.
    let out_dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("appearance");
    std::fs::create_dir_all(&out_dir).expect("the output folder must be creatable");
    let path = out_dir.join("gallery.png");

    let mut bytes = Vec::with_capacity(first.len() * 3);
    for pixel in &first {
        bytes.push(pixel.r);
        bytes.push(pixel.g);
        bytes.push(pixel.b);
    }
    let image = image::RgbImage::from_raw(WIDTH, HEIGHT, bytes)
        .expect("the buffer must match the declared size");
    image.save(&path).expect("the picture must be writable");

    println!("gallery rendered to {}", path.display());
}
