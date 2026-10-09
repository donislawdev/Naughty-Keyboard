#!/usr/bin/env python3
"""Phase A of the release: what .github/workflows/release.yml asks of this
repository, step by step, so that each step is code a test can run.

    python3 .github/scripts/release.py plan --event push --ref-type tag --ref-name v0.1.0 --run-number 7
    python3 .github/scripts/release.py released --tag v0.1.0
    python3 .github/scripts/release.py green --sha <commit>
    python3 .github/scripts/release.py changelog --version 0.1.0 --notes notes.md
    python3 .github/scripts/release.py contents --tag v0.1.0 --out contents
    python3 .github/scripts/release.py build --os windows
    python3 .github/scripts/release.py pack --tag v0.1.0 --os windows --notices contents/notices --out dist
    python3 .github/scripts/release.py try --tag v0.1.0 --os windows --unpacked unpacked
    python3 .github/scripts/release.py handover --tag v0.1.0 --dir incoming
    python3 .github/scripts/release.py draft --tag v0.1.0 --notes notes.md

The four phases
---------------
A  release.yml, on a tag: check the tree, build, run what was built, describe
   it, attest how the UNSIGNED build was made, open an empty draft, and hand
   the build over as a workflow artifact. Nothing is published.
B  on the machines that hold the signing keys: verify A's statement before
   touching anything, sign, write the checksums over what will be downloaded,
   upload to the draft.
C  attest the bill of materials against the signed bytes.
D  when a person publishes the draft, check the page the way a user would.

Run by hand or on a pull request that changes the release, A builds and runs
everything and writes nothing to the releases of the repository. The version
of such a build is the workspace version with `-dev.<run number>`, so its files
can never be taken for a release.

Why a script and not shell in the workflow: every refusal here is a decision,
and a decision written as shell in YAML is tested only on the day of a
release. Here each one has a test, and the workflow only calls them.
"""
import argparse
import dataclasses
import gzip
import io
import json
import os
import plistlib
import re
import struct
import subprocess
import sys
import tarfile
import time
import zipfile

import contents
from contents import ROOT, Refused

# The workflows that run on every push to main and have to be green on the
# commit a release is built from. By file, not "every check of the commit": a
# question about every check is answered by whatever happened to report, so a
# workflow that never ran reads exactly like one that passed.
REQUIRED_WORKFLOWS = ("ci.yml", "supply-chain.yml", "semgrep.yml", "workflow-lint.yml")
MAIN = "main"
# A tag is often pushed right after a merge, while CI on that commit still runs.
WAIT_SECONDS = 25 * 60
POLL_SECONDS = 30

# The bill of materials describes both programs, so it carries the name of the
# project and not of one program. `verify-` sorts it after the archives, because
# a release page lists its files by name and nothing else: the files a person
# checks a download against sit together at the end.
SBOM_STEM = "verify-naughty-keyboard"
BUILD_SUMS = "build.sha256"
BESIDE = ("LICENSE", "README.md")

# The C runtime of Microsoft's compiler. .cargo/config.toml links it into the
# programs, because Windows does not ship it and a program that imports it does
# not start on a machine without the Visual C++ Redistributable. Asked of the
# released file, because a RUSTFLAGS variable would replace that setting without
# a word.
C_RUNTIME = re.compile(r"^((vcruntime|msvcp|concrt|vccorlib|vcomp)\d+d?(_[a-z0-9_]+)?|ucrtbased?|"
                       r"api-ms-win-crt-[a-z0-9-]+)\.dll$", re.IGNORECASE)
# Every variable Cargo reads compiler flags from. Any of them, even set to an
# empty string, replaces the flags in .cargo/config.toml.
FLAG_VARIABLES = re.compile(r"^(RUSTFLAGS|CARGO_ENCODED_RUSTFLAGS|CARGO_BUILD_RUSTFLAGS|"
                            r"CARGO_TARGET_[A-Z0-9_]+_RUSTFLAGS)$")

CHANGELOG = os.path.join(ROOT, "CHANGELOG.md")
VERSION_HEADING = re.compile(r"^## \[([^\]]+)\](.*)$")
DATED = re.compile(r"^ - \d{4}-\d{2}-\d{2}$")
LINK_DEFINITION = re.compile(r"^\[[^\]]+\]: \S+")


def say(text):
    print(text, flush=True)


# --------------------------------------------------------------------------- #
# what is being built
# --------------------------------------------------------------------------- #

@dataclasses.dataclass(frozen=True)
class Plan:
    release: bool        # a tag was pushed: the build is attested and a draft is opened
    tag: str
    version: str         # the tag without its v
    base: str            # the version of the workspace, the one the programs print
    prerelease: bool

    def sbom(self):
        return sbom_name(self.version)


def sbom_name(version):
    return "%s_%s.spdx.json" % (SBOM_STEM, version)


def plan_of(event, ref_type, ref_name, run_number, workspace):
    """A pushed tag is a release. Anything else builds the same files under a
    version no release can carry."""
    release = event == "push" and ref_type == "tag"
    if release:
        tag = ref_name
    else:
        if not re.fullmatch(r"\d+", str(run_number)):
            raise Refused(["the run number %r is not a number" % run_number])
        tag = "v%s-dev.%s" % (workspace.version, run_number)
    version = contents.version_of_tag(tag, workspace)
    return Plan(release, tag, version, workspace.version, "-" in version)


def platforms_of(archives):
    """One entry per platform, in the order archives.toml lists them."""
    seen = []
    for archive in archives:
        entry = {"os": archive.os, "arch": archive.arch, "target": archive.target,
                 "runner": archive.runner}
        if entry not in seen:
            seen.append(entry)
    return seen


def write_outputs(values, path):
    """Step outputs, one per line. A value with a line break would end the file
    format GitHub reads, so it is refused instead of written."""
    lines = []
    for key, value in values.items():
        text = str(value)
        if "\n" in text or "\r" in text:
            raise Refused(["the output %s has a line break in it" % key])
        lines.append("%s=%s" % (key, text))
    with open(path, "a", encoding="utf-8", newline="\n") as handle:
        handle.write("\n".join(lines) + "\n")


def command_plan(args):
    plan = plan_of(args.event, args.ref_type, args.ref_name, args.run_number, contents.load_workspace())
    archives = contents.load_archives()
    outputs = {
        "release": "true" if plan.release else "false",
        "tag": plan.tag,
        "version": plan.version,
        "base": plan.base,
        "prerelease": "true" if plan.prerelease else "false",
        "sbom": plan.sbom(),
        "platforms": json.dumps(platforms_of(archives), separators=(",", ":")),
    }
    for key, value in outputs.items():
        say("%s = %s" % (key, value))
    if args.output:
        write_outputs(outputs, args.output)
    return 0


# --------------------------------------------------------------------------- #
# what has to be true before anything is built
# --------------------------------------------------------------------------- #

def capture(command, run=subprocess.run, cwd=None):
    """A command's exit code and what it printed, read as UTF-8 on every system.
    A program that cannot be started answers 127 rather than raising."""
    try:
        return run(command, cwd=cwd, capture_output=True, text=True, encoding="utf-8", errors="replace")
    except OSError as error:
        return subprocess.CompletedProcess(command, 127, "", str(error))


def gh(arguments, run=subprocess.run):
    """gh, with what it said kept whatever the exit code."""
    return capture(["gh"] + list(arguments), run)


def release_state(tag, run=subprocess.run):
    """missing, draft or published. A draft is visible only to a token that can
    write, so a read-only token answers missing for one."""
    done = gh(["release", "view", tag, "--json", "isDraft", "--jq", ".isDraft"], run)
    if done.returncode == 0:
        return "draft" if done.stdout.strip() == "true" else "published"
    if "release not found" in done.stderr.lower():
        return "missing"
    raise Refused(["gh could not say whether %s has a release:\n%s" % (tag, done.stderr.strip())])


def command_released(args):
    if release_state(args.tag) == "published":
        raise Refused(["%s is already a published release, and a published release cannot "
                       "change. Release the fix under a new version." % args.tag])
    say("%s has no published release" % args.tag)
    return 0


def verdict(runs):
    """What the runs of one workflow on one commit say: (done, answer).

    Only runs on main count, so a tag on a commit that never reached main has
    nothing to show. The newest of them decides, because a re-run replaces the
    answer of the run it repeats and a later scheduled run is a newer answer."""
    on_main = [r for r in runs if r.get("headBranch") == MAIN and r.get("event") in ("push", "schedule")]
    if not on_main:
        return True, "never ran on %s for this commit" % MAIN
    newest = max(on_main, key=lambda r: r.get("createdAt", ""))
    if newest.get("status") != "completed":
        return False, "still %s: %s" % (newest.get("status"), newest.get("url", ""))
    return True, newest.get("conclusion") or "no conclusion"


def runs_of(workflow, sha, run=subprocess.run):
    done = gh(["run", "list", "--workflow", workflow, "--commit", sha, "--limit", "50",
               "--json", "status,conclusion,event,headBranch,createdAt,url"], run)
    if done.returncode != 0:
        raise Refused(["gh could not list the runs of %s:\n%s" % (workflow, done.stderr.strip())])
    return json.loads(done.stdout or "[]")


def wait_for_green(sha, run=subprocess.run, sleep=time.sleep, clock=time.monotonic,
                   workflows=REQUIRED_WORKFLOWS, limit=WAIT_SECONDS):
    """Every required workflow concluded success on this commit, waiting while
    one of them still runs."""
    started = clock()
    while True:
        answers = {w: verdict(runs_of(w, sha, run)) for w in workflows}
        waiting = [w for w, (done, _) in answers.items() if not done]
        for workflow, (done, answer) in answers.items():
            say("  %-20s %s" % (workflow, answer))
        if not waiting:
            break
        if clock() - started > limit:
            raise Refused(["%s still running after %d minutes. Re-run this job when it ends."
                           % (", ".join(waiting), limit // 60)])
        say("waiting for %s" % ", ".join(waiting))
        sleep(POLL_SECONDS)
    red = ["%s: %s" % (w, a) for w, (_, a) in answers.items() if a != "success"]
    if red:
        raise Refused(["a release is built from a commit every required workflow passed on %s, "
                       "and on %s:" % (MAIN, sha)] + red)
    return answers


def command_green(args):
    wait_for_green(args.sha)
    say("every required workflow passed on %s" % args.sha)
    return 0


def changelog_section(text, version):
    """The problems with closing the changelog for this version, and the text
    of its section."""
    lines = text.splitlines()
    headings = [(i, VERSION_HEADING.match(line)) for i, line in enumerate(lines)]
    headings = [(i, m) for i, m in headings if m]
    problems = []
    unreleased = [i for i, m in headings if m.group(1) == "Unreleased"]
    if len(unreleased) != 1:
        problems.append("CHANGELOG.md needs exactly one ## [Unreleased] heading, and has %d" % len(unreleased))
    found = [(i, m) for i, m in headings if m.group(1) == version]
    if not found:
        problems.append("CHANGELOG.md has no section for %s. Move what is under [Unreleased] into "
                        "## [%s] - YYYY-MM-DD first." % (version, version))
    elif len(found) > 1:
        problems.append("CHANGELOG.md has %d sections for %s" % (len(found), version))
    elif not DATED.match(found[0][1].group(2)):
        problems.append("the section for %s has no date: it has to read ## [%s] - YYYY-MM-DD"
                        % (version, version))

    def body(start):
        after = [i for i, _ in headings if i > start]
        end = after[0] if after else len(lines)
        return [line for line in lines[start + 1:end] if not LINK_DEFINITION.match(line)]

    if len(unreleased) == 1:
        left = [line for line in body(unreleased[0]) if line.strip()]
        if left:
            problems.append("CHANGELOG.md still has %d line(s) under [Unreleased]. They would go out "
                            "undescribed. Move them into %s." % (len(left), version))
    section = ""
    if len(found) == 1:
        section = "\n".join(body(found[0][0])).strip("\n")
        if not section.strip():
            problems.append("the section for %s is empty" % version)
    return problems, section


def command_changelog(args):
    with open(args.changelog, encoding="utf-8") as handle:
        problems, section = changelog_section(handle.read(), args.version)
    if problems:
        raise Refused(problems)
    if args.notes:
        contents.write(args.notes, section + "\n")
        say("wrote %s from the section for %s" % (args.notes, args.version))
    say("CHANGELOG.md is closed for %s" % args.version)
    return 0


# --------------------------------------------------------------------------- #
# the bill of materials and the notices
# --------------------------------------------------------------------------- #

def notices_path(directory, archive, version):
    return os.path.join(directory, archive.name(version), contents.NOTICES_NAME)


def command_contents(args):
    """The SBOM and the notices of every archive, in one job, so that every
    archive carries notices written by the same code on the same system."""
    workspace = contents.load_workspace()
    plan_version = contents.version_of_tag(args.tag, workspace)
    sbom = os.path.join(args.out, "sbom", sbom_name(plan_version))
    if contents.main(["sbom", "--tag", args.tag, "--out", sbom]) != 0:
        return 1
    for archive in contents.load_archives():
        out = notices_path(os.path.join(args.out, "notices"), archive, plan_version)
        if contents.main(["notices", "--tag", args.tag, "--program", archive.program,
                          "--os", archive.os, "--out", out]) != 0:
            return 1
    return 0


# --------------------------------------------------------------------------- #
# building
# --------------------------------------------------------------------------- #

def archives_for(os_name, archives=None):
    found = [a for a in (archives or contents.load_archives()) if a.os == os_name]
    if not found:
        raise Refused(["archives.toml has no platform %r" % os_name])
    return found


def build_commands(archives):
    """One cargo build per program. One invocation naming both packages would
    build each with the features of both, and the bill of materials, which asks
    Cargo about each program alone, would describe a different program."""
    return [["cargo", "build", "--release", "--locked", "--target", a.target, "-p", a.package]
            for a in archives]


def build_environment(archives, environ):
    """The environment cargo is given: refused when it carries compiler flags,
    and told the oldest macOS where there is one."""
    flags = sorted(name for name in environ if FLAG_VARIABLES.match(name))
    if flags:
        raise Refused(["%s is set. Cargo would read it in place of the flags in .cargo/config.toml, "
                       "and on Windows the programs would then need the Visual C++ runtime to "
                       "start." % ", ".join(flags)])
    built = dict(environ)
    minimum = {a.minimum_os for a in archives if a.minimum_os}
    if len(minimum) > 1:
        raise Refused(["one platform with two minimum versions: %s" % ", ".join(sorted(minimum))])
    if minimum:
        built["MACOSX_DEPLOYMENT_TARGET"] = minimum.pop()
    return built


def command_build(args):
    archives = archives_for(args.os)
    environment = build_environment(archives, os.environ)
    for command in build_commands(archives):
        say("+ " + " ".join(command))
        if subprocess.run(command, cwd=ROOT, env=environment).returncode != 0:
            raise Refused(["%s failed" % " ".join(command)])
    return 0


# --------------------------------------------------------------------------- #
# packing
# --------------------------------------------------------------------------- #

@dataclasses.dataclass(frozen=True)
class Entry:
    name: str            # the path inside the archive, with forward slashes
    kind: str            # file, directory or link
    mode: int
    data: bytes = b""
    target: str = ""     # what a link points at, relative to where it stands


def program_file(archive):
    return archive.program + (".exe" if archive.os == "windows" else "")


def info_plist(archive, base, icon):
    """The bundle's description. Version strings are the version of the
    workspace: macOS reads them as up to three numbers, and a suffix such as
    -rc.1 is not one."""
    return plistlib.dumps({
        "CFBundleDevelopmentRegion": "en",
        "CFBundleExecutable": archive.program,
        "CFBundleIconFile": os.path.splitext(os.path.basename(icon))[0],
        "CFBundleIdentifier": archive.bundle_id,
        "CFBundleInfoDictionaryVersion": "6.0",
        "CFBundleName": archive.program,
        "CFBundlePackageType": "APPL",
        "CFBundleShortVersionString": base,
        "CFBundleVersion": base,
        "LSMinimumSystemVersion": archive.minimum_os,
        "NSHighResolutionCapable": True,
    }, fmt=plistlib.FMT_XML, sort_keys=True)


def entries_of(archive, base, program, notices, root=ROOT):
    """Everything an archive holds, in the order it is written."""
    entries = []
    name = program_file(archive)
    if archive.os == contents.MACOS:
        # The program inside a bundle, and a link to it beside the bundle so
        # that a person still types ./nkb. A copy in place of the link would be
        # refused by macOS, because the signature covers the bundle and not the
        # file, so only a real link is ever written.
        app = name + ".app"
        icon = archive.packaged[0]
        icon_bytes = read_bytes(os.path.join(root, icon))
        entries += [
            Entry(app, "directory", 0o755),
            Entry(app + "/Contents", "directory", 0o755),
            Entry(app + "/Contents/Info.plist", "file", 0o644, info_plist(archive, base, icon)),
            Entry(app + "/Contents/MacOS", "directory", 0o755),
            Entry(app + "/Contents/MacOS/" + name, "file", 0o755, program),
            Entry(app + "/Contents/Resources", "directory", 0o755),
            Entry(app + "/Contents/Resources/" + os.path.basename(icon), "file", 0o644, icon_bytes),
            Entry(name, "link", 0o755, target=app + "/Contents/MacOS/" + name),
        ]
    else:
        entries.append(Entry(name, "file", 0o755, program))
    for beside in BESIDE:
        entries.append(Entry(beside, "file", 0o644, read_bytes(os.path.join(root, beside))))
    entries.append(Entry(contents.NOTICES_NAME, "file", 0o644, notices))
    return sorted(entries, key=lambda e: e.name)


def expected_names(archive):
    """What a person finds after unpacking: every file and link, no directories."""
    name = program_file(archive)
    names = [name] + list(BESIDE) + [contents.NOTICES_NAME]
    if archive.os == contents.MACOS:
        app = name + ".app/Contents/"
        names += [app + "Info.plist", app + "MacOS/" + name,
                  app + "Resources/" + os.path.basename(archive.packaged[0])]
    return sorted(names)


def read_bytes(path):
    with open(path, "rb") as handle:
        return handle.read()


def write_archive(path, archive_format, entries, epoch):
    """The same entries and the same time give the same bytes, on any system:
    entries in name order, no owner, every time the commit's. Written whole or
    not at all."""
    temporary = path + ".partial"
    try:
        write_entries(temporary, archive_format, entries, epoch)
    except BaseException:
        if os.path.exists(temporary):
            os.remove(temporary)
        raise
    os.replace(temporary, path)


def write_entries(temporary, archive_format, entries, epoch):
    if archive_format == "zip":
        stamp = time.gmtime(max(epoch, 315532800))[:6]   # a zip cannot say a time before 1980
        with zipfile.ZipFile(temporary, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as out:
            for entry in entries:
                if entry.kind != "file":
                    raise Refused(["a zip here holds files only, and %s is a %s" % (entry.name, entry.kind)])
                info = zipfile.ZipInfo(entry.name, date_time=stamp)
                info.compress_type = zipfile.ZIP_DEFLATED
                info.create_system = 3
                info.external_attr = (0o100000 | entry.mode) << 16
                out.writestr(info, entry.data)
    elif archive_format == "tar.gz":
        with open(temporary, "wb") as raw:
            with gzip.GzipFile(filename="", mode="wb", fileobj=raw, mtime=epoch, compresslevel=9) as packed:
                with tarfile.open(fileobj=packed, mode="w", format=tarfile.USTAR_FORMAT) as out:
                    for entry in entries:
                        info = tarfile.TarInfo(entry.name)
                        info.mtime = epoch
                        info.mode = entry.mode
                        info.uid = info.gid = 0
                        info.uname = info.gname = ""
                        if entry.kind == "directory":
                            info.type = tarfile.DIRTYPE
                            out.addfile(info)
                        elif entry.kind == "link":
                            info.type = tarfile.SYMTYPE
                            info.linkname = entry.target
                            out.addfile(info)
                        else:
                            info.size = len(entry.data)
                            out.addfile(info, io.BytesIO(entry.data))
    else:
        raise Refused(["no archive format %r" % archive_format])


def commit_epoch(root=ROOT, environ=os.environ):
    """The time every entry of an archive carries: when the commit was made.
    SOURCE_DATE_EPOCH, the convention of reproducible builds, says it where
    there is no repository to ask, such as a machine that repacks a signed
    program."""
    given = environ.get("SOURCE_DATE_EPOCH")
    if given is not None:
        if not given.isdigit():
            raise Refused(["SOURCE_DATE_EPOCH is %r, and it has to be seconds since 1970" % given])
        return int(given)
    done = capture(["git", "log", "-1", "--format=%ct"], cwd=root)
    if done.returncode != 0 or not done.stdout.strip().isdigit():
        raise Refused(["git cannot say when HEAD was committed:\n%s" % done.stderr.strip()])
    return int(done.stdout.strip())


def built_program(archive, root=ROOT, environ=os.environ):
    """Where cargo put the program: under target/ unless CARGO_TARGET_DIR moved it."""
    target_dir = os.path.join(root, environ.get("CARGO_TARGET_DIR") or "target")
    return os.path.join(target_dir, archive.target, "release", program_file(archive))


def command_pack(args):
    workspace = contents.load_workspace()
    version = contents.version_of_tag(args.tag, workspace)
    epoch = commit_epoch()
    os.makedirs(args.out, exist_ok=True)
    for archive in archives_for(args.os):
        program = built_program(archive)
        notices = notices_path(args.notices, archive, version)
        missing = [p for p in (program, notices) if not os.path.isfile(p)]
        if missing:
            raise Refused(["%s cannot be packed without %s" % (archive.name(version), ", ".join(missing))])
        entries = entries_of(archive, workspace.version, read_bytes(program), read_bytes(notices))
        path = os.path.join(args.out, archive.name(version))
        write_archive(path, archive.format, entries, epoch)
        say("packed %s: %s" % (archive.name(version), ", ".join(e.name for e in entries if e.kind != "directory")))
    return 0


# --------------------------------------------------------------------------- #
# running what was built, from where an archive puts it
# --------------------------------------------------------------------------- #

def unpacked_names(directory):
    """Every file and link under a directory, as the archive names them."""
    names = []
    for base, dirs, files in os.walk(directory):
        for name in files + [d for d in dirs if os.path.islink(os.path.join(base, d))]:
            names.append(os.path.relpath(os.path.join(base, name), directory).replace(os.sep, "/"))
    return sorted(names)


def dlls_imported_by(path):
    """The libraries a Windows program asks the loader for, read out of its
    import table and its delay-load table."""
    data = read_bytes(path)

    def u16(at):
        return struct.unpack_from("<H", data, at)[0]

    def u32(at):
        return struct.unpack_from("<I", data, at)[0]

    if data[:2] != b"MZ" or data[u32(0x3C):u32(0x3C) + 4] != b"PE\0\0":
        raise Refused(["%s is not a Windows program" % path])
    header = u32(0x3C) + 4
    count, optional_size = u16(header + 2), u16(header + 16)
    optional = header + 20
    magic = u16(optional)
    if magic not in (0x10B, 0x20B):
        raise Refused(["%s has an optional header this does not read: %#x" % (path, magic)])
    directories = optional + (96 if magic == 0x10B else 112)
    entries = u32(directories - 4)
    sections = []
    for index in range(count):
        at = optional + optional_size + 40 * index
        sections.append((u32(at + 12), max(u32(at + 8), u32(at + 16)), u32(at + 20)))

    def offset(rva):
        for start, size, raw in sections:
            if start <= rva < start + size:
                return raw + rva - start
        raise Refused(["%s points at %#x, which no section holds" % (path, rva)])

    def text_at(rva):
        at = offset(rva)
        return data[at:data.index(b"\0", at)].decode("ascii")

    names = []
    for index, size, name_at in ((1, 20, 12), (13, 32, 4)):
        if index >= entries or not u32(directories + 8 * index):
            continue
        at = offset(u32(directories + 8 * index))
        while any(data[at:at + size]):
            names.append(text_at(u32(at + name_at)))
            at += size
    return names


def c_runtime_in(names):
    return sorted(n for n in names if C_RUNTIME.match(n))


def tried_programs(archive, base, directory, packs, run=subprocess.run):
    """Problems found by running what an archive holds, from where it holds it."""
    problems = []
    found = unpacked_names(directory)
    if found != expected_names(archive):
        problems.append("%s holds %s, and should hold %s" % (archive.program, found, expected_names(archive)))
        return problems
    program = os.path.join(directory, program_file(archive))
    if archive.os == contents.MACOS:
        target = os.readlink(program) if os.path.islink(program) else ""
        if not target or os.path.isabs(target) or not target.startswith(program_file(archive) + ".app/"):
            problems.append("%s beside the bundle is not a relative link into it: %r" % (archive.program, target))
        plist = os.path.join(directory, program_file(archive) + ".app", "Contents", "Info.plist")
        lint = capture(["plutil", "-lint", plist], run)
        if lint.returncode != 0:
            problems.append("plutil does not accept the Info.plist of %s: %s" % (archive.program, lint.stdout + lint.stderr))
    if archive.os != "windows" and not os.access(program, os.X_OK):
        problems.append("%s is not executable after unpacking" % archive.program)
    if archive.os == "windows":
        runtime = c_runtime_in(dlls_imported_by(program))
        if runtime:
            problems.append("%s imports %s, so it would not start on a Windows without the Visual C++ "
                            "Redistributable. Something replaced the flags in .cargo/config.toml."
                            % (archive.program, ", ".join(runtime)))
    if archive.package == "nkb-cli":
        problems += tried_command_line(program, base, packs, run)
    return problems


def tried_command_line(program, base, packs, run):
    """The contract other people's pipelines depend on, asked of the shipped file."""
    problems = []

    def ask(*arguments):
        return capture([program] + list(arguments), run)

    version = ask("--version")
    if version.returncode != 0 or version.stdout.strip() != "nkb %s" % base:
        problems.append("nkb --version printed %r, and the workspace is %s" % (version.stdout.strip(), base))
    if ask("packs").returncode != 0:
        problems.append("nkb packs failed")
    for pack in packs:
        name = os.path.splitext(os.path.basename(pack))[0]
        if ask("lint", pack).returncode != 0:
            problems.append("%s does not pass nkb lint" % pack)
        emitted = ask("emit", name, "--format", "json")
        if emitted.returncode != 0 or not emitted.stdout.strip():
            problems.append("nkb emit %s printed nothing" % name)
    return problems


def glibc_needed(program, run=subprocess.run):
    """The newest glibc a Linux program asks for, which is the oldest glibc it
    starts on. Built on Ubuntu 24.04 the palette asked for 2.39, measured on
    2026-10-09, which is why Linux is built on the oldest image GitHub offers."""
    symbols = capture(["objdump", "-T", program], run)
    if symbols.returncode != 0:
        return None
    versions = re.findall(r"GLIBC_(\d+(?:\.\d+)+)", symbols.stdout)
    if not versions:
        return None
    return max(versions, key=lambda v: tuple(int(n) for n in v.split(".")))


def command_try(args):
    workspace = contents.load_workspace()
    version = contents.version_of_tag(args.tag, workspace)
    packs = sorted(os.path.join(ROOT, "packs", n) for n in os.listdir(os.path.join(ROOT, "packs"))
                   if n.endswith(".toml"))
    problems = []
    for archive in archives_for(args.os):
        directory = os.path.join(args.unpacked, archive.name(version))
        if not os.path.isdir(directory):
            problems.append("%s was not unpacked into %s" % (archive.name(version), directory))
            continue
        found = tried_programs(archive, workspace.version, directory, packs)
        problems += found
        if not found:
            say("ran %s from the unpacked archive" % archive.name(version))
        if not found and archive.os == "linux":
            needed = glibc_needed(os.path.join(directory, program_file(archive)))
            say("  %s starts on glibc %s or newer" % (archive.program, needed) if needed else
                "  objdump could not say which glibc %s needs" % archive.program)
    if problems:
        raise Refused(problems)
    return 0


# --------------------------------------------------------------------------- #
# handing the build over, and the draft
# --------------------------------------------------------------------------- #

def handover_names(tag, archives=None):
    version = contents.version_of_tag(tag, contents.load_workspace())
    return sorted([a.name(version) for a in (archives or contents.load_archives())] + [sbom_name(version)])


def command_handover(args):
    wanted = handover_names(args.tag)
    found = sorted(os.listdir(args.dir))
    missing = sorted(set(wanted) - set(found))
    extra = sorted(set(found) - set(wanted))
    if missing or extra:
        raise Refused(["the build to hand over is not what archives.toml lists"] +
                      ["missing: %s" % n for n in missing] + ["not listed: %s" % n for n in extra])
    say("the build holds the %d files archives.toml lists" % len(wanted))
    return 0


def draft_command(tag, notes, prerelease):
    """An EMPTY draft. The archives are signed after this and uploaded then, and
    an unsigned program on a release page is a program somebody downloads."""
    command = ["release", "create", tag, "--draft", "--verify-tag", "--title", tag, "--notes-file", notes]
    if prerelease:
        command.append("--prerelease")
    return command


def open_draft(tag, notes, run=subprocess.run):
    state = release_state(tag, run)
    if state == "published":
        raise Refused(["%s is already published, and this run built it again. Nothing was "
                       "opened." % tag])
    if state == "draft":
        say("the draft of %s is already open and is left as it is" % tag)
        return state
    done = gh(draft_command(tag, notes, "-" in tag), run)
    if done.returncode != 0:
        raise Refused(["gh could not open the draft of %s:\n%s" % (tag, done.stderr.strip())])
    say("opened %s as an empty draft" % tag)
    return state


def command_draft(args):
    open_draft(args.tag, args.notes)
    return 0


# --------------------------------------------------------------------------- #
# the command line
# --------------------------------------------------------------------------- #

def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    commands = parser.add_subparsers(dest="command", required=True)
    plan = commands.add_parser("plan", help="what this run builds, as step outputs")
    plan.add_argument("--event", required=True)
    plan.add_argument("--ref-type", required=True)
    plan.add_argument("--ref-name", required=True)
    plan.add_argument("--run-number", required=True)
    plan.add_argument("--output", default=os.environ.get("GITHUB_OUTPUT"))
    released = commands.add_parser("released", help="refuse a tag that is already a published release")
    released.add_argument("--tag", required=True)
    green = commands.add_parser("green", help="every required workflow passed on main on this commit")
    green.add_argument("--sha", required=True)
    changelog = commands.add_parser("changelog", help="the changelog is closed for this version")
    changelog.add_argument("--version", required=True)
    changelog.add_argument("--notes")
    changelog.add_argument("--changelog", default=CHANGELOG)
    described = commands.add_parser("contents", help="the bill of materials and every archive's notices")
    described.add_argument("--tag", required=True)
    described.add_argument("--out", required=True)
    build = commands.add_parser("build", help="build the programs of one platform")
    build.add_argument("--os", required=True)
    pack = commands.add_parser("pack", help="pack the programs of one platform")
    pack.add_argument("--tag", required=True)
    pack.add_argument("--os", required=True)
    pack.add_argument("--notices", required=True)
    pack.add_argument("--out", required=True)
    tried = commands.add_parser("try", help="run what the unpacked archives of one platform hold")
    tried.add_argument("--tag", required=True)
    tried.add_argument("--os", required=True)
    tried.add_argument("--unpacked", required=True)
    handover = commands.add_parser("handover", help="the build holds every file it should and no other")
    handover.add_argument("--tag", required=True)
    handover.add_argument("--dir", required=True)
    draft = commands.add_parser("draft", help="open the empty draft of a release")
    draft.add_argument("--tag", required=True)
    draft.add_argument("--notes", required=True)
    args = parser.parse_args(argv)
    handlers = {"plan": command_plan, "released": command_released, "green": command_green,
                "changelog": command_changelog, "contents": command_contents, "build": command_build,
                "pack": command_pack, "try": command_try, "handover": command_handover,
                "draft": command_draft}
    try:
        return handlers[args.command](args)
    except Refused as refusal:
        print("release: %s refused:" % args.command, file=sys.stderr)
        for problem in refusal.problems:
            print("  - " + problem.replace("\n", "\n    "), file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
