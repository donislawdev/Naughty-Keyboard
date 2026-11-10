---
title: Write your own pack
seoTitle: Write your own pack of test values, step by step
description: "How to write a pack of your own test values: where the file goes, every field, characters nobody can see, long values, dates, pairs, and nkb lint."
lead: "A pack is one text file. Put it in a folder, and the palette offers its values beside the built-in ones. This page goes from an empty folder to your first value typed into a field."
---

## What is a pack?

A pack is one text file in [TOML](https://toml.io/), named after the pack, that holds test values in the order a tester meets them. Each value says what it is, what it usually breaks in an application and what a correct application does with it. This is a whole pack with one value:

{{< template-part "first-pack" "pack" "trailing-space" >}}

The lines down to the first `[[values]]` describe the pack, and each `[[values]]` block is one value. Text after a `#` is a comment, which the program skips. Text goes in double quotes, a number and a date do not, and a list is written in square brackets.

## How do I make my first pack?

1. **Open your own folder.** In the palette, press {{< kbd "Alt+Shift+Space" >}} to open the value window, click **Pack folders** at the top, and then **Create and open** beside your own folder. The folder opens in your file manager.
2. **Put a template in it.** Download {{< template-link "first-pack" >}} into that folder, or one of the other [pack templates](/docs/pack-templates/).
3. **Open the file in a text editor**, such as Notepad, TextEdit or a code editor. A word processor saves formatting the program cannot read.
4. **Make it yours.** Give the pack its own `id` and rename the file to match, so `checkout.toml` holds `id = "checkout"`. Then replace the example values with your own.
5. **Use it.** Press {{< kbd "Alt+Shift+Space" >}} again: your pack is in the list, marked `own folder`. Choose it, click into the field you are testing and press {{< kbd "Alt+Shift+N" >}}.

The value window reads the folder each time it opens, so a change to the file shows without restarting the palette. A pack with a mistake in it is listed as not loading, with the number of problems, and [checking a pack](#how-do-i-check-a-pack) shows where they are.

If you use the command line, `nkb new-pack checkout` writes a new pack file with one example value, ready to edit, and the rest of this page applies to it the same way.

## Where does the file go?

In your own folder, or in a team folder. Your own folder is `packs` beside the settings file:

- Windows: `%APPDATA%\Naughty Keyboard\packs`
- macOS: `~/Library/Application Support/Naughty Keyboard/packs`
- Linux: `~/.config/naughty-keyboard/packs`

A team folder is any folder your team keeps pack files in, [described below](#can-my-team-share-packs). In both, a pack file stands straight in the folder, not in a folder inside it, and its name is the pack's `id` with `.toml` at the end. A folder may hold up to {{< format "folder_files" >}} pack files, and one file up to {{< format "pack_file_mib" >}} MiB.

A pack with the same `id` as a built-in pack is used instead of it, and a pack in your own folder instead of one in the team folder. The palette, the value window and the report block say where a pack came from, so nobody types values from one pack while thinking of another.

## What goes at the top of a pack?

The first line, `format = 1`, and then the `[pack]` table, which describes the whole pack:

| Field | Needed | What to write |
|---|---|---|
| `id` | yes | The pack's name for the program: lower case letters, digits and hyphens, starting with a letter. The file is named the same, with `.toml` at the end. |
| `name` | yes | A short name people read in the palette, up to {{< format "name_limit" >}} characters. |
| `description` | yes | One or two sentences on what the pack is for, up to {{< format "description_limit" >}} characters. |
| `version` | yes | The version of the pack's values, such as `"1.0"`. Raise it when the values change. |
| `updated` | yes | The day of the last change, as `2026-11-10`, without quotes. |
| `license` | yes | The licence of the values, such as `"CC-BY-4.0"`. For a pack that stays in your team, write what your team uses, such as `"proprietary"`. |
| `authors` | yes | A list of names, such as `["Jan Kowalski"]`. |
| `language` | yes | The language the sentences are written in, such as `"en"` or `"pl"`. |
| `tags` | advised | Words the search of the value window finds the pack by. Without any, `nkb lint` warns: only somebody who knows the pack's name finds it. |
| `fields` | no | The kinds of field the values suit: {{< format "field_kinds" >}}. |
| `risk` | no | {{< format "risk_levels" >}}. A value marked `offensive` is marked in the palette before it is typed. |
| `source` | no | Where the values came from, when they are somebody else's work. |

## What goes in a value?

One `[[values]]` block for each value:

| Field | Needed | What to write |
|---|---|---|
| `id` | yes | The value's own name: lower case letters, digits and hyphens, different from every other `id` in the pack. A report block cites a value as `pack/id`, so keep it once others use the pack. |
| `name` | yes | A short label the palette shows. |
| `value` | yes, for a value written out | The text that is typed, in double quotes. |
| `breaks` | yes | What this value usually breaks, and why, in at least {{< format "min_breaks" >}} characters. |
| `expect` | advised | What a correct application does with it. Without it, `nkb lint` warns. |
| `since` | yes | The `version` of the pack the value first appeared in. |
| `fields`, `tags`, `risk` | no | The same as at the top, for this one value. |
| `type` | no | A value the program works out: `"repeat"` for a [very long value](#how-do-i-write-a-very-long-value), `"next-date"` for a [date that stays in the future](#how-do-i-write-a-date-that-stays-in-the-future). |

The palette types the values in the order they stand in the file, so the value most likely to find a bug goes first. A tester who presses {{< kbd "Alt+Shift+N" >}} five times and moves on gets your five best values. A pack of more than {{< format "max_values" >}} values works, and `nkb lint` suggests splitting it.

## How do I write a good breaks sentence?

Say what goes wrong and why, so that the person who types the value knows where to look. `breaks` is the reason a pack is worth more than a list of strings, and it is the one part nobody can write for you.

- Too short, and refused by `nkb lint`: `Breaks the login.`
- Says what, not why: `Login fails with a trailing space.`
- Says what and why: `The sign-up form keeps the space and the login form trims it, or the other way round, so the account is created and then cannot be found.`

`expect` is the other half: what a correct application does, so a tester can tell a bug from the right answer. For the value above: `Trimmed everywhere or kept everywhere, so the same name always finds the same account.`

## How do I write a character nobody can see?

Write it as an escape inside double quotes, such as `\u0020` for a space at the end. A character you cannot see survives one editor and vanishes in the next, and a reviewer cannot tell it is there at all. Everything you can see stays as it is: letters with accents, other scripts and emoji are written out.

| Write | For |
|---|---|
| `\u0020` | a space at the start or the end of a value |
| `\u00A0` | a non-breaking space |
| `\t` | a tab, which the palette types with the Tab key |
| `\n` | a new line, which the palette types with the Enter key |
| `\u200B` | a zero-width space |
| `\u202E` | a right-to-left override |
| `\uFEFF` | a byte order mark |
| `\\` | a backslash |
| `\"` | a double quote |
| `\U0001F600` | any character by its code point, with eight digits for one above `\uFFFF` |

A letter that looks like another, such as a Cyrillic o among Latin letters, is clearer escaped as well, as `\u043E`, so a reviewer sees that it is not an `o`.

Not sure which characters need it? Write the value as it is and run `nkb lint`: it names each character that needs an escape and gives the escape to write.

In single quotes nothing is an escape. That makes `'C:\temp'` a path as it is written, and makes `'Kowalski\u0020'` the letters `\u0020`, not a space. Use double quotes for anything with an escape in it.

## How do I write a very long value?

Write a short piece and how many times it repeats, with `type = "repeat"`, instead of the whole value:

{{< template-part "text-field-limit" "len-51" >}}

`unit` is the piece, which can hold escapes, and `count` is how many times it repeats, from 1 to {{< format "max_repeat_count" >}}. The whole value may be up to {{< format "max_code_points" >}} code points. The palette types all of it, at the pace the application takes it, and {{< kbd "Escape" >}} stops it. A value written out that is longer than {{< format "literal_length_hint" >}} characters and made of one piece repeated gets a warning from `nkb lint` that suggests `repeat`.

## How do I write a date that stays in the future?

Write the rule of the day, with `type = "next-date"`, instead of the date:

{{< template-part "booking-dates" "spring-forward" >}}

When the value is typed, the program works out the first day that fits the rule and is at least two days after today in UTC, so it is in the future in every time zone. It types the day as `YYYY-MM-DD`, or `YYYY-MM-DD HH:MM` with a `time`. A date written out passes into the past one day and quietly stops testing what it says.

- `month` is 1 to 12.
- `week` is the week of that month: {{< format "weeks" >}}.
- `weekday` is {{< format "weekdays" >}}.
- `time` is optional, as `HH:MM` on a 24-hour clock.

The report block names the day that was typed, so a finding can be typed again exactly as it was.

## How do I make a pair?

Write the two values one after the other, then a `[[pairs]]` block that joins them. A pair is two values typed into two fields, such as an e-mail and the field that repeats it:

{{< template-part "sign-up-form" "email" "email-trailing-space" "repeat-email" >}}

`a` and `b` are the `id` of the two values, and `b` stands right after `a` in the file, because the second press types whatever comes next. The palette shows `pair 1 of 2` and `pair 2 of 2` beside the name of each half. `relation` is one of {{< format "relations" >}}:

- `look-alike`: two values that look the same and differ in their bytes, such as two logins in two alphabets.
- `range`: a start and an end, such as an evening start and a morning end.
- `identity`: two values the application should take as the same, or as different, such as a password and its repetition.

## How do I check a pack?

Run `nkb lint checkout.toml`, with the name of your file. It checks the file against every rule of the format and lists every problem in one go, each on its own line, and ends with how many errors and warnings it found. The [examples below](#what-do-the-most-common-errors-mean) show what it prints.

A line names the file, the line the problem is on, a code and a sentence that says what to do. A code starting with `E` is an error: the pack does not load until it is fixed. A code starting with `W` is a warning: the pack loads, and the line says what it would gain. Then `nkb fmt checkout.toml` lines the file up the way `nkb lint` expects, and never changes a value. [nkb lint](/docs/cli/lint/) and [nkb fmt](/docs/cli/fmt/) describe every option.

`nkb` is the command line, the second program of every release. The Windows installer puts it on the `PATH` with the palette, and [Download](/download/) has its archive for each system.

## What do the most common errors mean?

Each example below is a file with one mistake in it, and what `nkb lint` says about it, word for word.

### The file name and the id differ

{{< lint-example "file-name" >}}

Rename the file or change `id`, so that the two are the same. The palette finds a pack by its file name.

### A value has no breaks

{{< lint-example "no-breaks" >}}

Add `breaks` to the value, [a sentence on what it breaks](#how-do-i-write-a-good-breaks-sentence).

### The breaks sentence is too short

{{< lint-example "short-breaks" >}}

Say what goes wrong and why, in at least {{< format "min_breaks" >}} characters.

### A space is written out

{{< lint-example "space-written-out" >}}

The value ends with a space nobody can see. Write it as `\u0020`, as the sentence says.

### A value in single quotes needs an escape

{{< lint-example "single-quotes" >}}

Put the value in double quotes and write the space as `\u0020`.

### The file is not valid TOML

{{< lint-example "curly-quotes" >}}

The quotes around the name are curly ones, which some editors put in for straight quotes. TOML takes straight quotes only. Write them again, or use an editor that keeps them straight. The same error comes with a quote left open, or a bracket missing.

### A field kind is not on the list

{{< lint-example "wrong-field-kind" >}}

Pick the nearest word from the list in the sentence, here `email`.

### A key has a typo

{{< lint-example "typo-in-a-key" >}}

The format has no `expected`, so the value counts as having no `expect` at all. Correct the name, and both lines go.

### The file is not lined up

{{< lint-example "untidy" >}}

Run `nkb fmt untidy.toml`. It lines the equals signs up and puts the fields in order, and never changes a value.

## Can my team share packs?

Yes. Keep the pack files in a folder your team shares, for example a `qa-packs` folder in the repository of the application you test. Each tester points the palette at it once: **Pack folders** in the value window, then **Choose folder**, or drop the folder on that window. [The palette](/docs/palette/#can-it-offer-packs-of-my-own-or-of-my-team) says more about the window.

A team folder on a network share that does not answer in {{< format "folder_wait_seconds" >}} seconds is skipped, with a sentence that says so, and the other packs keep working. In a pipeline, `--team-folder` names the folder for one run, so a build can check the packs and use them:

```console
$ for f in qa-packs/*.toml; do nkb lint "$f" || exit 1; done
$ nkb emit checkout --team-folder qa-packs --format json > checkout.json
```

## Can I write a pack in my own language?

Yes. Set `language` to your language, such as `"pl"`, and write `name`, `description`, `breaks` and `expect` in it. The names of the fields and the words of the closed lists, such as `email` or `look-alike`, stay as they are, because the program reads them.

## Can I change a built-in pack?

Yes, by putting a pack with the same `id` in your own folder: it is used instead of the built-in one, and the palette says so. The files of the built-in packs are in the [packs folder of the repository](https://github.com/donislawdev/Naughty-Keyboard/tree/main/packs), to copy and change. A value that belongs in everyone's catalogue is better [suggested](https://github.com/donislawdev/Naughty-Keyboard/issues/new?template=suggest_value.yml), with what it breaks and where you saw it.

## Is the format final?

Not yet. A later release may still change it, and its notes will say how. `nkb lint` says so on every run, so that a file passing today is not read as a promise about tomorrow.
