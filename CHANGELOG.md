# Changelog

What changed for someone who uses Naughty Keyboard, newest first. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the versions follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

A release is made from a dated section of this file. That section becomes the top of its release notes
word for word, followed by the same few lines on every release about downloading and checking a file.
Changes wait under Unreleased until then.

## [Unreleased]

### Added

- `nkb`, the command line tool, for Windows, macOS and Linux. It checks a pack file against the format
  rules, puts it in canonical shape, writes the skeleton of a new one, lists the packs it can read and
  prints their values for a script. On Windows it also types a value into the field that has the
  keyboard.
- `nkb-gui`, the palette, for Windows. One shortcut types the next value of a pack into whatever window
  has the keyboard, without taking the focus from it. Values can go through the clipboard instead, and
  one more shortcut copies a report block for the last value.
- Eighteen built-in packs with 207 values: whitespace, Unicode text, length bombs, magic values,
  extreme numbers, impossible dates, export breakers, Polish locale data, file names and paths,
  look-alike pairs, complex emoji, encoding and mojibake, text pasted from a document, e-mail addresses
  and URLs, names, time zones, money and separators, and pseudolocalisation.
- Archives for Windows, macOS and Linux. Each carries the licence notices of everything compiled into
  the program, and a bill of materials in SPDX describes all of them.
- A Windows installer, `nkb-setup_VERSION_windows_amd64.msi`, with both programs for every account on
  the machine: the palette in the Start menu and `nkb` on the system `PATH`. It is signed like the
  programs, and a newer version replaces an older one without closing anything that runs. A release
  candidate has none.
- Signed downloads. The Windows programs carry an Authenticode signature with a timestamp, the macOS
  bundles are signed and notarised by Apple, and every release carries signed statements of how it was
  built and of what each archive holds, which `gh attestation verify` checks. The README shows the
  commands.
