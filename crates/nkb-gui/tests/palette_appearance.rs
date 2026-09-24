//! The palette, rendered in every state it has, and the resting rule MEASURED.
//!
//! # Why this is its own binary
//!
//! `slint::platform::set_platform` may be called once per process, so a second
//! rendering test in `appearance.rs` would draw into the gallery's window. One
//! file, one process, one platform.
//!
//! # What this proves that reading the screen cannot
//!
//! D53 forbids `text-muted` in the resting state: at the resting alpha it holds
//! only 4.55:1 over a white window lying under the palette, and would need 0.96.
//! `tools/kontrast.py` checks the arithmetic of the dictionary. It cannot check
//! that the palette actually keeps those roles out of the resting state, because
//! that is a fact about the PIXELS.
//!
//! So this measures the pixels - document 13 section 10 and GUI rule 10, look at
//! the effect and never at whether the property is written down. And it carries
//! its own positive control: the same palette, shown, MUST contain muted pixels.
//! Without that, hiding every band or breaking the render would turn the rule
//! green, which is the exact failure this project has already measured twice.

#![allow(clippy::panic, clippy::expect_used)]

mod offscreen;

use nkb_gui::{HintRow, Marker, Palette};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

const WIDTH: u32 = 420;
const HEIGHT: u32 = 460;

/// `text-muted`, the one role the resting state may not show. Written here
/// rather than read from the dictionary on purpose: two independent copies of a
/// value disagree loudly, whereas a test that reads the value it checks can
/// never fail. `typeface.rs` uses the same reasoning for the font family.
const TEXT_MUTED: (u8, u8, u8) = (0x7E, 0x87, 0x97);

/// `text-primary` from the dictionary, for the same reason as the role above.
const TEXT_PRIMARY: (u8, u8, u8) = (0xE6, 0xE9, 0xEF);

/// `accent` and `risk` from the dictionary, copied for the same reason: the
/// clipboard-mode bar must wear the first and never the second (D71).
const ACCENT: (u8, u8, u8) = (0x7A, 0xA2, 0xF7);
const RISK: (u8, u8, u8) = (0xE0, 0xAF, 0x68);

/// Nothing but the substitution marker, so its own ink can be measured apart
/// from the rest of the value.
const MARKER_ONLY: &str = "\u{2423}\u{2423}\u{2423}\u{2423}";

/// A value from the shipped `cjk-mixed` and `emoji-in-name`, which the shipped
/// typeface does not carry - and the note the product writes for it, as
/// `i18n::not_guaranteed` composes it. The test's own copy, for the reason the
/// colours above are copies.
const NOT_GUARANTEED_PREVIEW: &str = "\u{540D}\u{524D} \u{30C6}\u{30B9}\u{30C8} \u{1F600}";
const NOT_GUARANTEED_NOTE: &str =
    "not guaranteed by the bundled font: U+540D U+524D U+30C6 U+30B9 U+30C8 U+1F600";

/// The counters at the format's ceiling: a million code points, four bytes and
/// two UTF-16 units each - the longest line `i18n::counts` can compose from a
/// value the format accepts.
const LONGEST_COUNTS: &str = "1000000 graphemes, 1000000 code points, 4000000 bytes, \
                              2000000 UTF-16 units";

/// A reference at the identifier limits of `pack-format.md` 7: forty characters
/// of pack, forty-eight of value, and no space anywhere to break at.
const LONGEST_REFERENCE: &str = "abcdefghijklmnopqrstuvwxyzabcdefghijklmn/\
                                 abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuv";

/// Specimen data. The literals are the test's own - the test is not the product,
/// and what it feeds in is a sample to look at, the same job the gallery's
/// labels do. The product fills these from `nkb-adapters::i18n`.
fn fill(palette: &Palette) {
    palette.set_window_title("Naughty Keyboard".into());
    palette.set_pack("unicode-text".into());
    palette.set_counter("7 / 34".into());
    palette.set_value_name("Three zero-width spaces".into());
    palette.set_value_reference("unicode-text/zero-width-spaces".into());
    palette.set_value_counts("7 graphemes, 7 code points, 13 bytes, 7 UTF-16 units".into());
    // The preview and the shape, exactly as the sketch in `ux-spec.md` 2 draws
    // them. The marker is U+2423, whose glyph is IN the shipped typeface -
    // measured from the file's `cmap`, so the marker cannot itself render as the
    // empty rectangle it exists to prevent.
    palette.set_value_preview("ab\u{2423}\u{2423}\u{2423}cd".into());
    palette.set_value_shape("zero-width \u{D7} 3".into());
    palette.set_has_shape(true);
    // No elision for a seven-code-point value, and the render proves the band
    // does not draw an empty line for it.
    palette.set_has_elided(false);
    // Latin letters and the marker are all inside the typeface guarantee, so
    // the specimen of the sketch has no note - the note is measured on its own
    // below, with a value that earns one.
    palette.set_has_not_guaranteed(false);
    palette.set_clipboard_mode_label("clipboard mode".into());
    // There IS a value, so the value band is gated by `showing` alone - which is
    // what the resting rule below is about, and what mutation M51 flips.
    palette.set_has_value(true);

    palette.set_markers(ModelRc::new(VecModel::from(vec![
        Marker {
            text: "offensive".into(),
            risky: true,
        },
        Marker {
            text: "cleared first".into(),
            risky: false,
        },
        Marker {
            text: "pack warnings: 2".into(),
            risky: true,
        },
    ])));

    palette.set_messages(ModelRc::new(VecModel::from(vec![SharedString::from(
        "End of pack (34/34). Press again to start over.",
    )])));

    palette.set_hints(ModelRc::new(VecModel::from(vec![
        HintRow {
            key: "Ctrl+Alt+N".into(),
            action: "Next value".into(),
        },
        HintRow {
            key: "Ctrl+Alt+P".into(),
            action: "Previous value".into(),
        },
        HintRow {
            key: "Ctrl+Alt+B".into(),
            action: "Copy report block".into(),
        },
    ])));
}

fn render(
    window: &std::rc::Rc<slint::platform::software_renderer::MinimalSoftwareWindow>,
) -> Vec<offscreen::Pixel> {
    offscreen::draw(window, WIDTH, HEIGHT)
}

/// The last row the palette draws anything on. The buffer is taller than the
/// palette and keeps its initial value below it, so this is where the window
/// ends - `offscreen::save_cropped` finds the same edge for the picture.
fn bottom_edge(buffer: &[offscreen::Pixel]) -> usize {
    let empty = offscreen::Pixel::default();
    buffer
        .chunks(WIDTH as usize)
        .rposition(|row| row.iter().any(|pixel| *pixel != empty))
        .unwrap_or(0)
}

/// The last row the palette's SURFACE covers, read in the right margin, where
/// no text ever stands.
fn surface_edge(buffer: &[offscreen::Pixel]) -> usize {
    let empty = offscreen::Pixel::default();
    let column = (WIDTH - SURFACE_PROBE_INSET) as usize;
    buffer
        .chunks(WIDTH as usize)
        .rposition(|row| row[column] != empty)
        .unwrap_or(0)
}

/// How far in from the right edge the surface is read: inside the band padding,
/// so no text reaches it, and far enough from the edge that the rounded corner
/// takes only a row or two.
const SURFACE_PROBE_INSET: u32 = 6;

/// Rows the rounded corner may take off the surface at the probe column. The
/// corner is `radius-window`. At six pixels in from the edge its curve rises by
/// less than two rows, so three is a margin and not a hiding place.
const CORNER_ROWS: usize = 3;

/// Fails when anything is drawn below the palette's own surface.
///
/// 🔴 The failure this exists for, measured 2026-09-23 on Slint 1.18.1: a band
/// behind an `if` whose text wrapped reported one line per wrapped text, the
/// palette sized itself from that, and everything after the wrap was drawn
/// below the surface - on a live window, cut off. `bottom_edge` alone cannot see
/// it: spilled text still moves the bottom edge. The surface is what stops short.
fn assert_nothing_escapes_the_surface(buffer: &[offscreen::Pixel], state: &str, picture: &str) {
    let drawn = bottom_edge(buffer);
    let surface = surface_edge(buffer);
    assert!(
        drawn <= surface + CORNER_ROWS,
        "{state}: the palette draws down to row {drawn} but its surface ends at row \
         {surface}, so the bottom of the palette spills out of it. Look at {picture}"
    );
}

#[test]
fn the_palette_renders_every_state_and_keeps_muted_text_out_of_the_resting_one() {
    let window = offscreen::start(WIDTH, HEIGHT);
    let palette = Palette::new().expect("the palette must build");
    fill(&palette);
    palette.show().expect("the palette must show");

    // ---- resting: the state the rule is about ------------------------------
    // In clipboard mode as well, because that is the WORST resting case: the
    // standing bar is the only extra thing the dimmed palette ever shows, so if
    // any role in it were muted this is where it would appear.
    palette.set_showing(false);
    palette.set_clipboard_mode(true);
    let resting = render(&window);
    let resting_again = render(&window);
    assert!(
        resting == resting_again,
        "two renders of an unchanged palette differ, so comparing appearance between \
         runs is impossible and the loop this test supports does not work"
    );
    let resting_path = offscreen::save_cropped(&resting, WIDTH, HEIGHT, "palette-resting.png");

    // ---- showing: the positive control, and the rest of the window ---------
    palette.set_showing(true);
    palette.set_clipboard_mode(false);
    let showing = render(&window);
    let showing_path = offscreen::save_cropped(&showing, WIDTH, HEIGHT, "palette-showing.png");

    // ---- the preview really draws, measured rather than assumed ------------
    // 🔴 Zasada GUI 10: a property written into a view can be silently ignored by
    // the toolkit and looks identical to a working one. So the check is a
    // DIFFERENCE between two renders: blanking the preview and the shape must
    // take ink away. Measuring the presence of `text-primary` alone would not
    // work - the value's name is that colour too, so the count would stay
    // non-zero with the preview gone.
    let with_preview = offscreen::count_exactly(&showing, TEXT_PRIMARY);
    palette.set_value_preview("".into());
    palette.set_value_shape("".into());
    palette.set_has_shape(false);
    let without_preview = offscreen::count_exactly(&render(&window), TEXT_PRIMARY);
    assert!(
        with_preview > without_preview,
        "blanking the preview and the shape changed nothing on screen ({with_preview} vs \
         {without_preview} pixels of text-primary), so the value band is not drawing them"
    );
    // The marker is the point: U+2423 must leave ink of its own, or the preview
    // shows a value with a hole where the invisible character was.
    palette.set_value_preview(MARKER_ONLY.into());
    let marker_only = offscreen::count_exactly(&render(&window), TEXT_PRIMARY);
    assert!(
        marker_only > without_preview,
        "the substitution marker left no ink, so it has no glyph in the shipped \
         typeface and the preview would show nothing where an invisible character sits"
    );
    // ---- the typeface note draws, measured rather than assumed ------------
    // `D66`: a value with characters the shipped typeface does not carry gets a
    // note under the preview. It is a muted line, so switching it on must ADD
    // muted ink - the same difference-of-two-renders shape as above, because a
    // line that the view silently drops looks exactly like a line never asked for.
    palette.set_value_preview(NOT_GUARANTEED_PREVIEW.into());
    palette.set_value_not_guaranteed(NOT_GUARANTEED_NOTE.into());
    palette.set_has_not_guaranteed(false);
    let muted_without_note = offscreen::count_exactly(&render(&window), TEXT_MUTED);
    palette.set_has_not_guaranteed(true);
    let with_note = render(&window);
    let muted_with_note = offscreen::count_exactly(&with_note, TEXT_MUTED);
    let note_path =
        offscreen::save_cropped(&with_note, WIDTH, HEIGHT, "palette-not-guaranteed.png");
    // The note wraps onto a second line, which is exactly the case that used to
    // push the hint bar out of the palette.
    assert_nothing_escapes_the_surface(
        &with_note,
        "with the typeface note",
        &note_path.display().to_string(),
    );
    assert!(
        muted_with_note > muted_without_note,
        "switching the typeface note on added no muted ink ({muted_without_note} vs \
         {muted_with_note}), so the palette does not draw it. Look at {}",
        note_path.display()
    );

    // ---- long lines wrap rather than run off the window -------------------
    // Measured 2026-09-23 on the shipped `len-100000`: the counters ran past the
    // right edge and were cut mid-word. The specimen above uses small numbers and
    // never tried. A line that wraps pushes the palette's bottom edge down, and a
    // line that is cut leaves it where it was - so the edge is the measurement.
    fill(&palette);
    palette.set_showing(true);
    let edge_short = bottom_edge(&render(&window));
    palette.set_value_counts(LONGEST_COUNTS.into());
    let edge_counts = bottom_edge(&render(&window));
    palette.set_value_reference(LONGEST_REFERENCE.into());
    let long_lines = render(&window);
    let edge_reference = bottom_edge(&long_lines);
    let long_path = offscreen::save_cropped(&long_lines, WIDTH, HEIGHT, "palette-long-lines.png");
    assert_nothing_escapes_the_surface(
        &long_lines,
        "with the longest counters and reference",
        &long_path.display().to_string(),
    );
    assert!(
        edge_counts > edge_short,
        "the longest counters did not move the palette's bottom edge ({edge_short} -> \
         {edge_counts}), so they are cut off rather than wrapped. Look at {}",
        long_path.display()
    );
    assert!(
        edge_reference > edge_counts,
        "the longest reference did not move the palette's bottom edge ({edge_counts} -> \
         {edge_reference}), so it is cut off rather than wrapped. Look at {}",
        long_path.display()
    );

    // Put the specimen back, so the renders saved below are the ones the gallery
    // and the calibration tool compare against.
    fill(&palette);
    palette.set_showing(true);

    // ---- clipboard mode: the standing bar wears the accent, never risk -----
    // D71. Measured as a DIFFERENCE of two renders, like the preview above:
    // switching the mode on must ADD accent ink and must add NO risk ink. A bar
    // in the risk colour - the colour `offensive` wears - would tell the tester
    // without a word that the mode is a failure, which `ux-spec.md` 8 forbids.
    // In this mode the field is not cleared, so the value carries the marker
    // that it went to the clipboard instead of `cleared first`.
    palette.set_markers(ModelRc::new(VecModel::from(vec![
        Marker {
            text: "offensive".into(),
            risky: true,
        },
        Marker {
            text: "on the clipboard".into(),
            risky: false,
        },
    ])));
    let direct = render(&window);
    palette.set_clipboard_mode(true);
    let clipboard = render(&window);
    let clipboard_path =
        offscreen::save_cropped(&clipboard, WIDTH, HEIGHT, "palette-clipboard-mode.png");
    assert_nothing_escapes_the_surface(
        &clipboard,
        "in clipboard mode",
        &clipboard_path.display().to_string(),
    );
    let accent_gained = offscreen::count_exactly(&clipboard, ACCENT)
        .saturating_sub(offscreen::count_exactly(&direct, ACCENT));
    assert!(
        accent_gained > 0,
        "switching clipboard mode on added no accent ink, so the standing bar is not drawn \
         in the accent. Look at {}",
        clipboard_path.display()
    );
    assert_eq!(
        offscreen::count_exactly(&clipboard, RISK),
        offscreen::count_exactly(&direct, RISK),
        "switching clipboard mode on changed the amount of risk ink, so the standing bar \
         wears the colour of `offensive` - `ux-spec.md` 8 says the mode is not a failure. \
         Look at {}",
        clipboard_path.display()
    );

    // ---- anti-vacuity ------------------------------------------------------
    // An empty buffer compares equal to itself and holds no muted pixel either,
    // so without this every assertion below would pass on a palette that drew
    // nothing at all.
    let background = resting[0];
    let ink = resting.iter().filter(|p| **p != background).count();
    assert!(
        ink > (WIDTH * HEIGHT) as usize / 100,
        "only {ink} of {} pixels differ from the corner colour - the resting palette drew \
         (almost) nothing",
        WIDTH * HEIGHT
    );

    // ---- the positive control for the rule itself --------------------------
    let muted_when_showing = offscreen::count_exactly(&showing, TEXT_MUTED);
    assert!(
        muted_when_showing > 0,
        "the shown palette holds no pixel of `text-muted` at all, so the resting check \
         below cannot distinguish a palette that obeys D53 from one that simply has no \
         muted role anywhere - see this file's header"
    );

    // ---- the rule ----------------------------------------------------------
    let muted_when_resting = offscreen::count_exactly(&resting, TEXT_MUTED);
    assert_eq!(
        muted_when_resting,
        0,
        "the resting palette shows {muted_when_resting} pixels of `text-muted`. At the \
         resting alpha that role holds 4.55:1 over a white window under the palette and \
         needs 0.96 - D53 keeps it out of this state entirely. Look at {}",
        resting_path.display()
    );

    // ---- the states have to differ from each other -------------------------
    // Three properties that change nothing would render three identical
    // pictures, and every assertion above would still hold.
    assert!(
        resting != showing,
        "resting and showing rendered identically, so `showing` gates nothing"
    );
    assert!(
        direct != clipboard,
        "the standing bar changed no pixel, so `clipboard-mode` gates nothing"
    );

    println!(
        "palette rendered to {}, {}, {}",
        resting_path.display(),
        showing_path.display(),
        clipboard_path.display()
    );
}
