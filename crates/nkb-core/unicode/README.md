# Unicode Character Database files, vendored

These are upstream files from the Unicode Character Database, unmodified. They are
the source the grapheme cluster table in `src/graphemes/table.rs` is derived from,
and the conformance suite that proves the derivation is right.

## Why they are here rather than fetched

`nkb-core` has no dependencies, and that is a closed decision rather than a habit
(`architektura.md` 2.1). Counting graphemes is UAX #29, which is a rule set plus a
large table, and the table has to come from somewhere. Downloading it at build time
would make an offline build impossible and a reproducible build a matter of trust in
a web server, so the bytes live here instead.

Nothing in this directory is compiled into the product. `table.rs` is, and a test
re-derives it from these files for every one of the 1 114 112 code points.

## What each file answers

| File | What is read from it | Lines used |
|---|---|---|
| `GraphemeBreakProperty.txt` | `Grapheme_Cluster_Break` for every code point that is not `Other` | 1429 |
| `emoji-data.txt` | `Extended_Pictographic`, which rule GB11 is stated in | 451 |
| `DerivedCoreProperties.txt` | `Indic_Conjunct_Break`, which rule GB9c is stated in | 506 |
| `GraphemeBreakTest.txt` | the official conformance suite: 766 cases | all |

`DerivedCoreProperties.txt` is large and only 506 of its lines are read. It is kept
whole anyway: a trimmed copy is a file this project made up, whose checksum matches
nothing upstream and whose provenance is somebody's word.

## Provenance

Every file was taken from `https://www.unicode.org/Public/17.0.0/ucd/` on 2026-09-23,
except `LICENSE-UNICODE.txt`, which is `https://www.unicode.org/license.txt`.

    24c7fed1195c482faaefd5c1e7eb821c5ee1fb6de07ecdbaa64b56a99da22c08  DerivedCoreProperties.txt
    d6b51d1d2ae5c33b451b7ed994b48f1f4dc62b2272a5831e7fd418514a6bae89  GraphemeBreakProperty.txt
    e2d134d2c52919bace503ebb6a551c1855fe1a1faec18478c78fff254a1793ec  GraphemeBreakTest.txt
    2cb2bb9455cda83e8481541ecf5b6dfda66a3bb89efa3fa7c5297eccf607b72b  emoji-data.txt
    e7a93b009565cfce55919a381437ac4db883e9da2126fa28b91d12732bc53d96  LICENSE-UNICODE.txt

Those hashes are provenance, not protection: they say which bytes were taken, and
anyone can check them against unicode.org. The thing that actually guards this
directory is `tests/unicode_data.rs`, which fails if the table and these files stop
agreeing.

## Why the version is 17.0.0 and not the newest

The victim application used to check this product counts graphemes with a different
implementation on purpose, so that a bug in the counter cannot stand on both sides of
the comparison. That implementation is `unicode-segmentation` 1.13.3, and it states
Unicode 17.0.0. Two implementations of different versions of the standard would
disagree on real text and the disagreement would look like a bug in one of them.

Upgrading means: replace these four files, run `cargo test -p nkb-core`, expect the
table test to name every code point that moved, regenerate, and move the victim's
dependency in the same commit.

## Licence

Unicode License V3, the full text of which is in `LICENSE-UNICODE.txt`. It permits
redistribution provided the copyright and permission notice travels with the copies,
which is what that file is for. It is a permissive licence and imposes no condition
that conflicts with GPL-3.0, under which the rest of this repository is published.

## One warning about the bytes

`emoji-data.txt` contains literal invisible characters - 341 occurrences of
`U+FE0F VARIATION SELECTOR-16` inside its comments. Everywhere else in this
repository that would be a defect, and a guard says so. Here it is upstream content
that has to stay byte for byte, exactly like the pack files under `tests/packs/`
whose broken bytes are the thing being tested. The guard covers `.rs`, `.md` and
`.toml` files and does not reach into this directory, which is deliberate: an
exception carved into a safety check is a door, and no door was carved.
