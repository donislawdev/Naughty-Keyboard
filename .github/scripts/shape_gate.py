#!/usr/bin/env python3
"""Holds the shape of the Rust code to its ceilings, as a ratchet.

    python3 .github/scripts/shape_gate.py            # nothing is over a ceiling
    python3 .github/scripts/shape_gate.py --pinned   # and every number is the measurement

Why a ceiling, when nothing has gone wrong
------------------------------------------
Nobody reads this code line by line, and no reviewer notices a function that
reached three hundred lines over four changes, each of which added twenty and
each of which was reasonable on its own. A ceiling is the one thing that
notices, and it costs nothing while nothing grows. The numbers are today's
largest, not a textbook figure: down is routine work, up is the owner's
decision and never a way to get a red build green.

What is measured, and by whom
-----------------------------
Clippy measures, and this script only asks it. Four lints, each a different
question: how long a function is (`too_many_lines`, executable lines, so a
comment is free), how much it branches (`cognitive_complexity`), how deep its
blocks go (`excessive_nesting`) and how many things a caller has to line up
(`too_many_arguments`). A second metric written here beside clippy's would be a
second thing to get wrong.

Two scopes, because the two differ threefold. The code the programs are made of
(`--lib --bins`) answers to the ceilings in .github/shape.toml. Everything,
tests and examples included (`--all-targets`), answers to clippy.toml at the
root, which every `cargo clippy` reads, so `-D warnings` in CI and in the local
gates already holds that scope to its numbers.

Two knobs, because one is blind
-------------------------------
A ceiling on the worst function sees one thing growing to a record. It does not
see five functions creeping up together, none of them a record. So the number
of functions standing near each ceiling is frozen too. For length and
complexity, near means at or above `band` of the ceiling. For nesting and
arguments, whose numbers are small, near means at the ceiling itself, because a
band that is a fraction of 7 reshapes itself whenever the ceiling moves.

The ceiling must BE the measurement
-----------------------------------
A ceiling left above the largest function grants room nobody decided to grant,
and the next arrival slips in under it in silence. "Is the code under the
number" can never see that. So with `--pinned` each ceiling has to be reached
exactly, and each frozen count has to equal what is measured. Shrinking a
function therefore comes with a chore: bring its number down in the same change.

How it asks
-----------
Clippy reports a function only above a threshold, and says by how much. So the
script writes a clippy.toml of its own into a temporary directory, with each
threshold lowered to the edge of the band, points `CLIPPY_CONF_DIR` at it and
reads the numbers out of the JSON. Nesting reports no number, so its run asks
one level below the ceiling, and a second run at the ceiling itself says that
nothing goes deeper. Lints are capped at warn in these runs, so one crate that
fails cannot stop the crates above it from being measured.

Where it is pinned
------------------
Code behind `cfg(windows)` is compiled on Windows only, and the holder of a
ceiling may be such code. Measured 2026-10-08: on Linux and macOS the holders of
the crates without a window are the same and their numbers are not higher. So
`--pinned` runs on Windows, and the other two systems ask only that nothing is
over a ceiling. A ceiling held by code that compiles only on Linux or macOS
would show as room on Windows. That is a limit, and it is written here.

Exit codes: 0 passes, 1 blocks, 2 the measurement cannot be used.
"""

import argparse
import json
import math
import os
import re
import subprocess
import sys
import tempfile
import tomllib
from dataclasses import dataclass
from fractions import Fraction

EXIT_PASSES = 0
EXIT_BLOCKS = 1
EXIT_UNUSABLE = 2

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
CLIPPY_CONFIG = "clippy.toml"
SHAPE_CONFIG = os.path.join(".github", "shape.toml")


@dataclass(frozen=True)
class Axis:
    """One ceiling. `numbered` says whether clippy reports how far over it is."""

    key: str
    lint: str
    numbered: bool
    near_is_band: bool


# The four axes, keyed as clippy.toml keys them. A threshold this script does
# not know is refused, so a fifth one is added here on purpose and never passes
# unmeasured.
AXES = (
    Axis("too-many-lines-threshold", "too_many_lines", numbered=True, near_is_band=True),
    Axis(
        "cognitive-complexity-threshold",
        "cognitive_complexity",
        numbered=True,
        near_is_band=True,
    ),
    Axis(
        "too-many-arguments-threshold",
        "too_many_arguments",
        numbered=True,
        near_is_band=False,
    ),
    Axis(
        "excessive-nesting-threshold",
        "excessive_nesting",
        numbered=False,
        near_is_band=False,
    ),
)

SCOPES = {
    "product": ("--lib", "--bins"),
    "all": ("--all-targets",),
}

# `(75/74)` in "the function has a cognitive complexity of (75/74)".
NUMBERS = re.compile(r"\((\d+)/(\d+)\)")
FUNCTION = re.compile(r"\bfn\s+([A-Za-z_][A-Za-z0-9_]*)")
THRESHOLD_LINE = re.compile(r"^(\s*)([a-z-]+-threshold)(\s*=\s*)(\d+)(.*)$")


class Unusable(Exception):
    """The measurement cannot be trusted to say what the code looks like."""


@dataclass(frozen=True)
class Finding:
    """One function or block that clippy reported over a threshold."""

    lint: str
    path: str
    line: int
    value: int | None
    name: str

    def where(self):
        shown = f"{self.path}:{self.line}"
        return f"{shown} {self.name}" if self.name else shown


@dataclass(frozen=True)
class Settings:
    """The ceilings of both scopes, the band and the frozen counts."""

    ceilings: dict
    near: dict
    band: float
    members: tuple


def read_thresholds(text, source):
    """The shape thresholds in a clippy.toml, refusing any it does not know."""
    known = {axis.key for axis in AXES}
    found = {}
    for line in text.splitlines():
        match = THRESHOLD_LINE.match(line)
        if not match:
            continue
        key = match.group(2)
        if key not in known:
            raise Unusable(
                f"{source} sets {key}, which this gate does not measure - add it to AXES on purpose"
            )
        found[key] = int(match.group(4))
    missing = sorted(known - set(found))
    if missing:
        raise Unusable(f"{source} has no {', '.join(missing)}, so that axis runs at clippy's default")
    return found


def read_settings(root):
    """Both scopes' ceilings and counts, checked for being whole before anything runs."""
    try:
        with open(os.path.join(root, CLIPPY_CONFIG), encoding="utf-8") as handle:
            clippy_text = handle.read()
        with open(os.path.join(root, SHAPE_CONFIG), "rb") as handle:
            shape = tomllib.load(handle)
        with open(os.path.join(root, "Cargo.toml"), "rb") as handle:
            members = tuple(tomllib.load(handle)["workspace"]["members"])
    except (OSError, tomllib.TOMLDecodeError, KeyError) as error:
        raise Unusable(f"the configuration cannot be read: {error}") from error

    ceilings = {"all": read_thresholds(clippy_text, CLIPPY_CONFIG)}
    clippy = shape.get("clippy", {})
    product = clippy.get("product", {})
    ceilings["product"] = read_thresholds(
        "\n".join(f"{key} = {value}" for key, value in product.items()), SHAPE_CONFIG
    )
    near = {}
    for scope in SCOPES:
        counts = clippy.get("near", {}).get(scope, {})
        if set(counts) != {axis.key for axis in AXES}:
            raise Unusable(f"{SHAPE_CONFIG} [clippy.near.{scope}] does not name each axis once")
        if any(not isinstance(count, int) or count < 0 for count in counts.values()):
            raise Unusable(f"{SHAPE_CONFIG} [clippy.near.{scope}] holds a count that is not a whole number")
        near[scope] = dict(counts)
    band = clippy.get("band")
    if not isinstance(band, float) or not 0.0 < band < 1.0:
        raise Unusable(f"{SHAPE_CONFIG} [clippy] band must be a fraction, not {band!r}")
    for axis in AXES:
        if ceilings["product"][axis.key] > ceilings["all"][axis.key]:
            raise Unusable(
                f"the product ceiling of {axis.lint} is above the ceiling of all the code, "
                "which tests and examples are part of"
            )
    return Settings(ceilings=ceilings, near=near, band=band, members=members)


def near_edge(axis, ceiling, band):
    """The smallest value that counts as near the ceiling.

    At or above `ceiling * band` for a band axis, and the ceiling itself for the
    others. Worked out in exact fractions, because floating point moves the edge
    for some bands. Measured 2026-10-08: at 0.7 it never does for a ceiling below
    5000, but at 0.55 the product 100 * 0.55 is a hair above 55 and the edge
    would move to 56, and twelve of the 99 two-digit bands do the same somewhere.
    """
    if axis.near_is_band:
        return math.ceil(Fraction(ceiling) * Fraction(repr(band)))
    return ceiling


def band_threshold(axis, ceiling, band):
    """The threshold above which clippy reports exactly what is near the ceiling.

    Clippy reports a value only when it is greater than the threshold, so the
    threshold is one below the edge. Nesting has no number, so its run asks one
    level below the ceiling and every block reported there is at the ceiling
    or deeper.
    """
    return max(near_edge(axis, ceiling, band) - 1, 0)


def with_thresholds(text, thresholds):
    """`text` of a clippy.toml with each shape threshold replaced, the rest kept."""
    out = []
    for line in text.splitlines():
        match = THRESHOLD_LINE.match(line)
        if match and match.group(2) in thresholds:
            line = (
                f"{match.group(1)}{match.group(2)}{match.group(3)}"
                f"{thresholds[match.group(2)]}{match.group(5)}"
            )
        out.append(line)
    return "\n".join(out) + "\n"


def findings_from(json_lines, thresholds, members):
    """What clippy reported for the shape lints, and whether every package was read.

    The same function is reported once for each target that compiles it, so a
    finding is kept once per place. A numbered lint without its numbers, or
    with a threshold other than the one asked, means clippy did not measure
    what this script asked, and the run is not used.
    """
    by_lint = {axis.lint: axis for axis in AXES}
    found = {}
    seen = set()
    errors = []
    for raw in json_lines:
        if not raw.startswith("{"):
            continue
        message = _json(raw)
        reason = message.get("reason")
        if reason == "compiler-artifact":
            seen.add(_member_of(message.get("manifest_path", ""), members))
            continue
        if reason != "compiler-message":
            continue
        body = message.get("message", {})
        if body.get("level") == "error":
            errors.append(body.get("rendered") or body.get("message", ""))
        code = (body.get("code") or {}).get("code") or ""
        axis = by_lint.get(code.removeprefix("clippy::"))
        if axis is None or not code.startswith("clippy::"):
            continue
        finding = _finding(axis, body, thresholds[axis.key])
        found[(axis.lint, finding.path, finding.line)] = finding
    if errors:
        raise Unusable("the code did not compile:\n" + "\n".join(errors))
    missing = sorted(set(members) - seen)
    if missing:
        raise Unusable(f"clippy did not read {', '.join(missing)}, so a clean answer would mean nothing")
    return list(found.values())


def _json(raw):
    try:
        return json.loads(raw)
    except ValueError as error:
        raise Unusable(f"a line of cargo output is not JSON: {raw[:120]}") from error


def _member_of(manifest_path, members):
    path = manifest_path.replace("\\", "/")
    for member in members:
        if path.endswith(f"/{member}/Cargo.toml"):
            return member
    return None


def _finding(axis, body, threshold):
    spans = body.get("spans") or []
    span = next((s for s in spans if s.get("is_primary")), spans[0] if spans else {})
    path = span.get("file_name", "?").replace("\\", "/")
    text = " ".join(part.get("text", "") for part in span.get("text") or [])
    name = FUNCTION.search(text)
    value = None
    if axis.numbered:
        numbers = NUMBERS.search(body.get("message", ""))
        if not numbers:
            raise Unusable(
                f"clippy reported {axis.lint} without its numbers, so its wording changed: "
                f"{body.get('message', '')!r}"
            )
        value, asked = int(numbers.group(1)), int(numbers.group(2))
        if asked != threshold:
            raise Unusable(
                f"clippy measured {axis.lint} against {asked}, not the {threshold} this gate "
                "asked for, so another configuration was read"
            )
    return Finding(
        lint=axis.lint,
        path=path,
        line=int(span.get("line_start", 0)),
        value=value,
        name=name.group(1) if name else "",
    )


def judge_over(scope, findings):
    """Problems from a run at the ceilings themselves: anything reported is over one."""
    problems = []
    for finding in sorted(findings, key=lambda f: (f.lint, f.path, f.line)):
        amount = f" at {finding.value}" if finding.value is not None else ""
        problems.append(
            f"{scope}: {finding.lint} is over its ceiling{amount} in {finding.where()} - "
            "move code out, do not raise the number"
        )
    return problems


def judge_near(scope, ceilings, near, band, findings):
    """Problems from a run at the edge of each band: over, room, and the crowd."""
    problems = []
    for axis in AXES:
        ceiling = ceilings[axis.key]
        mine = [f for f in findings if f.lint == axis.lint]
        if axis.numbered:
            over = [f for f in mine if f.value > ceiling]
            problems += judge_over(scope, over)
            highest = max((f.value for f in mine), default=None)
            if not over and (highest is None or highest < ceiling):
                shown = "nothing reaches the edge of the band" if highest is None else f"the highest is {highest}"
                problems.append(
                    f"{scope}: the {axis.lint} ceiling {ceiling} has room in it, {shown} - "
                    "lower the ceiling to the measurement"
                )
            edge = near_edge(axis, ceiling, band)
            crowd = [f for f in mine if f.value >= edge]
        else:
            if not mine:
                problems.append(
                    f"{scope}: the {axis.lint} ceiling {ceiling} has room in it, no block reaches "
                    "it - lower the ceiling to the measurement"
                )
            crowd = mine
        frozen = near[axis.key]
        if len(crowd) > frozen:
            problems.append(
                f"{scope}: {len(crowd)} stand near the {axis.lint} ceiling, frozen at {frozen} - "
                "shrink one before another joins"
            )
        elif len(crowd) < frozen:
            problems.append(
                f"{scope}: only {len(crowd)} stand near the {axis.lint} ceiling, frozen at "
                f"{frozen} - lower the frozen count to the measurement"
            )
    return problems


def report(scope, ceilings, band, findings, out):
    """The numbers behind the verdict, with names, so the next to fall is visible."""
    print(f"{scope}:", file=out)
    for axis in AXES:
        ceiling = ceilings[axis.key]
        mine = [f for f in findings if f.lint == axis.lint]
        if axis.numbered:
            mine.sort(key=lambda f: (-f.value, f.path, f.line))
            edge = near_edge(axis, ceiling, band)
            print(f"  {axis.lint}: ceiling {ceiling}, near from {edge}, {len(mine)} near", file=out)
            for finding in mine:
                print(f"    {finding.value:5}  {finding.where()}", file=out)
        else:
            print(f"  {axis.lint}: ceiling {ceiling}, {len(mine)} block(s) at it", file=out)
            for finding in sorted(mine, key=lambda f: (f.path, f.line)):
                print(f"           {finding.where()}", file=out)


def run_clippy(root, scope, clippy_text, thresholds, members, jobs):
    """Runs clippy over one scope with the given thresholds and returns its findings."""
    with tempfile.TemporaryDirectory(prefix="nkb-shape-") as conf:
        with open(os.path.join(conf, "clippy.toml"), "w", encoding="utf-8", newline="\n") as handle:
            handle.write(with_thresholds(clippy_text, thresholds))
        command = ["cargo", "clippy", "--workspace", *SCOPES[scope], "--locked", "--message-format=json"]
        if jobs:
            command += ["--jobs", str(jobs)]
        command += ["--", "--cap-lints", "warn"]
        env = dict(os.environ, CLIPPY_CONF_DIR=conf)
        try:
            done = subprocess.run(
                command,
                cwd=root,
                env=env,
                capture_output=True,
                text=True,
                encoding="utf-8",
                errors="replace",
                check=False,
            )
        except OSError as error:
            raise Unusable(f"cargo could not be started: {error}") from error
    lines = done.stdout.splitlines()
    if done.returncode != 0:
        # A compile error is in the JSON, and reading it raises with its words.
        # Anything else that stopped cargo is in the last lines it wrote.
        findings_from(lines, thresholds, members)
        tail = "\n".join(done.stderr.splitlines()[-20:])
        raise Unusable(f"cargo clippy ended with {done.returncode}:\n{tail}")
    return findings_from(lines, thresholds, members)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "--pinned",
        action="store_true",
        help="also require every ceiling and count to be the measurement (Windows)",
    )
    parser.add_argument("--jobs", type=int, default=0, help="passed to cargo as --jobs")
    args = parser.parse_args(argv)

    try:
        settings = read_settings(ROOT)
        with open(os.path.join(ROOT, CLIPPY_CONFIG), encoding="utf-8") as handle:
            clippy_text = handle.read()
        problems = []
        product = settings.ceilings["product"]
        over = run_clippy(ROOT, "product", clippy_text, product, settings.members, args.jobs)
        problems += judge_over("product", over)
        if args.pinned:
            for scope in ("product", "all"):
                ceilings = settings.ceilings[scope]
                asked = {
                    axis.key: band_threshold(axis, ceilings[axis.key], settings.band) for axis in AXES
                }
                found = run_clippy(ROOT, scope, clippy_text, asked, settings.members, args.jobs)
                report(scope, ceilings, settings.band, found, sys.stdout)
                problems += judge_near(
                    scope, ceilings, settings.near[scope], settings.band, found
                )
    except Unusable as error:
        print(f"shape: NO VERDICT - {error}")
        return EXIT_UNUSABLE

    # A product function over its ceiling is seen by the run at the ceiling and
    # again by the run at the edge of the band. It is one problem.
    problems = list(dict.fromkeys(problems))
    for problem in problems:
        print(f"shape: {problem}")
    if problems:
        print(f"shape: BLOCKED - {len(problems)} problem(s) above")
        return EXIT_BLOCKS
    if args.pinned:
        print("shape: passes - nothing is over a ceiling, and every ceiling and count is the measurement")
    else:
        print(
            "shape: passes - nothing in the product is over its ceiling. The ceilings of all the "
            "code are clippy.toml, held by every clippy run with -D warnings"
        )
    return EXIT_PASSES


if __name__ == "__main__":
    sys.exit(main())
