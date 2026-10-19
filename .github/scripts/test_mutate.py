"""Tests for mutate.py, the mutation runner, and the check that its registry still
fits the code. They need the standard library and git.

    python3 -m unittest discover -s .github/scripts -p "test_*.py"

The first class is the one a pull request cannot get past. An entry whose text to
find has moved, or whose test was renamed, turns the semgrep job red in the same
pull request that moved it, so the registry is fixed while the change is fresh
and not found broken by the weekly run.

The rest prove the runner on planted cases whose answer is known, without
building anything: the verdicts it reads from the output of cargo and unittest,
the file it puts back, the entries a change reaches, the parts it cuts and the
report it makes of them. A runner that can only be seen saying "caught" is one
nobody has seen work.

The last class reads the workflow that runs it, for the properties that are
quiet when lost: a part that turns red on its own, a pull request that gates, a
cache that pushes out the one of main, a result the report never finds.
"""

import contextlib
import io
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import unittest
from argparse import Namespace
from unittest import mock

import mutate
from test_release_ritual import block, jobs, read, runs, steps

MUTATE = os.path.abspath(mutate.__file__)


def outcome(text, code=0, timed_out=False):
    return mutate.Outcome(code, text, 1.0, timed_out)


def libtest(results, code=None):
    """The output of libtest for {name: ok | FAILED | ignored}."""
    lines = ["running %d tests" % len(results)]
    lines += ["test %s ... %s" % (name, status) for name, status in results.items()]
    failed = "FAILED" in results.values()
    return outcome("\n".join(lines) + "\n", 101 if failed else 0 if code is None else code)


def driver(tests, not_loaded=(), setup_errors=()):
    """The driver's report for {id: [status, name or reason]} or with the
    message as a third element, which is empty when left out."""
    tests = {name: (list(said) + ["", ""])[:3] for name, said in tests.items()}
    return outcome(mutate.DRIVER_MARK + json.dumps({
        "tests": tests, "not_loaded": list(not_loaded), "setup_errors": list(setup_errors)}) + "\n")


def entry(identifier="M1", file="crates/nkb-x/src/lib.rs", find="x = 1", replace="x = 2",
          test="t_ok", runner="rust:nkb-x:lib", why="a break", **more):
    item = {"id": identifier, "why": why, "file": file, "find": find, "replace": replace,
            "test": test, "runner": runner}
    item.update(more)
    return item


class Planted(unittest.TestCase):
    """A tree in a temporary folder, removed after the test."""

    def setUp(self):
        self.root = tempfile.mkdtemp(prefix="mutate-test-")
        self.addCleanup(shutil.rmtree, self.root, ignore_errors=True)
        self.addCleanup(self.remove_rescue)

    def remove_rescue(self):
        path = mutate.rescue_path(self.root)
        if os.path.exists(path):
            os.remove(path)

    def write(self, relative, text):
        path = os.path.join(self.root, *relative.split("/"))
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "wb") as handle:
            handle.write(text.encode("utf-8"))

    def read(self, relative):
        with open(os.path.join(self.root, *relative.split("/")), "rb") as handle:
            return handle.read().decode("utf-8")

    def registry(self, entries, canaries=()):
        self.write(mutate.REGISTRY, json.dumps(
            {"schema": 1, "canaries": list(canaries), "entries": list(entries)}, indent=2))


# ---- the registry of this repository ---------------------------------------------


class TheRegistry(unittest.TestCase):
    def test_every_entry_still_fits_the_code(self):
        # The text to find is in its file exactly once, and the test the entry
        # names is still defined where its runner looks for it. When this fails,
        # move the entry with the code it breaks, in this pull request.
        self.assertEqual(mutate.check(mutate.ROOT), [])

    def test_the_canaries_know_every_verdict_of_both_kinds_of_test(self):
        data = mutate.read_registry(mutate.ROOT)
        known = {(mutate.parse_runner(c["runner"]).kind, c["expect"]) for c in data["canaries"]}
        for kind in ("rust", "pyunit"):
            for verdict in mutate.EXPECTABLE:
                with self.subTest(kind=kind, verdict=verdict):
                    self.assertIn((kind, verdict), known)

    def test_a_full_run_is_never_one_command(self):
        with self.assertRaises(mutate.Refused) as raised:
            mutate.chosen_entries(mutate.ROOT, mutate.read_registry(mutate.ROOT), None, None, None)
        self.assertIn("--only, --base or --shard", str(raised.exception))


# ---- the shape of a registry ----------------------------------------------------


class Shape(unittest.TestCase):
    def problems(self, entries=(), canaries=()):
        data = {"schema": 1, "canaries": list(canaries), "entries": list(entries)}
        return mutate.shape_problems(data)

    def assertProblem(self, words, **changes):
        found = self.problems([entry(**changes)])
        self.assertTrue(any(words in p for p in found), found)

    def test_a_good_entry_has_no_problem(self):
        self.assertEqual(self.problems([entry()]), [])

    def test_each_wrong_field_is_named(self):
        self.assertProblem("the id does not have the form", identifier="K1")
        self.assertProblem("replace is the same text as find", replace="x = 1")
        self.assertProblem("relative path with forward slashes", file="/etc/passwd")
        self.assertProblem("relative path with forward slashes", file="crates/../x")
        self.assertProblem("runner", runner="rust:other:lib")
        self.assertProblem("runner", runner="pyunit:.github/scripts:osv_gate")
        self.assertProblem("runner", runner="samotest:x.py:-y")
        self.assertProblem("keys this script does not know", expect="caught")
        self.assertProblem("why is missing or empty", why="")

    def test_an_entry_finds_and_replaces_or_appends_but_not_both(self):
        both = entry(append="tail")
        self.assertTrue(any("not both" in p for p in self.problems([both])))
        neither = entry()
        del neither["find"], neither["replace"]
        self.assertTrue(any("not both" in p for p in self.problems([neither])))
        appends = entry()
        del appends["find"], appends["replace"]
        appends["append"] = "tail"
        self.assertEqual(self.problems([appends]), [])

    def test_an_id_is_used_once_across_entries_and_canaries(self):
        found = self.problems([entry(), entry()])
        self.assertTrue(any("used 2 times" in p for p in found), found)

    def test_a_canary_says_what_it_expects(self):
        self.assertEqual(self.problems(canaries=[entry(identifier="K1", expect="MISSED")]), [])
        found = self.problems(canaries=[entry(identifier="K1", expect="green")])
        self.assertTrue(any("expect is one of" in p for p in found), found)
        found = self.problems(canaries=[entry(identifier="K1")])
        self.assertTrue(any("expect is missing" in p for p in found), found)

    def test_the_registry_is_ascii_json_of_this_schema(self):
        self.assertIn("outside ASCII", mutate.parse_registry('{"a": "\xe9"}'.encode("utf-8"))[1][0])
        self.assertIn("not JSON", mutate.parse_registry(b"{")[1][0])
        self.assertIn("schema 2", mutate.parse_registry(
            b'{"schema": 2, "canaries": [], "entries": []}')[1][0])
        self.assertIn("exactly schema", mutate.parse_registry(b'{"schema": 1}')[1][0])


# ---- whether an entry fits the code ---------------------------------------------


class FitsTheCode(Planted):
    def setUp(self):
        super().setUp()
        self.write("crates/nkb-x/Cargo.toml", "[package]\n")
        self.write("crates/nkb-x/src/lib.rs", "let x = 1\nmod tests {\n    fn t_ok() {}\n}\n")
        self.write("crates/nkb-x/tests/guard.rs", "fn guards() {}\n")
        self.write("crates/nkb-x/tests/shared/mod.rs", "fn helper() {}\n")
        self.write("crates/nkb-x/src/a/b.rs", "mod tests {\n    fn t_deep() {}\n}\n")
        self.write("crates/nkb-x/src/c/mod.rs", "mod tests {\n    fn t_folder() {}\n}\n")
        self.write(".github/scripts/test_gate.py", "class Gate:\n    def test_blocks(self):\n        pass\n")
        self.tree = mutate.Tree(self.root)

    def problems(self, **changes):
        return mutate.tree_problems(entry(**changes), self.tree)

    def test_a_fitting_entry_has_no_problem(self):
        self.assertEqual(self.problems(), [])
        self.assertEqual(self.problems(test="tests::t_ok"), [])
        self.assertEqual(self.problems(runner="rust:nkb-x:guard", test="guards"), [])
        self.assertEqual(self.problems(runner="rust:nkb-x:bin=x"), [])
        self.assertEqual(self.problems(runner="pyunit:.github/scripts:test_gate", test="test_blocks"), [])

    def test_text_that_moved_or_is_there_twice_is_named(self):
        self.assertIn("0 times", self.problems(find="x = 3")[0])
        self.write("crates/nkb-x/src/lib.rs", "x = 1\nx = 1\nfn t_ok() {}\n")
        self.assertIn("2 times", mutate.tree_problems(entry(), mutate.Tree(self.root))[0])

    def test_a_renamed_test_is_named(self):
        self.assertIn("no function t_gone", self.problems(test="tests::t_gone")[0])
        # A test of the library is looked for in src/, an integration test in tests/.
        self.assertIn("no function guards", self.problems(test="guards")[0])
        self.assertIn("no function t_ok", self.problems(runner="rust:nkb-x:guard")[0])
        self.assertIn("does not exist", self.problems(runner="rust:nkb-x:gone", test="guards")[0])
        self.assertIn("is not a crate", self.problems(runner="rust:nkb-y:lib")[0])
        self.assertIn("named like", self.problems(runner="pyunit:.github/scripts:test_gate", test="test_x")[0])
        self.assertIn("does not exist", self.problems(runner="pyunit:.github/scripts:test_none")[0])

    def test_a_test_of_the_library_is_named_by_the_path_of_its_file(self):
        # cargo test -- --exact runs a test of the library only by its whole
        # path, and a shorter name runs nothing without a word. The path begins
        # with the modules of the file, a folder's mod.rs is the folder, and
        # lib.rs and main.rs add nothing.
        self.assertEqual(self.problems(test="a::b::tests::t_deep"), [])
        self.assertEqual(self.problems(test="c::tests::t_folder"), [])
        self.assertEqual(self.problems(runner="rust:nkb-x:bin=x", test="a::b::tests::t_deep"), [])
        for short in ("t_deep", "tests::t_deep", "b::tests::t_deep", "a::bb::tests::t_deep"):
            with self.subTest(test=short):
                found = self.problems(test=short)
                self.assertEqual(len(found), 1, found)
                self.assertIn("crates/nkb-x/src/a/b.rs, so libtest names it from a::b::", found[0])
        self.assertIn("names it from c::", self.problems(test="tests::t_folder")[0])
        # A name defined in two files fits when it begins with either path.
        self.write("crates/nkb-x/src/c/mod.rs", "mod tests {\n    fn t_folder() {}\n    fn t_deep() {}\n}\n")
        tree = mutate.Tree(self.root)
        self.assertEqual(mutate.tree_problems(entry(test="c::tests::t_deep"), tree), [])
        self.assertIn("a::b:: or c::", mutate.tree_problems(entry(test="t_deep"), tree)[0])
        # An integration test is a program of its own, and the rule is not its.
        self.assertEqual(self.problems(runner="rust:nkb-x:guard", test="helper"), [])

    def test_a_file_that_is_gone_is_named(self):
        self.assertIn("does not exist", self.problems(file="crates/nkb-x/src/gone.rs")[0])


# ---- verdicts -------------------------------------------------------------------


class JudgingRust(unittest.TestCase):
    def judge(self, out, name="a::t"):
        return mutate.judge_rust(out, name, 30).verdict

    def test_the_named_test_failing_is_caught(self):
        self.assertEqual(self.judge(libtest({"a::t": "FAILED"})), mutate.CAUGHT)

    def test_the_named_test_passing_is_missed(self):
        self.assertEqual(self.judge(libtest({"a::t": "ok"})), mutate.MISSED)

    def test_a_break_that_does_not_compile_is_broken_and_never_caught(self):
        out = outcome("   Compiling nkb-x\nerror[E0308]: mismatched types\nerror: could not compile\n", 101)
        verdict = mutate.judge_rust(out, "a::t", 30)
        self.assertEqual(verdict.verdict, mutate.BROKEN)
        self.assertIn("E0308", verdict.detail)

    def test_a_test_that_did_not_run_is_no_test(self):
        self.assertEqual(self.judge(libtest({})), mutate.NO_TEST)
        self.assertEqual(self.judge(libtest({"a::t": "ignored"})), mutate.NO_TEST)
        self.assertEqual(self.judge(outcome("", 0)), mutate.NO_TEST)

    def test_red_that_does_not_name_the_test_is_the_wrong_test(self):
        crashed = outcome("running 1 test\nerror: test failed\n(exit code: 0xc00000fd, STATUS_STACK_OVERFLOW)\n", 101)
        self.assertEqual(self.judge(crashed), mutate.WRONG_TEST)
        self.assertEqual(self.judge(libtest({"a::t": "ok"}, code=101)), mutate.WRONG_TEST)

    def test_a_run_past_its_time_is_a_timeout(self):
        self.assertEqual(self.judge(outcome("", None, timed_out=True)), mutate.TIMEOUT)

    def test_a_name_inside_another_name_is_not_it(self):
        self.assertEqual(self.judge(libtest({"a::t_longer": "FAILED"})), mutate.WRONG_TEST)

    def test_a_test_that_should_panic_is_read_by_its_name(self):
        # libtest adds a tail to the name of a #[should_panic] test.
        self.assertEqual(self.judge(libtest({"a::t - should panic": "FAILED"})), mutate.CAUGHT)
        self.assertEqual(self.judge(libtest({"a::t - should panic": "ok"})), mutate.MISSED)
        self.assertEqual(self.judge(libtest({"a::t_other - should panic": "FAILED"})), mutate.WRONG_TEST)


class JudgingPython(unittest.TestCase):
    def judge(self, out, name="test_blocks"):
        return mutate.judge_pyunit(out, name, 30).verdict

    def test_a_failure_or_an_error_of_the_named_test_is_caught(self):
        self.assertEqual(self.judge(driver({"m.C.test_blocks": ["fail", "AssertionError"]})), mutate.CAUGHT)
        self.assertEqual(self.judge(driver({"m.C.test_blocks": ["error", "KeyError"]})), mutate.CAUGHT)

    def test_code_that_does_not_load_is_broken(self):
        self.assertEqual(self.judge(driver({}, not_loaded=["SyntaxError: invalid syntax"])), mutate.BROKEN)
        self.assertEqual(self.judge(driver({"m.C.test_blocks": ["error", "NameError"]})), mutate.BROKEN)
        self.assertEqual(self.judge(outcome("Traceback\nSyntaxError: invalid syntax\n", 1)), mutate.BROKEN)

    def test_a_pass_is_missed_and_a_skip_is_no_test(self):
        self.assertEqual(self.judge(driver({"m.C.test_blocks": ["ok", ""]})), mutate.MISSED)
        verdict = mutate.judge_pyunit(driver({"m.C.test_blocks": ["skipped", "WiX is not here"]}), "test_blocks", 30)
        self.assertEqual(verdict, mutate.Verdict(mutate.NO_TEST, "skipped here: WiX is not here"))
        self.assertEqual(self.judge(driver({"m.C.test_other": ["fail", "AssertionError"]})), mutate.NO_TEST)

    def test_a_set_up_that_failed_is_the_wrong_test(self):
        self.assertEqual(self.judge(driver({}, setup_errors=["setUpClass (m.C): OSError"])), mutate.WRONG_TEST)


class Baseline(unittest.TestCase):
    def test_one_red_test_of_a_group_leaves_the_others_fit(self):
        rust = mutate.parse_runner("rust:nkb-x:lib")
        out = libtest({"a::red": "FAILED", "a::green": "ok", "a::off": "ignored"})
        self.assertIsNone(mutate.untouched(rust, out, "a::green", 30))
        self.assertEqual(mutate.untouched(rust, out, "a::red", 30).verdict, mutate.BASELINE_RED)
        self.assertEqual(mutate.untouched(rust, out, "a::off", 30).verdict, mutate.NO_TEST)
        self.assertEqual(mutate.untouched(rust, out, "a::gone", 30).verdict, mutate.NO_TEST)

    def test_a_red_test_says_why(self):
        # Without the cause a red baseline in CI can only be guessed at.
        rust = mutate.parse_runner("rust:nkb-x:lib")
        printed = "\n".join([
            "running 2 tests",
            "test a::red ... FAILED",
            "test a::other ... FAILED",
            "",
            "failures:",
            "",
            "---- a::other stdout ----",
            "",
            "thread 'a::other' panicked at src/other.rs:1:1:",
            "not this one",
            "",
            "---- a::red stdout ----",
            "",
            "thread 'a::red' panicked at src/a.rs:10:5:",
            "assertion `left == right` failed",
            "  left: 1",
            "note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace",
            "",
            "failures:",
            "    a::other",
            "    a::red",
            ""])
        self.assertEqual(mutate.untouched(rust, outcome(printed, 101), "a::red", 30).detail,
                         "the test fails on the untouched tree: panicked at src/a.rs:10:5: "
                         "assertion `left == right` failed")
        # A failure without a section of its own says what it can.
        self.assertEqual(mutate.untouched(rust, libtest({"a::red": "FAILED"}), "a::red", 30).detail,
                         "the test fails on the untouched tree")
        python = mutate.parse_runner("pyunit:.github/scripts:test_gate")
        out = driver({"m.C.test_red": ["error", "ValueError", "path is on mount 'D:', start on mount 'C:'"]})
        self.assertEqual(mutate.untouched(python, out, "test_red", 30).detail,
                         "the test fails on the untouched tree: ValueError: path is on mount 'D:', "
                         "start on mount 'C:'")

    def test_an_untouched_tree_that_does_not_build_judges_nothing(self):
        rust = mutate.parse_runner("rust:nkb-x:lib")
        out = outcome("error[E0425]: cannot find value\n", 101)
        self.assertEqual(mutate.untouched(rust, out, "a::green", 30).verdict, mutate.BASELINE_RED)

    def test_the_same_for_python(self):
        python = mutate.parse_runner("pyunit:.github/scripts:test_gate")
        out = driver({"m.C.test_red": ["fail", "AssertionError"], "m.C.test_green": ["ok", ""],
                      "m.C.test_away": ["skipped", "not here"]})
        self.assertIsNone(mutate.untouched(python, out, "test_green", 30))
        self.assertEqual(mutate.untouched(python, out, "test_red", 30).verdict, mutate.BASELINE_RED)
        self.assertEqual(mutate.untouched(python, out, "test_away", 30).verdict, mutate.NO_TEST)
        self.assertEqual(mutate.untouched(python, out, "test_gone", 30).verdict, mutate.NO_TEST)


class RunningACommand(Planted):
    def test_colours_are_stripped(self):
        printed = mutate.execute([sys.executable, "-c", "print('\x1b[32mok\x1b[0m')"], self.root, 60)
        self.assertEqual(printed.text.strip(), "ok")

    def test_a_command_past_its_time_is_stopped(self):
        slept = mutate.execute([sys.executable, "-c", "import time\ntime.sleep(60)"], self.root, 1)
        self.assertTrue(slept.timed_out)
        self.assertLess(slept.seconds, 30)


class TheUnittestDriver(Planted):
    """The driver run for real, on a module whose every outcome is known."""

    def report(self, module, *patterns):
        result = subprocess.run([sys.executable, MUTATE, "unittest-driver", "suite", module, *patterns],
                                cwd=self.root, capture_output=True, check=False)
        return mutate.driver_report(result.stdout.decode("utf-8", "replace"))

    def test_each_outcome_is_read_by_its_test(self):
        self.write("suite/test_known.py", "\n".join([
            "import unittest",
            "class Known(unittest.TestCase):",
            "    def test_passes(self):",
            "        print('noise that must not reach the report')",
            "    def test_fails(self):",
            "        self.assertEqual(1, 2)",
            "    def test_raises(self):",
            "        {}['missing']",
            "    def test_names_nothing(self):",
            "        undefined_name",
            "    @unittest.skip('not here')",
            "    def test_skipped(self):",
            "        pass",
            "    def test_a_subtest_fails(self):",
            "        for i in range(2):",
            "            with self.subTest(i=i):",
            "                self.assertEqual(i, 0)",
            "",
        ]))
        tests = self.report("test_known", "test_")["tests"]
        self.assertEqual(tests, {
            "test_known.Known.test_passes": ["ok", "", ""],
            "test_known.Known.test_fails": ["fail", "AssertionError", "1 != 2"],
            "test_known.Known.test_raises": ["error", "KeyError", "'missing'"],
            "test_known.Known.test_names_nothing": ["error", "NameError", "name 'undefined_name' is not defined"],
            "test_known.Known.test_skipped": ["skipped", "not here", ""],
            "test_known.Known.test_a_subtest_fails": ["fail", "AssertionError", "1 != 0"],
        })
        # The driver picks the way -k does, by a part of the name.
        only = self.report("test_known", "test_fails", "test_passes")
        self.assertEqual(sorted(only["tests"]), ["test_known.Known.test_a_subtest_fails",
                                                 "test_known.Known.test_fails",
                                                 "test_known.Known.test_passes"])
        # The verdict listens to the test of exactly that name.
        report = outcome(mutate.DRIVER_MARK + json.dumps(only))
        self.assertEqual(mutate.judge_pyunit(report, "test_passes", 30).verdict, mutate.MISSED)
        self.assertEqual(sorted(mutate.matching(only, "test_fails")), ["test_known.Known.test_fails"])
        self.assertEqual(len(mutate.matching(only, "Known")), 3)

    def test_a_module_that_does_not_import_is_not_loaded(self):
        self.write("suite/test_broken.py", "import unittest\nclass Broken(unittest.TestCase)\n")
        report = self.report("test_broken", "test_")
        self.assertTrue(report["not_loaded"])
        self.assertIn("SyntaxError", report["not_loaded"][0])


# ---- breaking a file and putting it back ----------------------------------------


class Breaking(Planted):
    def setUp(self):
        super().setUp()
        self.write("crates/nkb-x/src/lib.rs", "let x = 1\n")

    def test_the_test_sees_the_break_and_the_file_comes_back(self):
        seen = []

        def run_test(item):
            seen.append(self.read(item["file"]))
            self.assertTrue(os.path.exists(mutate.rescue_path(self.root)))
            return mutate.Verdict(mutate.CAUGHT, "")

        self.assertEqual(mutate.attempt(entry(), self.root, run_test).verdict, mutate.CAUGHT)
        self.assertEqual(seen, ["let x = 2\n"])
        self.assertEqual(self.read("crates/nkb-x/src/lib.rs"), "let x = 1\n")
        self.assertFalse(os.path.exists(mutate.rescue_path(self.root)))

    def test_the_file_comes_back_when_the_test_throws(self):
        def run_test(item):
            raise KeyboardInterrupt

        with self.assertRaises(KeyboardInterrupt):
            mutate.attempt(entry(), self.root, run_test)
        self.assertEqual(self.read("crates/nkb-x/src/lib.rs"), "let x = 1\n")
        self.assertFalse(os.path.exists(mutate.rescue_path(self.root)))

    def test_append_adds_at_the_end(self):
        item = entry()
        del item["find"], item["replace"]
        item["append"] = "tail\n"
        seen = []
        mutate.attempt(item, self.root, lambda i: seen.append(self.read(i["file"])) or mutate.Verdict(mutate.CAUGHT, ""))
        self.assertEqual(seen, ["let x = 1\ntail\n"])

    def test_text_that_is_not_there_once_breaks_nothing(self):
        def run_test(item):
            raise AssertionError("no test may run for a stale entry")

        self.assertEqual(mutate.attempt(entry(find="x = 9"), self.root, run_test).verdict, mutate.STALE)
        self.write("crates/nkb-x/src/lib.rs", "x = 1\nx = 1\n")
        self.assertEqual(mutate.attempt(entry(), self.root, run_test).verdict, mutate.STALE)
        self.assertEqual(self.read("crates/nkb-x/src/lib.rs"), "x = 1\nx = 1\n")

    def test_a_killed_run_blocks_the_next_until_the_file_is_put_back(self):
        # What a run killed between the break and the put-back leaves behind.
        original = "let x = 1\n".encode("utf-8")
        with open(mutate.rescue_path(self.root), "wb") as handle:
            handle.write(json.dumps({"crates/nkb-x/src/lib.rs": original.hex()}).encode("utf-8"))
        self.write("crates/nkb-x/src/lib.rs", "let x = 2\n")
        with self.assertRaises(mutate.Refused) as raised:
            mutate.refuse_after_a_killed_run(self.root)
        self.assertIn("crates/nkb-x/src/lib.rs", str(raised.exception))
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(mutate.restore(self.root), 0)
        self.assertEqual(self.read("crates/nkb-x/src/lib.rs"), "let x = 1\n")
        mutate.refuse_after_a_killed_run(self.root)


# ---- a whole run, with a cargo that is planted ----------------------------------


def planted_cargo(command, root, timeout):
    """Answers like cargo test: t_ok fails when the file says BREAK, t_red always
    fails, t_gone does not exist, and NOCOMPILE does not build."""
    names = command[command.index("never") + 1:]
    with open(os.path.join(root, "crates", "nkb-x", "src", "lib.rs"), "rb") as handle:
        text = handle.read().decode("utf-8")
    if "NOCOMPILE" in text:
        return outcome("error[E0308]: mismatched types\n", 101)
    results = {}
    for name in names:
        if name != "t_gone":
            red = name == "t_red" or (name == "t_ok" and "BREAK" in text)
            results[name] = "FAILED" if red else "ok"
    return libtest(results)


class ARun(Planted):
    def setUp(self):
        super().setUp()
        self.write("crates/nkb-x/src/lib.rs", "let x = 1\nlet y = 1\n")
        self.canaries = [
            entry("K1", replace="BREAK", expect="caught"),
            entry("K2", replace="x = 3", expect="MISSED"),
            entry("K3", replace="NOCOMPILE", expect="BROKEN"),
        ]

    def run_entries(self, entries, canaries=None):
        data = {"schema": 1, "canaries": self.canaries if canaries is None else canaries, "entries": entries}
        record = {"canaries": [], "entries": []}
        code = mutate.run_entries(data, entries, self.root, record, "unused", 30,
                                  lambda: None, lambda line: None, planted_cargo)
        return code, record

    def test_every_verdict_of_a_run(self):
        entries = [
            entry("M1", replace="BREAK"),
            entry("M2", find="y = 1", replace="y = 2"),
            entry("M3", test="t_red", replace="BREAK"),
            entry("M4", test="t_gone", replace="BREAK"),
            entry("M5", find="z = 1"),
            entry("M6", find="y = 1", replace="NOCOMPILE"),
        ]
        code, record = self.run_entries(entries)
        self.assertEqual(code, 1)
        self.assertTrue(record["trusted"])
        self.assertTrue(record["finished"])
        self.assertEqual([c["verdict"] for c in record["canaries"]], ["caught", "MISSED", "BROKEN"])
        self.assertEqual({e["id"]: e["verdict"] for e in record["entries"]}, {
            "M1": mutate.CAUGHT, "M2": mutate.MISSED, "M3": mutate.BASELINE_RED,
            "M4": mutate.NO_TEST, "M5": mutate.STALE, "M6": mutate.BROKEN})
        self.assertEqual(self.read("crates/nkb-x/src/lib.rs"), "let x = 1\nlet y = 1\n")

    def test_a_run_of_caught_entries_exits_0(self):
        self.assertEqual(self.run_entries([entry("M1", replace="BREAK")])[0], 0)

    def test_a_canary_that_comes_out_otherwise_stops_the_run(self):
        lying = [entry("K1", replace="x = 3", expect="caught")]
        code, record = self.run_entries([entry("M1", replace="BREAK")], canaries=lying)
        self.assertEqual(code, 1)
        self.assertFalse(record["trusted"])
        self.assertEqual(record["entries"], [])


# ---- choosing the entries a change reaches --------------------------------------


class Reaching(Planted):
    def setUp(self):
        super().setUp()
        self.write("crates/nkb-x/src/lib.rs", "let x = 1\n")
        self.write("crates/nkb-x/src/tests.rs", "fn t_ok() {}\n")
        self.write("crates/nkb-x/src/other.rs", "fn unrelated() {}\n")
        self.entries = [
            entry("M1"),
            entry("M2", file="crates/nkb-x/src/other.rs", find="unrelated", replace="related",
                  runner="rust:nkb-x:guard", test="guards"),
            entry("M3", file=".github/scripts/gate.py", find="a", replace="b",
                  runner="pyunit:.github/scripts:test_gate", test="test_blocks"),
        ]

    def reached(self, changed, base=None):
        base = [dict(e) for e in self.entries] if base is None else base
        chosen, _ = mutate.reached(self.entries, base, changed, mutate.Tree(self.root))
        return [e["id"] for e in chosen]

    def test_a_change_elsewhere_reaches_nothing(self):
        self.assertEqual(self.reached(["README.md", "web/content/en/_index.md"]), [])

    def test_the_file_an_entry_breaks(self):
        self.assertEqual(self.reached(["crates/nkb-x/src/lib.rs"]), ["M1"])

    def test_the_file_a_library_test_is_defined_in(self):
        self.assertEqual(self.reached(["crates/nkb-x/src/tests.rs"]), ["M1"])
        self.assertEqual(self.reached(["crates/nkb-y/src/tests.rs"]), [])

    def test_an_integration_test_and_a_helper_of_the_crate_s_tests(self):
        self.assertEqual(self.reached(["crates/nkb-x/tests/guard.rs"]), ["M2"])
        self.assertEqual(self.reached(["crates/nkb-x/tests/other.rs"]), [])
        self.assertEqual(self.reached(["crates/nkb-x/tests/shared/mod.rs"]), ["M2"])

    def test_a_test_module_of_a_script(self):
        self.assertEqual(self.reached([".github/scripts/test_gate.py"]), ["M3"])

    def test_an_entry_that_is_new_or_changed(self):
        base = [dict(e) for e in self.entries]
        base[0]["why"] = "said otherwise before"
        self.assertEqual(self.reached([mutate.REGISTRY], base=base), ["M1"])
        self.assertEqual(self.reached([mutate.REGISTRY], base=base[1:]), ["M1"])

    def test_the_runner_the_workflow_or_no_base_reach_everything(self):
        for changed in mutate.EVERYTHING:
            with self.subTest(changed=changed):
                self.assertEqual(self.reached([changed]), ["M1", "M2", "M3"])
        chosen, reason = mutate.reached(self.entries, None, [], mutate.Tree(self.root))
        self.assertEqual(len(chosen), 3)
        self.assertIn("no registry", reason)


class ChangedPaths(Planted):
    def git(self, *arguments):
        subprocess.run(["git", "-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false",
                        *arguments], cwd=self.root, check=True, capture_output=True)

    def test_both_sides_of_a_rename_and_the_working_tree_are_changed(self):
        self.write("a.txt", "a\n")
        self.write("b.txt", "b\n")
        self.git("init", "-q")
        self.git("add", ".")
        self.git("commit", "-q", "-m", "base")
        self.git("mv", "a.txt", "c.txt")
        self.git("commit", "-q", "-m", "rename")
        self.write("b.txt", "changed and not committed\n")
        base = "HEAD~1"
        self.assertEqual(mutate.changed_paths(self.root, base), ["a.txt", "b.txt", "c.txt"])
        self.assertIsNone(mutate.base_registry(self.root, base))


# ---- cutting into parts ---------------------------------------------------------


class Cutting(unittest.TestCase):
    def entries(self):
        made = []
        for number in range(1, 61):
            runner = ("rust:nkb-gui:lib", "rust:nkb-core:lib", "pyunit:.github/scripts:test_gate")[number % 3]
            place = {"rust:nkb-gui:lib": "crates/nkb-gui/src/a.rs", "rust:nkb-core:lib": "crates/nkb-core/src/a.rs",
                     "pyunit:.github/scripts:test_gate": ".github/scripts/gate.py"}[runner]
            made.append(entry("M%d" % number, file=place, runner=runner))
        return made

    def test_every_entry_lands_in_one_part_in_order(self):
        entries = self.entries()
        for parts in (1, 2, 5, 16):
            with self.subTest(parts=parts):
                slices = mutate.cut(entries, parts)
                self.assertEqual(len(slices), parts)
                self.assertEqual([e for s in slices for e in s], mutate.ordered(entries))

    def test_parts_carry_about_the_same_weight(self):
        entries = self.entries()
        slices = mutate.cut(entries, 4)
        weights = [sum(mutate.weight(e) for e in s) for s in slices]
        heaviest = max(mutate.weight(e) for e in entries)
        self.assertLessEqual(max(weights) - min(weights), 2 * heaviest, weights)

    def test_entries_of_one_crate_stay_together(self):
        places = [mutate.place_of(e) for e in mutate.ordered(self.entries())]
        self.assertEqual(places, sorted(places))

    def test_how_many_parts(self):
        entries = self.entries()
        self.assertEqual(mutate.parts_for([], True), 0)
        self.assertEqual(mutate.parts_for(entries, True), 16)
        self.assertEqual(mutate.parts_for(entries, False), 3)
        self.assertEqual(mutate.parts_for(entries[:5], False), 1)
        self.assertEqual(mutate.parts_for(entries[:3], True), 3)
        for whole in (True, False):
            parts = mutate.parts_for(entries[:7], whole)
            self.assertTrue(all(mutate.cut(entries[:7], parts)))

    def test_a_shard_is_part_slash_parts(self):
        self.assertEqual(mutate.parse_shard("3/16"), (3, 16))
        for wrong in ("16/16", "3", "a/b", "-1/4", "0/0"):
            with self.subTest(shard=wrong):
                with self.assertRaises(mutate.Refused):
                    mutate.parse_shard(wrong)


class Choosing(unittest.TestCase):
    def test_only_names_known_entries_and_goes_alone(self):
        data = {"entries": [entry("M1"), entry("M2")], "canaries": []}
        chosen, _, part = mutate.chosen_entries("unused", data, "M2,M1,M2", None, None)
        self.assertEqual([e["id"] for e in chosen], ["M2", "M1"])
        self.assertEqual(part, (0, 1))
        with self.assertRaises(mutate.Refused):
            mutate.chosen_entries("unused", data, "M3", None, None)
        with self.assertRaises(mutate.Refused):
            mutate.chosen_entries("unused", data, "M1", None, "0/2")


# ---- the plan and the report ----------------------------------------------------


class PlanAndReport(Planted):
    def setUp(self):
        super().setUp()
        self.write("crates/nkb-x/src/lib.rs", "let x = 1\nlet y = 1\n")
        self.registry([entry("M1", why="a, b: c"), entry("M2", find="y = 1")])
        self.results = os.path.join(self.root, "results")
        os.makedirs(self.results)
        environment = mock.patch.dict(os.environ)
        environment.start()
        self.addCleanup(environment.stop)
        os.environ.pop("GITHUB_STEP_SUMMARY", None)

    def result(self, part, parts, verdicts, finished=True, trusted=True):
        mutate.write_json(os.path.join(self.results, "result-%d.json" % part), {
            "schema": 1, "part": part, "parts": parts, "selected": len(verdicts),
            "entries": [{"id": i, "verdict": v, "detail": "", "seconds": 1.0} for i, v in verdicts.items()],
            "canaries": [], "trusted": trusted, "finished": finished, "seconds": 60})

    def report(self, parts=2, entries=2, gate=True):
        printed = io.StringIO()
        with contextlib.redirect_stdout(printed):
            code = mutate.report(Namespace(folder=self.results, entries=entries, parts=parts,
                                           gate=gate, root=self.root))
        return code, printed.getvalue()

    def test_everything_caught_is_green(self):
        self.result(0, 2, {"M1": "caught"})
        self.result(1, 2, {"M2": "caught"})
        code, text = self.report()
        self.assertEqual(code, 0)
        self.assertIn("2 of 2 entries caught, in 2 of 2 parts", text)
        self.assertNotIn("::", text)

    def test_a_missed_entry_is_red_in_the_week_and_a_warning_on_a_pull_request(self):
        self.result(0, 2, {"M1": "MISSED"})
        self.result(1, 2, {"M2": "caught"})
        code, text = self.report()
        self.assertEqual(code, 1)
        self.assertIn("::error file=crates/nkb-x/src/lib.rs,line=1,title=M1 MISSED::a, b: c", text)
        code, text = self.report(gate=False)
        self.assertEqual(code, 0)
        self.assertIn("::warning file=", text)

    def test_a_part_that_did_not_report_or_finish_is_a_failure_not_a_smaller_run(self):
        self.result(0, 2, {"M1": "caught"})
        code, text = self.report()
        self.assertEqual(code, 1)
        self.assertIn("part 1 did not report", text)
        self.result(1, 2, {}, finished=False)
        self.assertIn("part 1 stopped before its last entry", self.report()[1])
        self.result(1, 2, {}, trusted=False)
        self.assertIn("a canary came out otherwise", self.report()[1])

    def test_a_count_that_does_not_add_up_is_a_failure(self):
        self.result(0, 1, {"M1": "caught"})
        code, text = self.report(parts=1, entries=2)
        self.assertEqual(code, 1)
        self.assertIn("the plan chose 2 entries and the parts took 1", text)

    def test_the_plan_writes_what_the_workflow_reads(self):
        output = os.path.join(self.root, "output")
        with contextlib.redirect_stdout(io.StringIO()):
            mutate.plan(Namespace(root=self.root, base=None, output=output))
        with open(output, encoding="utf-8") as handle:
            self.assertEqual(handle.read(), "entries=2\nparts=2\nshards=[0, 1]\n")

    def test_annotation_text_is_escaped(self):
        self.assertEqual(mutate.escape_property("a,b:c%"), "a%2Cb%3Ac%25")
        self.assertEqual(mutate.escape_data("a\nb\r%"), "a%0Ab%0D%25")


# ---- the workflow that runs it --------------------------------------------------


class TheWorkflow(unittest.TestCase):
    """mutation.yml read as text, each property from the narrowest piece that
    carries it, with the readers test_release_ritual.py uses."""

    def setUp(self):
        self.text = read(os.path.join(mutate.ROOT, ".github", "workflows", "mutation.yml"))
        self.jobs = jobs(self.text)

    def step(self, job, name):
        found = [s for s in steps(self.jobs[job]) if "name: %s\n" % name in s]
        self.assertEqual(len(found), 1, name)
        return found[0]

    def test_it_runs_on_a_pull_request_every_thursday_and_by_hand_and_never_on_a_push(self):
        on = block(self.text, "on:", 0)
        self.assertIn("branches: [main]", block(on, "pull_request:", 2))
        self.assertIn('- cron: "13 2 * * 4"', block(on, "schedule:", 2))
        self.assertRegex(on, r"(?m)^  workflow_dispatch:\s*$")
        self.assertNotRegex(on, r"(?m)^  push:")

    def test_it_only_reads_and_every_checkout_forgets_its_token(self):
        self.assertEqual(block(self.text, "permissions:", 0).split(), ["contents:", "read"])
        self.assertEqual(self.text.count("permissions:"), 1)
        self.assertIn("cancel-in-progress: ${{ github.event_name == 'pull_request' }}",
                      block(self.text, "concurrency:", 0))
        checkouts = [s for job in self.jobs.values() for s in steps(job) if "actions/checkout@" in s]
        self.assertEqual(len(checkouts), 3)
        for checkout in checkouts:
            self.assertIn("persist-credentials: false", checkout)
        for action in re.findall(r"uses: (\S+)", self.text):
            with self.subTest(action=action):
                self.assertRegex(action, r"^(\./\.github/actions/[a-z-]+|[\w.-]+/[\w.-]+@[0-9a-f]{40})$")

    def test_no_expression_is_pasted_into_a_shell(self):
        for command in runs(self.text):
            self.assertNotIn("${{", command)

    def test_the_plan_decides_the_parts_and_a_pull_request_is_compared_with_its_base(self):
        self.assertEqual(mutate.MAX_PARTS, 16)
        self.assertIn("shard: ${{ fromJSON(needs.plan.outputs.shards) }}", self.jobs["run"])
        for job, name in (("plan", "Choose the entries"), ("run", "Break the entries of this part")):
            with self.subTest(job=job):
                self.assertIn("fetch-depth: 0", self.step(job, "Check out the code"))
                self.assertIn("BASE: ${{ github.event.pull_request.base.sha }}", self.step(job, name))
        self.assertIn('--base "$BASE"', self.step("run", "Break the entries of this part"))
        self.assertIn('--shard "$SHARD/$PARTS"', self.step("run", "Break the entries of this part"))

    def test_the_parts_run_on_windows_without_a_cache_and_never_turn_red_themselves(self):
        part = self.jobs["run"]
        self.assertIn("runs-on: windows-latest", part)
        self.assertIn("fail-fast: false", part)
        self.assertIn('cache: "false"', self.step("run", "Set up Rust"))
        self.assertNotIn("CARGO_INCREMENTAL", self.text)
        breaking = self.step("run", "Break the entries of this part")
        self.assertIn("continue-on-error: true", breaking)
        job_limit = int(re.search(r"(?m)^    timeout-minutes: (\d+)", part).group(1))
        step_limit = int(re.search(r"timeout-minutes: (\d+)", breaking).group(1))
        self.assertLess(step_limit, job_limit)
        self.assertLessEqual(job_limit, 360)
        self.assertIn("if: ${{ always() }}", self.step("run", "Keep the verdicts of this part"))

    def test_what_a_part_keeps_is_what_the_report_reads(self):
        self.assertIn('--json "result-$SHARD.json"', self.step("run", "Break the entries of this part"))
        kept = self.step("run", "Keep the verdicts of this part")
        self.assertIn("name: mutation-result-${{ matrix.shard }}", kept)
        self.assertIn("path: result-${{ matrix.shard }}.json", kept)
        collected = self.step("report", "Collect the verdicts of every part")
        self.assertIn("pattern: mutation-result-*", collected)
        self.assertIn("path: results", collected)
        self.assertIn("merge-multiple: true", collected)
        self.assertIn("--folder results", self.step("report", "Report what the parts found"))

    def test_only_a_run_off_a_pull_request_is_red(self):
        reporting = self.step("report", "Report what the parts found")
        self.assertIn("EVENT: ${{ github.event_name }}", reporting)
        self.assertIn('if [ "$EVENT" != "pull_request" ]\n          then\n            gate=(--gate)',
                      reporting)
        self.assertIn("!cancelled()", self.jobs["report"])

    def test_a_change_to_the_runner_or_this_workflow_reaches_every_entry(self):
        for path in mutate.EVERYTHING:
            with self.subTest(path=path):
                self.assertTrue(os.path.isfile(os.path.join(mutate.ROOT, *path.split("/"))))


if __name__ == "__main__":
    unittest.main()
