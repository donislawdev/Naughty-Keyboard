// Puts the file's details and the program's icon into `nkb.exe` on Windows -
// what the program is, which version, whose, under which licence, and the
// palette's icon - so the file manager, a terminal's tab and the task manager
// can show them. The work is in `file_details.rs`, shared with the palette's
// build script so both programs describe themselves one way, and the script it
// compiles is `assets/nkb.rc`. Nothing is linked into `nkb` but the resource:
// it is data, and the icon file is only read here.

mod file_details;

fn main() {
    // Once a script names a file, only the named ones make it run again. The
    // words live in the two manifests - the version, the author, the address
    // are inherited from the workspace's - so both are named, and so is the
    // icon the resource script reaches for in the palette's folder.
    println!("cargo:rerun-if-changed=file_details.rs");
    println!("cargo:rerun-if-changed=assets/nkb.rc");
    println!("cargo:rerun-if-changed=../nkb-gui/assets/edamame.ico");
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=../../Cargo.toml");
    file_details::describe(
        &file_details::Program {
            name: "nkb",
            description: "Naughty Keyboard command line tool",
        },
        std::path::Path::new("assets/nkb.rc"),
    );
}
