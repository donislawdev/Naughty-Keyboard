# Changelog

What changed for someone who uses Naughty Keyboard, newest first. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the versions follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

A release is made from a dated section of this file. That section becomes the top of its release notes
word for word, followed by the same few lines on every release about downloading and checking a file.
Changes wait under Unreleased until then.

## [Unreleased]

### Added

- On Linux, `nkb send` types into programs running under an X11 session. A character the keyboard
  layout has is pressed as its own key, so a page sees the key events of a person's keyboard. Any
  other character goes through a spare key, rewritten for the moment. A program that reads keys
  through an input method, such as xterm with IBus, can lose the end of a long run of those. Nothing
  is sent in a Wayland session, for a remote display, or while Caps Lock is on or a modifier is held.
  `--clear` is not available yet.
- The palette runs under an X11 session. Its shortcuts work while another program is in front, and
  a click on the palette leaves the keyboard where it was. Values go in the way `nkb send` puts them in.
  A layout that switches on the same two keys as a shortcut, as `Alt+Shift` often does, takes that
  shortcut from the palette, and the palette says it could not register it. Nothing works in a Wayland
  session. Remembering the window's place and recording a new shortcut come later.
- On macOS the palette and `nkb send` now say when the system's text substitutions could have changed
  a value that went in: a capital letter at the start of a sentence, a period after two spaces, and
  a straight quote made curly in a rich text document. The field can then hold something other than
  the value shown, and nothing in the field says so. The note appears only when a substitution that
  is switched on could act on that very value, and names where to switch it off.
- The palette runs on macOS. Its shortcuts, Option and Command with a letter, work while another
  application is in front, and the palette does not take the keyboard from it: a click on it gives
  the keyboard straight back. Values go in with the same care as `nkb send`, and `Escape` stops a
  long one. The window that finds a value opens on Option+Command+F, because Option+Command+Space
  is the Finder's. Without the Accessibility permission for the palette's own app the values go to
  the clipboard.
- On macOS a shortcut made of `Option`, with or without `Shift`, and neither `Command` nor `Control`
  is refused when a keyboard layout of the Mac types a character on that key. Taken globally it would
  take that character from every application, the field under test included: `Option+A` types
  `U+0105` on the Polish layout and `U+00E5` on the U.S. one. The palette says which character and
  keeps the default shortcut. This is the rule `AltGr` has on Windows, and the default shortcuts,
  which hold `Command` and `Option`, are not touched by it.
- On macOS the shortcuts window records a shortcut: choose an action, press Enter, then press the
  combination. The window reads which key and which modifiers were pressed from the key events it is
  given itself, and never asks the system what is held down in other applications. Three
  combinations belong to the application menu and cannot be recorded there: Command+Q, Command+H and
  Option+Command+H. The sentences about a shortcut without a modifier name the keys of a Mac,
  Control, Option and Command, instead of Ctrl, Alt and Win.
- On macOS the palette remembers where it stood, separately for each layout of screens, and opens
  there the next time. The place is kept when the window is closed with its close button. It is
  not kept when the palette is quit with Command+Q. A place that lies off every screen of the layout,
  after a display was unplugged, is not used, and the palette says so.
- On macOS a second palette of the same user starts without shortcuts and says why, with the sentence
  the palette already uses when every shortcut is taken. Until now both palettes heard every shortcut,
  because macOS does not refuse a shortcut that another program holds, and each typed its own value
  into the same field. The first palette keeps the shortcuts. When it ends, however it ends, the
  second takes them as soon as its shortcuts window is closed. The palette keeps a small empty file,
  `shortcuts.lock`, beside its settings file for this.
- `nkb send` types into the focused field on macOS with the same care as on Windows. It waits until
  the keys are let go, watches the window in front, stops at `Escape` and goes at the pace of the
  application. `--clear` clears the line first. macOS lets it type only when the app it runs from,
  for a command in a terminal that is the terminal, has the Accessibility permission. Without it
  `nkb send` says what to turn on, sends nothing and exits with code 4.
- A value can name a date worked out on the day it is typed: `type = "next-date"`, with a `month`, a
  `week` from `first` to `fourth` or `last`, a `weekday` and an optional `time`. It inserts the next
  such day, at least two days ahead in UTC so that it lies ahead in every time zone, written like
  `2027-03-28 02:30`. The palette and its Copy buttons, `nkb send`, `nkb emit` and the report block
  all use the date of the day the value goes out, and `nkb show` and the website show the rule.
- The palette says which half of a pair the next value is, with `pair 1 of 2` or `pair 2 of 2` beside
  its name. The window that finds a value says the same beside each half, and typing the name of a
  pair finds both of its values. `nkb show` prints what a correct application does with a pair as
  well as what it breaks, and a pack's page on the website lists its pairs, each linked with the two
  values it joins.

### Changed

- The pack format gained the value type `next-date`, its four fields `month`, `week`, `weekday` and
  `time`, and rule E018, which refuses a rule that names no day. A pack written for 0.2.0 reads
  exactly as before. A translation may not carry the four new fields, because they decide what is
  typed (E050).
- `nkb lint` warns (W043) when the second half of a pair is not the value written right after its
  first half, because the press after the first half then types another value. It now checks 41 of
  its 47 rules.
- `time-zones` 2.0: the hour the spring clock change skips and the hour the autumn one repeats are now
  the next ones rather than dates of 2027, so the pack no longer goes stale.

### Fixed

- On macOS a window that shows the system nothing inside it, such as Chrome or an application built
  on Electron before accessibility is on in it, is no longer taken for an unknown focus. When the line
  was not cleared first, the sentence now says why. Typing a value turns accessibility on there until
  the application is restarted, so the next send clears the line. That also makes those pages heavier
  until the restart, which matters when you measure performance.
- The windows the palette opens - Find a value, Shortcuts and About - came back minimized after being
  minimized once, with the keyboard in them and nothing on the screen, and one could stay in the
  taskbar where no key and not even its close button closed it. Minimizing one now closes it like
  leaving it for another window, every opening shows it, and its title bar keeps only the close
  button.
- On macOS the palette is now on the screen while the application in front is in full screen. It
  stayed in the space it was made in, so the shortcut worked and the value arrived while the palette
  was out of sight. It now stands in every space, and says so in a sentence if the system would
  not let it.

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
