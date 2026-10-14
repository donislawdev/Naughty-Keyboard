---
title: nkb packs
command: packs
layout: command
seoTitle: nkb packs - list the packs of test values
description: nkb packs lists every pack of test values the program can offer, how many values each holds, and which sources of packs it read and which it did not.
lead: "List the packs this build can offer, with how many values each one holds."
---

## What does it print?

One line per pack: its name to type, how many values it holds and its title. Then a line that counts them all, and then where the packs came from. A list drawn from one source looks exactly like a complete one, so every run also says which sources were read and which were not.

```console
$ nkb packs
whitespace                12 values  Whitespace
unicode-text              12 values  Unicode and text
length-bombs              12 values  Length bombs
magic-values              12 values  Magic values
numbers-extreme           12 values  Extreme numbers
dates-impossible          12 values  Impossible dates
export-breakers           12 values  Export breakers
locale-pl                  6 values  Polish locale
filenames-paths           12 values  File names and paths
look-alike-pairs          12 values  Look-alike pairs
emoji-complex             12 values  Complex emoji
encoding-mojibake         11 values  Encoding and mojibake
pasted-from-word          12 values  Pasted from a document
emails-urls               12 values  E-mail addresses and URLs
names-identity            12 values  Names and identity
time-zones                12 values  Time and time zones
currency-separators       12 values  Money and separators
pseudolocalization        10 values  Pseudolocalisation

18 of 18 packs loaded, 207 values.
Read these pack sources: built-in.
Not read: team - the settings file has no key for this folder yet, so it cannot be named.
Not read: own - the settings file has no key for this folder yet, so it cannot be named.
```

The name in the first column is what the other commands take: `nkb show whitespace`, `nkb emit magic-values`. [The packs](/packs/) shows every one of them with every value.
