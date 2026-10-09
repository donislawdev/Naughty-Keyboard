# Changelog

What changed for someone who uses Naughty Keyboard, newest first. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the versions follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

A release is made from a dated section of this file, and that section becomes its release notes word
for word. Changes wait under Unreleased until then.

## [Unreleased]

### Added

- `nkb`, the command line tool, for Windows, macOS and Linux. It checks a pack file against the format
  rules, puts it in canonical shape, writes the skeleton of a new one, lists the packs it can read and
  prints their values for a script. On Windows it also types a value into the field that has the
  keyboard.
- `nkb-gui`, the palette, for Windows. One shortcut types the next value of a pack into whatever window
  has the keyboard, without taking the focus from it. Values can go through the clipboard instead, and
  one more shortcut copies a report block for the last value.
- Nine built-in packs with 102 values: whitespace, Unicode text, length bombs, magic values, extreme
  numbers, impossible dates, export breakers, file names and paths, and Polish locale data.
- Archives for Windows, macOS and Linux. Each carries the licence notices of everything compiled into
  the program, and a bill of materials in SPDX describes all of them.
