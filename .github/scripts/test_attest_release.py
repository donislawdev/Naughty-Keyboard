"""Tests for attest_release.py, phase C of the release.

Run from the root of the repository:

    python3 -m unittest discover -s .github/scripts -p "test_attest_release.py"

None of these needs gh or the network. What gh would answer is made up, and the
draft is a folder of made-up files. Each test is about a way the statement could
end up describing bytes nobody downloads, or not answering the commands a
person is told to run.
"""
import base64
import json
import os
import shutil
import subprocess
import tempfile
import types
import unittest
import unittest.mock

import attest_release
import contents
import release

TAG = "v0.1.0"
VERSION = "0.1.0"
ON_TAG = {"GITHUB_REF": "refs/tags/" + TAG}
SPDX = "https://spdx.dev/Document/v2.3"


def setUpModule():
    unittest.mock.patch.object(attest_release, "say", lambda text: None).start()


def tearDownModule():
    unittest.mock.patch.stopall()


def bundle(predicate, subjects):
    statement = {"_type": "https://in-toto.io/Statement/v1", "predicateType": predicate,
                 "subject": [{"name": n, "digest": {"sha256": d}} for n, d in sorted(subjects.items())]}
    payload = base64.b64encode(json.dumps(statement).encode()).decode()
    return json.dumps({"dsseEnvelope": {"payload": payload, "payloadType": "application/vnd.in-toto+json"}})


class Gh:
    """A stand-in for gh: a draft made of files in a folder, and a record of
    what was asked."""

    def __init__(self, draft, state="true\n", verifies=True):
        self.draft, self.state, self.verifies, self.asked = draft, state, verifies, []

    def __call__(self, command, **_):
        self.asked.append(command)
        words = command[1:]
        if words[:2] == ["release", "view"]:
            return subprocess.CompletedProcess(command, 0, self.state, "")
        if words[:2] == ["release", "download"]:
            target = words[words.index("--dir") + 1]
            shutil.copytree(self.draft, target)
            return subprocess.CompletedProcess(command, 0, "", "")
        if words[:2] == ["attestation", "verify"]:
            return subprocess.CompletedProcess(command, 0 if self.verifies else 1, "", "no attestation found")
        return subprocess.CompletedProcess(command, 0, "", "")

    def uploads(self):
        return [c for c in self.asked if c[1:3] == ["release", "upload"]]


class Draft(unittest.TestCase):
    def setUp(self):
        self.root = tempfile.mkdtemp(prefix="attest-test-")
        self.addCleanup(shutil.rmtree, self.root, ignore_errors=True)
        self.draft = os.path.join(self.root, "draft")
        os.makedirs(self.draft)
        for name in release.published_files(TAG):
            if name != release.SUMS:
                self.put(name, name.encode())
        self.sums()

    def put(self, name, data):
        with open(os.path.join(self.draft, name), "wb") as handle:
            handle.write(data)

    def sums(self, names=None):
        names = names or [n for n in release.published_files(TAG) if n != release.SUMS]
        self.put(release.SUMS, "".join("%s  %s\n" % (contents.sha256_of(os.path.join(self.draft, n)), n)
                                       for n in sorted(names)).encode())
        return contents.sha256_of(os.path.join(self.draft, release.SUMS))

    def fetch(self, gh, digest=None, environ=ON_TAG):
        args = types.SimpleNamespace(tag=TAG, digest=digest or self.sums(), dir=os.path.join(self.root, "signed"),
                                     subjects=os.path.join(self.root, "subjects.sha256"),
                                     output=os.path.join(self.root, "output"))
        return attest_release.command_fetch(args, environ, gh)


class Fetching(Draft):
    def test_the_draft_is_fetched_and_the_subjects_are_the_six_archives(self):
        self.assertEqual(self.fetch(Gh(self.draft)), 0)
        with open(os.path.join(self.root, "subjects.sha256"), encoding="utf-8") as handle:
            subjects = release.listed_digests(handle.read(), "subjects")
        archives = sorted(a.name(VERSION) for a in contents.load_archives())
        self.assertEqual(sorted(subjects), archives)
        self.assertFalse(any(n.startswith("verify-") for n in subjects))
        with open(os.path.join(self.root, "output"), encoding="utf-8") as handle:
            self.assertEqual(handle.read().strip(), "sbom=" + os.path.join(self.root, "signed", release.sbom_name(VERSION)))

    def test_a_run_not_on_the_tag_it_attests_is_refused_before_anything_is_fetched(self):
        for ref in ({"GITHUB_REF": "refs/heads/main"}, {}, {"GITHUB_REF": "refs/tags/v0.1.0-rc.1"}):
            with self.subTest(ref=ref):
                gh = Gh(self.draft)
                with self.assertRaises(contents.Refused):
                    self.fetch(gh, environ=ref)
                self.assertEqual(gh.asked, [])

    def test_a_digest_that_is_not_one_and_a_release_that_is_not_a_draft_are_refused(self):
        with self.assertRaises(contents.Refused):
            self.fetch(Gh(self.draft), digest="abc")
        for state in ("false\n",):
            gh = Gh(self.draft, state=state)
            with self.assertRaises(contents.Refused):
                self.fetch(gh)
            self.assertFalse(any(c[1:3] == ["release", "download"] for c in gh.asked))

    def test_checksums_other_than_the_ones_signed_are_refused(self):
        signed = self.sums()
        self.put("nkb_0.1.0_linux_amd64.tar.gz", b"changed on the way")
        self.sums()
        with self.assertRaises(contents.Refused) as refusal:
            self.fetch(Gh(self.draft), digest=signed)
        self.assertIn("hashes to", refusal.exception.problems[0])

    def test_a_file_that_is_not_the_one_listed_is_refused(self):
        digest = self.sums()
        self.put("nkb_0.1.0_linux_amd64.tar.gz", b"changed after the list")
        with self.assertRaises(contents.Refused) as refusal:
            self.fetch(Gh(self.draft), digest=digest)
        self.assertIn("not the file", refusal.exception.problems[0])

    def test_a_list_that_leaves_a_file_out_is_refused(self):
        names = [n for n in release.published_files(TAG) if n not in (release.SUMS, "nkb_0.1.0_linux_amd64.tar.gz")]
        digest = self.sums(names)
        with self.assertRaises(contents.Refused) as refusal:
            self.fetch(Gh(self.draft), digest=digest)
        self.assertIn("lists", refusal.exception.problems[0])

    def test_a_file_missing_or_one_too_many_is_refused(self):
        digest = self.sums()
        self.put("stray.zip", b"x")
        with self.assertRaises(contents.Refused) as refusal:
            self.fetch(Gh(self.draft), digest=digest)
        self.assertIn("stray.zip", refusal.exception.problems[0])
        os.remove(os.path.join(self.draft, "stray.zip"))
        os.remove(os.path.join(self.draft, "nkb_0.1.0_linux_amd64.tar.gz"))
        with self.assertRaises(contents.Refused) as refusal:
            self.fetch(Gh(self.draft), digest=digest)
        self.assertIn("phase B uploads", refusal.exception.problems[0])

    def test_a_statement_an_earlier_run_left_is_replaced_and_not_counted(self):
        self.put(release.sbom_bundle_name(VERSION), b"{}")
        self.assertEqual(self.fetch(Gh(self.draft)), 0)
        self.assertFalse(os.path.exists(os.path.join(self.root, "signed", release.sbom_bundle_name(VERSION))))


class Publishing(Draft):
    def setUp(self):
        super().setUp()
        self.fetch(Gh(self.draft))
        self.signed = os.path.join(self.root, "signed")
        self.archives = {a.name(VERSION): contents.sha256_of(os.path.join(self.signed, a.name(VERSION)))
                         for a in contents.load_archives()}

    def publish(self, gh, predicate=SPDX, subjects=None):
        made = os.path.join(self.root, "bundle.json")
        with open(made, "w", encoding="utf-8") as handle:
            handle.write(bundle(predicate, self.archives if subjects is None else subjects))
        args = types.SimpleNamespace(tag=TAG, dir=self.signed, bundle=made, out=os.path.join(self.root, "out"))
        return attest_release.command_publish(args, ON_TAG, gh)

    def test_the_statement_goes_on_the_draft_under_its_name_after_every_archive_verified(self):
        gh = Gh(self.draft)
        self.assertEqual(self.publish(gh), 0)
        checks = [c for c in gh.asked if c[1:3] == ["attestation", "verify"]]
        self.assertEqual(sorted(c[3] for c in checks), sorted(os.path.join(self.signed, n) for n in self.archives))
        for check in checks:
            self.assertEqual(check[check.index("--predicate-type") + 1], SPDX)
            self.assertIn("--bundle", check)
        upload = gh.uploads()
        self.assertEqual(len(upload), 1)
        self.assertEqual(os.path.basename(upload[0][4]), "verify-naughty-keyboard_0.1.0.sbom.sigstore.json")
        self.assertIn("--clobber", upload[0])
        self.assertLess(gh.asked.index(checks[-1]), gh.asked.index(upload[0]))

    def test_a_statement_of_another_type_than_readme_asks_for_is_never_uploaded(self):
        gh = Gh(self.draft)
        with self.assertRaises(contents.Refused) as refusal:
            self.publish(gh, predicate="https://slsa.dev/provenance/v1")
        self.assertIn("README.md", refusal.exception.problems[0])
        self.assertEqual(gh.uploads(), [])

    def test_a_statement_about_other_bytes_or_other_files_is_never_uploaded(self):
        other = dict(self.archives, **{"nkb_0.1.0_linux_amd64.tar.gz": "0" * 64})
        fewer = {n: d for n, d in self.archives.items() if "linux" not in n}
        more = dict(self.archives, **{release.SUMS: "0" * 64})
        for subjects in (other, fewer, more):
            with self.subTest(subjects=sorted(subjects)):
                gh = Gh(self.draft)
                with self.assertRaises(contents.Refused):
                    self.publish(gh, subjects=subjects)
                self.assertEqual(gh.uploads(), [])

    def test_an_archive_the_command_does_not_verify_is_named_and_nothing_is_uploaded(self):
        gh = Gh(self.draft, verifies=False)
        with self.assertRaises(contents.Refused) as refusal:
            self.publish(gh)
        self.assertEqual(len(refusal.exception.problems), 6)
        self.assertEqual(gh.uploads(), [])

    def test_a_release_published_meanwhile_gets_no_file(self):
        gh = Gh(self.draft, state="false\n")
        with self.assertRaises(contents.Refused):
            self.publish(gh)
        self.assertEqual(gh.uploads(), [])


if __name__ == "__main__":
    unittest.main()
