---
title: Bug Magnet and Naughty Keyboard
linkTitle: Bug Magnet
url: /compare/bug-magnet/
seoTitle: Bug Magnet vs Naughty Keyboard - also outside the browser
description: Bug Magnet and Naughty Keyboard compared - a right-click menu of test values in the browser, and a shortcut that types them into any window.
lead: "Both put awkward test values into a field so you do not type them by hand. One lives in the browser's right-click menu, the other in a palette driven by shortcuts. This page says what each one does, and when the browser extension is enough."
---

## What is Bug Magnet?

An extension for Chrome and Firefox by Gojko Adzic, started in 2015 and published under the MIT licence. It adds a menu to the right click on an input field, a text area or an editable part of a web page, and the item you click goes into that field. Its menus hold 324 values in 14 groups: names, e-mail addresses, URLs, numbers, amounts, text sizes, whitespace, Unicode, a few format checks, sample text in several scripts, post codes, cities, and 88 test card numbers of payment providers. You can add menus of your own from a configuration file.

The figures are from [the repository](https://github.com/gojko/bugmagnet), counted in November 2026. Version 4.0 came out in November 2025, and the file of values was last changed in December 2017.

## What does Naughty Keyboard do differently?

- **It types into any window.** A web page, a desktop application, a terminal, a dialog of the system. One shortcut types the next value of a pack into the field that has the keyboard focus, on Windows, macOS and Linux, and the palette never takes the focus.
- **No mouse between values.** The values of a pack come in a fixed order, one shortcut each, so you go through a pack without opening a menu every time. The value window searches every pack when you want one value in particular.
- **Every value says what it breaks.** Two sentences per value: what it usually breaks and why, and what a correct application does instead. A menu item has a label.
- **It writes the bug report.** One more shortcut copies a report block with the value written so that someone else can type it back, how large it is, and whether all of it arrived.
- **It counts what you cannot see.** An invisible character is drawn as a marker, and every value is counted in graphemes, code points, bytes and UTF-16 units.
- **It makes values for a length limit.** Give the limit a field states, and it makes the values one under, at and one over it, in characters that weigh differently in bytes. See [character limit test text](/character-limit-test-text/).
- **It works in a script too.** `nkb emit` prints any pack as JSON, CSV or one value per line, for an automated test.

## When is Bug Magnet enough?

When you test web forms only, in Chrome or Firefox, and a right-click menu suits the way you work. It needs nothing installed outside the browser. It also has groups Naughty Keyboard does not have: test card numbers of payment providers, sample text in several scripts, post codes and cities of several countries.

## How do they compare?

| | Bug Magnet | Naughty Keyboard |
|---|---|---|
| What it is | an extension for Chrome and Firefox | two programs, the palette and the `nkb` command line, over a catalogue of packs |
| Where it types | fields of web pages | the field with the keyboard focus, in any application |
| How you pick a value | the right-click menu | one shortcut for the next value, or the value window to search |
| Size | 324 values in 14 groups, 88 of them card numbers | {{< count "value" >}} values in {{< count "pack" >}} packs |
| What it says about a value | its label in the menu | what it breaks and what a correct application does |
| Values of your own | menus from a configuration file | packs from a folder of your own or of your team, checked by `nkb lint` |
| A bug report | no | a report block in one shortcut |
| In an automated test | not what it is made for | `nkb emit`, as JSON, CSV or one value per line |
| Licence | MIT | GPL-3.0 for the programs, CC BY 4.0 for the packs |

## Can I use both?

Yes. Keep Bug Magnet in the browser for the card numbers and the sample text, and take Naughty Keyboard to the desktop applications, to the fields you test value after value, and to the bugs you want to report with the value written exactly. [Getting started](/docs/getting-started/) takes about two minutes.
