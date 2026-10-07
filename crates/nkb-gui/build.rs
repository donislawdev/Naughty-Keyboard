// Compiles the interface, and puts the program's icon into the executable. The
// first is one call, one entry point - see ui/all.slint for why a second call
// would silently discard the first.

// A build script that cannot build has nothing to fall back to, and the message
// is the whole point: it names the file and the line the Slint compiler
// rejected. The workspace bans `expect` in product code because a panic there
// lands in someone else's CI. Nothing this script does ever runs on a user's
// machine.
#![allow(clippy::expect_used)]

fn main() {
    slint_build::compile("ui/all.slint").expect("the interface must compile");
    embed_icon();
}

/// The icon a file manager, a desktop shortcut and the taskbar show for
/// `nkb-gui.exe`.
///
/// It lives inside the executable and nowhere beside it, so on Windows this
/// compiles `assets/nkb-gui.rc` and links the result into `nkb-gui` alone - the
/// library and the tests do not carry it. On every other target the crate does
/// nothing, which is the right answer there: the window's own icon
/// (`Tokens.app-icon`) is drawn by Slint on all of them.
///
/// ⚠️ A Windows build that cannot compile the resource fails instead of building
/// a program without its icon. `manifest_required` is what asks for that: with
/// `manifest_optional` a machine with no resource compiler would build quietly
/// and the missing icon would be noticed on someone else's desktop. The
/// resource compiler ships with the Windows SDK, which an MSVC toolchain needs
/// for linking anyway.
fn embed_icon() {
    // Cargo does not know this script reads these two files. Listing them is not
    // free: once a script names any file, only the named ones make it run again,
    // and the interface's own files are named by `slint_build::compile`.
    println!("cargo:rerun-if-changed=assets/nkb-gui.rc");
    println!("cargo:rerun-if-changed=assets/edamame.ico");
    embed_resource::compile_for("assets/nkb-gui.rc", ["nkb-gui"], embed_resource::NONE)
        .manifest_required()
        .expect("the icon must be embedded in nkb-gui.exe");
}
