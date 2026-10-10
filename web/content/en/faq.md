---
title: Questions
seoTitle: FAQ - test data, the Big List of Naughty Strings, shortcuts
description: Answers about Naughty Keyboard - how it differs from the Big List of Naughty Strings, whether it is free, what it needs, which systems it runs on, and more.
lead: "The questions people ask first, answered in the first sentence."
layout: faq
faq:
  - question: What is Naughty Keyboard?
    answer: "A tool for testers and developers that types the next awkward value into the field in front of you with one shortcut: a trailing space, a zero-width space, `30.02.2026`, `=1+1`, a name of 65,535 characters. Every value says what it usually breaks and what a correct application does. It is a window, the palette, and a command line, `nkb`, over one engine."
  - question: How is it different from the Big List of Naughty Strings?
    answer: "That list is a plain file of strings under headings, and a good one. Naughty Keyboard is the rest of the job: it types the value into the field for you, one press at a time and in a fixed order, says what each value usually breaks and what a correct application does, shows an invisible character as a marker and counts the value four ways, and turns a finding into a report block for a ticket. If a list is what you want, `nkb emit` prints any pack as one. [The comparison](/big-list-of-naughty-strings/) goes into more detail."
  - question: Is it free?
    answer: "Yes. The program is GPL-3.0-only, and the built-in packs are CC BY 4.0, which each pack states in its own licence field. There is no account and no paid edition."
  - question: Does it need the internet?
    answer: "No. It opens no network connection, has no account and sends nothing anywhere. [Security](/security/) says how that is checked."
  - question: Which systems does it run on?
    answer: "Windows 10 and 11, macOS 11 or newer on Apple silicon, and Linux on x64. Typing into other windows works on Windows. On macOS and Linux the command line works in full and the palette puts values on the clipboard for you to paste. [Honest limits](/docs/limits/) has the details."
  - question: Can I use the values in automated tests?
    answer: "Yes. `nkb emit` prints any pack as JSON, CSV or one value per line, escaped or raw, and every command has exit codes a pipeline can branch on. [nkb emit](/docs/cli/emit/) shows how."
  - question: Can I change the shortcuts?
    answer: "Yes, every one of the ten, in the shortcuts window or in `settings.toml`. A chord that would take a key away from every application is refused with the reason. [Shortcuts](/docs/shortcuts/) lists them."
  - question: Can I add my own values?
    answer: "You can write a pack of your own and check it with `nkb new-pack`, `nkb lint` and `nkb fmt` today. Loading your own packs into the palette is not wired up yet. A value that cost you an afternoon once belongs in the catalogue, and the [Suggest a value](https://github.com/donislawdev/Naughty-Keyboard/issues/new?template=suggest_value.yml) form asks for the value, what it breaks and where you saw it."
  - question: Is this an attack tool?
    answer: "No. It is built for testing software you are responsible for, and the welcome window says so: use it only on systems you are allowed to test. The built-in packs hold values that real users and real data produce, and no exploit payloads."
  - question: Why does it type rather than paste?
    answer: "Because a field meets most values from a keyboard, and some applications treat typing and pasting differently, which is worth testing on its own. When you want the other way, [clipboard mode](/docs/clipboard-mode/) puts each value on the clipboard instead."
  - question: Why is it called Naughty?
    answer: "After the Big List of Naughty Strings, which is where the idea of this catalogue starts and which most testers already know. The word is theirs."
---
