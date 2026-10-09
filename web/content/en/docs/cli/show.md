---
title: nkb show
command: show
layout: command
seoTitle: nkb show - print a pack of test values in full
description: nkb show prints one pack in full - every value escaped so invisible characters can be read, its size, what it breaks and what a correct application does.
lead: "Print one pack in full, every value with what it breaks and what a correct application does."
---

## What does it print?

The pack's name, its description, its version and licence, and then every value in the order a tester meets them. Values are printed escaped, which is the only readable form for a value made of characters nobody can see: a trailing space is `\u0020` and not a blank at the end of a line.

```console
$ nkb show whitespace
whitespace - Whitespace
Characters that take up space, or claim to, and are impossible to see in a form.
version 1.0, updated 2026-09-07, CC-BY-4.0, fields: any

  trailing-space
    Trailing space
    value    Kowalski\u0020
    size     9 code points, 9 bytes
    breaks   Login and e-mail comparisons differ between browser and server; the account is created but cannot be found.
    expect   Trimmed everywhere or preserved everywhere - never one on sign-up and the other on sign-in.
```

A value too long to write out, such as 65,535 characters, is kept in the pack as a recipe, and `show` prints the recipe rather than the value. To get the value itself, use [nkb emit](/docs/cli/emit/).
