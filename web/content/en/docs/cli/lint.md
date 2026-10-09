---
title: nkb lint
command: lint
layout: command
seoTitle: nkb lint - check a pack file against the format rules
description: nkb lint checks a pack file of test values against the rules of the pack format and reports every problem in one pass, as text or as JSON for CI.
lead: "Check a pack file against the rules of the pack format, and report every problem in one pass."
---

## What does it check?

Everything the pack format asks of a file, from its encoding to the sentence that says what each value breaks. A value without that sentence is an error, because it is the reason a catalogue is worth more than a list. Every run says how many of the format's rules this build checks, and `--explain` lists every rule and why the few it does not check wait.

## Examples

A file that passes:

```console
$ nkb lint my-pack.toml
0 errors, 0 warnings.
Checked 39 of 45 rules, 1 of them only in part. 5 not checked - run `nkb lint --explain` to see which and why.
The pack format is not frozen yet: it freezes with the first public release that ships packs.
```

A value that says nothing about what it breaks:

```console
$ nkb lint my-pack.toml
my-pack.toml:38  E030  value `trailing-space` declares no `breaks`. It is the field this catalogue exists for: say what this value usually breaks and why, or the value is a curiosity rather than a test case.
1 error, 0 warnings.
```

The same verdict for a pipeline, with `--json`, and nothing else on standard output:

```console
$ nkb lint my-pack.toml --json
{
  "schema": 1,
  "tool_version": "0.1.0",
  "pack_format": 1,
  "format_frozen": false,
  "accepted": false,
```

## Is the format final?

Not yet. It freezes with the first public release that ships packs, and `nkb lint` says so on every run, so that a file passing today is not read as a promise about tomorrow.
