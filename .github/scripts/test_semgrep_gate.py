"""Tests for semgrep_gate.py. They need nothing but the standard library.

    python3 -m unittest discover -s .github/scripts -p "test_*.py"

Every case that decides a merge has a test that shows the deciding. A gate that
can only be seen passing is a gate nobody has seen work.
"""

import contextlib
import io
import json
import os
import tempfile
import unittest

import semgrep_gate


def finding(severity, rule="some.rule", path="src/a.rs", line=3, message="a message"):
    extra = {"message": message}
    if severity is not None:
        extra["severity"] = severity
    return {"check_id": rule, "path": path, "start": {"line": line}, "extra": extra}


def report(results=(), errors=(), scanned=("src/a.rs",)):
    return {"results": list(results), "errors": list(errors), "paths": {"scanned": list(scanned)}}


class Run:
    """One run of the gate on a report, with what it printed."""

    def __init__(self, content, *flags, raw=False):
        with tempfile.TemporaryDirectory() as directory:
            path = os.path.join(directory, "report.json")
            with open(path, "w", encoding="utf-8") as handle:
                handle.write(content if raw else json.dumps(content))
            out, err = io.StringIO(), io.StringIO()
            with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
                self.code = semgrep_gate.main([path, *flags])
        self.out, self.err = out.getvalue(), err.getvalue()


class SeverityTests(unittest.TestCase):
    def test_a_clean_report_passes(self):
        run = Run(report())
        self.assertEqual(run.code, semgrep_gate.EXIT_PASSES)
        self.assertIn("0 blocking", run.out)

    def test_error_high_and_critical_block_in_any_case(self):
        for severity in ("ERROR", "HIGH", "CRITICAL", "error", "High", "critical"):
            with self.subTest(severity=severity):
                run = Run(report([finding(severity)]))
                self.assertEqual(run.code, semgrep_gate.EXIT_BLOCKS)
                self.assertIn("blocking findings:", run.out)

    def test_the_severity_that_the_flag_ignores_is_the_one_that_blocks(self):
        # The measured reason this script exists. `--severity ERROR` returned no
        # finding at all for a rule that declared HIGH.
        run = Run(report([finding("HIGH", rule="probe.high")]))
        self.assertEqual(run.code, semgrep_gate.EXIT_BLOCKS)
        self.assertIn("probe.high", run.out)

    def test_info_warning_and_the_middle_of_the_newer_scale_pass(self):
        for severity in ("INFO", "WARNING", "LOW", "MEDIUM", "EXPERIMENT", "INVENTORY", "info"):
            with self.subTest(severity=severity):
                run = Run(report([finding(severity)]))
                self.assertEqual(run.code, semgrep_gate.EXIT_PASSES)
                self.assertIn("other findings", run.out)

    def test_a_severity_nobody_taught_the_gate_blocks_and_says_so(self):
        run = Run(report([finding("SEVERE")]))
        self.assertEqual(run.code, semgrep_gate.EXIT_BLOCKS)
        self.assertIn("severity not known", run.out)

    def test_a_finding_with_no_severity_blocks(self):
        run = Run(report([finding(None)]))
        self.assertEqual(run.code, semgrep_gate.EXIT_BLOCKS)
        self.assertIn("NO SEVERITY", run.out)

    def test_one_blocking_finding_among_many_passing_ones_still_blocks(self):
        findings = [finding("INFO") for _ in range(50)] + [finding("ERROR", rule="the.one")]
        run = Run(report(findings))
        self.assertEqual(run.code, semgrep_gate.EXIT_BLOCKS)
        self.assertIn("the.one", run.out)
        self.assertIn("1 blocking, 50 other", run.out)


class ScanErrorTests(unittest.TestCase):
    def test_an_error_of_level_error_blocks(self):
        problem = {"level": "error", "type": ["RuleParseError", []], "message": "bad rule"}
        run = Run(report(errors=[problem]))
        self.assertEqual(run.code, semgrep_gate.EXIT_BLOCKS)
        self.assertIn("RuleParseError: bad rule", run.out)

    def test_an_error_of_level_warn_is_reported_and_does_not_block(self):
        problem = {"level": "warn", "type": "PartialParsing", "message": "line 3"}
        run = Run(report(errors=[problem]))
        self.assertEqual(run.code, semgrep_gate.EXIT_PASSES)
        self.assertIn("PartialParsing: line 3", run.out)

    def test_the_level_is_read_in_any_case(self):
        run = Run(report(errors=[{"level": "ERROR", "type": "X", "message": "m"}]))
        self.assertEqual(run.code, semgrep_gate.EXIT_BLOCKS)


class UnusableReportTests(unittest.TestCase):
    def check_unusable(self, run):
        self.assertEqual(run.code, semgrep_gate.EXIT_UNUSABLE)
        self.assertIn("semgrep gate:", run.err)
        self.assertEqual(run.out, "")

    def test_a_file_that_is_not_json_is_unusable(self):
        self.check_unusable(Run("this is not json", raw=True))

    def test_an_empty_file_is_unusable(self):
        self.check_unusable(Run("", raw=True))

    def test_a_json_list_is_unusable(self):
        self.check_unusable(Run([]))

    def test_an_object_without_results_is_unusable(self):
        self.check_unusable(Run({"paths": {"scanned": ["a"]}}))

    def test_a_report_that_scanned_nothing_is_unusable_and_not_green(self):
        self.check_unusable(Run(report(scanned=())))

    def test_a_report_with_no_paths_is_unusable(self):
        self.check_unusable(Run({"results": [], "errors": []}))

    def test_a_missing_file_is_unusable(self):
        err = io.StringIO()
        with contextlib.redirect_stderr(err):
            code = semgrep_gate.main(["no-such-report.json"])
        self.assertEqual(code, semgrep_gate.EXIT_UNUSABLE)
        self.assertIn("cannot read", err.getvalue())

    def test_a_finding_that_is_not_an_object_is_unusable(self):
        self.check_unusable(Run(report(["a string"])))


class CanaryTests(unittest.TestCase):
    def test_a_canary_that_is_blocked_proves_the_gate(self):
        run = Run(report([finding("ERROR")]), "--canary")
        self.assertEqual(run.code, semgrep_gate.EXIT_PASSES)
        self.assertIn("can block", run.out)

    def test_a_canary_that_is_not_blocked_fails_the_gate(self):
        run = Run(report([finding("INFO")]), "--canary")
        self.assertEqual(run.code, semgrep_gate.EXIT_CANARY_PASSED)
        self.assertIn("NOT blocked", run.out)

    def test_a_canary_that_found_nothing_fails_the_gate(self):
        self.assertEqual(Run(report(), "--canary").code, semgrep_gate.EXIT_CANARY_PASSED)

    def test_a_canary_report_that_is_unusable_stays_unusable(self):
        self.assertEqual(Run("nope", "--canary", raw=True).code, semgrep_gate.EXIT_UNUSABLE)


class OutputTests(unittest.TestCase):
    def test_passing_findings_are_grouped_by_rule_with_counts(self):
        findings = [finding("INFO", rule="r.one", path=f"f{n % 3}.rs") for n in range(7)]
        findings.append(finding("WARNING", rule="r.two"))
        run = Run(report(findings))
        self.assertIn("7 x [INFO] r.one in 3 file(s)", run.out)
        self.assertIn("1 x [WARNING] r.two in 1 file(s)", run.out)
        self.assertEqual(run.out.count("r.one"), 1)

    def test_a_long_message_is_cut_and_a_multiline_one_is_one_line(self):
        message = "word " * 200 + "\n" + "tail"
        run = Run(report([finding("ERROR", message=message)]))
        shown = [line for line in run.out.splitlines() if line.startswith("      ")]
        self.assertEqual(len(shown), 1)
        self.assertLessEqual(len(shown[0].strip()), semgrep_gate.MESSAGE_LIMIT)

    def test_the_summary_counts_the_files_that_were_scanned(self):
        run = Run(report(scanned=["a", "b", "c"]))
        self.assertIn("3 file(s) scanned", run.out)

    def test_a_blocking_finding_names_its_file_and_line(self):
        run = Run(report([finding("ERROR", path="crates/x.rs", line=42)]))
        self.assertIn("crates/x.rs:42  [ERROR]", run.out)


if __name__ == "__main__":
    unittest.main()
