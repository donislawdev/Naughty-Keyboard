---
title: Honest limits
seoTitle: Honest limits - what Naughty Keyboard does not do yet
description: "Where Naughty Keyboard stops today: typing has conditions on each system, Linux was measured on one, own packs cannot be loaded, the format is not frozen."
lead: "A tool that quietly does less than it seems to is worse than one that says so. This is what it does not do yet."
---

## Does it type into other windows everywhere?

On Windows, macOS and Linux it does, and each system asks for something first. Windows asks for nothing. On macOS the program that types, `nkb` run from a terminal or the palette, needs the Accessibility permission. Without it `nkb send` says what to turn on, sends nothing and exits with code 4, and the palette puts values on the clipboard. On Linux under X11 it needs a local display. Under Wayland it needs an IBus daemon to type through, and `--clear` and the palette's shortcuts need the desktop's permission, which the desktop asks for in a window of its own the first time.

Where a system has no way, `nkb send` says so and exits with code 4, and a value reaches a field through the clipboard instead, with the palette's Copy buttons and its clipboard switch. The command line and the pack tools, `packs`, `show`, `emit`, `lint`, `fmt` and `new-pack`, do not touch the system and work everywhere.

## Was it tried on every Linux desktop?

No. It was measured on Ubuntu 26.04 only: GNOME 50 on Wayland, and X11 with openbox and xfwm4. KDE and other desktops are untested. Under Wayland `nkb send` hands the text to the input method and cannot tell whether a field had the focus. Under X11 a program that reads keys through an input method, such as xterm with IBus, can lose the end of a long run of characters the keyboard layout lacks. On Linux the palette does not remember where it stood, and under X11 its shortcuts window cannot record a chord.

## Can the system change what is typed?

On macOS, yes. The text substitutions of the system can turn a typed value into another one, and the palette and `nkb send` say so when one that is switched on could have acted on that very value.

## Can I load my own packs?

You can write them and check them, but not load them yet. `nkb new-pack`, `nkb fmt` and `nkb lint` all work on a file of your own. Reading a folder of your own packs into the palette and the command line is not wired up, and `nkb packs` says so: it reports the folders for your own packs and your team's as not read.

## Is the pack format stable?

Not yet. A later release may still change a field, and its notes will say how. `nkb lint` says so every time it runs, and `nkb lint --explain` lists every rule of the format and which ones this build checks.

## Does it tell me whether my application coped?

No. It types the value and does not read what the window shows, only the file name of its program and the kind of control that has the keyboard. Whether the application coped is for you to judge. Marking a result with its own chord is not there yet, so the report block is the record.

## Will it type into anything?

No, and on purpose. `nkb send` and the palette refuse a window running with higher privileges than themselves, because Windows would drop the keystrokes without a word, and a control where keys would act on the control rather than on text, such as a button or a list item. Where they cannot tell which kind of control has the focus, they send.

## How big is the catalogue?

{{< count "pack" >}} packs and {{< count "value" >}} values, chosen as the values that cost the most to miss. It is a start, and it is not a complete list of anything. [Suggest a value](https://github.com/donislawdev/Naughty-Keyboard/issues/new?template=suggest_value.yml) that cost you an afternoon once.
