//! The icon's drawings, rendered the way the window renders them.
//!
//! The example that writes `assets/edamame.ico` and the test that checks the file
//! both draw the two SVG files here, through the toolkit's own software renderer.
//! So the picture that goes into the file and the picture the test holds it
//! against come from one place, and that place is the one the window icon is
//! drawn by: Slint hands an SVG to resvg at the size it is asked for.
//!
//! This lives in a subdirectory so that Cargo treats it as a module, and the
//! example includes it with a `#[path]`. Nothing here is a test or an example.
//!
//! ⚠️ `set_platform` may be called once per process, so a program makes one
//! `Renderer` and keeps it. A second one panics, which is the useful behaviour:
//! it would otherwise draw into the first one's window.

use std::path::{Path, PathBuf};
use std::rc::Rc;

use slint::ComponentHandle;
use slint::platform::software_renderer::{
    MinimalSoftwareWindow, PremultipliedRgbaColor, RepaintBufferType, TargetPixel,
};
use slint::platform::{Platform, PlatformError, WindowAdapter};

/// The sizes the file holds: 16, 20, 24 and 32 for the title bar and the taskbar
/// at 100, 125, 150 and 200 % scaling, 32, 40, 48 and 64 for large icons, 256 for
/// the file manager's extra large view. 128 is there for tools that look for it.
pub const SIZES: [u32; 9] = [16, 20, 24, 32, 40, 48, 64, 128, 256];

/// The entries of the macOS icon, `assets/edamame.icns`, in the order the file
/// holds them: the four-letter type macOS looks an entry up by, and its side in
/// pixels. Ten, the set Apple's own `iconutil` writes. Several share a size - a
/// 32 px picture is both the 32 point icon of a plain screen and the 16 point
/// icon of a Retina one - and macOS asks for each one by its type.
pub const MAC_ENTRIES: [(&str, u32); 10] = [
    ("icp4", 16),
    ("icp5", 32),
    ("ic11", 32),
    ("ic12", 64),
    ("ic07", 128),
    ("ic13", 256),
    ("ic08", 256),
    ("ic14", 512),
    ("ic09", 512),
    ("ic10", 1024),
];

/// Up to this size the heavier drawing is used. See `assets/README.md`.
pub const SMALL_UP_TO: u32 = 32;

/// The drawing an entry of this size is made from, as a name inside `assets/`.
pub fn drawing_for(size: u32) -> &'static str {
    if size <= SMALL_UP_TO {
        "edamame-small.svg"
    } else {
        "edamame.svg"
    }
}

pub fn assets_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("assets")
}

slint::slint! {
    export component Canvas inherits Window {
        in property <image> drawing;
        background: transparent;
        Image {
            source: root.drawing;
            width: parent.width;
            height: parent.height;
        }
    }
}

/// A pixel as the renderer blends it: the colour already multiplied by the
/// opacity, and an opacity that starts at nothing.
#[derive(Clone, Copy, Default)]
struct Premultiplied {
    red: u8,
    green: u8,
    blue: u8,
    alpha: u8,
}

impl TargetPixel for Premultiplied {
    fn blend(&mut self, source: PremultipliedRgbaColor) {
        let through = 255 - u32::from(source.alpha);
        let over = |below: u8, above: u8| (u32::from(below) * through / 255) as u8 + above;
        self.red = over(self.red, source.red);
        self.green = over(self.green, source.green);
        self.blue = over(self.blue, source.blue);
        self.alpha = over(self.alpha, source.alpha);
    }

    fn from_rgb(red: u8, green: u8, blue: u8) -> Self {
        Self {
            red,
            green,
            blue,
            alpha: 255,
        }
    }

    // The default is opaque black, which the renderer lays down first under a
    // window whose own background is clear. An icon is a shape on nothing.
    fn background() -> Self {
        Self::default()
    }
}

/// The same pixel with the colour and the opacity apart, which is how a PNG and
/// the bitmap inside an icon file keep it.
fn straight(pixel: Premultiplied) -> [u8; 4] {
    if pixel.alpha == 0 {
        return [0; 4];
    }
    let alpha = u32::from(pixel.alpha);
    let lift = |colour: u8| ((u32::from(colour) * 255 + alpha / 2) / alpha).min(255) as u8;
    [
        lift(pixel.red),
        lift(pixel.green),
        lift(pixel.blue),
        pixel.alpha,
    ]
}

struct Headless {
    window: Rc<MinimalSoftwareWindow>,
}

impl Platform for Headless {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        Ok(self.window.clone())
    }
}

pub struct Renderer {
    window: Rc<MinimalSoftwareWindow>,
    canvas: Canvas,
}

impl Renderer {
    pub fn new() -> Self {
        let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
        slint::platform::set_platform(Box::new(Headless {
            window: window.clone(),
        }))
        .expect("no other platform may be installed in this process");
        let canvas = Canvas::new().expect("the canvas must build");
        canvas.show().expect("the canvas must show");
        Self { window, canvas }
    }

    /// The drawing in `assets/` at `size` pixels square: colour and opacity
    /// apart, rows top first, a background that is clear.
    pub fn draw(&self, name: &str, size: u32) -> Vec<[u8; 4]> {
        let path = assets_dir().join(name);
        let drawing = slint::Image::load_from_path(&path)
            .unwrap_or_else(|_| panic!("{} must load as an image", path.display()));
        self.window.set_size(slint::PhysicalSize::new(size, size));
        self.canvas.set_drawing(drawing);
        let mut buffer = vec![Premultiplied::default(); (size * size) as usize];
        self.window.request_redraw();
        slint::platform::update_timers_and_animations();
        let drew = self.window.draw_if_needed(|renderer| {
            renderer.render(&mut buffer, size as usize);
        });
        assert!(drew, "the renderer reported that it had nothing to draw");
        buffer.into_iter().map(straight).collect()
    }
}
