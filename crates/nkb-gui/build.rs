// Compiles the interface. One call, one entry point - see ui/all.slint for why
// a second call would silently discard the first.

// A build script that cannot build has nothing to fall back to, and the message
// is the whole point: it names the file and the line the Slint compiler
// rejected. The workspace bans `expect` in product code because a panic there
// lands in someone else's CI. Nothing this script does ever runs on a user's
// machine.
#![allow(clippy::expect_used)]

fn main() {
    slint_build::compile("ui/all.slint").expect("the interface must compile");
}
