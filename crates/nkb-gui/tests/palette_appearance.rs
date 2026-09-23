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

/// Nothing but the substitution marker, so its own ink can be measured apart
/// from the rest of the value.
const MARKER_ONLY: &str = "\u{2423}\u{2423}\u{2423}\u{2423}";

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
    palette.set_degraded_label("direct input refused".into());
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

#[test]
fn the_palette_renders_every_state_and_keeps_muted_text_out_of_the_resting_one() {
    let window = offscreen::start(WIDTH, HEIGHT);
    let palette = Palette::new().expect("the palette must build");
    fill(&palette);
    palette.show().expect("the palette must show");

    // ---- resting: the state the rule is about ------------------------------
    // Degraded as well, because that is the WORST resting case: the standing bar
    // is the only extra thing the dimmed palette ever shows, so if any role in
    // it were muted this is where it would appear.
    palette.set_showing(false);
    palette.set_degraded(true);
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
    palette.set_degraded(false);
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
        "blanking the preview and the shape changed nothing on screen ({with_preview} vs          {without_preview} pixels of text-primary), so the value band is not drawing them"
    );
    // The marker is the point: U+2423 must leave ink of its own, or the preview
    // shows a value with a hole where the invisible character was.
    palette.set_value_preview(MARKER_ONLY.into());
    let marker_only = offscreen::count_exactly(&render(&window), TEXT_PRIMARY);
    assert!(
        marker_only > without_preview,
        "the substitution marker left no ink, so it has no glyph in the shipped          typeface and the preview would show nothing where an invisible character sits"
    );
    // Put the specimen back, so the renders saved below are the ones the gallery
    // and the calibration tool compare against.
    fill(&palette);
    palette.set_showing(true);

    palette.set_degraded(true);
    let degraded = render(&window);
    let degraded_path = offscreen::save_cropped(&degraded, WIDTH, HEIGHT, "palette-degraded.png");

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
        showing != degraded,
        "the standing bar changed no pixel, so `degraded` gates nothing"
    );

    println!(
        "palette rendered to {}, {}, {}",
        resting_path.display(),
        showing_path.display(),
        degraded_path.display()
    );
}
