//! `nkb` must not contain a graphical toolkit. Not lazily, not conditionally.
//!
//! # Why this is worth a test
//!
//! D25 says the two executables are two full products, and gives `nkb` one job
//! the other cannot do: run inside somebody else's CI, in a container with no
//! screen. architektura.md section 7 puts it plainly - the absence of a
//! graphical toolkit is a property OF THE ARTEFACT, not of anyone's discipline.
//!
//! Criterion 7 in architektura.md section 12 was satisfied on the day the stack
//! was chosen. Nothing kept it satisfied. One line in `nkb-app` or
//! `nkb-adapters` - both of which sit underneath `nkb-cli` - would pull Slint
//! into the command line tool, and NOTHING WOULD BREAK. The build succeeds, the
//! tests pass, the binary quietly grows, and it stops working in the one place
//! nobody looks until a stranger's pipeline fails.
//!
//! That is the shape of failure a guard is for: no error, no symptom, and a
//! long delay before anyone finds out.
//!
//! # What this checks, and what it cannot
//!
//! It reads the manifests of the four packages that make up `nkb` and refuses a
//! dependency whose name is on the list below.
//!
//! 🔴 What it CANNOT see, written down rather than discovered later:
//!
//! - a graphical crate pulled in INDIRECTLY, by some other dependency. Today
//!   the only external dependency under `nkb` is `toml_edit`, so the surface is
//!   small - but it is not zero, and it grows with every dependency added.
//! - a graphical crate whose name is not on the list. The list is a list of
//!   names, so it is incomplete by construction. It catches the realistic case,
//!   which is a session adding the toolkit it happens to be working with.
//! - anything about the size of the produced binary. The deep version of this
//!   check is `cargo tree -p nkb-cli`, which is what measured criterion 7 in the
//!   first place. It is not run here because invoking cargo from inside a cargo
//!   test is a known way to deadlock on the package cache.

// A failed expectation in a test is a failed test.
#![allow(clippy::panic, clippy::expect_used)]

use std::path::{Path, PathBuf};

/// The packages that end up inside the `nkb` executable.
///
/// `nkb-gui` is deliberately absent: it is the one package that IS allowed a
/// graphical toolkit, and it is not underneath `nkb-cli`.
const PACKAGES_INSIDE_NKB: &[&str] = &["nkb-core", "nkb-app", "nkb-adapters", "nkb-cli"];

/// Crate names that mean "this draws on a screen".
///
/// Slint and its internals first, then the toolkits it was measured against in
/// ADR-4, then the layers underneath all of them. A name here is a name that
/// must never appear in a manifest above.
const GRAPHICAL: &[&str] = &[
    "slint",
    "slint-build",
    "i-slint-core",
    "i-slint-backend-selector",
    "i-slint-backend-winit",
    "i-slint-renderer-software",
    "i-slint-renderer-skia",
    "i-slint-renderer-femtovg",
    "egui",
    "eframe",
    "iced",
    "iced_winit",
    "winit",
    "wgpu",
    "glutin",
    "femtovg",
    "skia-safe",
    "tiny-skia",
    "softbuffer",
    "raw-window-handle",
];

fn workspace_root() -> PathBuf {
    // CARGO_MANIFEST_DIR is crates/nkb-cli. The workspace is two levels up.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the workspace root must be two levels above this package")
        .to_path_buf()
}

#[test]
fn no_package_inside_nkb_declares_a_graphical_dependency() {
    let root = workspace_root();
    let mut problems = Vec::new();
    let mut manifests_read = 0;

    for package in PACKAGES_INSIDE_NKB {
        let manifest = root.join("crates").join(package).join("Cargo.toml");
        let body = std::fs::read_to_string(&manifest)
            .unwrap_or_else(|e| panic!("{} must be readable: {e}", manifest.display()));
        manifests_read += 1;

        for (number, raw) in body.lines().enumerate() {
            // Comments in these manifests name the toolkits on purpose - the
            // whole point of several of them is to explain what is NOT here.
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            // A dependency appears either as `name = ...` or as a table header
            // ending in `.name`.
            let declared = line
                .split_once('=')
                .map(|(left, _)| left.trim().trim_matches('"'))
                .or_else(|| line.strip_prefix('[').and_then(|t| t.strip_suffix(']')))
                .unwrap_or("");
            let name = declared.rsplit('.').next().unwrap_or("");

            if GRAPHICAL.contains(&name) {
                problems.push(format!(
                    "  {package}/Cargo.toml:{}  declares `{name}`",
                    number + 1
                ));
            }
        }
    }

    // Without this the test would pass just as happily if the paths were wrong
    // and it had read nothing at all.
    assert_eq!(
        manifests_read,
        PACKAGES_INSIDE_NKB.len(),
        "read {manifests_read} manifests but expected {} - the check inspected less than it \
         claims to, so a clean result means nothing",
        PACKAGES_INSIDE_NKB.len()
    );

    assert!(
        problems.is_empty(),
        "\na graphical toolkit reached the command line tool:\n\n{}\n\n\
         `nkb` runs in other people's CI, inside containers with no screen. D25 makes the \
         absence of a graphical toolkit a property of the artefact rather than of anyone's \
         discipline - architektura.md sections 7 and 12.\n\
         If a screen is genuinely needed, it belongs in nkb-gui.\n",
        problems.join("\n")
    );
}
