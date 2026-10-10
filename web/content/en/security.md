---
title: Security
seoTitle: Security - what it reads, what it writes, and how to check
description: Naughty Keyboard types into the window you are looking at. What it reads to do that, what it never reads, where it writes, and the tests that hold each promise.
lead: "Naughty Keyboard types text into the window you are looking at. That is the product. Everything below follows from that sentence."
---

## What does it read?

On Windows, before it sends a value, it reads the kind of control that holds the keyboard focus, from a closed list of properties, the file name of the program that owns the window, never its title, and one number: whether that program runs with higher privileges than it does. It reads no title, no text and no value of any control, and it does not watch which window comes to the front. The tests `field_reads_only_kinds` and `program_reads_only_its_name` hold those reads to a list, so a new one fails the build until somebody changes the list on purpose.

## Which keys does it send?

The characters of the value, and nothing else. The one exception is clearing the line first, with Home, Shift+End and Delete, which the palette does by default and `nkb send` only with `--clear`. The test `keystrokes_have_named_doors` fails if a call that sends keys appears anywhere other than the places it names.

## Where does it refuse?

Where the keys would be lost or misread. A window running with higher privileges would drop them without a word, so for such a window the palette uses the clipboard. It sends nothing while Ctrl, Alt, Shift or Win is held down, so the value is not turned into shortcuts, and nothing when the focus is on a button, a link or a list item. Where it cannot tell what has the focus, it sends, and says so.

## Does it use the clipboard?

In three places, and only writing to it: copying the report block, clipboard mode, and the palette's Copy buttons, each after a press or a click of yours. It never pastes for you and never reads the clipboard. The test `clipboard_has_named_doors` names the places.

## Does it install a keyboard hook?

No. Its shortcuts are registered with the system's hotkey call, which tells it only that one of its own chords was pressed. Around a send it asks the system whether Ctrl, Alt, Shift, Win and Escape are down, and whether the keys it has just sent arrived, and nothing else about the keyboard.

## Does it need administrator rights?

No. The programs carry no manifest that asks for them, and neither raises its own privileges.

## Does it connect to the internet?

No. No telemetry, no update check, nothing downloaded while it runs. The test `nothing_reaches_the_network` reads the source of every package and view and fails on a socket, a name lookup, an HTTP client or a dependency banned as a network client. The test `links_nothing_off_the_machine` reads the import table of both built programs and fails if either links a network library. On Linux, the palette's toolkit talks to the X server and the D-Bus session bus that your desktop names, which are sockets on this machine unless the desktop was set up to reach them over a network.

## Where does it write?

The palette's settings go in `settings.toml` in your configuration folder, written through a temporary file. `nkb new-pack` writes one file in the current folder and never over an existing one. `nkb fmt` rewrites only the file it is given, and refuses and writes nothing if the change would alter a value.

## Is unsafe code contained?

Yes. The workspace forbids `unsafe`, and `nkb-sys`, the package that calls the operating system, is the only one where it compiles. The test `unsafe_lives_here_only` fails if another package gains the right.

## How do I report a security problem?

Privately, through [GitHub's private vulnerability reporting](https://github.com/donislawdev/Naughty-Keyboard/security/advisories/new), not in a public issue. [SECURITY.md](https://github.com/donislawdev/Naughty-Keyboard/blob/main/SECURITY.md) says what is in scope and what to include. A program that types into the window in front is the product, so a report has to show it typing into a window you did not choose, sending keys it says it never sends, or reading more than it says it reads.

## How do I know the download is genuine?

[Verifying a download](/docs/verify-a-download/) has the commands, the fingerprints of the signing certificates and what each check proves.
