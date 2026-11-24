---
title: Names with special characters for testing
url: /guides/names-with-special-characters/
seoTitle: Names with special characters for testing
description: Real names that break name fields - an apostrophe, a hyphen, two surnames, one name, the surname Null, letters outside ASCII - and what each one breaks.
lead: "A name field is usually built for a first name and a surname of plain letters. Real names are not like that. These are the names to test it with, each with what it usually breaks."
---

## Which names should a name field be tested with?

With the names real people have and the form did not expect: an apostrophe, a hyphen, two surnames, a single name, letters outside ASCII, and surnames a program reads as a value. None of them is a trick. A name field that refuses or changes one of them refuses a real person, usually on the day they try to sign up.

## What do I check after each name?

The same four things every time:

1. **Is it stored and shown as typed?** Open the record again, and look at the name in a list and in an email the application sends.
2. **Is it found by search?** Type it the same way, and with the apostrophe or the accent left out.
3. **Is the greeting right?** "Dear Mr O'Brien" built from the field, not "Dear Mr O" or "Dear Mr Obrien".
4. **Does it survive the next system?** The export, the invoice, the courier's label.

## Apostrophes, hyphens and two surnames

Test the punctuation inside real names. The apostrophe is the most common real surname that breaks forms, and a space or a hyphen inside a name breaks every rule that splits a name into words.

{{< values "names-identity/apostrophe-surname" "names-identity/hyphen-first-name" "names-identity/two-surnames" "names-identity/particle-prefix" >}}

## Surnames a program reads as a value

Test the surnames that are also words a program understands. Null and True are real surnames, and a layer that treats text loosely turns them into "nothing" and "yes".

{{< values "names-identity/surname-null" "names-identity/surname-true" >}}

## One name, one letter, a digit, a title

Test the names that break the shape the form assumed: a person with one name, a name of one letter, a digit in a name, and a title typed into the name field.

{{< values "names-identity/single-name" "names-identity/single-letter-name" "names-identity/digits-in-name" "names-identity/title-inside" >}}

## Letters outside ASCII

Test the letters your users have in their names, and two that look the same and are stored differently. An encoding narrower than the full character set turns them into question marks on save.

{{< values "locale-pl/diacritics-full" "locale-pl/surname-hyphenated" "look-alike-pairs/cyrillic-small-a" "unicode-text/emoji-in-name" >}}

## Capitals, length and spaces

Test a surname written in capitals, as on documents, a name longer than the column and the layout were sized for, and the spaces that arrive with a name copied from somewhere else.

{{< values "names-identity/all-caps-surname" "names-identity/very-long-name" "look-alike-pairs/nbsp-name" "whitespace/trailing-space" >}}

## How do I go through all of them quickly?

With the palette, one shortcut per name: choose the pack **Names and identity**, click the field, press {{< kbd "Alt+Shift+N" >}}, look at the result, and press it again for the next. When a name breaks something, {{< kbd "Alt+Shift+B" >}} copies a report block for the ticket. [Getting started](/docs/getting-started/) takes about two minutes.

For an automated test, `nkb emit` prints the pack as JSON, CSV or one value per line:

```console
$ nkb emit names-identity --format json > names.json
```

Every name is on [the page of the pack](/packs/names-identity/), with what a correct application does instead. For an email field, see [email field test cases](/email-field-test-cases/).
