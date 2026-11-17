---
title: Shortcuts
seoTitle: Keyboard shortcuts of the palette, and how to change them
description: Every shortcut of the Naughty Keyboard palette with its default chord, which ones work today, and how to change any of them in the window or in settings.toml.
lead: "Every action of the palette has a chord. These are the defaults, and every one of them can be changed."
---

## Which chords does the palette use?

All the defaults are one modifier pair and a key, so they are learned as one family. On Windows it is `Alt+Shift`, and a default is never `Ctrl+Alt`, because Windows reads `Ctrl+Alt` as AltGr, and a global shortcut on it would take a letter away from every application on a keyboard layout that types one there. On macOS it is `Option+Command`, because Option types characters and Command does not, and the chord of Find a value is `Option+Command+F`, because the Finder holds `Option+Command+Space`. On Linux it is `Ctrl+Alt`, because a keyboard layout switch often takes `Alt+Shift`. The table below shows the Windows chords. Under Wayland the desktop decides which keys a shortcut gets, and the palette names the keys the desktop gave.

{{< shortcuts >}}

The actions marked as not available yet are registered, so their chords are kept free for them, and pressing one does nothing.

## How do I change a shortcut?

In the palette, choose the **Change shortcuts** link. The window that opens lists every action with its chord. Choose an action and press the chord you want. While the window is open, the palette's own shortcuts are paused, so pressing a chord to record it does not also run it. Delete or Backspace on an action brings back its default.

Or write it in `settings.toml`, under `[shortcuts]`, with the action's name from the last column of the table above:

```toml
[shortcuts]
next-value = "Ctrl+Alt+Win+N"
```

The file is the one [the palette's page](/docs/palette/) names. The palette reads it when it starts.

## Which chords does it refuse?

A chord the palette cannot use is refused with the reason, and the action keeps the chord it had:

- a key other than a letter A to Z, a digit 0 to 9, F1 to F24 or Space,
- a chord another action already has, or that another program already holds.

## Which chords does it warn about?

A chord that takes something away from every other application is kept. The shortcuts window marks its row and says under the list what it takes, and the palette says the same when it starts with such a chord in `settings.toml`:

- a chord without Ctrl, Alt or Win (Control, Option or Command on a Mac): while the palette runs, that key no longer types or works in any application, the palette's own windows included,
- `Ctrl+Alt` with a key that one of the installed keyboard layouts types as a character with AltGr, or on a Mac `Option` with a key a layout types a character with: while the palette runs, that character cannot be typed that way.

When the palette holds a key alone as a shortcut, it types the values through other keys on Linux and on a Mac, so they still arrive whole.

`nkb send` runs apart from the palette and does not know which keys it holds. While the palette holds a key alone, a value `nkb send` types loses the characters of that key on Linux with X11, and on a Mac a shortcut on the A key alone takes every character `nkb send` types. `nkb send` cannot see this and still says the application took every keystroke.
