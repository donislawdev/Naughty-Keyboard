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

The icon is read from `crates/nkb-gui/assets/edamame.svg` and not copied, so a
new icon shows up here the next time the picture is rendered.

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
| 98 more values in 9 packs | the 102 values in the 9 packs in `packs/`, counted on 2026-10-07, less the four shown |
| Windows | typing into another window exists only there, and `send_text` in `nkb-sys` answers `Unsupported` elsewhere |
| works offline | nothing in the workspace opens a network connection |
| GUI + CLI | the two programs, `nkb-gui` and `nkb` |
| Malicious test data, one shortcut away | the first line of `nkb --help` |
