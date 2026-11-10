---
title: Pack templates
seoTitle: Pack templates - test value packs to download and change
description: "Four packs of test values to download and change into your own: a first pack, a field with a length limit, a sign-up form and booking dates."
lead: "Four packs to download and change into your own. Each one passes `nkb lint` with no error and no warning, and each shows a different part of the format."
---

## How do I use a template?

Download it into your own folder, and the palette offers it at once. In the palette, press {{< kbd "Alt+Shift+Space" >}}, click **Pack folders**, then **Create and open** to open that folder. Then change the template into your own pack: give it its own `id`, rename the file to match, and replace the values with yours. [Write your own pack](/docs/write-a-pack/) explains every line.

Every template stays usable as it is, so you can try one in the palette before you change anything in it.

## A first pack

Three values for a name field: a space at the end, an apostrophe and an empty value. The shortest pack to start from, with a comment on the lines that matter the first time.

{{< template "first-pack" >}}

## A field with a length limit

Values at the edge of a 50-character limit: one under, exactly at and one over it, and the same length in Polish letters, in emoji, in spaces and in letters with a separate accent, which count differently in bytes, in code points and in the units a browser counts. Change 50 to the limit of your field, in every `count` and every name. It shows how `type = "repeat"` writes a long value in a few lines.

{{< template "text-field-limit" >}}

## A sign-up form

An e-mail field with a second field that repeats it, and a login that has to be unique. It shows two pairs: the same address with and without a space at the end, and two logins that look the same and are written in two alphabets, the second with an escaped Cyrillic letter.

{{< template "sign-up-form" >}}

## Booking dates

A check-in at an hour the clocks skip or repeat, a stay that ends the next morning, and a date far ahead. It shows `type = "next-date"`, a date written as a rule, so that it never falls into the past, and a pair of a start and an end.

{{< template "booking-dates" >}}
