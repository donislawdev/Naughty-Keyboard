#!/usr/bin/env python3
"""What the release archives contain that somebody else wrote: the bill of
materials published beside them, and the licence notices that travel inside.

    python3 .github/scripts/contents.py check
    python3 .github/scripts/contents.py sbom --tag v0.1.0 --out verify-nkb_0.1.0.spdx.json
    python3 .github/scripts/contents.py notices --tag v0.1.0 --program nkb --os windows --out THIRD-PARTY-NOTICES.txt

`check` does everything for every archive and writes nothing. It is what CI
runs, so a change that would make a release unable to describe itself fails on
its pull request rather than on the day of the release.

Two sources, each for what it knows
-----------------------------------
The crates come from Cargo. `cargo tree` is asked which crates are compiled into
each program for each target, without build scripts and procedural macros,
which run on the build machine and are not shipped. That is the build's own
input list, resolved with the same features as a build of that one program, so
it is neither a guess nor a scan of the finished binary. A scan of a built
artefact misses what packaging stripped and assigns licences by guessing, and a
register written by hand for some 320 crates would be wrong after the first
weekly dependency update. Each crate's licence is read from its manifest, its
digest from Cargo.lock.

Everything Cargo cannot see is in .github/release/components.toml: files
compiled in through `include_bytes!`, `include_str!`, a Slint import or a
resource script, and tables generated from somebody else's data. That list is
policed, not trusted. The source of the programs is searched for every such
reference and every generated table, and one the register does not account for
is refused, and so is an entry that points at nothing.

Which licence, and which text
-----------------------------
A crate offered under a choice of licences (`MIT OR Apache-2.0`) is used under
one of them, and the notices give that one. The choice is mechanical: the
licences deny.toml allows, in the order it lists them, and among those the
first whose text the crate ships, so that the notice carries the crate's own
copyright line. A crate that ships no text for any allowed choice gets the
standard text of the first one, from .github/release/licence-texts, and the
notice says so. A crate with no allowed choice at all, or none that can be
given a text, is refused rather than described.

What is refused, and why that is the useful answer
--------------------------------------------------
A bill of materials that leaves out what it could not see is worse than none,
because it reads as complete. So every gap is a refusal, all of them are listed
in one run, and nothing is written when there is one.
"""
import argparse
import dataclasses
import datetime
import hashlib
import json
import os
import re
import subprocess
import sys
import tomllib

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
RELEASE = os.path.join(ROOT, ".github", "release")
TEXTS = os.path.join(RELEASE, "licence-texts")

CRATES_IO = "registry+https://github.com/rust-lang/crates.io-index"
NOTICES_NAME = "THIRD-PARTY-NOTICES.txt"
TOOL = "naughty-keyboard-contents-1"
EXTRACTED = "hasExtractedLicensingInfos"
# The top-level keys of an SPDX 2.3 JSON document this script may write.
DOCUMENT_KEYS = {"spdxVersion", "dataLicense", "SPDXID", "name", "documentNamespace",
                 "creationInfo", "comment", "packages", "relationships", EXTRACTED}
SUPPLIER = "Organization: DonislawDev"

# Where a crate keeps its licence texts. The REUSE layout, LICENSES/<id>.<ext>,
# names the licence by its file name and is read first. Anything else at the top
# of the crate whose name starts like this is read and recognised by what it says.
LICENCE_FILE = re.compile(r"^(licen[cs]e|copying|unlicense|copyright|notice)", re.IGNORECASE)

# What a licence text says that no other one does. Lower case, with runs of white
# space folded into one and typographic quotes made plain, which is how a text is
# compared. Several may match one file, which is how a file holding two licences
# is recognised as both.
SIGNATURES = (
    ("Apache-2.0", ("apache license", "version 2.0, january 2004")),
    ("MIT", ("permission is hereby granted, free of charge, to any person obtaining a copy",)),
    ("BSL-1.0", ("boost software license",)),
    ("Unicode-3.0", ("unicode license v3",)),
    ("Zlib", ("this software is provided 'as-is', without any express or implied warranty",
              "altered source versions must be plainly marked")),
    ("ISC", ("permission to use, copy, modify, and/or distribute this software for any purpose "
             "with or without fee is hereby granted, provided that the above copyright notice",)),
    ("BSD-3-Clause", ("redistribution and use in source and binary forms", "neither the name")),
    ("BSD-2-Clause", ("redistribution and use in source and binary forms",)),
    ("CC0-1.0", ("cc0 1.0 universal",)),
    ("Unlicense", ("this is free and unencumbered software released into the public domain",)),
    ("GPL-3.0-only", ("gnu general public license", "version 3, 29 june 2007")),
    ("LLVM-exception", ("llvm exceptions to the apache 2.0 license",)),
)
# Typographic quotes some licence files use, made plain before a text is compared.
# Spelled as code points, so no editor can turn them into the characters they name.
TYPOGRAPHIC_QUOTES = ((chr(0x2019), "'"), (chr(0x2018), "'"), (chr(0x201C), '"'), (chr(0x201D), '"'))
ZERO_BSD = ("permission to use, copy, modify, and/or distribute this software for any purpose "
            "with or without fee is hereby granted")

# Source references that compile a file into a program, and the marker a
# generated table opens with. Read from the source of the programs only: tests,
# examples and benches are not shipped.
INCLUDE = re.compile(r'include_(?:bytes|str)!\(\s*"([^"]+)"\s*\)')
SLINT_FILE_IMPORT = re.compile(r'^\s*import\s+"([^"]+)"\s*;', re.MULTILINE)
SLINT_IMAGE = re.compile(r'@image-url\(\s*"([^"]+)"')
RESOURCE_FILE = re.compile(r'"([^"]+)"')
GENERATED = "//! Generated from "
NOT_SHIPPED_DIRS = {"tests", "examples", "benches", "target"}


class Refused(Exception):
    """Every reason a release cannot be described, collected rather than raised one by one."""

    def __init__(self, problems):
        self.problems = list(problems)
        super().__init__("\n".join(self.problems))


def read_toml(path):
    with open(path, "rb") as handle:
        return tomllib.load(handle)


def relative(path):
    return os.path.relpath(path, ROOT).replace(os.sep, "/")


def sha256_of(path):
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for block in iter(lambda: handle.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


# --------------------------------------------------------------------------- #
# what the workspace and the release say about themselves
# --------------------------------------------------------------------------- #

@dataclasses.dataclass(frozen=True)
class Workspace:
    version: str
    repository: str
    licence: str


def load_workspace(root=ROOT):
    package = read_toml(os.path.join(root, "Cargo.toml")).get("workspace", {}).get("package", {})
    missing = [key for key in ("version", "repository", "license") if key not in package]
    if missing:
        raise Refused(["the workspace Cargo.toml has no %s under [workspace.package]" % ", ".join(missing)])
    return Workspace(package["version"], package["repository"].rstrip("/"), package["license"])


TAG = re.compile(r"^v(\d+\.\d+\.\d+)(-[0-9A-Za-z][0-9A-Za-z.-]*)?$")


def version_of_tag(tag, workspace):
    """The version a tag releases, refused unless its base is the workspace's."""
    found = TAG.match(tag or "")
    if not found:
        raise Refused(["%r is not a release tag: it has to read v<major>.<minor>.<patch>, "
                       "with an optional -suffix for a release candidate" % tag])
    if found.group(1) != workspace.version:
        raise Refused(["the tag %s releases %s, and the workspace Cargo.toml says %s. "
                       "One of them was not moved." % (tag, found.group(1), workspace.version)])
    return tag[1:]


@dataclasses.dataclass(frozen=True)
class Archive:
    program: str
    package: str
    summary: str
    os: str
    arch: str
    target: str
    format: str

    def name(self, version):
        return "%s_%s_%s_%s.%s" % (self.program, version, self.os, self.arch, self.format)


def load_archives(path=os.path.join(RELEASE, "archives.toml")):
    data = read_toml(path)
    problems = []
    if data.get("schema") != 1:
        problems.append("%s: schema %r, and this script reads 1" % (relative(path), data.get("schema")))
    programs = data.get("programs") or {}
    platforms = data.get("platforms") or []
    if not programs:
        problems.append("%s names no programs" % relative(path))
    if not platforms:
        problems.append("%s names no platforms" % relative(path))
    for name, program in programs.items():
        for key in ("package", "summary", "macos-bundle-id"):
            if not program.get(key):
                problems.append("program %s has no %s" % (name, key))
    seen = set()
    for platform in platforms:
        for key in ("os", "arch", "target", "format"):
            if not platform.get(key):
                problems.append("a platform has no %s: %r" % (key, platform))
        if platform.get("format") not in ("zip", "tar.gz"):
            problems.append("platform %s has format %r, and an archive is a zip or a tar.gz"
                            % (platform.get("os"), platform.get("format")))
        key = (platform.get("os"), platform.get("arch"))
        if key in seen:
            problems.append("platform %s %s is listed twice" % key)
        seen.add(key)
    if problems:
        raise Refused(problems)
    return [
        Archive(name, program["package"], program["summary"],
                platform["os"], platform["arch"], platform["target"], platform["format"])
        for name, program in sorted(programs.items())
        for platform in platforms
    ]


# --------------------------------------------------------------------------- #
# the register of what Cargo cannot see
# --------------------------------------------------------------------------- #

@dataclasses.dataclass(frozen=True)
class Component:
    name: str
    version: str
    licence: str
    licence_text: str
    source: str
    programs: tuple
    files: tuple       # (path, sha256 or None)
    derived: tuple
    own: bool


def load_register(path=os.path.join(RELEASE, "components.toml"), root=ROOT, programs=None):
    data = read_toml(path)
    problems = []
    if data.get("schema") != 1:
        problems.append("%s: schema %r, and this script reads 1" % (relative(path), data.get("schema")))
    components = []
    for entry in data.get("components", []):
        name = entry.get("name", "")
        for key in ("name", "licence", "source", "programs"):
            if not entry.get(key):
                problems.append("component %r has no %s" % (name, key))
        licence = entry.get("licence", "")
        try:
            alternatives(licence)
        except Refused as refusal:
            problems.extend("component %r: %s" % (name, p) for p in refusal.problems)
        text = entry.get("licence-text", "")
        if "LicenseRef-" in licence and not text:
            problems.append("component %r declares %s and names no licence-text to define it" % (name, licence))
        if text and not os.path.isfile(os.path.join(root, text)):
            problems.append("component %r: its licence-text %s is not there" % (name, text))
        for program in entry.get("programs", []):
            if programs is not None and program not in programs:
                problems.append("component %r is in program %r, which archives.toml does not list" % (name, program))
        files = []
        for item in entry.get("files", []):
            where = item.get("path", "")
            full = os.path.join(root, where)
            if where.endswith("/"):
                if not os.path.isdir(full):
                    problems.append("component %r: the directory %s is not there" % (name, where))
            elif not os.path.isfile(full):
                problems.append("component %r: the file %s is not there" % (name, where))
            elif item.get("sha256") and sha256_of(full) != item["sha256"]:
                problems.append("component %r: %s no longer has the digest the register pins, "
                                "%s. Somebody else's bytes changed - read what changed, then "
                                "move the version and the digest together" % (name, where, item["sha256"]))
            files.append((where, item.get("sha256")))
        for where in entry.get("derived", []):
            if not os.path.isfile(os.path.join(root, where)):
                problems.append("component %r: the generated file %s is not there" % (name, where))
        components.append(Component(
            name, entry.get("version", ""), licence, text, entry.get("source", ""),
            tuple(entry.get("programs", [])), tuple(files), tuple(entry.get("derived", [])),
            bool(entry.get("own", False))))
    allowances = [(a.get("file", ""), a.get("reference", ""), a.get("reason", ""))
                  for a in data.get("not-compiled-in", [])]
    for file, reference, reason in allowances:
        if not (file and reference and reason):
            problems.append("a not-compiled-in entry needs a file, a reference and a reason: %r"
                            % ((file, reference, reason),))
    if problems:
        raise Refused(problems)
    return components, allowances


def shipped_sources(root=ROOT):
    """Every file of the programs' own source that could compile a file in."""
    crates = os.path.join(root, "crates")
    found = []
    for crate in sorted(os.listdir(crates)):
        for base, dirs, names in os.walk(os.path.join(crates, crate)):
            dirs[:] = sorted(d for d in dirs if d not in NOT_SHIPPED_DIRS)
            for name in sorted(names):
                if name.endswith((".rs", ".slint", ".rc")):
                    found.append(os.path.join(base, name))
    return found


def references_in(path):
    """The files a source file compiles in, as written, and whether it is generated."""
    with open(path, encoding="utf-8") as handle:
        text = handle.read()
    if path.endswith(".rs"):
        code = "\n".join(line for line in text.splitlines() if not line.lstrip().startswith("//"))
        return INCLUDE.findall(code), text.startswith(GENERATED)
    if path.endswith(".slint"):
        code = "\n".join(line for line in text.splitlines() if not line.lstrip().startswith("//"))
        files = [f for f in SLINT_FILE_IMPORT.findall(code) if not f.endswith(".slint")]
        return files + SLINT_IMAGE.findall(code), False
    code = "\n".join(line for line in text.splitlines() if not line.lstrip().startswith("//"))
    return RESOURCE_FILE.findall(code), False


def check_register_against_source(components, allowances, root=ROOT):
    """The source check: every file compiled in is accounted for, both ways."""
    problems = []
    covered_files = {}
    covered_dirs = {}
    derived = {}
    for component in components:
        for where, _ in component.files:
            (covered_dirs if where.endswith("/") else covered_files)[where] = component.name
        for where in component.derived:
            derived[where] = component.name
    used_allowances = set()
    used_entries = set()
    for path in shipped_sources(root):
        source = os.path.relpath(path, root).replace(os.sep, "/")
        references, generated = references_in(path)
        if generated:
            if source in derived:
                used_entries.add(source)
            else:
                problems.append("%s says it was generated from somebody's data, and no component "
                                "in .github/release/components.toml lists it under derived" % source)
        for reference in references:
            allowance = next((a for a in allowances if a[0] == source and a[1] == reference), None)
            if allowance:
                used_allowances.add(allowance)
                continue
            target = os.path.normpath(os.path.join(os.path.dirname(path), reference))
            target = os.path.relpath(target, root).replace(os.sep, "/")
            owner = covered_files.get(target)
            if owner is None:
                owner = next((n for d, n in covered_dirs.items() if target.startswith(d)), None)
                if owner is not None:
                    used_entries.add(next(d for d in covered_dirs if target.startswith(d)))
            else:
                used_entries.add(target)
            if owner is None:
                problems.append("%s compiles in %s, and no component in "
                                ".github/release/components.toml names it" % (source, target))
    for where, name in list(covered_files.items()) + list(covered_dirs.items()) + list(derived.items()):
        if where not in used_entries:
            problems.append("component %r lists %s, and nothing in the source of the programs "
                            "compiles it in" % (name, where))
    for allowance in allowances:
        if allowance not in used_allowances:
            problems.append("the not-compiled-in entry for %s (%s) matches nothing any more - "
                            "remove it" % (allowance[0], allowance[1]))
    return problems


def check_standard_texts(directory=TEXTS):
    """The standard licence texts are the bytes the SPDX License List published."""
    index = read_toml(os.path.join(directory, "index.toml"))
    pins = index.get("sha256", {})
    problems = []
    for licence, digest in sorted(pins.items()):
        path = os.path.join(directory, licence + ".txt")
        if not os.path.isfile(path):
            problems.append("the standard text of %s is not in %s" % (licence, relative(directory)))
        elif sha256_of(path) != digest:
            problems.append("%s.txt is not the text the SPDX License List published "
                            "(its digest moved), so the notices would quote an edited text" % licence)
    for name in sorted(os.listdir(directory)):
        if name.endswith(".txt") and name[:-4] not in pins:
            problems.append("%s/%s has no digest in index.toml" % (relative(directory), name))
    return problems, index.get("list", "")


# --------------------------------------------------------------------------- #
# licence expressions
# --------------------------------------------------------------------------- #

EXPRESSION_TOKEN = re.compile(r"\s*(\(|\)|[A-Za-z0-9][A-Za-z0-9.+:-]*)")


def normalised(expression):
    """The manifest's expression in SPDX syntax. Old manifests wrote `MIT/Apache-2.0`."""
    return re.sub(r"\s*/\s*", " OR ", (expression or "").strip())


def alternatives(expression):
    """The expression as the choices it offers: a list of sets of terms, any one of
    which satisfies it. `(MIT OR Apache-2.0) AND Unicode-3.0` is two choices, each
    of two terms. A term is a licence, or a licence WITH an exception."""
    text = normalised(expression)
    tokens = []
    position = 0
    while position < len(text):
        found = EXPRESSION_TOKEN.match(text, position)
        if not found:
            raise Refused(["%r is not a licence expression this script can read" % expression])
        tokens.append(found.group(1))
        position = found.end()
        while position < len(text) and text[position].isspace():
            position += 1
    if not tokens:
        raise Refused(["an empty licence expression"])

    def operator(token):
        return token.upper() if token.upper() in ("AND", "OR", "WITH") else None

    def parse_or(at):
        choices, at = parse_and(at)
        while at < len(tokens) and operator(tokens[at]) == "OR":
            more, at = parse_and(at + 1)
            choices = choices + [c for c in more if c not in choices]
        return choices, at

    def parse_and(at):
        choices, at = parse_primary(at)
        while at < len(tokens) and operator(tokens[at]) == "AND":
            more, at = parse_primary(at + 1)
            choices = [a | b for a in choices for b in more]
        return choices, at

    def parse_primary(at):
        if at >= len(tokens):
            raise Refused(["%r ends where a licence was expected" % expression])
        token = tokens[at]
        if token == "(":
            choices, at = parse_or(at + 1)
            if at >= len(tokens) or tokens[at] != ")":
                raise Refused(["%r opens a bracket it does not close" % expression])
            return choices, at + 1
        if token == ")" or operator(token):
            raise Refused(["%r has %s where a licence was expected" % (expression, token)])
        if at + 1 < len(tokens) and operator(tokens[at + 1]) == "WITH":
            if at + 2 >= len(tokens) or tokens[at + 2] == "(" or operator(tokens[at + 2]):
                raise Refused(["%r has WITH and no exception after it" % expression])
            return [frozenset(["%s WITH %s" % (token, tokens[at + 2])])], at + 3
        return [frozenset([token])], at + 1

    choices, at = parse_or(0)
    if at != len(tokens):
        raise Refused(["%r has %s where it should have ended" % (expression, tokens[at])])
    return choices


def parts_of(term):
    """The licences a term needs a text for: `Apache-2.0 WITH LLVM-exception` needs two."""
    return term.split(" WITH ")


def allowed_licences(path=os.path.join(ROOT, "deny.toml")):
    """The licences this project may ship under, in the order deny.toml lists them."""
    allow = read_toml(path).get("licenses", {}).get("allow", [])
    if not allow:
        raise Refused(["deny.toml allows no licence, so nothing could be shipped"])
    return {licence: rank for rank, licence in enumerate(allow)}


def choose(choices, allowed, has_text):
    """The choice a work is used under: allowed, and the first whose text it ships."""
    usable = [c for c in choices if all(term in allowed for term in c)]
    if not usable:
        return None
    usable.sort(key=lambda c: (sorted(allowed[t] for t in c), len(c), sorted(c)))
    for choice in usable:
        if all(has_text(part) for term in choice for part in parts_of(term)):
            return choice
    return usable[0]


def spelled(choice, allowed):
    """A choice written as an SPDX expression, in a stable order."""
    return " AND ".join(sorted(choice, key=lambda t: (allowed.get(t, len(allowed)), t)))


# --------------------------------------------------------------------------- #
# licence texts
# --------------------------------------------------------------------------- #

def folded(text):
    for curly, plain in TYPOGRAPHIC_QUOTES:
        text = text.replace(curly, plain)
    return re.sub(r"\s+", " ", text.lower())


def recognised(text):
    """Which licences a text holds, by what it says."""
    plain = folded(text)
    found = set()
    for licence, phrases in SIGNATURES:
        if all(phrase in plain for phrase in phrases):
            found.add(licence)
    if "BSD-3-Clause" in found:
        found.discard("BSD-2-Clause")
    # ISC and 0BSD open with the same grant, and only ISC asks for the notice to be kept.
    if "ISC" not in found and ZERO_BSD in plain:
        found.add("0BSD")
    return found


def read_text(path):
    with open(path, "rb") as handle:
        raw = handle.read()
    try:
        text = raw.decode("utf-8-sig")
    except UnicodeDecodeError:
        raise Refused(["%s is not UTF-8, and a notice has to quote it exactly" % path])
    return text.replace("\r\n", "\n").replace("\r", "\n").rstrip() + "\n"


def texts_shipped_in(directory, licence_file=None):
    """The licence texts a crate ships, by the licence each one holds.

    Returns {licence: (file name, text)} and the texts nothing recognised."""
    known = {}
    unknown = []
    reuse = os.path.join(directory, "LICENSES")
    if os.path.isdir(reuse):
        for name in sorted(os.listdir(reuse)):
            path = os.path.join(reuse, name)
            if os.path.isfile(path):
                known.setdefault(os.path.splitext(name)[0], ("LICENSES/" + name, read_text(path)))
    candidates = [n for n in sorted(os.listdir(directory))
                  if LICENCE_FILE.match(n) and os.path.isfile(os.path.join(directory, n))]
    if licence_file:
        named = os.path.normpath(licence_file)
        if os.path.isfile(os.path.join(directory, named)) and named not in candidates:
            candidates.append(named)
    for name in candidates:
        text = read_text(os.path.join(directory, name))
        holds = recognised(text)
        if not holds:
            unknown.append((name, text))
        for licence in holds:
            known.setdefault(licence, (name.replace(os.sep, "/"), text))
    return known, unknown


def standard_text(licence, directory=TEXTS):
    path = os.path.join(directory, licence + ".txt")
    return read_text(path) if os.path.isfile(path) else None


# --------------------------------------------------------------------------- #
# what goes into an archive
# --------------------------------------------------------------------------- #

@dataclasses.dataclass
class Part:
    """One thing somebody else wrote, as the bill of materials and the notices see it."""
    key: str
    name: str
    version: str
    declared: str
    concluded: str
    texts: list            # (licence, origin, text)
    references: dict       # LicenseRef id -> text, for the bill of materials
    crate: bool
    own: bool = False
    checksum: str = ""
    download: str = ""
    authors: tuple = ()


def cargo(*args, root=ROOT):
    done = subprocess.run(["cargo", *args], cwd=root, capture_output=True, text=True, encoding="utf-8")
    if done.returncode != 0:
        raise Refused(["cargo %s failed:\n%s" % (" ".join(args), done.stderr.strip())])
    return done.stdout


TREE_LINE = re.compile(r"^(?P<name>[A-Za-z0-9_-]+) v(?P<version>\S+)(?P<rest>.*)$")


def crates_compiled_into(package, target, members, root=ROOT):
    """The crates `cargo tree` says a program is built from for one target, minus our own."""
    out = cargo("tree", "-p", package, "--target", target, "-e", "normal,no-proc-macro",
                "--prefix", "none", "--format", "{p}", "--locked", root=root)
    found = set()
    for line in out.splitlines():
        line = line.strip()
        if not line:
            continue
        parsed = TREE_LINE.match(line)
        if not parsed:
            raise Refused(["cargo tree printed a line this script cannot read: %r" % line])
        if parsed.group("name") not in members:
            found.add((parsed.group("name"), parsed.group("version")))
    if not found:
        raise Refused(["cargo tree names no crate in %s for %s, which cannot be right" % (package, target)])
    return found


@dataclasses.dataclass
class Crates:
    packages: dict         # (name, version) -> cargo metadata of the package
    members: set           # names of this workspace's own packages
    lock: dict             # (name, version) -> Cargo.lock entry


def load_crates(root=ROOT):
    data = json.loads(cargo("metadata", "--format-version", "1", "--locked", root=root))
    packages = {(p["name"], p["version"]): p for p in data["packages"]}
    by_id = {p["id"]: p["name"] for p in data["packages"]}
    members = {by_id[i] for i in data["workspace_members"]}
    lock = {(p["name"], p["version"]): p for p in read_toml(os.path.join(root, "Cargo.lock")).get("package", [])}
    return Crates(packages, members, lock)


def authors_of(package):
    names = [re.sub(r"\s*<[^>]*>", "", a).strip() for a in package.get("authors") or []]
    return tuple(n for n in names if n)


def crate_part(name, version, crates, allowed, root_licence):
    """A crate as the bill of materials and the notices describe it, or its refusals."""
    problems = []
    package = crates.packages.get((name, version))
    entry = crates.lock.get((name, version), {})
    called = "%s %s" % (name, version)
    if package is None:
        return None, ["%s is in the tree and not in cargo metadata" % called]
    if entry.get("source") != CRATES_IO:
        problems.append("%s comes from %r, and a release is built from crates.io alone"
                        % (called, entry.get("source")))
    checksum = entry.get("checksum", "")
    if not re.fullmatch(r"[0-9a-f]{64}", checksum or ""):
        problems.append("%s has no sha256 in Cargo.lock" % called)
    declared = package.get("license")
    if not declared:
        problems.append("%s declares no licence expression, only %r. Teach deny.toml what it is "
                        "with a [[licenses.clarify]] entry, then name it here"
                        % (called, package.get("license_file")))
        return None, problems
    directory = os.path.dirname(package["manifest_path"])
    known, unknown = texts_shipped_in(directory, package.get("license_file"))
    try:
        choices = alternatives(declared)
    except Refused as refusal:
        return None, problems + ["%s: %s" % (called, p) for p in refusal.problems]
    # A crate under one licence that ships one text nothing recognises: that text is
    # its licence as its authors wrote it, and it is the one to give.
    if len(choices) == 1 and len(choices[0]) == 1 and len(unknown) == 1:
        only = next(iter(choices[0]))
        if only not in known and " WITH " not in only:
            known[only] = unknown.pop()
    choice = choose(choices, allowed, lambda licence: licence in known)
    if choice is None:
        return None, problems + ["%s is offered under %s, and deny.toml allows none of its choices"
                                 % (called, declared)]
    texts = []
    for term in sorted(choice, key=lambda t: allowed[t]):
        for licence in parts_of(term):
            if licence == root_licence and (licence not in known or recognised(known[licence][1]) == {licence}):
                # The program's own licence and nothing else in the file: LICENSE is
                # already in the archive, and thirty-five kilobytes of it again, in
                # another layout, would help nobody.
                texts.append((licence, "the same licence as LICENSE beside this file", ""))
            elif licence in known:
                file, text = known[licence]
                texts.append((licence, "as the crate ships it in %s" % file, text))
            else:
                text = standard_text(licence)
                if text is None:
                    problems.append("%s ships no text of %s, and %s has no standard text of it"
                                    % (called, licence, relative(TEXTS)))
                    continue
                texts.append((licence, "the standard text, because the crate ships none of its own", text))
    # Whatever else the crate ships about its licence and its authors goes in as it
    # is: a COPYRIGHT or COPYING statement, the short Apache notice some crates keep
    # in place of the licence, a list of third-party code inside the crate. These
    # hold the copyright lines a standard text has none of, and leaving one out is
    # the one mistake a notices file must not make.
    for file, text in unknown:
        texts.append(("Notice", "as the crate ships it in %s" % file, text))
    references = {}
    for choice_terms in choices:
        for term in choice_terms:
            for licence in parts_of(term):
                if licence.startswith("LicenseRef-"):
                    if licence in known:
                        references[licence] = known[licence][1]
                    else:
                        problems.append("%s declares %s and ships no LICENSES/%s file, so the bill "
                                        "of materials could not say what it is" % (called, licence, licence))
    if problems:
        return None, problems
    part = Part(
        key="crate:%s@%s" % (name, version), name=name, version=version,
        declared=normalised(declared), concluded=spelled(choice, allowed), texts=texts,
        references=references, crate=True, checksum=checksum,
        download="https://crates.io/api/v1/crates/%s/%s/download" % (name, version),
        authors=authors_of(package))
    return part, []


def component_part(component, root=ROOT):
    text = read_text(os.path.join(root, component.licence_text)) if component.licence_text else ""
    references = {}
    for choice in alternatives(component.licence):
        for term in choice:
            for licence in parts_of(term):
                if licence.startswith("LicenseRef-"):
                    references[licence] = text
    texts = [(component.licence, "as its authors ship it in %s" % component.licence_text, text)] if text else []
    return Part(
        key="component:%s" % component.name, name=component.name, version=component.version,
        declared=component.licence, concluded=component.licence, texts=texts,
        references=references, crate=False, own=component.own, download=component.source)


@dataclasses.dataclass
class Contents:
    archive: Archive
    parts: list            # Part, crates first, then the register, each sorted by name


def gather(archives, components, crates, allowed, root=ROOT):
    """What each archive contains, or every reason it cannot be said."""
    root_licence = load_workspace(root).licence
    if not os.path.isfile(os.path.join(root, "LICENSE")):
        raise Refused(["there is no LICENSE at the root, and the notices point at it"])
    problems = []
    made = {}
    result = []
    for archive in archives:
        parts = []
        try:
            found = crates_compiled_into(archive.package, archive.target, crates.members, root)
        except Refused as refusal:
            problems.extend(refusal.problems)
            continue
        for name, version in sorted(found):
            key = (name, version)
            if key not in made:
                made[key] = crate_part(name, version, crates, allowed, root_licence)
                problems.extend(made[key][1])
            if made[key][0] is not None:
                parts.append(made[key][0])
        for component in sorted(components, key=lambda c: c.name):
            if archive.program in component.programs:
                parts.append(component_part(component, root))
        result.append(Contents(archive, parts))
    if problems:
        raise Refused(sorted(set(problems)))
    return result


# --------------------------------------------------------------------------- #
# the bill of materials
# --------------------------------------------------------------------------- #

def spdx_id(kind, *words):
    return "SPDXRef-%s-%s" % (kind, re.sub(r"[^A-Za-z0-9.-]+", "-", "-".join(words)).strip("-"))


def purl(name, version):
    return "pkg:cargo/%s@%s" % (name, version.replace("+", "%2B"))


def commit_time(root=ROOT):
    """When the commit being described was made, so two runs on one tag agree."""
    done = subprocess.run(["git", "log", "-1", "--format=%cI"], cwd=root, capture_output=True, text=True)
    if done.returncode != 0 or not done.stdout.strip():
        raise Refused(["git cannot say when HEAD was committed:\n%s" % done.stderr.strip()])
    when = datetime.datetime.fromisoformat(done.stdout.strip()).astimezone(datetime.timezone.utc)
    return when.strftime("%Y-%m-%dT%H:%M:%SZ"), commit_of(root)


def commit_of(root=ROOT):
    done = subprocess.run(["git", "rev-parse", "HEAD"], cwd=root, capture_output=True, text=True)
    return done.stdout.strip()


def bill_of_materials(contents, workspace, tag, version, created, commit, licence_list):
    """One SPDX 2.3 document. Each archive is a package of its own, with what it contains."""
    seen = {}
    packages = {}
    relationships = []
    references = {}

    def claim(identifier, key):
        if seen.setdefault(identifier, key) != key:
            raise Refused(["%s and %s both become %s once their names are made SPDX identifiers"
                           % (seen[identifier], key, identifier)])
        return identifier

    for content in contents:
        archive = content.archive
        name = archive.name(version)
        root_id = claim(spdx_id("Archive", name), "archive:" + name)
        packages[root_id] = {
            "SPDXID": root_id,
            "name": name,
            "versionInfo": version,
            "downloadLocation": "%s/releases/download/%s/%s" % (workspace.repository, tag, name),
            "filesAnalyzed": False,
            "licenseConcluded": workspace.licence,
            "licenseDeclared": workspace.licence,
            "copyrightText": "NOASSERTION",
            "supplier": SUPPLIER,
            "primaryPackagePurpose": "APPLICATION",
            "comment": "%s, %s, for %s on %s, built from %s at %s (%s)" % (
                archive.program, archive.summary, archive.os, archive.arch,
                workspace.repository, tag, commit or "commit unknown"),
        }
        relationships.append({"spdxElementId": "SPDXRef-DOCUMENT",
                              "relationshipType": "DESCRIBES", "relatedSpdxElement": root_id})
        for part in content.parts:
            if part.crate:
                identifier = claim(spdx_id("Crate", part.name, part.version), part.key)
                packages.setdefault(identifier, {
                    "SPDXID": identifier,
                    "name": part.name,
                    "versionInfo": part.version,
                    "downloadLocation": part.download,
                    "filesAnalyzed": False,
                    "checksums": [{"algorithm": "SHA256", "checksumValue": part.checksum}],
                    "licenseConcluded": part.concluded,
                    "licenseDeclared": part.declared,
                    "copyrightText": "NOASSERTION",
                    "primaryPackagePurpose": "LIBRARY",
                    "externalRefs": [{"referenceCategory": "PACKAGE-MANAGER",
                                      "referenceType": "purl",
                                      "referenceLocator": purl(part.name, part.version)}],
                })
            else:
                identifier = claim(spdx_id("Component", part.name), part.key)
                entry = {
                    "SPDXID": identifier,
                    "name": part.name,
                    "downloadLocation": part.download or "NOASSERTION",
                    "filesAnalyzed": False,
                    "licenseConcluded": part.concluded,
                    "licenseDeclared": part.declared,
                    "copyrightText": "NOASSERTION",
                }
                if part.version:
                    entry["versionInfo"] = part.version
                if part.own:
                    entry["supplier"] = SUPPLIER
                packages.setdefault(identifier, entry)
            references.update(part.references)
            relationships.append({"spdxElementId": root_id,
                                  "relationshipType": "CONTAINS", "relatedSpdxElement": identifier})

    digest = hashlib.sha256(("%s|%s" % (tag, commit)).encode("utf-8")).hexdigest()[:16]
    document = {
        "spdxVersion": "SPDX-2.3",
        "dataLicense": "CC0-1.0",
        "SPDXID": "SPDXRef-DOCUMENT",
        "name": "naughty-keyboard-%s" % version,
        "documentNamespace": "%s/spdx/%s/%s" % (workspace.repository, tag, digest),
        "creationInfo": {
            "created": created,
            "creators": ["Tool: %s" % TOOL, SUPPLIER],
            "licenseListVersion": re.sub(r"^.*?(\d+\.\d+).*$", r"\1", licence_list),
            "comment": "The crates are what cargo tree names for each program and target, with "
                       "licences from their manifests and digests from Cargo.lock. The other "
                       "components come from .github/release/components.toml, which a check holds "
                       "to the source. Written by .github/scripts/contents.py, not by a scanner "
                       "of the built archives.",
        },
        "comment": "Every archive carries %s, with the licence text of everything listed here."
                   % NOTICES_NAME,
        "packages": [packages[k] for k in sorted(packages)],
        "relationships": sorted(relationships, key=lambda r: (r["spdxElementId"], r["relationshipType"],
                                                              r["relatedSpdxElement"])),
    }
    if references:
        # Plural in JSON. SPDX 2.3 spells the key `hasExtractedLicensingInfos` in its
        # JSON schema and `hasExtractedLicensingInfo` only in RDF, and a parser given
        # the singular in JSON drops every definition without a word - measured with
        # spdx-tools 0.8.5 on 2026-10-09, which then called each LicenseRef unknown.
        document[EXTRACTED] = [
            {"licenseId": ref, "name": ref[len("LicenseRef-"):], "extractedText": text}
            for ref, text in sorted(references.items())]
    problems = validate(document)
    if problems:
        raise Refused(problems)
    return document


def validate(document):
    """The checks a malformed document would fail, run before anything is written."""
    problems = []
    ids = [p["SPDXID"] for p in document["packages"]]
    if not ids:
        problems.append("the bill of materials lists no packages")
    if len(set(ids)) != len(ids):
        problems.append("two packages share an SPDX identifier")
    for identifier in ids:
        if not re.fullmatch(r"SPDXRef-[A-Za-z0-9.-]+", identifier):
            problems.append("%s is not an SPDX identifier" % identifier)
    known = set(ids) | {"SPDXRef-DOCUMENT"}
    for relationship in document["relationships"]:
        for end in (relationship["spdxElementId"], relationship["relatedSpdxElement"]):
            if end not in known:
                problems.append("a relationship points at %s, which is in no package" % end)
    unknown_keys = sorted(set(document) - DOCUMENT_KEYS)
    if unknown_keys:
        problems.append("the document carries %s, which SPDX 2.3 JSON does not define, so a "
                        "reader would drop it without a word" % ", ".join(unknown_keys))
    defined = {e["licenseId"] for e in document.get(EXTRACTED, [])}
    for package in document["packages"]:
        for field in ("name", "downloadLocation", "licenseConcluded", "licenseDeclared", "copyrightText"):
            if not package.get(field):
                problems.append("%s has no %s, which SPDX requires" % (package["SPDXID"], field))
        for field in ("licenseConcluded", "licenseDeclared"):
            for ref in re.findall(r"LicenseRef-[A-Za-z0-9.-]+", package.get(field, "")):
                if ref not in defined:
                    problems.append("%s uses %s, which the document does not define" % (package["SPDXID"], ref))
    described = [r for r in document["relationships"] if r["relationshipType"] == "DESCRIBES"]
    if not described:
        problems.append("the document describes nothing")
    return problems


# --------------------------------------------------------------------------- #
# the notices
# --------------------------------------------------------------------------- #

RULE = "=" * 78
THIN = "-" * 78


def notices(content, workspace, tag, version, licence_list):
    """The text that goes into an archive as THIRD-PARTY-NOTICES.txt."""
    archive = content.archive
    lines = [
        "Third-party notices for %s %s (%s, %s)" % (archive.program, version, archive.os, archive.arch),
        "",
        "%s is free software under %s. The full text of that licence is in" % (archive.program, workspace.licence),
        "LICENSE beside this file, and the source this build was made from is at",
        "%s/tree/%s" % (workspace.repository, tag),
        "",
        "The program also contains the works listed below. Each one is used under",
        "the licence named beside it, and the text of that licence follows, given",
        "once for every work that ships the same text. Where a work ships no licence",
        "text of its own, the standard text from the %s is" % licence_list,
        "given instead, and its entry says so.",
        "",
    ]
    others = [p for p in content.parts if not p.own]
    own = [p for p in content.parts if p.own and p.concluded != workspace.licence]
    lines.append("Contents")
    lines.append(THIN)
    width = max(len(label(p)) for p in others) if others else 0
    for part in others:
        lines.append("  %s  %s" % (label(part).ljust(width), part.concluded))
    lines.append("")
    if own:
        lines.append("Parts of this program under another licence than %s" % workspace.licence)
        lines.append(THIN)
        for part in own:
            lines.append("  %s: %s, https://spdx.org/licenses/%s.html"
                         % (part.name, part.concluded, part.concluded))
        lines.append("")

    groups = {}
    order = []
    for part in others:
        for licence, origin, text in part.texts:
            key = (licence, origin.startswith("the standard text"), text)
            if key not in groups:
                groups[key] = []
                order.append(key)
            groups[key].append(part)
    for key in order:
        licence, standard, text = key
        members = groups[key]
        lines.append(RULE)
        lines.extend(wrapped(", ".join(label(p) for p in members)))
        origins = sorted({o for p in members for (l, o, t) in p.texts if l == licence and t == text})
        lines.append("%s, %s" % (licence, origins[0] if len(origins) == 1 else "as each of them ships it"))
        if standard:
            for part in members:
                if part.crate:
                    who = ", ".join(part.authors) if part.authors else "no authors named in its manifest"
                    lines.extend(wrapped("%s: %s" % (label(part), who)))
        if text:
            lines.append(THIN)
            lines.append(text.rstrip("\n"))
        lines.append("")
    return "\n".join(lines).rstrip("\n") + "\n"


def label(part):
    return "%s %s" % (part.name, part.version) if part.version else part.name


def wrapped(text, width=78):
    words = text.split(" ")
    lines = []
    current = ""
    for word in words:
        if current and len(current) + 1 + len(word) > width:
            lines.append(current)
            current = word
        else:
            current = word if not current else current + " " + word
    if current:
        lines.append(current)
    return lines


# --------------------------------------------------------------------------- #
# writing, and the command line
# --------------------------------------------------------------------------- #

def write(path, text):
    """Written whole or not at all, with LF line ends on every system."""
    folder = os.path.dirname(os.path.abspath(path))
    os.makedirs(folder, exist_ok=True)
    temporary = path + ".partial"
    with open(temporary, "w", encoding="utf-8", newline="\n") as handle:
        handle.write(text)
    os.replace(temporary, path)


def prepare(root=ROOT):
    """Everything every command needs, with every refusal collected."""
    problems = []
    workspace = load_workspace(root)
    archives = load_archives()
    components, allowances = load_register(programs={a.program for a in archives}, root=root)
    problems.extend(check_register_against_source(components, allowances, root))
    text_problems, licence_list = check_standard_texts()
    problems.extend(text_problems)
    allowed = allowed_licences(os.path.join(root, "deny.toml"))
    if problems:
        raise Refused(problems)
    return workspace, archives, components, allowed, licence_list


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("check", help="describe every archive, write nothing")
    sbom = commands.add_parser("sbom", help="write the bill of materials for every archive of a tag")
    sbom.add_argument("--tag", required=True)
    sbom.add_argument("--out", required=True)
    notice = commands.add_parser("notices", help="write the notices for one archive of a tag")
    notice.add_argument("--tag", required=True)
    notice.add_argument("--program", required=True)
    notice.add_argument("--os", required=True)
    notice.add_argument("--out", required=True)
    args = parser.parse_args(argv)

    try:
        workspace, archives, components, allowed, licence_list = prepare()
        tag = args.tag if args.command != "check" else "v" + workspace.version
        version = version_of_tag(tag, workspace)
        if args.command == "notices":
            archives = [a for a in archives if a.program == args.program and a.os == args.os]
            if not archives:
                raise Refused(["archives.toml has no archive of %s for %s" % (args.program, args.os)])
        contents = gather(archives, components, load_crates(), allowed)
        if args.command == "notices":
            write(args.out, notices(contents[0], workspace, tag, version, licence_list))
            print("wrote %s for %s" % (args.out, contents[0].archive.name(version)))
            return 0
        created, commit = commit_time() if args.command == "sbom" else ("1970-01-01T00:00:00Z", "")
        document = bill_of_materials(contents, workspace, tag, version, created, commit, licence_list)
        for content in contents:
            text = notices(content, workspace, tag, version, licence_list)
            crates = sum(1 for p in content.parts if p.crate)
            standard = sum(1 for p in content.parts
                           if any(o.startswith("the standard text") for _, o, _ in p.texts))
            print("  %-36s %3d crates, %d other, notices %4d KB, %d with a standard text"
                  % (content.archive.name(version), crates, len(content.parts) - crates,
                     len(text.encode("utf-8")) // 1024, standard))
        if args.command == "sbom":
            write(args.out, json.dumps(document, indent=2, ensure_ascii=False) + "\n")
            print("wrote %s: %d packages, %d archives" % (args.out, len(document["packages"]), len(contents)))
        else:
            print("every archive can be described: %d packages in the bill of materials" % len(document["packages"]))
        return 0
    except Refused as refusal:
        print("contents: the release cannot be described, and nothing was written:", file=sys.stderr)
        for problem in refusal.problems:
            print("  - " + problem.replace("\n", "\n    "), file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
