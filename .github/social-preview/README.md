# Social preview

The picture GitHub shows when a link to this repository is shared, and on the
repository's own page. It is 1280 by 640 pixels, the size GitHub shows best, and
well under the 1 MB it allows.

## What is here

| File | What it is |
|---|---|
| `social-preview.svg` | The drawing. This is the file to edit. |
| `social-preview.png` | The picture, rendered from the drawing. **A build output**, committed because GitHub does not read the image from the repository. Someone has to upload it. |

## Uploading it

GitHub keeps the image in the repository's settings, so a changed `.png` here
changes nothing on its own. Open the repository, then Settings, find Social
preview, choose Edit, then Upload an image, and pick `social-preview.png`. The
image is shown only for a public repository.

## Changing the drawing

Edit the SVG, then render it from this directory with Playwright:

```
npx playwright screenshot --viewport-size=1280,640 social-preview.svg social-preview.png
```

Two typefaces have to be installed on the machine that renders, because the
drawing names them and does not carry them. Inter is the words. DejaVu Sans Mono
is the values, and the product ships it in `crates/nkb-gui/ui/fonts`. Without
either one the browser substitutes another and the layout moves, so look at the
result before committing it.

Two more things move the picture, both found when it was rendered again on
Windows on 2026-10-14:

- Inter 4 as the variable font switches to its display cut at large sizes, and
  the headline comes out about 30 pixels narrower. Use the static Inter, or turn
  optical sizing off (`font-optical-sizing: none`).
- Chromium on Windows draws text with the coloured edges of subpixel smoothing,
  which do not belong in a picture shown on every kind of screen. Start the
  browser with `--disable-lcd-text`. The `playwright screenshot` command does not
  pass browser flags, so a short script with the `playwright` package does it,
  with `chromium.launch({ args: ['--disable-lcd-text'] })`.

Neither typeface has to be installed for this: a temporary copy of the drawing,
next to it, with a `<style>` block of `@font-face` rules pointing at the two font
files renders the same picture. Delete the copy afterwards.

The icon is read from `crates/nkb-gui/assets/edamame.svg` and not copied, so a
new icon shows up here the next time the picture is rendered.

## The platform line

This is the line that has to follow the program as it grows. The drawing holds it
twice, as the groups `platforms-today` and `platforms-everywhere`, and shows
exactly one of them:

| Group | Says | Use it |
|---|---|---|
| `platforms-today` | Types on Windows, CLI and clipboard on macOS and Linux | when typing into other windows works only on Windows |
| `platforms-everywhere` | Windows, macOS, Linux | when typing works on all three, which it does since 0.3.0 and is the one shown |

To switch, move `display="none"` from the group that is shown to the group that is
hidden, render the picture again, and upload it again. Edit the wording in the
same place if a third state is needed.

## Where each word comes from

The card is a drawing in the product's own colours (`crates/nkb-gui/ui/tokens.slint`),
not a screenshot. Everything it says is taken from this repository, and has to be
checked again when its source changes.

| On the picture | Comes from |
|---|---|
| `null`, `=1+1`, `30.02.2026`, `Kowalski` with a trailing space | the packs `magic-values`, `export-breakers`, `dates-impossible` and `whitespace` |
| what each of them does | the `breaks` field of that value |
| the open box after `Kowalski` | the marker the palette prints for an invisible character, from `crates/nkb-core/src/preview.rs` |
| Alt, Shift, N, "types the next value" | the default shortcut for the next value |
| 18 packs, 207 values, and the nine chips | `nkb packs`, which ends with "18 of 18 packs loaded, 207 values". The chips are the first nine packs it lists, in that order. Counted on 2026-10-14 |
| Windows, macOS, Linux | typing into another window has a route on each of the three, in `send_text` of `nkb-sys` (`lib.rs`, `macos` and `linux/send.rs`), and the release has an archive of each program for each system. Linux was measured on Ubuntu 26.04 only, and the Honest limits of the front page say so |
| works offline | nothing in the workspace opens a network connection |
| GUI + CLI | the two programs, `nkb-gui` and `nkb` |
| Malicious test data, one shortcut away | the first line of `nkb --help` |
