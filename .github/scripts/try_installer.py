#!/usr/bin/env python3
"""Ask THIS machine what the Windows installer does, and say what each step left behind.

    python .github/scripts/try_installer.py --tag v0.1.0-dev.7 --archives <dir> --work <dir>
    python .github/scripts/try_installer.py --tag v0.1.0-dev.7 --archives <dir> --work <dir> --palette
    python .github/scripts/try_installer.py --tag v0.1.0-dev.7 --archives <dir> --work <dir> --kit-only
    python try_installer.py --kit <dir>/kit --work <dir>
    python .github/scripts/try_installer.py --cleanup

What an installer does on a machine is not in its source, so the tests of
build_msi.py can only hold the lines the behaviour rests on. This installs it,
upgrades it while nkb is running from the installed folder, installs a rebuild
of the same version over itself, offers it an older version, and uninstalls it
with a file of somebody else's in its folder. After every step it asks the
machine, not the installer, what is there. release.yml runs it on a Windows
runner for every build, and a person runs it on a machine of their own.

The kit
-------
Three installers, built by build_msi.py from the archives of one build, under
trial versions no release can carry:

    old   0.0.1  the archives as they are
    new   0.0.2  both programs and README.md with a few bytes added: the same
                 file version, other bytes
    same  0.0.2  the archives as they are, under the number of new: a rebuild
                 of a version, whose files have to replace those of new

--kit-only builds the kit and stops, with kit.json beside it, which holds
everything a run needs. --kit runs such a kit on a machine that has neither WiX
nor this repository - only this script and the three it imports, build_msi.py,
contents.py and release.py, in one folder.

What it asks
------------
The state it starts from, then after each step: the exit code, Programs and
Features, every file of the installed folder against the archives, the machine
PATH, a new process finding nkb by name, the Start menu entry, and what an
uninstall leaves. --palette also keeps the installed palette running through
the upgrade, which needs a desktop and registers the palette's shortcuts while
it runs.

Every line is ok, FAILED or NOT MEASURED with the reason, and nothing passes by
default. Exit codes: 0 every check ok, 1 something FAILED, 2 the machine was not
clean or an input was wrong and nothing was touched, 3 nothing FAILED and
something was NOT MEASURED.

Safety
------
It refuses to start when the product is installed, when its folder exists or
when the folder is on the machine PATH. It ends only the processes it started.
However it ends, it uninstalls by product code whatever it installed, and
--cleanup does that by hand. It never writes the machine PATH: the installer
adds its entry and removes it, and a PATH that did not come back is a FAILED
line, with the value it had before written into the work folder.
"""
import argparse
import hashlib
import json
import os
import subprocess
import sys
import time

import build_msi
import contents
from contents import Refused

TRIALS = (("old", "0.0.1"), ("new", "0.0.2"), ("same", "0.0.2"))
# Added to both programs and README.md in the kit's middle installer. The
# programs keep their file version and change their bytes, which is the case a
# rebuild of one version is.
CHANGED = b"\0nkb-setup kit\0"
# A run of nkb that stays in progress without writing anywhere: its output goes
# into a pipe nobody reads, and this pack's longest value fills it.
HELD = ("emit", "length-bombs", "--format", "json")
HELD_OUTPUT = 100000
PLANTED = "somebody-elses.txt"
UNINSTALL = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall"
ENVIRONMENT = r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment"
SESSION = r"SYSTEM\CurrentControlSet\Control\Session Manager"
MSI_TIMEOUT = 900
SHORTCUT_SCRIPT = "\n".join([
    "$link = (New-Object -ComObject WScript.Shell).CreateShortcut($env:NKB_LINK)",
    "[pscustomobject]@{",
    "  target = $link.TargetPath",
    "  folder = $link.WorkingDirectory",
    "  description = $link.Description",
    "} | ConvertTo-Json -Compress",
])


class Report:
    """The lines of one run, said as they come and kept for the work folder."""

    def __init__(self):
        self.lines, self.failed, self.unmeasured = [], 0, 0

    def say(self, text):
        print(text, flush=True)
        self.lines.append(text)

    def check(self, ok, what):
        self.say(("ok            " if ok else "FAILED        ") + what)
        if not ok:
            self.failed += 1
        return ok

    def unmeasured_because(self, what, why):
        self.say("NOT MEASURED  %s - %s" % (what, why))
        self.unmeasured += 1

    def note(self, what):
        self.say("note          " + what)

    def code(self):
        return 1 if self.failed else 3 if self.unmeasured else 0


# --------------------------------------------------------------------------- #
# the machine
# --------------------------------------------------------------------------- #

def winreg():
    import winreg as module
    return module


def elevated():
    import ctypes
    return bool(ctypes.windll.shell32.IsUserAnAdmin())


def install_folder(product):
    return os.path.join(os.environ.get("ProgramW6432") or os.environ["ProgramFiles"], product)


def shortcut_path(product):
    return os.path.join(os.environ["ProgramData"], "Microsoft", "Windows", "Start Menu", "Programs",
                        product + ".lnk")


def entries(product):
    """What Programs and Features lists under the product's name: key -> values."""
    reg = winreg()
    found = {}
    with reg.OpenKey(reg.HKEY_LOCAL_MACHINE, UNINSTALL) as root:
        index = 0
        while True:
            try:
                key = reg.EnumKey(root, index)
            except OSError:
                break
            index += 1
            with reg.OpenKey(root, key) as entry:
                values = {}
                number = 0
                while True:
                    try:
                        name, value, _ = reg.EnumValue(entry, number)
                    except OSError:
                        break
                    values[name] = value
                    number += 1
            if values.get("DisplayName") == product:
                found[key] = values
    return found


def machine_path():
    """The machine PATH as written, and its kind (REG_SZ or REG_EXPAND_SZ)."""
    reg = winreg()
    with reg.OpenKey(reg.HKEY_LOCAL_MACHINE, ENVIRONMENT) as key:
        return reg.QueryValueEx(key, "Path")


def user_path():
    reg = winreg()
    try:
        with reg.OpenKey(reg.HKEY_CURRENT_USER, "Environment") as key:
            return reg.QueryValueEx(key, "Path")
    except OSError:
        return "", reg.REG_SZ


def expanded(value, kind):
    reg = winreg()
    return reg.ExpandEnvironmentStrings(value) if kind == reg.REG_EXPAND_SZ else value


def ours_on_path(folder, value):
    wanted = folder.rstrip("\\").casefold()
    return [part for part in value.split(";") if part.rstrip("\\").casefold() == wanted]


def our_key_exists(publisher, product):
    reg = winreg()
    try:
        reg.OpenKey(reg.HKEY_LOCAL_MACHINE, "Software\\%s\\%s" % (publisher, product)).Close()
        return True
    except OSError:
        return False


def pending_renames():
    reg = winreg()
    try:
        with reg.OpenKey(reg.HKEY_LOCAL_MACHINE, SESSION) as key:
            return len([v for v in reg.QueryValueEx(key, "PendingFileRenameOperations")[0] if v])
    except OSError:
        return 0


def tree_digests(folder):
    """relative name -> SHA-256 of every file under a folder, or None without one."""
    if not os.path.isdir(folder):
        return None
    found = {}
    for base, _, files in os.walk(folder):
        for name in files:
            path = os.path.join(base, name)
            found[os.path.relpath(path, folder).replace(os.sep, "/")] = contents.sha256_of(path)
    return found


def carries_a_file_version(path):
    """Whether a program carries a version resource. Windows Installer replaces
    a program that has one only when it is missing or older, and one that has
    none by its hash - so only programs with a version tell an install that
    replaces the files of its version from one that keeps the old ones."""
    import ctypes
    from ctypes import wintypes
    version = ctypes.WinDLL("version")
    version.GetFileVersionInfoSizeW.argtypes = [wintypes.LPCWSTR, ctypes.POINTER(wintypes.DWORD)]
    return version.GetFileVersionInfoSizeW(path, ctypes.byref(wintypes.DWORD(0))) > 0


def powershell():
    for name in ("pwsh", "powershell"):
        for folder in os.environ.get("PATH", "").split(os.pathsep):
            if folder and os.path.isfile(os.path.join(folder, name + ".exe")):
                return os.path.join(folder, name + ".exe")
    raise Refused(["there is no PowerShell to read the Start menu entry with"])


def shortcut_of(path):
    """Where a Start menu entry points, read by the shell that follows it."""
    done = subprocess.run([powershell(), "-NoProfile", "-NonInteractive", "-Command", SHORTCUT_SCRIPT],
                          capture_output=True, text=True, env={**os.environ, "NKB_LINK": path})
    if done.returncode != 0 or not done.stdout.strip():
        return None
    return json.loads(done.stdout)


def new_terminal_environment():
    """The environment a terminal opened now would get: PATH as the machine and
    this account have it written, not as this process inherited it."""
    machine, user = machine_path(), user_path()
    return {**os.environ, "PATH": expanded(*machine) + ";" + expanded(*user)}


def log_text(path):
    """What msiexec wrote into its log, which is UTF-16 with a byte order mark
    on some machines and the system's code page on others."""
    if not os.path.isfile(path):
        return ""
    with open(path, "rb") as handle:
        data = handle.read()
    if data.startswith(b"\xff\xfe"):
        return data[2:].decode("utf-16-le", errors="replace")
    return data.decode("utf-8", errors="replace")


def msiexec(verb, target, log):
    """msiexec with its arguments as a list and a limit on the wait: a quote
    that swallows a file name leaves it waiting on a help window nobody sees."""
    started = time.monotonic()
    # A path with forward slashes is a package msiexec cannot open (1619).
    if os.path.isabs(target):
        target = os.path.normpath(target)
    command = [os.path.join(os.environ["SystemRoot"], "System32", "msiexec.exe"), verb, target,
               "/qn", "/norestart", "/l*v", os.path.normpath(log)]
    try:
        code = subprocess.run(command, timeout=MSI_TIMEOUT).returncode
    except subprocess.TimeoutExpired:
        code = None
    return code, time.monotonic() - started


# --------------------------------------------------------------------------- #
# the kit
# --------------------------------------------------------------------------- #

def changed(files, programs):
    """The files of the kit's middle installer: the programs and README.md with
    bytes added, everything else as it is."""
    altered = dict(files)
    for name in programs + ["README.md"]:
        altered[name] = files[name] + CHANGED
    return altered


KIT = "kit.json"


def build_kit(tag, archives, kit):
    """old, new and same, each in a folder of its own, with what each one lays
    down, and kit.json beside them: everything a run needs, so that a machine
    with neither WiX nor this repository can run the kit."""
    files = build_msi.payload_files(archives, tag)
    programs = sorted(n for n in files if n.endswith(".exe"))
    installer = contents.load_installer()
    built = {}
    for label, trial in TRIALS:
        folder = os.path.join(kit, label)
        os.makedirs(folder)
        laid = changed(files, programs) if label == "new" else files
        path = build_msi.build(tag, archives, folder, trial=trial, files=laid)
        built[label] = {"path": path, "version": trial, "code": build_msi.product_code(path),
                        "files": build_msi.digests(laid)}
    described = {"product": installer.product, "publisher": build_msi.workspace_words()[0],
                 "programs_version": contents.load_workspace().version, "shortcut": installer.shortcut + ".exe",
                 "installers": built}
    # Written with each installer's path relative to the kit, so the kit can
    # move to another machine, and returned with the whole path, because
    # msiexec cannot open a package by a path relative to where it was not
    # started (1619, measured on the first run that used a kit built here).
    with open(os.path.join(kit, KIT), "w", encoding="utf-8", newline="\n") as handle:
        json.dump(dict(described, installers={label: dict(entry, path=os.path.relpath(entry["path"], kit))
                                              for label, entry in built.items()}),
                  handle, indent=1, sort_keys=True)
    return described


def load_kit(kit):
    """A kit built earlier, its installers' paths made whole again."""
    path = os.path.join(kit, KIT)
    if not os.path.isfile(path):
        raise Refused(["%s holds no %s. Build a kit with --kit-only first" % (kit, KIT)])
    with open(path, encoding="utf-8") as handle:
        described = json.load(handle)
    for label, _ in TRIALS:
        entry = described["installers"][label]
        entry["path"] = os.path.join(kit, entry["path"])
        if not os.path.isfile(entry["path"]):
            raise Refused(["the kit has no %s installer at %s" % (label, entry["path"])])
    return described


# --------------------------------------------------------------------------- #
# what is asked after each step
# --------------------------------------------------------------------------- #

class Machine:
    def __init__(self, report, product, publisher, programs_version, shortcut):
        self.report = report
        self.product = product
        self.publisher = publisher
        self.programs_version = programs_version
        self.shortcut = shortcut
        self.folder = install_folder(product)

    def installed(self, kit, laid):
        """One entry, the folder holding exactly what `laid` installs, the folder
        on PATH once, nkb found by a new process, and the Start menu entry."""
        r = self.report
        found = entries(self.product)
        r.check(len(found) == 1, "one entry in Programs and Features (%s)"
                % ", ".join("%s %s" % (k, v.get("DisplayVersion")) for k, v in sorted(found.items())))
        if len(found) == 1:
            key, values = next(iter(found.items()))
            r.check(key == kit["code"], "it is the installer just run (%s)" % key)
            r.check(values.get("DisplayVersion") == kit["version"], "it says %s (%s)"
                    % (kit["version"], values.get("DisplayVersion")))
            r.check(values.get("Publisher") == self.publisher, "the publisher is %s (%s)"
                    % (self.publisher, values.get("Publisher")))
            r.check(values.get("NoModify") == 1 and values.get("NoRepair") == 1,
                    "nothing to modify or repair (NoModify %s, NoRepair %s)" % (values.get("NoModify"), values.get("NoRepair")))
            r.note("about %s, help %s, update %s" % (values.get("URLInfoAbout"), values.get("HelpLink"),
                                                     values.get("URLUpdateInfo")))
        on_disk = tree_digests(self.folder) or {}
        r.check(sorted(on_disk) == sorted(laid), "the folder holds exactly %s (%s)"
                % (", ".join(sorted(laid)), ", ".join(sorted(on_disk)) or "no folder"))
        differ = sorted(n for n in laid if n in on_disk and on_disk[n] != laid[n])
        r.check(not differ and sorted(on_disk) == sorted(laid),
                "every file has the bytes of this installer%s" % (" - not: " + ", ".join(differ) if differ else ""))
        path, _ = machine_path()
        r.check(len(ours_on_path(self.folder, path)) == 1, "the folder is on the machine PATH once (%d)"
                % len(ours_on_path(self.folder, path)))
        self.nkb_by_name()
        self.start_menu()

    def nkb_by_name(self):
        r = self.report
        environment = new_terminal_environment()
        where = subprocess.run(["where.exe", "nkb"], capture_output=True, text=True, env=environment)
        lines = [line.strip() for line in where.stdout.splitlines() if line.strip()]
        ours = os.path.join(self.folder, "nkb.exe").casefold()
        r.check(ours in [line.casefold() for line in lines], "a new terminal finds nkb on PATH (%s)"
                % (", ".join(lines) or "nowhere"))
        if lines and lines[0].casefold() != ours:
            r.unmeasured_because("nkb by name answers from this folder", "another nkb comes first on PATH: %s" % lines[0])
            return
        said = subprocess.run(["cmd.exe", "/d", "/c", "nkb --version"], capture_output=True, text=True,
                              env=environment)
        r.check(said.stdout.strip() == "nkb %s" % self.programs_version,
                "nkb --version, found by name, answers nkb %s (%r)" % (self.programs_version, said.stdout.strip()))

    def start_menu(self):
        r = self.report
        link = shortcut_path(self.product)
        if not r.check(os.path.isfile(link), "the Start menu of every account has %s" % link):
            return
        read = shortcut_of(link)
        if read is None:
            r.unmeasured_because("where the Start menu entry points", "PowerShell could not read it")
            return
        r.check(read.get("target", "").casefold() == os.path.join(self.folder, self.shortcut).casefold(),
                "it starts %s (%s)" % (self.shortcut, read.get("target")))
        r.check(read.get("folder", "").rstrip("\\").casefold() == self.folder.casefold(),
                "it starts in the install folder (%s)" % read.get("folder"))
        r.note("its tooltip says %r" % read.get("description"))


# --------------------------------------------------------------------------- #
# the run
# --------------------------------------------------------------------------- #

def palette_settings(work):
    """A profile of its own for the palette this run starts, with the welcome
    window already closed, so it opens the palette and never the welcome, and
    writes nothing into the profile of whoever runs this."""
    appdata = os.path.join(work, "appdata")
    folder = os.path.join(appdata, "Naughty Keyboard")
    os.makedirs(folder, exist_ok=True)
    with open(os.path.join(folder, "settings.toml"), "w", encoding="utf-8", newline="\n") as handle:
        handle.write("schema = 1\n\n[welcome]\ndone = true\n")
    return appdata


def started(command, **kw):
    return subprocess.Popen(command, stdin=subprocess.DEVNULL, **kw)


STEPS_AFTER = ("an upgrade while nkb runs", "a rebuild of the same version", "an older version refused",
               "the uninstall")


def measure(args, report, kit, machine, started_processes):
    r = report
    work, folder = args.work, machine.folder
    product = machine.product

    r.say("")
    r.say("== install 0.0.1")
    code, took = msiexec("/i", kit["old"]["path"], os.path.join(work, "install-old.log"))
    if not r.check(code == 0, "installed (exit %s, %d s)" % (code, took)):
        # Every step after this one starts from an installed copy, and there is none.
        for step in STEPS_AFTER:
            r.unmeasured_because(step, "the first install failed - its log is install-old.log")
        return
    machine.installed(kit["old"], kit["old"]["files"])
    unversioned = sorted(n for n in kit["old"]["files"] if n.endswith(".exe")
                         and not carries_a_file_version(os.path.join(folder, n)))
    if unversioned:
        r.unmeasured_because("a rebuild of a version replaces programs that carry that version",
                             "%s carry no file version, so Windows Installer compares them by their hash and "
                             "this run cannot tell the two schedules apart. Build the archives from a commit "
                             "whose programs carry one" % ", ".join(unversioned))
    else:
        r.check(True, "both programs carry a file version, so the rebuild below is the case that matters")

    r.say("")
    r.say("== upgrade to 0.0.2 while nkb runs from the installed folder")
    held = started([os.path.join(folder, "nkb.exe")] + list(HELD), stdout=subprocess.PIPE,
                   stderr=subprocess.DEVNULL, creationflags=subprocess.CREATE_NO_WINDOW)
    started_processes.append(held)
    palette = None
    if args.palette:
        palette = started([os.path.join(folder, machine.shortcut)],
                          env={**os.environ, "APPDATA": palette_settings(work)})
        started_processes.append(palette)
    time.sleep(3)
    holding = held.poll() is None
    r.check(holding, "nkb is running from the installed folder before the upgrade")
    if palette is not None:
        r.check(palette.poll() is None, "the palette is running from the installed folder before the upgrade")
    renames = pending_renames()
    code, took = msiexec("/i", kit["new"]["path"], os.path.join(work, "upgrade-held.log"))
    r.check(code == 0, "the upgrade succeeds with nkb running and needs no restart (exit %s, %d s)" % (code, took))
    if holding:
        r.check(held.poll() is None, "nkb is still running - nothing ended it")
        try:
            output, _ = held.communicate(timeout=120)
            r.check(held.returncode == 0 and len(output) > HELD_OUTPUT,
                    "the nkb that was running finished with 0 and all of its output (%s, %d bytes)"
                    % (held.returncode, len(output)))
        except subprocess.TimeoutExpired:
            r.check(False, "the nkb that was running finished within two minutes of the upgrade")
    else:
        r.unmeasured_because("an upgrade while nkb runs", "nkb finished before the upgrade started")
    if palette is not None:
        r.check(palette.poll() is None, "the palette is still running - nothing closed it")
        palette.kill()
        palette.wait(30)
    r.note("restart operations pending: %d before the upgrade, %d after" % (renames, pending_renames()))
    machine.installed(kit["new"], kit["new"]["files"])

    r.say("")
    r.say("== a rebuild of 0.0.2 over 0.0.2, the same version with another ProductCode")
    code, took = msiexec("/i", kit["same"]["path"], os.path.join(work, "same-version.log"))
    r.check(code == 0, "installed (exit %s, %d s)" % (code, took))
    machine.installed(kit["same"], kit["same"]["files"])

    r.say("")
    r.say("== 0.0.1 over 0.0.2")
    log = os.path.join(work, "downgrade.log")
    code, took = msiexec("/i", kit["old"]["path"], log)
    r.check(code not in (0, 3010, None), "refused (exit %s, %d s)" % (code, took))
    r.check("A newer version of %s is already installed" % product in log_text(log),
            "with the sentence the installer carries")
    machine.installed(kit["same"], kit["same"]["files"])

    r.say("")
    r.say("== uninstall, with a file of somebody else's in the folder")
    with open(os.path.join(folder, PLANTED), "w", encoding="utf-8") as handle:
        handle.write("not ours\n")
    code, took = msiexec("/x", kit["same"]["code"], os.path.join(work, "uninstall.log"))
    r.check(code == 0, "uninstalled (exit %s, %d s)" % (code, took))
    r.check(not entries(product), "no entry in Programs and Features")
    r.check(sorted(tree_digests(folder) or {}) == [PLANTED], "the folder holds only the planted file (%s)"
            % ", ".join(sorted(tree_digests(folder) or {})))
    r.check(not os.path.exists(shortcut_path(product)), "no Start menu entry")
    r.check(not our_key_exists(machine.publisher, product), "no key HKLM\\Software\\%s\\%s"
            % (machine.publisher, product))


def run(args):
    report = Report()
    if not sys.platform.startswith("win"):
        report.say("STOP: an installer is tried on Windows")
        return 2
    if args.kit_only:
        return kit_only(report, args)
    if not elevated():
        report.say("STOP: run this from an elevated terminal. An install for every account, run quietly, "
                   "has no way to ask for the rights it needs")
        return 2
    try:
        described = load_kit(args.kit) if args.kit else None
    except Refused as refusal:
        for problem in refusal.problems:
            report.say("STOP: " + problem)
        return 2
    machine = machine_of(report, described)
    if args.cleanup:
        return cleanup(report, machine, entries(machine.product))

    os.makedirs(args.work, exist_ok=True)
    if os.listdir(args.work):
        report.say("STOP: %s is not empty, and this run keeps its kit and its logs there" % args.work)
        return 2
    path, kind = machine_path()
    report.say("machine %s, %s" % (os.environ.get("COMPUTERNAME"), time.strftime("%Y-%m-%dT%H:%M:%S")))
    product = machine.product
    if entries(product) or os.path.exists(machine.folder) or ours_on_path(machine.folder, path):
        report.say("STOP: something of %s is here already - entries %d, folder %s, on PATH %d. Nothing "
                   "was touched. try_installer.py --cleanup removes what an earlier run left"
                   % (product, len(entries(product)), os.path.exists(machine.folder),
                      len(ours_on_path(machine.folder, path))))
        return 2
    with open(os.path.join(args.work, "machine-path-before.txt"), "w", encoding="utf-8") as handle:
        handle.write("%s\n%s\n" % (kind, path))
    profile = os.path.join(os.environ.get("APPDATA", ""), product)
    profile_before = tree_digests(profile)

    if described is None:
        report.say("building the kit from %s" % args.archives)
        try:
            described = build_kit(args.tag, args.archives, os.path.join(args.work, "kit"))
        except Refused as refusal:
            for problem in refusal.problems:
                report.say("STOP: " + problem)
            return 2
    kit = described["installers"]
    for label, _ in TRIALS:
        report.note("%s: %s, %s" % (label, os.path.basename(kit[label]["path"]), kit[label]["code"]))

    processes = []
    try:
        measure(args, report, kit, machine, processes)
        after, after_kind = machine_path()
        path_came_back(report, path, kind, after, after_kind)
        report.check(tree_digests(profile) == profile_before, "%s is as it was, byte for byte (%s)"
                     % (profile, "absent before and after" if profile_before is None else
                        "%d file(s)" % len(profile_before)))
    finally:
        for process in processes:
            if process.poll() is None:
                process.kill()
        left = {k: v for k, v in entries(product).items() if k in {kit[label]["code"] for label, _ in TRIALS}}
        if left:
            cleanup(report, machine, left)
        planted = os.path.join(machine.folder, PLANTED)
        if os.path.isfile(planted):
            os.remove(planted)
        if os.path.isdir(machine.folder) and not os.listdir(machine.folder):
            os.rmdir(machine.folder)
    report.check(not os.path.exists(machine.folder), "the planted file and its folder are gone, removed by this run")
    report.say("")
    report.say("failed %d, not measured %d" % (report.failed, report.unmeasured))
    with open(os.path.join(args.work, "try-installer.txt"), "w", encoding="utf-8", newline="\n") as handle:
        handle.write("\n".join(report.lines) + "\n")
    return report.code()


def machine_of(report, described):
    """What the run asks the machine about: from the kit when one is given, so
    that a machine without this repository can run it, and from the
    repository otherwise."""
    if described is not None:
        return Machine(report, described["product"], described["publisher"], described["programs_version"],
                       described["shortcut"])
    installer = contents.load_installer()
    return Machine(report, installer.product, build_msi.workspace_words()[0], contents.load_workspace().version,
                   installer.shortcut + ".exe")


def kit_only(report, args):
    """Build the kit into the work folder and stop. Installs nothing, so it
    needs no elevated terminal - only WiX."""
    os.makedirs(args.work, exist_ok=True)
    kit = os.path.join(args.work, "kit")
    if os.path.exists(kit):
        report.say("STOP: %s is there already" % kit)
        return 2
    try:
        described = build_kit(args.tag, args.archives, kit)
    except Refused as refusal:
        for problem in refusal.problems:
            report.say("STOP: " + problem)
        return 2
    report.say("the kit is in %s: %s" % (kit, ", ".join(
        os.path.basename(described["installers"][label]["path"]) for label, _ in TRIALS)))
    report.say("run it on a machine without WiX: python try_installer.py --kit <its folder> --work <an empty folder>")
    return 0


def path_came_back(report, before, kind, after, after_kind):
    """The machine PATH after the uninstall: every entry, in its order, of its
    kind. An empty entry means nothing to Windows, and Windows Installer drops
    one where it takes its own entry off - measured on a PATH that ended with a
    separator - so that alone is said, not failed."""
    entries_before = [p for p in before.split(";") if p]
    entries_after = [p for p in after.split(";") if p]
    report.check(entries_after == entries_before and after_kind == kind,
                 "the machine PATH has the entries it had before, in their order and of their kind "
                 "(%d entries, kind %s)" % (len(entries_after), after_kind))
    if after != before and entries_after == entries_before:
        report.note("the machine PATH lost %d empty entry/entries, such as a trailing separator. Before: "
                    "%d characters, after: %d" % (before.count(";") - after.count(";"), len(before), len(after)))


def cleanup(report, machine, found):
    """Uninstall each entry by its product code, and say what is left."""
    for code in sorted(found):
        done, took = msiexec("/x", code, os.path.join(os.environ.get("TEMP", "."), "nkb-cleanup.log"))
        report.say("uninstalled %s %s: exit %s, %d s" % (code, found[code].get("DisplayVersion"), done, took))
    left = entries(machine.product)
    report.say("left in Programs and Features: %s" % (", ".join(sorted(left)) or "nothing"))
    return 0 if not left else 1


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--tag", help="the tag the archives are named by, for example v0.1.0-dev.7")
    parser.add_argument("--archives", help="the folder holding the archives of the installer's platform")
    parser.add_argument("--work", help="an empty folder for the kit, the logs and the report")
    parser.add_argument("--palette", action="store_true",
                        help="also keep the installed palette running through the upgrade (needs a desktop)")
    parser.add_argument("--kit-only", action="store_true",
                        help="build the kit into the work folder and stop, for a machine without WiX")
    parser.add_argument("--kit", help="a kit built earlier with --kit-only, to run instead of building one")
    parser.add_argument("--cleanup", action="store_true", help="uninstall every copy of the product and stop")
    args = parser.parse_args(argv)
    if args.cleanup:
        pass
    elif args.kit:
        if not args.work or args.tag or args.archives or args.kit_only:
            parser.error("--kit takes --work and nothing to build from")
    elif not (args.tag and args.archives and args.work):
        parser.error("--tag, --archives and --work are needed, or --kit and --work, or --cleanup")
    for name in ("archives", "work", "kit"):
        if getattr(args, name):
            setattr(args, name, os.path.normpath(os.path.abspath(getattr(args, name))))
    try:
        return run(args)
    except Refused as refusal:
        print("try_installer: refused:", file=sys.stderr)
        for problem in refusal.problems:
            print("  - " + problem.replace("\n", "\n    "), file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
