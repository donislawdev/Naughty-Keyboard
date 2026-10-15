# Changelog

What changed for someone who uses Naughty Keyboard, newest first. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the versions follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

A release is made from a dated section of this file. That section becomes the top of its release notes
word for word, followed by the same few lines on every release about downloading and checking a file.
Changes wait under Unreleased until then.

## [Unreleased]

### Added

- A value can name a date worked out on the day it is typed: `type = "next-date"`, with a `month`, a
  `week` from `first` to `fourth` or `last`, a `weekday` and an optional `time`. It inserts the next
  such day, at least two days ahead in UTC so that it lies ahead in every time zone, written like
  `2027-03-28 02:30`. The palette, `nkb send`, `nkb emit` and the report block all use the date of the
  day the value goes out, and `nkb show` and the website show the rule.
- The palette says which half of a pair the next value is, with `pair 1 of 2` or `pair 2 of 2` beside
  its name. `nkb show` prints what a correct application does with a pair as well as what it breaks,
  and a pack's page on the website lists its pairs, each linked with the two values it joins.

### Changed

- The pack format gained the value type `next-date`, its four fields `month`, `week`, `weekday` and
  `time`, and rule E018, which refuses a rule that names no day. `nkb lint` now checks 40 of its 46
  rules. A pack written for 0.2.0 reads exactly as before. A translation may not carry the four new
  fields, because they decide what is typed (E050).
- `time-zones` 2.0: the hour the spring clock change skips and the hour the autumn one repeats are now
  the next ones rather than dates of 2027, so the pack no longer goes stale.

## [0.2.0] - 2026-10-14

The first public release. The project is in early development, and the pack format is not frozen yet:
a later release may still change it, and its notes will say how.

### Added

- `nkb`, the command line tool, for Windows, macOS and Linux. It checks a pack file against the format
  rules, puts it in canonical shape, writes the skeleton of a new one, lists the packs it can read and
  prints their values for a script. On Windows it also types a value into the field that has the
  keyboard.
- `nkb-gui`, the palette. On Windows one shortcut types the next value of a pack into whatever window
  has the keyboard, without taking the focus from it. Values can go through the clipboard instead, and
  one more shortcut copies a report block for the last value. On macOS and Linux the palette puts
  values on the clipboard.
- Eighteen built-in packs with 207 values: whitespace, Unicode text, length bombs, magic values,
  extreme numbers, impossible dates, export breakers, Polish locale data, file names and paths,
  look-alike pairs, complex emoji, encoding and mojibake, text pasted from a document, e-mail addresses
  and URLs, names, time zones, money and separators, and pseudolocalisation.
- Archives for Windows, macOS and Linux. Each carries the licence notices of everything compiled into
  the program, and a bill of materials in SPDX describes all of them.
- A Windows installer, `nkb-setup_0.2.0_windows_amd64.msi`, with both programs for every account on
  the machine: the palette in the Start menu and `nkb` on the system `PATH`. A newer version replaces
  an older one without closing anything that runs.
- Signed downloads. The Windows programs and the installer carry an Authenticode signature with a
  timestamp, the macOS bundles are signed and notarised by Apple, and the release carries signed
  statements of how it was built and of what each archive holds, which `gh attestation verify` checks.
  The README shows the commands.

[Unreleased]: https://github.com/donislawdev/Naughty-Keyboard/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/donislawdev/Naughty-Keyboard/releases/tag/v0.2.0
