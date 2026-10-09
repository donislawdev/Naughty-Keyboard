# Security Policy

## Supported versions

Naughty Keyboard is in early development and has no release yet. It has a single line of
development, and security fixes are made against the `main` branch. Please confirm you can
reproduce a problem on the latest `main` before reporting it.

Once there are releases, the supported version will be the **latest release** and `main`. The
release before it stops being supported the day a new one is published: no security updates, no
backports, no patched builds. There will be no long-term support line, so the upgrade path for a
security fix is always to move to the newest release.

## Reporting a vulnerability

**Please do not open a public issue for security problems.**

Use GitHub's private vulnerability reporting: open the
[Security tab](https://github.com/donislawdev/Naughty-Keyboard/security/advisories/new)
of this repository and choose **Report a vulnerability**. That keeps the report private until a
fix is available.

Please include:

- the version (`nkb --version` prints it, and a report block from the palette has it on its
  `Tool:` line) and your operating system,
- whether you used the palette (`nkb-gui`) or the command line (`nkb`),
- a clear description and the smallest steps to reproduce, ideally one command line or one pack
  file,
- the impact you believe it has.

You can expect an initial response within 14 days or fewer. Once a fix is ready it ships on
`main`, and in the next release once there is one, and the advisory is published crediting the
reporter unless you prefer to stay anonymous.

## What this tool does, and what that means for scope

**Naughty Keyboard types text into the window you are looking at. That is the product, not a
vulnerability.** Everything below follows from that sentence, so it is worth reading before writing
a report.

On Windows the tool sends a value to the window that has the keyboard focus, the way a keyboard
does, and reads a very little about that window first so it can say where the value went. A report
saying "this program sends keystrokes into other programs" describes the feature rather than a
defect. What is in scope is the tool doing that to a window **you did not choose**, sending keys it
says it never sends, or reading more than it says it reads.

Each property below is a promise this project makes, and most are held by a test in the repository
that fails the build when the promise is broken. Those tests are text scans, and each one says in its
own header what it cannot see.

**It does not read what a window shows.** Before a send it reads the kind of control that holds the
keyboard focus, from a closed list of properties, the file name of the program that owns the window
and never its title, and one number: whether that program runs with higher privileges than the tool
does. It reads no title, no text and no value of a control, and it does not watch which window
comes to the front. The tests `field_reads_only_kinds` and `program_reads_only_its_name` in `nkb-sys`
hold those reads to a list, so a new one fails the build until somebody edits the list on purpose.

**It sends no key other than the characters of the value.** The one exception is clearing the line
first, with Home, Shift+End and Delete, which the palette does by default and `nkb send` does only
with `--clear`. The test `keystrokes_have_named_doors` fails if a call that sends keys appears
anywhere except in the places it names.

**It refuses to type where the keys would be lost or misread.** A window running with higher
privileges than the tool would drop the keystrokes without a word, and the tool asks before it
sends, so for such a window clipboard mode is the way in. `nkb send` also refuses while Ctrl, Alt,
Shift or Win is held down, so the value is not turned into shortcuts, and refuses when the focus is
on a button, a link or a list item. Where it cannot tell what has the focus, it sends, and says so
in its help.

**It uses the clipboard in three places, and only writes to it.** Copying the report block, clipboard
mode and the palette's Copy buttons, each after a press or a click of yours. It never pastes for you
and never reads the clipboard. The test `clipboard_has_named_doors` names the places.

**It installs no keyboard hook.** Its shortcuts are registered with the system's hotkey call, which
tells it only that one of its own chords was pressed. Around a send it asks the system whether Ctrl,
Alt, Shift, Win and Escape are down, and whether the keys it has just sent arrived, and nothing
else about the keyboard.

**It asks for no administrator rights.** The executables carry no manifest that requests them, and
the tool does not raise its own privileges.

**`unsafe` code lives in one package.** The workspace forbids it, and `nkb-sys`, the package that
calls the operating system, is the only one where it compiles. The test `unsafe_lives_here_only`
fails if another package gains the right.

**A pack is loaded whole or not at all.** A pack file is checked first and refused on any error
before it is parsed, through one place in the code, held there by the test `one_way_into_a_pack`.
The format also bounds what a file can ask for, including a generated value, whose size is its unit
times its repeat count, so a small file cannot describe an enormous one (rules E024 and E026 of
`nkb lint`).

**It makes no connection off the machine.** No telemetry, no update check, nothing downloaded
while it runs. The test `nothing_reaches_the_network` reads the source of every package and view,
and fails on a socket, a name lookup, an HTTP client, a call that hands an address to the shell or
the browser, or a crate in `Cargo.lock` that `deny.toml` bans as a network client. The test
`links_nothing_off_the_machine` reads the import table of both built programs, and fails if either
links a network library, or a call that starts a program or opens a socket, beyond what its
register names with a reason.

**It writes only where it says.** The palette's settings go in `settings.toml` in your
configuration folder, written through a temporary file, and through the link if the file is a
symbolic link. `nkb new-pack` writes one file in the current folder and never over an existing one.
`nkb fmt` rewrites only the file it is given, and refuses and writes nothing if the change would
alter a value.

### In scope

- A way to make the tool type into a window the user did not select.
- A way to make it send a key that is not a character of the value, or the clearing keys.
- A way around the refusals above, for example typing into a window with higher privileges than the
  tool, or while a modifier is held.
- A way to make it read more than it says: a window's title, its text, the value of a control, or
  which windows the user moves between.
- A pack file or a settings file that causes code execution, that makes the tool write outside its
  settings folder and the folder it was started in, or that makes the pack check use unbounded
  time or memory.
- A way to put something other than the value on the clipboard, or to put anything on it without a
  press or a click.
- A memory-safety problem in the `unsafe` code in `nkb-sys`.
- A way to make the tool open a network connection.

### Not in scope

- **Typing into the window in front.** That is the product.
- **Values that break your application.** Finding that is the point. The values in the packs are
  awkward on purpose, and they are values real users and real data produce, not exploit payloads. A
  bug they uncover belongs in your application's tracker.
- **Other programs reading the clipboard.** In clipboard mode a value sits on the clipboard until
  something replaces it, and any program running as you can read it. That is how the clipboard
  works, and the values are not secret.
- **What a user with administrator rights can do on their own machine.** The tool does not raise
  privileges, and it is not a sandbox.
- **Using the tool on systems you are not allowed to test.** That is a legal question rather than a
  vulnerability in this code. The welcome window and the README both say to use it only on systems
  you may test.
- **A problem in a dependency that the tool does not reach.** Please report that upstream.

## Verifying a download

There is no release yet, so there is nothing to verify: you build from source. The program is
GPL-3.0-only, so the code that produced a binary is the code in this repository. When the first
release is published, this section will say what is signed and how to check it.

## Dependencies and automation

`Cargo.lock` is tracked on purpose, so the same tag builds to the same bytes. The direct
dependencies are pinned deliberately, and where one needs an argument about its licence, the argument
is written beside it in `Cargo.toml`. The workflows in `.github/workflows` run with a read-only token
and use no secret, so a pull request from a fork gets the same checks as one from the owner. The static
analysis (Semgrep) fetches its rules from the public registry. The scan itself runs on the CI machine,
sends no usage data and does not send the code anywhere.

## Code of conduct

Behaviour in this repository is covered by [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).
