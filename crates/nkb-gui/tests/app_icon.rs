//! The application icon: what the file holds, and that everything meant to show
//! it names it.
//!
//! # The claim being guarded
//!
//! A program's icon fails quietly. A mangled offset in the `.ico`, a window
//! nobody gave the icon to, a resource script pointing at a file that was
//! renamed - none of them stops the build or any other test, and each one is
//! found as a blank square on somebody else's desktop. The Windows half of the
//! work, embedding the file in the executable, cannot be exercised on the
//! machines that run these tests, so what is checked here is everything that
//! can be: the file, the names, and the two drawings behind it.
//!
//! # What none of it can check
//!
//! That Windows shows the icon. That is looked at on a desktop, not asserted.
//! What the file holds is checked against the drawings it is made from, so a
//! changed drawing with an old `.ico` turns this red until
//! `cargo run -p nkb-gui --example make_icon` is run and its output committed.

// A failed expectation in a test is a failed test.
#![allow(clippy::panic, clippy::expect_used)]

// The two drawings, drawn by the toolkit's own renderer. Shared with the example
// that writes the file, which uses the half of it this test does not.
#[allow(dead_code)]
mod icon_render;

use std::path::{Path, PathBuf};

use icon_render::{Renderer, SIZES, drawing_for};

fn crate_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn read_text(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("{} must be readable: {e}", path.display()))
}

fn read_bytes(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|e| panic!("{} must be readable: {e}", path.display()))
}

fn le16(bytes: &[u8], at: usize) -> u16 {
    let raw = bytes
        .get(at..at + 2)
        .unwrap_or_else(|| panic!("the file ends before byte {at}"));
    u16::from_le_bytes([raw[0], raw[1]])
}

fn le32(bytes: &[u8], at: usize) -> u32 {
    let raw = bytes
        .get(at..at + 4)
        .unwrap_or_else(|| panic!("the file ends before byte {at}"));
    u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]])
}

/// One picture of the icon file, as its directory describes it.
struct Entry<'a> {
    size: u32,
    data: &'a [u8],
}

/// The directory of an `.ico`: what it says it holds and where.
fn entries(ico: &[u8]) -> Vec<Entry<'_>> {
    assert_eq!(le16(ico, 0), 0, "the first two bytes of an icon file are 0");
    assert_eq!(le16(ico, 2), 1, "type 1 is an icon, 2 would be a cursor");
    let count = usize::from(le16(ico, 4));
    (0..count)
        .map(|i| {
            let at = 6 + 16 * i;
            let width = u32::from(ico[at]);
            let height = u32::from(ico[at + 1]);
            // 0 stands for 256 in a directory entry.
            let size = if width == 0 { 256 } else { width };
            assert_eq!(width, height, "entry {i} is not square");
            assert_eq!(le16(ico, at + 6), 32, "entry {i} is not 32 bits per pixel");
            let length = le32(ico, at + 8) as usize;
            let offset = le32(ico, at + 12) as usize;
            let data = ico
                .get(offset..offset + length)
                .unwrap_or_else(|| panic!("entry {i} points outside the file"));
            Entry { size, data }
        })
        .collect()
}

/// Every pixel of an entry as RGBA, rows top first, whichever way it is stored.
fn decode(entry: &Entry<'_>) -> Vec<[u8; 4]> {
    let size = entry.size as usize;
    if entry.data.starts_with(b"\x89PNG") {
        let picture = image::load_from_memory_with_format(entry.data, image::ImageFormat::Png)
            .unwrap_or_else(|e| panic!("the {size} px PNG does not decode: {e}"))
            .to_rgba8();
        assert_eq!(
            (picture.width(), picture.height()),
            (entry.size, entry.size),
            "the PNG at {size} px is another size inside"
        );
        return picture.pixels().map(|p| p.0).collect();
    }
    // The classic form: a 40-byte header, the pixels bottom row first as BGRA,
    // and a 1-bit mask after them whose length is fixed by the size.
    let data = entry.data;
    assert_eq!(
        le32(data, 0),
        40,
        "the {size} px bitmap has an unusual header"
    );
    assert_eq!(
        le32(data, 4) as usize,
        size,
        "the {size} px bitmap is another width"
    );
    assert_eq!(
        le32(data, 8) as usize,
        size * 2,
        "an icon bitmap's height counts the mask as well as the picture"
    );
    assert_eq!(
        le16(data, 14),
        32,
        "the {size} px bitmap is not 32 bits per pixel"
    );
    assert_eq!(le32(data, 16), 0, "the {size} px bitmap is compressed");
    let mask = size.div_ceil(32) * 4 * size;
    assert_eq!(
        data.len(),
        40 + size * size * 4 + mask,
        "the {size} px bitmap is not the header, the pixels and the mask"
    );
    let mut pixels = vec![[0; 4]; size * size];
    for row in 0..size {
        let source = 40 + (size - 1 - row) * size * 4;
        for column in 0..size {
            let at = source + column * 4;
            pixels[row * size + column] = [data[at + 2], data[at + 1], data[at], data[at + 3]];
        }
    }
    pixels
}

#[test]
fn the_icon_file_holds_every_size_and_each_one_decodes() {
    let ico = read_bytes(&crate_dir().join("assets/edamame.ico"));
    let found = entries(&ico);
    assert_eq!(
        found.iter().map(|e| e.size).collect::<Vec<_>>(),
        SIZES,
        "the sizes Windows asks for at 100 to 200 % scaling, in the file manager and in the \
         title bar, are all in the file"
    );

    for entry in &found {
        let size = entry.size as usize;
        let pixels = decode(entry);
        let corners = [0, size - 1, size * (size - 1), size * size - 1];
        for corner in corners {
            assert_eq!(
                pixels[corner][3], 0,
                "the {size} px picture has a corner that is not clear: the icon is meant to be a \
                 free-standing shape"
            );
        }
        // Measured on the shipped file: 18 % of the pixels fully opaque at 16 px
        // up to 30 % at 256 px, and 55 % fully clear up to 68 %. The bounds are
        // wide on purpose - they refuse a blank picture and a filled square, not
        // a change of drawing.
        let opaque = pixels.iter().filter(|p| p[3] == 255).count() * 100 / (size * size);
        let clear = pixels.iter().filter(|p| p[3] == 0).count() * 100 / (size * size);
        assert!(
            (8..=60).contains(&opaque),
            "{opaque} % of the {size} px picture is opaque - a blank icon and a filled square \
             are both wrong"
        );
        assert!(
            clear >= 30,
            "only {clear} % of the {size} px picture is clear - the icon has lost its shape"
        );
    }
}

#[test]
fn the_icon_file_is_what_the_drawings_draw() {
    // Every entry of the file against the drawing it is made from, drawn again
    // here by the same renderer. Compared with the colour already multiplied by
    // the opacity, because a pixel that is nearly clear has no colour worth
    // comparing. The tolerance is a few levels in 255, which an update of the
    // renderer's anti-aliasing may use up. Another drawing uses up far more.
    let ico = read_bytes(&crate_dir().join("assets/edamame.ico"));
    let renderer = Renderer::new();
    let weighted = |pixel: &[u8; 4]| {
        let alpha = f64::from(pixel[3]);
        [
            f64::from(pixel[0]) * alpha / 255.0,
            f64::from(pixel[1]) * alpha / 255.0,
            f64::from(pixel[2]) * alpha / 255.0,
            alpha,
        ]
    };
    for entry in entries(&ico) {
        let size = entry.size;
        let drawn = renderer.draw(drawing_for(size), size);
        let stored = decode(&entry);
        assert_eq!(
            drawn.len(),
            stored.len(),
            "the {size} px entry is another size"
        );
        let total: f64 = drawn
            .iter()
            .zip(&stored)
            .map(|(a, b)| {
                weighted(a)
                    .iter()
                    .zip(weighted(b))
                    .map(|(x, y)| (x - y).abs())
                    .sum::<f64>()
            })
            .sum();
        let mean = total / (drawn.len() as f64 * 4.0);
        assert!(
            mean <= 2.0,
            "the {size} px entry is {mean:.2} levels away from {} on average: run \
             `cargo run -p nkb-gui --example make_icon` and commit the file",
            drawing_for(size)
        );
    }
}

/// The `.slint` lines of a file with the comments taken off.
fn slint_lines(body: &str) -> Vec<&str> {
    body.lines()
        .map(|line| line.split("//").next().unwrap_or(""))
        .collect()
}

#[test]
fn every_window_shows_the_icon_and_the_files_it_names_exist() {
    let ui = crate_dir().join("ui");

    // The dictionary names the drawing, and the drawing is there.
    let tokens = read_text(&ui.join("tokens.slint"));
    assert!(
        slint_lines(&tokens)
            .iter()
            .any(|line| line.contains("app-icon: @image-url(\"../assets/edamame.svg\");")),
        "ui/tokens.slint no longer names assets/edamame.svg as `app-icon`"
    );
    let drawing = read_text(&crate_dir().join("assets/edamame.svg"));
    assert!(
        drawing.contains("<svg") && drawing.contains("viewBox=\"0 0 256 256\""),
        "assets/edamame.svg is not the 256 by 256 drawing the .ico is made from"
    );

    // And every window of the product takes it from there. Counted per file, so
    // a window added without it turns this red instead of showing the toolkit's
    // blank icon in the taskbar.
    let mut screens: Vec<PathBuf> = std::fs::read_dir(ui.join("screens"))
        .expect("ui/screens must be readable")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|x| x == "slint"))
        .collect();
    screens.sort();
    assert!(!screens.is_empty(), "ui/screens holds no window to check");
    for path in screens {
        let body = read_text(&path);
        let lines = slint_lines(&body);
        let windows = lines
            .iter()
            .filter(|line| line.contains("inherits Window"))
            .count();
        let icons = lines
            .iter()
            .filter(|line| line.trim() == "icon: Tokens.app-icon;")
            .count();
        assert_eq!(
            icons,
            windows,
            "{} declares {windows} window(s) and gives the icon to {icons}",
            path.display()
        );
    }
}

#[test]
fn the_executable_embeds_the_icon_that_ships() {
    // The build script compiles the resource script, which names the .ico. Windows
    // is the one place that chain runs, so here each link is checked to point at
    // the next one and the last one is checked to exist.
    let build = read_text(&crate_dir().join("build.rs"));
    assert!(
        build.contains("\"assets/nkb-gui.rc\""),
        "build.rs does not compile assets/nkb-gui.rc"
    );
    let script = read_text(&crate_dir().join("assets/nkb-gui.rc"));
    let statements: Vec<&str> = script
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("//"))
        .collect();
    assert_eq!(
        statements,
        ["1 ICON \"edamame.ico\""],
        "the resource script adds the icon and nothing else"
    );
    assert!(
        crate_dir().join("assets/edamame.ico").is_file(),
        "assets/edamame.ico is what the resource script names, and it is not there"
    );
}

/// The values of every ` name="..."` attribute in a drawing, in file order.
fn attribute_values(drawing: &str, name: &str) -> Vec<String> {
    let marker = format!(" {name}=\"");
    drawing
        .match_indices(&marker)
        .filter_map(|(at, _)| {
            let rest = &drawing[at + marker.len()..];
            rest.find('"').map(|end| rest[..end].to_owned())
        })
        .collect()
}

#[test]
fn the_two_drawings_share_their_shapes() {
    // The small drawing is the master with heavier lines for a 16 px grid, and
    // that is only true while every shape and the transform that turns and fits
    // the pod are the same in both. Compared as text, because that is also what a
    // person editing one of them has in front of them.
    let master = read_text(&crate_dir().join("assets/edamame.svg"));
    let small = read_text(&crate_dir().join("assets/edamame-small.svg"));

    let group = |drawing: &str| {
        attribute_values(drawing, "transform")
            .into_iter()
            .find(|value| value.starts_with("translate("))
            .expect("a drawing turns and fits the pod in one group")
    };
    assert_eq!(
        group(&master),
        group(&small),
        "the pod is turned and fitted differently"
    );

    let mut master_paths = attribute_values(&master, "d");
    let mut small_paths = attribute_values(&small, "d");
    assert_eq!(master_paths.len(), 4, "stem twice, far wall, near wall");
    master_paths.sort();
    small_paths.sort();
    assert_eq!(
        master_paths, small_paths,
        "a pod is shaped differently in the two drawings"
    );

    // The beans are the ellipses that are not highlights, and a highlight is the
    // only ellipse that carries a transform of its own.
    let beans = |drawing: &str| {
        drawing
            .lines()
            .map(str::trim)
            .filter(|line| line.starts_with("<ellipse") && !line.contains("transform="))
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    assert_eq!(beans(&master).len(), 3, "three beans");
    assert_eq!(
        beans(&master),
        beans(&small),
        "the beans differ between the drawings"
    );
}
