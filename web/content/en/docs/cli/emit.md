---
title: nkb emit
command: emit
layout: command
seoTitle: nkb emit - test values as JSON, CSV or one per line
description: nkb emit prints the values of a pack for a script or a test file, as JSON, CSV or one value per line, escaped or raw, optionally in base64.
lead: "Print the values of a pack for a script or a test file, as JSON, CSV or one value per line."
---

## What does it print?

Values, on standard output, and nothing else. The account of what came out, and any note about what a format could not carry, goes to standard error, so a file you redirect into holds values only. A pack with an error is not printed at all: a pack is whole or absent.

## Examples

One value per line, for a test that reads a text file:

```console
$ nkb emit magic-values --format lines > magic-values.txt
nkb emit: 12 values, 47 code points, 47 bytes
$ head -4 magic-values.txt
no
null
true
NaN
```

JSON, the default, carries every field of every value and both of its forms, the value itself and the value escaped:

```console
$ nkb emit locale-pl
{
  "schema": 1,
  "values": [
    {
      "pack": "locale-pl",
      "ref": "locale-pl/pesel-valid",
      "id": "pesel-valid",
      "name": "PESEL with a valid checksum",
      "value": "99023012343",
      "escaped": "99023012343",
```

CSV, one row per value, escaped by default, because this catalogue holds values that destroy CSV files:

```console
$ nkb emit dates-impossible --format csv
nkb emit: 12 values, 111 code points, 111 bytes
pack,ref,id,name,value,breaks,expect,fields,tags,risk,since,source,generated
dates-impossible,dates-impossible/feb-30,feb-30,30 February,30.02.2026,...
```

## Which form should I ask for?

The escaped form writes a character nobody can see as its escape, the way a pack file stores it, so a reviewer can read it and a file survives an editor. The raw form writes the value itself, which is what a test should feed to the application. `--escaped` and `--raw` choose one of the two, and asking for both is refused rather than settled quietly. `--base64` encodes every value for a channel that would damage text, such as a shell variable or an environment that rewrites line endings.

`lines` refuses a value that contains a line break, by name, rather than splitting it into two lines silently.
