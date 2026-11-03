# Package sources: WinGet and Chocolatey

These are **templates**, not packages. Every `{{PLACEHOLDER}}` is filled by
`.github/scripts/build_packages.py` from the one place that owns the value: the
names of the archives and of the installer from `.github/release/archives.toml`,
the installer's UpgradeCode from `.github/scripts/build_msi.py`, the publisher,
the addresses and the licence from the workspace `Cargo.toml`, the release date
from `CHANGELOG.md` and the checksums from the release's own
`verify-SHA256SUMS.txt`. The installer's ProductCode is new with every build, so
it is read out of the published installer itself:

    gh release download vX.Y.Z -R donislawdev/Naughty-Keyboard -p verify-SHA256SUMS.txt -p "nkb-setup_*.msi"
    python .github/scripts/build_packages.py --tag vX.Y.Z --sums verify-SHA256SUMS.txt \
        --msi nkb-setup_X.Y.Z_windows_amd64.msi --out <a directory outside the repository>

The installer is read through `msi.dll`, so WinGet is rendered on Windows.
Chocolatey alone needs no installer file: `--only chocolatey`.

The renderer refuses, with a sentence that names the input, and renders all or
nothing: a tag that is a release candidate or not a tag, a version the changelog
never dated, a checksum file that is missing, too large, not text or of another
release, an installer that is not the one the checksum file names or that says
another UpgradeCode, version or name, a destination inside the repository or
one that already holds something, and a value that would break the file it
lands in.

## Four packages, two per feed

| | the palette | the command line |
|---|---|---|
| WinGet | `DonislawDev.NaughtyKeyboard` | `DonislawDev.NaughtyKeyboard.CLI` |
| Chocolatey | `naughty-keyboard` | `naughty-keyboard-cli` |
| installs | `nkb-setup_<version>_windows_amd64.msi`, and in WinGet `nkb-gui_<version>_windows_amd64.zip` second | `nkb_<version>_windows_amd64.zip` |
| command | `nkb-gui`, and `nkb` beside it | `nkb` |

The identifiers are public for good: every install finds its upgrades by them.
The palette's package carries the command line as well, through the installer,
so a person who wants both takes that one. The command line's package is for a
build agent or a script, which wants `nkb` without a window.

A template named `name.<kind>.ext.in` belongs to one kind of package only
(`window` or `cli`). One named `name.ext.in` belongs to every package.

## What each package has to get right

Each line below was measured on 2026-11-03 on Windows 11 with WinGet 1.29.380 and
Chocolatey 2.7.4, with the packages of v0.3.0 rendered from its published files.

**The palette in WinGet: the installer first, the archive second.** A portable
package gets no Start menu entry, and nothing in a manifest can give it one -
the schema has no field for it, and
[microsoft/winget-cli#2299](https://github.com/microsoft/winget-cli/issues/2299)
asking for one has been open since 2022. So the palette's manifest lists two
installers, and the order is load-bearing. A fresh install took the installer:
both programs in `Program Files\Naughty Keyboard`, signed, the folder on the
machine `PATH` once, the Start menu entry starting `nkb-gui.exe`, one entry in
Installed apps. `--scope user` took the archive, with no administrator rights
and no Start menu entry, and an upgrade of that install stayed the archive.
Keep the archive once it is published: WinGet upgrades only within the kind
that is installed, so a manifest without it would leave every such install
where it is.

**The ProductCode is in the manifest.** WinGet finds the installed program for
a manifest by it. Without the line an upgrade and an uninstall from the manifest
both answered "No installed package found matching input criteria", and with it
both worked. The UpgradeCode is there too, so WinGet knows an install made from
the release page by hand.

**`ArchiveBinariesDependOnPath: true` in both archives.** WinGet then makes no
link in its own links folder and puts the package's folder on `PATH`. The folder
holds the program, its licence, the readme and its notices, and no library
another program could load from `PATH`. Without the field the shape depends on
the machine - a link where symbolic links are allowed, the folder on `PATH` where
they are not. One cost is not ours to fix: an uninstall leaves the folder on the
account's `PATH` although the folder is gone, for both packages. That is
[microsoft/winget-cli#6160](https://github.com/microsoft/winget-cli/issues/6160),
open since April 2026.

**Chocolatey runs the installer for the palette.** The package carries no
binaries, so it owes the moderators no `VERIFICATION.txt`, and it makes no shims.
Everything it puts on the machine comes from the installer, the same one WinGet
installs first, so the two feeds install one thing in one shape. An install over
the installer put down by hand left one entry in Installed apps. The uninstall
finds the installer by its UpgradeCode, and an installer removed outside
Chocolatey first makes it say so and succeed.

**Chocolatey unpacks the command line.** The package downloads the release
archive rather than carrying it, and Chocolatey makes one shim, `nkb`, which
answers with the program's own exit code and survives an upgrade. With both
packages installed, a new terminal finds the shim first.

**Nothing touches the settings.** Each account's palette keeps its settings under
`%APPDATA%\Naughty Keyboard`, and every file there had the same checksum after
every install, upgrade and uninstall of every package.

## Submitting

Submitting is a person's step and stays one. Nothing here is wired into a
release: a package is published under the project's name to a feed somebody else
moderates.

1. The release is published, and its `verify-SHA256SUMS.txt` and installer are the
   files you render from.
2. Render, then `winget validate --manifest` both WinGet folders and `choco pack`
   both nuspecs. Every address in them answers 200, except `packageSourceUrl` before
   this folder is on `main`.
3. Install, upgrade and remove every package on a machine you can break, and read
   the effect rather than the exit code.
4. WinGet: one pull request per package against `microsoft/winget-pkgs`, with the
   three files under `manifests/d/DonislawDev/NaughtyKeyboard/<version>/` and
   `manifests/d/DonislawDev/NaughtyKeyboard/CLI/<version>/`. Take the
   `ManifestVersion` from manifests freshly merged there, never from the schema
   folder of that repository. Chocolatey: `choco push` of each tested package.

A published release asset is never replaced. Every package version names its
files by address and checksum, so a replaced file breaks every install of it.
