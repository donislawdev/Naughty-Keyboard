---
title: Big List of Naughty Strings and Naughty Keyboard
linkTitle: Big List of Naughty Strings
url: /compare/big-list-of-naughty-strings/
seoTitle: Big List of Naughty Strings vs Naughty Keyboard
description: The Big List of Naughty Strings and Naughty Keyboard compared - a list of strings for tests, and a tool that types a value into a field and says what it breaks.
lead: "One is a list and the other is a tool, built on the same idea. This page says what each one does, when the list is enough, and how to use both."
---

## What is the Big List of Naughty Strings?

A list of strings with a high probability of causing issues when used as user input, started by Max Woolf in 2015 and published under the MIT licence. Its `blns.txt` holds more than 500 strings in 31 sections, from reserved words and numbers to Unicode, right-to-left text, script and SQL injection, and the file names Windows reserves. Each section has a one-line description. `blns.json` holds the same strings without the comments, for a program to read, and packages maintained by other people bring it into tests in several languages. It is the list most testers know, and the name of this project comes from it.

The figures are from [the repository](https://github.com/minimaxir/big-list-of-naughty-strings), counted in October 2026. `blns.txt` was last changed in April 2021.

## What does Naughty Keyboard do differently?

It does the rest of the job around a list like that:

- **It types the value into the field.** One shortcut types the next value of a pack into the field that has the keyboard focus, in a fixed order, and the cursor never leaves the field. On macOS and Linux the value goes through the clipboard instead.
- **Every value says what it breaks.** Not one line per section but two sentences per value: what it usually breaks and why, and what a correct application does instead.
- **It shows what cannot be seen.** An invisible character is drawn as a marker, and every value is counted four ways: graphemes, code points, bytes and UTF-16 units.
- **It writes the bug report.** One more shortcut copies a report block, with the value written so that someone else can type it back.
- **It has values too long for a text file.** The Big List leaves out strings of 255 characters or more on purpose, to keep the file readable. The packs keep such values as recipes, like these two:

{{< values "length-bombs/len-65535" "length-bombs/len-100000" >}}

## When is the Big List enough?

When the strings go into an automated test and nobody types them by hand. It has many more strings, sections Naughty Keyboard does not have, such as script, SQL and command injection, and ready packages for several programming languages. Naughty Keyboard has no pack of injection strings today: its values are what real users and real data produce.

## How do they compare?

| | Big List of Naughty Strings | Naughty Keyboard |
|---|---|---|
| What it is | a list of strings, as text, JSON and base64 | two programs, the palette and the `nkb` command line, over a catalogue of packs |
| Size | more than 500 strings in 31 sections | {{< count "value" >}} values in {{< count "pack" >}} packs |
| What it says about a value | one line for each section | what it breaks and what a correct application does, for each value |
| Values of 255 characters or more | left out on purpose | kept as recipes |
| Script, SQL and command injection | yes, with known CVEs | none today |
| Getting a value into a field | copy and paste | one shortcut on Windows, the clipboard on macOS and Linux |
| In an automated test | `blns.json`, and packages for several languages | `nkb emit`, as JSON, CSV or one value per line |
| Licence | MIT | GPL-3.0 for the programs, CC BY 4.0 for the packs |

## Can I use both?

Yes, at different moments. Run the Big List through an automated test of every input the application has, and take Naughty Keyboard to the fields a person tests by hand, where the sentence about what a value breaks says what to look for. When you want the catalogue of Naughty Keyboard in a test as well, `nkb emit` prints any pack:

```console
$ nkb emit magic-values --format json > magic-values.json
```

## Why is it called Naughty Keyboard?

After the Big List of Naughty Strings, which is where the idea of this catalogue starts and which most testers already know. The word is theirs.
