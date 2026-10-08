#!/usr/bin/env python3
"""Decides a pull request from a Semgrep JSON report.

    semgrep scan --config p/default --json --output semgrep.json
    python3 .github/scripts/semgrep_gate.py semgrep.json

Why the verdict is made here and not by `semgrep --severity ERROR --error`
------------------------------------------------------------------------
That flag does not mean what it reads like. `--severity` accepts exactly INFO,
WARNING and ERROR, while a rule may declare the newer scale, and rules in the
registry do. Measured on another project with a probe rule that declared HIGH:
without the filter the scan reported 19 findings, with `--severity ERROR` it
reported none. A gate built on the flag silently ignores exactly the severities
it was asked to block. Reading the report and deciding here cannot do that.

What blocks
-----------
- A finding of severity ERROR, HIGH or CRITICAL.
- A finding of a severity this script does not know. A word that appears in a
  future Semgrep release must fail loudly once and be added here on purpose,
  and must not pass in silence.
- A scan error of level error. A rule that could not run is not a rule that
  found nothing.

What passes, and is printed
---------------------------
INFO, WARNING, LOW, MEDIUM, EXPERIMENT and INVENTORY findings, grouped by rule.
A WARNING here is usually a rule meeting a shape it cannot resolve, and a gate
that fires on those is a gate nobody reads.

What is not a verdict at all
----------------------------
A report that is missing, is not JSON, is not a Semgrep report, or lists no
scanned file. "The scan was green" must never mean "the scan did not happen".
The exit code is then 2.

Exit codes: 0 passes, 1 blocks, 2 the report cannot be used. With `--canary`
the meaning of 0 and 1 is turned around, for a report made from a file known to
be bad: 0 means the gate blocked it, 3 means the gate let it through.
"""

import argparse
import collections
import json
import sys

PASSING = ("INFO", "WARNING", "LOW", "MEDIUM", "EXPERIMENT", "INVENTORY")

EXIT_PASSES = 0
EXIT_BLOCKS = 1
EXIT_UNUSABLE = 2
EXIT_CANARY_PASSED = 3

MESSAGE_LIMIT = 300


class Unusable(Exception):
    """The report cannot be trusted to say that a scan happened."""


def load(path):
    """The report as a dict, or Unusable with the reason."""
    try:
        with open(path, encoding="utf-8") as handle:
            report = json.load(handle)
    except (OSError, ValueError) as error:
        raise Unusable(f"cannot read {path}: {error}") from error
    if not isinstance(report, dict) or not isinstance(report.get("results"), list):
        raise Unusable(f"{path} is not a Semgrep report")
    paths = report.get("paths")
    scanned = paths.get("scanned") if isinstance(paths, dict) else None
    if not isinstance(scanned, list) or not scanned:
        raise Unusable(f"{path} lists no scanned file, so nothing was checked")
    return report


def severity_of(result):
    """The finding's severity in capitals, or an empty string when it has none."""
    extra = result.get("extra") if isinstance(result, dict) else None
    value = extra.get("severity") if isinstance(extra, dict) else None
    return str(value).upper() if value is not None else ""


def split(report):
    """(blocking findings, passing findings, scan errors, scan notes)."""
    blocking, passing = [], []
    for result in report["results"]:
        if not isinstance(result, dict):
            raise Unusable("a finding in the report is not an object")
        (passing if severity_of(result) in PASSING else blocking).append(result)
    errors, notes = [], []
    for problem in report.get("errors") or []:
        if not isinstance(problem, dict):
            continue
        level = str(problem.get("level", "")).lower()
        (errors if level == "error" else notes).append(problem)
    return blocking, passing, errors, notes


def squash(text):
    return " ".join(str(text).split())


def describe(result):
    extra = result.get("extra") if isinstance(result.get("extra"), dict) else {}
    start = result.get("start") if isinstance(result.get("start"), dict) else {}
    severity = severity_of(result) or "NO SEVERITY"
    unknown = "" if severity in ("ERROR", "HIGH", "CRITICAL") else " (severity not known)"
    return "{}:{}  [{}]{} {}\n      {}".format(
        result.get("path", "?"),
        start.get("line", "?"),
        severity,
        unknown,
        result.get("check_id", "?"),
        squash(extra.get("message", ""))[:MESSAGE_LIMIT],
    )


def problem_line(problem):
    kind = problem.get("type", "?")
    if isinstance(kind, list) and kind:
        kind = kind[0]
    return f"{kind}: {squash(problem.get('message', ''))[:200]}"


def grouped(passing):
    """One line per rule and severity: how many findings, in how many files."""
    seen = collections.defaultdict(list)
    for result in passing:
        key = (severity_of(result), str(result.get("check_id", "?")))
        seen[key].append(str(result.get("path", "?")))
    rows = sorted(seen.items(), key=lambda item: (-len(item[1]), item[0]))
    return [
        f"  {len(files)} x [{severity}] {rule} in {len(set(files))} file(s)"
        for (severity, rule), files in rows
    ]


def report_lines(blocking, passing, errors, notes, scanned):
    lines = []
    if errors:
        lines.append("scan errors (a rule that could not run is not a rule that passed):")
        lines += ["  " + problem_line(problem) for problem in errors]
    if blocking:
        lines.append("blocking findings:")
        lines += ["  " + describe(result) for result in blocking]
    if passing:
        lines.append("other findings (reported, not blocking):")
        lines += grouped(passing)
    if notes:
        lines.append("scan notes (not blocking):")
        lines += ["  " + problem_line(problem) for problem in notes]
    lines.append(
        f"semgrep: {len(blocking)} blocking, {len(passing)} other, "
        f"{len(errors)} scan error(s), {scanned} file(s) scanned"
    )
    return lines


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("report", help="the JSON file that semgrep --output wrote")
    parser.add_argument(
        "--canary",
        action="store_true",
        help="the report was made from a file known to be bad, so the gate must block it",
    )
    args = parser.parse_args(argv)
    try:
        report = load(args.report)
        blocking, passing, errors, notes = split(report)
    except Unusable as reason:
        print(f"semgrep gate: {reason}", file=sys.stderr)
        return EXIT_UNUSABLE
    scanned = len(report["paths"]["scanned"])
    for line in report_lines(blocking, passing, errors, notes, scanned):
        print(line)
    blocked = bool(blocking or errors)
    if args.canary:
        if blocked:
            print("semgrep gate: the canary was blocked, so the gate can block")
            return EXIT_PASSES
        print("semgrep gate: the canary was NOT blocked, so this gate cannot be trusted")
        return EXIT_CANARY_PASSED
    return EXIT_BLOCKS if blocked else EXIT_PASSES


if __name__ == "__main__":
    raise SystemExit(main())
