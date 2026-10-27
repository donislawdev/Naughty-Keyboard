# Changelog

What changed for someone who uses Naughty Keyboard, newest first. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the versions follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

A release is made from a dated section of this file. That section becomes the top of its release notes
word for word, followed by the same few lines on every release about downloading and checking a file.
Changes wait under Unreleased until then.

## [Unreleased]

### Fixed

- A value no longer goes to a window that comes to the front after the shortcut or after the
  countdown of `nkb send`. The window is read once, when the send begins, and the clearing keys and
  the value go to it or nowhere. Before, a send waiting for a held modifier key took whatever window
  was in front once the key came up. Now it stops before its first key and says that another window
  came to the front. Windows and X11 compare the window, macOS the program, and Wayland, which does
  not name the window, is unchanged.
- On Windows a modifier key pressed while a value is being typed no longer changes what the value
  does. Under Alt the characters became menu commands of the application under test, under Ctrl a
  browser dropped them while the send said they were typed, and Ctrl during the clearing keys
  emptied the whole document. Now Alt stops the send and says so, and Ctrl or Win holds the next key
  until it comes up, for up to two seconds. Shift holds only a clearing key, because a character
  arrives unchanged under it.
- Closing the palette while a long value is still being typed now stops the value before its next
  key, and the program ends at once. Until now the window went away and the keys went on, for as
  long as the value took, into whatever window had the focus. The one exception is a Wayland
  desktop that is still waiting for the person to allow the keys.
- `nkb lint` and `nkb fmt` read and write only the file they are given. A name that does not end
  in `.toml` was read as another file: `nkb fmt notes.txt` formatted and rewrote `notes.toml`
  beside it. Such a name is now a usage error, exit code 2, on every system, `.TOML` included.
- A pack file larger than 4 MiB is no longer read into memory whole. `nkb lint`, `nkb fmt` and
  `nkb packs` say that the file is too large, and the commands that read it end with exit code 5.
- `nkb lint --json` now ends with exit code 5 for a file it could not read, as the text output
  always did. It ended with 3, the code for a file that is not there.
- What `nkb lint` and `nkb fmt` print from a pack file, its name, the keys and identifiers a
  problem names and the parser's message, now shows control characters and invisible ones as
  escapes such as `\u202E`. A file name or a key could otherwise change the colours of the
  terminal or the order of the line it was printed on.
- On Linux and macOS `nkb fmt` no longer rewrites a read-only pack file. It refuses it with exit
  code 5, as it always did on Windows. A file it does rewrite keeps its permission bits, where
  until now it came out with the defaults of a new file.
- `nkb fmt` on a pack file that is a symbolic link now formats the file the link points to and
  leaves the link in place. Until now it replaced the link with a plain file and left the real one
  unformatted.
- `nkb fmt` no longer removes a file named `<pack>.toml.nkb-new` that it finds beside the pack.
  It writes under a name of its own now, as saving the settings already did.
- Two palettes that save their settings at the same moment, for example when both are closed, no
  longer write over each other's change. A save waits up to a second for the other one, using a
  new empty file `settings.lock` beside the settings file, and then saves anyway. On Linux and
  macOS a new settings file is now readable by its owner only.

## [0.3.0] - 2026-10-25

This release brings typing into other windows to macOS and Linux. The Linux routes were measured on
Ubuntu 26.04 only, on GNOME 50 for Wayland and with openbox and xfwm4 for X11, so another desktop may
behave differently. The Linux programs are still not signed.

### Added

- On Linux, `nkb send` types into programs running under an X11 session. A character the keyboard
  layout has is pressed as its own key, so a page sees the key events of a person's keyboard. Any
  other character goes through a spare key, rewritten for the moment. A program that reads keys
  through an input method, such as xterm with IBus, can lose the end of a long run of those.
  `--clear` clears the line first with Home, Shift+End and Delete. Nothing is sent for a remote
  display, or while Caps Lock is on or a modifier is held.
- On Linux, `nkb send` types into a Wayland session by handing the text to the desktop's input
  method, IBus, so every character arrives, those the keyboard layout lacks included. The command
  says the value was handed over and never that it is in the field, because the desktop does not say
  whether a field had the focus. A long value goes in pieces, 100000 characters in about 2.5
  seconds, and `Escape` stops it. Nothing is sent without an IBus daemon on a local socket.
- On a Wayland session `nkb send --clear` clears the line with Home, Shift+End and Delete, pressed
  through the desktop's remote-desktop portal. The first time, the desktop asks in a window of its
  own whether keys may be pressed, and `nkb send` says it is waiting, for up to five minutes. The
  permission is kept, so the next clearing asks nothing unless the desktop has forgotten it. Nothing
  is pressed while a modifier key is held, because Ctrl held would clear the whole document.
- On Linux both programs ask the accessibility bus what kind of control has the keyboard, and only
  its kind, never a name or text. A control that takes no text, such as a button, is refused as on
  Windows. A line is cleared only when the control is confirmed to be a text field. An application
  that shows the bus nothing is typed into as before, without clearing the line first, and the
  sentence says why.
- The palette runs under an X11 session. Its shortcuts, `Ctrl+Alt` with a letter, work while another
  program is in front, and a click on the palette leaves the keyboard where it was. Values go in the
  way `nkb send` puts them in. A layout that switches on the same two keys as a shortcut, as
  `Ctrl+Alt` rarely does, takes that shortcut from the palette, and the palette says it could not
  register it. Remembering the window's place and recording a new shortcut come later.
- The palette opens in a Wayland session as a window without a frame of the system's, run through
  XWayland, so that a click on it does not take the keyboard from the field under test. Its header
  moves it and a cross at the end closes it. It types the way `nkb send` does there. While the
  desktop asks whether keys may be pressed, it steps behind the window that asks and says it waits.
- On a Wayland session the palette asks the desktop for its shortcuts through the global shortcuts
  portal, and the desktop decides which keys they are. The palette names the keys the desktop gave
  and says so when it gave none. Where the desktop wants an entry for the program, a button writes
  one small file for it and another takes it away. Nothing is written without a click. The shortcuts
  window shows the keys and cannot change them.
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

[Unreleased]: https://github.com/donislawdev/Naughty-Keyboard/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/donislawdev/Naughty-Keyboard/releases/tag/v0.3.0
[0.2.0]: https://github.com/donislawdev/Naughty-Keyboard/releases/tag/v0.2.0
