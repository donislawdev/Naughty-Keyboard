//! Nothing `nkb` writes can crash it.
//!
//! # Why this is worth a test
//!
//! `println!` panics when its write fails, and a pipe whose reader has gone
//! fails every write. Measured 2026-10-08 on Windows, before `src/output.rs`
//! existed: `nkb packs`, `nkb --help`, `nkb show whitespace` and `nkb lint` on
//! a shipped pack, each with a reader that had already closed the pipe, ended
//! in `failed printing to stdout` (os error 232) and exit code 101, two runs out
//! of two each. 101 is not in the table of exit codes, which `exit.rs` calls
//! complete, so a script branching on the code met a number it was promised
//! would never come. `nkb emit` survived, and ended with 0 on every failed
//! write, a full disk included.
//!
//! Nothing showed it. Every test ran the commands with a reader that read to
//! the end, and that is the only reader a test writes by accident.
//!
//! # What this checks
//!
//! - every command that writes on standard output, run as the built binary
//!   with a pipe whose reader is closed BEFORE it starts: the run ends with the
//!   code it would have found anyway, says nothing, and does not panic. A
//!   reader that closes first makes the failure certain rather than a race
//!   with the pipe's buffer, which is what a reader that merely stops early
//!   would be.
//! - the same commands with a standard output that refuses every write for a
//!   reason that is not a closed pipe: the run ends with 5 and says so on
//!   standard error. What refuses is not the same on every system, see
//!   `refusing_output`.
//! - on Linux, a full disk, through `/dev/full`.
//! - that no package of the workspace writes with a macro that panics, so the
//!   next command or the next package cannot bring the failure back.
//!
//! # What it cannot see
//!
//! - standard error. Its writes go through `let _ = writeln!(err, ...)`, and a
//!   failure there is dropped on purpose (`src/output.rs`). Nothing here proves
//!   a run with standard error closed, because nothing could be told about it.
//! - `nkb send`, beyond its help. It writes nothing on standard output, and
//!   running it presses keys in whatever window has the focus.
//! - a writer that is not a macro: `std::io::stdout().write_all(..).unwrap()`
//!   would panic the same way and passes the scan. The workspace denies
//!   `unwrap_used` and `expect_used`, which is the net for that shape.
//! - the scan strips `//` comments at the first `//` on a line, so code after a
//!   string holding `//` is not read. That can only miss, never accuse.

// A failed expectation in a test is a failed test.
#![allow(clippy::panic, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const NKB: &str = env!("CARGO_BIN_EXE_nkb");

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

/// One command and the exit code it ends with when its output reaches nobody
/// or reaches somebody, which must be the same.
struct Case {
    args: Vec<String>,
    code: i32,
    /// Writes a file in the folder it runs in.
    writes_here: bool,
}

fn case(args: &[&str], code: i32) -> Case {
    Case {
        args: args.iter().map(|a| (*a).to_string()).collect(),
        code,
        writes_here: false,
    }
}

/// Every command that writes on standard output.
fn cases() -> Vec<Case> {
    let root = workspace_root();
    let shipped = root.join("packs").join("whitespace.toml");
    let shipped = shipped.to_str().expect("the path is text");
    let rejected = root
        .join("tests")
        .join("packs")
        .join("rejected")
        .join("breaks-too-short.toml");
    let rejected = rejected.to_str().expect("the path is text");
    let mut all = vec![
        case(&["--version"], 0),
        case(&["--help"], 0),
        case(&[], 0),
        case(&["packs"], 0),
        case(&["packs", "--help"], 0),
        case(&["show", "whitespace"], 0),
        case(&["show", "--help"], 0),
        case(&["emit", "whitespace"], 0),
        case(&["emit", "length-bombs", "--format", "lines", "--raw"], 0),
        case(&["emit", "--help"], 0),
        case(&["send", "--help"], 0),
        case(&["lint", shipped], 0),
        case(&["lint", shipped, "--json"], 0),
        case(&["lint", "--explain"], 0),
        // A failing verdict stays failing when its reader leaves.
        case(&["lint", rejected], 1),
        case(&["lint", "--help"], 0),
        case(&["fmt", shipped, "--dry-run"], 0),
        case(&["fmt", "--help"], 0),
        case(&["new-pack", "--help"], 0),
    ];
    all.push(Case {
        writes_here: true,
        ..case(&["new-pack", "closed-output-probe"], 0)
    });
    all
}

/// A folder of its own for one run, so a command that writes a file writes it
/// somewhere nothing else looks.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("nkb-closed-output-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a scratch folder can be made");
    dir
}

/// Runs `nkb` with this standard output and gives back its exit code and
/// everything it said on standard error.
fn run(args: &[String], stdout: Stdio, here: &Path) -> (i32, String) {
    let output = Command::new(NKB)
        .args(args)
        .current_dir(here)
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(Stdio::piped())
        .output()
        .expect("nkb starts");
    let code = output
        .status
        .code()
        .expect("nkb ends with a code, not a signal");
    (code, String::from_utf8_lossy(&output.stderr).into_owned())
}

/// A pipe nobody will ever read: its reading end is closed before the command
/// starts, so the first write fails for certain.
fn closed_pipe() -> Stdio {
    let (reader, writer) = std::io::pipe().expect("a pipe can be made");
    drop(reader);
    Stdio::from(writer)
}

/// A standard output that refuses every write for a reason that is not a
/// closed pipe.
///
/// 🔴 Not the same thing on every system, and that was measured on the first
/// pull request rather than assumed. On Windows a file opened for reading only
/// refuses writes (access denied). On Linux and macOS the same file fails with
/// `EBADF`, and the standard library takes `EBADF` on standard output as an
/// output somebody closed on purpose and reports SUCCESS: `nkb` ended with 0
/// there, every write gone. So Linux and macOS get a datagram socket connected
/// to nobody, which refuses every write with an error of its own.
#[cfg(windows)]
fn refusing_output(dir: &Path) -> Stdio {
    let path = dir.join("stdout.txt");
    std::fs::write(&path, b"").expect("the file can be made");
    Stdio::from(std::fs::File::open(&path).expect("the file opens for reading"))
}

#[cfg(unix)]
fn refusing_output(_dir: &Path) -> Stdio {
    let socket = std::os::unix::net::UnixDatagram::unbound().expect("a socket can be made");
    Stdio::from(std::os::fd::OwnedFd::from(socket))
}

fn says_it_panicked(stderr: &str) -> bool {
    stderr.contains("panicked") || stderr.contains("RUST_BACKTRACE")
}

const FAILURE_SENTENCE: &str = "nkb: standard output could not be written (";

#[test]
fn a_reader_that_left_changes_neither_the_exit_code_nor_what_is_said() {
    let mut wrong = Vec::new();
    for (index, case) in cases().iter().enumerate() {
        let here = scratch(&format!("closed-{index}"));
        let (code, stderr) = run(&case.args, closed_pipe(), &here);
        if code != case.code || says_it_panicked(&stderr) || stderr.contains(FAILURE_SENTENCE) {
            wrong.push(format!(
                "nkb {:?} -> {code} (want {}): {stderr}",
                case.args, case.code
            ));
        }
        if case.writes_here && !here.join("closed-output-probe.toml").is_file() {
            wrong.push(format!("nkb {:?} wrote no file", case.args));
        }
        let _ = std::fs::remove_dir_all(&here);
    }
    assert!(wrong.is_empty(), "with the reader gone: {wrong:#?}");
}

#[test]
fn an_output_that_refuses_writes_ends_the_run_with_5_and_says_so() {
    let mut wrong = Vec::new();
    for (index, case) in cases().iter().enumerate() {
        let here = scratch(&format!("refused-{index}"));
        let (code, stderr) = run(&case.args, refusing_output(&here), &here);
        let said_once = stderr.matches(FAILURE_SENTENCE).count() == 1;
        if code != 5 || says_it_panicked(&stderr) || !said_once {
            wrong.push(format!("nkb {:?} -> {code} (want 5): {stderr}", case.args));
        }
        let _ = std::fs::remove_dir_all(&here);
    }
    assert!(
        wrong.is_empty(),
        "with standard output refusing writes: {wrong:#?}"
    );
}

/// The case the rule was written for, staged where a system can stage it: a
/// full disk. Only Linux has a device for that.
#[cfg(target_os = "linux")]
#[test]
fn a_full_disk_ends_the_run_with_5_and_says_so() {
    let here = scratch("full");
    let full = std::fs::OpenOptions::new()
        .write(true)
        .open("/dev/full")
        .expect("/dev/full opens for writing");
    let args = ["emit".to_string(), "length-bombs".to_string()];
    let (code, stderr) = run(&args, Stdio::from(full), &here);
    let _ = std::fs::remove_dir_all(&here);
    assert_eq!(code, 5, "{stderr}");
    assert_eq!(stderr.matches(FAILURE_SENTENCE).count(), 1, "{stderr}");
}

/// The control: the harness runs the binary it means to, and with a reader
/// that reads, a command answers. Without this, the two tests above would pass
/// against a program that printed nothing at all.
#[test]
fn with_a_reader_that_reads_the_answer_arrives_whole() {
    let here = scratch("reads");
    let output = Command::new(NKB)
        .args(["emit", "length-bombs", "--format", "lines", "--raw"])
        .current_dir(&here)
        .stdin(Stdio::null())
        .output()
        .expect("nkb starts");
    let _ = std::fs::remove_dir_all(&here);
    assert_eq!(output.status.code(), Some(0));
    // Large enough to outgrow any pipe buffer, so the closed-pipe case above
    // has writes left to fail after the first one.
    assert!(
        output.stdout.len() > 100_000,
        "the answer was {} bytes",
        output.stdout.len()
    );
    assert!(String::from_utf8_lossy(&output.stderr).starts_with("nkb emit: "));
}

// ---------------------------------------------------------------------------
// The macros that panic stay out of every package
// ---------------------------------------------------------------------------

/// The macros that panic when their write fails.
const PANICKING_WRITERS: &[&str] = &["println!", "print!", "eprintln!", "eprint!", "dbg!"];

/// What `nkb` writes with instead (`src/output.rs`), counted by the same scan.
const SAFE_WRITERS: &[&str] = &["outln!", "out!"];

/// Every use of `names` in one line of code, where nothing in front of a hit
/// makes it part of a longer name (`eprintln!` holds `println!`).
fn macro_uses<'a>(code: &str, names: &[&'a str]) -> Vec<&'a str> {
    let mut found = Vec::new();
    for name in names.iter().copied() {
        let mut from = 0;
        while let Some(hit) = code[from..].find(name) {
            let at = from + hit;
            let before = code[..at].chars().next_back();
            if !before.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_') {
                found.push(name);
            }
            from = at + name.len();
        }
    }
    found
}

/// A line without its `//` comment, so a rule can be written about in the file
/// it governs.
fn code_only(line: &str) -> &str {
    line.find("//").map_or(line, |cut| &line[..cut])
}

/// The part of a source file that ships: everything before a `#[cfg(test)]`
/// that opens the test module, whatever attributes stand between the two.
fn shipped_part(text: &str) -> &str {
    // Inclusive of the line break, so the offsets hold for `\r\n` as well.
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let mut offset = 0;
    for (index, line) in lines.iter().enumerate() {
        if line.trim_end() == "#[cfg(test)]" {
            let opens = lines[index + 1..]
                .iter()
                .find(|l| l.starts_with(|c: char| c.is_ascii_alphabetic()))
                .is_some_and(|l| l.starts_with("mod "));
            if opens {
                return &text[..offset];
            }
        }
        offset += line.len();
    }
    text
}

fn rust_files(dir: &Path, into: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries {
        let path = entry.expect("a directory entry").path();
        if path.is_dir() {
            rust_files(&path, into);
        } else if path.extension().is_some_and(|e| e == "rs") {
            into.push(path);
        }
    }
}

/// Every panicking writer in one file's shipped code, as `line: macro`.
fn panicking_writers(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    for (number, line) in shipped_part(text).lines().enumerate() {
        for bad in macro_uses(code_only(line), PANICKING_WRITERS) {
            found.push(format!("{}: {bad}", number + 1));
        }
    }
    found
}

#[test]
fn no_package_writes_with_a_macro_that_panics() {
    let crates = workspace_root().join("crates");
    let mut files = Vec::new();
    for entry in std::fs::read_dir(&crates).expect("a crates folder") {
        rust_files(
            &entry.expect("a directory entry").path().join("src"),
            &mut files,
        );
    }
    let mut safe = 0;
    let mut offenders = Vec::new();
    for path in &files {
        let text = std::fs::read_to_string(path).expect("a source file is readable");
        for line in shipped_part(&text).lines() {
            safe += macro_uses(code_only(line), SAFE_WRITERS).len();
        }
        for hit in panicking_writers(&text) {
            offenders.push(format!("{}:{hit}", path.display()));
        }
    }

    // Literals, not counts derived from what they check: 89 source files and
    // 159 safe writes when this was written. Fewer means the scan stopped
    // reading where the writing happens, which would look exactly like a
    // clean workspace.
    assert!(
        files.len() >= 80,
        "the scan read only {} source files",
        files.len()
    );
    assert!(
        safe >= 150,
        "the scan saw only {safe} uses of {SAFE_WRITERS:?} - it went blind or the writers were renamed"
    );
    assert!(
        offenders.is_empty(),
        "these macros panic when the output they write to is closed (`| head`), which ends \
         the run with exit code 101. Write with `outln!` or `out!` from nkb-cli/src/output.rs, \
         or with a `writeln!` whose result is handled: {offenders:#?}"
    );
}

/// The scan on text written for it, both directions.
#[test]
fn the_scan_finds_a_panicking_writer_in_code_and_nowhere_else() {
    let source = "fn a() {\n    println!(\"x\");\n    // println!(\"in a comment\");\n    \
                  eprint!(\"y\");\n    outln!(\"z\");\n    writeln!(err, \"w\");\n    \
                  let _ = dbg!(1);\n}\n\n#[cfg(test)]\n#[allow(\n    clippy::panic,\n)]\n\
                  mod tests {\n    fn b() { println!(\"in a test\"); }\n}\n";
    assert_eq!(
        panicking_writers(source),
        ["2: println!", "4: eprint!", "7: dbg!"],
        "the scan missed a writer, read a comment, or read the test module"
    );
    assert_eq!(macro_uses("eprintln!(x)", PANICKING_WRITERS), ["eprintln!"]);
    assert_eq!(
        macro_uses("outln!(x) + out!(y)", SAFE_WRITERS),
        ["outln!", "out!"]
    );
    // A `#[cfg(test)]` on something other than the test module cuts nothing.
    let gated = "#[cfg(test)]\nuse std::fmt;\nfn c() { print!(\"q\"); }\n";
    assert_eq!(panicking_writers(gated), ["3: print!"]);
}
