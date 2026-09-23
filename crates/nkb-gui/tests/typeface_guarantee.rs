//! The typeface guarantee (`D52`, `D66`), judged three ways.
//!
//! # The claim being guarded
//!
//! The palette names, under every preview, the characters the shipped typeface
//! does not draw by itself. That sentence is only as true as the table behind
//! it: a table that promises too much hides exactly the characters a tester
//! needs warning about, and one that promises too little cries wolf over Latin
//! letters. Neither failure is visible in use - the note simply says something
//! plausible.
//!
//! # Why three checks and not one
//!
//! Each answers a question the others cannot:
//!
//! 1. **the table says what the renderer reads.** Every one of the 1 114 112
//!    code points is looked up in the font through `skrifa` - the very library
//!    the toolkit's text layout asks when it chooses a typeface (`parley`,
//!    measured in its source 2026-09-23) - and compared with the table. Not "the
//!    table equals our parser", which would be a circle: there is no parser of
//!    ours, and the table was first produced by a different implementation
//!    (fontTools) before this check agreed with it;
//! 2. **every promised character leaves ink.** A font can map a character to a
//!    glyph with no outline, and the character map would still say "covered".
//!    Only a render tells, so every promised character is drawn without a screen
//!    and its cell measured - GUI rule 10, the effect rather than the claim;
//! 3. **the shipped packs reach outside the guarantee exactly here.** The list
//!    is written out below, so a pack bringing a new script turns this red until
//!    somebody acknowledges it - the lesson of `OBS-85`, that added data must be
//!    visible. `D52` placed this in `shipped_packs.rs`; that test lives in the
//!    CLI crate, which may not depend on the interface (`D25`), so it is here.
//!
//! # What none of them can check
//!
//! That a character OUTSIDE the table fails to render on some machine. The
//! toolkit falls back to the machine's own fonts, and this machine has CJK and
//! emoji faces, so a render here would ink them. The guarantee is about what the
//! product carries, and that is what is checked.

// A failed expectation in a test is a failed test.
#![allow(clippy::panic, clippy::expect_used)]

// Shared with the palette's render test, which uses the two helpers this file
// does not: cropping and counting one exact colour. A module compiled into each
// test binary is judged per binary, so without this the unused half warns here.
#[allow(dead_code)]
mod offscreen;

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use nkb_adapters::{BuiltInCatalogue, TomlPackFormat};
use nkb_app::ports::{PackCatalogue, PackFormat, PackSource};
use nkb_core::preview::{MARKER, is_invisible};
use nkb_core::typeface::outside_guarantee;
use nkb_gui::typeface::SHIPPED;
use skrifa::MetadataProvider;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

/// The last code point in the standard.
const MAX_CODE_POINT: u32 = 0x10_FFFF;

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn font_bytes() -> Vec<u8> {
    let path = manifest_dir()
        .join("ui")
        .join("fonts")
        .join("DejaVuSansMono.ttf");
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Every code point the font maps to a real glyph, asked the way the renderer
/// asks: `charmap().map(c)`, where glyph 0 means "missing".
fn mapped_by_the_font(font: &[u8]) -> Vec<bool> {
    let font = skrifa::FontRef::new(font).expect("the shipped file must parse as a font");
    let charmap = font.charmap();
    (0..=MAX_CODE_POINT)
        .map(|code| {
            charmap
                .map(code)
                .is_some_and(|glyph| glyph != skrifa::GlyphId::NOTDEF)
        })
        .collect()
}

/// The header the generated table carries. Kept here because THIS test is the
/// generator from now on: when the table and the font disagree, it writes the
/// table the font implies, byte for byte in the committed shape.
const TABLE_HEADER: &str = "\
//! Generated from `ui/fonts/DejaVuSansMono.ttf`. Do not edit by hand.
//!
//! Every code point the shipped typeface maps to a glyph, as inclusive ranges,
//! sorted and merged. Read from the typeface's own character map - the format
//! 12 subtable, which is the only one of its five that reaches above the basic
//! plane - and not from a render or from anyone's opinion.
//!
//! Produced first with fontTools and checked against `skrifa`, the library the
//! toolkit's own text layout asks, so the table started life with two
//! independent readings agreeing. What keeps it honest from then on is
//! `tests/typeface_guarantee.rs`: it compares every one of the 1 114 112 code
//! points, and when they disagree it writes the table it expected next to its
//! other output, ready to replace this file.

";

fn table_source(mapped: &[bool]) -> String {
    let mut ranges: Vec<(u32, u32)> = Vec::new();
    for (code, &is_mapped) in (0_u32..).zip(mapped) {
        if !is_mapped {
            continue;
        }
        match ranges.last_mut() {
            Some((_, end)) if *end + 1 == code => *end = code,
            _ => ranges.push((code, code)),
        }
    }
    let mut out = String::from(TABLE_HEADER);
    let _ = writeln!(
        out,
        "pub(super) static RANGES: [(u32, u32); {}] = [",
        ranges.len()
    );
    for (start, end) in ranges {
        let _ = writeln!(out, "    (0x{start:04X}, 0x{end:04X}),");
    }
    out.push_str("];\n");
    out
}

#[test]
fn the_table_says_exactly_what_the_renderers_own_reading_says() {
    let mapped = mapped_by_the_font(&font_bytes());

    // Anti-vacuity: a font that failed to parse maps nothing, and an empty table
    // would then agree with it perfectly.
    let total = mapped.iter().filter(|m| **m).count();
    assert!(
        total > 3000,
        "the font maps only {total} code points - it did not parse, or it is not the \
         shipped DejaVu Sans Mono, and every comparison below would pass for the wrong reason"
    );

    let mut wrong: Vec<(u32, bool, bool)> = Vec::new();
    for (code, &font_says) in (0_u32..).zip(&mapped) {
        // Surrogates are not characters and cannot reach the lookup.
        let Some(c) = char::from_u32(code) else {
            continue;
        };
        let table_says = SHIPPED.covers(c);
        if table_says != font_says {
            wrong.push((code, font_says, table_says));
        }
    }

    if !wrong.is_empty() {
        let out_dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("typeface");
        std::fs::create_dir_all(&out_dir).expect("the output folder must be creatable");
        let regenerated = out_dir.join("table.rs");
        std::fs::write(&regenerated, table_source(&mapped)).expect("the table must be writable");
        let shown: Vec<String> = wrong
            .iter()
            .take(20)
            .map(|(code, font, table)| {
                format!("  U+{code:04X}: font says {font}, table says {table}")
            })
            .collect();
        panic!(
            "{} code points disagree between the shipped font and the guarantee table \
             (first twenty shown):\n{}\n\nThe table the font implies was written to {} - \
             if the FONT changed on purpose, that file replaces src/typeface/table.rs.",
            wrong.len(),
            shown.join("\n"),
            regenerated.display()
        );
    }
}

#[test]
fn the_substitution_marker_is_inside_the_guarantee() {
    // `D64` chose U+2423 after reading the character map by hand. From here on
    // that reading is a test: a marker outside the guarantee would draw as the
    // empty rectangle it exists to replace, on exactly the machines the
    // guarantee was written for.
    assert!(SHIPPED.covers(MARKER));
}

#[test]
fn the_oracle_is_the_same_reader_the_renderer_uses() {
    // The whole value of check 1 is that `skrifa` is the RENDERER'S reading. Two
    // versions in the lock file would mean the test asks one and the toolkit the
    // other - a second opinion wearing the renderer's name.
    let lock_path = manifest_dir().join("..").join("..").join("Cargo.lock");
    let lock = std::fs::read_to_string(&lock_path)
        .unwrap_or_else(|e| panic!("{}: {e}", lock_path.display()));
    let versions = lock.matches("\nname = \"skrifa\"\n").count();
    assert_eq!(
        versions, 1,
        "Cargo.lock holds {versions} versions of skrifa. The dev-dependency in \
         crates/nkb-gui/Cargo.toml has to move together with the one Slint locks."
    );
}

// ---------------------------------------------------------------------------
// Check 2: every promised character leaves ink
// ---------------------------------------------------------------------------

// A grid of cells, one character each, drawn with the preview's own role so the
// family is the one the product names. White on black and no dictionary colours:
// this is a measuring instrument, not a view, and maximum contrast is what makes
// "no ink at all" an unambiguous reading.
slint::slint! {
    import { TextPreview } from "ui/components.slint";

    export component InkGrid inherits Window {
        in property <[string]> cells;
        in property <int> columns;
        in property <length> cell;
        background: black;
        for text[i] in cells: TextPreview {
            x: mod(i, columns) * cell + cell / 2;
            y: floor(i / columns) * cell + cell / 4;
            text: text;
            color: white;
        }
    }
}

/// Characters in the table whose glyph legitimately draws nothing ON ITS OWN.
///
/// Measured 2026-09-23 by rendering all 3322 promised characters: 100 cells came
/// out empty, and every one of them falls into one of three kinds.
fn draws_nothing_alone(c: char) -> bool {
    // Spaces and format characters. The preview never draws these - it draws
    // the marker in their place - so their blank glyphs are never seen.
    is_invisible(c)
        // A character that joins the one before it: combining marks. Alone they
        // have nothing to sit on, and measured, the layout draws most of them
        // only on a base of their own script - Latin marks on `o` leave ink,
        // Arabic and Lao ones do not. Joining is asked of the core's own
        // segmentation rather than listed here.
        || nkb_core::graphemes::count(&format!("o{c}")) == 1
        // Interlinear annotation controls and the object replacement character.
        // Format characters the preview's `is_invisible` does not list - a gap
        // recorded in `OBS-121` - and the layout draws nothing for them.
        || matches!(c, '\u{FFF9}'..='\u{FFFC}')
}

#[test]
fn every_character_the_table_promises_leaves_ink() {
    let promised: Vec<char> = (0..=MAX_CODE_POINT)
        .filter_map(char::from_u32)
        .filter(|c| SHIPPED.covers(*c))
        .collect();
    // Two cells of controls in front: a space that MUST come out empty and a
    // letter that MUST leave ink. Without them, a detector that never sees ink -
    // or always sees it - would pass or fail this test for the wrong reason.
    let mut cells: Vec<char> = vec![' ', 'A'];
    cells.extend(&promised);

    let columns: u32 = 64;
    let cell: u32 = 40;
    let count = u32::try_from(cells.len()).expect("a few thousand cells");
    let rows = count.div_ceil(columns);
    let (width, height) = (columns * cell, rows * cell);

    let window = offscreen::start(width, height);
    let grid = InkGrid::new().expect("the grid must build");
    grid.set_cells(ModelRc::new(VecModel::from(
        cells
            .iter()
            .map(|c| SharedString::from(c.to_string()))
            .collect::<Vec<_>>(),
    )));
    grid.set_columns(i32::try_from(columns).expect("small"));
    grid.set_cell(cell as f32);
    grid.show().expect("the grid must show");
    let buffer = offscreen::draw(&window, width, height);

    let inked = |index: u32| -> bool {
        let (left, top) = ((index % columns) * cell, (index / columns) * cell);
        (top..top + cell).any(|y| {
            (left..left + cell).any(|x| {
                let pixel = buffer[(y * width + x) as usize];
                pixel.r > 40 || pixel.g > 40 || pixel.b > 40
            })
        })
    };

    assert!(
        !inked(0),
        "the space came out inked, so the detector sees ink everywhere"
    );
    assert!(
        inked(1),
        "the letter A left no ink, so the detector sees nothing at all"
    );

    let mut blank_and_unexplained: Vec<String> = Vec::new();
    let mut blank_and_explained = 0;
    for (index, c) in (2_u32..).zip(&promised) {
        if inked(index) {
            continue;
        }
        if draws_nothing_alone(*c) {
            blank_and_explained += 1;
        } else {
            blank_and_unexplained.push(format!("U+{:04X}", u32::from(*c)));
        }
    }

    // The exemptions must be doing SOMETHING, or the rule above is dead text and
    // a change to it would never be noticed. Measured: 100.
    assert!(
        blank_and_explained > 50,
        "only {blank_and_explained} promised characters came out blank for a known reason - \
         the render or the exemptions changed shape"
    );
    let saved = offscreen::save(&buffer, width, height, "typeface-guarantee.png");
    assert!(
        blank_and_unexplained.is_empty(),
        "{} characters are promised by the table and draw nothing: {}. Look at {}",
        blank_and_unexplained.len(),
        blank_and_unexplained.join(" "),
        saved.display()
    );
}

// ---------------------------------------------------------------------------
// Check 3: the shipped packs, written out
// ---------------------------------------------------------------------------

/// Every character a shipped value carries that the typeface does not draw by
/// itself, with the first value (in catalogue order) that brings it.
///
/// Counted over the WHOLE value, not over the preview. The palette asks about
/// what it draws; this asks about what the catalogue carries, so a new script in
/// the middle of a long value still has to be acknowledged here.
///
/// ⚠️ Eighteen, not the twenty-nine `D52` gives. That number is the whole
/// catalogue of `catalog-v0.1.md`; three of its twenty packs are shipped today.
const SHIPPED_OUTSIDE: [(char, &str); 18] = [
    ('\u{30B9}', "unicode-text/cjk-mixed"),
    ('\u{30C6}', "unicode-text/cjk-mixed"),
    ('\u{30C8}', "unicode-text/cjk-mixed"),
    ('\u{524D}', "unicode-text/cjk-mixed"),
    ('\u{540D}', "unicode-text/cjk-mixed"),
    ('\u{FF44}', "unicode-text/fullwidth-latin"),
    ('\u{FF46}', "unicode-text/fullwidth-latin"),
    ('\u{FF48}', "unicode-text/fullwidth-latin"),
    ('\u{FF49}', "unicode-text/fullwidth-latin"),
    ('\u{FF4C}', "unicode-text/fullwidth-latin"),
    ('\u{FF54}', "unicode-text/fullwidth-latin"),
    ('\u{FF55}', "unicode-text/fullwidth-latin"),
    ('\u{FF57}', "unicode-text/fullwidth-latin"),
    ('\u{1D407}', "unicode-text/math-bold"),
    ('\u{1D41E}', "unicode-text/math-bold"),
    ('\u{1D425}', "unicode-text/math-bold"),
    ('\u{1D428}', "unicode-text/math-bold"),
    // Also in `length-bombs/emoji-truncation`; named by the value the catalogue
    // lists first.
    ('\u{1F600}', "unicode-text/emoji-in-name"),
];

#[test]
fn the_shipped_packs_reach_outside_the_guarantee_exactly_here() {
    let catalogue = BuiltInCatalogue::new();
    let ids = catalogue
        .list()
        .expect("the built-in catalogue lists its packs");
    assert!(
        !ids.is_empty(),
        "the catalogue is empty, so nothing below means anything"
    );

    let mut found: BTreeMap<char, String> = BTreeMap::new();
    let mut values = 0;
    for id in &ids {
        let text = catalogue.read(id).expect("a listed pack is readable");
        let pack = TomlPackFormat
            .parse(&text)
            .unwrap_or_else(|| panic!("the shipped pack {id} does not parse"));
        for value in &pack.values {
            let literal = value
                .body
                .materialise()
                .unwrap_or_else(|e| panic!("{id}/{}: {e:?}", value.id));
            values += 1;
            for c in outside_guarantee(&literal, &SHIPPED) {
                found
                    .entry(c)
                    .or_insert_with(|| format!("{}/{}", pack.id, value.id));
            }
        }
    }
    assert!(
        values > 30,
        "only {values} values were read from the shipped packs"
    );

    let expected: BTreeMap<char, String> = SHIPPED_OUTSIDE
        .iter()
        .map(|(c, reference)| (*c, (*reference).to_owned()))
        .collect();
    let describe = |set: &BTreeMap<char, String>| -> String {
        set.iter()
            .map(|(c, reference)| format!("  U+{:04X} from {reference}", u32::from(*c)))
            .collect::<Vec<_>>()
            .join("\n")
    };
    assert!(
        found == expected,
        "the shipped packs reach outside the typeface guarantee differently than this \
         test says. If a pack now carries a new script ON PURPOSE, write it into \
         SHIPPED_OUTSIDE - that is the acknowledgement. Found:\n{}\nExpected:\n{}",
        describe(&found),
        describe(&expected)
    );
}
