---
title: nkb send
command: send
layout: command
seoTitle: nkb send - type a test value into the focused field
description: nkb send types one test value into the field that has the keyboard focus on Windows, clears the line first with --clear, and refuses where keys get lost.
lead: "Type one value of a pack into the field that has the keyboard focus. On Windows."
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

## Does it work on macOS and Linux?

Not yet. There the command says that it has no way to deliver keystrokes, and exits with 4. [Honest limits](/docs/limits/) says more.
