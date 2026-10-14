"""Tests for osv_gate.py and for the workflow that runs it. They need nothing but
the standard library.

    python3 -m unittest discover -s .github/scripts -p "test_*.py"

Every case that decides a merge has a test that shows the deciding. A gate that
can only be seen passing is a gate nobody has seen work. The workflow is read as
text, with the helpers of test_release_ritual.py, because the flags that keep the
scan honest live there and not in the gate.
"""

import contextlib
import io
import json
import os
import re
import tempfile
import unittest

import osv_gate
import release
from test_release_ritual import WORKFLOWS, block, jobs, read, steps

ROOT = "/github/workspace"
WORKFLOW = os.path.join(WORKFLOWS, "osv-scanner.yml")
EXPECTED = ("Cargo.lock", ".github/scripts/requirements.txt")


def package(name="smallvec", version="1.6.0", ecosystem="crates.io", vulnerabilities=(), groups=None):
    entry = {"package": {"name": name, "version": version, "ecosystem": ecosystem}}
    if vulnerabilities:
        entry["vulnerabilities"] = list(vulnerabilities)
    if groups is not None:
        entry["groups"] = groups
    return entry


def advisory(id, summary="a summary"):
    return {"id": id, "summary": summary}


def result(path, *packages):
    return {"source": {"path": ROOT + "/" + path, "type": "lockfile"}, "packages": list(packages)}


def clean():
    """A report of the two files the workflow expects, with nothing found."""
    return {"results": [result("Cargo.lock", package("slint", "1.18.1"), package("serde", "1.0.0")),
                        result(".github/scripts/requirements.txt", package("semgrep", "1.179.0", "PyPI"))]}


class Run:
    """One run of the gate on a report, with what it printed."""

    def __init__(self, content, *flags, raw=False, expect=EXPECTED):
        with tempfile.TemporaryDirectory() as directory:
            path = os.path.join(directory, "osv.json")
            with open(path, "w", encoding="utf-8") as handle:
                handle.write(content if raw else json.dumps(content))
            args = [path, "--root", ROOT, *flags]
            for name in expect:
                args += ["--expect", name]
            out, err = io.StringIO(), io.StringIO()
            with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
                self.code = osv_gate.main(args)
        self.out, self.err = out.getvalue(), err.getvalue()


class Verdicts(unittest.TestCase):
    def test_a_clean_report_of_every_expected_file_passes_and_says_what_it_read(self):
        run = Run(clean())
        self.assertEqual(run.code, osv_gate.EXIT_PASSES)
        self.assertIn("Cargo.lock: 2 package(s)", run.out)
        self.assertIn(".github/scripts/requirements.txt: 1 package(s)", run.out)
        self.assertIn("osv: 0 advisory finding(s), 3 package(s) in 2 file(s)", run.out)

    def test_any_advisory_blocks_whatever_its_kind(self):
        # The owner's choice: every known advisory blocks, an unmaintained notice
        # with no severity as much as a critical one.
        for summary in ("Buffer overflow in SmallVec::insert_many", "bincode is unmaintained", ""):
            with self.subTest(summary=summary):
                report = clean()
                report["results"][0]["packages"].append(
                    package(vulnerabilities=[advisory("RUSTSEC-2021-0003", summary)]))
                run = Run(report)
                self.assertEqual(run.code, osv_gate.EXIT_BLOCKS)
                self.assertIn("RUSTSEC-2021-0003 smallvec 1.6.0 (crates.io) in Cargo.lock", run.out)

    def test_an_advisory_in_the_python_file_blocks_too(self):
        report = clean()
        report["results"][1]["packages"][0]["vulnerabilities"] = [advisory("PYSEC-2026-1")]
        run = Run(report)
        self.assertEqual(run.code, osv_gate.EXIT_BLOCKS)
        self.assertIn("in .github/scripts/requirements.txt", run.out)

    def test_the_ids_of_one_advisory_are_one_finding(self):
        report = clean()
        report["results"][0]["packages"].append(package(
            vulnerabilities=[advisory("RUSTSEC-2021-0003", "overflow"), advisory("GHSA-43w2-9j62-hq99", "")],
            groups=[{"ids": ["RUSTSEC-2021-0003", "GHSA-43w2-9j62-hq99"]}]))
        run = Run(report)
        self.assertEqual(run.code, osv_gate.EXIT_BLOCKS)
        self.assertIn("RUSTSEC-2021-0003 / GHSA-43w2-9j62-hq99 smallvec", run.out)
        self.assertIn("overflow", run.out)
        self.assertIn("osv: 1 advisory finding(s)", run.out)

    def test_an_advisory_no_group_names_still_counts(self):
        report = clean()
        report["results"][0]["packages"].append(package(
            vulnerabilities=[advisory("RUSTSEC-1"), advisory("RUSTSEC-2")],
            groups=[{"ids": ["RUSTSEC-1"]}]))
        run = Run(report)
        self.assertEqual(run.code, osv_gate.EXIT_BLOCKS)
        self.assertIn("osv: 2 advisory finding(s)", run.out)


class Unusable(unittest.TestCase):
    def assertUnusable(self, run, words):
        self.assertEqual(run.code, osv_gate.EXIT_UNUSABLE)
        self.assertIn(words, run.err)

    def test_a_report_without_an_expected_file_is_no_verdict(self):
        # The quiet failure this gate exists for: Cargo.lock moved, the walk read
        # the rest, found nothing and would have passed.
        report = clean()
        del report["results"][0]
        self.assertUnusable(Run(report), "not read, so not checked: Cargo.lock")

    def test_an_expected_file_with_no_package_is_no_verdict(self):
        # What a scan without --all-packages looks like when it finds nothing.
        report = clean()
        report["results"][0]["packages"] = []
        self.assertUnusable(Run(report), "not read, so not checked: Cargo.lock")

    def test_a_file_of_the_same_name_elsewhere_does_not_stand_in(self):
        report = clean()
        report["results"][0]["source"]["path"] = ROOT + "/tools/Cargo.lock"
        self.assertUnusable(Run(report), "Cargo.lock")

    def test_a_missing_or_broken_report_is_no_verdict(self):
        self.assertUnusable(Run("{not json", raw=True), "cannot read")
        self.assertUnusable(Run({"something": "else"}), "is not an OSV-Scanner report")
        self.assertUnusable(Run({"results": [{"packages": []}]}), "names no file")
        self.assertUnusable(Run({"results": [{"source": {"path": ROOT + "/Cargo.lock"}}]}),
                            "lists no packages")

    def test_an_advisory_without_an_id_is_no_verdict(self):
        report = clean()
        report["results"][0]["packages"].append(package(vulnerabilities=[{"summary": "?"}]))
        self.assertUnusable(Run(report), "not an object with an id")

    def test_the_gate_refuses_to_run_without_a_file_it_must_see(self):
        with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
            osv_gate.main(["osv.json"])


class Canary(unittest.TestCase):
    def test_a_canary_that_is_blocked_proves_the_gate(self):
        report = {"results": [result(".osv-run/Cargo.lock",
                                     package(vulnerabilities=[advisory("RUSTSEC-2021-0003")]))]}
        run = Run(report, "--canary", expect=(".osv-run/Cargo.lock",))
        self.assertEqual(run.code, osv_gate.EXIT_PASSES)
        self.assertIn("the canary was blocked", run.out)

    def test_a_canary_that_passes_fails_the_run(self):
        report = {"results": [result(".osv-run/Cargo.lock", package())]}
        run = Run(report, "--canary", expect=(".osv-run/Cargo.lock",))
        self.assertEqual(run.code, osv_gate.EXIT_CANARY_PASSED)
        self.assertIn("NOT blocked", run.out)

    def test_a_canary_that_was_not_read_is_no_verdict(self):
        run = Run({"results": []}, "--canary", expect=(".osv-run/Cargo.lock",))
        self.assertEqual(run.code, osv_gate.EXIT_UNUSABLE)


class Workflow(unittest.TestCase):
    """The flags that keep the scan honest, read from the workflow itself."""

    def setUp(self):
        self.text = read(WORKFLOW)
        self.job = jobs(self.text)["osv-scanner"]
        self.steps = {re.search(r"name: (.*)", step).group(1).strip(): step for step in steps(self.job)}

    def scan_args(self, name):
        return block(self.steps[name], "scan-args: |-", 10)

    def test_every_scan_fails_on_no_lock_file_and_reads_the_one_list_of_exceptions(self):
        for name in ("Scan it", "Scan the lock files", "Scan the lock files for the Security tab"):
            with self.subTest(step=name):
                args = self.scan_args(name)
                self.assertIn("--allow-no-lockfiles=false", args)
                self.assertIn("--config=osv-scanner.toml", args)
                self.assertNotIn("--no-ignore", args)

    def test_the_repository_is_scanned_whole_without_guessing_and_without_the_canary(self):
        for name in ("Scan the lock files", "Scan the lock files for the Security tab"):
            with self.subTest(step=name):
                args = self.scan_args(name).split()
                self.assertIn("--recursive", args)
                self.assertIn("--no-resolve", args)
                self.assertEqual(args[-1], "./")
                excluded = [a for a in args if a.startswith("--x-exclude")]
                self.assertEqual(excluded, ["--x-exclude=.osv-run"])

    def test_the_report_the_gate_reads_lists_every_package(self):
        # Without --all-packages a file with no finding is not in the report, and
        # the gate would refuse every clean run.
        args = self.scan_args("Scan the lock files")
        self.assertIn("--all-packages", args)
        self.assertIn("--output-file=.osv-run/osv.json", args)
        decide = self.steps["Decide"]
        self.assertIn(".osv-run/osv.json", decide)
        for name in EXPECTED:
            self.assertIn("--expect " + name, decide)

    def test_the_canary_is_proved_before_the_verdict(self):
        names = list(self.steps)
        self.assertLess(names.index("Prove the gate can block"), names.index("Decide"))
        self.assertIn("--canary", self.steps["Prove the gate can block"])
        self.assertNotIn("continue-on-error", self.steps["Prove the gate can block"])
        self.assertNotIn("continue-on-error", self.steps["Decide"])
        self.assertEqual(names[-1], "Decide")

    def test_only_runs_on_main_send_anything_to_the_security_tab(self):
        for name in ("Scan the lock files for the Security tab", "Send the result to the Security tab"):
            with self.subTest(step=name):
                self.assertIn("if: github.event_name != 'pull_request'", self.steps[name])
        self.assertIn("category: osv-scanner", self.steps["Send the result to the Security tab"])

    def test_it_starts_on_every_pull_request_and_push_to_main_and_every_week(self):
        on = block(self.text, "on:", 0)
        self.assertIn("branches: [main]", block(on, "pull_request:", 2))
        self.assertIn("branches: [main]", block(on, "push:", 2))
        self.assertIn("cron:", block(on, "schedule:", 2))
        self.assertNotIn("paths", on)

    def test_only_this_job_may_write_and_only_to_code_scanning(self):
        self.assertIn("permissions:\n  contents: read\n", self.text)
        allowed = block(self.job, "permissions:", 4)
        self.assertEqual(sorted(line.split(":")[0].strip() for line in allowed.splitlines()
                                if line.strip() and not line.strip().startswith("#")),
                         ["contents", "security-events"])

    def test_a_release_waits_for_it(self):
        self.assertIn("osv-scanner.yml", release.REQUIRED_WORKFLOWS)


if __name__ == "__main__":
    unittest.main()
