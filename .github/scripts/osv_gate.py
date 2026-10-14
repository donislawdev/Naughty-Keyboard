#!/usr/bin/env python3
"""Decides a pull request from an OSV-Scanner JSON report.

    osv-scanner scan source --recursive --format=json --all-packages --output-file=osv.json ./
    python3 .github/scripts/osv_gate.py osv.json --root /github/workspace \\
        --expect Cargo.lock --expect .github/scripts/requirements.txt

Why the verdict is made here and not by the scanner's exit code
----------------------------------------------------------------
Two reasons, both about a scan that read less than it should have.

- The action that runs the scanner turns "no lock file found" into success,
  unless it is told `--allow-no-lockfiles=false`. The workflow tells it, and
  this does not rely on that alone.
- No exit code says which files were read. If Cargo.lock were moved, renamed or
  left out by a wrong exclusion, the scanner would read the rest, find nothing
  and pass. This is told which files must be read, by `--expect`, and refuses a
  report in which one of them is missing or holds no package.

What blocks
-----------
Any advisory in the report, of any kind and any severity. An advisory that
does not apply to this project is named in osv-scanner.toml with the reason,
and the scanner leaves it out of the report and says so in its log.

What is not a verdict at all
----------------------------
A report that is missing, is not JSON, is not an OSV-Scanner report, or lacks a
file it was expected to read. The exit code is then 2.

Exit codes: 0 passes, 1 blocks, 2 the report cannot be used. With `--canary`
the meaning of 0 and 1 is turned around, for a report made from a lock file
known to be bad: 0 means the gate blocked it, 3 means the gate let it through.
"""

import argparse
import json
import posixpath
import sys

EXIT_PASSES = 0
EXIT_BLOCKS = 1
EXIT_UNUSABLE = 2
EXIT_CANARY_PASSED = 3

SUMMARY_LIMIT = 200


class Unusable(Exception):
    """The report cannot be trusted to say what was scanned."""


def load(path):
    """The report as a dict, or Unusable with the reason."""
    try:
        with open(path, encoding="utf-8") as handle:
            report = json.load(handle)
    except (OSError, ValueError) as error:
        raise Unusable(f"cannot read {path}: {error}") from error
    if not isinstance(report, dict) or not isinstance(report.get("results"), list):
        raise Unusable(f"{path} is not an OSV-Scanner report")
    return report


def relative(path, root):
    """A scanned file's path from the root of the scan, the way --expect names it."""
    path = str(path)
    root = root.rstrip("/")
    if root and path.startswith(root + "/"):
        return path[len(root) + 1:]
    return posixpath.normpath(path)


def packages_by_file(report, root):
    """{file: [package entries]} for every file the report says was read."""
    files = {}
    for result in report["results"]:
        source = result.get("source") if isinstance(result, dict) else None
        if not isinstance(source, dict) or not source.get("path"):
            raise Unusable("a result in the report names no file")
        packages = result.get("packages")
        if not isinstance(packages, list):
            raise Unusable(f"the result for {source['path']} lists no packages")
        for entry in packages:
            if not isinstance(entry, dict) or not isinstance(entry.get("package"), dict):
                raise Unusable(f"a package of {source['path']} is not an object")
        files.setdefault(relative(source["path"], root), []).extend(packages)
    return files


def missing(files, expected):
    """The expected files the report did not read, or read with no package."""
    return [name for name in expected if not files.get(name)]


def squash(text):
    return " ".join(str(text).split())


def groups_of(entry):
    """The advisories against one package, each a list of the ids that name it.

    The scanner groups the ids that name one advisory, RUSTSEC and GHSA for
    example. Without groups every advisory stands alone."""
    vulnerabilities = entry.get("vulnerabilities") or []
    if not isinstance(vulnerabilities, list):
        raise Unusable("a package's vulnerabilities are not a list")
    known = {}
    for vulnerability in vulnerabilities:
        if not isinstance(vulnerability, dict) or not vulnerability.get("id"):
            raise Unusable("an advisory in the report is not an object with an id")
        known[str(vulnerability["id"])] = vulnerability
    out, seen = [], set()
    for group in entry.get("groups") or []:
        ids = [str(i) for i in group.get("ids", [])] if isinstance(group, dict) else []
        ids = [i for i in ids if i in known and i not in seen]
        if ids:
            out.append(ids)
            seen.update(ids)
    out += [[i] for i in known if i not in seen]
    return out, known


def findings(files):
    """One line per advisory against a package, with the file it came from."""
    lines = []
    for name in sorted(files):
        for entry in files[name]:
            package = entry["package"]
            groups, known = groups_of(entry)
            for ids in groups:
                summary = next((known[i].get("summary") for i in ids if known[i].get("summary")), "")
                lines.append("{} {} {} ({}) in {}\n      {}".format(
                    " / ".join(ids),
                    package.get("name", "?"),
                    package.get("version", "?"),
                    package.get("ecosystem", "?"),
                    name,
                    squash(summary)[:SUMMARY_LIMIT] or "no summary",
                ))
    return lines


def report_lines(files, found):
    lines = []
    if found:
        lines.append("advisories (each blocks, unless osv-scanner.toml names it with a reason):")
        lines += ["  " + line for line in found]
    lines.append("files read:")
    lines += [f"  {name}: {len(files[name])} package(s)" for name in sorted(files)]
    lines.append(f"osv: {len(found)} advisory finding(s), {sum(map(len, files.values()))} "
                 f"package(s) in {len(files)} file(s)")
    return lines


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("report", help="the JSON file that osv-scanner --output-file wrote")
    parser.add_argument("--root", default="", help="the directory the scan ran in")
    parser.add_argument(
        "--expect",
        action="append",
        default=[],
        help="a file, from the root, that the scan must have read (repeatable)",
    )
    parser.add_argument(
        "--canary",
        action="store_true",
        help="the report was made from a lock file known to be bad, so the gate must block it",
    )
    args = parser.parse_args(argv)
    if not args.expect:
        parser.error("name at least one file with --expect, or a scan of nothing would pass")
    try:
        files = packages_by_file(load(args.report), args.root)
        unread = missing(files, args.expect)
        if unread:
            raise Unusable("not read, so not checked: " + ", ".join(unread))
        found = findings(files)
    except Unusable as reason:
        print(f"osv gate: {reason}", file=sys.stderr)
        return EXIT_UNUSABLE
    for line in report_lines(files, found):
        print(line)
    if args.canary:
        if found:
            print("osv gate: the canary was blocked, so the gate can block")
            return EXIT_PASSES
        print("osv gate: the canary was NOT blocked, so this gate cannot be trusted")
        return EXIT_CANARY_PASSED
    return EXIT_BLOCKS if found else EXIT_PASSES


if __name__ == "__main__":
    raise SystemExit(main())
