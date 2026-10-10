---
title: Test cases for a text field
url: /guides/test-cases-for-a-text-field/
seoTitle: Test cases for a text field - values that break forms
description: Test cases for a text input field, value by value - empty and invisible input, length limits, Unicode, numbers, dates, words a parser misreads, and exports.
lead: "A text field is tested with the values it will meet, not only with the one the form was designed for. These are the cases, each with a real value and what it usually breaks."
---

## What should a text field be tested with?

With what users and data really put into it, beyond the name the form was designed around: nothing at all, spaces nobody can see, a value at the length limit and one past it, letters outside ASCII, numbers and dates written another way, words a parser reads as something else, and characters that only cause trouble in an export. The sections below take them one at a time. Every value comes from [the catalogue](/packs/), with what it usually breaks, and leads to what a correct application does instead.

## What do I check after each value?

The same five things every time:

1. **Is it stored as typed?** Open the record again and compare.
2. **Do the browser and the server agree?** A value the form accepts and the server rejects, or the other way round, is one of the most common findings.
3. **Does the message name the problem?** "Invalid input" for a value one character too long is a defect of its own.
4. **Does it survive the round trip?** Search for it, edit it, see it in a list.
5. **Does it survive the export?** Open the CSV or the spreadsheet the application produces.

## Empty, spaces and invisible characters

Test the boundary between empty and filled in. A field that trims spaces on one side and not on the other ends up with a value that is empty to one layer and filled in to the next.

{{< values "whitespace/space-only" "whitespace/zwsp-only" "whitespace/leading-space" "whitespace/trailing-space" >}}

## Length and limits

Test one character under the limit, the limit itself and one over it, then a value far beyond it. Check whether the limit counts characters or bytes: in UTF-8 a letter outside ASCII is one character and two bytes or more.

{{< values "length-bombs/len-255" "length-bombs/len-256" "length-bombs/bytes-vs-chars" "length-bombs/len-65535" >}}

## Letters outside ASCII and Unicode

Test the letters your users have in their names, and the characters that change how text is counted, compared or drawn.

{{< values "locale-pl/diacritics-full" "unicode-text/combining-acute" "unicode-text/emoji-in-name" "unicode-text/rtl-override" >}}

## Words a parser reads as something else

Test the words that mean "no value", "false" or "not a number" to a program somewhere between the field and the database.

{{< values "magic-values/word-null" "magic-values/bool-no" "magic-values/word-nan" "magic-values/word-undefined" >}}

## Numbers

Test the edges of the number type, a negative value where none was expected, and numbers written the way another country writes them.

{{< values "numbers-extreme/minus-one" "numbers-extreme/comma-decimal" "numbers-extreme/int32-max-plus-one" "numbers-extreme/js-safe-plus-one" >}}

## Dates

Test dates that do not exist, dates that can be read two ways, and years outside the usual range.

{{< values "dates-impossible/feb-30" "dates-impossible/feb-29-non-leap" "dates-impossible/ambiguous-day-month" "dates-impossible/year-10000" >}}

## Values that break only in the export

Test what happens after the form. A value that passes validation can still break the file the application exports, and the spreadsheet of whoever opens it.

{{< values "export-breakers/formula-equals" "export-breakers/csv-comma" "export-breakers/leading-zero-code" "export-breakers/sheet-precision" >}}

## File names

Test a field whose value becomes a file name with names a file system rejects or reads differently.

{{< values "filenames-paths/reserved-con" "filenames-paths/trailing-dot" "filenames-paths/double-extension" "filenames-paths/dotdot" >}}

## How do I go through all of them quickly?

With the palette, one shortcut per value: click the field, press {{< kbd "Alt+Shift+N" >}}, look at the result, and press it again for the next. When a value breaks something, {{< kbd "Alt+Shift+B" >}} copies a report block for the ticket. [Getting started](/docs/getting-started/) takes about two minutes.

For an automated test, `nkb emit` prints any pack as JSON, CSV or one value per line:

```console
$ nkb emit length-bombs --format json > length-bombs.json
```

The catalogue holds {{< count "value" >}} values in {{< count "pack" >}} packs, and [the packs](/packs/) show every one of them.
