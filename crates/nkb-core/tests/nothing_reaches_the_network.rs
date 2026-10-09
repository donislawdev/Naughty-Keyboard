//! The tool promises it opens no network connection, and this is the first
//! thing that looks at the code rather than repeating the claim.
//!
//! The README says it three times and `SECURITY.md` once. Untouchable rule 17
//! puts "sends nothing over the network" first among the negative promises,
//! and none of them is visible in use: a tool that types into somebody else's
//! window and then quietly talks to a server would look exactly like one that
//! does not.
//!
//! # Two questions, two registers
//!
//! **Forbidden** spellings have no legitimate place in this workspace at all:
//! the standard library's sockets, an HTTP client, a name lookup, handing an
//! address to the shell or the browser. Their presence is the finding.
//!
//! **Watched** spellings are legitimate in named test files and nowhere else:
//! starting a process, and a local socket. Two tests start the `nkb` they were
//! built with, and one stands in for an output that refuses every write with a
//! socket nobody connects. Each use is registered below with its reason, a use
//! anywhere else fails, and the register may only name a test - the product
//! itself starts nothing and opens nothing.
//!
//! Views are read too. Slint has `Platform.open-url(...)`, which hands an
//! address to the system's browser, and a view can call it without a line of
//! Rust.
//!
//! # What this reads
//!
//! Every Rust and Slint file under `crates/`, through the lexer that
//! `hygiene.rs` and `prose_punctuation.rs` share (`repo_text`). Comments are
//! left out, so a rule can be written ABOUT without being reported. String
//! literals are read, because `LoadLibraryW("ws2_32.dll")` is exactly the
//! route a name-only scan of code would miss. A file whose literals list what
//! it refuses - this one, and the binary half of the guard - is registered as
//! such, and its CODE is still read like any other.
//!
//! The manifests are read for the networking features of `windows-sys`, and
//! `Cargo.lock` against the crates `deny.toml` bans outright. That list lives
//! in `deny.toml` alone: CI asks `cargo deny` the same question on every pull
//! request, and this asks it in every local `cargo test`, from the one list.
//!
//! # What this canNOT prove, said plainly
//!
//! - **It reads source, not binaries.** A dependency that links Winsock would
//!   not appear here. That is the other half of the guard,
//!   `links_nothing_off_the_machine.rs` in `nkb-cli` and in `nkb-gui`, which
//!   reads the import table of each built program.
//! - **It reads what is written, not what runs.** A name assembled at run time
//!   (`concat!`, a format string handed to `LoadLibrary`) defeats it.
//! - **On Linux the toolkit connects where the environment points.** Slint
//!   reaches the X server named by `DISPLAY`, over TCP when it names a host,
//!   and the session bus. That is the desktop the tester runs, not an address
//!   of ours, and it happens in Slint's code, not here.
//! - **Data can leave a machine without a socket** - a file in a synchronised
//!   folder, a value pasted into an issue. Nothing here looks at that.
//!
//! So this is not a proof of silence. That proof is `P1` in
//! `test-strategy.md`: watching the running program from outside. This is a
//! lock on the surface, so nobody adds a way out by accident, and adding one on
//! purpose means editing a register here and writing down why.
//!
//! # The canary is not decoration
//!
//! Every check is pointed at code it must reject and at clean code it must
//! pass. A guard nobody has watched fail is indistinguishable from one that
//! reads nothing.

#![allow(clippy::panic, clippy::expect_used)]

#[allow(dead_code)]
mod repo_text;

use repo_text::{Literal, Syntax, code, lex, product_tree, relative, workspace_root};
use std::collections::BTreeSet;
use std::path::Path;

// ---------------------------------------------------------------------------
// Register 1: spellings with no legitimate place here
// ---------------------------------------------------------------------------

/// Matched in code and in string literals of every Rust and Slint file, case
/// as written. A prefix covers the `A`, `W` and `Ex` forms of a Windows call.
const FORBIDDEN: &[(&str, &str)] = &[
    ("std::net", "the standard library's network"),
    ("TcpStream", "a TCP connection"),
    ("TcpListener", "a TCP server"),
    ("UdpSocket", "a UDP socket"),
    ("SocketAddr", "a network address"),
    (
        "ToSocketAddrs",
        "a name lookup, which is a network round trip",
    ),
    (
        "to_socket_addrs",
        "a name lookup, which is a network round trip",
    ),
    ("reqwest", "an HTTP client"),
    ("ureq", "an HTTP client"),
    ("hyper::", "an HTTP implementation"),
    ("isahc", "an HTTP client"),
    ("attohttpc", "an HTTP client"),
    ("curl::", "libcurl"),
    ("tokio::net", "an asynchronous network runtime"),
    ("async_std::net", "an asynchronous network runtime"),
    ("webbrowser::", "hands an address to the browser"),
    (
        "open::that",
        "hands an address or a file to the system's opener",
    ),
    (
        "opener::",
        "hands an address or a file to the system's opener",
    ),
    (
        "open_url",
        "Slint's call that hands an address to the browser",
    ),
    ("WinHttp", "WinHTTP, the Windows HTTP stack"),
    ("InternetOpen", "WinINet, the other Windows HTTP stack"),
    ("URLDownloadToFile", "a one-call downloader"),
    ("URLOpen", "the stream form of the same downloader"),
    ("WSAStartup", "Winsock"),
    ("WSASocket", "Winsock"),
    ("getaddrinfo", "a name lookup"),
    ("GetAddrInfo", "a name lookup"),
    ("gethostbyname", "a name lookup"),
    ("DnsQuery", "a name lookup"),
    ("Win32::Networking", "the networking calls of windows-sys"),
    (
        "Win32::NetworkManagement",
        "the network management calls of windows-sys",
    ),
    ("Win32::Web", "the web calls of windows-sys"),
    ("ShellExecute", "hands an address or a program to the shell"),
    ("WinExec", "starts a program through the shell's rules"),
    ("HlinkNavigate", "follows a hyperlink"),
    (
        "NtDeviceIoControlFile",
        "the call beneath Winsock, which reaches a socket without it",
    ),
    (
        "CoCreateInstanceEx",
        "a COM object that may live on another machine",
    ),
    ("NSURLSession", "the macOS HTTP client"),
    ("NSURLConnection", "the older macOS HTTP client"),
    ("CFNetwork", "the macOS network framework"),
    ("CFSocket", "a macOS socket"),
    ("nw_connection", "Network.framework"),
    ("NSWorkspace", "opens an address or a program on macOS"),
    (
        "LSOpen",
        "Launch Services, which opens an address or a program",
    ),
    ("xdg-open", "hands an address to the Linux desktop's opener"),
    ("libc::socket", "a socket through the C library"),
    ("libc::connect", "a connection through the C library"),
    ("SYS_socket", "a socket by system call number"),
    ("SYS_connect", "a connection by system call number"),
];

/// Libraries whose only purpose is the network, matched in string literals
/// without regard to case - where `#[link(name = ...)]`, `LoadLibraryW` and a
/// framework path all spell them.
const FORBIDDEN_LIBRARIES: &[(&str, &str)] = &[
    ("ws2_32", "Winsock"),
    ("wsock32", "the old Winsock"),
    ("mswsock", "the Winsock provider"),
    ("winhttp", "WinHTTP"),
    ("wininet", "WinINet"),
    ("urlmon", "the URL moniker library, a downloader"),
    ("dnsapi", "DNS"),
    ("iphlpapi", "the IP helper library"),
    ("websocket.dll", "the Windows WebSocket library"),
    ("network.framework", "Network.framework"),
];

/// In the code of a view. Slint's built-in that opens an address.
const FORBIDDEN_IN_VIEWS: &[(&str, &str)] = &[(
    "open-url",
    "Slint's call that hands an address to the browser",
)];

/// In the literals of a view. Views name files of this package, never an
/// address: the sentence guard of `ui_guard.rs` keeps every other literal out.
const ADDRESS_IN_A_VIEW: (&str, &str) = ("://", "an address in a view");

// ---------------------------------------------------------------------------
// Register 2: spellings legitimate in named test files and nowhere else
// ---------------------------------------------------------------------------

/// Reaching outside this process. `process::Command` and `Command::new`
/// rather than `Command`, which is also the palette's own `live::Command`.
/// `.spawn(` is not here: `nkb-sys` spawns a THREAD with it.
const WATCHED: &[(&str, &str)] = &[
    ("process::Command", "spawn"),
    ("Command::new", "spawn"),
    ("CreateProcess", "spawn"),
    ("posix_spawn", "spawn"),
    ("libc::fork", "spawn"),
    ("execv", "spawn"),
    ("UnixStream", "local-socket"),
    ("UnixDatagram", "local-socket"),
    ("UnixListener", "local-socket"),
    ("unix::net", "local-socket"),
];

/// Every place a watched spelling is allowed, and why. A use anywhere else
/// fails, an entry naming code that is gone fails, and an entry naming a file
/// that is not a test fails.
const ALLOWED: &[(&str, &str, &str)] = &[
    (
        "crates/nkb-cli/tests/closed_output.rs",
        "spawn",
        "runs the `nkb` this test run built, with its output closed, refused or full, to see it \
         keep the exit code it gives with somebody reading. It starts nothing else, and nothing \
         reaches past this machine",
    ),
    (
        "crates/nkb-cli/tests/closed_output.rs",
        "local-socket",
        "on Linux and macOS, a datagram socket that was never connected stands in for an output \
         that refuses every write. It is bound to no address and nothing is sent through it",
    ),
    (
        "crates/nkb-cli/tests/help_lists_every_command.rs",
        "spawn",
        "runs the `nkb` this test run built to ask each command for its help, and reads what it \
         answers. It starts nothing else, and nothing reaches past this machine",
    ),
];

/// Files whose string literals NAME what they refuse, so their literals are not
/// read for either register. Their code is read like any other file's.
const REFUSING: &[(&str, &str)] = &[
    (
        "crates/nkb-core/tests/nothing_reaches_the_network.rs",
        "this guard, which names every spelling it refuses and the code its canary must reject",
    ),
    (
        "crates/nkb-cli/tests/imports/mod.rs",
        "the binary half of this guard, which names the libraries and functions it refuses to \
         find in the built programs. Naming a library in order to refuse it is the opposite of \
         linking it",
    ),
];

// ---------------------------------------------------------------------------
// The scan itself, as pure functions over one file's text
// ---------------------------------------------------------------------------

/// One finding: which register was broken, the spelling and what it means, and
/// the line.
type Finding = (&'static str, String, usize);

/// Every place `needle` stands in `haystack`, as byte offsets.
fn offsets(haystack: &str, needle: &str, fold_case: bool) -> Vec<usize> {
    if fold_case {
        let lower = haystack.to_ascii_lowercase();
        let needle = needle.to_ascii_lowercase();
        lower.match_indices(&needle).map(|(at, _)| at).collect()
    } else {
        haystack.match_indices(needle).map(|(at, _)| at).collect()
    }
}

fn is_allowed(rel: &str, kind: &str) -> bool {
    ALLOWED
        .iter()
        .any(|(file, granted, _)| *file == rel && *granted == kind)
}

fn refuses(rel: &str) -> bool {
    REFUSING.iter().any(|(file, _)| *file == rel)
}

/// The findings in one piece of text: a line of code, or a literal. `line_of`
/// turns a byte offset into the line it stands on.
fn findings_in(
    text: &str,
    syntax: Syntax,
    literal: bool,
    rel: &str,
    line_of: &dyn Fn(usize) -> usize,
) -> Vec<Finding> {
    let mut out = Vec::new();
    let mut forbid = |list: &[(&'static str, &'static str)], fold: bool| {
        for (name, what) in list.iter().copied() {
            for at in offsets(text, name, fold) {
                out.push(("forbidden", format!("{name} ({what})"), line_of(at)));
            }
        }
    };
    forbid(FORBIDDEN, false);
    if literal {
        forbid(FORBIDDEN_LIBRARIES, true);
    }
    if syntax == Syntax::Slint && literal {
        forbid(&[ADDRESS_IN_A_VIEW], false);
    }
    if syntax == Syntax::Slint && !literal {
        forbid(FORBIDDEN_IN_VIEWS, false);
    }
    for (name, kind) in WATCHED.iter().copied() {
        if is_allowed(rel, kind) {
            continue;
        }
        for at in offsets(text, name, false) {
            out.push(("unregistered", format!("{name} ({kind})"), line_of(at)));
        }
    }
    out
}

/// One literal's text, and the line each of its bytes stands on.
fn literal_text(literal: &Literal) -> (String, Vec<usize>) {
    let mut text = String::new();
    let mut lines = Vec::new();
    for &(c, line) in &literal.chars {
        text.push(c);
        lines.extend(std::iter::repeat_n(line, c.len_utf8()));
    }
    (text, lines)
}

/// Every finding in one file. A pure function over text, so the canary can
/// point it at code that is not in the repository at all.
fn findings(syntax: Syntax, source: &str, rel: &str) -> Vec<Finding> {
    let lexed = lex(syntax, source);
    let mut out = Vec::new();
    for (index, line) in code(source, &lexed).lines().enumerate() {
        out.extend(findings_in(line, syntax, false, rel, &|_| index + 1));
    }
    if refuses(rel) {
        return out;
    }
    for literal in &lexed.literals {
        let (text, lines) = literal_text(literal);
        let line_of = |at: usize| lines.get(at).copied().unwrap_or(literal.first_line);
        out.extend(findings_in(&text, syntax, true, rel, &line_of));
    }
    out
}

// ---------------------------------------------------------------------------
// Reading the workspace
// ---------------------------------------------------------------------------

/// Every Rust and Slint file under `crates/`, with its path from the root.
fn scanned_files() -> Vec<(String, Syntax, String)> {
    let root = workspace_root();
    let mut out = Vec::new();
    for (path, syntax) in product_tree(&root).files {
        let rel = relative(&root, &path);
        if !rel.starts_with("crates/") || !matches!(syntax, Syntax::Rust | Syntax::Slint) {
            continue;
        }
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{rel} must be readable: {e}"));
        out.push((rel, syntax, text));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("{} must be readable: {e}", path.display()))
}

// ---------------------------------------------------------------------------
// N1. Nothing reaches outside the registers
// ---------------------------------------------------------------------------

#[test]
fn nothing_reaches_the_network_outside_the_named_exceptions() {
    let files = scanned_files();
    let mut offenders = Vec::new();
    for (rel, syntax, text) in &files {
        for (kind, detail, line) in findings(*syntax, text, rel) {
            offenders.push(format!("{rel}:{line} {kind}: {detail}"));
        }
    }
    let rust = files.iter().filter(|f| f.1 == Syntax::Rust).count();
    let views = files.iter().filter(|f| f.1 == Syntax::Slint).count();
    println!("network scan: {rust} Rust files, {views} Slint files");

    // Literals, not counts derived from what they check: 138 Rust files and 14
    // views when this was written. Fewer means the scan stopped reading where
    // the code is, which would look exactly like a clean workspace.
    assert!(rust >= 130, "the network scan read only {rust} Rust files");
    assert!(
        views >= 12,
        "the network scan read only {views} Slint files"
    );
    assert!(
        offenders.is_empty(),
        "the tool promises it opens no network connection. Either this is a mistake, or the \
         register at the top of nothing_reaches_the_network.rs needs an entry saying why it is \
         not: {offenders:#?}"
    );
}

// ---------------------------------------------------------------------------
// N2. The register grants nothing to the product
// ---------------------------------------------------------------------------

/// The registered files that are not tests. A test may start the program it
/// was built with. The program starts nothing and opens nothing.
fn grants_outside_tests<'a>(files: impl Iterator<Item = &'a str>) -> Vec<&'a str> {
    files
        .filter(|file| !file.split('/').any(|part| part == "tests"))
        .collect()
}

#[test]
fn the_registers_name_only_tests() {
    let granted = ALLOWED.iter().map(|(file, _, _)| *file);
    let refusing = REFUSING.iter().map(|(file, _)| *file);
    let outside = grants_outside_tests(granted.chain(refusing));
    assert!(
        outside.is_empty(),
        "the registers may only name tests. The product itself starts no process and opens no \
         socket, and a grant to a file of the product would be a different product: {outside:?}"
    );
}

// ---------------------------------------------------------------------------
// N3. No crate deny.toml bans outright is in the lock file
// ---------------------------------------------------------------------------

/// The crates `deny.toml` bans everywhere: the `deny` entries without
/// `wrappers`. An entry WITH wrappers bans a crate in some places only (the
/// toolkit outside `nkb-gui`), which only `cargo deny` can see.
fn absolute_bans(deny_toml: &str) -> Vec<String> {
    deny_toml
        .lines()
        .map(str::trim)
        .filter(|line| !line.contains("wrappers"))
        .filter_map(|line| line.strip_prefix("{ crate = \""))
        .filter_map(|rest| rest.split('"').next())
        .map(str::to_string)
        .collect()
}

/// Every package name in a lock file.
fn locked_names(lock: &str) -> BTreeSet<String> {
    lock.lines()
        .filter_map(|line| line.strip_prefix("name = \""))
        .map(|rest| rest.trim_end_matches('"').to_string())
        .collect()
}

#[test]
fn no_crate_banned_in_deny_toml_is_in_the_lock_file() {
    let root = workspace_root();
    let bans = absolute_bans(&read(&root.join("deny.toml")));
    let locked = locked_names(&read(&root.join("Cargo.lock")));
    println!(
        "lock scan: {} bans, {} locked packages",
        bans.len(),
        locked.len()
    );

    // 15 bans and 524 package names when this was written.
    assert!(
        bans.len() >= 12,
        "read only {} bans from deny.toml",
        bans.len()
    );
    assert!(
        bans.iter().any(|b| b == "reqwest"),
        "the bans read from deny.toml do not include reqwest - the scan reads the wrong lines"
    );
    assert!(
        locked.len() >= 400,
        "read only {} names from Cargo.lock",
        locked.len()
    );
    let found: Vec<&String> = bans.iter().filter(|b| locked.contains(*b)).collect();
    assert!(
        found.is_empty(),
        "a crate that deny.toml bans because it speaks to a network is in Cargo.lock: {found:?}"
    );
}

// ---------------------------------------------------------------------------
// N4. No manifest enables a networking feature of windows-sys
// ---------------------------------------------------------------------------

/// Feature names that switch on a network API of `windows-sys` or `windows`.
const NETWORK_FEATURES: &[&str] = &[
    "\"Win32_Networking",
    "\"Win32_NetworkManagement",
    "\"Win32_Web",
    "\"Web_",
];

/// The networking features one manifest enables, comments left out.
fn network_features(manifest: &str) -> Vec<String> {
    manifest
        .lines()
        .map(|line| line.split('#').next().unwrap_or("").trim())
        .filter(|line| NETWORK_FEATURES.iter().any(|f| line.contains(f)))
        .map(str::to_string)
        .collect()
}

#[test]
fn no_manifest_enables_a_networking_feature() {
    let root = workspace_root();
    let mut manifests = vec![root.join("Cargo.toml")];
    for entry in std::fs::read_dir(root.join("crates")).expect("a crates folder") {
        let manifest = entry.expect("a directory entry").path().join("Cargo.toml");
        if manifest.is_file() {
            manifests.push(manifest);
        }
    }
    let mut offenders = Vec::new();
    let mut seen_features = false;
    for manifest in &manifests {
        let text = read(manifest);
        seen_features |= text.contains("\"Win32_UI_Input_KeyboardAndMouse\"");
        for line in network_features(&text) {
            offenders.push(format!("{}: {line}", relative(&root, manifest)));
        }
    }
    assert!(
        manifests.len() >= 7,
        "read only {} manifests",
        manifests.len()
    );
    assert!(
        seen_features,
        "no manifest held the feature list of windows-sys - the scan reads the wrong files"
    );
    assert!(
        offenders.is_empty(),
        "a networking feature of windows-sys was enabled. The features are enumerated one by one \
         precisely so that reaching Winsock or WinHTTP through windows-sys needs a line this \
         check can refuse: {offenders:?}"
    );
}

// ---------------------------------------------------------------------------
// N5. The registers describe code that exists
// ---------------------------------------------------------------------------

/// What is stale in the registers, given each named file's text (`None` when
/// the file is gone).
fn stale_entries(text_of: &dyn Fn(&str) -> Option<String>) -> Vec<String> {
    let mut stale = Vec::new();
    for (file, kind, _) in ALLOWED.iter().copied() {
        let Some(text) = text_of(file) else {
            stale.push(format!(
                "{file} is gone but still holds a {kind} permission"
            ));
            continue;
        };
        let lexed = lex(Syntax::Rust, &text);
        let code = code(&text, &lexed);
        let uses = WATCHED
            .iter()
            .any(|(name, k)| *k == kind && code.contains(name));
        if !uses {
            stale.push(format!("{file} no longer uses anything of kind '{kind}'"));
        }
    }
    for (file, _) in REFUSING.iter().copied() {
        let Some(text) = text_of(file) else {
            stale.push(format!(
                "{file} is gone but is still registered as refusing"
            ));
            continue;
        };
        let mut names = FORBIDDEN.iter().chain(FORBIDDEN_LIBRARIES);
        if !names.any(|(name, _)| text.contains(name)) {
            stale.push(format!("{file} no longer names anything it refuses"));
        }
    }
    stale
}

#[test]
fn every_registered_exception_still_names_live_code() {
    let root = workspace_root();
    let stale = stale_entries(&|file| std::fs::read_to_string(root.join(file)).ok());
    assert!(
        stale.is_empty(),
        "a permission outlived the code it was granted for. A register that keeps dead entries is \
         one nobody has read since the code moved: {stale:?}"
    );
}

// ---------------------------------------------------------------------------
// N6. The canary, in both directions
// ---------------------------------------------------------------------------

/// Code this scanner exists to reject, one case per shape, as a file of the
/// product. A canary that only tries the spelling the author had in mind
/// proves nothing about the ones they did not.
const BAD_RUST: &[(&str, &str, &str)] = &[
    (
        "the module path",
        "use std::net::ToSocketAddrs as _;",
        "forbidden",
    ),
    (
        "a TCP connection",
        "let s = TcpStream::connect(addr)?;",
        "forbidden",
    ),
    (
        "a datagram socket",
        "let u = UdpSocket::bind(addr)?;",
        "forbidden",
    ),
    (
        "a name lookup",
        "let a = host.to_socket_addrs()?;",
        "forbidden",
    ),
    (
        "an HTTP client crate",
        "let c = reqwest::blocking::Client::new();",
        "forbidden",
    ),
    (
        "WinHTTP",
        "let h = WinHttpOpen(agent, 0, none, none, 0);",
        "forbidden",
    ),
    (
        "Winsock by name, any case",
        "let m = LoadLibraryW(w!(\"WS2_32.dll\"));",
        "forbidden",
    ),
    (
        "Winsock linked by attribute",
        "#[link(name = \"ws2_32\")]",
        "forbidden",
    ),
    (
        "the networking module of windows-sys",
        "use windows_sys::Win32::Networking::WinSock::*;",
        "forbidden",
    ),
    (
        "the shell",
        "ShellExecuteW(0, verb, url, none, none, 1);",
        "forbidden",
    ),
    ("a browser crate", "webbrowser::open(url)?;", "forbidden"),
    (
        "Slint's opener from Rust",
        "slint::private_api::open_url(url, window);",
        "forbidden",
    ),
    (
        "the macOS HTTP client",
        "let s = NSURLSession::sharedSession();",
        "forbidden",
    ),
    (
        "the Linux opener, in a literal",
        "let c = run(\"xdg-open\", url);",
        "forbidden",
    ),
    (
        "a line that begins with a star",
        "*slot = TcpStream::connect(addr)?;",
        "forbidden",
    ),
    (
        "a name after an address in a literal",
        "let s = open_with(\"https://x\", TcpStream::connect)?;",
        "forbidden",
    ),
    (
        "a process",
        "Command::new(\"curl\").arg(url).spawn()?;",
        "unregistered",
    ),
    (
        "a process by its full path",
        "let c = std::process::Command::new(p);",
        "unregistered",
    ),
    (
        "a process through Windows",
        "CreateProcessW(app, line, none, none, 0, 0, none, none, si, pi);",
        "unregistered",
    ),
    (
        "a local socket",
        "let s = UnixStream::connect(path)?;",
        "unregistered",
    ),
];

/// The same for a view.
const BAD_SLINT: &[(&str, &str, &str)] = &[
    (
        "the opener in a view",
        "clicked => { Platform.open-url(link); }",
        "forbidden",
    ),
    (
        "an address in a view",
        "source: @image-url(\"https://example.invalid/a.png\");",
        "forbidden",
    ),
];

/// Kinds of finding the scanner reports for one source, as a product file.
fn kinds(syntax: Syntax, source: &str, rel: &str) -> BTreeSet<&'static str> {
    findings(syntax, source, rel)
        .iter()
        .map(|(k, _, _)| *k)
        .collect()
}

#[test]
fn the_scanner_catches_every_shape_it_exists_to_catch() {
    let mut missed = Vec::new();
    let cases = BAD_RUST
        .iter()
        .map(|c| (Syntax::Rust, "crates/nkb-gui/src/live.rs", c))
        .chain(
            BAD_SLINT
                .iter()
                .map(|c| (Syntax::Slint, "crates/nkb-gui/ui/screens/palette.slint", c)),
        );
    for (syntax, rel, (label, source, want)) in cases {
        let seen = kinds(syntax, source, rel);
        if !seen.contains(want) {
            missed.push(format!("{label} -> saw {seen:?}, wanted {want}"));
        }
    }
    assert!(
        missed.is_empty(),
        "the scanner missed a shape it exists to catch. A guard that has only ever seen clean code \
         has been shown to run, not to look: {missed:?}"
    );
}

#[test]
fn ordinary_code_and_prose_raise_no_finding() {
    // The negative half of the same claim. A scanner that flagged everything
    // would satisfy the canary above and be useless, and every spelling below
    // stands in this workspace today.
    let rust = "\
        // The palette never opens a TcpStream, and ws2_32 is not linked.\n\
        /// See `Command::new` in the tests, never here.\n\
        /* A block comment naming std::net and ShellExecuteW. */\n\
        let command = live::Command::Pause;\n\
        let worker = std::thread::Builder::new().spawn(move || run())?;\n\
        std::process::exit(4);\n\
        let id = std::process::id();\n\
        let code = std::process::ExitCode::from(2);\n\
        let source = \"https://example.invalid/the-pack\";\n\
        let connected = window.is_connected();\n";
    let found = findings(Syntax::Rust, rust, "crates/nkb-gui/src/live.rs");
    assert!(
        found.is_empty(),
        "ordinary code and prose raised a finding: {found:?}"
    );

    let view = "\
        // A view never calls Platform.open-url, nor names https://example.invalid.\n\
        import { Zone } from \"../components.slint\";\n\
        export component Example inherits Rectangle { in property <string> link; }\n";
    let found = findings(
        Syntax::Slint,
        view,
        "crates/nkb-gui/ui/screens/example.slint",
    );
    assert!(
        found.is_empty(),
        "an ordinary view raised a finding: {found:?}"
    );
}

#[test]
fn a_registered_file_may_use_what_it_was_granted_and_nothing_more() {
    // The register is per file AND per kind, which is the half most easily got
    // wrong. The help test may start `nkb`, and still may not open a socket.
    let help = "crates/nkb-cli/tests/help_lists_every_command.rs";
    assert!(
        findings(Syntax::Rust, "let run = Command::new(nkb).output();", help).is_empty(),
        "the registered test was refused the process it was granted"
    );
    assert_eq!(
        kinds(Syntax::Rust, "let s = UnixStream::connect(path);", help),
        BTreeSet::from(["unregistered"]),
        "a file granted a process was also allowed a socket, which the register does not say"
    );
}

#[test]
fn a_refusing_file_is_excused_its_literals_and_nothing_else() {
    let guard = "crates/nkb-cli/tests/imports/mod.rs";
    assert!(
        findings(
            Syntax::Rust,
            "const REFUSED: &[&str] = &[\"ws2_32.dll\", \"ShellExecute\"];",
            guard
        )
        .is_empty(),
        "a refusing file was reported for the names it refuses"
    );
    assert_eq!(
        kinds(Syntax::Rust, "let s = TcpStream::connect(addr);", guard),
        BTreeSet::from(["forbidden"]),
        "a refusing file was excused its CODE as well as its literals"
    );
    // The excuse goes by the whole path. A file of the product that happens to
    // share this guard's name is read like any other.
    let namesake = "crates/nkb-gui/src/nothing_reaches_the_network.rs";
    assert_eq!(
        kinds(Syntax::Rust, "const A: &str = \"ws2_32.dll\";", namesake),
        BTreeSet::from(["forbidden"]),
        "a file was excused because of its name rather than its path"
    );
}

#[test]
fn a_finding_names_the_line_it_stands_on() {
    let source =
        "fn a() {}\nconst B: &str = \"\\\n  ws2_32.dll\";\nlet c = TcpStream::connect(d);\n";
    let lines: Vec<usize> = findings(Syntax::Rust, source, "crates/nkb-gui/src/live.rs")
        .iter()
        .map(|(_, _, line)| *line)
        .collect();
    assert_eq!(
        lines,
        [4, 3],
        "a finding points at the wrong line: {lines:?}"
    );
}

#[test]
fn the_helpers_of_n2_n3_and_n4_see_what_they_exist_to_see() {
    assert_eq!(
        grants_outside_tests(
            ["crates/nkb-cli/tests/a.rs", "crates/nkb-gui/src/live.rs"].into_iter()
        ),
        ["crates/nkb-gui/src/live.rs"],
        "N2 did not tell a test from a file of the product"
    );

    let deny = "deny = [\n    { crate = \"slint\", wrappers = [\"nkb-gui\"] },\n    \
                # { crate = \"commented\" },\n    { crate = \"hyper\", reason = \"x\" },\n]\n";
    assert_eq!(
        absolute_bans(deny),
        ["hyper"],
        "N3 read the wrong entries of deny.toml"
    );
    let lock = "[[package]]\nname = \"hyper\"\nversion = \"1.0.0\"\n";
    assert!(
        locked_names(lock).contains("hyper"),
        "N3 did not read a lock file entry"
    );

    let manifest = "features = [\n    \"Win32_Foundation\",\n    \"Win32_Networking_WinSock\",\n]\n\
                    # \"Win32_Web\" is not here on purpose\n";
    assert_eq!(
        network_features(manifest),
        ["\"Win32_Networking_WinSock\","],
        "N4 missed a networking feature, or read one in a comment"
    );
}

#[test]
fn the_staleness_check_sees_a_dead_entry() {
    // Every registered file gone: every entry is stale.
    let gone = stale_entries(&|_| None);
    assert_eq!(
        gone.len(),
        ALLOWED.len() + REFUSING.len(),
        "a missing file was not reported: {gone:?}"
    );
    // Every registered file present but empty: still every entry is stale.
    let empty = stale_entries(&|_| Some(String::new()));
    assert_eq!(
        empty.len(),
        ALLOWED.len() + REFUSING.len(),
        "an empty file was not reported: {empty:?}"
    );
}
