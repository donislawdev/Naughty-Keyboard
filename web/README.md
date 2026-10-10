# The project website

The source of `naughtykeyboard.donislawdev.com`, built with [Hugo](https://gohugo.io/). It is the
documentation and the catalogue, in English, and in more languages as each one is finished.

## What it may say

Only what is true of the program. Everything the program can say about itself, the site reads from
the program and never writes by hand:

| What | From | Kept true by |
|---|---|---|
| every pack and every value, with what the palette shows for it | `data/facts/catalogue.json` | `cargo test -p nkb-cli --test site_facts` |
| every command, option, format and exit code its help names, and the help word for word | `data/facts/cli.json` | the same test |
| every exit code | `data/facts/exit-codes.json` | `cargo test -p nkb-cli the_site_lists_every_exit_code` |
| every shortcut, its default chord and whether it works | `data/facts/shortcuts.json` | `cargo test -p nkb-gui --test site_facts` |
| the six archives of a release | `../.github/release/archives.toml`, mounted | the release reads the same file |
| the fingerprints of the signing certificates | `../.github/release/codesign.toml`, mounted | the signing and the release check pin them there |
| the commands that check a download | the `verify-commands` block of `../README.md` | the release notes and the release check run the same block |
| the oldest Rust a build from source needs | `../Cargo.toml`, mounted | the workspace itself |

When the program changes, those tests fail until the facts are written again:

```
NKB_WRITE_SITE=1 cargo test -p nkb-cli --test site_facts
NKB_WRITE_SITE=1 cargo test -p nkb-cli the_site_lists_every_exit_code
NKB_WRITE_SITE=1 cargo test -p nkb-gui --test site_facts
```

Then look at the site's diff before committing it.

## Building it

```
cd web
hugo --printI18nWarnings --panicOnWarning
python ../.github/scripts/site_check.py public --config hugo.toml
```

The version of Hugo is the one `.github/workflows/site.yml` pins with its digest. The build refuses,
rather than writing a page with a hole in it, when a page links to a path with no page, a page names
a value or an option the program does not have, a language file lacks a word, or a translation was
made from English the program no longer says. `site_check.py` then reads every page it wrote: one
`h1`, title and description lengths, canonical addresses, `hreflang` that names back, the sitemap,
every internal link and fragment, and nothing loaded from another host.

## Languages

English is served at the root and every other language under its own prefix, with its paths
translated where the language is written in the Latin alphabet and kept in English where it is not.
A language goes live only when every page is translated: an English page under another language's
address is a duplicate to a search engine.

A language lives in three places, and its words may live nowhere else (a test in `nkb-core` holds
that line):

- `content/<language>/`, its pages,
- `i18n/<language>.toml`, the words of the site's interface, with every plural form the language has,
- `data/translations/<language>/`, its translations of what the program says.

Each translation of a sentence of the program carries `source`, the first twelve characters of the
SHA-256 of the English it was made from. When the program changes that sentence, the build stops and
names the new digest, to be written once the translation has been checked against the new English.

## Publishing

`.github/workflows/site.yml` builds and checks the site on every pull request and publishes nothing
there. It publishes after CI has passed on `main`, for the commit CI passed on, so a page never goes
up while the tests that compare its facts with the program are red. Starting the workflow by hand
from `main` publishes `main` again. GitHub Pages serves the site from the address in its settings,
and `static/CNAME` holds the same address.
