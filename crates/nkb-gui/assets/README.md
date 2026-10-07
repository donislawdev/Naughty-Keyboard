# The application icon

An edamame pod opened to show its three beans. It is original artwork drawn for
this project, and it is licensed like the rest of the repository.

## What is here, and who reads it

| File | What it is | Read by |
|---|---|---|
| `edamame.svg` | The master drawing. | The window icon (`Tokens.app-icon` in `ui/tokens.slint`), which Slint draws at the size the system asks for. Also the example below, for the 40 px and larger sizes of the `.ico`. |
| `edamame-small.svg` | The same drawing for 32 px and below: an outline about twice as heavy, a line between the beans, flat fills. | The example below, for the 16, 20, 24 and 32 px sizes of the `.ico`. |
| `edamame.ico` | Nine sizes, 16 to 256 px. **A build output**, committed because the build needs it and nobody should have to run a tool to compile. | `build.rs`, through `nkb-gui.rc`, which embeds it in `nkb-gui.exe` on Windows. That is the icon a file manager, a desktop shortcut and the taskbar show for the program. |
| `nkb-gui.rc` | One line: the resource script that names the `.ico`. | `build.rs`. |

Only `nkb-gui` carries the icon. `nkb` is a command line tool and runs in a
terminal, which has its own icon.

## Changing the drawing

Edit the SVG files, then run

```
cargo run -p nkb-gui --example make_icon
```

and commit the `.ico` it writes. The example draws both files with the toolkit's
own renderer (`tests/icon_render`), which is also what draws the window icon, so
no browser and no image tool is needed and the result is the same on every
machine.

`tests/app_icon.rs` holds the file to the drawings. It fails when the `.ico` is
older than an SVG file, when a size is missing or an entry does not decode, when
a window does not take the icon from the dictionary, and when the two SVG files
stop sharing their shapes.

## Why two drawings

A 16 px icon is a 16 by 16 grid. The master's outline is 5 px in 256, which is a
third of a pixel at 16 px, so it turns into a smear instead of an edge and the
three beans merge into one. The small drawing is the same shapes with the line
weights a small grid can hold. The two share every coordinate and the transform
that turns and fits the pod, and a test fails when they stop doing so.

## Colours

Greens only, so the icon sits beside the interface's own `ok` green. The shell
runs from `#76B03F` to `#3F7A28`, the beans from `#E2F6A0` through `#BDE46E` to
`#8CC445`, the inside of the pod is `#E0EDB3`, and one outline colour, `#2B5A20`,
draws everything. The outline is dark enough to hold the shape on a white desktop
and the beans are light enough to hold it on a black one.

## Not done, on purpose

There is nothing here for macOS (`.icns`) or for Linux desktops (a `.desktop`
file and the icon installed under `hicolor`), because the repository has no
packaging step that could use them. Both are made from `edamame.svg`, and they
belong with the packaging.
