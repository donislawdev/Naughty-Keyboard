## Download

`nkb` is the command line and `nkb-gui` the palette, one archive for each system: Windows x64, macOS on
Apple silicon and Linux x64. What each needs, and how to start it, is under
[Get it](https://github.com/donislawdev/Naughty-Keyboard#get-it).

The Windows programs are signed, with a timestamp. The macOS bundles are signed and notarised by Apple,
with the ticket stapled inside, so keep each `.app` and the link next to it together. The Linux programs
are not signed.

## Check what you downloaded

The four files whose names start with `verify-` are for checking a download, and sit together at the end
of this list. Compare an archive with `verify-SHA256SUMS.txt`, or ask for the statements made about it:

<!-- verify-commands -->

The first works for every file, the next two for every archive, and the last two for the Linux archives
and the bill of materials, `verify-naughty-keyboard_VERSION.spdx.json`. What each one answers, and why a
signed archive needs `--predicate-type`, is under
[Checking a download](https://github.com/donislawdev/Naughty-Keyboard#checking-a-download).
