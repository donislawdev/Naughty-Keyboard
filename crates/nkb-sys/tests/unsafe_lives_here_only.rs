//! `unsafe` exists in this package and nowhere else in the workspace.
//!
//! # Why this is worth a test
//!
//! The workspace sets `unsafe_code = "forbid"`, which is genuinely airtight
//! while it stands: measured 2026-09-08, `#![allow(unsafe_code)]` under `forbid`
//! does not compile (`error[E0453]`). So the compiler already guards every
//! package that INHERITS those lints.
//!
//! The hole is one line wide, and it is the line this package itself had to
//! write. Cargo's lint inheritance is all-or-nothing: a package that declares
//! its own `[lints]` table stops inheriting the workspace's, silently and
//! successfully. Measured the same day, in a scratch workspace: a package with
//! `[lints.rust] unsafe_code = "allow"` compiles `unsafe` under a `forbid`
//! workspace, with no warning that anything was overridden.
//!
//! So a session that needs `unsafe` somewhere else does not have to argue with
//! `forbid` at all - it can copy four lines out of this manifest into another
//! one and the build stays green. That is not a hypothetical: it is the shortest
//! path, and it is the path this very package took an hour before this test was
//! written.
//!
//! 🔴 What that would cost is not style. `nkb-core` is pure, and its purity is
//! why architektura.md 6.5 can say the core is thread-safe by definition rather
//! than by care. `unsafe` arriving in `nkb-adapters` would put memory safety
//! back into the TOML reader and the filesystem code, where nothing needs it.
//!
//! # What this checks, and what it cannot
//!
//! 🔴 Written down rather than discovered later:
//!
//! - it reads `.rs` files under `crates/*/src` and `crates/*/tests`. Code
//!   generated at build time is not on disk when this runs and is not seen.
//! - it strips `//` line comments before looking, so the prose above does not
//!   trip it - but it does NOT strip `/* */` blocks or string literals. A file
//!   using either around the word would be a false positive, loudly rather than
//!   quietly, and no file in this workspace does today.
//! - it says nothing about whether the `unsafe` in THIS package is correct. That
//!   is what the probe and the review are for. This only answers "where".

// A failed expectation in a test is a failed test.
#![allow(clippy::panic, clippy::expect_used)]

use std::path::{Path, PathBuf};

/// The one package allowed to contain `unsafe`.
const THE_EXCEPTION: &str = "nkb-sys";

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the workspace root must be two levels above this package")
        .to_path_buf()
}

/// Every `.rs` file under a directory, recursively.
fn rust_files(dir: &Path, into: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, into);
        } else if path.extension().is_some_and(|e| e == "rs") {
            into.push(path);
        }
    }
}

/// Lines carrying the `unsafe` keyword, ignoring `//` comments.
fn unsafe_lines(body: &str) -> Vec<(usize, String)> {
    body.lines()
        .enumerate()
        .filter_map(|(index, raw)| {
            let code = raw.split("//").next().unwrap_or("");
            // Word boundary, so `unsafely` or `unsafe_code = "forbid"` in a
            // manifest-shaped string does not count. The keyword is always
            // followed by a space, a brace or a newline in real code.
            let hit = code
                .split_whitespace()
                .any(|word| word == "unsafe" || word == "unsafe{" || word.starts_with("unsafe("));
            hit.then(|| (index + 1, raw.trim().to_string()))
        })
        .collect()
}

#[test]
fn no_package_other_than_this_one_contains_unsafe() {
    let root = workspace_root();
    let crates = root.join("crates");
    let entries = std::fs::read_dir(&crates).expect("crates/ must be readable");

    let mut problems = Vec::new();
    let mut packages_seen = 0;
    let mut files_read = 0;
    let mut unsafe_here = 0;

    for entry in entries.flatten() {
        let package_dir = entry.path();
        if !package_dir.is_dir() {
            continue;
        }
        let package = package_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        packages_seen += 1;

        let mut files = Vec::new();
        rust_files(&package_dir.join("src"), &mut files);
        rust_files(&package_dir.join("tests"), &mut files);

        for file in files {
            let Ok(body) = std::fs::read_to_string(&file) else {
                continue;
            };
            files_read += 1;
            let hits = unsafe_lines(&body);
            if package == THE_EXCEPTION {
                unsafe_here += hits.len();
                continue;
            }
            for (line, text) in hits {
                let shown = file.strip_prefix(&root).unwrap_or(&file).display();
                problems.push(format!("  {shown}:{line}  {text}"));
            }
        }
    }

    // Without these, a clean result would also be what a wrong path produces.
    assert!(
        packages_seen >= 5,
        "saw {packages_seen} packages, expected at least 5 - the check looked in the wrong place, \
         so a clean result means nothing"
    );
    assert!(
        files_read > 20,
        "read {files_read} files, expected many more - see above"
    );
    // The strongest of the three: if the exception itself stopped containing
    // `unsafe`, this test would be guarding an empty rule and would keep passing
    // forever while doing nothing.
    assert!(
        unsafe_here > 0,
        "found no `unsafe` in {THE_EXCEPTION} at all. Either the FFI moved elsewhere - which is \
         exactly what this test exists to catch - or this test is now watching nothing."
    );

    assert!(
        problems.is_empty(),
        "\n`unsafe` appeared outside {THE_EXCEPTION}:\n\n{}\n\n\
         The workspace forbids it, so this compiled only because the package declares its own \
         `[lints]` table - which silently drops the workspace's rules for that whole package, \
         `unwrap_used` and `panic` included.\n\
         If a system call is genuinely needed, it belongs in {THE_EXCEPTION} behind a safe \
         function - see that package's manifest for why it is a package rather than an exception.\n",
        problems.join("\n")
    );
}

#[test]
fn the_workspace_still_forbids_unsafe_everywhere_else() {
    // The test above compares packages against each other, so it would stay
    // green if the workspace rule were removed and nobody had yet written an
    // `unsafe`. This one reads the rule itself.
    let manifest = workspace_root().join("Cargo.toml");
    let body = std::fs::read_to_string(&manifest).expect("the workspace manifest must be readable");

    assert!(
        body.contains(r#"unsafe_code = "forbid""#),
        "the workspace manifest no longer forbids unsafe.\n\
         `forbid` is not decoration here: it is the reason four of the six packages cannot \
         acquire `unsafe` even with an `#[allow]`, and downgrading it to `deny` would make that \
         a matter of review instead of a matter of compilation."
    );

    // And the packages that are supposed to inherit it must still say so.
    let root = workspace_root();
    let mut not_inheriting = Vec::new();
    for package in ["nkb-core", "nkb-app", "nkb-adapters", "nkb-cli"] {
        let path = root.join("crates").join(package).join("Cargo.toml");
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{} must be readable: {e}", path.display()));
        if !text.contains("[lints]") || !text.contains("workspace = true") {
            not_inheriting.push(package);
        }
    }
    assert!(
        not_inheriting.is_empty(),
        "these packages stopped inheriting the workspace lints: {not_inheriting:?}\n\
         Cargo's inheritance is all-or-nothing, so a package with its own `[lints]` table loses \
         `unsafe_code = \"forbid\"` AND the clippy denials in one step, without a warning."
    );
}
