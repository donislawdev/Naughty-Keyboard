---
title: Command line
seoTitle: The nkb command line - every command and option
description: The nkb command line lists, prints and checks packs of test values and types one into a field. Every command, every option and every exit code.
lead: "`nkb` lists, prints and checks packs, and on Windows types one value into the focused field. It runs in a terminal, a script or a CI pipeline."
layout: cli
---

`nkb` is its own program, separate from the palette, and it links no graphical toolkit, so it runs on a build server with no desktop at all. Everything it prints for a person goes to standard error, and everything a script takes goes to standard output, so `nkb emit pack > file` leaves a file of values and nothing else.

Every command answers `--help` with its usage, its options and what it does. The pages below are built from those answers, word for word, by the program's own test, so they cannot drift from the program you run.
