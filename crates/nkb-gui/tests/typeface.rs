//! Proves the product's monospaced roles are monospaced ON THIS MACHINE.
//!
//! # The failure being guarded against
//!
//! The dictionary used to name "Consolas", a family that exists only on
//! Windows. OBS-72 measured what that did elsewhere: `font_family` in Slint is
//! ONE family name with no fallback list, so on macOS and on Linux the
//! monospaced roles silently became proportional. Nothing failed, nothing was
//! logged, and the reason monospace was chosen - a changing counter must not
//! shift the characters beside it - disappeared without trace.
//!
//! That is a defect of a particular shape: invisible to the author, visible
//! only to someone on another system, and reintroduced by any edit that touches
//! one string in the dictionary.
//!
//! # Why TWO checks and not one
//!
//! Neither is sufficient, and they fail in different directions.
//!
//! The static check reads the shipped font's own name table and compares it to
//! the family the dictionary names. It catches the token drifting from the
//! file, the file being deleted, and the file being swapped for a different
//! face. It cannot tell whether the result is usable.
//!
//! The rendering check draws with that family and measures whether the glyphs
//! advance by equal steps. It catches registration being broken, the name being
//! unresolvable, and a shipped file that is not actually a monospaced face.
//!
//! 🔴 That second check is BLIND to the font going missing from the binary, and
//! this is measured rather than feared: deleting the `import` line from the
//! dictionary left both the numbers and the verdict unchanged, because this
//! machine carries DejaVu in C:\Windows\Fonts and the name resolved from there.
//! A machine without it would have failed. So the rendering check answers "are
//! the roles monospaced HERE", never "is the product carrying its own face" -
//! and the static check is the one that answers the second question.
//!
//! # Proof that each check is alive
//!
//! Each rule has a mutation that lights IT AND NOTHING ELSE, because a rule
//! that only ever fails alongside another one cannot be told apart from a rule
//! that does nothing:
//!
//! - dictionary back to "Consolas" -> static fails, rendering passes (Consolas
//!   is present and monospaced on Windows, so rendering has no complaint);
//! - ui/fonts/ holding DejaVu Sans instead of DejaVu Sans Mono, with the
//!   dictionary naming it correctly -> static passes, rendering fails at 67%.
//!
//! And the negative control: with neither mutation applied, both pass, at a
//! spread of 0% against the default face's 73%.
//!
//! # What neither check sees
//!
//! Coverage. This says the roles are monospaced; it says nothing about whether
//! a given character has a glyph at all. That gap is OBS-66, it belongs to the
//! preview rather than to the counters, and it is open.

// A failed expectation in a test is a failed test.
#![allow(clippy::panic, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::rc::Rc;

use slint::ComponentHandle;
use slint::platform::software_renderer::{
    MinimalSoftwareWindow, PremultipliedRgbaColor, RepaintBufferType, TargetPixel,
};
use slint::platform::{Platform, PlatformError, WindowAdapter};

use nkb_gui::Gallery;

fn ui_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("ui")
}

fn font_path() -> PathBuf {
    ui_dir().join("fonts").join("DejaVuSansMono.ttf")
}

// ---------------------------------------------------------------------------
// The font's own name table
// ---------------------------------------------------------------------------

fn be_u16(data: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes([
        *data.get(at)?,
        *data.get(at.checked_add(1)?)?,
    ]))
}

fn be_u32(data: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes([
        *data.get(at)?,
        *data.get(at.checked_add(1)?)?,
        *data.get(at.checked_add(2)?)?,
        *data.get(at.checked_add(3)?)?,
    ]))
}

/// Every family name (name ID 1) the file declares, decoded from both the
/// Macintosh and the Windows encodings. Returns a list rather than one string
/// because a font may spell its family differently per platform, and the
/// dictionary only has to match one of them.
fn declared_families(data: &[u8]) -> Vec<String> {
    let table_count = be_u16(data, 4).expect("the font must carry a table directory");
    let mut name_table = None;
    for i in 0..usize::from(table_count) {
        let entry = 12 + i * 16;
        if data.get(entry..entry + 4) == Some(b"name") {
            name_table = be_u32(data, entry + 8).map(|o| o as usize);
        }
    }
    let name_table = name_table.expect("the font must carry a `name` table");

    let records = be_u16(data, name_table + 2).expect("the name table must carry a record count");
    let storage =
        name_table + usize::from(be_u16(data, name_table + 4).expect("string offset must be read"));

    let mut families = Vec::new();
    for i in 0..usize::from(records) {
        let record = name_table + 6 + i * 12;
        let Some(platform) = be_u16(data, record) else {
            continue;
        };
        let Some(name_id) = be_u16(data, record + 6) else {
            continue;
        };
        if name_id != 1 {
            continue;
        }
        let (Some(length), Some(offset)) = (be_u16(data, record + 8), be_u16(data, record + 10))
        else {
            continue;
        };
        let from = storage + usize::from(offset);
        let Some(raw) = data.get(from..from + usize::from(length)) else {
            continue;
        };
        // Platform 3 is Windows and spells its strings in UTF-16BE; platform 1
        // is Macintosh and uses one byte per character.
        let text = if platform == 3 {
            let units: Vec<u16> = raw
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| u16::from_be_bytes(*pair))
                .collect();
            String::from_utf16_lossy(&units)
        } else {
            raw.iter().map(|b| char::from(*b)).collect()
        };
        if !families.contains(&text) {
            families.push(text);
        }
    }
    families
}

/// The family the dictionary asks for, read out of the dictionary itself so
/// that this test cannot drift away from the thing it is checking.
fn family_named_by_the_dictionary() -> String {
    let tokens = std::fs::read_to_string(ui_dir().join("tokens.slint"))
        .expect("ui/tokens.slint must be readable");
    for line in tokens.lines() {
        let line = line.trim();
        if line.starts_with("//") {
            continue;
        }
        let Some(rest) = line.split_once("font-mono:") else {
            continue;
        };
        let value = rest.1.trim().trim_end_matches(';').trim();
        return value.trim_matches('"').to_string();
    }
    panic!("ui/tokens.slint declares no `font-mono` property any more");
}

#[test]
fn the_dictionary_names_exactly_the_typeface_the_product_ships() {
    let path = font_path();
    assert!(
        path.is_file(),
        "{} is missing. The dictionary names a family that the product is supposed to \
         CARRY; without the file the name falls through to whatever the machine happens \
         to have, which is the OBS-72 defect returning.",
        path.display()
    );

    let data = std::fs::read(&path).expect("the shipped font must be readable");
    let families = declared_families(&data);
    let wanted = family_named_by_the_dictionary();

    assert!(
        families.contains(&wanted),
        "ui/tokens.slint asks for the family {wanted:?}, but the font shipped in \
         ui/fonts/ declares {families:?}. A family name that no shipped file answers to \
         does not fail loudly - it degrades to the system's default face, on someone \
         else's machine."
    );
}

// ---------------------------------------------------------------------------
// Does it actually render monospaced?
// ---------------------------------------------------------------------------

slint::slint! {
    // Deliberately not part of the product's own screens: this is a measuring
    // instrument, and putting it in ui/ would make the gallery hold a test
    // fixture. It takes the family by property so the same component can draw
    // the shipped face and the control.
    export component Ruler inherits Window {
        in property <string> sample;
        in property <string> family;
        width: 900px;
        height: 90px;
        background: #000000;
        Text {
            x: 4px;
            y: 4px;
            text: root.sample;
            font-family: root.family;
            font-size: 40px;
            color: #FFFFFF;
        }
    }
}

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

const WIDTH: u32 = 900;
const HEIGHT: u32 = 90;

/// The x of the rightmost inked column, which for a run of identical letters is
/// a stand-in for how far the text advanced.
fn ink_reaches(window: &Rc<MinimalSoftwareWindow>) -> u32 {
    let mut buffer = vec![Pixel::default(); (WIDTH * HEIGHT) as usize];
    window.request_redraw();
    slint::platform::update_timers_and_animations();
    let drew = window.draw_if_needed(|renderer| {
        renderer.render(&mut buffer, WIDTH as usize);
    });
    assert!(drew, "the renderer reported that it had nothing to draw");

    let background = Pixel { r: 0, g: 0, b: 0 };
    let mut rightmost = 0;
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            if buffer[(y * WIDTH + x) as usize] != background {
                rightmost = rightmost.max(x);
            }
        }
    }
    rightmost
}

const NARROW: &str = "iiiiiiiiii";
const WIDE: &str = "MMMMMMMMMM";

/// How far the ten wide letters outrun the ten narrow ones, as a percentage of
/// the wide run. Zero means every glyph advanced by the same step.
fn width_spread(
    ruler: &Ruler,
    window: &Rc<MinimalSoftwareWindow>,
    family: &str,
) -> (u32, u32, u32) {
    ruler.set_family(family.into());

    ruler.set_sample(NARROW.into());
    let narrow = ink_reaches(window);

    ruler.set_sample(WIDE.into());
    let wide = ink_reaches(window);

    assert!(
        narrow > 0 && wide > 0,
        "family {family:?} drew nothing at all, so nothing below it means anything"
    );

    let spread = wide.saturating_sub(narrow) * 100 / wide.max(1);
    (narrow, wide, spread)
}

#[test]
fn the_monospaced_roles_really_do_advance_by_equal_steps() {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(Headless {
        window: window.clone(),
    }))
    .expect("no other platform may be installed in this process");

    // Building a screen from ui/all.slint is what registers the imported font
    // with this process. Doing it through the product's own component - rather
    // than importing the .ttf a second time here - is the point: what is being
    // measured is the product's registration, not this test's.
    let gallery = Gallery::new().expect("the gallery must build");
    gallery.show().expect("the gallery must show");
    gallery.hide().expect("the gallery must hide again");

    let ruler = Ruler::new().expect("the ruler must build");
    window.set_size(slint::PhysicalSize::new(WIDTH, HEIGHT));
    ruler.show().expect("the ruler must show");

    let family = family_named_by_the_dictionary();
    let (mono_narrow, mono_wide, mono_spread) = width_spread(&ruler, &window, &family);

    // The control. An empty family is the machine's own default face, which on
    // every desktop system is proportional - and if it is not, this assertion
    // fails and says so, rather than letting the real check pass for want of
    // anything to compare against.
    let (ctl_narrow, ctl_wide, ctl_spread) = width_spread(&ruler, &window, "");

    println!(
        "shipped {family:?}: i-run reaches {mono_narrow}, M-run reaches {mono_wide}, spread {mono_spread}%"
    );
    println!(
        "control <default>:  i-run reaches {ctl_narrow}, M-run reaches {ctl_wide}, spread {ctl_spread}%"
    );

    assert!(
        ctl_spread > 25,
        "NEGATIVE CONTROL FAILED: this machine's default face put ten M's only \
         {ctl_spread}% further than ten i's, so it is monospaced too and this test cannot \
         tell a monospaced face from a proportional one here. The result below would be \
         meaningless."
    );

    assert!(
        mono_spread < 10,
        "the family {family:?} put ten M's {mono_spread}% further along than ten i's \
         ({mono_narrow} against {mono_wide}), so it is NOT rendering monospaced. Two \
         causes look identical from here and both are real: the name resolved to \
         nothing and fell through to this machine's default face, whose own spread is \
         {ctl_spread}%; or it resolved perfectly well to a shipped file that is simply \
         not a monospaced face - ui/fonts/ holding the proportional sibling, whose \
         filename differs by three letters. Either way it is OBS-72 again: no error, \
         no log, just a counter that shifts the characters beside it."
    );
}
