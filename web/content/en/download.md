---
title: Download
seoTitle: Download for Windows, macOS and Linux - nothing to install
description: Download Naughty Keyboard for Windows, macOS or Linux. Two programs, the palette and the command line, signed on Windows and macOS, with nothing to install.
lead: "Two programs for three systems. Unpack the archive and run it: nothing is installed and nothing else is needed."
---

## Is there a release yet?

The project is in early development. If [the releases page](https://github.com/donislawdev/Naughty-Keyboard/releases) is still empty, there is no release yet, and you build it from source as shown at the end of this page. The pack format is not frozen yet either: it freezes with the first public release that ships packs.

## Which file do I need?

`nkb-gui` is the palette, the window that types a value into the field you are in. `nkb` is the command line. Every release has one archive for each program and system, and `VERSION` below stands for the version, such as `0.1.0`:

{{< downloads >}}

Typing into other windows works on Windows. On macOS and Linux the command line works in full and the palette puts values on the clipboard. [Honest limits](/docs/limits/) says more.

## What is in an archive?

The program, `LICENSE`, `README.md` and `THIRD-PARTY-NOTICES.txt`, which carries the licence of everything compiled into the program. There is no folder around them, so unpack an archive into a folder of its own.

- **Windows.** The programs are signed, with a timestamp, so Windows names the publisher. Run `nkb.exe` from a terminal, or start `nkb-gui.exe`.
- **macOS.** Each program is a bundle, `nkb.app` or `nkb-gui.app`, signed and notarised by Apple with the ticket stapled inside, and next to it is a link, so `./nkb` still works. Keep the bundle and the link together. A copy of the program taken out of its bundle is refused, because the signature covers the bundle.
- **Linux.** The programs are not signed, and the statements that come with every release say where they came from. The palette needs a desktop session, X11 or Wayland.

## How do I check a download?

Every release carries the SHA-256 of every archive and two signed statements, one of how the build was made and one of what each archive holds. [Verifying a download](/docs/verify-a-download/) has the commands and what each one proves.

## How do I build it from source?

You need [Rust](https://rustup.rs/) {{< rust-version >}} or newer.

```console
$ git clone https://github.com/donislawdev/Naughty-Keyboard
$ cd Naughty-Keyboard
$ cargo build --release -p nkb-cli
$ cargo build --release -p nkb-gui
```

The command line is then `target/release/nkb` and the palette `target/release/nkb-gui`. `cargo test --workspace` runs the whole test suite.
