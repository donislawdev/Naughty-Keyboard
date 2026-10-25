# Media

The animation the front page of the repository plays.

## What is here

| File | What it is |
|---|---|
| `naughty.webp` | A looping animation of the palette beside Notepad on Windows, 25 seconds long, 1200 by 733 pixels at 20 frames per second, 1 MB. It types values of two packs into the document, collapses the palette and opens the value window. |

## Why it is an animation and not a film

GitHub takes a `<video>` tag out of a page of the repository unless the file was
uploaded through its own editor, with a relative address and with an absolute one
alike. It does not play an `.mp4` kept in the repository either: the page of the
file says it cannot show a file that big, and the raw file is served as a download.
A looping picture is shown everywhere, so the front page plays this one.

The film it was made from is not kept here, because a file in the history of a
repository stays there for good and nothing could play it. It was a 25 second
recording of 5.9 MB, and the animation was made from it with FFmpeg:

```
ffmpeg -i naughty.mp4 -vf "fps=20,scale=1200:-1:flags=lanczos" -c:v libwebp_anim -lossless 0 -q:v 80 -compression_level 6 -loop 0 naughty.webp
```

If the front page should play a film with controls instead, drag the `.mp4` into
the editor of the README on GitHub. The editor uploads it and writes an address
on the `github.com/user-attachments` path, which GitHub shows as a player.

## Changing it

Replace the file and keep the name, so the README does not change. Every version
stays in the history of the repository for good, so a new animation should be no
larger than this one.

Nothing here is read by a test except this page. `crates/nkb-core/tests/repo_text/mod.rs`
skips the `.webp` in this directory. A new kind of file here has to be taught to
that module, the way the `.png` of the social preview already is, or the test
refuses it.

Look at the animation before committing it. What is on screen goes public with
it: window titles, tab names and file names.
