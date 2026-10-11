// Puts the file's details into `nkb.exe` on Windows - what the program is,
// which version, whose, under which licence - so the file manager's
// Properties and the task manager can say it. The work is in
// `file_details.rs`, shared with the palette's build script so both programs
// describe themselves one way. Nothing is linked into `nkb`: the resource is
// data, and `nkb` has no window, so it carries no icon.

mod file_details;

fn main() {
    // Once a script names a file, only the named ones make it run again. The
    // words live in the two manifests - the version, the author, the address
    // are inherited from the workspace's - so both are named.
    println!("cargo:rerun-if-changed=file_details.rs");
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=../../Cargo.toml");
    file_details::describe(
        &file_details::Program {
            name: "nkb",
            description: "Naughty Keyboard command line tool",
        },
        None,
    );
}
