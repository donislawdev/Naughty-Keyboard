---
title: Shortcuts
seoTitle: Keyboard shortcuts of the palette, and how to change them
description: Every shortcut of the Naughty Keyboard palette with its default chord, which ones work today, and how to change any of them in the window or in settings.toml.
lead: "Every action of the palette has a chord. These are the defaults, and every one of them can be changed."
---

## Which chords does the palette use?

All the defaults are `Alt+Shift` and a key, so they are learned as one family. On Windows a default is never `Ctrl+Alt`, because Windows reads `Ctrl+Alt` as AltGr, and a global shortcut on it would take a letter away from every application on a keyboard layout that types one there.

{{< shortcuts >}}

The actions marked as not available yet are registered, so their chords are kept free for them, and pressing one does nothing.

## How do I change a shortcut?

In the palette, choose the **Change shortcuts** link. The window that opens lists every action with its chord. Choose an action and press the chord you want. While the window is open, the palette's own shortcuts are paused, so pressing a chord to record it does not also run it. Delete or Backspace on an action brings back its default.

Or write it in `settings.toml`, under `[shortcuts]`, with the action's name from the last column of the table above:

```toml
[shortcuts]
next-value = "Ctrl+Alt+Win+N"
```

On Windows the file is `%APPDATA%\Naughty Keyboard\settings.toml`. The palette reads it when it starts.

## Which chords does it refuse?

A chord the palette cannot use is refused with the reason, and the action keeps the chord it had:

- a chord without Ctrl, Alt or Win, which would take a key away from typing,
- a key other than a letter A to Z, a digit 0 to 9, F1 to F24 or Space,
- a chord another action already has, or that another program already holds,
- `Ctrl+Alt` with a key that one of the installed keyboard layouts types as a character with AltGr, named in the message.
