# Format test packs

Two sets of files, exactly as the pack format specification asks for: one that a
validator **must accept**, and one that it **must reject with a named rule code**.

The point of these files is not to test the validator that happens to exist. It is
to make the specification checkable by something other than a careful reader, so
that the validator can be rewritten - or written a second time, by somebody else -
and still be shown to agree.

## How a rejected file declares what it expects

The first line of every file in `rejected/` is a comment naming the rule codes that
the file must produce:

```toml
# expect: E001
```

A file may name more than one code, separated by commas. The test asserts the set,
not the order: a validator reports everything it finds in one run rather than
stopping at the first problem, so ordering is not part of the contract.

## Not every file in `rejected/` is refused, and that is the point

A code beginning with `W` is a warning. The pack it appears in **loads normally**
and its contents are unchanged - the file is merely harder to review than it needs
to be. `long-literal-value.toml` is such a file: it produces a code and exits zero.

The suite checks that too. A file whose codes all begin with `W` must be accepted,
and one carrying any `E` must not be, because otherwise the letter in front of a
code is decoration rather than a promise. The directory is named for the common
case and the assertion is named for the actual rule.

## Two files are about bytes rather than about text

`E005` (byte order mark) and `E006` (line endings other than a single newline) do
not describe what a file says. They describe what it is made of, which makes them
the two files most likely to be quietly repaired by something in the toolchain.

Measured on 2026-09-06 rather than assumed, because the two behave differently:

| File | Survives this repository's line-ending normalisation | Needs an exception |
|---|---|---|
| `byte-order-mark.toml` | **yes** - the mark is not a line ending and nothing rewrites it | no |
| `crlf-line-endings.toml` | **no** - the two-character ending is rewritten to one | **yes**, one path in `.gitattributes` |

Without that exception the second file would be converted on checkout and would go
on passing while testing nothing - a guard that is green and blind, which is the
exact failure this product exists to find elsewhere.

So the suite checks their **bytes** as well as their verdicts. If an editor tidies
either file, or the exception is lost, the failure says which file lost what and
where to look, rather than reporting a missing rule code and leaving the reader to
work out why.

## Naming

A file is named after the `pack.id` it declares, because a rule not yet implemented
here requires the two to match. Getting that right now costs nothing and saves
renaming every file later.
