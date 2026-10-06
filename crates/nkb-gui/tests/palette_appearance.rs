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
/// Taller than any state below draws. 460 until the next band (UX-GUI-001): the
/// palette with both value bands and the longest counters outgrew it, every edge
/// measured the buffer's floor instead of the palette's, and "the counters wrap"
/// failed on a palette that wrapped them fine. A buffer that clips is a check
/// that cannot see.
const HEIGHT: u32 = 720;

/// `text-muted`, the one role the resting state may not show. Written here
/// rather than read from the dictionary on purpose: two independent copies of a
/// value disagree loudly, whereas a test that reads the value it checks can
/// never fail. `typeface.rs` uses the same reasoning for the font family.
const TEXT_MUTED: (u8, u8, u8) = (0x7E, 0x87, 0x97);

/// `text-primary` from the dictionary, for the same reason as the role above.
const TEXT_PRIMARY: (u8, u8, u8) = (0xE6, 0xE9, 0xEF);

/// `text-secondary`, the role of the value band's empty state.
const TEXT_SECONDARY: (u8, u8, u8) = (0x9A, 0xA4, 0xB4);

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
///
/// Composed by the product rather than written out: until 2026-09-24 this was a
/// literal, and the pattern changed under it (`D80`) - a copy of the text would
/// have kept testing a line the palette no longer shows.
fn longest_counts() -> String {
    nkb_adapters::i18n::counts(1_000_000, 1_000_000, 4_000_000, 2_000_000)
}

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
    palette.set_value_counts("graphemes: 7, code points: 7, bytes: 13, UTF-16 units: 7".into());
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
    // There IS a value, so the value band is gated by `compact` alone - which is
    // what the compact rule below is about, and what mutation M52 flips.
    palette.set_has_value(true);
    palette.set_last_sent_label("Last sent".into());
    palette.set_copy_label("Copy".into());
    // The next value (UX-GUI-001): a different value from the one above, so the
    // picture shows both bands telling two values apart.
    palette.set_has_next(true);
    palette.set_next_has_value(true);
    palette.set_next_heading("Next: value 8 of 34".into());
    palette.set_next_name("Trailing no-break space".into());
    palette.set_next_preview("Kowalski\u{2423}".into());
    palette.set_next_markers(ModelRc::new(VecModel::from(vec![Marker {
        text: "offensive".into(),
        risky: true,
    }])));

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
            key: "Alt+Shift+N".into(),
            action: "Next value".into(),
        },
        HintRow {
            key: "Alt+Shift+P".into(),
            action: "Previous value".into(),
        },
        HintRow {
            key: "Alt+Shift+B".into(),
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
    palette.set_compact(true);
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
    palette.set_compact(false);
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
    palette.set_compact(false);
    let edge_short = bottom_edge(&render(&window));
    palette.set_value_counts(longest_counts().into());
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

    // ---- the next value: drawn, one line per field, gone while a send runs --
    // UX-GUI-001. Before the floor of the section below exists, because a held
    // height would hide the first measurement. Each a difference of two renders
    // or of two edges, never the presence of a colour (GUI rule 10).
    fill(&palette);
    palette.set_compact(false);
    palette.set_has_next(false);
    let edge_without_next = bottom_edge(&render(&window));
    palette.set_has_next(true);
    let with_next = render(&window);
    let edge_with_next = bottom_edge(&with_next);
    let next_path = offscreen::save_cropped(&with_next, WIDTH, HEIGHT, "palette-next.png");
    assert_nothing_escapes_the_surface(
        &with_next,
        "with the next value",
        &next_path.display().to_string(),
    );
    assert!(
        edge_with_next > edge_without_next,
        "switching the next band on added no height ({edge_without_next} -> {edge_with_next}), \
         so the palette does not draw it. Look at {}",
        next_path.display()
    );
    // ELIDED, not wrapped: the band changes with every press, and a band that
    // grew with the value would move everything below it (D83).
    palette.set_next_preview("\u{2423}x".repeat(150).into());
    palette.set_next_name(
        "A name long enough to need two lines in a palette this wide, \
                           and then some more words after that"
            .into(),
    );
    let long_next = render(&window);
    let long_next_path =
        offscreen::save_cropped(&long_next, WIDTH, HEIGHT, "palette-next-long.png");
    assert_eq!(
        bottom_edge(&long_next),
        edge_with_next,
        "a long next value moved the palette's bottom edge, so the band wraps rather than \
         elides and the window grows and shrinks with every press. Look at {}",
        long_next_path.display()
    );
    // While a send runs the band is not drawn: which value comes next depends on
    // how the one in flight ends.
    palette.set_sending_label("Typing".into());
    palette.set_sending_counter("1784 / 65535".into());
    palette.set_sending_hint("Press Esc to stop.".into());
    palette.set_sending(true);
    let sending_with_next = render(&window);
    palette.set_has_next(false);
    assert!(
        render(&window) == sending_with_next,
        "the next band changed the picture while a send was running, so it promises a value \
         the press in flight has not settled yet"
    );
    palette.set_sending(false);
    fill(&palette);

    // ---- the expanded palette never gets shorter by itself (D83) ----------
    // Through the product's own wiring: the `changed` handler in the view and
    // `hold_height` in the worker module. Measured on the SURFACE, because the
    // surface is what the window's height draws - a floor that only moved a
    // property would leave the bottom edge where the content ends.
    nkb_gui::live::hold_height_on_change(&palette);
    fill(&palette);
    palette.set_compact(false);
    palette.set_value_preview("\u{2423}".repeat(100).into());
    render(&window);
    let tall = render(&window);
    let edge_tall = surface_edge(&tall);
    palette.set_value_preview("a".into());
    render(&window);
    let after = render(&window);
    let after_path = offscreen::save_cropped(&after, WIDTH, HEIGHT, "palette-height-held.png");
    assert_eq!(
        surface_edge(&after),
        edge_tall,
        "a one-letter value after a hundred-character one moved the palette's bottom edge \
         ({edge_tall} -> {}), so the window shrinks under the tester's eyes. Look at {}",
        surface_edge(&after),
        after_path.display()
    );
    // The positive control: collapsing and expanding starts the floor over, so
    // the same short value now gets a SHORTER palette. Without this, a floor
    // stuck at the buffer's height would pass the check above.
    nkb_gui::live::set_compact(&palette, true);
    render(&window);
    nkb_gui::live::set_compact(&palette, false);
    render(&window);
    let refit = render(&window);
    assert!(
        surface_edge(&refit) < edge_tall,
        "expanding again did not fit the palette to the short value ({} vs {edge_tall}), so \
         the check above cannot tell a held height from one that never moves",
        surface_edge(&refit)
    );

    // Put the specimen back, so the renders saved below are the ones the gallery
    // and the calibration tool compare against.
    fill(&palette);
    palette.set_compact(false);

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

    // ---- a send in progress: the band draws, in both states (OBS-160) -------
    // Measured as a difference of two renders: switching the band on must ADD
    // accent ink - the counter and the filled part of the bar - and a bar that
    // grows with the share must add more at a larger share. Compact, the band
    // stays, and the resting rule (D53) holds for it as for everything else.
    palette.set_clipboard_mode(false);
    palette.set_sending_label("Typing".into());
    palette.set_sending_counter("1784 / 65535".into());
    palette.set_sending_hint("Press Esc to stop.".into());
    palette.set_sending_fraction(0.25);
    palette.set_sending(false);
    let accent_idle = offscreen::count_exactly(&render(&window), ACCENT);
    palette.set_sending(true);
    let sending = render(&window);
    let sending_path = offscreen::save_cropped(&sending, WIDTH, HEIGHT, "palette-sending.png");
    assert_nothing_escapes_the_surface(
        &sending,
        "while a value is being typed",
        &sending_path.display().to_string(),
    );
    let accent_quarter = offscreen::count_exactly(&sending, ACCENT);
    assert!(
        accent_quarter > accent_idle,
        "switching the send band on added no accent ink ({accent_idle} vs {accent_quarter}), \
         so the palette does not draw it. Look at {}",
        sending_path.display()
    );
    palette.set_sending_fraction(0.75);
    let accent_three_quarters = offscreen::count_exactly(&render(&window), ACCENT);
    assert!(
        accent_three_quarters > accent_quarter,
        "three quarters of the bar drew no more accent than one quarter ({accent_quarter} vs \
         {accent_three_quarters}), so the bar does not follow the share. Look at {}",
        sending_path.display()
    );
    palette.set_compact(true);
    // A difference of two renders again, and for a measured reason: the pack
    // band's counter wears the accent too, so "some accent in the compact
    // palette" held with the band hidden - mutation M392 passed against it.
    palette.set_sending(false);
    let accent_compact_idle = offscreen::count_exactly(&render(&window), ACCENT);
    palette.set_sending(true);
    let sending_compact = render(&window);
    let sending_compact_path = offscreen::save_cropped(
        &sending_compact,
        WIDTH,
        HEIGHT,
        "palette-sending-compact.png",
    );
    assert!(
        offscreen::count_exactly(&sending_compact, ACCENT) > accent_compact_idle,
        "the compact palette hides the send band, so a tester who made it small is not told \
         that a send runs or how to stop it. Look at {}",
        sending_compact_path.display()
    );
    assert_eq!(
        offscreen::count_exactly(&sending_compact, TEXT_MUTED),
        0,
        "the compact palette with a send in progress shows `text-muted` - D53 keeps that \
         role out of the resting state. Look at {}",
        sending_compact_path.display()
    );
    palette.set_compact(false);
    palette.set_sending(false);

    // ---- before the first value: the empty state draws (D83) ---------------
    // GUI rule 3: the value band has an empty state and it is on screen from
    // the first frame. Measured as a difference of two renders, like everything
    // above: the sentence must ADD ink in its own role, `text-secondary`.
    palette.set_clipboard_mode(false);
    palette.set_has_value(false);
    palette.set_no_value("".into());
    let secondary_without = offscreen::count_exactly(&render(&window), TEXT_SECONDARY);
    palette.set_no_value(
        "Nothing sent yet. Put the cursor in any field and press Alt+Shift+N.".into(),
    );
    let empty_state = render(&window);
    let empty_path = offscreen::save_cropped(&empty_state, WIDTH, HEIGHT, "palette-empty.png");
    assert_nothing_escapes_the_surface(
        &empty_state,
        "before the first value",
        &empty_path.display().to_string(),
    );
    assert!(
        offscreen::count_exactly(&empty_state, TEXT_SECONDARY) > secondary_without,
        "the empty-state sentence left no ink, so the value band shows nothing before the \
         first value. Look at {}",
        empty_path.display()
    );
    fill(&palette);
    palette.set_compact(false);

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
        "compact and expanded rendered identically, so `compact` gates nothing"
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
