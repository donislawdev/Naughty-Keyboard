"""Tests for shape_gate.py. They need nothing but the standard library, and no cargo.

    python3 -m unittest discover -s .github/scripts -p "test_*.py"

Every case that decides a merge has a test that shows the deciding. A gate that
can only be seen passing is a gate nobody has seen work. Clippy itself is not
run here: the runs are replaced by the findings they would have produced, so
what is tested is what the gate makes of them.
"""

import contextlib
import io
import json
import os
import tempfile
import unittest
from unittest import mock

import shape_gate
from shape_gate import AXES, Finding, Unusable

LINES, COMPLEXITY, ARGUMENTS, NESTING = AXES
KEYS = [axis.key for axis in AXES]
MEMBERS = ("crates/one", "crates/two")

CLIPPY_TOML = """# A comment that stays.
too-many-lines-threshold = 400
cognitive-complexity-threshold = 70
too-many-arguments-threshold = 7
excessive-nesting-threshold = 10
"""


def message(lint, text, path="crates/one/src/lib.rs", line=10, source="fn some_function() {"):
    return json.dumps(
        {
            "reason": "compiler-message",
            "manifest_path": "C:/repo/crates/one/Cargo.toml",
            "message": {
                "level": "warning",
                "message": text,
                "code": {"code": f"clippy::{lint}"},
                "spans": [
                    {
                        "file_name": path,
                        "line_start": line,
                        "is_primary": True,
                        "text": [{"text": source}],
                    }
                ],
            },
        }
    )


def artifact(member):
    return json.dumps({"reason": "compiler-artifact", "manifest_path": f"C:\\repo\\{member}\\Cargo.toml"})


def every_member():
    return [artifact(member) for member in MEMBERS]


ASKED = {LINES.key: 114, COMPLEXITY.key: 52, ARGUMENTS.key: 6, NESTING.key: 9}


def found(lint, value, line=1, name="f"):
    return Finding(lint=lint, path="crates/one/src/lib.rs", line=line, value=value, name=name)


def at_the_measurement():
    """Findings that match ceilings 160/70/7/10 and the counts in NEAR exactly."""
    return [
        found("too_many_lines", 160, 1),
        found("too_many_lines", 120, 2),
        found("cognitive_complexity", 70, 3),
        found("too_many_arguments", 7, 4),
        found("excessive_nesting", None, 5),
    ]


CEILINGS = {LINES.key: 160, COMPLEXITY.key: 70, ARGUMENTS.key: 7, NESTING.key: 10}
NEAR = {LINES.key: 2, COMPLEXITY.key: 1, ARGUMENTS.key: 1, NESTING.key: 1}


class ThresholdTests(unittest.TestCase):
    def test_every_threshold_is_read(self):
        self.assertEqual(
            shape_gate.read_thresholds(CLIPPY_TOML, "clippy.toml"),
            {LINES.key: 400, COMPLEXITY.key: 70, ARGUMENTS.key: 7, NESTING.key: 10},
        )

    def test_a_threshold_the_gate_does_not_measure_is_refused(self):
        with self.assertRaisesRegex(Unusable, "type-complexity-threshold"):
            shape_gate.read_thresholds(CLIPPY_TOML + "type-complexity-threshold = 250\n", "x")

    def test_an_axis_without_a_threshold_is_refused(self):
        text = CLIPPY_TOML.replace("excessive-nesting-threshold = 10\n", "")
        with self.assertRaisesRegex(Unusable, "excessive-nesting-threshold"):
            shape_gate.read_thresholds(text, "x")

    def test_the_derived_configuration_changes_the_numbers_and_keeps_the_rest(self):
        derived = shape_gate.with_thresholds(CLIPPY_TOML + "msrv = \"1.98\"\n", ASKED)
        self.assertIn("# A comment that stays.", derived)
        self.assertIn('msrv = "1.98"', derived)
        self.assertEqual(shape_gate.read_thresholds(derived, "derived"), ASKED)

    def test_the_edge_of_the_band_is_exact_and_not_a_float_a_hair_above_it(self):
        # 100 * 0.55 is a hair above 55 in floating point, so its ceiling is 56.
        self.assertEqual(shape_gate.near_edge(LINES, 100, 0.55), 55)
        self.assertEqual(shape_gate.near_edge(LINES, 100, 0.7), 70)
        self.assertEqual(shape_gate.band_threshold(LINES, 100, 0.7), 69)
        self.assertEqual(shape_gate.near_edge(LINES, 163, 0.7), 115)
        self.assertEqual(shape_gate.near_edge(COMPLEXITY, 76, 0.7), 54)

    def test_the_small_axes_are_near_only_at_the_ceiling(self):
        self.assertEqual(shape_gate.near_edge(ARGUMENTS, 7, 0.7), 7)
        self.assertEqual(shape_gate.band_threshold(ARGUMENTS, 7, 0.7), 6)
        self.assertEqual(shape_gate.band_threshold(NESTING, 10, 0.7), 9)


class SettingsTests(unittest.TestCase):
    def read(self, shape, clippy=CLIPPY_TOML):
        with tempfile.TemporaryDirectory() as root:
            os.makedirs(os.path.join(root, ".github"))
            with open(os.path.join(root, "clippy.toml"), "w", encoding="utf-8") as handle:
                handle.write(clippy)
            with open(os.path.join(root, ".github", "shape.toml"), "w", encoding="utf-8") as handle:
                handle.write(shape)
            with open(os.path.join(root, "Cargo.toml"), "w", encoding="utf-8") as handle:
                handle.write('[workspace]\nmembers = ["crates/one", "crates/two"]\n')
            return shape_gate.read_settings(root)

    @staticmethod
    def shape(product=(160, 60, 6, 7), band="0.7", near_all=(3, 2, 1, 3)):
        lines = [f"[clippy]\nband = {band}\n", "[clippy.product]"]
        lines += [f"{key} = {value}" for key, value in zip(KEYS, product)]
        lines.append("[clippy.near.product]")
        lines += [f"{key} = 1" for key in KEYS]
        lines.append("[clippy.near.all]")
        lines += [f"{key} = {value}" for key, value in zip(KEYS, near_all)]
        return "\n".join(lines) + "\n"

    def test_a_whole_configuration_is_read(self):
        settings = self.read(self.shape())
        self.assertEqual(settings.ceilings["product"][LINES.key], 160)
        self.assertEqual(settings.ceilings["all"][LINES.key], 400)
        self.assertEqual(settings.near["all"][COMPLEXITY.key], 2)
        self.assertEqual(settings.members, MEMBERS)

    def test_a_product_ceiling_above_the_ceiling_of_all_the_code_is_refused(self):
        with self.assertRaisesRegex(Unusable, "above the ceiling of all the code"):
            self.read(self.shape(product=(500, 60, 6, 7)))

    def test_a_band_that_is_not_a_fraction_is_refused(self):
        with self.assertRaisesRegex(Unusable, "band"):
            self.read(self.shape(band="1.5"))
        with self.assertRaisesRegex(Unusable, "band"):
            self.read(self.shape(band="70"))

    def test_a_scope_whose_counts_miss_an_axis_is_refused(self):
        text = self.shape().replace(f"{NESTING.key} = 1\n[clippy.near.all]", "[clippy.near.all]")
        with self.assertRaisesRegex(Unusable, "near.product"):
            self.read(text)

    def test_a_count_that_is_not_a_whole_number_is_refused(self):
        with self.assertRaisesRegex(Unusable, "whole number"):
            self.read(self.shape(near_all=(3, 2, 1, -1)))


class FindingsTests(unittest.TestCase):
    def test_a_number_and_the_function_name_are_read(self):
        lines = every_member() + [message("too_many_lines", "this function has too many lines (130/114)")]
        (finding,) = shape_gate.findings_from(lines, ASKED, MEMBERS)
        self.assertEqual((finding.lint, finding.value, finding.name), ("too_many_lines", 130, "some_function"))

    def test_one_place_reported_by_two_targets_is_one_finding(self):
        text = "this function has too many lines (130/114)"
        lines = every_member() + [message("too_many_lines", text), message("too_many_lines", text)]
        self.assertEqual(len(shape_gate.findings_from(lines, ASKED, MEMBERS)), 1)

    def test_a_backslash_path_is_shown_with_forward_slashes(self):
        lines = every_member() + [
            message("excessive_nesting", "this block is too nested", path="crates\\one\\src\\lib.rs")
        ]
        (finding,) = shape_gate.findings_from(lines, ASKED, MEMBERS)
        self.assertEqual(finding.path, "crates/one/src/lib.rs")
        self.assertIsNone(finding.value)

    def test_a_numbered_lint_without_its_numbers_is_not_used(self):
        lines = every_member() + [message("too_many_lines", "this function is long")]
        with self.assertRaisesRegex(Unusable, "wording changed"):
            shape_gate.findings_from(lines, ASKED, MEMBERS)

    def test_a_threshold_other_than_the_one_asked_means_another_configuration_was_read(self):
        lines = every_member() + [message("too_many_lines", "this function has too many lines (130/100)")]
        with self.assertRaisesRegex(Unusable, "another configuration"):
            shape_gate.findings_from(lines, ASKED, MEMBERS)

    def test_a_package_that_was_not_read_makes_the_answer_unusable(self):
        lines = [artifact("crates/one")]
        with self.assertRaisesRegex(Unusable, "crates/two"):
            shape_gate.findings_from(lines, ASKED, MEMBERS)

    def test_a_compile_error_is_unusable_and_quoted(self):
        error = json.dumps(
            {"reason": "compiler-message", "message": {"level": "error", "rendered": "error[E0425]: x", "spans": []}}
        )
        with self.assertRaisesRegex(Unusable, "E0425"):
            shape_gate.findings_from(every_member() + [error], ASKED, MEMBERS)

    def test_other_lints_and_other_lines_are_left_alone(self):
        lines = every_member() + [
            message("unwrap_used", "used `unwrap()` on a `Result` value"),
            "   Compiling something",
        ]
        self.assertEqual(shape_gate.findings_from(lines, ASKED, MEMBERS), [])


class JudgeTests(unittest.TestCase):
    def judge(self, findings, ceilings=CEILINGS, near=NEAR):
        return shape_gate.judge_near("product", ceilings, near, 0.7, findings)

    def test_numbers_that_are_the_measurement_pass(self):
        self.assertEqual(self.judge(at_the_measurement()), [])

    def test_a_function_over_its_ceiling_blocks(self):
        findings = at_the_measurement() + [found("cognitive_complexity", 71, 9)]
        problems = self.judge(findings, near={**NEAR, COMPLEXITY.key: 2})
        self.assertEqual(len(problems), 1)
        self.assertIn("over its ceiling at 71", problems[0])

    def test_a_ceiling_with_room_in_it_blocks(self):
        findings = [f for f in at_the_measurement() if f.value != 160] + [found("too_many_lines", 159, 7)]
        problems = self.judge(findings)
        self.assertTrue(any("has room in it, the highest is 159" in p for p in problems), problems)

    def test_an_axis_with_nothing_near_it_blocks_as_room(self):
        findings = [f for f in at_the_measurement() if f.lint != "too_many_arguments"]
        problems = self.judge(findings, near={**NEAR, ARGUMENTS.key: 0})
        self.assertEqual(len(problems), 1)
        self.assertIn("nothing reaches the edge of the band", problems[0])

    def test_one_more_near_a_ceiling_than_frozen_blocks(self):
        # The edge for 160 is 112: one below it is not near, the edge itself is.
        findings = at_the_measurement() + [found("too_many_lines", 111, 8)]
        self.assertEqual(self.judge(findings), [])
        findings = at_the_measurement() + [found("too_many_lines", 112, 8)]
        problems = self.judge(findings)
        self.assertEqual(len(problems), 1)
        self.assertIn("3 stand near the too_many_lines ceiling, frozen at 2", problems[0])

    def test_fewer_near_a_ceiling_than_frozen_blocks_until_the_count_comes_down(self):
        problems = self.judge(at_the_measurement(), near={**NEAR, LINES.key: 3})
        self.assertEqual(len(problems), 1)
        self.assertIn("lower the frozen count", problems[0])

    def test_nesting_with_no_block_at_the_ceiling_has_room(self):
        findings = [f for f in at_the_measurement() if f.lint != "excessive_nesting"]
        problems = self.judge(findings, near={**NEAR, NESTING.key: 0})
        self.assertEqual(len(problems), 1)
        self.assertIn("no block reaches it", problems[0])

    def test_a_run_at_the_ceilings_blocks_on_anything_it_reports(self):
        self.assertEqual(shape_gate.judge_over("product", []), [])
        problems = shape_gate.judge_over("product", [found("excessive_nesting", None, 4, "deep")])
        self.assertEqual(len(problems), 1)
        self.assertIn("excessive_nesting is over its ceiling in crates/one/src/lib.rs:4 deep", problems[0])


class MainTests(unittest.TestCase):
    """The whole gate, with each clippy run replaced by what it found."""

    def run_gate(self, runs, *flags):
        settings = shape_gate.Settings(
            ceilings={"product": CEILINGS, "all": CEILINGS},
            near={"product": NEAR, "all": NEAR},
            band=0.7,
            members=MEMBERS,
        )
        calls = []

        def fake_clippy(root, scope, text, thresholds, members, jobs):
            calls.append((scope, dict(thresholds)))
            outcome = runs[len(calls) - 1]
            if isinstance(outcome, Exception):
                raise outcome
            return outcome

        out = io.StringIO()
        with (
            mock.patch.object(shape_gate, "read_settings", return_value=settings),
            mock.patch.object(shape_gate, "run_clippy", side_effect=fake_clippy),
            mock.patch("builtins.open", mock.mock_open(read_data=CLIPPY_TOML)),
            contextlib.redirect_stdout(out),
        ):
            code = shape_gate.main(list(flags))
        return code, out.getvalue(), calls

    def test_without_pinned_only_the_product_is_asked_at_its_ceilings(self):
        code, said, calls = self.run_gate([[]])
        self.assertEqual(code, shape_gate.EXIT_PASSES, said)
        self.assertEqual(calls, [("product", CEILINGS)])

    def test_pinned_asks_the_product_at_its_ceilings_and_both_scopes_at_the_band(self):
        code, said, calls = self.run_gate([[], at_the_measurement(), at_the_measurement()], "--pinned")
        self.assertEqual(code, shape_gate.EXIT_PASSES, said)
        self.assertEqual([scope for scope, _ in calls], ["product", "product", "all"])
        self.assertEqual(calls[1][1][LINES.key], 111)

    def test_something_over_a_ceiling_blocks(self):
        over = [found("too_many_lines", 170, 1)]
        code, said, _ = self.run_gate([over])
        self.assertEqual(code, shape_gate.EXIT_BLOCKS)
        self.assertIn("BLOCKED", said)

    def test_a_problem_seen_by_two_runs_is_said_once(self):
        over = found("too_many_lines", 170, 1)
        band = at_the_measurement() + [over]
        code, said, _ = self.run_gate([[over], band, at_the_measurement()], "--pinned")
        self.assertEqual(code, shape_gate.EXIT_BLOCKS)
        self.assertEqual(said.count("over its ceiling at 170"), 1, said)

    def test_a_measurement_that_cannot_be_used_is_no_verdict_and_not_green(self):
        code, said, _ = self.run_gate([Unusable("clippy did not read crates/two")])
        self.assertEqual(code, shape_gate.EXIT_UNUSABLE)
        self.assertIn("NO VERDICT", said)


if __name__ == "__main__":
    unittest.main()
