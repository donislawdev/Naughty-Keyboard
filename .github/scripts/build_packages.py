#!/usr/bin/env python3
"""Render the WinGet and Chocolatey packages of one published release.

    python .github/scripts/build_packages.py --tag v0.3.0 --sums verify-SHA256SUMS.txt \\
        --msi nkb-setup_0.3.0_windows_amd64.msi --out <a directory outside the repository>
    python .github/scripts/build_packages.py --tag v0.3.0 --sums verify-SHA256SUMS.txt \\
        --only chocolatey --out <a directory outside the repository>

Two programs ship, the palette and the command line, and each becomes a package
of its own in both feeds, so that a build agent can take the command line
without the palette and one package waiting in moderation does not hold the
other:

                 the palette                      the command line
    WinGet       DonislawDev.NaughtyKeyboard      DonislawDev.NaughtyKeyboard.CLI
    Chocolatey   naughty-keyboard                 naughty-keyboard-cli
    installs     the Windows installer, and in    the command line's archive
                 WinGet the palette's archive
                 second

The identifiers are public for good: every install finds its upgrades by them.

The templates in packaging/ hold the shape. This fills them from the one place
each value lives: the names of the archives and of the installer from
archives.toml, the UpgradeCode from build_msi.py, the publisher, the addresses
and the licence from the workspace Cargo.toml, the year of the copyright from
the programs' own file details, the date from CHANGELOG.md and the checksums
from the release's own verify-SHA256SUMS.txt. The installer's ProductCode is
new with every build, so it is read out of the published installer itself,
through msi.dll - which is why WinGet is rendered on Windows and with --msi. That
file is held to the checksum file first, and its UpgradeCode, version and name
to what this tree says.

The renderer refuses, with a sentence that names the input, and renders all or
nothing: a tag that is a release candidate or not a tag, a version the changelog
never dated, a checksum file that is missing, too large, not text or of another
release, an installer that is not the one the checksum file names, a
destination inside the repository or one that already holds something, and a
value that would break the file it lands in.

Exit codes: 0 every package was rendered into --out, 1 refused, and the message
says which input and why.

Nothing here submits anything. A package is published under the project's name
to a feed somebody else moderates, and a person decides when.
"""
import argparse
import os
import re
import shutil
import stat
import sys
import tempfile
from collections import namedtuple

import build_msi
import contents
from contents import ROOT, Refused

TEMPLATES = os.path.join(ROOT, "packaging")
TEMPLATE_SUFFIX = ".in"
PLACEHOLDER = re.compile(r"\{\{([A-Z0-9_]+)\}\}")
CHANGELOG = os.path.join(ROOT, "CHANGELOG.md")
FILE_DETAILS = os.path.join(ROOT, "crates", "nkb-cli", "file_details.rs")

# The icon Chocolatey shows, as a path in the repository at the tag. An SVG is
# accepted (package validator rule CPMR0058, "Use PNG or SVG for package icons").
ICON = "crates/nkb-gui/assets/edamame.svg"

# The manifest schema WinGet's submission pipeline accepts TODAY, and it moves.
# Take it from manifests freshly merged into microsoft/winget-pkgs, never from
# the schema folder of that repository: a sibling project's submission with
# 1.28.0 was refused on 2026-08-25 while the folder already listed it, and the
# pipeline marked 1.12.0 deprecated on 2026-10-30. On 2026-11-03 manifests
# merged that day carried 1.28.0 and one was accepted with it.
WINGET_SCHEMA = "1.28.0"

Package = namedtuple("Package", "kind winget_id choco_id title moniker")

# The product's name, from archives.toml, is the palette's title. The command
# line's package says what it is in its title, the way the two read side by side
# in a search.
PACKAGES = (
    Package("window", "DonislawDev.NaughtyKeyboard", "naughty-keyboard", "{product}", "naughty-keyboard"),
    Package("cli", "DonislawDev.NaughtyKeyboard.CLI", "naughty-keyboard-cli", "{product} CLI", "nkb"),
)

DESCRIPTION = (
    "Naughty Keyboard is a tool for testers and developers. One shortcut types the next awkward value "
    "into the field in front of you - null, a trailing space, a zero-width space, 30.02.2026, =1+1, a "
    "name of 65 535 characters. They are the values real users and real data produce. Every value "
    "comes with what it usually breaks and what a correct application does instead, and one more "
    "shortcut copies a report block to paste into a ticket. It runs entirely on your machine and "
    "sends nothing over the network."
)

SHORT = {
    "window": "Types the values that break your forms into the field you are in, with one shortcut. "
              "The palette window.",
    "cli": "Types the values that break your forms into the field you are in, and prints and checks "
           "the packs they come in. The command line, for scripts and pipelines.",
}

TAGS = ("qa", "testing", "test-data", "naughty-strings", "unicode", "edge-cases", "forms", "keyboard")
KIND_TAG = {"window": "gui", "cli": "cli"}


def how_to_start(package, feed, window, cli):
    """The paragraph that differs by feed: what the package gives and how to start it.

    The palette's WinGet package has two installers, so it describes two shapes.
    The archive is what --scope user gets, and what an install of the archive
    keeps, because WinGet upgrades only within the kind installed."""
    other = next(p for p in PACKAGES if p is not package)
    other_id = other.winget_id if feed == "winget" else other.choco_id
    if package.kind == "cli":
        return ("This package is the command line, for scripts and pipelines. The palette window, with "
                "a Start menu entry, is the package %s, and it carries %s as well. Open a new terminal "
                "and type %s --help to see the commands." % (other_id, cli, cli))
    if feed == "winget":
        return ("This package is the palette window. WinGet installs it with the Windows installer, for "
                "every account on the machine, and asks for administrator rights. That puts the palette "
                "in the Start menu and puts both %s and the command line %s on PATH, so the package %s "
                "is not needed beside it. With --scope user it needs no administrator rights, but WinGet "
                "installs the archive instead, which has no Start menu entry - open a new terminal and "
                "type %s. An install of the archive stays the archive when it is upgraded. Close the "
                "palette before you upgrade, because a program that is running cannot be replaced."
                % (window, cli, other_id, window))
    return ("This package is the palette window. It runs the Windows installer of the release, which "
            "puts the palette in the Start menu of every account and puts both %s and the command line "
            "%s on PATH, so the package %s is not needed beside it. Open a new terminal for the "
            "commands." % (window, cli, other_id))


# --------------------------------------------------------------------------- #
# what the tree and the release say
# --------------------------------------------------------------------------- #

def read_text(path):
    """A file of this repository - a template, the changelog. A checkout without
    one is broken, and the traceback says so. What a person chooses, the
    checksum file and the installer, is read with a refusal instead."""
    with open(path, encoding="utf-8-sig") as handle:
        return handle.read().replace("\r\n", "\n")


Programs = namedtuple("Programs", "installer window cli")


def programs():
    """The Windows installer and the two archives of its platform. The program
    its shortcut starts is the palette, and the other one is the command line."""
    installer = contents.load_installer()
    archives = [a for a in contents.load_archives() if (a.os, a.arch) == (installer.os, installer.arch)]
    window = [a for a in archives if a.program == installer.shortcut]
    cli = [a for a in archives if a.program != installer.shortcut]
    if len(window) != 1 or len(cli) != 1:
        raise Refused(["archives.toml has %d programs on %s %s, and the packages are one palette and one "
                       "command line" % (len(archives), installer.os, installer.arch)])
    if (installer.os, installer.arch) != ("windows", "amd64"):
        raise Refused(["the installer is for %s %s, and the templates write the x64 of Windows"
                       % (installer.os, installer.arch)])
    if window[0].format != "zip" or cli[0].format != "zip":
        raise Refused(["the Windows archives are not zips, and WinGet and Chocolatey unpack a zip here"])
    return Programs(installer, window[0], cli[0])


def parse_tag(tag):
    """The version a tag names, or a refusal saying why it names none."""
    if re.fullmatch(r"v\d+\.\d+\.\d+-.+", tag or ""):
        raise Refused(["%s is a release candidate. The feeds get published releases only, because a "
                       "person installing from them has no way to see the difference, and a candidate has "
                       "no installer" % tag])
    found = re.fullmatch(r"v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)", tag or "")
    if not found:
        raise Refused(["%r is not a release tag. Pass it the way the release is tagged, for example v0.3.0"
                       % tag])
    return tag[1:]


def release_date(version):
    """The date the changelog gives the version - one reader, not a second answer."""
    found = re.search(r"^## \[%s\] - (\d{4}-\d{2}-\d{2})\s*$" % re.escape(version), read_text(CHANGELOG), re.M)
    if not found:
        raise Refused(["CHANGELOG.md has no dated section for %s. Package a version that has been released"
                       % version])
    return found.group(1)


def project():
    """The publisher, the addresses and the licence, from the workspace Cargo.toml."""
    publisher, homepage = build_msi.workspace_words()
    workspace = contents.load_workspace()
    found = re.fullmatch(r"https://github\.com/([^/\s]+/[^/\s]+)", workspace.repository)
    if not found:
        raise Refused(["the workspace Cargo.toml names the repository %s, and the packages download from "
                       "a release on github.com" % workspace.repository])
    return publisher, homepage.rstrip("/") + "/", workspace.repository, found.group(1), workspace.licence


def copyright_line(publisher):
    """The copyright both programs carry in their file details, from the one year
    file_details.rs names, in ASCII for the feeds."""
    found = re.search(r'^const FIRST_YEAR: &str = "(\d{4})";$', read_text(FILE_DETAILS), re.M)
    if not found:
        raise Refused(["%s does not name FIRST_YEAR, the year the copyright names" % contents.relative(FILE_DETAILS)])
    return "Copyright (C) %s %s" % (found.group(1), publisher)


# A release's checksum file is under a kilobyte - 907 bytes for v0.3.0. Anything
# near this is another file passed by mistake, an archive for instance, and
# reading stops one byte past it.
SUMS_LIMIT = 1024 * 1024
SUMS_HINT = "Download verify-SHA256SUMS.txt from the release you are packaging and pass that"


def read_sums(path):
    """{file name: lowercase sha256} from a checksum file, in any of its spellings.

    sha256sum writes '<hash>  <name>' in text mode and '<hash> *<name>' in binary
    mode, and that star is not part of the name. A file saved on Windows may
    carry a byte order mark and CRLF. None of that may reach an address."""
    # Opened once and read at most one byte past the limit, from a regular file
    # only: a size asked for first and a read made after could see two
    # different files, and a pipe or a device has no size to ask. A folder is
    # asked first, because Windows refuses to open one with "Permission denied",
    # which would name the wrong cause.
    if os.path.isdir(path):
        raise Refused(["%s is not a file. %s" % (path, SUMS_HINT)])
    try:
        with open(path, "rb") as handle:
            if not stat.S_ISREG(os.fstat(handle.fileno()).st_mode):
                raise Refused(["%s is not a file. %s" % (path, SUMS_HINT)])
            data = handle.read(SUMS_LIMIT + 1)
    except OSError as err:
        raise Refused(["cannot read the checksum file %s: %s. %s" % (path, err.strerror or err, SUMS_HINT)])
    if len(data) > SUMS_LIMIT:
        raise Refused(["%s is larger than %d bytes, so it is not a release's checksum file. %s"
                       % (path, SUMS_LIMIT, SUMS_HINT)])
    try:
        text = data.decode("utf-8-sig").replace("\r\n", "\n")
    except UnicodeDecodeError:
        raise Refused(["%s is not UTF-8 text, so it is not a release's checksum file. %s" % (path, SUMS_HINT)])
    sums = {}
    for number, line in enumerate(text.split("\n"), 1):
        if not line.strip():
            continue
        found = re.fullmatch(r"([0-9A-Fa-f]{64}) [ *](\S.*)", line.strip())
        if not found:
            raise Refused(["%s line %d is not '<sha256>  <file>': %r" % (path, number, line)])
        sums[found.group(2).strip()] = found.group(1).lower()
    return sums


def released_files(sums, version, path, found):
    """{name: sha256} of the three files the packages download, or a refusal
    naming each one the checksum file has no line for."""
    names = (found.window.name(version), found.cli.name(version), found.installer.file_name(version))
    problems = []
    for name in names:
        if name in sums:
            continue
        pattern = re.escape(name).replace(re.escape(version), r"[^_]+")
        other = sorted(n for n in sums if re.fullmatch(pattern, n))
        problems.append("%s has no line for %s.%s" % (path, name, (
            " It lists %s - that is the checksum file of another release." % other[0] if other else
            " Every release since v0.2.0 publishes it, so this is not a release's checksum file.")))
    if problems:
        raise Refused(problems)
    return {name: sums[name] for name in names}


PRODUCT_CODE = re.compile(r"\{[0-9A-F]{8}-[0-9A-F]{4}-[0-9A-F]{4}-[0-9A-F]{4}-[0-9A-F]{12}\}")


def product_code(path, sha256, version, product):
    """The ProductCode of the published installer, read through msi.dll, after
    the file is held to its line in the checksum file and its database to this
    tree. A file that is not the release's installer is refused before a single
    value is taken from it."""
    try:
        digest = contents.sha256_of(path)
    except OSError as err:
        raise Refused(["cannot read the installer %s: %s. Download the installer of the release you are "
                       "packaging and pass that" % (path, err.strerror or err)])
    if digest != sha256:
        raise Refused(["%s is not the installer the checksum file names (its sha256 is %s, and the file "
                       "lists %s). Download the installer of the release you are packaging and pass that"
                       % (path, digest, sha256)])
    properties = dict(build_msi.database_rows(path, "SELECT `Property`, `Value` FROM `Property`", 2, "Property"))
    problems = []
    for name, wanted in (("UpgradeCode", "{%s}" % build_msi.UPGRADE_CODE), ("ProductVersion", version),
                         ("ProductName", product)):
        if properties.get(name) != wanted:
            problems.append("the installer says %s = %r, and this tree says %r" % (name, properties.get(name), wanted))
    code = properties.get("ProductCode", "")
    if not PRODUCT_CODE.fullmatch(code):
        problems.append("the installer's ProductCode %r is not a code in braces" % code)
    if problems:
        raise Refused(problems)
    return code


# --------------------------------------------------------------------------- #
# the values, and filling the templates
# --------------------------------------------------------------------------- #

def values(package, version, tag, files, found, code):
    """Every placeholder a template may use, for one package. `code` is the
    installer's ProductCode, absent when WinGet is not rendered."""
    publisher, homepage, repo_url, repo, licence = project()
    download = "%s/releases/download/%s/" % (repo_url, tag)
    window_zip, cli_zip = found.window.name(version), found.cli.name(version)
    msi = found.installer.file_name(version)
    table = {
        "VERSION": version,
        "WINGET_SCHEMA": WINGET_SCHEMA,
        "WINGET_ID": package.winget_id,
        "CHOCO_ID": package.choco_id,
        "TITLE": package.title.format(product=found.installer.product),
        "MONIKER": package.moniker,
        "PUBLISHER": publisher,
        "LICENSE": licence,
        "COPYRIGHT": copyright_line(publisher),
        "REPO_URL": repo_url,
        "ISSUES_URL": repo_url + "/issues",
        "PROJECT_URL": homepage,
        "DOCS_URL": homepage + "docs/",
        "LICENSE_URL": "%s/blob/%s/LICENSE" % (repo_url, tag),
        "RELEASE_NOTES_URL": "%s/releases/tag/%s" % (repo_url, tag),
        "PACKAGE_SOURCE_URL": "%s/tree/main/packaging/chocolatey" % repo_url,
        # A CDN pinned to the tag, both halves load-bearing: Chocolatey's
        # moderation refuses raw.githubusercontent.com and github.com/.../raw
        # alike (rule CPMR0033), and an icon on a branch keeps changing under a
        # package that is already approved.
        "ICON_URL": "https://cdn.jsdelivr.net/gh/%s@%s/%s" % (repo, tag, ICON),
        "RELEASE_DATE": release_date(version),
        "WINDOW_PROGRAM": found.window.program,
        "WINDOW_EXE": found.window.program + ".exe",
        "CLI_PROGRAM": found.cli.program,
        "CLI_EXE": found.cli.program + ".exe",
        "WINDOW_ZIP_URL": download + window_zip,
        "WINDOW_ZIP_SHA256": files[window_zip].upper(),
        "CLI_ZIP_URL": download + cli_zip,
        "CLI_ZIP_SHA256": files[cli_zip].upper(),
        "MSI_URL": download + msi,
        "MSI_SHA256": files[msi].upper(),
        # In braces, as Windows Installer spells a code and as WinGet compares it.
        "MSI_UPGRADE_CODE": "{%s}" % build_msi.UPGRADE_CODE,
        "SHORT_DESCRIPTION": SHORT[package.kind],
        "DESCRIPTION": DESCRIPTION,
        "WINGET_HOW_TO_START": how_to_start(package, "winget", found.window.program, found.cli.program),
        "CHOCO_HOW_TO_START": how_to_start(package, "chocolatey", found.window.program, found.cli.program),
        "WINGET_TAGS": "\n".join("- " + t for t in TAGS + (KIND_TAG[package.kind],)),
        "CHOCO_TAGS": " ".join(TAGS + (KIND_TAG[package.kind],)),
    }
    if code:
        table["MSI_PRODUCT_CODE"] = code
    return table


# What a value substituted inside a line must not hold, by the file it lands in.
# Each would still render, and each would hand the feed a file that means
# something else: a quote ends a PowerShell string early, a bracket or an
# ampersand is markup in the nuspec and ]]> ends its description, and a colon
# followed by a space or a space followed by a hash turns the rest of a plain
# YAML value into a key or a comment.
BREAKS = (
    (".ps1", "'", "a single quote ends the PowerShell string it sits in"),
    (".nuspec", "<", "the nuspec reads it as markup"),
    (".nuspec", ">", "the nuspec reads it as markup"),
    (".nuspec", "&", "the nuspec reads it as markup"),
    (".yaml", ": ", "YAML reads the rest as a key"),
    (".yaml", " #", "YAML reads the rest as a comment"),
)

# The first characters that make a plain YAML value something other than text.
YAML_INDICATORS = "-?:,[]{}#&*!|>'\"%@`"


def breaking(name, value, quoted):
    """Why `value` breaks the file `name` when written into a line, or nothing.
    A value between single quotes in a YAML file is broken by a quote only."""
    if name.endswith(".yaml") and quoted:
        return "a single quote ends the YAML string it sits in" if "'" in value else ""
    for suffix, text, why in BREAKS:
        if name.endswith(suffix) and text in value:
            return why
    if name.endswith(".yaml") and value[:1] in YAML_INDICATORS:
        return "YAML reads its first character as syntax"
    return ""


def render(text, table, name):
    """Fill every placeholder, or refuse. An unknown one is an error, never an empty string.

    A placeholder alone on its line may hold several lines, and each takes the
    indentation the placeholder had - that is how a YAML block stays a block and
    the nuspec's description keeps its paragraphs. A placeholder inside a line
    takes one line only."""
    out = []
    for line in text.split("\n"):
        for key in PLACEHOLDER.findall(line):
            if key not in table:
                raise Refused(["%s uses {{%s}}, which the renderer does not know" % (name, key)])
        alone = re.fullmatch(r"( *)\{\{([A-Z0-9_]+)\}\}", line)
        if alone:
            value = table[alone.group(2)]
            if name.endswith(".nuspec") and "]]>" in value:
                raise Refused(["{{%s}} holds ]]>, which ends the nuspec's description" % alone.group(2)])
            out += [alone.group(1) + part if part else "" for part in value.split("\n")]
            continue
        for key in PLACEHOLDER.findall(line):
            if "\n" in table[key]:
                raise Refused(["%s puts the several lines of {{%s}} inside a line" % (name, key)])
            why = breaking(name, table[key], "'{{%s}}'" % key in line)
            if why:
                raise Refused(["{{%s}} holds %r, which breaks %s: %s" % (key, table[key], name, why)])
        out.append(PLACEHOLDER.sub(lambda m: table[m.group(1)], line))
    return "\n".join(out)


def check_script(text, name):
    """A package script is read by Windows PowerShell 5.1, which takes a file
    without a byte order mark as the machine's ANSI code page - so a script that
    is not ASCII reaches it changed."""
    for number, line in enumerate(text.split("\n"), 1):
        if any(ord(c) > 127 for c in line):
            raise Refused(["%s line %d is not ASCII: %r" % (name, number, line)])


FEEDS = ("winget", "chocolatey")


def templates(kind, feeds=FEEDS):
    """(template path, output path relative to the package) for one kind of package.

    A template named name.<kind>.ext.in belongs to that kind only, and renders to
    name.ext. One named name.ext.in belongs to every package."""
    kinds = {p.kind for p in PACKAGES}
    for feed in feeds:
        for base, _, names in sorted(os.walk(os.path.join(TEMPLATES, feed))):
            for file in sorted(names):
                if not file.endswith(TEMPLATE_SUFFIX):
                    continue
                parts = file[:-len(TEMPLATE_SUFFIX)].split(".")
                owner = parts[1] if len(parts) > 2 and parts[1] in kinds else None
                if owner not in (None, kind):
                    continue
                target = ".".join(p for i, p in enumerate(parts) if not (i == 1 and owner))
                relative = os.path.relpath(os.path.join(base, target), TEMPLATES)
                yield os.path.join(base, file), relative.replace(os.sep, "/")


def destination(package, relative, version):
    """Where a rendered file goes: WinGet's in the folder layout winget-pkgs uses,
    so the three files of a pull request can be copied across as they are."""
    feed, _, rest = relative.partition("/")
    if feed == "winget":
        kind = rest[:-len(".yaml")]
        name = package.winget_id + ("" if kind == "version" else "." + kind) + ".yaml"
        parts = package.winget_id.split(".")
        return "/".join(["winget", "manifests", parts[0][0].lower()] + parts + [version, name])
    if rest == "package.nuspec":
        rest = package.choco_id + ".nuspec"
    return "/".join(["chocolatey", package.choco_id, rest])


# --------------------------------------------------------------------------- #
# writing, all or nothing
# --------------------------------------------------------------------------- #

def check_out(out):
    """--out resolved, outside the repository and empty or absent, or a refusal.
    Resolved, so that a link or a junction into the tree does not get round it."""
    out = os.path.realpath(out)
    root = os.path.normcase(os.path.realpath(ROOT))
    if os.path.normcase(out) == root or os.path.normcase(out).startswith(root + os.sep):
        raise Refused(["--out %s is inside the repository. Rendered packages are not source - put them "
                       "somewhere outside it" % out])
    if os.path.exists(out):
        try:
            holds = not os.path.isdir(out) or bool(os.listdir(out))
        except OSError as err:
            raise Refused(["cannot look inside --out %s: %s. Pass a new directory, or one you can read and "
                           "write" % (out, err.strerror or err)])
        if holds:
            raise Refused(["--out %s already holds something. Nothing is overwritten - pass an empty or new "
                           "directory" % out])
    return out


def missing_folders(path):
    """The folders of path that do not exist yet, deepest first - what this run
    will have made, and the most a failed run may take away again."""
    made = []
    while not os.path.exists(path):
        made.append(path)
        up = os.path.dirname(path)
        if up == path:
            break
        path = up
    return made


def remove_empty(folders):
    """Remove the folders this run made, deepest first, stopping at the first one
    that is not empty - it then holds something that is not ours."""
    for folder in folders:
        try:
            os.rmdir(folder)
        except OSError:
            return


def build(tag, sums_path, out, msi=None, feeds=FEEDS):
    """Render every package of the given feeds into out, all or nothing."""
    version = parse_tag(tag)
    out = check_out(out)
    found = programs()
    files = released_files(read_sums(sums_path), version, sums_path, found)
    if not os.path.isfile(os.path.join(ROOT, ICON)):
        raise Refused(["the icon %s is not in the repository" % ICON])
    code = None
    if "winget" in feeds:
        if not msi:
            raise Refused(["WinGet names the installer by its ProductCode, which is read out of the published "
                           "installer - pass it with --msi, or render Chocolatey alone with --only chocolatey"])
        code = product_code(msi, files[found.installer.file_name(version)], version, found.installer.product)

    parent = os.path.dirname(out)
    made = missing_folders(parent)
    try:
        os.makedirs(parent, exist_ok=True)
        work = tempfile.mkdtemp(prefix=".packages-", dir=parent)
    except OSError as err:
        remove_empty(made)
        raise Refused(["cannot create a working folder in %s: %s. Check that you can write there, or pass "
                       "another --out" % (parent, err.strerror or err)])
    finished = False
    try:
        # What every template uses, read from all of them whichever feeds are
        # rendered, so a run for one feed does not call the other's values unused.
        used = {key for package in PACKAGES for source, _ in templates(package.kind)
                for key in PLACEHOLDER.findall(read_text(source))}
        known = set()
        for package in PACKAGES:
            table = values(package, version, tag, files, found, code)
            known |= set(table)
            for source, relative in templates(package.kind, feeds):
                rendered = render(read_text(source), table, relative)
                if relative.endswith(".ps1"):
                    check_script(rendered, relative)
                target = os.path.join(work, *destination(package, relative, version).split("/"))
                os.makedirs(os.path.dirname(target), exist_ok=True)
                with open(target, "w", encoding="utf-8", newline="\n") as handle:
                    handle.write(rendered)
        if known - used:
            raise Refused(["no template uses %s any more - take it out of the renderer"
                           % ", ".join(sorted(known - used))])
        if os.path.isdir(out):
            os.rmdir(out)
        os.rename(work, out)
        finished = True
    except OSError as err:
        raise Refused(["cannot write the packages to %s: %s. Check that you can write there, or pass another "
                       "--out. Nothing was left behind" % (out, err.strerror or err)])
    finally:
        # A refusal reaches here too: nothing half rendered, and no folder this
        # run made, outlives a run that failed.
        if os.path.isdir(work):
            shutil.rmtree(work)
        if not finished:
            remove_empty(made)
    return out


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--tag", required=True, help="the published release, for example v0.3.0")
    parser.add_argument("--sums", required=True, help="that release's verify-SHA256SUMS.txt")
    parser.add_argument("--msi", help="that release's installer, for the ProductCode WinGet needs")
    parser.add_argument("--out", required=True, help="an empty or new directory outside the repository")
    parser.add_argument("--only", choices=FEEDS, help="render the packages of one feed")
    args = parser.parse_args(argv)
    try:
        out = build(args.tag, args.sums, args.out, args.msi, (args.only,) if args.only else FEEDS)
    except Refused as refusal:
        print("build_packages: refused:", file=sys.stderr)
        for problem in refusal.problems:
            print("  - " + problem.replace("\n", "\n    "), file=sys.stderr)
        return 1
    for base, _, names in sorted(os.walk(out)):
        for file in sorted(names):
            print(os.path.join(base, file))
    return 0


if __name__ == "__main__":
    sys.exit(main())
