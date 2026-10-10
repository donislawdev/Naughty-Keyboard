+++
title = "Naughty Keyboard: test data that breaks forms"
description = "Type awkward test values into any field with one shortcut: invisible spaces, impossible dates, formulas, 65,535 characters. Free, offline, open source."

limits = "**Where it works today.** Typing into other windows works on Windows. On macOS and Linux the command line and the pack tools work, and a value reaches a field through the clipboard."

[hero]
eyebrow = "GUI + CLI"
title = "Your form works."
turn = "Until someone types this."
lead = "**Naughty Keyboard** types the next awkward value into the field in front of you, with one shortcut. {values} values in {packs} packs, and each one says what it usually breaks and what a correct application does instead."
facts = [
  "Free and open source, GPL-3.0",
  "No telemetry, no network connection",
  "Types on Windows, CLI and clipboard on macOS and Linux",
]
chord_says = "types the next value"
rows_label = "Four values and what they do to a form"
rows = [
  { ref = "magic-values/word-null", says = "comes back empty" },
  { ref = "export-breakers/formula-equals", says = "exports as a formula" },
  { ref = "dates-impossible/feb-30", says = "rolls over to 2 March" },
  { ref = "whitespace/trailing-space", says = "account not found" },
]

[catalogue]
eyebrow = "The catalogue"
title = "Values your form has never met"
text = "They are not clever and not secret. They are what real users and real data produce, and nobody types them by hand because typing them is tedious."
examples = [
  "whitespace/trailing-space",
  "export-breakers/formula-equals",
  "magic-values/bool-no",
  "dates-impossible/feb-30",
  "whitespace/zwsp-only",
  "length-bombs/len-65535",
]
link = { page = "/test-cases-for-a-text-field", text = "Test cases for a text field, value by value" }

[steps]
eyebrow = "How it works"
title = "Three steps, and the cursor never leaves the field"
items = [
  { title = "Click a field", text = "In any application on Windows: a browser, a desktop program, a terminal." },
  { title = "Press", chord = "Alt+Shift+N", text = "The palette clears the line and types the next value of the pack. It never takes the keyboard focus, so the cursor stays where you put it." },
  { title = "Look at the result", text = "When a value breaks something, [[Alt+Shift+B]] copies a report block for the ticket: which value, typed as what, how large, and whether all of it arrived." },
]

[packs]
eyebrow = "{packs} packs, {values} values"
title = "Pick a pack by what you are testing"
text = "Every value carries what it usually breaks and what a correct application does. The packs are free to use under CC BY 4.0."

[cli]
eyebrow = "Command line"
title = "Made for scripts too"
text = "`nkb` prints any pack as JSON, CSV or one value per line, escaped or raw. Values go to standard output and the account of what came out goes to standard error, so a redirected file holds values and nothing else. Every command has exit codes a pipeline can branch on."
link = "Every command and option"
terminal_title = "nkb emit"
terminal = """
$ nkb emit magic-values --format lines > magic-values.txt
nkb emit: 12 values, 47 code points, 47 bytes
$ head -4 magic-values.txt
no
null
true
NaN"""

[promises]
eyebrow = "What it never does"
title = "A tool that types into other windows has to be trusted"
text = "Each of these is a promise, and most are held by a test in the repository that fails the build when it is broken."
link = "What it reads, what it writes, and how to check"
items = [
  { title = "No network connection", text = "No telemetry, no update check, nothing downloaded while it runs." },
  { title = "Reads no window content", text = "Only the kind of control that has the focus and the file name of its program. Never a title, a text or a value." },
  { title = "Clipboard only when you ask", text = "The report block, clipboard mode and the Copy buttons, each after a press or a click. It never pastes for you and never reads the clipboard." },
  { title = "No keyboard hook", text = "Its shortcuts are registered with the system's hotkey call, which tells it only about its own chords." },
  { title = "No administrator rights", text = "It asks for none and does not raise its own privileges." },
  { title = "A pack is whole or absent", text = "A pack file is checked before it is read, and refused on any error." },
]

[cta]
title = "Find it before your users do"
text = "Free, no account, no telemetry. The palette and the command line, for Windows, macOS and Linux."
+++
