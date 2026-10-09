//! The project website lists the palette's shortcuts the way this build has them.
//!
//! `web/data/facts/shortcuts.json` holds every action, the name the palette
//! gives it, its default chord in the Windows and Linux table, and whether a
//! press of it does anything in this
//! build (`live::wired`). The macOS table is left out until shortcuts work
//! there: the settings grammar spells Command as `Win`, which on a page about
//! macOS would read as a mistake. The site's page of shortcuts is
//! built from it, so a changed default or a newly wired action cannot leave the
//! page saying the old thing.
//!
//! With `NKB_WRITE_SITE=1` this writes the file, and otherwise it fails when the
//! file and the program disagree. The words for each action are the site's own,
//! in every language it is built in, and the site build refuses an action that
//! has none.

#![allow(clippy::panic, clippy::expect_used)]

use nkb_adapters::i18n::action_name;
use nkb_core::hotkeys::{DEFAULT_BINDINGS, HotkeyAction, HotkeyChord};
use nkb_gui::live::wired;

fn chord_of(table: &[(HotkeyAction, HotkeyChord)], action: HotkeyAction) -> String {
    table
        .iter()
        .find(|(a, _)| *a == action)
        .map(|(_, chord)| chord.text())
        .unwrap_or_else(|| panic!("{} has no default chord", action.id()))
}

fn facts() -> String {
    let rows: Vec<String> = HotkeyAction::ALL
        .iter()
        .map(|action| {
            format!(
                "  {{\n    \"id\": \"{}\",\n    \"name\": \"{}\",\n    \"chord\": \"{}\",\n    \"works\": {}\n  }}",
                action.id(),
                action_name(*action),
                chord_of(&DEFAULT_BINDINGS, *action),
                wired(*action)
            )
        })
        .collect();
    format!("[\n{}\n]\n", rows.join(",\n"))
}

#[test]
fn the_site_lists_every_shortcut_the_palette_has() {
    let now = facts();
    assert!(
        now.is_ascii() && !now.contains('\\') && !now.contains("\"\""),
        "an id or a chord that would need escaping in JSON, or an empty one: {now}"
    );
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../web/data/facts/shortcuts.json");
    if std::env::var_os("NKB_WRITE_SITE").is_some_and(|v| v == "1") {
        std::fs::write(&path, &now).expect("the facts can be written");
        return;
    }
    let kept = std::fs::read_to_string(&path).expect(
        "web/data/facts/shortcuts.json is readable. \
         Write it: NKB_WRITE_SITE=1 cargo test -p nkb-gui --test site_facts",
    );
    assert_eq!(
        kept, now,
        "the website lists other shortcuts than the palette has. If the palette is right: \
         NKB_WRITE_SITE=1 cargo test -p nkb-gui --test site_facts"
    );
}
