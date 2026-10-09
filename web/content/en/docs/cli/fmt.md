---
title: nkb fmt
command: fmt
layout: command
seoTitle: nkb fmt - put a pack file in canonical shape
description: nkb fmt puts a pack file in canonical shape - fields in the order the format defines, equals signs lined up - and never changes a value.
lead: "Put a pack file in canonical shape, and never change what it says."
---

## What does it change?

The order of the fields in each table, to the order the format defines, and the space before each equals sign, so they line up. Nothing else: comments, blank lines and the order of the values stay exactly as they are, because the order of the values is the order a tester meets them.

If a change would alter a value, `fmt` writes nothing and exits with 4. A formatter that could change the data it formats would be one more place for a value to break.

## Examples

```console
$ nkb fmt my-pack.toml --dry-run
my-pack.toml is not in shape. Run without --dry-run to rewrite it.
$ nkb fmt my-pack.toml
Wrote my-pack.toml in canonical shape.
$ nkb fmt my-pack.toml
my-pack.toml is already in shape. Nothing was written.
```
