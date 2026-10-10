"""Guards over the website workflow: what it publishes, when, and who may write.

Run from the root of the repository:

    python3 -m unittest discover -s .github/scripts -p "test_site_workflow.py"

Each of these is about a failure that is quiet. A publish job that a fork's
branch called main can reach still runs and still looks finished, and so does
one that puts up pages the check never read, or one that publishes while CI is
red about the facts the pages are built from. The address it writes to is the
project's own, so a mistake here is a defaced website.

The workflow is read as text, the way test_release_ritual.py reads the release
workflows, and with its helpers.
"""
import os
import re
import unittest

from test_release_ritual import WORKFLOWS, block, jobs, read, runs, steps

SITE = os.path.join(WORKFLOWS, "site.yml")
PINNED = re.compile(r"^\s*(?:- )?uses: [^@\s]+@[0-9a-f]{40}(?:\s|$)")


def step_named(job, name):
    for step in steps(job):
        if re.search(r"(?m)^\s*(?:- )?name: %s\s*$" % re.escape(name), step):
            return step
    raise AssertionError("no step named %r" % name)


def condition(job):
    """The text of a job's `if`, folded or on one line."""
    match = re.search(r"(?m)^    if: (.*)$", job)
    if not match:
        raise AssertionError("the job has no condition")
    if match.group(1).strip() in (">-", ">", "|"):
        return block(job, "if: " + match.group(1).strip(), 4)
    return match.group(1)


class Triggers(unittest.TestCase):
    def setUp(self):
        self.on = block(read(SITE), "on:", 0)

    def test_ci_finishing_on_main_starts_it_and_a_push_never_does(self):
        # A push of its own would race CI, and would make a release wait for the
        # website (test_release_ritual.RequiredWorkflows).
        self.assertIsNone(re.search(r"(?m)^  push:", self.on))
        trigger = re.search(r"(?m)^  workflow_run:.*\n((?:    .*\n)+)", self.on + "\n")
        self.assertIsNotNone(trigger)
        self.assertIn('workflows: ["CI"]', trigger.group(1))
        self.assertIn("types: [completed]", trigger.group(1))
        self.assertIn("branches: [main]", trigger.group(1))

    def test_a_deployment_is_never_cancelled_halfway(self):
        text = read(SITE)
        self.assertIn("cancel-in-progress: ${{ github.event_name == 'pull_request' }}", text)


class Publishing(unittest.TestCase):
    def setUp(self):
        self.text = read(SITE)
        self.jobs = jobs(self.text)
        self.build = self.jobs["build"]
        self.publish = self.jobs["publish"]

    def test_only_ci_that_passed_on_a_push_to_this_repository_gets_built_and_published(self):
        # The branch filter matches a NAME, and a fork chooses its branch names.
        # Without the last two conditions a pull request from a fork's branch
        # called main, green in CI, would reach the job that publishes.
        guard = condition(self.build)
        for part in ("github.event_name != 'workflow_run'",
                     "github.event.workflow_run.conclusion == 'success'",
                     "github.event.workflow_run.event == 'push'",
                     "github.event.workflow_run.head_repository.full_name == github.repository"):
            with self.subTest(part=part):
                self.assertIn(part, guard)
        self.assertEqual(guard.count("&&"), 2)
        self.assertEqual(guard.count("||"), 1)

    def test_a_pull_request_or_another_branch_publishes_nothing(self):
        self.assertIn("  PUBLISH: ${{ github.event_name == 'workflow_run' || "
                      "(github.event_name == 'workflow_dispatch' && "
                      "github.ref == 'refs/heads/main') }}\n", self.text)
        self.assertIn("publish: ${{ env.PUBLISH }}", self.build)
        self.assertEqual(condition(self.publish), "needs.build.outputs.publish == 'true'")
        self.assertIn("needs: build", self.publish)

    def test_the_checkout_is_the_commit_ci_passed_on_and_forgets_its_token(self):
        checkout = step_named(self.build, "checkout")
        self.assertIn("ref: ${{ github.event.workflow_run.head_sha || github.ref }}", checkout)
        self.assertIn("persist-credentials: false", checkout)

    def test_the_pages_handed_over_are_the_ones_just_built_and_checked(self):
        names = [re.search(r"name: (.*)", step).group(1).strip() for step in steps(self.build)]
        self.assertEqual(names[-3:], ["build", "check every page", "hand the checked pages over"])
        build, check, hand = (step_named(self.build, name) for name in names[-3:])
        self.assertIn('--destination "$RUNNER_TEMP/public"', runs(build)[0])
        self.assertIn("--printI18nWarnings --panicOnWarning", runs(build)[0])
        self.assertIn('site_check.py "$RUNNER_TEMP/public"', runs(check)[0])
        self.assertIn("path: ${{ runner.temp }}/public", hand)
        self.assertIn("if: env.PUBLISH == 'true'", hand)
        self.assertIn("uses: actions/upload-pages-artifact@", hand)

    def test_hugo_is_held_to_its_digest(self):
        self.assertRegex(self.text, r'(?m)^  HUGO_SHA256: "[0-9a-f]{64}"$')
        install = runs(step_named(self.build, "install Hugo, held to its digest"))[0]
        self.assertIn('echo "${HUGO_SHA256}  $RUNNER_TEMP/hugo.tar.gz" | sha256sum --check --strict', install)


class Permissions(unittest.TestCase):
    def setUp(self):
        self.text = read(SITE)
        self.jobs = jobs(self.text)

    def test_the_workflow_reads_and_only_the_publish_job_may_write(self):
        self.assertEqual(block(self.text, "permissions:", 0).strip(), "contents: read")
        for name, job in self.jobs.items():
            with self.subTest(job=name):
                held = re.search(r"(?m)^    permissions:", job)
                self.assertEqual(bool(held), name == "publish")
        granted = [line.split("#", 1)[0].strip()
                   for line in block(self.jobs["publish"], "permissions:", 4).splitlines()]
        self.assertEqual(sorted(g for g in granted if g), ["id-token: write", "pages: write"])

    def test_the_publish_job_runs_nothing_from_the_repository(self):
        publish = self.jobs["publish"]
        self.assertEqual(runs(publish), [])
        self.assertNotIn("actions/checkout", publish)
        self.assertEqual(re.findall(r"uses: ([^@\s]+)@", publish), ["actions/deploy-pages"])
        self.assertIn("name: github-pages", block(publish, "environment:", 4))

    def test_every_action_is_pinned_to_a_commit(self):
        for line in self.text.splitlines():
            if re.match(r"^\s*(?:- )?uses:", line):
                with self.subTest(line=line.strip()):
                    self.assertRegex(line, PINNED)


if __name__ == "__main__":
    unittest.main()
