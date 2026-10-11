// Compiles the interface, and puts the program's icon and the file's details
// into the executable. The first is one call, one entry point - see
// ui/all.slint for why a second call would silently discard the first.

// A build script that cannot build has nothing to fall back to, and the message
// is the whole point: it names the file and the line the Slint compiler
// rejected. The workspace bans `expect` in product code because a panic there
// lands in someone else's CI. Nothing this script does ever runs on a user's
// machine.
#![allow(clippy::expect_used)]

// Shared with the build script of `nkb`, so both programs describe themselves
// one way - why it lives there: the module's own header.
#[path = "../nkb-cli/file_details.rs"]
mod file_details;

fn main() {
    slint_build::compile("ui/all.slint").expect("the interface must compile");
    embed_resources();
}

/// The icon a file manager, a desktop shortcut and the taskbar show for
/// `nkb-gui.exe`, and the details its Properties and the task manager read -
/// what the program is, which version, whose, under which licence.
///
/// Both live inside the executable and nowhere beside it, so on Windows this
/// compiles `assets/nkb-gui.rc` - which includes the version block
/// `file_details.rs` writes - and links the result into `nkb-gui` alone: the
/// library and the tests do not carry it. On every other target nothing is
/// compiled, which is the right answer there: the window's own icon
/// (`Tokens.app-icon`) is drawn by Slint on all of them, and the year the
/// About window names reaches the code on all of them.
///
/// ⚠️ A Windows build that cannot compile the resource fails instead of building
/// a program without its icon. `manifest_required` is what asks for that: with
/// `manifest_optional` a machine with no resource compiler would build quietly
/// and the missing icon would be noticed on someone else's desktop. The
/// resource compiler ships with the Windows SDK, which an MSVC toolchain needs
/// for linking anyway.
fn embed_resources() {
    // Cargo does not know this script reads these files. Listing them is not
    // free: once a script names any file, only the named ones make it run
    // again, and the interface's own files are named by `slint_build::compile`.
    // The details' words live in the two manifests.
    println!("cargo:rerun-if-changed=assets/nkb-gui.rc");
    println!("cargo:rerun-if-changed=assets/edamame.ico");
    println!("cargo:rerun-if-changed=../nkb-cli/file_details.rs");
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=../../Cargo.toml");
    file_details::describe(
        &file_details::Program {
            name: "nkb-gui",
            description: "Naughty Keyboard",
        },
        Some(std::path::Path::new("assets/nkb-gui.rc")),
    );
}
