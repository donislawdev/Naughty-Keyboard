#!/usr/bin/env python3
"""Build the Windows installer of one release from the signed archives of its platform.

    python .github/scripts/build_msi.py --tag v0.1.0 --archives <dir> --out-dir <dir>
    python .github/scripts/build_msi.py --tag v0.1.0-dev.2 --trial 0.1.0 --archives <dir> --out-dir <dir>
    python .github/scripts/build_msi.py --tag v0.1.0 --check
    python .github/scripts/build_msi.py --tag v0.1.0 --source-only
    python .github/scripts/build_msi.py --tag v0.1.0 --product-name

One installer carries both programs, the command line and the palette, for every
account on the machine. It is built from the SIGNED archives of the platform the
[installer] table of archives.toml names, because the signature is on the
programs inside them: an installer made before the card signed them would carry
unsigned copies. So a release builds it in phase B, in sign_release.py, on the
machine with the card, and never in release.yml. Phase A builds unsigned ones
under trial versions, in try_installer.py, to ask a clean machine what the
installer does.

Every value comes from the tree this script sits in: the template
.github/release/nkb-setup.wxs, the [installer] table, the workspace Cargo.toml
and the icon. sign_release.py runs it from the tree of the TAG, so the installer
is built from what was tagged, whatever the checkout happens to stand on.

A release candidate gets no installer. Windows Installer reads only the three
numbers of a version, so a candidate's installer would carry the release's own
version. --trial gives such a build a version of its own and a file name no
release can carry, for an installer that is tried and never published.

--check asks only whether this machine can build it (the tag, the template, the
icon, WiX), so sign_release.py can refuse before the card signs anything.
--source-only prints the rendered installer source and builds nothing.
--product-name prints the name the installer carries, which its signature names.

Every refusal about the inputs comes before WiX is looked for, so the tests can
ask each of them on a machine without WiX. After the build the database itself
is read back through msi.dll, the library Windows Installer reads it with, and
an installer that carries anything but our files and Windows Installer's own
tables is refused before it is moved where anybody would find it.

Exit codes: 0 done, 1 refused, and the message says which input and why.
Nothing here signs or publishes anything.
"""
import argparse
import hashlib
import os
import re
import shutil
import subprocess
import sys
import tempfile
import xml.etree.ElementTree as ElementTree
import zipfile

import contents
import release
from contents import ROOT, Refused

TEMPLATE = os.path.join(ROOT, ".github", "release", "nkb-setup.wxs")
ICON = os.path.join(ROOT, "crates", "nkb-gui", "assets", "edamame.ico")
PLACEHOLDER = re.compile(r"\{\{([A-Z_]+)\}\}")

# The product, not a build, and it never changes - see the comment on it in the
# template. Generated once on 2026-10-14 and pinned a second time by the tests.
UPGRADE_CODE = "C64D1A47-17D7-43FE-BF82-76CC3DBA4981"

# The WiX this project builds with. Another version builds another package from
# the same source, so it is refused rather than used. 5.0.2 is the last release
# before the Open Source Maintenance Fee of version 6, the version the sibling
# projects of this author build with, and the version release.yml installs.
WIX_VERSION = "5.0.2"

# Each archive carries the notices of its own program under this name, and the
# installer puts both programs in one folder, so each one keeps its notices
# under a name that says which program they belong to.
NOTICES_INSTALLED = "THIRD-PARTY-NOTICES-%s.txt"

# The largest version Windows Installer holds: 255 for the first two numbers,
# 65535 for the third.
LIMITS = (255, 255, 65535)


def say(text):
    print(text, flush=True)


# --------------------------------------------------------------------------- #
# the version, and the name of the file
# --------------------------------------------------------------------------- #

def fitting(version, what):
    """Three numbers Windows Installer can hold, or a refusal."""
    found = re.fullmatch(r"(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)", version or "")
    if not found:
        raise Refused(["%s %r is not three numbers, such as 0.1.0" % (what, version)])
    if any(int(n) > limit for n, limit in zip(found.groups(), LIMITS)):
        raise Refused(["%s %s does not fit an installer version, which holds at most 255 for the first "
                       "two numbers and 65535 for the third" % (what, version)])
    return version


def installer_version(tag, trial=None):
    """The version the installer carries: the tag's, or the trial's.

    The tag has to name the workspace's version either way, because it is also
    what the archives are named by."""
    contents.version_of_tag(tag, contents.load_workspace())
    if trial is not None:
        return fitting(trial, "--trial")
    if release.is_candidate(tag):
        raise Refused(["%s is a release candidate, and a candidate gets no installer. Windows Installer "
                       "reads only the three numbers, so it would take %s for the release itself and the "
                       "release would not replace it. A build that is only tried takes --trial."
                       % (tag, tag[1:].split("-")[0])])
    return fitting(tag[1:], "the tag's version")


def output_name(tag, trial=None):
    """The file the installer is written to. A trial is named so that no release
    can ever carry its name, whatever its tag."""
    installer = contents.load_installer()
    version = tag[1:]
    return installer.file_name(version if trial is None else "%s-trial-%s" % (version, trial))


# --------------------------------------------------------------------------- #
# the source
# --------------------------------------------------------------------------- #

def workspace_words(root=ROOT):
    """The publisher and the addresses, from the workspace Cargo.toml, where the
    programs read them for their own file details."""
    package = contents.read_toml(os.path.join(root, "Cargo.toml")).get("workspace", {}).get("package", {})
    authors, homepage = package.get("authors") or [], package.get("homepage", "")
    problems = []
    if len(authors) != 1 or not authors[0]:
        problems.append("the workspace Cargo.toml names %d authors, and the installer names one publisher"
                        % len(authors))
    if not homepage.startswith("https://"):
        problems.append("the workspace Cargo.toml has no https homepage for Programs and Features to show")
    if problems:
        raise Refused(problems)
    return authors[0], homepage


def values(tag, version):
    """Every placeholder the template may use."""
    installer = contents.load_installer()
    publisher, homepage = workspace_words()
    repository = contents.load_workspace().repository
    return {
        "PRODUCT": installer.product,
        "PUBLISHER": publisher,
        "VERSION": version,
        "UPGRADE_CODE": UPGRADE_CODE,
        "PROJECT_URL": homepage,
        "ISSUES_URL": repository + "/issues",
        "RELEASE_URL": "%s/releases/tag/%s" % (repository, tag),
        "SHORTCUT_PROGRAM": installer.shortcut + ".exe",
    }


def escaped(value):
    """A value as it may stand inside an XML attribute."""
    return (value.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")
            .replace('"', "&quot;"))


def render(text, table):
    """The template with every placeholder filled. A placeholder nobody gives a
    value, a value the template no longer asks for, and a value that would break
    the line it lands in are refused rather than built half right."""
    asked = set(PLACEHOLDER.findall(text))
    problems = []
    if asked - set(table):
        problems.append("the template asks for %s, which build_msi.py does not give" % ", ".join(sorted(asked - set(table))))
    if set(table) - asked:
        problems.append("the template uses no %s any more - take it out of build_msi.py"
                        % ", ".join(sorted(set(table) - asked)))
    for key, value in sorted(table.items()):
        if not value or any(c in value for c in "\r\n\t"):
            problems.append("the value of %s is empty or spans lines: %r" % (key, value))
    if problems:
        raise Refused(problems)
    rendered = PLACEHOLDER.sub(lambda m: escaped(table[m.group(1)]), text)
    try:
        ElementTree.fromstring(rendered.encode("utf-8"))
    except ElementTree.ParseError as error:
        raise Refused(["the rendered installer source is not XML: %s" % error]) from None
    return rendered


def source(tag, version):
    """The installer source for one build, every placeholder filled."""
    if not os.path.isfile(TEMPLATE):
        raise Refused(["there is no template at %s. A tree from before the installer existed has none, "
                       "and its release cannot carry one" % contents.relative(TEMPLATE)])
    return render(contents.read_text(TEMPLATE), values(tag, version))


# --------------------------------------------------------------------------- #
# what the installer puts down
# --------------------------------------------------------------------------- #

def payload_files(archives_dir, tag):
    """installed name -> bytes, out of the archives of the installer's platform.

    Each archive has to hold exactly what release.py packs into it, so an
    archive somebody else made, or one that lost a file, is refused rather than
    installed. A file both archives hold goes in once, and only when both hold
    the same bytes. One name means one file to Windows, so names are compared
    folded to one case: LICENSE and License would be the same file there."""
    installer = contents.load_installer()
    version = tag[1:]
    archives = [a for a in contents.load_archives() if (a.os, a.arch) == (installer.os, installer.arch)]
    found = {}
    came_from = {}
    problems = []
    for archive in archives:
        name = archive.name(version)
        path = os.path.join(archives_dir, name)
        if not os.path.isfile(path):
            problems.append("%s holds no %s. Pass the folder that holds the archives of %s" % (archives_dir, name, tag))
            continue
        try:
            with zipfile.ZipFile(path) as packed:
                entries = {info.filename: info for info in packed.infolist()}
                wanted = release.expected_names(archive)
                if sorted(entries) != wanted:
                    problems.append("%s holds %s, and an archive of %s holds %s"
                                    % (name, sorted(entries), archive.program, wanted))
                    continue
                for entry_name, info in sorted(entries.items()):
                    data = packed.read(info)
                    installed = NOTICES_INSTALLED % archive.program if entry_name == contents.NOTICES_NAME else entry_name
                    key = installed.casefold()
                    if key in came_from:
                        first, first_name = came_from[key]
                        if found[first_name] != data:
                            problems.append("%s and %s both hold %s, one file on Windows, and not the same bytes. "
                                            "One installer carries one copy - look at how the release built them"
                                            % (first, name, installed))
                        continue
                    came_from[key] = (name, installed)
                    found[installed] = data
        except zipfile.BadZipFile as error:
            problems.append("%s is not a zip archive it can read (%s)" % (path, error))
    if not archives:
        problems.append("archives.toml has no archive for %s %s" % (installer.os, installer.arch))
    if problems:
        raise Refused(problems)
    return found


def write_payload(files, into):
    for name, data in sorted(files.items()):
        with open(os.path.join(into, name), "wb") as handle:
            handle.write(data)


def digests(files):
    return {name: hashlib.sha256(data).hexdigest() for name, data in files.items()}


# --------------------------------------------------------------------------- #
# WiX, and the database it writes
# --------------------------------------------------------------------------- #

def find_wix(which=shutil.which, run=subprocess.run, home=None):
    """The wix command at the pinned version, or a refusal with the command that
    installs it. `dotnet tool install --global` puts it in .dotnet/tools, which
    is not always on PATH."""
    tools = os.path.join(home or os.path.expanduser("~"), ".dotnet", "tools")
    found = which("wix") or which("wix", path=tools)
    install = ("    dotnet tool install --global wix --version %s\n"
               "It runs on .NET - install the .NET SDK first if there is no dotnet command." % WIX_VERSION)
    if not found:
        raise Refused(["WiX is not installed, and the installer is built with it. Install the version "
                       "this project builds with:\n" + install])
    said = run([found, "--version"], capture_output=True, text=True)
    version = said.stdout.strip().split("+")[0]
    if said.returncode != 0 or version != WIX_VERSION:
        raise Refused(["%s says it is version %r, and the installer is built with %s - another version "
                       "builds another package from the same source. Replace it:\n"
                       "    dotnet tool uninstall --global wix\n%s" % (found, version, WIX_VERSION, install)])
    return found


def database_rows(path, query, fields, table):
    """The rows one query returns from an installer database, each a tuple of
    its fields as text, read through msi.dll. A table the database does not
    have answers with no rows, which is the answer for "no custom action" too.

    Every handle is closed on every path: a view or a record left open keeps
    the file open, and the installer could then not be moved out of staging."""
    if not sys.platform.startswith("win"):
        raise Refused(["an installer database is read through msi.dll, which is on Windows"])
    import ctypes
    from ctypes import wintypes
    msi = ctypes.WinDLL("msi")
    handle = wintypes.DWORD
    msi.MsiOpenDatabaseW.argtypes = [wintypes.LPCWSTR, wintypes.LPCWSTR, ctypes.POINTER(handle)]
    msi.MsiDatabaseIsTablePersistentW.argtypes = [handle, wintypes.LPCWSTR]
    msi.MsiDatabaseOpenViewW.argtypes = [handle, wintypes.LPCWSTR, ctypes.POINTER(handle)]
    msi.MsiViewExecute.argtypes = [handle, handle]
    msi.MsiViewFetch.argtypes = [handle, ctypes.POINTER(handle)]
    msi.MsiRecordGetStringW.argtypes = [handle, wintypes.UINT, wintypes.LPWSTR, ctypes.POINTER(wintypes.DWORD)]
    msi.MsiViewClose.argtypes = [handle]
    msi.MsiCloseHandle.argtypes = [handle]
    no_more_items, more_data, table_missing = 259, 234, 2

    database, view, rows = handle(0), handle(0), []
    try:
        # A null persistence mode is MSIDBOPEN_READONLY.
        if msi.MsiOpenDatabaseW(path, None, ctypes.byref(database)) != 0:
            raise Refused(["msi.dll cannot open %s as an installer database" % path])
        if msi.MsiDatabaseIsTablePersistentW(database, table) == table_missing:
            return rows
        if msi.MsiDatabaseOpenViewW(database, query, ctypes.byref(view)) != 0:
            raise Refused(["%s does not answer %r" % (os.path.basename(path), query)])
        if msi.MsiViewExecute(view, 0) != 0:
            raise Refused(["%s cannot run %r" % (os.path.basename(path), query)])
        while True:
            record = handle(0)
            answer = msi.MsiViewFetch(view, ctypes.byref(record))
            if answer == no_more_items:
                break
            if answer != 0:
                raise Refused(["%s stopped answering %r (%d)" % (os.path.basename(path), query, answer)])
            try:
                row = []
                for field in range(1, fields + 1):
                    size = wintypes.DWORD(0)
                    if msi.MsiRecordGetStringW(record, field, "", ctypes.byref(size)) not in (0, more_data):
                        raise Refused(["%s gave a field it cannot read" % os.path.basename(path)])
                    size.value += 1
                    text = ctypes.create_unicode_buffer(size.value)
                    if msi.MsiRecordGetStringW(record, field, text, ctypes.byref(size)) != 0:
                        raise Refused(["%s gave a field it cannot read" % os.path.basename(path)])
                    row.append(text.value)
                rows.append(tuple(row))
            finally:
                msi.MsiCloseHandle(record)
    finally:
        if view.value:
            msi.MsiViewClose(view)
            msi.MsiCloseHandle(view)
        if database.value:
            msi.MsiCloseHandle(database)
    return rows


def database_problems(read, version, product, names):
    """What is wrong with a built installer, asked of its database. `read` is
    database_rows for one file."""
    problems = []
    properties = dict(read("SELECT `Property`, `Value` FROM `Property`", 2, "Property"))
    if len(properties) < 5:
        return ["read %d properties out of the installer, so this check is not reading it" % len(properties)]
    for name, wanted in (("ProductVersion", version), ("UpgradeCode", "{%s}" % UPGRADE_CODE),
                         ("ProductName", product), ("MSIRESTARTMANAGERCONTROL", "Disable"),
                         ("ARPNOREPAIR", "1"), ("ARPNOMODIFY", "1")):
        if properties.get(name) != wanted:
            problems.append("the installer says %s = %r, and it has to say %r" % (name, properties.get(name), wanted))
    for table in ("CustomAction", "Binary"):
        found = read("SELECT * FROM `%s`" % table, 1, table)
        if found:
            problems.append("the installer carries %d row(s) in %s (%s). It carries our files and Windows "
                            "Installer's own tables, and no code of its own or of WiX's"
                            % (len(found), table, ", ".join(sorted(r[0] for r in found))))
    files = [r[0].split("|")[-1] for r in read("SELECT `FileName` FROM `File`", 1, "File")]
    if sorted(n.casefold() for n in files) != sorted(n.casefold() for n in names):
        problems.append("the installer lays down %s, and the archives hold %s" % (sorted(files), sorted(names)))
    sequence = dict(read("SELECT `Action`, `Sequence` FROM `InstallExecuteSequence`", 2, "InstallExecuteSequence"))
    initialize, remove = sequence.get("InstallInitialize"), sequence.get("RemoveExistingProducts")
    if initialize is None or remove is None or int(remove) != int(initialize) + 1:
        problems.append("the old version is removed at %s and the install begins at %s. It is removed "
                        "right after the install begins, so a rebuild of a version replaces its files"
                        % (remove, initialize))
    return problems


def product_code(path):
    """The ProductCode of a built installer, which is new with every build."""
    found = [v for k, v in database_rows(path, "SELECT `Property`, `Value` FROM `Property`", 2, "Property")
             if k == "ProductCode"]
    if len(found) != 1:
        raise Refused(["%s has %d ProductCode(s)" % (path, len(found))])
    return found[0]


# --------------------------------------------------------------------------- #
# building
# --------------------------------------------------------------------------- #

def build(tag, archives_dir, out_dir, trial=None, files=None):
    """Write the installer into out_dir, all or nothing, and return its path.

    `files` replaces what the archives hold, for a trial whose files a test has
    changed on purpose."""
    version = installer_version(tag, trial)
    text = source(tag, version)
    if not os.path.isdir(out_dir):
        raise Refused(["--out-dir %s is not a folder. Pass one that exists" % out_dir])
    target = os.path.join(out_dir, output_name(tag, trial))
    if os.path.exists(target):
        raise Refused(["%s is already there. Nothing is overwritten - remove it or pass another --out-dir" % target])
    if not os.path.isfile(ICON):
        raise Refused(["the icon %s is not in the tree" % contents.relative(ICON)])
    files = payload_files(archives_dir, tag) if files is None else files
    wix = find_wix()

    # Beside nothing of the caller's: sign_release.py hands its own folder of
    # files to publish as --out-dir, and a folder left inside it would be
    # published, or would break the checksums written over it.
    staging = tempfile.mkdtemp(prefix="nkb-msi-")
    try:
        payload = os.path.join(staging, "payload")
        os.makedirs(payload)
        write_payload(files, payload)
        wxs = os.path.join(staging, "nkb-setup.wxs")
        with open(wxs, "w", encoding="utf-8", newline="\n") as handle:
            handle.write(text)
        built = os.path.join(staging, os.path.basename(target))
        command = [wix, "build", wxs, "-arch", "x64", "-pdbtype", "none",
                   "-d", "PayloadDir=" + payload, "-d", "IconFile=" + ICON, "-o", built]
        say("    $ " + " ".join(command))
        done = subprocess.run(command)
        if done.returncode != 0 or not os.path.isfile(built):
            raise Refused(["wix build exited %d, and nothing was written to %s. What it said is above"
                           % (done.returncode, out_dir)])
        problems = database_problems(lambda q, n, t: database_rows(built, q, n, t), version,
                                     contents.load_installer().product, list(files))
        if problems:
            raise Refused(problems + ["Nothing was written to %s." % out_dir])
        # Moved in only once it is whole and checked, so a run that fails or is
        # stopped leaves no half written installer where the caller would find it.
        shutil.move(built, target)
    except OSError as error:
        raise Refused(["cannot build the installer: %s. Nothing was written to %s" % (error, out_dir)]) from None
    finally:
        shutil.rmtree(staging, ignore_errors=True)
    return target


def check(tag, trial=None):
    """Whether this machine can build the installer of this tag, asked before
    anything is signed."""
    version = installer_version(tag, trial)
    source(tag, version)
    if not os.path.isfile(ICON):
        raise Refused(["the icon %s is not in the tree" % contents.relative(ICON)])
    return "ready to build %s, version %s, with %s" % (output_name(tag, trial), version, find_wix())


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--tag", required=True, help="the tag the archives are named by, for example v0.1.0")
    parser.add_argument("--trial", help="a version of three numbers for an installer that is only tried")
    parser.add_argument("--archives", help="the folder holding the archives of the installer's platform")
    parser.add_argument("--out-dir", help="the folder the installer is written into")
    modes = parser.add_mutually_exclusive_group()
    modes.add_argument("--check", action="store_true", help="only say whether this machine can build it")
    modes.add_argument("--source-only", action="store_true", help="print the installer source and build nothing")
    modes.add_argument("--product-name", action="store_true", help="print the name the installer carries")
    args = parser.parse_args(argv)
    try:
        if args.source_only:
            sys.stdout.write(source(args.tag, installer_version(args.tag, args.trial)))
        elif args.product_name:
            installer_version(args.tag, args.trial)
            say(contents.load_installer().product)
        elif args.check:
            say(check(args.tag, args.trial))
        else:
            if not args.archives or not args.out_dir:
                raise Refused(["--archives and --out-dir are needed to build"])
            say(build(args.tag, args.archives, args.out_dir, args.trial))
        return 0
    except Refused as refusal:
        print("build_msi: refused:", file=sys.stderr)
        for problem in refusal.problems:
            print("  - " + problem.replace("\n", "\n    "), file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
