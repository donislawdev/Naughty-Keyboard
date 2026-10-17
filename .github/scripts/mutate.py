#!/usr/bin/env python3
"""Breaks every guard on purpose, one entry at a time, to see that it notices.

    python3 .github/scripts/mutate.py check
    python3 .github/scripts/mutate.py run --only M199,M1020
    python3 .github/scripts/mutate.py run --base origin/main
    python3 .github/scripts/mutate.py run --shard 3/16 --json result-3.json
    python3 .github/scripts/mutate.py plan --base <commit> --output "$GITHUB_OUTPUT"
    python3 .github/scripts/mutate.py report --folder results --entries 890 --parts 16 --gate
    python3 .github/scripts/mutate.py restore

What it answers
---------------
A green test suite says the code does what the tests check. It does not say the
tests check anything. That takes each guard broken on purpose and watched going
red, and .github/mutation/entries.json holds one entry per break: the file, the
text to find in it, what to put in its place, and the one test that has to fail.

Named, not "anything red". An entry is caught only when the test it names fails.
A break that some other test happens to catch proves nothing about the guard the
entry is about.

What it refuses
---------------
A whole run on one machine. Every entry rebuilds a crate and its tests, so all of
them take many hours, and they run in CI instead: .github/workflows/mutation.yml
cuts them into sixteen parts once a week and runs the entries a pull request
reaches. Here a run needs --only, --base or --shard, and no flag means all.

The safeguards, each from a failure seen before it was written
--------------------------------------------------------------
- A file is read whole before it is changed, put back in a finally block, and the
  file put back is compared with the original by SHA-256, not assumed to match.
- Before the first change a rescue file is written outside the repository. A run
  killed with a file still broken leaves it behind, and the next run refuses to
  start until `restore` has put the file back.
- A break that does not compile proves nothing about the guard. It is BROKEN,
  never caught.
- Every named test runs once on the untouched tree first. A test that is already
  red, that no longer exists or that is skipped here cannot catch anything, and
  its entries say so instead of being judged.
- Canaries go first in every run: breaks whose verdict is known in advance, one
  caught, one missed and one broken for each kind of test. A run whose canaries
  come out otherwise is not believed.
- The broken product is built in target/mutation and never in target/debug, so a
  program built from a break is never left where other tools pick it up.
- Colours are switched off and stripped. On another project the colour codes of a
  test runner turned two hundred verdicts into errors.

How a change is tied to the entries it reaches (--base)
------------------------------------------------------
An entry is run when the change touches the file it breaks, the file its test is
in, or the entry itself. For a Rust test inside the library or a program, the
file is any changed file of that crate's src/ that defines the test's function.
For an integration test it is tests/<name>.rs, and a changed helper in a folder
under tests/ reaches every integration test of the crate. A change to this
script or to the workflow, or a base without the registry, reaches every entry.
A guard that stopped reaching its code through a change in some third file is
what the weekly run of everything is for.

Exit codes: 0 everything asked for was caught (or the check passed), 1 something
was not, 2 the run refused to start.
"""

import argparse
import collections
import fnmatch
import hashlib
import json
import math
import os
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import unittest

SCRIPTS = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(SCRIPTS))
REGISTRY = ".github/mutation/entries.json"
SCHEMA = 1

# A change to either of these changes what a verdict means, so it reaches every
# entry.
EVERYTHING = (".github/scripts/mutate.py", ".github/workflows/mutation.yml")

# A pull request gets one part for every twenty entries, and a run never gets more
# than sixteen parts. A free plan runs twenty jobs at once for the whole account,
# and the other projects of the account run theirs on other days.
ENTRIES_PER_PART = 20
MAX_PARTS = 16

TIMEOUT_MINUTES = 30

CAUGHT = "caught"
MISSED = "MISSED"
WRONG_TEST = "WRONG TEST"
BROKEN = "BROKEN"
STALE = "STALE"
NO_TEST = "NO TEST"
BASELINE_RED = "BASELINE RED"
TIMEOUT = "TIMEOUT"
VERDICTS = (CAUGHT, MISSED, WRONG_TEST, BROKEN, STALE, NO_TEST, BASELINE_RED, TIMEOUT)

ENTRY_KEYS = frozenset({"id", "why", "file", "find", "replace", "append", "test", "runner"})
CANARY_KEYS = ENTRY_KEYS | {"expect"}
REQUIRED_KEYS = ("id", "why", "file", "test", "runner")
EXPECTABLE = (CAUGHT, MISSED, BROKEN)
ENTRY_ID = re.compile(r"^M[1-9][0-9]*$")
CANARY_ID = re.compile(r"^K[1-9][0-9]*$")

RUST_PACKAGE = re.compile(r"^nkb-[a-z]+$")
RUST_TARGET = re.compile(r"^(lib|bin=[a-z0-9_-]+|[a-z0-9_]+)$")
PYTHON_FOLDER = re.compile(r"^[A-Za-z0-9_.][A-Za-z0-9_./-]*$")
PYTHON_MODULE = re.compile(r"^test_[a-z0-9_]+$")

# The exceptions that say the code under test did not load or did not parse, so
# the test never got as far as checking anything.
NOT_LOADED = ("SyntaxError", "IndentationError", "ImportError", "ModuleNotFoundError", "NameError")

DRIVER_MARK = "MUTATE-UNITTEST "
COLOURS = re.compile(r"\x1b\[[0-9;?]*[ -/]*[@-~]")
LIBTEST_RUNNING = re.compile(r"^running \d+ tests?$", re.M)
LIBTEST_LINE = re.compile(r"^test (\S+) \.\.\. (ok|FAILED|ignored)\b", re.M)


class Refused(Exception):
    """A run that must not start, with the sentence that says why."""


class RestoreFailed(Exception):
    """A file that did not come back as it was. Nothing may run after this."""


Runner = collections.namedtuple("Runner", "kind where target")
Outcome = collections.namedtuple("Outcome", "code text seconds timed_out")
Verdict = collections.namedtuple("Verdict", "verdict detail")


# ---- the registry ---------------------------------------------------------------


def parse_runner(text):
    """rust:<crate>:<lib | bin=<program> | integration test> or
    pyunit:<folder>:<test module>. Raises ValueError with the reason."""
    parts = text.split(":")
    if len(parts) != 3:
        raise ValueError("a runner has three parts separated by colons")
    kind, where, target = parts
    if kind == "rust":
        if not RUST_PACKAGE.match(where):
            raise ValueError("%r is not a crate of this workspace" % where)
        if not RUST_TARGET.match(target):
            raise ValueError("%r is not lib, bin=<program> or an integration test" % target)
    elif kind == "pyunit":
        if not PYTHON_FOLDER.match(where) or ".." in where.split("/"):
            raise ValueError("%r is not a folder inside the repository" % where)
        if not PYTHON_MODULE.match(target):
            raise ValueError("%r is not a test module" % target)
    else:
        raise ValueError("%r is not a kind of runner this script knows" % kind)
    return Runner(kind, where, target)


def shape_problems(data):
    """Every problem with the registry that its text alone shows."""
    if not isinstance(data, dict) or set(data) != {"schema", "canaries", "entries"}:
        return ["the registry is an object with exactly schema, canaries and entries"]
    if data["schema"] != SCHEMA:
        return ["the registry says schema %r and this script reads %d" % (data["schema"], SCHEMA)]
    problems = []
    seen = collections.Counter()
    for group, allowed, pattern in (
        ("canaries", CANARY_KEYS, CANARY_ID),
        ("entries", ENTRY_KEYS, ENTRY_ID),
    ):
        if not isinstance(data[group], list):
            problems.append("%s is not a list" % group)
            continue
        for index, item in enumerate(data[group]):
            name = item.get("id", "%s[%d]" % (group, index)) if isinstance(item, dict) else index
            problems.extend("%s: %s" % (name, p) for p in item_problems(item, allowed, pattern))
            if isinstance(item, dict) and isinstance(item.get("id"), str):
                seen[item["id"]] += 1
    for identifier, count in sorted(seen.items()):
        if count > 1:
            problems.append("%s: the id is used %d times, and an id is an address" % (identifier, count))
    return problems


def item_problems(item, allowed, pattern):
    if not isinstance(item, dict):
        return ["not an object"]
    problems = []
    unknown = sorted(set(item) - allowed)
    if unknown:
        problems.append("keys this script does not know: %s" % ", ".join(unknown))
    for key in REQUIRED_KEYS + (("expect",) if "expect" in allowed else ()):
        if not isinstance(item.get(key), str) or not item.get(key):
            problems.append("%s is missing or empty" % key)
    if problems:
        return problems
    if not pattern.match(item["id"]):
        problems.append("the id does not have the form %s" % pattern.pattern)
    replaces = "find" in item or "replace" in item
    if replaces == ("append" in item):
        problems.append("an entry either has find and replace, or append, and not both")
    elif replaces:
        if not isinstance(item.get("find"), str) or not item["find"]:
            problems.append("find is missing or empty")
        if not isinstance(item.get("replace"), str):
            problems.append("replace is missing")
        if item.get("find") == item.get("replace"):
            problems.append("replace is the same text as find, so nothing would break")
    elif not isinstance(item["append"], str) or not item["append"]:
        problems.append("append is empty")
    path = item["file"]
    if path.startswith("/") or "\\" in path or ".." in path.split("/"):
        problems.append("file is not a relative path with forward slashes")
    if "expect" in item and item["expect"] not in EXPECTABLE:
        problems.append("expect is one of %s" % ", ".join(EXPECTABLE))
    try:
        parse_runner(item["runner"])
    except ValueError as error:
        problems.append("runner: %s" % error)
    return problems


def parse_registry(raw):
    """The registry from its bytes, or the problems that stop it being read."""
    if any(byte > 127 for byte in raw):
        return None, ["the registry carries a byte outside ASCII, so write it with ensure_ascii"]
    try:
        data = json.loads(raw.decode("ascii"))
    except ValueError as error:
        return None, ["the registry is not JSON: %s" % error]
    problems = shape_problems(data)
    return (None, problems) if problems else (data, [])


def read_registry(root):
    with open(os.path.join(root, REGISTRY), "rb") as handle:
        data, problems = parse_registry(handle.read())
    if problems:
        raise Refused("the registry cannot be used:\n  " + "\n  ".join(problems))
    return data


RUST_FUNCTION = re.compile(r"\bfn\s+([A-Za-z_][A-Za-z0-9_]*)")
PYTHON_NAME = re.compile(r"^\s*(?:def|class)\s+(\w+)", re.M)


class Tree:
    """The files of one checkout, each read once, and the names of the functions
    each folder of Rust defines, gathered once."""

    def __init__(self, root):
        self.root = root
        self.texts = {}
        self.indexes = {}

    def path(self, relative):
        return os.path.join(self.root, *relative.split("/"))

    def text(self, relative):
        if relative not in self.texts:
            try:
                with open(self.path(relative), "rb") as handle:
                    self.texts[relative] = handle.read().decode("utf-8")
            except (OSError, UnicodeDecodeError):
                self.texts[relative] = None
        return self.texts[relative]

    def functions(self, folder):
        """Every function name defined in a .rs file under the folder."""
        if folder not in self.indexes:
            names = set()
            for directory, _, files in os.walk(self.path(folder)):
                for name in files:
                    if name.endswith(".rs"):
                        relative = os.path.relpath(os.path.join(directory, name), self.root)
                        names.update(RUST_FUNCTION.findall(self.text(relative.replace(os.sep, "/")) or ""))
            self.indexes[folder] = names
        return self.indexes[folder]


def defines_function(text, name):
    return name in RUST_FUNCTION.findall(text or "")


def python_names(text):
    return PYTHON_NAME.findall(text or "")


def tree_problems(item, tree):
    """Whether the entry still fits the code: the text to find is there exactly
    once, and the test it names is still written somewhere it would be found."""
    problems = []
    text = tree.text(item["file"])
    if text is None:
        return ["%s does not exist or is not UTF-8" % item["file"]]
    if "find" in item:
        count = text.count(item["find"])
        if count != 1:
            problems.append("the text to find is in %s %d times, not once" % (item["file"], count))
    runner = parse_runner(item["runner"])
    if runner.kind == "pyunit":
        module = "%s/%s.py" % (runner.where, runner.target)
        if tree.text(module) is None:
            problems.append("the test module %s does not exist" % module)
        elif not any(item["test"] in name for name in python_names(tree.text(module))):
            problems.append("no test or class in %s is named like %r" % (module, item["test"]))
        return problems
    crate = "crates/%s" % runner.where
    if tree.text(crate + "/Cargo.toml") is None:
        return problems + ["%s is not a crate" % crate]
    if runner.target == "lib" or runner.target.startswith("bin="):
        scope = crate + "/src"
    else:
        if tree.text("%s/tests/%s.rs" % (crate, runner.target)) is None:
            return problems + ["%s/tests/%s.rs does not exist" % (crate, runner.target)]
        scope = crate + "/tests"
    leaf = item["test"].rsplit("::", 1)[-1]
    if leaf not in tree.functions(scope):
        problems.append("no function %s is defined in %s" % (leaf, scope))
    return problems


def check(root):
    """Every problem with the registry and with how it fits the code."""
    try:
        data = read_registry(root)
    except OSError as error:
        return ["the registry cannot be read: %s" % error]
    except Refused as refusal:
        return [str(refusal)]
    tree = Tree(root)
    problems = []
    for item in data["canaries"] + data["entries"]:
        problems.extend("%s: %s" % (item["id"], p) for p in tree_problems(item, tree))
    return problems


# ---- choosing and cutting -------------------------------------------------------


def id_number(identifier):
    return int(identifier[1:])


def place_of(entry):
    parts = entry["file"].split("/")
    return parts[1] if parts[0] == "crates" and len(parts) > 1 else parts[0]


def weight(entry):
    """A guess of the cost, used only to cut parts of about the same length. A
    Python test runs in a second, a Rust entry rebuilds a crate and links its
    tests, and the graphical crate is the slowest to build."""
    runner = parse_runner(entry["runner"])
    if runner.kind == "pyunit":
        return 1
    return 40 if runner.where == "nkb-gui" else 20


def ordered(entries):
    """Entries that break the same crate and run the same tests side by side, so
    that one rebuild is followed by the next one of the same crate and not by a
    rebuild of everything that depends on another."""
    return sorted(entries, key=lambda e: (place_of(e), e["runner"], id_number(e["id"])))


def cut(entries, parts):
    """Contiguous slices of the ordered entries, of about the same weight, and no
    slice empty while there are entries enough to go round."""
    order = ordered(entries)
    slices = [[] for _ in range(parts)]
    index = 0
    rest = sum(weight(e) for e in order)
    share = rest / parts if parts else 0
    held = 0
    for position, entry in enumerate(order):
        size = weight(entry)
        later_parts = parts - index - 1
        # The next part begins when this one has its share by the middle of the
        # entry, or when the entries left only just give every later part one.
        if slices[index] and later_parts and (
                held + size / 2 > share or len(order) - position <= later_parts):
            index += 1
            share = rest / (parts - index)
            held = 0
        slices[index].append(entry)
        held += size
        rest -= size
    return slices


def parts_for(entries, whole):
    """How many parts: sixteen for a run of everything, one for every twenty
    entries on a pull request, and never a part left with nothing in it."""
    if not entries:
        return 0
    wanted = min(MAX_PARTS, len(entries)) if whole else min(
        MAX_PARTS, math.ceil(len(entries) / ENTRIES_PER_PART))
    while wanted > 1 and not all(cut(entries, wanted)):
        wanted -= 1
    return wanted


def git(root, *arguments):
    result = subprocess.run(["git", *arguments], cwd=root, capture_output=True, check=False)
    if result.returncode != 0:
        raise Refused("git %s failed: %s" % (" ".join(arguments), result.stderr.decode("utf-8", "replace").strip()))
    return result.stdout.decode("utf-8", "replace")


def changed_paths(root, base):
    """Every path that differs between the base and the working tree, both sides
    of a rename included."""
    output = git(root, "diff", "--name-only", "--no-renames", base, "--")
    return sorted({line.strip() for line in output.splitlines() if line.strip()})


def base_registry(root, base):
    try:
        raw = git(root, "show", "%s:%s" % (base, REGISTRY)).encode("utf-8")
    except Refused:
        return None
    data, problems = parse_registry(raw)
    return None if problems else data


def touches_test(entry, changed, tree):
    runner = parse_runner(entry["runner"])
    if runner.kind == "pyunit":
        return "%s/%s.py" % (runner.where, runner.target) in changed
    crate = "crates/%s/" % runner.where
    if runner.target == "lib" or runner.target.startswith("bin="):
        leaf = entry["test"].rsplit("::", 1)[-1]
        return any(
            path.startswith(crate + "src/") and path.endswith(".rs")
            and defines_function(tree.text(path), leaf)
            for path in changed
        )
    tests = crate + "tests/"
    for path in changed:
        if path.startswith(tests):
            rest = path[len(tests):]
            if rest == runner.target + ".rs" or "/" in rest:
                return True
    return False


def reached(entries, base_entries, changed, tree):
    """The entries a change reaches, and the sentence that says why."""
    changed = set(changed)
    if base_entries is None:
        return list(entries), "the base has no registry this script can read, so every entry runs"
    touched = sorted(changed.intersection(EVERYTHING))
    if touched:
        return list(entries), "%s changed, so every entry runs" % ", ".join(touched)
    before = {e["id"]: e for e in base_entries}
    chosen = [
        e for e in entries
        if before.get(e["id"]) != e or e["file"] in changed or touches_test(e, changed, tree)
    ]
    return chosen, "%d of %d entries are reached by the change" % (len(chosen), len(entries))


def selection(root, data, base):
    if base is None:
        return list(data["entries"]), "every entry runs"
    previous = base_registry(root, base)
    return reached(data["entries"], previous and previous["entries"], changed_paths(root, base), Tree(root))


# ---- running a test -------------------------------------------------------------


def stop_tree(process):
    """Ends the command and everything it started. A test program left running
    would hold its files, and the next build could not replace them."""
    if os.name == "nt":
        subprocess.run(["taskkill", "/F", "/T", "/PID", str(process.pid)],
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False)
    else:
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass


def execute(command, root, timeout):
    environment = dict(os.environ)
    environment.update(CARGO_TERM_COLOR="never", NO_COLOR="1",
                       PYTHONIOENCODING="utf-8", PYTHONDONTWRITEBYTECODE="1")
    options = {}
    if os.name == "nt":
        options["creationflags"] = subprocess.CREATE_NEW_PROCESS_GROUP
    else:
        options["start_new_session"] = True
    started = time.monotonic()
    process = subprocess.Popen(command, cwd=root, env=environment, stdin=subprocess.DEVNULL,
                               stdout=subprocess.PIPE, stderr=subprocess.STDOUT, **options)
    try:
        output, _ = process.communicate(timeout=timeout)
        timed_out = False
    except subprocess.TimeoutExpired:
        stop_tree(process)
        output, _ = process.communicate()
        timed_out = True
    except BaseException:
        # Ctrl+C reaches this script and not the command, which runs in a group
        # of its own. Without this it would go on building after the file was
        # put back.
        stop_tree(process)
        raise
    text = COLOURS.sub("", output.decode("utf-8", "replace")).replace("\r\n", "\n")
    return Outcome(process.returncode, text, time.monotonic() - started, timed_out)


def rust_command(runner, names, target_dir):
    if runner.target == "lib":
        scope = ["--lib"]
    elif runner.target.startswith("bin="):
        scope = ["--bin", runner.target[len("bin="):]]
    else:
        scope = ["--test", runner.target]
    return ["cargo", "test", "-p", runner.where, "--locked", "--target-dir", target_dir,
            *scope, "--", "--exact", "--color", "never", *names]


# The copy of this script that runs the Python tests. A run points it at a copy
# made before the first break, because some entries break this very file, and a
# broken driver would judge its own break.
DRIVER = os.path.abspath(__file__)


def pyunit_command(runner, names):
    return [sys.executable, DRIVER, "unittest-driver", runner.where, runner.target, *names]


def command_for(runner, names, target_dir):
    if runner.kind == "rust":
        return rust_command(runner, names, target_dir)
    return pyunit_command(runner, names)


def first_line(text, prefix):
    for line in text.splitlines():
        if line.strip().startswith(prefix):
            return line.strip()[:200]
    lines = [line.strip() for line in text.splitlines() if line.strip()]
    return lines[-1][:200] if lines else "no output"


def libtest_results(text):
    return dict(LIBTEST_LINE.findall(text))


def judge_rust(outcome, name, minutes):
    if outcome.timed_out:
        return Verdict(TIMEOUT, "stopped after %d minutes" % minutes)
    if not LIBTEST_RUNNING.search(outcome.text):
        if outcome.code != 0:
            return Verdict(BROKEN, first_line(outcome.text, "error"))
        return Verdict(NO_TEST, "no test program ran")
    status = libtest_results(outcome.text).get(name)
    if status == "FAILED":
        return Verdict(CAUGHT, "")
    if status == "ignored":
        return Verdict(NO_TEST, "the test is ignored")
    if status == "ok":
        if outcome.code == 0:
            return Verdict(MISSED, "")
        return Verdict(WRONG_TEST, "the test passed and something else went red")
    if outcome.code != 0:
        return Verdict(WRONG_TEST, "red, and no line names the test: %s" % first_line(outcome.text, "error"))
    return Verdict(NO_TEST, "no test of that name ran")


def driver_report(text):
    for line in reversed(text.splitlines()):
        if line.startswith(DRIVER_MARK):
            try:
                return json.loads(line[len(DRIVER_MARK):])
            except ValueError:
                return None
    return None


def pattern_of(name):
    return name if any(c in name for c in "*?[") else "*%s*" % name


def matching(report, name):
    """The tests the entry names. `-k` picks every test whose id contains the
    name, so test_fails would bring test_a_subtest_fails along. The test of
    exactly that name decides when there is one, and the wider match only when
    the entry names a class or a part of a name."""
    exact = {k: v for k, v in report["tests"].items() if k.rsplit(".", 1)[-1] == name}
    if exact:
        return exact
    pattern = pattern_of(name)
    return {k: v for k, v in report["tests"].items() if fnmatch.fnmatchcase(k, pattern)}


def judge_pyunit(outcome, name, minutes):
    if outcome.timed_out:
        return Verdict(TIMEOUT, "stopped after %d minutes" % minutes)
    report = driver_report(outcome.text)
    if report is None:
        return Verdict(BROKEN, "the test driver did not report: %s" % first_line(outcome.text, "Error"))
    if report["not_loaded"]:
        return Verdict(BROKEN, report["not_loaded"][0])
    found = matching(report, name)
    if not found:
        if report["setup_errors"]:
            return Verdict(WRONG_TEST, "a set-up failed before the test: %s" % report["setup_errors"][0])
        return Verdict(NO_TEST, "no test of that name ran")
    stopped = [kind for status, kind in found.values() if status == "error" and kind in NOT_LOADED]
    if stopped:
        return Verdict(BROKEN, "the test stopped on %s" % stopped[0])
    if any(status in ("fail", "error") for status, _ in found.values()):
        return Verdict(CAUGHT, "")
    if all(status == "skipped" for status, _ in found.values()):
        return Verdict(NO_TEST, "skipped here: %s" % next(iter(found.values()))[1])
    return Verdict(MISSED, "")


def judge(runner, outcome, name, minutes):
    if runner.kind == "rust":
        return judge_rust(outcome, name, minutes)
    return judge_pyunit(outcome, name, minutes)


class Recording(unittest.TestResult):
    """What each test came to, by its id, with the exception's name."""

    def __init__(self):
        super().__init__()
        self.buffer = True
        self.outcomes = {}
        self.setup_errors = []

    def addSuccess(self, test):
        super().addSuccess(test)
        self.outcomes.setdefault(test.id(), ["ok", ""])

    def addFailure(self, test, err):
        super().addFailure(test, err)
        self.outcomes[test.id()] = ["fail", err[0].__name__]

    def addError(self, test, err):
        super().addError(test, err)
        if isinstance(test, unittest.TestCase):
            self.outcomes[test.id()] = ["error", err[0].__name__]
        else:
            self.setup_errors.append("%s: %s" % (test.id(), err[0].__name__))

    def addSkip(self, test, reason):
        super().addSkip(test, reason)
        self.outcomes[test.id()] = ["skipped", reason]

    def addExpectedFailure(self, test, err):
        super().addExpectedFailure(test, err)
        self.outcomes[test.id()] = ["ok", ""]

    def addUnexpectedSuccess(self, test):
        super().addUnexpectedSuccess(test)
        self.outcomes[test.id()] = ["fail", "UnexpectedSuccess"]

    def addSubTest(self, test, subtest, err):
        super().addSubTest(test, subtest, err)
        if err is not None:
            failed = issubclass(err[0], test.failureException)
            self.outcomes[test.id()] = ["fail" if failed else "error", err[0].__name__]


def unittest_driver(folder, module, patterns):
    """Runs the tests of one module that match the patterns, the way
    `python -m unittest -k` picks them, and prints one line with what each came
    to. Read by judge_pyunit, never by a person."""
    loader = unittest.TestLoader()
    loader.testNamePatterns = [pattern_of(p) for p in patterns]
    # Relative to the folder the run started in, which is the root of the tree
    # under test.
    start = os.path.abspath(os.path.join(*folder.split("/")))
    suite = loader.discover(start, pattern=module + ".py", top_level_dir=start)
    result = Recording()
    suite.run(result)
    not_loaded = [error.strip().splitlines()[-1][:200] for error in loader.errors]
    print(DRIVER_MARK + json.dumps({
        "tests": result.outcomes,
        "not_loaded": not_loaded,
        "setup_errors": result.setup_errors,
    }))
    return 0


# ---- breaking and putting back --------------------------------------------------


def rescue_path(root):
    tag = hashlib.sha256(os.path.abspath(root).encode("utf-8")).hexdigest()[:12]
    return os.path.join(tempfile.gettempdir(), "nkb-mutation-rescue-%s.json" % tag)


def refuse_after_a_killed_run(root):
    path = rescue_path(root)
    if os.path.exists(path):
        with open(path, "rb") as handle:
            files = sorted(json.loads(handle.read().decode("utf-8")))
        raise Refused(
            "a run was stopped with a file still broken, and %s says which: %s. "
            "Put it back first: python3 .github/scripts/mutate.py restore"
            % (path, ", ".join(files)))


def restore(root):
    path = rescue_path(root)
    if not os.path.exists(path):
        print("nothing to put back: there is no rescue file at %s" % path)
        return 0
    with open(path, "rb") as handle:
        files = json.loads(handle.read().decode("utf-8"))
    tree = Tree(root)
    for relative, content in sorted(files.items()):
        write_bytes(tree.path(relative), bytes.fromhex(content))
        print("put back %s" % relative)
    os.remove(path)
    print("rescue file removed")
    return 0


def digest(data):
    return hashlib.sha256(data).hexdigest()


def read_bytes(path):
    with open(path, "rb") as handle:
        return handle.read()


def write_bytes(path, data):
    # Bytes, never text mode: text mode on Windows writes CRLF.
    with open(path, "wb") as handle:
        handle.write(data)


def write_json(path, data):
    temporary = path + ".partial"
    with open(temporary, "wb") as handle:
        handle.write(json.dumps(data, indent=1, sort_keys=True).encode("utf-8"))
    os.replace(temporary, path)


def broken_text(text, entry):
    if "append" in entry:
        return text + entry["append"]
    if text.count(entry["find"]) != 1:
        return None
    return text.replace(entry["find"], entry["replace"], 1)


def attempt(entry, root, run_test):
    """Breaks one file, runs the entry's test, puts the file back and proves it.
    `run_test` gets the entry and returns its Verdict."""
    tree = Tree(root)
    path = tree.path(entry["file"])
    original = read_bytes(path)
    broken = broken_text(original.decode("utf-8"), entry)
    if broken is None:
        return Verdict(STALE, "the text to find is not in %s exactly once" % entry["file"])
    rescue = rescue_path(root)
    write_bytes(rescue, json.dumps({entry["file"]: original.hex()}).encode("utf-8"))
    try:
        write_bytes(path, broken.encode("utf-8"))
        verdict = run_test(entry)
    finally:
        write_bytes(path, original)
        if digest(read_bytes(path)) != digest(original):
            raise RestoreFailed("%s did not come back as it was, and %s still holds it" % (entry["file"], rescue))
        os.remove(rescue)
    return verdict


# ---- a run ----------------------------------------------------------------------


def baseline(entries, root, target_dir, minutes, execute_command=execute):
    """Runs every named test once on the untouched tree, a command per runner.
    Returns {(runner, test): Verdict} for the tests that cannot judge anything."""
    names = collections.OrderedDict()
    for entry in entries:
        names.setdefault(entry["runner"], [])
        if entry["test"] not in names[entry["runner"]]:
            names[entry["runner"]].append(entry["test"])
    unfit = {}
    for text, tests in names.items():
        runner = parse_runner(text)
        outcome = execute_command(command_for(runner, tests, target_dir), root, minutes * 60)
        for name in tests:
            verdict = untouched(runner, outcome, name, minutes)
            if verdict is not None:
                unfit[(text, name)] = verdict
    return unfit


def untouched(runner, outcome, name, minutes):
    """None when the test passed on the untouched tree, else why it cannot judge.
    One test of a group failing says nothing about the others in it."""
    if outcome.timed_out:
        return Verdict(BASELINE_RED, "the untouched tree ran past %d minutes" % minutes)
    if runner.kind == "rust":
        if not LIBTEST_RUNNING.search(outcome.text):
            return Verdict(BASELINE_RED, "the untouched tree does not build: %s"
                           % first_line(outcome.text, "error"))
        status = libtest_results(outcome.text).get(name)
        if status == "ok":
            return None
        if status == "FAILED":
            return Verdict(BASELINE_RED, "the test fails on the untouched tree")
        if status == "ignored":
            return Verdict(NO_TEST, "the test is ignored")
        return Verdict(NO_TEST, "no test of that name ran")
    report = driver_report(outcome.text)
    if report is None or report["not_loaded"]:
        return Verdict(BASELINE_RED, "the tests do not load on the untouched tree: %s" % (
            report["not_loaded"][0] if report else first_line(outcome.text, "Error")))
    found = matching(report, name)
    if not found:
        return Verdict(NO_TEST, "no test of that name ran")
    if any(status in ("fail", "error") for status, _ in found.values()):
        return Verdict(BASELINE_RED, "the test fails on the untouched tree")
    if all(status == "skipped" for status, _ in found.values()):
        return Verdict(NO_TEST, "skipped here: %s" % next(iter(found.values()))[1])
    return None


def run_entries(data, entries, root, record, target_dir, minutes, save, say,
                execute_command=execute):
    """Canaries, then the baseline, then every entry. Writes `record` through
    `save` after each step. Returns the exit code."""

    def run_test(entry):
        runner = parse_runner(entry["runner"])
        outcome = execute_command(command_for(runner, [entry["test"]], target_dir), root, minutes * 60)
        return judge(runner, outcome, entry["test"], minutes), outcome.seconds

    def timed(entry):
        holder = {}

        def run_and_keep(e):
            verdict, seconds = run_test(e)
            holder["seconds"] = seconds
            return verdict

        verdict = attempt(entry, root, run_and_keep)
        return verdict, round(holder.get("seconds", 0.0), 1)

    say("canaries - breaks whose verdict is known before they run:")
    for canary in data["canaries"]:
        verdict, seconds = timed(canary)
        record["canaries"].append({"id": canary["id"], "expected": canary["expect"],
                                   "verdict": verdict.verdict, "detail": verdict.detail,
                                   "seconds": seconds})
        mark = "as expected" if verdict.verdict == canary["expect"] else "EXPECTED %s" % canary["expect"]
        say("  %-12s %-6s %s  [%s]" % (verdict.verdict, canary["id"], canary["why"], mark))
        save()
    record["trusted"] = all(c["verdict"] == c["expected"] for c in record["canaries"])
    if not record["trusted"]:
        record["finished"] = True
        save()
        say("a canary came out otherwise, so no verdict of this run would mean anything")
        return 1

    fit = [e for e in entries if broken_text(Tree(root).text(e["file"]) or "", e) is not None]
    say("baseline - every named test once on the untouched tree (%d entries)" % len(fit))
    unfit = baseline(fit, root, target_dir, minutes, execute_command)
    save()

    say("entries:")
    for entry in entries:
        if entry not in fit:
            verdict, seconds = Verdict(STALE, "the text to find is not in %s exactly once" % entry["file"]), 0.0
        elif (entry["runner"], entry["test"]) in unfit:
            verdict, seconds = unfit[(entry["runner"], entry["test"])], 0.0
        else:
            verdict, seconds = timed(entry)
        record["entries"].append({"id": entry["id"], "verdict": verdict.verdict,
                                  "detail": verdict.detail, "seconds": seconds})
        say("  %-12s %-6s %s%s" % (verdict.verdict, entry["id"], entry["why"],
                                   "  <- " + verdict.detail if verdict.detail else ""))
        save()
    record["finished"] = True
    save()
    caught = sum(1 for e in record["entries"] if e["verdict"] == CAUGHT)
    say("caught %d of %d" % (caught, len(entries)))
    return 0 if caught == len(entries) else 1


def parse_shard(text):
    match = re.match(r"^(\d+)/(\d+)$", text or "")
    if not match or int(match.group(2)) < 1 or int(match.group(1)) >= int(match.group(2)):
        raise Refused("--shard is <part>/<parts>, counted from 0, such as 3/16")
    return int(match.group(1)), int(match.group(2))


def chosen_entries(root, data, only, base, shard):
    if not (only or base or shard):
        raise Refused(
            "a run needs --only, --base or --shard. Every entry at once is many hours "
            "and runs in CI, cut into parts (.github/workflows/mutation.yml)")
    if only and (base or shard):
        raise Refused("--only names the entries itself, so it goes without --base and --shard")
    if only:
        wanted = [x.strip() for x in only.split(",") if x.strip()]
        known = {e["id"]: e for e in data["entries"]}
        unknown = [x for x in wanted if x not in known]
        if unknown:
            raise Refused("no entry has the id %s" % ", ".join(unknown))
        return [known[x] for x in dict.fromkeys(wanted)], "%d entries named with --only" % len(wanted), (0, 1)
    entries, reason = selection(root, data, base)
    if not shard:
        return ordered(entries), reason, (0, 1)
    part, parts = parse_shard(shard)
    return cut(entries, parts)[part] if entries else [], reason, (part, parts)


def run(arguments):
    global DRIVER
    root = arguments.root
    refuse_after_a_killed_run(root)
    data = read_registry(root)
    entries, reason, (part, parts) = chosen_entries(root, data, arguments.only, arguments.base, arguments.shard)
    snapshot = tempfile.mkdtemp(prefix="nkb-mutation-driver-")
    DRIVER = os.path.join(snapshot, "driver.py")
    write_bytes(DRIVER, read_bytes(os.path.abspath(__file__)))
    try:
        return run_chosen(arguments, data, entries, reason, part, parts)
    finally:
        shutil.rmtree(snapshot, ignore_errors=True)
        DRIVER = os.path.abspath(__file__)


def run_chosen(arguments, data, entries, reason, part, parts):
    root = arguments.root
    target_dir = os.path.abspath(arguments.target_dir or os.path.join(root, "target", "mutation"))
    record = {"schema": SCHEMA, "part": part, "parts": parts, "selected": len(entries),
              "reason": reason, "canaries": [], "entries": [], "trusted": None,
              "finished": False, "started": time.time()}

    def save():
        record["seconds"] = round(time.time() - record["started"], 1)
        if arguments.json:
            write_json(arguments.json, record)

    def say(line):
        print(line, flush=True)

    say("part %d of %d: %d entries (%s)" % (part, parts, len(entries), reason))
    save()
    return run_entries(data, entries, root, record, target_dir, arguments.timeout_minutes, save, say)


# ---- plan and report ------------------------------------------------------------


def plan(arguments):
    data = read_registry(arguments.root)
    entries, reason = selection(arguments.root, data, arguments.base)
    parts = parts_for(entries, arguments.base is None)
    lines = ["entries=%d" % len(entries), "parts=%d" % parts,
             "shards=%s" % json.dumps(list(range(parts)))]
    if arguments.output:
        with open(arguments.output, "a", encoding="utf-8") as handle:
            handle.write("\n".join(lines) + "\n")
    print("\n".join(lines))
    sentence = ("%s. %d entries in %d parts." % (reason[0].upper() + reason[1:], len(entries), parts)
                if entries else "No mutation entry is reached by this change, so none runs.")
    print(sentence)
    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary:
        with open(summary, "a", encoding="utf-8") as handle:
            handle.write(sentence + "\n")
    return 0


def escape_data(text):
    return text.replace("%", "%25").replace("\r", "%0D").replace("\n", "%0A")


def escape_property(text):
    return escape_data(text).replace(":", "%3A").replace(",", "%2C")


def anchor_line(tree, entry):
    text = tree.text(entry["file"]) or ""
    if "find" in entry and entry["find"] in text:
        return text[:text.index(entry["find"])].count("\n") + 1
    return max(1, text.count("\n"))


def gather(folder, parts):
    """The result of every part, and the problems of the parts themselves."""
    results, problems = [], []
    for part in range(parts):
        path = os.path.join(folder, "result-%d.json" % part)
        try:
            with open(path, "rb") as handle:
                result = json.loads(handle.read().decode("utf-8"))
        except (OSError, ValueError):
            problems.append("part %d did not report" % part)
            continue
        if not isinstance(result, dict) or result.get("part") != part or result.get("parts") != parts:
            problems.append("part %d left a result of the wrong shape" % part)
            continue
        if result.get("trusted") is False:
            problems.append("part %d: a canary came out otherwise, so its verdicts mean nothing" % part)
        elif not result.get("finished"):
            problems.append("part %d stopped before its last entry (%d of %d judged)" % (
                part, len(result.get("entries", [])), result.get("selected", 0)))
        results.append(result)
    return results, problems


def report(arguments):
    results, problems = gather(arguments.folder, arguments.parts)
    data = read_registry(arguments.root)
    known = {e["id"]: e for e in data["entries"]}
    judged = [e for r in results for e in r.get("entries", [])]
    selected = sum(r.get("selected", 0) for r in results)
    if not problems and selected != arguments.entries:
        problems.append("the plan chose %d entries and the parts took %d" % (arguments.entries, selected))
    counts = collections.Counter(e["verdict"] for e in judged)
    failing = [e for e in judged if e["verdict"] != CAUGHT]
    level = "error" if arguments.gate else "warning"
    tree = Tree(arguments.root)
    for problem in problems:
        print("::%s title=mutation run::%s" % (level, escape_data(problem)))
    for result in failing:
        entry = known.get(result["id"])
        if entry is None:
            print("::%s title=%s::%s" % (level, escape_property(result["id"]), "not in the registry"))
            continue
        print("::%s file=%s,line=%d,title=%s::%s" % (
            level, escape_property(entry["file"]), anchor_line(tree, entry),
            escape_property("%s %s" % (entry["id"], result["verdict"])),
            escape_data(entry["why"] + (" - " + result["detail"] if result["detail"] else ""))))
    lines = ["## Mutation testing", ""]
    lines.append("%d of %d entries caught, in %d of %d parts." % (
        counts[CAUGHT], arguments.entries, len(results), arguments.parts))
    lines.append("")
    lines.append("| verdict | entries |")
    lines.append("|---|---|")
    lines.extend("| %s | %d |" % (v, counts[v]) for v in VERDICTS if counts[v])
    if problems:
        lines += ["", "Problems with the run itself:", ""] + ["- %s" % p for p in problems]
    if failing:
        lines += ["", "| entry | verdict | what it breaks | file | detail |", "|---|---|---|---|---|"]
        for result in sorted(failing, key=lambda e: id_number(e["id"])):
            entry = known.get(result["id"], {"why": "", "file": ""})
            lines.append("| %s | %s | %s | `%s` | %s |" % (
                result["id"], result["verdict"], entry["why"].replace("|", "/"),
                entry["file"], result["detail"].replace("|", "/")))
    slowest = sorted(judged, key=lambda e: -e.get("seconds", 0))[:5]
    if slowest:
        lines += ["", "Slowest entries: " + ", ".join(
            "%s %ds" % (e["id"], e.get("seconds", 0)) for e in slowest)]
    durations = ["part %d %dmin" % (r["part"], r.get("seconds", 0) // 60) for r in results]
    if durations:
        lines += ["", "Time per part: " + ", ".join(durations)]
    text = "\n".join(lines) + "\n"
    print(text)
    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary:
        with open(summary, "a", encoding="utf-8") as handle:
            handle.write(text)
    bad = bool(problems or failing)
    return 1 if arguments.gate and bad else 0


# ---- the command line -----------------------------------------------------------


def arguments_of(argv):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--root", default=ROOT, help=argparse.SUPPRESS)
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("check", help="does every entry still fit the code")
    commands.add_parser("restore", help="put back the file a killed run left broken")
    planning = commands.add_parser("plan", help="how many entries and parts, for the workflow")
    planning.add_argument("--base", help="the commit a pull request is compared with")
    planning.add_argument("--output", help="a file to append name=value lines to")
    running = commands.add_parser("run", help="break the chosen entries one at a time")
    running.add_argument("--only", help="entry ids separated by commas")
    running.add_argument("--base", help="the entries a change since this commit reaches")
    running.add_argument("--shard", help="<part>/<parts>, counted from 0")
    running.add_argument("--json", help="a file the verdicts go to after every entry")
    running.add_argument("--timeout-minutes", type=int, default=TIMEOUT_MINUTES)
    running.add_argument("--target-dir", help="where the broken product is built")
    reporting = commands.add_parser("report", help="put the parts of a run together")
    reporting.add_argument("--folder", required=True)
    reporting.add_argument("--entries", type=int, required=True)
    reporting.add_argument("--parts", type=int, required=True)
    reporting.add_argument("--gate", action="store_true", help="exit 1 on anything not caught")
    driver = commands.add_parser("unittest-driver", help="used by run, not by a person")
    driver.add_argument("folder")
    driver.add_argument("module")
    driver.add_argument("patterns", nargs="+")
    return parser.parse_args(argv)


def main(argv=None):
    arguments = arguments_of(argv)
    try:
        if arguments.command == "unittest-driver":
            return unittest_driver(arguments.folder, arguments.module, arguments.patterns)
        if arguments.command == "check":
            problems = check(arguments.root)
            for problem in problems:
                print(problem)
            print("the registry does not fit the code: %d problems" % len(problems)
                  if problems else "every entry fits the code")
            return 1 if problems else 0
        if arguments.command == "restore":
            return restore(arguments.root)
        if arguments.command == "plan":
            return plan(arguments)
        if arguments.command == "report":
            return report(arguments)
        return run(arguments)
    except Refused as refusal:
        print("refused: %s" % refusal)
        return 2
    except RestoreFailed as failure:
        print("STOPPED: %s" % failure)
        return 2


if __name__ == "__main__":
    sys.exit(main())
