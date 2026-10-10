---
title: Exit codes
seoTitle: Exit codes of nkb - branch on them in CI
description: The six exit codes of nkb and what each one means, the same for every command, so a CI pipeline can tell a bad pack from a wrong call or an unreadable file.
lead: "Every command of `nkb` ends with one of six numbers, and a number means the same thing whichever command returns it."
---

## What does each code mean?

{{< exit-codes >}}

A command uses only the codes that can happen to it, and `nkb lint --help` and `nkb fmt --help` list theirs. Asking for help is a success, so `--help` exits with 0.

## Will the codes change?

Not in meaning. The set is complete from the first release on purpose: a code added later would turn somebody's green pipeline red without any change on their side.

## How do I use them in CI?

```sh
nkb lint packs/my-pack.toml
status=$?
if [ "$status" -eq 1 ]; then
  echo "the pack has errors"
elif [ "$status" -ne 0 ]; then
  echo "nkb could not check it"
fi
```
