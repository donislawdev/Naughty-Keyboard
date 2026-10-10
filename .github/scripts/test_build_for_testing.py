"""Tests for build_for_testing.py, and guards over .github/workflows/test-build.yml.

Run from the root of the repository:

    python3 -m unittest discover -s .github/scripts -p "test_build_for_testing.py"

A test build goes to a person outside the project, so the failures guarded here
are the ones that would reach that person quietly: a build shared before CI
answered, two builds under one name, a link to a zip with the archive inside it,
a test build on the release page, or a workflow that builds differently from a
release while it looks the same. None of these needs gh or the network. What gh
would answer is made up.
"""
import json
import os
import re
import subprocess
import tempfile
import unittest
import unittest.mock

import build_for_testing as testing
import contents
import release
from test_release_ritual import RELEASE, WORKFLOWS, block, code_of, jobs, read, runs, steps

WORKSPACE = contents.Workspace("0.1.0", "https://github.com/example/project", "GPL-3.0-only")
TEST_BUILD = os.path.join(WORKFLOWS, "test-build.yml")
SCRIPT = os.path.join(release.ROOT, ".github", "scripts", "build_for_testing.py")
SHA = "0123456789abcdef0123456789abcdef01234567"


def setUpModule():
    # What the steps say as they go is for the log of a run, not for this one.
    unittest.mock.patch.object(testing, "say", lambda text: None).start()


def tearDownModule():
    unittest.mock.patch.stopall()


def gh_answering(*pages):
    """A stand-in for subprocess.run that answers each call with the next page,
    as JSON, and keeps what it was asked."""
    asked = []

    def run(command, **_):
        asked.append(command)
        page = pages[min(len(asked), len(pages)) - 1]
        return subprocess.CompletedProcess(command, 0, json.dumps(page), "")

    run.asked = asked
    return run


def ci_run(status="completed", conclusion="success", created="2026-10-10T10:00:00Z",
           event="pull_request", url="https://github.com/o/r/actions/runs/1"):
    return {"status": status, "conclusion": conclusion, "event": event, "headBranch": "feature/fix",
            "createdAt": created, "url": url}


class Clock:
    """Time that moves only when the code under test sleeps."""

    def __init__(self):
        self.now = 0.0
        self.slept = 0

    def __call__(self):
        return self.now

    def sleep(self, seconds):
        self.slept += 1
        self.now += seconds


class Plan(unittest.TestCase):
    def test_the_tag_says_test_and_never_dev(self):
        # release.yml names a run by hand -dev.<its run number>, and each
        # workflow counts its own runs, so -dev here would give two different
        # builds one name.
        self.assertEqual(testing.tag_of(7, WORKSPACE), "v0.1.0-test.7")
        self.assertEqual(contents.version_of_tag(testing.tag_of(7, WORKSPACE), WORKSPACE), "0.1.0-test.7")

    def test_a_run_number_that_is_not_a_number_is_refused(self):
        for number in ("7 && true", "", "7.1", "-1"):
            with self.subTest(number=number):
                with self.assertRaises(contents.Refused):
                    testing.tag_of(number, WORKSPACE)

    def test_all_builds_every_archive_and_a_system_only_its_own(self):
        archives = contents.load_archives()
        self.assertEqual(testing.chosen(testing.ALL, archives), archives)
        for system in testing.systems_of(archives)[1:]:
            with self.subTest(system=system):
                picked = testing.chosen(system, archives)
                self.assertEqual({a.os for a in picked}, {system})
                self.assertEqual(sorted(a.program for a in picked), sorted({a.program for a in archives}))

    def test_a_system_archives_toml_does_not_list_is_refused(self):
        for system in ("freebsd", "", "Windows", "windows "):
            with self.subTest(system=system):
                with self.assertRaises(contents.Refused):
                    testing.chosen(system, contents.load_archives())

    def test_the_plan_gives_the_matrix_and_the_names_of_one_system(self):
        version = contents.load_workspace().version
        with tempfile.TemporaryDirectory() as folder:
            output = os.path.join(folder, "output")
            self.assertEqual(testing.main(["plan", "--run-number", "3", "--system", "linux", "--output", output]), 0)
            with open(output, encoding="utf-8") as handle:
                found = dict(line.split("=", 1) for line in handle.read().splitlines())
        self.assertEqual(found["tag"], "v%s-test.3" % version)
        self.assertEqual([p["os"] for p in json.loads(found["platforms"])], ["linux"])
        self.assertEqual(json.loads(found["archives"]),
                         [{"os": "linux", "name": "%s_%s-test.3_linux_amd64.tar.gz" % (program, version)}
                          for program in ("nkb", "nkb-gui")])


class Tested(unittest.TestCase):
    def wait(self, *pages, ref="feature/fix"):
        clock = Clock()
        run = gh_answering(*pages)
        address = testing.wait_for_ci(SHA, ref, run=run, sleep=clock.sleep, clock=clock)
        return address, clock, run

    def test_ci_is_asked_about_by_its_file_on_this_commit(self):
        _, _, run = self.wait([ci_run()])
        command = run.asked[0]
        self.assertEqual(command[:3], ["gh", "run", "list"])
        self.assertEqual(command[command.index("--workflow") + 1], "ci.yml")
        self.assertEqual(command[command.index("--commit") + 1], SHA)

    def test_a_pull_request_counts_and_the_newest_run_decides(self):
        older = ci_run(conclusion="failure", created="2026-10-10T09:00:00Z", url="old")
        newer = ci_run(conclusion="success", created="2026-10-10T10:00:00Z", url="new")
        self.assertEqual(self.wait([older, newer])[0], "new")
        with self.assertRaises(contents.Refused):
            self.wait([ci_run(conclusion="success", created="2026-10-10T09:00:00Z"),
                       ci_run(conclusion="failure", created="2026-10-10T10:00:00Z")])

    def test_a_run_still_going_is_waited_for(self):
        address, clock, _ = self.wait([ci_run(status="in_progress", conclusion="")], [ci_run(url="done")])
        self.assertEqual(address, "done")
        self.assertEqual(clock.slept, 1)

    def test_a_run_that_did_not_pass_is_refused(self):
        for conclusion in ("failure", "cancelled", "skipped", "timed_out", ""):
            with self.subTest(conclusion=conclusion):
                with self.assertRaises(contents.Refused) as refused:
                    self.wait([ci_run(conclusion=conclusion)])
                self.assertIn("CI passed", refused.exception.problems[0])

    def test_a_commit_ci_never_ran_on_is_waited_for_a_while_and_then_refused(self):
        # A run pushed a moment ago may not be listed yet, so not at once. A
        # branch nothing started CI on would otherwise wait the whole limit.
        clock = Clock()
        with self.assertRaises(contents.Refused) as refused:
            testing.wait_for_ci(SHA, "feature/fix", run=gh_answering([]), sleep=clock.sleep, clock=clock)
        self.assertGreater(clock.now, testing.GRACE_SECONDS)
        self.assertLess(clock.now, testing.GRACE_SECONDS + 2 * testing.POLL_SECONDS)
        self.assertIn("gh workflow run ci.yml --ref feature/fix", refused.exception.problems[0])

    def test_a_run_listed_late_is_still_heard(self):
        address, clock, _ = self.wait([], [], [ci_run(url="late")])
        self.assertEqual(address, "late")
        self.assertEqual(clock.slept, 2)

    def test_a_run_still_going_at_the_limit_is_refused(self):
        clock = Clock()
        with self.assertRaises(contents.Refused) as refused:
            testing.wait_for_ci(SHA, run=gh_answering([ci_run(status="queued", conclusion="")]),
                                sleep=clock.sleep, clock=clock)
        self.assertGreater(clock.now, testing.WAIT_SECONDS)
        self.assertIn("still running", refused.exception.problems[0])


ARCHIVE = "nkb_0.1.0-test.7_windows_amd64.zip"
PALETTE = "nkb-gui_0.1.0-test.7_windows_amd64.zip"


def artifact(number, name, expired=False):
    return {"id": number, "name": name, "size_in_bytes": 4200000, "expired": expired,
            "digest": "sha256:" + "ab" * 32, "expires_at": "2026-11-09T10:00:00Z"}


class Links(unittest.TestCase):
    wanted = [("windows", ARCHIVE), ("windows", PALETTE)]

    def test_each_archive_gets_the_address_of_its_own_artifact_and_nothing_else_is_listed(self):
        held = [artifact(11, ARCHIVE), artifact(12, "unchecked-build-windows"), artifact(13, "notices"),
                artifact(14, PALETTE)]
        links, missing = testing.links_of(self.wanted, held, "o/r", 99)
        self.assertEqual(missing, [])
        self.assertEqual([(l.name, l.address) for l in links],
                         [(ARCHIVE, "https://github.com/o/r/actions/runs/99/artifacts/11"),
                          (PALETTE, "https://github.com/o/r/actions/runs/99/artifacts/14")])
        self.assertEqual(links[0].digest, "ab" * 32)
        self.assertEqual(links[0].until, "2026-11-09")

    def test_an_archive_the_run_does_not_hold_or_no_longer_holds_is_missing(self):
        links, missing = testing.links_of(self.wanted, [artifact(11, ARCHIVE, expired=True)], "o/r", 99)
        self.assertEqual(links, [])
        self.assertEqual(missing, [ARCHIVE, PALETTE])

    def test_the_listing_asks_this_run_and_refuses_a_part_of_it(self):
        run = gh_answering({"total_count": 1, "artifacts": [artifact(11, ARCHIVE)]})
        self.assertEqual(len(testing.listed_artifacts("o/r", 99, run)), 1)
        self.assertEqual(run.asked[0], ["gh", "api", "repos/o/r/actions/runs/99/artifacts?per_page=100"])
        with self.assertRaises(contents.Refused):
            testing.listed_artifacts("o/r", 99, gh_answering({"total_count": 101, "artifacts": [artifact(11, ARCHIVE)]}))

    def links(self, *names):
        held = [artifact(index, name) for index, name in enumerate(names)]
        wanted = [(name.split("_")[2], name) for name in names]
        return testing.links_of(wanted, held, "o/r", 99)[0]

    def test_the_words_to_send_cover_only_the_systems_built(self):
        text = testing.message(self.links(ARCHIVE), SHA, "https://github.com/o/r/tree/" + SHA)
        self.assertIn("SmartScreen", text)
        self.assertIn("Smart App Control", text)
        self.assertNotIn("xattr", text)
        self.assertNotIn("Linux:", text)
        both = testing.message(self.links("nkb_0.1.0-test.7_macos_arm64.tar.gz", "nkb_0.1.0-test.7_linux_amd64.tar.gz"),
                               SHA, "source")
        self.assertIn("xattr -dr com.apple.quarantine", both)
        self.assertIn("Linux:", both)
        self.assertNotIn("SmartScreen", both)

    def test_the_words_to_send_say_unsigned_who_can_download_and_until_when(self):
        text = testing.message(self.links(ARCHIVE), SHA, "https://github.com/o/r/tree/" + SHA)
        for words in ("not a release", "not signed", "need a GitHub account", "until 2026-11-09",
                      "https://github.com/o/r/actions/runs/99/artifacts/0", "https://github.com/o/r/tree/" + SHA,
                      SHA[:12]):
            with self.subTest(words=words):
                self.assertIn(words, text)

    def test_every_system_archives_toml_lists_has_its_words(self):
        self.assertEqual(sorted(testing.UNSIGNED), sorted(testing.systems_of(contents.load_archives())[1:]))

    def test_a_pull_request_run_says_its_files_are_not_for_sending(self):
        links = self.links(ARCHIVE)
        self.assertIn("not for sending", testing.summary("v0.1.0-test.7", "0.1.0", SHA, "ci", links, [], "o/r", True))
        self.assertNotIn("not for sending", testing.summary("v0.1.0-test.7", "0.1.0", SHA, "ci", links, [], "o/r", False))

    def test_the_summary_says_what_the_programs_print(self):
        text = testing.summary("v0.1.0-test.7", "0.1.0", SHA, "https://ci", self.links(ARCHIVE), [], "o/r", False)
        self.assertIn("print 0.1.0", text)
        self.assertIn("[CI passed](https://ci)", text)

    def test_a_missing_archive_is_named_the_others_keep_their_links_and_the_job_fails(self):
        version = contents.load_workspace().version
        names = [a.name(version + "-test.7") for a in testing.chosen("windows", contents.load_archives())]
        held = {"total_count": 1, "artifacts": [artifact(11, names[0])]}
        with tempfile.TemporaryDirectory() as folder:
            written = os.path.join(folder, "summary.md")
            with unittest.mock.patch.object(release, "gh", lambda arguments, run=None: subprocess.CompletedProcess(
                    arguments, 0, json.dumps(held), "")), unittest.mock.patch("sys.stderr"):
                code = testing.main(["links", "--tag", "v%s-test.7" % version, "--system", "windows",
                                     "--run-id", "99", "--sha", SHA, "--repo", "o/r", "--summary", written])
            with open(written, encoding="utf-8") as handle:
                text = handle.read()
        self.assertEqual(code, 1)
        self.assertIn("/actions/runs/99/artifacts/11", text)
        self.assertIn("Not shared", text)
        self.assertIn(names[1], text)


class Workflow(unittest.TestCase):
    def setUp(self):
        self.text = read(TEST_BUILD)
        self.jobs = jobs(self.text)
        self.release = jobs(read(RELEASE))

    def test_only_a_person_or_a_pull_request_that_changes_it_starts_it(self):
        on = block(self.text, "on:", 0)
        self.assertEqual(re.findall(r"(?m)^  ([a-z_]+):", on), ["workflow_dispatch", "pull_request"])
        pull = block(on, "pull_request:", 2)
        for path in (".github/workflows/test-build.yml", ".github/scripts/build_for_testing.py"):
            self.assertIn('"%s"' % path, pull)
        self.assertNotIn("pull_request_target", self.text)

    def test_the_form_offers_the_systems_of_archives_toml(self):
        system = block(block(block(self.text, "on:", 0), "inputs:", 4), "system:", 6)
        options = re.search(r"(?m)^        options: \[(.*)\]$", system).group(1)
        self.assertEqual([o.strip() for o in options.split(",")], testing.systems_of(contents.load_archives()))
        self.assertIn("default: all", system)
        self.assertIn("type: choice", system)
        self.assertEqual(testing.ALL, "all")

    def test_it_only_reads_and_only_two_jobs_may_read_the_runs(self):
        self.assertEqual(block(self.text, "permissions:", 0).strip(), "contents: read")
        for forbidden in (": write", "id-token", "attestations", "secrets."):
            self.assertNotIn(forbidden, self.text)
        readers = {name for name, job in self.jobs.items() if "actions: read" in job}
        self.assertEqual(readers, {"tested", "links"})
        for name in readers:
            granted = [line.split("#")[0].strip() for line in block(self.jobs[name], "permissions:", 4).splitlines()]
            self.assertEqual(sorted(g for g in granted if g), ["actions: read", "contents: read"])

    def test_no_expression_reaches_a_shell_and_every_action_is_pinned(self):
        for script in runs(self.text):
            self.assertNotIn("${{", script)
        for line in re.findall(r"uses: (\S+)", self.text):
            if not line.startswith("./"):
                self.assertRegex(line, r"@[0-9a-f]{40}$")
        for step in (s for job in self.jobs.values() for s in steps(job)):
            if "uses: actions/checkout@" in step:
                self.assertIn("persist-credentials: false", step)
                # The commit of the run and no other, so nothing a run was
                # started with can choose what is checked out.
                self.assertNotIn("ref:", step)

    def test_nothing_is_built_from_a_cache_or_with_flags_from_the_environment(self):
        self.assertNotIn("RUSTFLAGS", self.text)
        self.assertNotRegex(self.text, r"uses: (Swatinem/rust-cache|actions/cache)")
        for step in (s for job in self.jobs.values() for s in steps(job)):
            if "uses: ./.github/actions/setup-rust" in step:
                self.assertIn('cache: "false"', step)

    def test_it_builds_packs_unpacks_and_runs_with_the_commands_of_a_release(self):
        # Word for word, and the same actions in the same order, so the program a
        # person tries is the program a release of that commit would hold.
        # The notices job keeps no bill of materials, which a release attests and
        # a test build does not, so of it only the commands are held.
        for name in ("contents", "build"):
            with self.subTest(job=name):
                self.assertEqual(runs(self.jobs[name]), runs(self.release[name]))
        self.assertEqual(re.findall(r"uses: (\S+)", self.jobs["build"]),
                         re.findall(r"uses: (\S+)", self.release["build"]))
        self.assertGreaterEqual(len(runs(self.jobs["build"])), 4)

    def test_the_systems_and_the_archives_come_from_the_plan(self):
        build, share = self.jobs["build"], self.jobs["share"]
        self.assertIn("include: ${{ fromJSON(needs.plan.outputs.platforms) }}", build)
        self.assertIn("runs-on: ${{ matrix.runner }}", build)
        self.assertIn("include: ${{ fromJSON(needs.plan.outputs.archives) }}", share)
        for job in (build, share):
            self.assertIn("fail-fast: false", job)
        for runner in ("windows-latest", "macos-latest", "ubuntu-22.04"):
            self.assertNotIn(runner, build)

    def test_the_build_is_kept_a_day_under_a_name_that_says_it_is_unchecked(self):
        upload = next(s for s in steps(self.jobs["build"]) if "uses: actions/upload-artifact@" in s)
        self.assertIn("name: unchecked-build-${{ matrix.os }}", upload)
        self.assertIn("retention-days: 1", upload)
        # Not the names release.yml gives, which phase B asks for.
        self.assertNotRegex(self.text, r"name: (build-|unsigned-build-)")

    def test_nothing_is_shared_before_ci_passed_on_the_commit(self):
        for name in ("share", "links"):
            with self.subTest(job=name):
                job = self.jobs[name]
                self.assertIn("tested", re.search(r"(?m)^    needs: \[(.*)\]$", job).group(1).split(", "))
                self.assertEqual(re.findall(r"(?m)^    if: (.*)$", job),
                                 ["${{ !cancelled() && needs.plan.result == 'success' && "
                                  "needs.tested.result == 'success' }}"])
        self.assertIn("build_for_testing.py tested", "\n".join(runs(self.jobs["tested"])))
        lasting = [name for name, job in self.jobs.items() if "retention-days: 30" in job]
        self.assertEqual(lasting, ["share"])

    def test_ci_is_asked_about_the_commit_it_files_its_runs_under(self):
        ask = next(s for s in steps(self.jobs["tested"]) if "build_for_testing.py tested" in s)
        self.assertIn("SHA: ${{ github.event.pull_request.head.sha || github.sha }}", ask)

    def test_each_archive_is_shared_as_itself_and_not_inside_a_zip(self):
        upload = next(s for s in steps(self.jobs["share"]) if "uses: actions/upload-artifact@" in s)
        for words in ("path: dist/${{ matrix.name }}", "archive: false", "retention-days: 30"):
            self.assertIn(words, upload)
        self.assertNotRegex(upload, r"(?m)^\s+name:")

    def test_the_links_name_the_commit_built_and_the_run_of_ci(self):
        write = next(s for s in steps(self.jobs["links"]) if "build_for_testing.py links" in s)
        self.assertIn("SHA: ${{ github.sha }}", write)
        self.assertIn("CI_RUN: ${{ needs.tested.outputs.ci-run }}", write)
        self.assertIn("ci-run: ${{ steps.ci.outputs.ci-run }}", self.jobs["tested"])

    def test_nothing_reaches_a_release_and_gh_is_only_asked(self):
        for forbidden in ("gh release", "release create", "softprops/action-gh-release", "actions/attest"):
            self.assertNotIn(forbidden, self.text)
        code = code_of(SCRIPT)
        for forbidden in ('"release"', '"upload"', '"delete"', '"-X"', '"--method"', "--draft"):
            self.assertNotIn(forbidden, code)
        self.assertEqual(re.findall(r'release\.gh\(\["(\w+)"', code), ["api"])


if __name__ == "__main__":
    unittest.main()
