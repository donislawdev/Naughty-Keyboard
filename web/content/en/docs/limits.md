---
title: Honest limits
seoTitle: Honest limits - what Naughty Keyboard does not do yet
description: Where Naughty Keyboard stops today - typing into other windows works on Windows only, your own packs cannot be loaded yet, and the pack format is not frozen.
lead: "A tool that quietly does less than it seems to is worse than one that says so. This is what it does not do yet."
---

## Does it type into other windows on macOS and Linux?

Not yet. On macOS and Linux this build has no way to register global shortcuts, so the palette cannot be driven from the keyboard there, and `nkb send` answers that there is no way to deliver keystrokes yet and exits with code 4. A value reaches a field through the clipboard instead, with the palette's Copy buttons and its clipboard switch. The command line and the pack tools, `packs`, `show`, `emit`, `lint`, `fmt` and `new-pack`, do not touch the system and work everywhere.

## Can I load my own packs?

You can write them and check them, but not load them yet. `nkb new-pack`, `nkb fmt` and `nkb lint` all work on a file of your own. Reading a folder of your own packs into the palette and the command line is not wired up, and `nkb packs` says so: it reports the folders for your own packs and your team's as not read.

## Is the pack format stable?

Not yet. It freezes with the first public release that ships packs, and until then a field may still change. `nkb lint` says so every time it runs, and `nkb lint --explain` lists every rule of the format and which ones this build checks.

## Does it tell me whether my application coped?

No. It types the value and does not read what the window shows, only the file name of its program and the kind of control that has the keyboard. Whether the application coped is for you to judge. Marking a result with its own chord is not there yet, so the report block is the record.

## Will it type into anything?

No, and on purpose. `nkb send` and the palette refuse a window running with higher privileges than themselves, because Windows would drop the keystrokes without a word, and a control where keys would act on the control rather than on text, such as a button or a list item. Where they cannot tell which kind of control has the focus, they send.

## How big is the catalogue?

{{< count "pack" >}} packs and {{< count "value" >}} values, chosen as the values that cost the most to miss. It is a start, and it is not a complete list of anything. [Suggest a value](https://github.com/donislawdev/Naughty-Keyboard/issues/new?template=suggest_value.yml) that cost you an afternoon once.
