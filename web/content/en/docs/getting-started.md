---
title: Getting started
seoTitle: Getting started - your first test value in two minutes
description: Start the palette, type the first test value into a field with Alt+Shift+N, copy a report block when it breaks something, and use the same packs from nkb.
lead: "From the download to the first awkward value in a field, in about two minutes."
---

## What do I need?

A computer with Windows 10 or 11 to type into other applications. On macOS and Linux the command line and the pack tools work in full, and the palette puts a value on the clipboard for you to paste. [Download](/download/) has the archive for each system, and nothing is installed: you unpack it and run it.

## How do I type the first value?

1. Start `nkb-gui`. The first time, a welcome window opens with a box to try the first value in, so that the first value does not land in somebody else's application. Closing it opens the palette, and the palette remembers that you have seen it.
2. Click the field you want to test, in any application.
3. Press {{< kbd "Alt+Shift+N" >}}. The palette clears the line and types the next value of the pack into the field.
4. Look at what the application did with it, then press {{< kbd "Alt+Shift+N" >}} again for the next value.

The palette never takes the keyboard focus, so the cursor stays in the field the whole time. It shows the value that comes next and how far through the pack you are.

## What do I do when a value breaks something?

Press {{< kbd "Alt+Shift+B" >}}. The palette copies a report block for the last value it typed: which value, from which version of the pack, typed as what, how large, and whether all of it arrived. Paste it into the ticket. The value is written in a form someone else can type back, never as the marker the palette draws for an invisible character.

## Can I pick another pack?

Press {{< kbd "Alt+Shift+Space" >}}, or click the name of the pack at the top of the palette. The window that opens searches the values of every pack at once, so typing `pesel` finds the Polish identifier and typing `date` finds the impossible dates. [The packs](/packs/) lists every one of them with every value.

## How do I use the same values in a script?

The command line reads the same packs. List them, read one, or print its values in a format a test can load:

```console
$ nkb packs
$ nkb show whitespace
$ nkb emit magic-values --format lines > magic-values.txt
```

[The command line](/docs/cli/) describes every command and option.

## Is it safe to point at my application?

It types into the window you are looking at, which is the product, and nothing else. It opens no network connection, reads no text from any window, and touches the clipboard only when you ask it to. [Security](/security/) says what it reads, what it writes and how to check. Use it on systems you are allowed to test.
