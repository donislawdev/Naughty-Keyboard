"""Tests for verify_release.py, phase D of the release.

Run from the root of the repository:

    python3 -m unittest discover -s .github/scripts -p "test_verify_release.py"

None of these needs gh, a Mac, PowerShell or the network. What they would answer
is made up, and the page is a folder of made-up files. Each test is about a way
a published page could look finished and not answer for itself, and a check
that would let it.
"""
import base64
import hashlib
import json
import os
import shutil
import subprocess
import tempfile
import types
import unittest
import unittest.mock

import contents
import release
import sign_release
import verify_release

TAG = "v0.1.0"
VERSION = "0.1.0"
REPOSITORY = "donislawdev/Naughty-Keyboard"
PIN = "a" * 64
MAC_PIN = hashlib.sha256(b"the certificate").hexdigest()


def setUpModule():
    unittest.mock.patch.object(verify_release, "say", lambda text: None).start()
    unittest.mock.patch.object(sign_release, "say", lambda text: None).start()


def tearDownModule():
    unittest.mock.patch.stopall()


def statement(subjects, predicate="https://slsa.dev/provenance/v1"):
    payload = {"predicateType": predicate,
               "subject": [{"name": n, "digest": {"sha256": d}} for n, d in sorted(subjects.items())]}
    return json.dumps({"dsseEnvelope": {"payload": base64.b64encode(json.dumps(payload).encode()).decode()}})


def answering(code=0, out="", err="", by=None):
    """A stand-in for subprocess.run that answers every command the same way,
    or by the first word that tells them apart, and keeps what it was asked."""
    asked = []

    def run(command, **kw):
        asked.append((command, kw))
        if by:
            for key, answer in by.items():
                if key in command:
                    return subprocess.CompletedProcess(command, *answer)
        return subprocess.CompletedProcess(command, code, out, err)

    run.asked = asked
    return run


class Page(unittest.TestCase):
    """A folder that holds a release page as it should be."""

    def setUp(self):
        self.root = tempfile.mkdtemp(prefix="verify-test-")
        self.addCleanup(shutil.rmtree, self.root, ignore_errors=True)
        archives = [a.name(VERSION) for a in contents.load_archives()]
        for name in archives + [release.sbom_name(VERSION), release.sbom_bundle_name(VERSION)]:
            self.put(name, name.encode())
        self.build = {n: (contents.sha256_of(self.path(n)) if "_linux_" in n or n.endswith(".spdx.json")
                          else "f" * 64) for n in archives + [release.sbom_name(VERSION)]}
        self.put(release.provenance_bundle_name(VERSION), statement(self.build).encode())
        listed = [n for n in release.published_files(TAG) if n != release.SUMS]
        self.put(release.SUMS, "".join("%s  %s\n" % (contents.sha256_of(self.path(n)), n) for n in listed).encode())

    def path(self, name):
        return os.path.join(self.root, name)

    def put(self, name, data):
        with open(self.path(name), "wb") as handle:
            handle.write(data)

    def view(self, **changes):
        assets = [{"name": n, "digest": "sha256:" + contents.sha256_of(self.path(n))} for n in release.page_files(TAG)]
        return dict({"assets": assets, "isDraft": False, "isImmutable": True, "isPrerelease": False}, **changes)


class TheRelease(Page):
    def test_a_finished_release_has_no_problem(self):
        self.assertEqual(verify_release.release_problems(self.view(), TAG), [])
        self.assertEqual(verify_release.files_problems(self.view(), self.root, TAG), [])

    def test_a_draft_a_mutable_release_and_an_unmarked_candidate_are_each_named(self):
        self.assertEqual(len(verify_release.release_problems(self.view(isDraft=True, isImmutable=False), TAG)), 2)
        candidate = self.view(assets=[dict(a, name=a["name"].replace(VERSION, "0.1.0-rc.1"))
                                      for a in self.view()["assets"]])
        self.assertEqual(len(verify_release.release_problems(candidate, "v0.1.0-rc.1")), 1)
        self.assertEqual(verify_release.release_problems(dict(candidate, isPrerelease=True), "v0.1.0-rc.1"), [])

    def test_a_file_missing_from_the_page_or_one_too_many_is_named(self):
        view = self.view()
        fewer = dict(view, assets=view["assets"][1:])
        more = dict(view, assets=view["assets"] + [{"name": "stray.zip"}])
        for changed in (fewer, more):
            self.assertTrue(verify_release.release_problems(changed, TAG))

    def test_a_download_that_is_not_what_github_lists_or_the_checksums_list_is_named(self):
        self.put("nkb_0.1.0_linux_amd64.tar.gz", b"other bytes")
        problems = verify_release.files_problems(self.view(), self.root, TAG)
        self.assertEqual(problems, ["nkb_0.1.0_linux_amd64.tar.gz is not the file verify-SHA256SUMS.txt lists"])
        view = self.view()
        view["assets"][0]["digest"] = "sha256:" + "0" * 64
        self.assertTrue(any("GitHub lists" in p for p in verify_release.files_problems(view, self.root, TAG)))

    def test_githubs_own_statement_has_to_name_every_file_with_its_digest(self):
        page = {n: contents.sha256_of(self.path(n)) for n in release.page_files(TAG)}
        answer = json.loads(statement(page))
        answer = {"verificationResult": {"statement": json.loads(base64.b64decode(answer["dsseEnvelope"]["payload"]))}}
        answer["verificationResult"]["statement"]["subject"].append({"uri": "pkg:github/x@v0.1.0",
                                                                     "digest": {"sha1": "0" * 40}})
        self.assertEqual(verify_release.immutable_problems(answer, self.root, TAG), [])
        answer["verificationResult"]["statement"]["subject"].pop(0)
        self.assertTrue(verify_release.immutable_problems(answer, self.root, TAG))


class Provenance(Page):
    def test_it_describes_the_linux_archives_and_the_bill_of_materials_and_no_signed_archive(self):
        self.assertEqual(verify_release.provenance_problems(self.root, TAG), [])

    def test_a_signed_archive_it_still_describes_was_never_signed(self):
        name = "nkb-gui_0.1.0_windows_amd64.zip"
        self.put(release.provenance_bundle_name(VERSION),
                 statement(dict(self.build, **{name: contents.sha256_of(self.path(name))})).encode())
        self.assertEqual(verify_release.provenance_problems(self.root, TAG),
                         ["%s is the file the build made, so it was never signed" % name])

    def test_a_linux_archive_or_a_bill_of_materials_it_does_not_describe_is_named(self):
        for name in ("nkb_0.1.0_linux_amd64.tar.gz", release.sbom_name(VERSION)):
            with self.subTest(name=name):
                self.put(release.provenance_bundle_name(VERSION), statement(dict(self.build, **{name: "0" * 64})).encode())
                self.assertEqual(len(verify_release.provenance_problems(self.root, TAG)), 1)


class Statements(Page):
    def test_every_archive_is_held_to_the_attesting_workflow_and_every_unsigned_file_to_the_build(self):
        run = answering()
        promise = release.load_promise()
        self.assertEqual(verify_release.statement_problems(self.root, TAG, promise, REPOSITORY, run), [])
        commands = [c for c, _ in run.asked]
        archives = [a.name(VERSION) for a in contents.load_archives()]
        for name in archives:
            mine = [c for c in commands if c[3] == name and "--predicate-type" in c]
            self.assertEqual(len(mine), 2, name)
            self.assertIn("verify-naughty-keyboard_0.1.0.sbom.sigstore.json", mine[0])
            self.assertEqual(mine[1][mine[1].index("--signer-workflow") + 1],
                             REPOSITORY + "/.github/workflows/attest-release.yml")
        built = [c for c in commands if "--predicate-type" not in c]
        self.assertEqual(sorted(c[3] for c in built), verify_release.unsigned_files(TAG))
        for command in built:
            self.assertEqual(command[command.index("--signer-workflow") + 1], REPOSITORY + "/.github/workflows/release.yml")
            self.assertEqual(command[command.index("--source-ref") + 1], "refs/tags/v0.1.0")
            self.assertIn("--deny-self-hosted-runners", command)
        self.assertTrue(all(kw.get("cwd") == self.root for _, kw in run.asked))

    def test_every_statement_that_does_not_answer_is_named_not_only_the_first(self):
        problems = verify_release.statement_problems(self.root, TAG, release.load_promise(), REPOSITORY,
                                                     answering(1, "", "no attestation found"))
        self.assertEqual(len(problems), 12 + 3)


class Commands(unittest.TestCase):
    def test_the_commands_of_readme_run_word_for_word_from_the_folder_and_without_a_shell(self):
        run = answering()
        promise = release.load_promise()
        self.assertEqual(verify_release.command_problems("here", TAG, promise, run), [])
        self.assertEqual([c for c, _ in run.asked], [line.split() for line in promise.rendered(TAG)])
        for _, kw in run.asked:
            self.assertEqual(kw.get("cwd"), "here")
            self.assertFalse(kw.get("shell"))

    def test_a_command_that_fails_is_named_with_what_it_said(self):
        problems = verify_release.command_problems("here", TAG, release.load_promise(),
                                                   answering(by={"verify-asset": (1, "", "no attestation")}))
        self.assertEqual(len(problems), 1)
        self.assertIn("no attestation", problems[0])


class Notes(unittest.TestCase):
    def setUp(self):
        self.promise = release.load_promise()
        with open(release.NOTES_FOOTER, encoding="utf-8") as handle:
            self.notes = release.release_notes("- A thing.", TAG, self.promise, handle.read())

    def test_the_notes_phase_a_writes_say_everything_this_asks_for(self):
        self.assertEqual(verify_release.notes_problems(self.notes, TAG, self.promise), [])

    def test_line_ends_from_the_web_editor_and_a_sentence_added_on_top_change_nothing(self):
        edited = "Withdrawn: use v0.1.1.\r\n\r\n" + self.notes.replace("\n", "\r\n")
        self.assertEqual(verify_release.notes_problems(edited, TAG, self.promise), [])

    def test_a_command_lost_from_the_notes_is_named_even_when_a_longer_one_contains_it(self):
        lines = self.promise.rendered(TAG)
        # The API command is the start of the offline one, so a check by
        # substring would find it in the line that is left.
        api = next(l for l in lines if "--predicate-type" in l and "--bundle" not in l)
        cut = self.notes.replace(api + "\n", "")
        self.assertEqual(verify_release.notes_problems(cut, TAG, self.promise),
                         ["the notes of v0.1.0 do not say: " + api])
        self.assertTrue(verify_release.notes_problems(self.notes.replace(release.SUMS, "the sums"), TAG, self.promise))
        self.assertEqual(len(verify_release.notes_problems("", TAG, self.promise)), len(lines) + 1)


class Latest(unittest.TestCase):
    def test_a_full_release_has_to_be_the_latest_and_a_pre_release_must_not_be(self):
        self.assertEqual(verify_release.latest_problems({"isPrerelease": False}, TAG, TAG), [])
        self.assertTrue(verify_release.latest_problems({"isPrerelease": False}, TAG, "v0.0.9"))
        self.assertTrue(verify_release.latest_problems({"isPrerelease": False}, TAG, ""))
        self.assertEqual(verify_release.latest_problems({"isPrerelease": True}, TAG, "v0.0.9"), [])
        self.assertTrue(verify_release.latest_problems({"isPrerelease": True}, TAG, TAG))


class Fetching(unittest.TestCase):
    def setUp(self):
        self.root = tempfile.mkdtemp(prefix="verify-fetch-")
        self.addCleanup(shutil.rmtree, self.root, ignore_errors=True)

    def gh(self, changed=None):
        names = [a.name(VERSION) for a in release.archives_for("windows")]

        def run(command, **_):
            run.asked.append(command)
            target = command[command.index("--dir") + 1]
            os.makedirs(target, exist_ok=True)
            for name in names:
                with open(os.path.join(target, name), "wb") as handle:
                    handle.write(name.encode())
            listed = {n: contents.sha256_of(os.path.join(target, n)) for n in names}
            if changed:
                with open(os.path.join(target, changed), "wb") as handle:
                    handle.write(b"other")
            with open(os.path.join(target, release.SUMS), "w", encoding="utf-8") as handle:
                handle.write("".join("%s  %s\n" % (d, n) for n, d in sorted(listed.items())))
            return subprocess.CompletedProcess(command, 0, "", "")

        run.asked = []
        return run

    def test_only_the_archives_of_one_system_and_the_checksums_are_fetched_by_exact_name(self):
        run = self.gh()
        args = types.SimpleNamespace(tag=TAG, os="windows", dir=os.path.join(self.root, "p"))
        self.assertEqual(verify_release.command_fetch(args, run), 0)
        patterns = [w for i, w in enumerate(run.asked[0]) if i and run.asked[0][i - 1] == "--pattern"]
        self.assertEqual(sorted(patterns), sorted([a.name(VERSION) for a in release.archives_for("windows")]
                                                  + [release.SUMS]))

    def test_an_archive_that_is_not_the_one_listed_is_refused(self):
        args = types.SimpleNamespace(tag=TAG, os="windows", dir=os.path.join(self.root, "p"))
        with self.assertRaises(contents.Refused):
            verify_release.command_fetch(args, self.gh("nkb_0.1.0_windows_amd64.zip"))


class Signatures(unittest.TestCase):
    def test_windows_reads_the_signature_of_the_file_it_is_given(self):
        good = json.dumps({"status": "Valid", "sha256": PIN, "timestamped": True})
        run = answering(0, good, "")
        self.assertEqual(verify_release.windows_problems("C:/x/nkb.exe", PIN, run), [])
        command, kw = run.asked[0]
        self.assertEqual(command[0], "pwsh")
        self.assertEqual(kw["env"]["NKB_SIGNED_FILE"], "C:/x/nkb.exe")
        for result in ({"status": "NotSigned", "sha256": "", "timestamped": False},
                       {"status": "Valid", "sha256": "b" * 64, "timestamped": True},
                       {"status": "Valid", "sha256": PIN, "timestamped": False}):
            with self.subTest(result=result):
                self.assertTrue(verify_release.windows_problems("nkb.exe", PIN, answering(0, json.dumps(result), "")))

    def mac(self, **answers):
        """A Mac that passes everything unless told otherwise, and that writes the
        pinned certificate where codesign is asked to extract it."""
        def run(command, **_):
            run.asked.append(command)
            for key, answer in answers.items():
                if key in " ".join(command):
                    return subprocess.CompletedProcess(command, *answer)
            if any(w.startswith("--extract-certificates=") for w in command):
                prefix = next(w for w in command if w.startswith("--extract-certificates="))
                with open(prefix.split("=", 1)[1] + "0", "wb") as handle:
                    handle.write(run.certificate)
            if command[:2] == ["codesign", "-d"]:
                return subprocess.CompletedProcess(command, 0, "", "flags=0x10000(runtime)\nTimestamp=Oct 9, 2026\n")
            if command[:2] == ["spctl", "--status"]:
                return subprocess.CompletedProcess(command, 0, "assessments enabled\n", "")
            return subprocess.CompletedProcess(command, 0, "", "")

        run.asked = []
        run.certificate = b"the certificate"
        return run

    def test_macos_asks_codesign_stapler_the_certificate_and_gatekeeper(self):
        run = self.mac()
        self.assertEqual(verify_release.macos_problems("/x/nkb.app", MAC_PIN, run), [])
        said = [" ".join(c) for c in run.asked]
        for needed in ("codesign --verify --strict --deep /x/nkb.app", "xcrun stapler validate /x/nkb.app",
                       "spctl --assess --type exec -vv /x/nkb.app"):
            self.assertIn(needed, said)

    def test_macos_names_every_problem_of_a_bundle(self):
        pin = MAC_PIN
        for answers, says in (({"--verify": (1, "", "a sealed resource is missing")}, "sealed"),
                              ({"stapler": (65, "", "does not have a ticket stapled")}, "ticket"),
                              ({"codesign -d --verbose": (0, "", "flags=0x0(none)\nTimestamp=x\n")}, "hardened"),
                              ({"codesign -d --verbose": (0, "", "flags=0x10000(runtime)\n")}, "timestamp"),
                              ({"--assess": (3, "", "rejected")}, "Gatekeeper")):
            with self.subTest(says=says):
                problems = verify_release.macos_problems("/x/nkb.app", pin, self.mac(**answers))
                self.assertEqual(len(problems), 1, problems)
                self.assertIn(says, problems[0])
        self.assertIn("DIFFERENT", verify_release.macos_problems("/x/nkb.app", "0" * 64, self.mac())[0])

    def test_gatekeeper_is_not_asked_when_it_is_off_because_then_it_accepts_everything(self):
        run = self.mac(**{"spctl --status": (0, "assessments disabled\n", "")})
        self.assertEqual(verify_release.macos_problems("/x/nkb.app", MAC_PIN, run), [])
        self.assertFalse(any("--assess" in c for c in run.asked))

    def test_each_system_is_held_to_its_own_signature_and_signed_files_to_the_signed_layout(self):
        asked = []

        def tried(archive, base, directory, packs, run, signed=False):
            asked.append(("tried", archive.os, archive.program, signed))
            return []

        with unittest.mock.patch.object(release, "tried_programs", tried), \
                unittest.mock.patch.object(verify_release, "windows_problems",
                                           lambda p, pin, run: asked.append(("windows", os.path.basename(p))) or []), \
                unittest.mock.patch.object(verify_release, "macos_problems",
                                           lambda p, pin, run: asked.append(("macos", os.path.basename(p))) or []), \
                unittest.mock.patch.object(verify_release, "glibc_problems",
                                           lambda p, run: asked.append(("linux", os.path.basename(p))) or []):
            for system in ("windows", "macos", "linux"):
                args = types.SimpleNamespace(tag=TAG, os=system, unpacked="unpacked")
                self.assertEqual(verify_release.command_programs(args, answering()), 0)
        self.assertEqual(sorted(a for a in asked if a[0] == "tried"),
                         sorted(("tried", s, p, s != "linux") for s in ("windows", "macos", "linux")
                                for p in ("nkb", "nkb-gui")))
        self.assertEqual(sorted(a for a in asked if a[0] != "tried"),
                         sorted([("windows", "nkb.exe"), ("windows", "nkb-gui.exe"), ("macos", "nkb.app"),
                                 ("macos", "nkb-gui.app"), ("linux", "nkb"), ("linux", "nkb-gui")]))

    def test_a_problem_of_one_program_is_named_with_its_archive_and_refuses(self):
        with unittest.mock.patch.object(release, "tried_programs", lambda *a, **k: []), \
                unittest.mock.patch.object(verify_release, "glibc_problems", lambda p, run: ["needs glibc 2.39"]):
            with self.assertRaises(contents.Refused) as refusal:
                verify_release.command_programs(types.SimpleNamespace(tag=TAG, os="linux", unpacked="u"), answering())
        self.assertIn("nkb_0.1.0_linux_amd64.tar.gz: needs glibc 2.39", refusal.exception.problems)

    def test_a_linux_program_may_not_need_a_newer_glibc_than_readme_promises(self):
        table = "0000 DF *UND* 0000 (GLIBC_%s) f\n"
        self.assertEqual(verify_release.glibc_problems("nkb", answering(0, table % "2.35", "")), [])
        self.assertTrue(verify_release.glibc_problems("nkb", answering(0, table % "2.36", "")))
        self.assertTrue(verify_release.glibc_problems("nkb", answering(0, table % "2.39", "")))
        self.assertTrue(verify_release.glibc_problems("nkb", answering(127, "", "no objdump")))


if __name__ == "__main__":
    unittest.main()
