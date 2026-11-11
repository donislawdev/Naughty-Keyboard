---
title: nkb boundary
command: boundary
layout: command
seoTitle: nkb boundary - boundary values for a field's length limit
description: nkb boundary makes the test values for a length limit - empty, one under, at and one over - in letters, multi-byte characters, emoji and a counterstring.
lead: "Give the limit a field states and get the values that test it: nothing, one under, exactly at and one over, in characters that weigh differently in bytes and UTF-16 units."
---

## What does it make?

For a field that takes at most 50 characters: the empty value, and then values of 49, 50 and 51 in each of six fills. With `--min-length` it adds the lengths around the least the field takes, and a length that two bounds share comes once. Every value says how long it is in four units, and whether its length is inside the limit.

The packs hold lists of odd characters. This command answers the one sentence a specification gives you about a field.

To type one of the values into a field, give [nkb send](/docs/cli/send/) the same limit and the place of the value in this order: `nkb send --max-length 50 --index 4` types `letters-51`.

## Why six fills?

Because "50 characters" is one of four numbers, and you rarely know which one the application counts:

- A browser's `maxlength` is measured the way [JavaScript measures a string](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/String/length), in UTF-16 units. An emoji is two.
- PostgreSQL's `varchar(n)` holds [n characters, not bytes](https://www.postgresql.org/docs/current/datatype-character.html).
- SQL Server's `varchar(n)` [counts bytes](https://learn.microsoft.com/sql/t-sql/data-types/char-and-varchar-transact-sql), and its `nvarchar(n)` counts pairs of bytes.
- The person who wrote the specification counts what they can see.

Fifty letters `a` are fifty in every unit, so they cannot tell these apart. Fifty of a two-byte letter are fifty characters and a hundred bytes, fifty emoji are a hundred UTF-16 units, and fifty family emoji are fifty characters to the eye and two hundred and fifty code points. A field that takes fifty letters `a` and refuses or cuts one of the others has told you what it counts.

None of the characters changes under a Unicode normal form, so a field that normalises its text still gets the length you asked for.

## Examples

One value per line, to paste from or to feed a test:

```console
$ nkb boundary --max-length 50 --format lines > limit-50.txt
nkb boundary: 19 values, 1500 code points, 4350 bytes
```

CSV, with the four sizes of every value side by side:

```console
$ nkb boundary --max-length 50 --format csv
nkb boundary: 19 values, 1500 code points, 4350 bytes
id,name,value,fill,length,within,graphemes,code_points,utf16_units,bytes
empty,Empty value,,,0,true,0,0,0,0
letters-49,49 x a,aaaaa...,letters,49,true,49,49,49,49
letters-50,50 x a,aaaaa...,letters,50,true,50,50,50,50
letters-51,51 x a,aaaaa...,letters,51,false,51,51,51,51
...
two-byte-50,50 x U+0142,...,two-byte,50,true,50,50,50,100
four-byte-50,50 x U+1F600,...,four-byte,50,true,50,50,100,200
cluster-50,50 x U+1F468 U+200D U+1F469 U+200D U+1F467,...,cluster,50,true,50,250,400,900
```

A limit counted in bytes. Every value has exactly the bytes its row says, with the letter `a` in front where whole characters do not add up:

```console
$ nkb boundary --max-length 50 --unit bytes --format csv
...
two-byte-49,1 x a + 24 x U+0142,...,two-byte,49,true,25,25,25,49
two-byte-50,25 x U+0142,...,two-byte,50,true,25,25,25,50
two-byte-51,1 x a + 25 x U+0142,...,two-byte,51,false,26,26,26,51
```

## Which unit should I ask for?

Start without `--unit`. The values are then counted the way a person counts, and the application's answer to the heavier fills shows what it counts instead.

Then run again in that unit. A value of 51 bytes made of a two-byte letter is one `a` and twenty-five letters, so a field that cuts at 50 bytes cuts through the middle of the last letter. In `utf16` the same happens to an emoji, and what is left is half a character: text that no longer displays, a save that fails, or an error far from the field. A value made of whole characters up to the limit never provokes this, which is why the padding stands in front.

## How do I read a counterstring?

Every star is preceded by the number of its own place. This is the counterstring of 51:

```text
*3*5*7*9*12*15*18*21*24*27*30*33*36*39*42*45*48*51*
```

If the field kept `...42*45*48*51`, the last star stands at 48 and two characters follow it, so 50 arrived. You read the cut from the field itself, without counting and without a second tool.

A counterstring is digits and stars. A field that takes letters only refuses it for that reason, and that is what the `letters` fill is for. A field that renders Markdown may swallow the stars.

## What does "within" promise?

That the length of the value is inside the limit you gave, in the unit you gave, and nothing more. A required field still refuses the empty value, and a field that takes letters only still refuses an emoji. When a value that is within the limit is refused for its length, either the application counts in another unit or the limit is not the one the specification states. Both are worth a report.
