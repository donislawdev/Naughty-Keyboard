---
title: Unicode test strings
url: /guides/unicode-test-strings/
seoTitle: Unicode test strings - and what each one breaks
description: Unicode test strings - emoji sequences, combining marks, look-alike letters, right-to-left text, invisible characters - and what each one breaks.
lead: "Unicode text breaks software in a few ways that keep coming back: it is counted wrong, compared wrong, drawn wrong or stored wrong. These test strings hit each of them, and every one says what it usually breaks."
---

## Which Unicode strings should I test with?

With one string for each way text goes wrong: a character that is several characters to the program, two strings that look the same and are stored differently, text that changes direction, characters nobody can see, and characters that storage built for a narrower encoding cannot hold. A field that passes all of them handles text, not only English.

## Strings counted wrong

Test text that is one character to the eye and several to the program. A limit, a counter or a cut made in the wrong unit splits it into unrelated pieces.

{{< values "emoji-complex/emoji-family-zwj" "emoji-complex/emoji-skin-tone" "emoji-complex/flag-country" "unicode-text/combining-acute" >}}

## Strings that look the same and compare different

Test pairs that no person can tell apart. Search, sign-in and every uniqueness check compare what is stored, not what is shown.

{{< values "look-alike-pairs/cyrillic-small-a" "look-alike-pairs/nfd-lodz" "unicode-text/fullwidth-latin" "unicode-text/math-bold" >}}

## Text that changes direction

Test right-to-left characters inside left-to-right text. What the screen shows is then in a different order from what is stored.

{{< values "unicode-text/rtl-override" "unicode-text/arabic-in-latin" >}}

## Characters nobody can see

Test characters that take no space on screen and still make two strings different.

{{< values "whitespace/zwsp-only" "unicode-text/soft-hyphen" "unicode-text/bom-inside" "emoji-complex/zwj-orphan" >}}

## Characters that break storage and layout

Test characters outside the basic plane, which storage configured for a narrow encoding cannot hold, and text that is taller or wider than the line it is drawn on.

{{< values "unicode-text/emoji-in-name" "emoji-complex/astral-cjk" "unicode-text/zalgo" "unicode-text/cjk-mixed" >}}

## How do I copy them without losing a character?

Copying from a page is the weak point: an invisible character, a joiner or a direction mark is easily lost on the way, and the test then proves nothing. The palette types every string exactly as the catalogue holds it, and `nkb emit` prints them escaped, so you can see every character:

```console
$ nkb emit unicode-text --format lines --escaped
```

## How do I go through all of them quickly?

With the palette, one shortcut per string: choose the pack **Unicode and text**, **Complex emoji** or **Look-alike pairs**, click the field, press {{< kbd "Alt+Shift+N" >}}, look at the result, and press it again for the next. When a string breaks something, {{< kbd "Alt+Shift+B" >}} copies a report block for the ticket, with the value written in a form someone else can type back. [Getting started](/docs/getting-started/) takes about two minutes.

The strings are on the pages of [Unicode and text](/packs/unicode-text/), [Complex emoji](/packs/emoji-complex/) and [Look-alike pairs](/packs/look-alike-pairs/), with what a correct application does instead.
