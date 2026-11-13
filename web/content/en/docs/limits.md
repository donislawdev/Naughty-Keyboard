---
title: Honest limits
seoTitle: Honest limits - what Naughty Keyboard does not do yet
description: "Where Naughty Keyboard stops: typing has conditions on each system, Linux was measured on one, the folder picker is the system's, the format is not frozen."
lead: "A tool that quietly does less than it seems to is worse than one that says so. This is what it does not do yet."
---

## Does it type into other windows everywhere?

On Windows, macOS and Linux it does, and each system asks for something first. Windows asks for nothing. On macOS the program that types, `nkb` run from a terminal or the palette, needs the Accessibility permission. Without it `nkb send` says what to turn on, sends nothing and exits with code 4, and the palette puts values on the clipboard. On Linux under X11 it needs a local display. Under Wayland it needs an IBus daemon to type through, and `--clear` and the palette's shortcuts need the desktop's permission, which the desktop asks for in a window of its own the first time.

Where a system has no way, `nkb send` says so and exits with code 4, and a value reaches a field through the clipboard instead, with the palette's Copy buttons and its clipboard switch. The command line and the pack tools, `packs`, `show`, `emit`, `lint`, `fmt` and `new-pack`, do not touch the system and work everywhere.

## Was it tried on every Linux desktop?

No. It was measured on Ubuntu 26.04 only: GNOME 50 on Wayland, and X11 with openbox and xfwm4. KDE and other desktops are untested. Under Wayland `nkb send` hands the text to the input method and cannot tell whether a field had the focus. Under X11 a program that reads keys through an input method, such as xterm with IBus, can lose the end of a long run of characters the keyboard layout lacks. Under Wayland such a program, which runs through XWayland, can also drop combining accents, the joiner inside emoji sequences and the end of a long value, so check the field. On Linux the palette does not remember where it stood, and under X11 its shortcuts window cannot record a chord.

## Can the system change what is typed?

On macOS, yes. The text substitutions of the system can turn a typed value into another one, and the palette and `nkb send` say so when one that is switched on could have acted on that very value.

## Do emoji reach every application?

Not on Windows. There a program that types text sends a character beyond the basic plane, such as most emoji, in two halves. Standard Windows fields and Edge put the halves back together. Applications built on winit, a windowing library many Rust programs use, including ones made with Slint, drop such a character, so a value with emoji arrives without them. `nkb send` and the palette cannot see this from outside, so check the field. On macOS and Linux these applications get the whole value.

## What do a tab and a line break in a value do?

What the keys do. On Windows, macOS and X11 a tab is pressed as the Tab key and a line feed as Enter, the way a keyboard types them. In a field of several lines a line feed starts a new line. In a single-line field it may send the form, and a tab may move the focus to the next control, so the rest of the value goes there. A carriage return is typed as a character, which most fields drop, so CR LF ends up as one line break, and as two on macOS. Under Wayland the value is handed to the input method whole, so a tab or a line feed arrives as a character.

## Can a key I hold change the value?

On macOS it still can, in one kind of application. A Command, Control or Option held while a value is typed makes the send wait, but the system shows a key pressed in the middle of a send to the tool only some time later, and the characters typed meanwhile go in under it. Safari, Chrome, Firefox and TextEdit read the modifiers of each key and type those characters as they are. An application that keeps its own account of the modifier keys, such as one built on winit, takes them as its shortcuts: under Command, `a` selects everything and the rest of the value replaces it.

## Can I load my own packs?

Yes, from your own folder and from a team folder, in the palette and on the command line - [the palette page](/docs/palette/) says how. The Pack folders window chooses the team folder in your system's folder window. On Windows and macOS that window runs inside the palette, as it does in any program, so it loads the add-ons other programs give the file browser, such as cloud drives, which may connect to the network while it is open, and its path box copies and pastes by itself. Dropping the folder on the window avoids both, except on a palette run as administrator, which Windows does not let a program with ordinary rights drop anything on. On macOS a team folder in Documents, Desktop or Downloads makes the system ask once whether Naughty Keyboard may read it.

## Is the pack format stable?

Not yet. A later release may still change a field, and its notes will say how. `nkb lint` says so every time it runs, and `nkb lint --explain` lists every rule of the format and which ones this build checks.

## Does it tell me whether my application coped?

No. It types the value and does not read what the window shows, only the file name of its program and the kind of control that has the keyboard. Whether the application coped is for you to judge. Marking a result with its own chord is not there yet, so the report block is the record.

## Will it type into anything?

No, and on purpose. `nkb send` and the palette refuse a window running with higher privileges than themselves, because Windows would drop the keystrokes without a word, and a control where keys would act on the control rather than on text, such as a button or a list item. Where they cannot tell which kind of control has the focus, they send.

## How big is the catalogue?

{{< count "pack" >}} packs and {{< count "value" >}} values, chosen as the values that cost the most to miss. It is a start, and it is not a complete list of anything. [Suggest a value](https://github.com/donislawdev/Naughty-Keyboard/issues/new?template=suggest_value.yml) that cost you an afternoon once.
