// The details a file manager shows for a program of ours on Windows: what it
// is, which version, whose, and under which licence. Shared by the build
// scripts of `nkb` and `nkb-gui` - `crates/nkb-gui/build.rs` includes this file
// with a `#[path]` - so the two programs describe themselves one way.
//
// # Where the words come from
//
// The version, the author, the licence and the address are read from the
// package manifest (`[workspace.package]`), so a release that raises the
// version changes the files without a line here changing. What no manifest
// holds - the product's name and the year the copyright names - stands below,
// once. The year also reaches the code of the package being built, where the
// palette's About window says it (`NKB_FIRST_YEAR`), and
// `crates/nkb-gui/tests/file_details.rs` holds the window and the file to each
// other.
//
// # Why a resource script is written rather than macros passed
//
// The resource compiler takes `/D NAME=value`, but a string value has to reach
// it with its quotes, through the quoting rules of the command line and of the
// compiler - and the GNU compiler has rules of its own. A script written here,
// with every value checked for the characters a resource string cannot hold as
// they are, reaches both compilers as a file.
//
// Nothing is written for any other target. A file manager there reads the
// `.app` bundle the release builds (`release.py`), or nothing at all.

// A build script that cannot describe the program has nothing to fall back to,
// and the message is the whole point. Nothing here runs on a user's machine.
#![allow(clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

/// The product both programs belong to.
const PRODUCT: &str = "Naughty Keyboard";
/// The year the program was first published, which the copyright names.
const FIRST_YEAR: &str = "2026";
/// The name of the version block a script includes.
const BLOCK: &str = "details.rc";

/// One of the two programs, as its file describes it.
pub struct Program {
    /// The file's name without `.exe`: `nkb`, `nkb-gui`.
    pub name: &'static str,
    /// What the task manager calls the running program.
    pub description: &'static str,
}

/// Describes `program` in its file: writes the version block and compiles
/// `script`, which includes it, or the block alone when there is no script.
///
/// The year of the copyright goes to the package's code on every target, for
/// the About window. The rest happens on Windows only.
///
/// ⚠️ A Windows build that cannot compile the resource fails, rather than
/// building a program that says nothing about itself - the reason
/// `crates/nkb-gui/build.rs` gives for its icon.
pub fn describe(program: &Program, script: Option<&Path>) {
    println!("cargo:rustc-env=NKB_FIRST_YEAR={FIRST_YEAR}");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("Cargo names the output directory"));
    let block = out.join(BLOCK);
    std::fs::write(&block, version_block(program)).expect("the version block must be written");
    let compiled = match script {
        Some(script) => embed_resource::compile_for(
            script,
            [program.name],
            embed_resource::ParamsIncludeDirs([&out]),
        ),
        None => embed_resource::compile_for(&block, [program.name], embed_resource::NONE),
    };
    compiled
        .manifest_required()
        .expect("the file's details must be embedded in the program");
}

/// The resource script of the version block, from the manifest's words.
///
/// ⚠️ It names its code page, which holds when the block is compiled on its
/// own, as for `nkb`. A script that includes it has to name the code page
/// itself, before the include - see `crates/nkb-gui/assets/nkb-gui.rc`.
fn version_block(program: &Program) -> String {
    let version = manifest("CARGO_PKG_VERSION");
    let author = manifest("CARGO_PKG_AUTHORS").replace(':', ", ");
    let numbers = version_numbers();
    let strings = [
        ("CompanyName", author.clone()),
        ("FileDescription", program.description.to_owned()),
        ("FileVersion", version.clone()),
        ("InternalName", program.name.to_owned()),
        (
            "LegalCopyright",
            format!(
                "Copyright \u{a9} {FIRST_YEAR} {author}. {}",
                manifest("CARGO_PKG_LICENSE")
            ),
        ),
        ("OriginalFilename", format!("{}.exe", program.name)),
        ("ProductName", PRODUCT.to_owned()),
        ("ProductVersion", version),
        ("Comments", manifest("CARGO_PKG_HOMEPAGE")),
    ];
    let values: String = strings
        .iter()
        .map(|(key, value)| format!("      VALUE \"{key}\", \"{}\"\n", resource_string(value)))
        .collect();
    format!(
        "// Written by file_details.rs for {name}. Edits here are lost on the next build.\n\
         #pragma code_page(65001)\n\
         1 VERSIONINFO\n\
         FILEVERSION {numbers}\n\
         PRODUCTVERSION {numbers}\n\
         FILEFLAGSMASK 0x3F\n\
         FILEFLAGS {flags:#X}\n\
         FILEOS 0x40004\n\
         FILETYPE 0x1\n\
         FILESUBTYPE 0x0\n\
         BEGIN\n\
         \x20 BLOCK \"StringFileInfo\"\n\
         \x20 BEGIN\n\
         \x20   BLOCK \"040904B0\"\n\
         \x20   BEGIN\n\
         {values}\
         \x20   END\n\
         \x20 END\n\
         \x20 BLOCK \"VarFileInfo\"\n\
         \x20 BEGIN\n\
         \x20   VALUE \"Translation\", 0x409, 1200\n\
         \x20 END\n\
         END\n",
        name = program.name,
        flags = flags(),
    )
}

/// The version as the four numbers of the block - the fourth is always 0,
/// because a version of ours has three.
fn version_numbers() -> String {
    let number = |key: &str| -> u16 {
        manifest(key)
            .parse()
            .unwrap_or_else(|_| panic!("{key} must fit the 16 bits of a version block"))
    };
    format!(
        "{},{},{},0",
        number("CARGO_PKG_VERSION_MAJOR"),
        number("CARGO_PKG_VERSION_MINOR"),
        number("CARGO_PKG_VERSION_PATCH")
    )
}

/// `VS_FF_PRERELEASE` for a version with a pre-release part, `VS_FF_DEBUG`
/// for a build without optimisation - so the file says what it is.
fn flags() -> u32 {
    let prerelease = !manifest("CARGO_PKG_VERSION_PRE").is_empty();
    let debug = std::env::var("PROFILE").as_deref() == Ok("debug");
    u32::from(debug) | (u32::from(prerelease) << 1)
}

/// What Cargo says about the package. An empty field is an empty string, and
/// one the block cannot do without is refused by the test of the built file.
fn manifest(key: &str) -> String {
    std::env::var(key).unwrap_or_default()
}

/// `value` as it may stand between the quotes of a resource string. A quote,
/// a backslash or a control character would change what the compiler reads,
/// and none belongs in these words - refused, never escaped.
fn resource_string(value: &str) -> &str {
    assert!(
        !value
            .chars()
            .any(|c| c == '"' || c == '\\' || c.is_control()),
        "a file detail holds a character a resource string cannot: {value:?}"
    );
    value
}
