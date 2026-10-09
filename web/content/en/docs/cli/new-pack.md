---
title: nkb new-pack
command: new-pack
layout: command
seoTitle: nkb new-pack - start a pack of your own test values
description: nkb new-pack writes the skeleton of a new pack of test values, with one example value and notes, already in shape and already passing nkb lint.
lead: "Write the skeleton of a new pack, ready to edit and already passing `nkb lint`."
---

## What does it write?

One file in the current folder, named after the pack, with one example value to replace and a short note on the three things worth knowing first: the sentence about what a value breaks is the point, a character nobody can see is written escaped, and `nkb lint` checks the result. It never writes over a file that is already there.

The name becomes both the file name and the pack's identifier, so it is lower case letters, digits and hyphens, starting with a letter.

```console
$ nkb new-pack my-pack
Wrote my-pack.toml.
Edit it, then run `nkb lint my-pack.toml` - it reports everything in one pass.
The pack format is not frozen yet: it freezes with the first public release that ships packs.
```

Your own packs can be written and checked today, and not yet loaded into the palette. [Honest limits](/docs/limits/) says more.
