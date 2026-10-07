//! What the tool reads about the window in front, beside the kind of control,
//! is the name of its program - and this file holds that to a list (`D104`).
//!
//! # The promise this stands for
//!
//! Untouchable rule 17: the tool does not read the contents of windows. UX8
//! gave `nkb-sys` one more read at the press: the FILE NAME of the program the
//! window belongs to, so the palette can say where a value went. The owner
//! drew the line: the program and the kind of control, never the title, never
//! a watch on which windows the tester moves between. This guard is that line
//! in code.
//!
//! # What this checks, and what it cannot
//!
//! - the one call that reads a process image path, `QueryFullProcessImageNameW`,
//!   appears in `program.rs` and nowhere else, so every caller goes through the
//!   function that cuts the path down to its file name,
//! - none of the calls that read a window's text or a process beyond its name
//!   appears anywhere in `crates/nkb-sys/src`: the window text calls and
//!   message, the process memory and command line, the module and image file
//!   names by other routes, the version resource, and the two process rights
//!   wider than the limited query,
//! - a window-event hook - the way to WATCH which window comes to the front -
//!   does not appear either,
//! - test modules and `//` comments are dropped first, as in
//!   `field_reads_only_kinds.rs`.
//!
//! It cannot see a read done through another crate - `unsafe_lives_here_only.rs`
//! keeps system calls inside this package - nor a read the platform does on its
//! own behalf.

#![allow(clippy::panic, clippy::expect_used)]

use std::path::Path;

/// The one call that reads where a process's program lives, and the file it
/// may stand in.
const IMAGE_PATH_CALL: &str = "QueryFullProcessImageNameW";
const IMAGE_PATH_FILE: &str = "program.rs";

/// Calls, messages and rights that read more than a program's name - or watch
/// the tester. Matched as plain text in code, so a prefix covers both the `A`
/// and the `W` form.
const FORBIDDEN: &[&str] = &[
    "GetWindowText",
    "InternalGetWindowText",
    "WM_GETTEXT",
    "ReadProcessMemory",
    "NtQueryInformationProcess",
    "GetModuleFileNameEx",
    "GetModuleBaseName",
    "GetProcessImageFileName",
    "GetFileVersionInfo",
    "VerQueryValue",
    "PROCESS_VM_READ",
    "PROCESS_QUERY_INFORMATION,",
    "PROCESS_QUERY_INFORMATION |",
    "PROCESS_ALL_ACCESS",
    "SetWinEventHook",
];

/// The code of one file with test modules and `//` comments taken out.
fn code_of(source: &str) -> String {
    let mut out = String::new();
    for line in source.lines() {
        if line.trim_start().starts_with("#[cfg(test)]") {
            break;
        }
        let code = line.split("//").next().unwrap_or("");
        out.push_str(code);
        out.push('\n');
    }
    out
}

/// What is wrong with these files of `crates/nkb-sys/src`, each named with
/// its code.
fn findings(files: &[(String, String)]) -> Vec<String> {
    let mut problems = Vec::new();
    for (name, code) in files {
        if code.contains(IMAGE_PATH_CALL) && name != IMAGE_PATH_FILE {
            problems.push(format!(
                "{name} reads a process image path - only {IMAGE_PATH_FILE} may, where the folder is cut off"
            ));
        }
        for forbidden in FORBIDDEN {
            if code.contains(forbidden) {
                problems.push(format!("{name} uses {forbidden}"));
            }
        }
    }
    problems
}

#[test]
fn the_window_in_front_gives_its_program_name_and_nothing_more() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    for entry in std::fs::read_dir(&src).expect("src is readable") {
        let path = entry.expect("an entry").path();
        if path.extension().is_some_and(|e| e == "rs") {
            let name = path
                .file_name()
                .expect("a file name")
                .to_string_lossy()
                .into_owned();
            let code = code_of(&std::fs::read_to_string(&path).expect("a source file is readable"));
            files.push((name, code));
        }
    }
    assert!(
        files
            .iter()
            .any(|(name, code)| name == IMAGE_PATH_FILE && code.contains(IMAGE_PATH_CALL)),
        "the guard no longer sees the one read it is about - was program.rs renamed?"
    );
    let problems = findings(&files);
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn the_scanner_sees_what_it_is_meant_to_forbid() {
    // A guard nobody has seen fail is indistinguishable from a broken one.
    let file = |name: &str, code: &str| (name.to_owned(), code.to_owned());
    let clean = vec![
        file(
            "program.rs",
            "let read = unsafe { QueryFullProcessImageNameW(p, 0, b, &mut s) };\n",
        ),
        file(
            "privilege.rs",
            "OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid)\n",
        ),
    ];
    assert!(findings(&clean).is_empty(), "{:?}", findings(&clean));

    let names_it = |extra: (String, String), culprit: &str| {
        let mut files = clean.clone();
        files.push(extra);
        let found = findings(&files);
        assert!(
            found.iter().any(|problem| problem.contains(culprit)),
            "expected a report naming {culprit}, got {found:?}"
        );
    };
    // The image path read somewhere the folder is not cut off.
    names_it(
        file("field.rs", "QueryFullProcessImageNameW(p, 0, b, &mut s);\n"),
        "field.rs reads a process image path",
    );
    // The window's title.
    names_it(
        file(
            "window.rs",
            "GetWindowTextW(hwnd, buffer.as_mut_ptr(), 256);\n",
        ),
        "GetWindowText",
    );
    // A process opened wide enough to read its memory.
    names_it(
        file(
            "program.rs",
            "OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, 0, pid)\n",
        ),
        "PROCESS_VM_READ",
    );
    // Watching which window comes to the front.
    names_it(
        file(
            "hotkey.rs",
            "SetWinEventHook(3, 3, null, Some(cb), 0, 0, 0);\n",
        ),
        "SetWinEventHook",
    );
    // The version resource of the program's file.
    names_it(
        file("program.rs", "GetFileVersionInfoW(path, 0, size, data);\n"),
        "GetFileVersionInfo",
    );
    // A call hidden in a test module is not code of the product.
    let in_test = code_of("fn x() {}\n#[cfg(test)]\nGetWindowTextW(h, b, 9);\n");
    assert!(findings(&[file("window.rs", &in_test)]).is_empty());
}
