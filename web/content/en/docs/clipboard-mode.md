---
title: Clipboard mode
seoTitle: Clipboard mode - test values you paste yourself
description: How the palette puts each test value on the clipboard instead of typing it, when it does that on its own, and how it keeps values out of clipboard history.
lead: "Instead of typing the value, the palette puts it on the clipboard, and you paste it where you want it."
---

## When is clipboard mode the right one?

When typing is the wrong way in. Some fields handle a pasted value differently from a typed one, and that difference is worth testing on its own. Some applications react to every key, so typing a long value runs their code a thousand times. And some windows cannot be typed into from outside at all.

## How do I turn it on?

In the palette, set **Send by** to **Clipboard**. From then on each value goes to the clipboard and nothing is pressed: you paste it with the application's own paste. Set it back to **Keyboard** to type again. To start in clipboard mode, run `nkb-gui --clipboard`, optionally with the name of a pack.

The palette never pastes for you. Pasting is a key press in somebody else's window, and every reason to be in clipboard mode is a reason that press would go wrong.

## When does the palette use the clipboard on its own?

For a window running with higher privileges than the palette, such as a program started as administrator. Windows would drop the keystrokes without a word, so the palette asks before it types, puts that value on the clipboard instead, and says why. It keeps doing that for the same window, and goes back to typing when you move to another.

On macOS and Linux this build has no global shortcuts, so a value reaches a field through the clipboard: with the palette's Copy buttons, or with its clipboard switch.

## What else puts a value on the clipboard?

The **Copy** button beside the next value and beside the last value puts that value on the clipboard the same way, and does not move the palette on through the pack. {{< kbd "Alt+Shift+B" >}} copies the report block for the last value. Those three are the only places the palette writes to the clipboard, each after a press or a click of yours, and it never reads the clipboard.

## Does a value end up in clipboard history?

No. On Windows a value the palette puts on the clipboard is marked to stay out of clipboard history and out of cloud clipboard, so a thousand-character test value does not fill the history you use for everything else. The report block is the exception: it is something to paste into a ticket, and it stays in history like anything you copy yourself.

## Does nkb have a clipboard mode?

No. The command line prints values to standard output, where a script or a pipe takes them, and `nkb send` types a value into the focused field on Windows. A program that puts something on the clipboard and exits would hand a value to whatever reads the clipboard next, which is not something to do from a script.
