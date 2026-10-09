"""Guards over the release workflow: what phase A must do and must never do.

Run from the root of the repository:

    python3 -m unittest discover -s .github/scripts -p "test_release_ritual.py"

Every one of these is about a failure that is quiet. A workflow that publishes an
unsigned program for a quarter of an hour, builds from a cache another run left
behind, builds both programs with each other's features, or asks whether "the
checks" were green rather than which ones, still runs and still looks finished.

The workflow is read as text, without a YAML library, because the job that runs
these tests has none. Each check reads the narrowest piece that carries the
property: the block of one job, the lines of one step, the commands of one run,
so that a word standing somewhere else in the file cannot answer for it.
"""
import os
import re
import unittest

import release

WORKFLOWS = os.path.join(release.ROOT, ".github", "workflows")
RELEASE = os.path.join(WORKFLOWS, "release.yml")
SCRIPT = os.path.join(release.ROOT, ".github", "scripts", "release.py")


def read(path):
    with open(path, encoding="utf-8") as handle:
        return handle.read()


def block(text, heading, indent):
    """The lines under `heading` at this indent, up to the next line at the same
    indent or less."""
    lines = text.splitlines()
    pattern = re.compile(r"^%s%s\s*$" % (" " * indent, re.escape(heading)))
    for start, line in enumerate(lines):
        if pattern.match(line):
            body = []
            for following in lines[start + 1:]:
                if following.strip() and len(following) - len(following.lstrip()) <= indent:
                    break
                body.append(following)
            return "\n".join(body)
    raise AssertionError("no %r at indent %d" % (heading, indent))


def jobs(text):
    names = re.findall(r"^  ([a-z][a-z0-9_-]*):\s*$", block(text, "jobs:", 0), re.MULTILINE)
    return {name: block(text, name + ":", 2) for name in names}


def steps(job):
    """Each step of a job as its own text, starting at its `- ` line."""
    parts = re.split(r"(?m)^      - ", job)
    return ["- " + part for part in parts[1:]]


def runs(text):
    """The shell of every `run:` in the text, block or single line."""
    found = []
    lines = text.splitlines()
    for index, line in enumerate(lines):
        match = re.match(r"^(\s*)run: ?(.*)$", line)
        if not match:
            continue
        indent, rest = len(match.group(1)), match.group(2)
        if rest.strip() in ("|", ">", "|-", ">-"):
            body = []
            for following in lines[index + 1:]:
                if following.strip() and len(following) - len(following.lstrip()) <= indent:
                    break
                body.append(following)
            found.append("\n".join(body))
        else:
            found.append(rest)
    return found


def code_of(path):
    """A Python file without its comments and docstrings, so that a sentence
    about a command cannot stand in for the command."""
    text = read(path)
    text = re.sub(r'(?s)"""(.*?)"""', '""', text)
    return "\n".join(line.split("#", 1)[0] if not line.lstrip().startswith(("'", '"')) else line
                     for line in text.splitlines())


class Triggers(unittest.TestCase):
    def setUp(self):
        self.text = read(RELEASE)
        self.on = block(self.text, "on:", 0)

    def test_a_tag_starts_a_release_and_a_person_can_start_a_rehearsal(self):
        self.assertIn('tags: ["v*"]', block(self.on, "push:", 2))
        self.assertIn("workflow_dispatch:", self.on)

    def test_a_pull_request_runs_it_only_when_it_changes_the_release(self):
        pull = block(self.on, "pull_request:", 2)
        self.assertIn("paths:", pull)
        for path in (".github/workflows/release.yml", ".github/scripts/release.py", ".cargo/**"):
            self.assertIn('"%s"' % path, pull)
        self.assertNotIn("pull_request_target", self.text)


class Permissions(unittest.TestCase):
    def setUp(self):
        self.text = read(RELEASE)
        self.jobs = jobs(self.text)

    def test_the_workflow_reads_and_only_the_named_jobs_may_write(self):
        self.assertEqual(block(self.text, "permissions:", 0).strip(), "contents: read")
        writers = {name for name, job in self.jobs.items() if ": write" in job}
        self.assertEqual(writers, {"handover", "draft"})
        self.assertNotIn("contents: write", self.jobs["handover"])
        draft = block(self.jobs["draft"], "permissions:", 4)
        granted = [line.split("#")[0].strip() for line in draft.splitlines()]
        self.assertEqual([line for line in granted if line], ["contents: write"])

    def test_a_pull_request_never_signs_a_statement_and_only_a_tag_opens_a_draft(self):
        # The condition of the job itself, at its indent, and not one of a step.
        self.assertEqual(re.findall(r"(?m)^    if: (.*)$", self.jobs["handover"]),
                         ["github.event_name != 'pull_request'"])
        self.assertEqual(re.findall(r"(?m)^    if: (.*)$", self.jobs["draft"]),
                         ["needs.check.outputs.release == 'true'"])
        self.assertEqual(re.findall(r"(?m)^    needs: (.*)$", self.jobs["draft"]), ["[check, handover]"])


class Commands(unittest.TestCase):
    def setUp(self):
        self.text = read(RELEASE)
        self.jobs = jobs(self.text)

    def test_no_expression_is_pasted_into_a_shell(self):
        # Template expansion happens before the shell reads the script, so a tag
        # with a quote in it would become code, in a job that can sign.
        for script in runs(self.text):
            self.assertNotIn("${{", script)

    def test_every_action_is_pinned_to_a_commit_and_every_checkout_forgets_its_token(self):
        for line in re.findall(r"uses: (\S+)", self.text):
            if not line.startswith("./"):
                self.assertRegex(line, r"@[0-9a-f]{40}$")
        for step in (s for job in self.jobs.values() for s in steps(job)):
            if "uses: actions/checkout@" in step:
                self.assertIn("persist-credentials: false", step)

    def test_nothing_is_built_from_a_cache_or_with_flags_from_the_environment(self):
        self.assertNotIn("RUSTFLAGS", self.text)
        self.assertNotRegex(self.text, r"uses: (Swatinem/rust-cache|actions/cache)")
        for step in (s for job in self.jobs.values() for s in steps(job)):
            if "uses: ./.github/actions/setup-rust" in step:
                self.assertIn('cache: "false"', step)

    def test_the_platforms_come_from_archives_toml_and_not_from_this_file(self):
        build = self.jobs["build"]
        self.assertIn("include: ${{ fromJSON(needs.check.outputs.platforms) }}", build)
        self.assertIn("runs-on: ${{ matrix.runner }}", build)
        self.assertIn("fail-fast: false", build)
        for runner in ("windows-latest", "macos-latest", "ubuntu-22.04"):
            self.assertNotIn(runner, build)

    def test_the_commit_is_checked_before_anything_is_built(self):
        check = self.jobs["check"]
        for command in ("release.py released", "release.py changelog", "release.py green"):
            step = next(s for s in steps(check) if command in s)
            self.assertIn("if: steps.plan.outputs.release == 'true'", step)
        advisories = next(s for s in steps(check) if "cargo-deny-action" in s)
        self.assertIn("command: check advisories", advisories)
        self.assertIn("if: steps.plan.outputs.release == 'true'", advisories)
        self.assertIn("check", re.search(r"needs: \[?([^\]\n]*)", self.jobs["contents"]).group(1))

    def test_the_build_is_run_from_its_archives_after_unpacking_with_system_tools(self):
        build = self.jobs["build"]
        order = [build.index(marker) for marker in
                 ("release.py build", "release.py pack", "Expand-Archive", "tar -xzf", "release.py try",
                  "name: build-${{ matrix.os }}")]
        self.assertEqual(order, sorted(order))


class Handover(unittest.TestCase):
    def setUp(self):
        self.jobs = jobs(read(RELEASE))
        self.handover = self.jobs["handover"]

    def test_it_lists_what_the_build_produced_and_never_the_checksums_a_person_reads(self):
        shell = "\n".join(runs(self.handover))
        self.assertIn("sha256sum -- * > build.sha256", shell)
        self.assertNotRegex(read(RELEASE), r">\s*verify-")

    def test_the_statement_is_provenance_of_that_list(self):
        attest = next(s for s in steps(self.handover) if "uses: actions/attest@" in s)
        self.assertIn("subject-checksums: incoming/build.sha256", attest)
        for other in ("sbom-path", "predicate", "subject-path"):
            self.assertNotIn(other, attest)
        shell = "\n".join(runs(self.handover))
        self.assertIn("incoming/build.provenance.sigstore.json", shell)
        self.assertLess(self.handover.index("release.py handover"), self.handover.index("sha256sum -- *"))
        self.assertLess(self.handover.index("sha256sum -- *"), self.handover.index("uses: actions/attest@"))

    def test_the_build_is_handed_over_under_the_name_phase_b_asks_for(self):
        upload = next(s for s in steps(self.handover) if "unsigned-build-" in s)
        self.assertIn("name: unsigned-build-${{ needs.check.outputs.tag }}", upload)
        self.assertIn("path: incoming/*", upload)
        self.assertIn("retention-days: 14", upload)
        # The name phase A gives is the name phase B asks for, and the statement
        # and the list travel under the names phase B checks.
        script = read(os.path.join(release.ROOT, ".github", "scripts", "sign_release.py"))
        self.assertIn('"unsigned-build-%s" % tag', script)
        self.assertIn('BUILD_BUNDLE = "build.provenance.sigstore.json"', script)
        self.assertIn("incoming/build.provenance.sigstore.json", "\n".join(runs(self.handover)))

    def test_the_end_of_phase_a_says_how_phase_b_is_run(self):
        last = steps(self.jobs["draft"])[-1]
        self.assertIn("python .github/scripts/sign_release.py $TAG --macos-host user@mac", last)


class NothingIsPublished(unittest.TestCase):
    def test_no_file_reaches_a_release_and_nothing_is_published(self):
        # An unsigned program on a release page is a program somebody downloads.
        for path in (RELEASE, SCRIPT):
            text = read(path) if path == RELEASE else code_of(path)
            for forbidden in ("release upload", "release edit", "gh release publish", "--draft=false",
                              "--generate-notes", "softprops/action-gh-release"):
                with self.subTest(path=os.path.basename(path), forbidden=forbidden):
                    self.assertNotIn(forbidden, text)

    def test_the_draft_is_opened_by_the_script_and_by_no_shell(self):
        for script in runs(read(RELEASE)):
            self.assertNotIn("gh release", script)
        self.assertIn('"--draft"', code_of(SCRIPT))


class RequiredWorkflows(unittest.TestCase):
    def test_every_workflow_a_release_waits_for_runs_on_every_push_to_main(self):
        # A workflow that does not run on a push to main never has a run on the
        # commit a tag points at, and a release would wait for it for good.
        self.assertEqual(release.REQUIRED_WORKFLOWS, ("ci.yml", "supply-chain.yml", "semgrep.yml",
                                                      "workflow-lint.yml"))
        for name in release.REQUIRED_WORKFLOWS:
            with self.subTest(workflow=name):
                on = block(read(os.path.join(WORKFLOWS, name)), "on:", 0)
                self.assertIn("branches: [main]", block(on, "push:", 2))

    def test_no_workflow_that_runs_on_main_is_left_out(self):
        # The reverse: a new workflow that checks every push to main is one the
        # release has to wait for too, or the release does not hear it.
        for name in sorted(os.listdir(WORKFLOWS)):
            if name == "release.yml" or not name.endswith(".yml"):
                continue
            on = block(read(os.path.join(WORKFLOWS, name)), "on:", 0)
            pushes = re.search(r"(?m)^  push:\s*$", on) and "branches: [main]" in block(on, "push:", 2)
            with self.subTest(workflow=name):
                self.assertEqual(bool(pushes), name in release.REQUIRED_WORKFLOWS)


if __name__ == "__main__":
    unittest.main()
