---
title: nkb send
command: send
layout: command
seoTitle: nkb send - type a test value into the focused field
description: nkb send types one test value into the focused field on Windows, macOS and Linux, clears the line first with --clear, and refuses where keys get lost.
lead: "Type one value of a pack into the field that has the keyboard focus. On Windows, macOS and Linux."
---

## How do I use it?

Name the pack and, with `--index`, which value. The command waits three seconds by default so you can click into the field, then types the value there:

```console
$ nkb send magic-values --index 2 --delay 3
```

The value goes to the focused window, not to standard output, and everything the command says is on standard error. Add `--clear` to clear the current line of the field first with Home, Shift+End and Delete, the way the palette does by default. Without it, the only keys sent are the value's own characters.

## When does it refuse?

It refuses rather than guesses, and every refusal ends with exit code 4:

- while Ctrl, Alt, Shift or Win is held on the keyboard, after waiting up to two seconds for them to come up, because the value would turn into shortcuts,
- in a window running with higher privileges than `nkb`, such as an application started as administrator, because Windows would drop the keystrokes without a word,
- when the keyboard focus is on a button, a link, a list item or a page that is not editable, because keys there act on that control.

Where it cannot tell what has the focus, it sends. With `--clear` it clears only a focus it can confirm is a text field, and never a terminal, where those keys go to the program running in it. When it skips the clearing it says so, and the value goes in on top of what was there.

## Can I stop it?

Press Escape. Nothing more is typed, the command says how much of the value arrived, and it exits with 4. Escape belongs to the command only while it types. Before and after, the key is the application's.

## What does each system need?

Windows needs nothing. On macOS the program you run `nkb` from, a terminal for a command typed there, needs the Accessibility permission. Without it `nkb` says what to turn on, sends nothing and exits with 4.

On Linux under X11 it needs a local display, and it types nothing for a remote one. Under Wayland it hands the text to the desktop's input method, IBus, so it needs an IBus daemon, and it says the value was handed over, never that it is in the field, because the desktop does not say whether a field had the focus. There `--clear` needs the desktop's permission to press keys. The desktop asks for it in a window of its own the first time, and `nkb` says it is waiting.

Where a system has no way, the command says so and exits with 4. [Honest limits](/docs/limits/) says more.
