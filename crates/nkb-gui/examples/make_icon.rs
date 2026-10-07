//! Rebuilds `assets/edamame.ico` from the two drawings beside it.
//!
//! ```text
//! cargo run -p nkb-gui --example make_icon
//! ```
//!
//! Run it when either SVG file changes, and commit the `.ico` it writes. The
//! drawings are drawn by the toolkit's own renderer (`tests/icon_render`), the
//! same one that draws the window icon, so no browser and no image tool is
//! needed. `tests/app_icon.rs` fails when the file and the drawings disagree.
//!
//! The file holds nine sizes. 16 to 32 px come from `edamame-small.svg` and 40 px
//! up from `edamame.svg`, which `assets/README.md` explains. 16 to 64 px are
//! stored as the classic 32-bit bitmap that every Windows version reads. 128 and
//! 256 px are stored as PNG, which keeps the file under 100 kB.

// A tool prints what it wrote, and a failure here is a failure of the tool.
#![allow(clippy::print_stdout, clippy::expect_used)]

#[allow(dead_code)]
#[path = "../tests/icon_render/mod.rs"]
mod icon_render;

use std::io::Cursor;

use icon_render::{Renderer, SIZES, assets_dir, drawing_for};

/// From this size up an entry is a PNG, and below it a bitmap.
const PNG_FROM: u32 = 128;

/// A 32-bit bitmap in the form an icon file stores it: the header, the pixels
/// bottom row first as blue, green, red and opacity, then a 1-bit mask that is
/// set where the picture is clear.
fn bitmap_entry(size: u32, pixels: &[[u8; 4]]) -> Vec<u8> {
    let side = size as usize;
    let mut data = Vec::new();
    data.extend(40_u32.to_le_bytes());
    data.extend(size.to_le_bytes());
    // The height of an icon bitmap counts the mask as well as the picture.
    data.extend((size * 2).to_le_bytes());
    data.extend(1_u16.to_le_bytes());
    data.extend(32_u16.to_le_bytes());
    // No compression, no size, no resolution, no palette.
    data.extend([0_u8; 24]);
    for row in (0..side).rev() {
        for column in 0..side {
            let [red, green, blue, alpha] = pixels[row * side + column];
            data.extend([blue, green, red, alpha]);
        }
    }
    let stride = side.div_ceil(32) * 4;
    for row in (0..side).rev() {
        let mut mask = vec![0_u8; stride];
        for column in 0..side {
            if pixels[row * side + column][3] == 0 {
                mask[column / 8] |= 0x80 >> (column % 8);
            }
        }
        data.extend(mask);
    }
    data
}

fn png_entry(size: u32, pixels: &[[u8; 4]]) -> Vec<u8> {
    let bytes: Vec<u8> = pixels.iter().flatten().copied().collect();
    let picture = image::RgbaImage::from_raw(size, size, bytes)
        .expect("the pixels must fill the picture exactly");
    let mut out = Cursor::new(Vec::new());
    picture
        .write_to(&mut out, image::ImageFormat::Png)
        .expect("a picture in memory must encode");
    out.into_inner()
}

fn main() {
    let renderer = Renderer::new();
    let entries: Vec<(u32, Vec<u8>)> = SIZES
        .iter()
        .map(|&size| {
            let pixels = renderer.draw(drawing_for(size), size);
            let data = if size >= PNG_FROM {
                png_entry(size, &pixels)
            } else {
                bitmap_entry(size, &pixels)
            };
            (size, data)
        })
        .collect();

    let mut file = Vec::new();
    file.extend([0_u8, 0, 1, 0]);
    file.extend(
        u16::try_from(entries.len())
            .expect("a few sizes")
            .to_le_bytes(),
    );
    let mut offset = 6 + 16 * entries.len();
    for (size, data) in &entries {
        // A width or height of 256 is written as 0 in an icon directory.
        let byte = u8::try_from(size % 256).expect("a remainder below 256");
        file.extend([byte, byte, 0, 0]);
        file.extend(1_u16.to_le_bytes());
        file.extend(32_u16.to_le_bytes());
        file.extend(
            u32::try_from(data.len())
                .expect("a small picture")
                .to_le_bytes(),
        );
        file.extend(u32::try_from(offset).expect("a small file").to_le_bytes());
        offset += data.len();
    }
    for (_, data) in &entries {
        file.extend(data);
    }

    let path = assets_dir().join("edamame.ico");
    std::fs::write(&path, &file).expect("the icon must be writable");
    println!("{}: {} bytes", path.display(), file.len());
}
