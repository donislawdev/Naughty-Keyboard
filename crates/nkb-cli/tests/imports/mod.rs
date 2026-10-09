//! What a built program links, read from its own import table: the binary half
//! of the guard that the tool opens no network connection.
//!
//! `crates/nkb-core/tests/nothing_reaches_the_network.rs` reads our source.
//! This reads what the LINKER put into `nkb` and `nkb-gui`: the libraries the
//! loader resolves before the program runs, and the functions the program takes
//! from them. A dependency that links Winsock, or starts a browser, shows up
//! here whether or not a line of ours spells it - and with 500 packages in the
//! lock file, most of them Slint's, that is where it would come from.
//!
//! Shared by `links_nothing_off_the_machine.rs` in `nkb-cli` and the file of the
//! same name in `nkb-gui`, because a package's own integration tests are the
//! only place Cargo hands over the path of the binary it has just built
//! (`CARGO_BIN_EXE_*`). The second includes this one with a `#[path]`, and the
//! checks of the registers below run in both.
//!
//! # Two registers
//!
//! **Refused** libraries and functions have no place in either program: the
//! network libraries of each system, starting a program, handing an address to
//! the shell, a socket. A refused LIBRARY may never stand in an expected list,
//! and a test below holds that.
//!
//! **Expected** is the closed list of libraries each program links on each
//! system, each with its reason, plus the few refused functions one program is
//! granted anyway, each with its reason. A library outside the list fails, and
//! so does an entry the program no longer needs: a register that keeps dead
//! entries is one nobody has read since the code moved.
//!
//! The lists are what was measured on 2026-10-09: Windows on this machine,
//! debug and release alike, macOS and Linux from the builds there. A new Slint
//! may well change them. That is a red test and an entry with a reason, which
//! is the point.
//!
//! # What this cannot see, said plainly
//!
//! - **A library loaded by name while the program runs.** Both programs import
//!   `LoadLibrary` and `GetProcAddress` on Windows (the standard library's
//!   shims, the OpenGL loader), and on Linux `nkb-gui` imports `dlopen`,
//!   because Slint finds X11, Wayland and xkbcommon that way. A name handed to
//!   them is a string, and `nothing_reaches_the_network.rs` is what reads
//!   strings.
//! - **A system call made without the C library**, which on Linux is one
//!   instruction. Nothing in this workspace does it, and the source scan
//!   refuses the system call numbers that would.
//! - **The release build.** This reads the build the test run made. On Windows
//!   the release build links the same libraries (measured 2026-10-09), and
//!   nothing here reads it.

use object::read::{BinaryFormat, File, NameOrOrdinal, Object};
use std::collections::BTreeSet;

/// The two programs this repository ships.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Program {
    Cli,
    Gui,
}

impl Program {
    fn name(self) -> &'static str {
        match self {
            Self::Cli => "nkb",
            Self::Gui => "nkb-gui",
        }
    }
}

/// A library or a function, and why. A name ending in `*` is a prefix.
type Entry = (&'static str, &'static str);

/// What one built program links: library names as the loader reads them, in
/// lower case on Windows where the loader ignores case, and the names of the
/// functions it imports, without the underscore Mach-O puts in front.
#[derive(Debug, Default, Clone)]
pub struct Linked {
    pub libraries: BTreeSet<String>,
    pub symbols: BTreeSet<String>,
}

fn matches(pattern: &str, name: &str) -> bool {
    match pattern.strip_suffix('*') {
        Some(prefix) => name.starts_with(prefix),
        None => name == pattern,
    }
}

// ---------------------------------------------------------------------------
// Register 1: refused in either program
// ---------------------------------------------------------------------------

const REFUSED_LIBRARIES: &[Entry] = &[
    ("ws2_32.dll", "Winsock"),
    ("wsock32.dll", "the old Winsock"),
    ("mswsock.dll", "the Winsock provider"),
    ("winhttp.dll", "WinHTTP, the Windows HTTP stack"),
    ("wininet.dll", "WinINet, the other Windows HTTP stack"),
    ("urlmon.dll", "the URL moniker library, a downloader"),
    ("dnsapi.dll", "DNS"),
    ("iphlpapi.dll", "the IP helper library"),
    ("websocket.dll", "the Windows WebSocket library"),
    ("httpapi.dll", "the Windows HTTP server"),
    ("libcurl*", "libcurl"),
    (
        "libssl*",
        "a TLS stack, which only a program that connects somewhere needs",
    ),
    (
        "libgnutls*",
        "a TLS stack, which only a program that connects somewhere needs",
    ),
    ("libresolv*", "the DNS resolver"),
    ("libsoup*", "an HTTP library"),
    (
        "/System/Library/Frameworks/CFNetwork.framework*",
        "the macOS network framework",
    ),
    (
        "/System/Library/Frameworks/Network.framework*",
        "Network.framework",
    ),
    (
        "/System/Library/Frameworks/WebKit.framework*",
        "a web engine",
    ),
];

const REFUSED_SYMBOLS: &[Entry] = &[
    (
        "ShellExecute*",
        "hands an address or a program to the shell",
    ),
    ("WinExec", "starts a program"),
    ("CreateProcess*", "starts a program"),
    (
        "NtDeviceIoControlFile",
        "speaks to a driver directly, which is how a socket is reached beneath Winsock",
    ),
    (
        "ZwDeviceIoControlFile",
        "speaks to a driver directly, which is how a socket is reached beneath Winsock",
    ),
    (
        "DeviceIoControl",
        "speaks to a driver directly, which is how a socket is reached beneath Winsock",
    ),
    (
        "CoCreateInstanceEx",
        "a COM object that may live on another machine",
    ),
    (
        "RoGetActivationFactory",
        "WinRT activation, the way to the Windows HTTP classes",
    ),
    (
        "RoActivateInstance",
        "WinRT activation, the way to the Windows HTTP classes",
    ),
    ("socket", "a socket"),
    ("socketpair", "a pair of connected sockets"),
    ("connect", "a connection"),
    ("getaddrinfo", "a name lookup"),
    ("gethostbyname*", "a name lookup"),
    ("bind", "a socket given an address"),
    ("listen", "a server"),
    ("accept", "a server"),
    ("accept4", "a server"),
    ("sendto", "a datagram sent to an address"),
    ("fork", "a second process"),
    ("vfork", "a second process"),
    ("execv*", "a program started"),
    ("execl*", "a program started"),
    ("posix_spawn", "a program started"),
    ("posix_spawnp", "a program started"),
    ("system", "a program started through the shell"),
    ("popen", "a program started through the shell"),
    (
        "LSOpen*",
        "Launch Services, which opens an address or a program",
    ),
    ("CFSocket*", "a macOS socket"),
    ("CFStreamCreatePairWithSocket*", "a macOS socket"),
    ("nw_*", "Network.framework"),
    ("OBJC_CLASS_$_NSURLSession", "the macOS HTTP client"),
    (
        "OBJC_CLASS_$_NSURLConnection",
        "the older macOS HTTP client",
    ),
    (
        "OBJC_CLASS_$_NSWorkspace",
        "opens an address or a program on macOS",
    ),
];

// ---------------------------------------------------------------------------
// Register 2: what each program links, on each system
// ---------------------------------------------------------------------------

const WINDOWS_BOTH: &[Entry] = &[
    (
        "kernel32.dll",
        "the system itself: files, threads, memory, the console",
    ),
    (
        "ntdll.dll",
        "beneath kernel32, where the standard library reads and writes files and pipes",
    ),
    (
        "user32.dll",
        "the window in front, the keyboard, SendInput and the global shortcuts",
    ),
    (
        "advapi32.dll",
        "the integrity level of the window in front, read from its token (D72)",
    ),
    (
        "ole32.dll",
        "COM, through which UI Automation tells which kind of control has the focus (D73)",
    ),
    (
        "oleaut32.dll",
        "the strings and variants UI Automation answers in",
    ),
    (
        "bcryptprimitives.dll",
        "ProcessPrng, the random seed of the standard library's hash maps",
    ),
    (
        "api-ms-win-core-synch-l1-2-0.dll",
        "WaitOnAddress, which the standard library's locks wait on",
    ),
    // Absent on purpose: vcruntime140.dll and the api-ms-win-crt-* sets of the
    // Universal C runtime. The C runtime is linked into the programs
    // (.cargo/config.toml), because vcruntime140.dll comes with the Visual C++
    // Redistributable and not with Windows, and a program that imports it does
    // not start on a machine without it. This list is closed, so a build that
    // loads the runtime again fails here. Do not add them back to make it pass.
];

const WINDOWS_WINDOW: &[Entry] = &[
    (
        "gdi32.dll",
        "device contexts, bitmaps and the pixel format of the OpenGL surface",
    ),
    ("opengl32.dll", "the OpenGL renderer Slint can draw with"),
    (
        "dwmapi.dll",
        "the desktop compositor: window attributes and the accent colour",
    ),
    (
        "uxtheme.dll",
        "SetWindowTheme, the system look of the window frame",
    ),
    ("dwrite.dll", "DirectWrite, the system's text layout"),
    (
        "imm32.dll",
        "the input method editor, for text composed before it is typed",
    ),
    (
        "comctl32.dll",
        "window subclassing, which keeps the palette from taking the focus (D62)",
    ),
    (
        "uiautomationcore.dll",
        "the windows' own accessibility tree, which screen readers read",
    ),
    (
        "combase.dll",
        "CoCreateFreeThreadedMarshaler, for the COM objects of that tree",
    ),
    (
        "api-ms-win-core-winrt-error-l1-1-0.dll",
        "RoOriginateErrorW, how the windows crate reports a COM error",
    ),
    (
        "shell32.dll",
        "DragQueryFileW and DragFinish, a file dropped on a window. ShellExecute is refused above",
    ),
    (
        "shlwapi.dll",
        "AssocQueryStringW, which the webbrowser crate asks for the default browser - see CreateProcessW",
    ),
];

const WINDOWS_WINDOW_GRANTED: &[Entry] = &[(
    "CreateProcessW",
    "the webbrowser crate starts the default browser when Slint is asked to open an address. Slint \
     keeps it linked because the call sits in the platform's table of methods, not because \
     anything makes it: nothing_reaches_the_network.rs refuses `open-url` in every view and \
     `open_url` in every Rust file, and those are the only ways to ask",
)];

const LINUX_BOTH: &[Entry] = &[
    (
        "libc.so.6",
        "the C library: files, memory, threads and the system calls",
    ),
    (
        "libgcc_s.so.1",
        "unwinding, which the standard library's panics use",
    ),
    (
        "ld-linux*",
        "the dynamic loader, whose name carries the architecture",
    ),
];

const LINUX_WINDOW: &[Entry] = &[
    ("libm.so.6", "mathematics for layout and drawing"),
    ("libfontconfig.so.1", "the system's font lookup"),
];

const LINUX_WINDOW_GRANTED: &[Entry] = &[
    (
        "socket",
        "Slint reaches the X server named by DISPLAY and the session bus (accessibility, the tray). \
         Both are sockets, and both go where the desktop points, never to an address of ours",
    ),
    (
        "socketpair",
        "the session bus client, a pair of local sockets between its own threads",
    ),
    (
        "connect",
        "the same X server and session bus. X11 goes over TCP when DISPLAY names a host, which is \
         the tester's own configuration",
    ),
    (
        "getaddrinfo",
        "the host part of DISPLAY, resolved by the X11 client when there is one",
    ),
    (
        "execvp",
        "the standard library's process start, which Slint's dependencies link: the webbrowser \
         crate (behind `open-url`, refused in every view) and the session bus client",
    ),
    ("fork", "the same process start"),
    ("posix_spawnp", "the same process start"),
];

const MACOS_BOTH: &[Entry] = &[(
    "/usr/lib/libSystem.B.dylib",
    "the C library and the system calls. macOS has no other way in",
)];

const MACOS_WINDOW: &[Entry] = &[
    (
        "/usr/lib/libobjc.A.dylib",
        "the Objective-C runtime the window toolkit speaks to AppKit through",
    ),
    (
        "/usr/lib/libiconv.2.dylib",
        "conversion between character sets",
    ),
    (
        "/System/Library/Frameworks/AppKit.framework/Versions/C/AppKit",
        "windows, menus and the event loop",
    ),
    (
        "/System/Library/Frameworks/Foundation.framework/Versions/C/Foundation",
        "the Objective-C base classes",
    ),
    (
        "/System/Library/Frameworks/CoreFoundation.framework/Versions/A/CoreFoundation",
        "the C base types under Foundation",
    ),
    (
        "/System/Library/Frameworks/CoreGraphics.framework/Versions/A/CoreGraphics",
        "drawing and the geometry of displays",
    ),
    (
        "/System/Library/Frameworks/QuartzCore.framework/Versions/A/QuartzCore",
        "the layer a window draws into",
    ),
    (
        "/System/Library/Frameworks/CoreVideo.framework/Versions/A/CoreVideo",
        "the display link that paces frames",
    ),
    (
        "/System/Library/Frameworks/OpenGL.framework/Versions/A/OpenGL",
        "the OpenGL renderer",
    ),
    (
        "/System/Library/Frameworks/CoreText.framework/Versions/A/CoreText",
        "the system's text and font lookup",
    ),
    (
        "/System/Library/Frameworks/ColorSync.framework/Versions/A/ColorSync",
        "the colour profile of a display",
    ),
    (
        "/System/Library/Frameworks/ApplicationServices.framework/Versions/A/ApplicationServices",
        "the umbrella over the frameworks above",
    ),
    (
        "/System/Library/Frameworks/Carbon.framework/Versions/A/Carbon",
        "the keyboard layout, read through Text Input Sources",
    ),
    (
        "/System/Library/Frameworks/CoreServices.framework/Versions/A/CoreServices",
        "the umbrella over Launch Services and the file system services. Launch Services opens \
         addresses, and its LSOpen calls are refused above",
    ),
];

/// The expected libraries of one program on one system, and the refused
/// functions it is granted. `None` for a system this repository does not build.
fn expected(program: Program, os: &str) -> Option<(Vec<Entry>, Vec<Entry>)> {
    let (both, window, granted): (&[Entry], &[Entry], &[Entry]) = match os {
        "windows" => (WINDOWS_BOTH, WINDOWS_WINDOW, WINDOWS_WINDOW_GRANTED),
        "linux" => (LINUX_BOTH, LINUX_WINDOW, LINUX_WINDOW_GRANTED),
        "macos" => (MACOS_BOTH, MACOS_WINDOW, &[]),
        _ => return None,
    };
    let mut libraries = both.to_vec();
    if program == Program::Gui {
        libraries.extend_from_slice(window);
        return Some((libraries, granted.to_vec()));
    }
    Some((libraries, Vec::new()))
}

// ---------------------------------------------------------------------------
// Reading a program, and judging what it links
// ---------------------------------------------------------------------------

/// The libraries and functions one built program imports. Any part of the
/// file the reader cannot parse fails the test: a table read halfway would
/// look exactly like a program that links less.
pub fn read(path: &str) -> Linked {
    let data = std::fs::read(path).unwrap_or_else(|e| panic!("{path} must be readable: {e}"));
    let file = File::parse(&*data).unwrap_or_else(|e| panic!("{path} is not a program: {e}"));
    let format = file.format();
    let mut linked = Linked::default();
    let libraries = file.import_libraries().expect("the library table is read");
    for library in libraries {
        let library = library.expect("a library entry is read");
        let name = String::from_utf8_lossy(library.name()).into_owned();
        let name = match format {
            BinaryFormat::Pe => name.to_ascii_lowercase(),
            _ => name,
        };
        linked.libraries.insert(name);
    }
    for import in file.imports().expect("the import table is read") {
        let import = import.expect("an import entry is read");
        if let NameOrOrdinal::Name(name) = import.name() {
            let name = String::from_utf8_lossy(name);
            let name = match format {
                BinaryFormat::MachO => name.strip_prefix('_').unwrap_or(&name).to_string(),
                _ => name.into_owned(),
            };
            linked.symbols.insert(name);
        }
    }
    linked
}

/// What is wrong with what one program links, on one system.
fn problems(program: Program, os: &str, linked: &Linked) -> Vec<String> {
    let name = program.name();
    let Some((libraries, granted)) = expected(program, os) else {
        return vec![format!("there is no register for {name} on {os}")];
    };
    let mut out = Vec::new();
    for library in &linked.libraries {
        if let Some((_, why)) = REFUSED_LIBRARIES.iter().find(|(p, _)| matches(p, library)) {
            out.push(format!("{name} links {library}: {why}"));
        } else if !libraries.iter().any(|(p, _)| matches(p, library)) {
            out.push(format!(
                "{name} links {library}, which the register does not name"
            ));
        }
    }
    for (pattern, _) in &libraries {
        if !linked.libraries.iter().any(|l| matches(pattern, l)) {
            out.push(format!(
                "the register names {pattern}, which {name} no longer links"
            ));
        }
    }
    for symbol in &linked.symbols {
        let refused = REFUSED_SYMBOLS.iter().find(|(p, _)| matches(p, symbol));
        let excused = granted.iter().any(|(p, _)| matches(p, symbol));
        if let (Some((_, why)), false) = (refused, excused) {
            out.push(format!("{name} imports {symbol}: {why}"));
        }
    }
    for (pattern, _) in &granted {
        if !linked.symbols.iter().any(|s| matches(pattern, s)) {
            out.push(format!(
                "{name} is granted {pattern}, which it no longer imports"
            ));
        }
    }
    out
}

/// The library every program links on this system, to prove the reader read.
fn base_library(os: &str) -> &'static str {
    match os {
        "windows" => "kernel32.dll",
        "linux" => "libc.so.6",
        _ => "/usr/lib/libSystem.B.dylib",
    }
}

/// The check each package runs on the program it built.
pub fn assert_links_nothing_off_the_machine(program: Program, path: &str) {
    let os = std::env::consts::OS;
    let linked = read(path);
    println!(
        "{}: {} libraries, {} imported functions",
        program.name(),
        linked.libraries.len(),
        linked.symbols.len()
    );

    // A reader that read nothing finds nothing, and looks exactly like a
    // program that links nothing it should not.
    assert!(
        linked.libraries.contains(base_library(os)),
        "the import table of {path} does not hold {} - the reader read the wrong thing: {:?}",
        base_library(os),
        linked.libraries
    );
    assert!(
        linked.symbols.len() >= 40,
        "the import table of {path} holds only {} functions",
        linked.symbols.len()
    );
    let found = problems(program, os, &linked);
    assert!(
        found.is_empty(),
        "the tool promises it opens no network connection, and the built program says otherwise, \
         or the register in nkb-cli/tests/imports/mod.rs is out of date. A library or function \
         added on purpose needs an entry with its reason: {found:#?}"
    );
}

// ---------------------------------------------------------------------------
// The registers themselves, and the canary
// ---------------------------------------------------------------------------

const SYSTEMS: [&str; 3] = ["windows", "linux", "macos"];
const PROGRAMS: [Program; 2] = [Program::Cli, Program::Gui];

#[test]
fn no_register_expects_a_refused_library_or_grants_what_is_not_refused() {
    let mut wrong = Vec::new();
    for os in SYSTEMS {
        for program in PROGRAMS {
            let (libraries, granted) = expected(program, os).expect("a register for each system");
            for (library, _) in &libraries {
                let probe = library.trim_end_matches('*');
                if REFUSED_LIBRARIES.iter().any(|(p, _)| matches(p, probe)) {
                    wrong.push(format!(
                        "{} on {os} expects the refused {library}",
                        program.name()
                    ));
                }
            }
            for (symbol, _) in &granted {
                if !REFUSED_SYMBOLS.iter().any(|(p, _)| matches(p, symbol)) {
                    wrong.push(format!(
                        "{} on {os} is granted {symbol}, which nothing refuses",
                        program.name()
                    ));
                }
            }
        }
    }
    assert!(
        wrong.is_empty(),
        "the expected lists may never hold a refused library, and a grant only means something \
         for a function the refused list names: {wrong:?}"
    );
}

/// One thing a program could link that it should not, named for the message.
type Change = (&'static str, fn(&mut Linked));

/// A program that links exactly its register, granted functions included.
fn exactly_the_register(program: Program, os: &str) -> Linked {
    let (libraries, granted) = expected(program, os).expect("a register");
    let mut linked = Linked::default();
    for (library, _) in libraries {
        linked.libraries.insert(library.replace('*', "x"));
    }
    for (symbol, _) in granted {
        linked.symbols.insert(symbol.to_string());
    }
    linked.symbols.insert(String::from("ExitProcess"));
    linked
}

#[test]
fn the_judgement_refuses_what_it_exists_to_refuse() {
    for os in SYSTEMS {
        for program in PROGRAMS {
            let clean = exactly_the_register(program, os);
            let found = problems(program, os, &clean);
            assert!(
                found.is_empty(),
                "{} on {os}, linking exactly its register: {found:?}",
                program.name()
            );

            let cases: [Change; 5] = [
                ("Winsock", |l| {
                    l.libraries.insert(String::from("ws2_32.dll"));
                }),
                ("a library nobody registered", |l| {
                    l.libraries.insert(String::from("unheard-of.dll"));
                }),
                ("a registered library gone", |l| {
                    l.libraries.pop_first();
                }),
                ("the shell", |l| {
                    l.symbols.insert(String::from("ShellExecuteW"));
                }),
                ("a server", |l| {
                    l.symbols.insert(String::from("listen"));
                }),
            ];
            for (label, change) in cases {
                let mut linked = clean.clone();
                change(&mut linked);
                assert!(
                    !problems(program, os, &linked).is_empty(),
                    "{} on {os}: {label} went unreported",
                    program.name()
                );
            }
        }
    }
}

#[test]
fn a_grant_belongs_to_one_program_on_one_system() {
    // The window may start the browser Slint links on Windows. The command line
    // tool may not, and the grant is not a hole the size of `CreateProcess*`.
    let mut cli = exactly_the_register(Program::Cli, "windows");
    cli.symbols.insert(String::from("CreateProcessW"));
    assert!(
        !problems(Program::Cli, "windows", &cli).is_empty(),
        "nkb was allowed to start a program because nkb-gui is"
    );
    let mut gui = exactly_the_register(Program::Gui, "windows");
    gui.symbols.insert(String::from("CreateProcessAsUserW"));
    assert!(
        !problems(Program::Gui, "windows", &gui).is_empty(),
        "the grant of CreateProcessW let a different call through"
    );
    let mut gone = exactly_the_register(Program::Gui, "windows");
    gone.symbols.remove("CreateProcessW");
    assert!(
        !problems(Program::Gui, "windows", &gone).is_empty(),
        "a grant outlived the import it was given for"
    );
}

#[test]
fn a_system_without_a_register_is_a_finding_not_a_pass() {
    let found = problems(Program::Cli, "freebsd", &Linked::default());
    assert_eq!(
        found.len(),
        1,
        "an unknown system passed in silence: {found:?}"
    );
}
