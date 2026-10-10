---
title: The palette
seoTitle: The palette - type the next test value into any field
description: How the palette works, what it shows about the next value, how it types without taking the focus, and how to find any value in every pack at once.
lead: "A small window that types the next value of a pack into the field you are in, and never takes the keyboard to do it."
---

## What is the palette?

The palette is `nkb-gui`, a window that stays beside the application you are testing and is driven by shortcuts. You click into a field of the application, press {{< kbd "Alt+Shift+N" >}}, and the next value of the pack arrives in that field. The palette does not take the keyboard focus, so the cursor never leaves the field you are testing, and you can press the same chord a hundred times in a row.

## What does it show?

- **The pack** at the top, with how far through it you are. Clicking its name opens the window for finding a value.
- **The next value**, the one the next press types, drawn so you can see it. A character nobody can see is drawn as a marker, a box open at the top, instead of as nothing.
- **The last value it typed**, folded until you open it, with four counts: graphemes, code points, bytes and UTF-16 units. The four differ exactly for the values most likely to break a field.
- **How each value goes in**: typed from the keyboard, or put on the clipboard for you to paste, and whether the line is cleared first or the value goes in at the cursor.
- **The keys** of every shortcut that works, along the bottom.

## How does a value reach the field?

By default the palette clears the current line of the field with Home, Shift+End and Delete, then types the value the way a keyboard does, one character after another, at the pace the application takes them. A long value shows its progress while it types, and {{< kbd "Escape" >}} stops it, after which the palette says how much of it arrived.

It refuses rather than guesses. It types nothing while Ctrl, Alt, Shift or Win is held down, because the value would turn into shortcuts. It types nothing into a button, a link or a list item that has the focus. It clears nothing it cannot confirm is a text field, and it never clears a terminal, where those keys go to whatever program runs in it.

A window running with higher privileges than the palette, such as a program started as administrator, would drop the keystrokes without a word. For such a window the palette puts the value on the clipboard instead, says why, and leaves pasting to you. [Clipboard mode](/docs/clipboard-mode/) says more.

## How do I find a value?

Press {{< kbd "Alt+Shift+Space" >}}. The window that opens shows every pack as a list you can open, and searches the values of all of them at once as you type. Choosing a value makes its pack the one in use and that value the next one, so the next press types it. The values you chose there most recently, from any pack, are at the top.

## Can I make it smaller?

{{< kbd "Alt+Shift+H" >}} collapses the palette to its header and expands it again. The palette never hides and never moves on its own. It is exactly as tall as what it shows.

## What does it remember?

The pack you chose, whether it is collapsed, where it stood on each arrangement of screens, and your shortcuts, in `settings.toml`. On Windows the file is `%APPDATA%\Naughty Keyboard\settings.toml`. A file it cannot use is left as it is and never overwritten.
