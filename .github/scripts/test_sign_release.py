"""Tests for sign_release.py and sign_macos.py, phase B of the release.

Run from the root of the repository:

    python3 -m unittest discover -s .github/scripts -p "test_sign_release.py"

Neither the card nor the Mac is needed. What PowerShell, gh, codesign and
notarytool would answer is made up, and every decision those answers feed is a
function of its own. The signing itself is measured by a person running the
ritual, and these hold everything around it.
"""
import datetime
import hashlib
import io
import json
import os
import re
import shutil
import subprocess
import tarfile
import tempfile
import unittest
import unittest.mock

import contents
import release
import sign_macos
import sign_release

PIN = "47b79ad3cfa53ef846cad03a59148f8c981d0b1196891e48b8c8d7982b10c148"
HERE = os.path.dirname(os.path.abspath(__file__))


def setUpModule():
    for module in (release, sign_release):
        unittest.mock.patch.object(module, "say", lambda text: None).start()
    unittest.mock.patch.object(sign_macos, "say", lambda text: None).start()


def tearDownModule():
    unittest.mock.patch.stopall()


def code_of(name):
    """A script without its comments and docstrings."""
    with open(os.path.join(HERE, name), encoding="utf-8") as handle:
        text = re.sub(r'(?s)""".*?"""', '""', handle.read())
    return "\n".join(line.split("#", 1)[0] for line in text.splitlines())


class Folder(unittest.TestCase):
    def setUp(self):
        self.root = tempfile.mkdtemp(prefix="sign-test-")
        self.addCleanup(shutil.rmtree, self.root, ignore_errors=True)

    def put(self, name, data):
        path = os.path.join(self.root, name)
        with open(path, "wb") as handle:
            handle.write(data)
        return path


class Pins(Folder):
    def test_the_pins_are_the_two_certificates_with_their_dates(self):
        pins = sign_release.load_pins()
        self.assertEqual(pins.windows.sha256, PIN)
        self.assertEqual(pins.windows.expires, datetime.date(2027, 8, 19))
        self.assertEqual(pins.macos.sha256, "c656a279aba94f535d6fcf9aff1ce05cd51295aea52295677ae5e1f1d7d77794")
        self.assertEqual(pins.timestamp_url, "http://time.certum.pl/")
        self.assertEqual(pins.notary_profile, "tfg-notary")

    def test_a_pin_that_is_not_a_digest_or_a_date_is_refused(self):
        with open(sign_release.PINS, encoding="utf-8") as handle:
            text = handle.read()
        for broken in (text.replace(PIN, PIN[:-1]), text.replace("expires = 2027-08-19", 'expires = "soon"'),
                       text.replace('notary-profile = "tfg-notary"', "")):
            with self.subTest():
                with self.assertRaises(contents.Refused):
                    sign_release.load_pins(self.put("codesign.toml", broken.encode("utf-8")))

    def test_the_subject_of_neither_certificate_is_in_the_pins(self):
        # The fields a subject is written in, and the form a Developer ID
        # identity is named by, which carries a person's name and team.
        with open(sign_release.PINS, encoding="utf-8") as handle:
            text = handle.read()
        self.assertNotRegex(text, r"\b(CN|O|OU|L|S|ST|C|E)=")
        self.assertNotRegex(text, r"Developer ID Application: \S")

    def test_an_expired_certificate_is_refused_and_a_near_one_warned_about(self):
        expires = datetime.date(2027, 8, 19)
        with self.assertRaises(contents.Refused):
            sign_release.expiry_notice(expires, datetime.date(2027, 8, 20))
        self.assertTrue(sign_release.expiry_notice(expires, datetime.date(2027, 6, 1)).startswith("WARNING"))
        self.assertFalse(sign_release.expiry_notice(expires, datetime.date(2026, 10, 9)).startswith("WARNING"))


class Windows(Folder):
    def test_the_certificate_is_chosen_by_its_digest_and_needs_its_key(self):
        other = {"sha256": "0" * 64, "thumbprint": "OTHER", "key": True}
        pinned = {"sha256": PIN, "thumbprint": "THUMB", "key": True}
        self.assertEqual(sign_release.thumbprint_for([other, pinned], PIN), "THUMB")
        with self.assertRaises(contents.Refused):
            sign_release.thumbprint_for([other], PIN)
        with self.assertRaises(contents.Refused):
            sign_release.thumbprint_for([dict(pinned, key=False)], PIN)

    def test_a_signature_counts_only_valid_by_the_pinned_certificate_with_a_timestamp(self):
        good = {"status": "Valid", "sha256": PIN, "timestamped": True}
        self.assertEqual(sign_release.signature_problems(good, PIN, "nkb.exe"), [])
        for bad, says in ((dict(good, status="NotSigned"), "not Valid"),
                          (dict(good, sha256="1" * 64), "DIFFERENT certificate"),
                          (dict(good, sha256=""), "DIFFERENT certificate"),
                          (dict(good, timestamped=False), "no timestamp")):
            with self.subTest(says=says):
                problems = sign_release.signature_problems(bad, PIN, "nkb.exe")
                self.assertTrue(any(says in p for p in problems), problems)

    def test_signtool_signs_with_a_timestamp_and_sha256_by_the_derived_thumbprint(self):
        command = sign_release.sign_command("signtool.exe", "THUMB", "http://time.example/", "nkb.exe")
        self.assertEqual(command, ["signtool.exe", "sign", "/sha1", "THUMB", "/fd", "sha256",
                                   "/tr", "http://time.example/", "/td", "sha256", "nkb.exe"])

    def test_the_newest_sdk_is_chosen_by_its_version_and_not_by_the_order_of_names(self):
        kits = os.path.join(self.root, "bin")
        for name in ("10.0.9999.0", "10.0.28000.0", "10.0.26100.0", "x64", "arm64"):
            os.makedirs(os.path.join(kits, name, "x64"))
            open(os.path.join(kits, name, "x64", "signtool.exe"), "w").close()
        self.assertEqual(sign_release.find_signtool(kits), os.path.join(kits, "10.0.28000.0", "x64", "signtool.exe"))
        with self.assertRaises(contents.Refused):
            sign_release.find_signtool(os.path.join(self.root, "nowhere"))

    def test_a_path_reaches_powershell_through_the_environment_and_never_its_text(self):
        for script in (sign_release.STORE_SCRIPT, sign_release.SIGNATURE_SCRIPT):
            self.assertNotIn("%s", script)
            self.assertNotIn("Cert:\\", script)
        self.assertIn("-LiteralPath $env:NKB_SIGNED_FILE", sign_release.SIGNATURE_SCRIPT)
        self.assertIn("X509Store", sign_release.STORE_SCRIPT)
        self.assertIn("$env:NKB_OID", sign_release.STORE_SCRIPT)

    def test_what_powershell_says_on_stderr_is_printed_even_with_exit_zero(self):
        said = []

        def run(command, **kw):
            self.assertEqual(kw["env"]["NKB_SIGNED_FILE"], "a'b.exe")
            return subprocess.CompletedProcess(command, 0, "{}", "Cannot find drive\n")

        with unittest.mock.patch.object(sign_release, "say", said.append):
            sign_release.powershell("x", {"NKB_SIGNED_FILE": "a'b.exe"}, run)
        self.assertTrue(any("Cannot find drive" in line for line in said))


class TheBuild(Folder):
    TAG = "v0.1.0-dev.2"

    def lay_out(self):
        names = release.handover_names(self.TAG)
        for name in names:
            self.put(name, name.encode("ascii"))
        self.put(release.BUILD_SUMS, "".join("%s  %s\n" % (contents.sha256_of(os.path.join(self.root, n)), n)
                                             for n in names).encode("ascii"))
        self.put(sign_release.BUILD_BUNDLE, b"{}")
        return names

    def test_every_file_is_verified_against_who_built_it_and_from_what(self):
        names = self.lay_out()
        asked = []
        sign_release.check_build(self.root, self.TAG, True, "owner/repo", asked.append)
        self.assertEqual(len(asked), len(names))
        for command in asked:
            self.assertEqual(command[:3], ["gh", "attestation", "verify"])
            for flag, value in (("--repo", "owner/repo"), ("--source-ref", "refs/heads/main"),
                                ("--signer-workflow", "owner/repo/.github/workflows/release.yml"),
                                ("--bundle", os.path.join(self.root, sign_release.BUILD_BUNDLE))):
                self.assertEqual(command[command.index(flag) + 1], value)
            self.assertIn("--deny-self-hosted-runners", command)

    def test_a_tag_is_verified_against_the_tag(self):
        command = sign_release.verify_command("x.zip", "v0.1.0", False, "owner/repo")
        self.assertEqual(command[command.index("--source-ref") + 1], "refs/tags/v0.1.0")

    def test_a_changed_file_a_missing_one_and_a_stray_one_are_refused_before_verifying(self):
        names = self.lay_out()
        self.put(names[0], b"changed")
        with self.assertRaises(contents.Refused):
            sign_release.check_build(self.root, self.TAG, True, "owner/repo", lambda c: None)
        self.lay_out()
        self.put("stray.txt", b"x")
        with self.assertRaises(contents.Refused):
            sign_release.check_build(self.root, self.TAG, True, "owner/repo", lambda c: None)
        os.remove(os.path.join(self.root, "stray.txt"))
        os.remove(os.path.join(self.root, names[-1]))
        with self.assertRaises(contents.Refused):
            sign_release.check_build(self.root, self.TAG, True, "owner/repo", lambda c: None)

    def test_a_list_of_digests_with_a_line_that_is_not_one_is_refused(self):
        with self.assertRaises(contents.Refused):
            release.listed_digests("not a digest  file.zip\n", release.BUILD_SUMS)


class Bundles(unittest.TestCase):
    APP = "nkb.app"
    BEFORE = [release.Entry("nkb.app", "directory", 0o755),
              release.Entry("nkb.app/Contents", "directory", 0o755),
              release.Entry("nkb.app/Contents/Info.plist", "file", 0o644, b"plist"),
              release.Entry("nkb.app/Contents/MacOS", "directory", 0o755),
              release.Entry("nkb.app/Contents/MacOS/nkb", "file", 0o755, b"program")]

    def signed(self, *extra):
        added = [release.Entry("nkb.app/Contents/_CodeSignature", "directory", 0o755),
                 release.Entry("nkb.app/Contents/_CodeSignature/CodeResources", "file", 0o644, b"seal"),
                 release.Entry("nkb.app/Contents/CodeResources", "file", 0o644, b"ticket")]
        return [release.Entry(e.name, e.kind, e.mode, b"signed program" if e.kind == "file" and e.mode == 0o755
                              else e.data) for e in self.BEFORE] + added + list(extra)

    def test_the_bundle_travels_to_the_mac_and_back_as_it_is(self):
        again = sign_release.entries_of_tar(sign_release.tar_bytes(self.BEFORE))
        self.assertEqual(again, self.BEFORE)

    def test_signing_may_add_the_seal_and_the_ticket_and_nothing_else(self):
        after = self.signed()
        self.assertEqual(sign_release.signed_bundle(self.BEFORE, after, self.APP), after)
        stray = release.Entry("nkb.app/Contents/._Info.plist", "file", 0o644, b"x")
        with self.assertRaises(contents.Refused):
            sign_release.signed_bundle(self.BEFORE, self.signed(stray), self.APP)

    def test_a_bundle_without_its_ticket_or_missing_a_file_or_not_executable_is_refused(self):
        without_ticket = [e for e in self.signed() if not e.name.endswith("Contents/CodeResources")]
        missing = [e for e in self.signed() if not e.name.endswith("Info.plist")]
        flat = [release.Entry(e.name, e.kind, 0o644 if e.kind == "file" else e.mode, e.data) for e in self.signed()]
        for after, says in ((without_ticket, "without"), (missing, "removed"), (flat, "no longer executable")):
            with self.subTest(says=says):
                with self.assertRaises(contents.Refused) as caught:
                    sign_release.signed_bundle(self.BEFORE, after, self.APP)
                self.assertTrue(any(says in p for p in caught.exception.problems), caught.exception.problems)


class Publication(Folder):
    TAG = "v0.1.0-dev.2"

    def test_the_page_gets_the_signed_files_the_statement_and_the_checksums(self):
        for name in release.handover_names(self.TAG):
            self.put(name, name.encode("ascii"))
        self.put(release.BUILD_SUMS, b"old list")
        self.put(sign_release.BUILD_BUNDLE, b"{}")
        sign_release.name_for_publication(self.root, self.TAG)
        sign_release.write_checksums(self.root, self.TAG)
        self.assertEqual(sorted(os.listdir(self.root)), release.published_files(self.TAG))
        self.assertIn("verify-naughty-keyboard_0.1.0-dev.2.provenance.sigstore.json", os.listdir(self.root))
        with open(os.path.join(self.root, sign_release.SUMS), "rb") as handle:
            listed = handle.read()
        self.assertNotIn(b"\r", listed)
        self.assertEqual(len(listed.splitlines()), 8)
        self.assertNotIn(release.BUILD_SUMS.encode("ascii"), listed)

    def test_the_checksums_refuse_a_file_that_is_not_for_the_page(self):
        for name in release.published_files(self.TAG):
            if name != sign_release.SUMS:
                self.put(name, b"x")
        self.put("nkb.exe", b"left behind")
        with self.assertRaises(contents.Refused):
            sign_release.write_checksums(self.root, self.TAG)

    def test_nothing_is_uploaded_anywhere_but_to_an_open_draft(self):
        asked = []
        for state in ("missing", "published"):
            with self.subTest(state=state):
                with unittest.mock.patch.object(release, "release_state", lambda tag: state), \
                        unittest.mock.patch.object(sign_release, "run", asked.append):
                    with self.assertRaises(contents.Refused):
                        sign_release.upload(self.root, self.TAG, "owner/repo")
        self.assertEqual(asked, [])

    def test_a_draft_missing_a_file_or_a_different_list_or_published_is_refused(self):
        expected = release.published_files(self.TAG)
        view = {"isDraft": True, "assets": [{"name": n} for n in expected]}
        self.assertEqual(sign_release.draft_problems(view, expected, b"sums", b"sums"), [])
        self.assertTrue(sign_release.draft_problems(dict(view, assets=view["assets"][1:]), expected, b"s", b"s"))
        self.assertTrue(sign_release.draft_problems(view, expected, b"sums", b"other"))
        self.assertTrue(sign_release.draft_problems(dict(view, isDraft=False), expected, b"s", b"s"))


class Statement(Folder):
    """Steps 7 and 8: phase C started from the tag, waited for, and the draft
    checked to be the whole release."""
    TAG = "v0.1.0"
    NAME = "verify-naughty-keyboard_0.1.0.sbom.sigstore.json"

    def gh(self, views, runs=()):
        """gh answering the draft with each view in turn and the runs of phase C
        with each list in turn, keeping what it was asked."""
        views, runs = list(views), list(runs)

        def run(command, **_):
            run.asked.append(command)
            if command[1:3] == ["release", "view"]:
                answer = views.pop(0) if len(views) > 1 else views[0]
            elif command[1:3] == ["run", "list"]:
                answer = runs.pop(0) if len(runs) > 1 else (runs[0] if runs else [])
            else:
                answer = None
            return subprocess.CompletedProcess(command, 0, json.dumps(answer), "")

        run.asked = []
        return run

    def wait(self, run, earlier_runs=(), earlier_id=None, limit=300):
        clock = iter(range(0, 10000, 10))
        return sign_release.wait_for_the_statement(self.TAG, "o/r", set(earlier_runs), earlier_id, run,
                                                   sleep=lambda s: None, clock=lambda: next(clock), limit=limit)

    def view(self, statement=None, draft=True):
        assets = [{"name": "a.zip"}] + ([{"name": self.NAME, "id": statement}] if statement else [])
        return {"isDraft": draft, "assets": assets}

    def test_phase_c_is_started_from_the_tag_with_the_tag_and_the_digest(self):
        run = self.gh([None], [[{"databaseId": 7, "status": "completed", "conclusion": "success"}]])
        earlier = sign_release.ask_for_the_statement(self.TAG, "d" * 64, "o/r", run)
        self.assertEqual(earlier, {"7"})
        started = run.asked[-1]
        self.assertEqual(started[:4], ["gh", "workflow", "run", "attest-release.yml"])
        self.assertEqual(started[started.index("--ref") + 1], self.TAG)
        self.assertIn("tag=" + self.TAG, started)
        self.assertIn("digest=" + "d" * 64, started)

    def test_the_wait_ends_when_a_new_statement_is_on_the_draft_and_not_on_an_old_one(self):
        run = self.gh([self.view("old"), self.view("old"), self.view("new")])
        self.assertEqual(sign_release.statement_id(self.wait(run, earlier_id="old"), self.TAG), "new")
        self.assertEqual(len([c for c in run.asked if c[1:3] == ["release", "view"]]), 3)

    def test_a_failed_run_of_phase_c_stops_the_wait_at_once(self):
        failed = [{"databaseId": 8, "status": "completed", "conclusion": "failure", "url": "https://x/8"}]
        run = self.gh([self.view()], [failed])
        with self.assertRaises(contents.Refused) as refusal:
            self.wait(run, earlier_runs={"7"})
        self.assertIn("https://x/8", refusal.exception.problems[0])
        self.assertIn("--attest-only", refusal.exception.problems[-1])
        self.assertEqual(len([c for c in run.asked if c[1:3] == ["run", "list"]]), 1)

    def test_a_run_from_before_is_not_this_one(self):
        old = [{"databaseId": 7, "status": "completed", "conclusion": "failure", "url": "https://x/7"}]
        run = self.gh([self.view(), self.view("new")], [old])
        self.assertEqual(sign_release.statement_id(self.wait(run, earlier_runs={"7"}), self.TAG), "new")

    def test_a_run_that_passed_and_left_no_statement_is_refused_and_not_waited_out(self):
        passed = [{"databaseId": 8, "status": "completed", "conclusion": "success"}]
        run = self.gh([self.view()], [passed])
        with self.assertRaises(contents.Refused) as refusal:
            self.wait(run)
        self.assertIn("no new statement", refusal.exception.problems[0])

    def test_a_statement_that_never_comes_is_refused_after_the_limit(self):
        running = [{"databaseId": 8, "status": "in_progress", "conclusion": ""}]
        run = self.gh([self.view()], [running])
        with self.assertRaises(contents.Refused) as refusal:
            self.wait(run, limit=60)
        self.assertIn("60 seconds", refusal.exception.problems[0])

    def lay_out(self):
        for name in release.published_files(self.TAG):
            self.put(name, name.encode("ascii"))
        return {"isDraft": True, "assets": [{"name": n, "id": n, "digest": "sha256:" + hashlib.sha256(
            n.encode("ascii")).hexdigest()} for n in release.published_files(self.TAG)] + [{"name": self.NAME}]}

    def test_the_whole_release_and_nothing_else_each_file_the_one_signed_here(self):
        view = self.lay_out()
        self.assertEqual(sign_release.complete_problems(view, self.TAG, self.root), [])
        fewer = dict(view, assets=view["assets"][:-1])
        more = dict(view, assets=view["assets"] + [{"name": "stray.zip"}])
        other = json.loads(json.dumps(view))
        other["assets"][0]["digest"] = "sha256:" + "0" * 64
        published = dict(view, isDraft=False)
        for changed, says in ((fewer, "holds"), (more, "holds"), (other, "not the file"), (published, "no longer")):
            with self.subTest(says=says):
                problems = sign_release.complete_problems(changed, self.TAG, self.root)
                self.assertEqual(len(problems), 1, problems)
                self.assertIn(says, problems[0])

    def test_the_statement_on_the_draft_is_checked_against_every_signed_archive(self):
        view = self.lay_out()
        asked = []
        with unittest.mock.patch.object(sign_release, "run", asked.append):
            sign_release.confirm_draft(view, self.TAG, self.root, "o/r", asked.append)
        download = asked[0]
        self.assertEqual(download[:4], ["gh", "release", "download", self.TAG])
        self.assertEqual(download[download.index("--pattern") + 1], self.NAME)
        checks = asked[1:]
        self.assertEqual(sorted(os.path.basename(c[3]) for c in checks),
                         sorted(a.name("0.1.0") for a in contents.load_archives()))
        for check in checks:
            self.assertEqual(check[check.index("--predicate-type") + 1], release.load_promise().predicate)
            self.assertTrue(check[check.index("--bundle") + 1].endswith(self.NAME))

    def test_an_incomplete_draft_is_refused_before_anything_is_downloaded(self):
        view = self.lay_out()
        asked = []
        with unittest.mock.patch.object(sign_release, "run", asked.append):
            with self.assertRaises(contents.Refused):
                sign_release.confirm_draft(dict(view, assets=view["assets"][:-1]), self.TAG, self.root, "o/r",
                                           asked.append)
        self.assertEqual(asked, [])

    def test_asking_again_takes_a_tag_and_nothing_to_try_dry(self):
        for argv in (["--attest-only", "--rehearse", "1"], ["v0.1.0", "--attest-only", "--dry-run"]):
            with self.subTest(argv=argv):
                with unittest.mock.patch("sys.stderr", io.StringIO()):
                    self.assertEqual(sign_release.main(argv), 1)


class Mac(Folder):
    LISTING = ("SHA-256 hash: C656A279ABA94F535D6FCF9AFF1CE05CD51295AEA52295677AE5E1F1D7D77794\n"
               "SHA-1 hash: AAAA1111\n"
               'keychain: "/Users/x/Library/Keychains/login.keychain-db"\n'
               "SHA-256 hash: 0000000000000000000000000000000000000000000000000000000000000000\n"
               "SHA-1 hash: BBBB2222\n")

    def test_the_identity_is_found_from_the_pin_and_never_by_its_name(self):
        self.assertEqual(sign_macos.sha1_for(self.LISTING, "c656a279aba94f535d6fcf9aff1ce05cd51295aea52295677ae5e1f1d7d77794"),
                         "AAAA1111")
        self.assertEqual(sign_macos.sha1_for(self.LISTING, "0" * 64), "BBBB2222")
        self.assertIsNone(sign_macos.sha1_for(self.LISTING, "1" * 64))

    def test_an_identity_without_its_key_is_refused(self):
        def run(command, **_):
            out = self.LISTING if "find-certificate" in command else "1) CCCC3333 \"another\"\n"
            return subprocess.CompletedProcess(command, 0, out, "")
        with self.assertRaises(sign_macos.Refused):
            sign_macos.find_identity("c656a279aba94f535d6fcf9aff1ce05cd51295aea52295677ae5e1f1d7d77794", run)

    def test_only_an_accepted_notarisation_counts(self):
        self.assertTrue(sign_macos.notarised("  id: 1\n  status: Accepted\n"))
        for said in ("  status: Invalid\n", "  status: In Progress\n", ""):
            self.assertFalse(sign_macos.notarised(said))

    def test_a_missing_profile_and_a_locked_keychain_are_told_apart(self):
        for said, says in (("Error: No Keychain password item found for profile", "no notary profile"),
                           ("Error: keychainLocked", "locked itself again")):
            run = lambda command, **_: subprocess.CompletedProcess(command, 1, "", said)
            with self.subTest(says=says):
                with self.assertRaises(sign_macos.Refused) as caught:
                    sign_macos.check_profile("p", run)
                self.assertIn(says, str(caught.exception))

    def mac(self, certificate=b"the pinned certificate", notary="  status: Accepted\n", gatekeeper="assessments enabled"):
        """A Mac that answers every command of sign_bundle, and keeps what it was asked."""
        asked = []

        def run(command, **_):
            asked.append(command)
            if command[:2] == ["codesign", "-d"]:
                prefix = next(c for c in command if c.startswith("--extract-certificates="))
                with open(prefix.split("=", 1)[1] + "0", "wb") as handle:
                    handle.write(certificate)
            if command[:3] == ["xcrun", "notarytool", "submit"]:
                return subprocess.CompletedProcess(command, 0, notary, "")
            if command[:2] == ["spctl", "--status"]:
                return subprocess.CompletedProcess(command, 0, gatekeeper, "")
            return subprocess.CompletedProcess(command, 0, "", "")

        run.asked = asked
        return run

    def sign(self, run):
        pin = hashlib.sha256(b"the pinned certificate").hexdigest()
        upload = os.path.join(self.root, "nkb.app.notarise.zip")
        open(upload, "w").close()
        sign_macos.sign_bundle("/w/nkb.app", "SHA1", pin, "profile", self.root, run)

    def test_a_bundle_is_signed_checked_notarised_stapled_and_shown_to_gatekeeper(self):
        run = self.mac()
        self.sign(run)
        firsts = [" ".join(c[:3]) for c in run.asked]
        self.assertEqual(firsts, ["codesign --force --options", "codesign -d --extract-certificates=" + firsts[1].split("=", 1)[1],
                                  "codesign --verify --strict", "ditto -c -k", "xcrun notarytool submit",
                                  "xcrun stapler staple", "xcrun stapler validate", "spctl --status", "spctl -a -t"])

    def test_a_different_certificate_or_a_refused_notarisation_stops_it(self):
        for run, says in ((self.mac(certificate=b"another"), "DIFFERENT certificate"),
                          (self.mac(notary="  status: Invalid\n"), "did not come back Accepted")):
            with self.subTest(says=says):
                with self.assertRaises(sign_macos.Refused) as caught:
                    self.sign(run)
                self.assertIn(says, str(caught.exception))

    def test_gatekeeper_is_asked_only_when_it_is_on(self):
        run = self.mac(gatekeeper="assessments disabled")
        self.sign(run)
        self.assertNotIn(["spctl", "-a", "-t", "exec", "-vv", "/w/nkb.app"], run.asked)

    def tar(self, *members):
        path = os.path.join(self.root, "x.app.tar")
        with tarfile.open(path, "w") as out:
            for name, kind in members:
                info = tarfile.TarInfo(name)
                info.type = kind
                if kind == tarfile.SYMTYPE:
                    info.linkname = "/etc/passwd"
                out.addfile(info, io.BytesIO(b"") if kind == tarfile.REGTYPE else None)
        return path

    def test_a_tar_with_anything_beside_the_bundle_or_out_of_it_is_refused_before_writing(self):
        work = os.path.join(self.root, "work")
        for members in (
                [("nkb.app", tarfile.DIRTYPE), ("stray.txt", tarfile.REGTYPE)],
                [("nkb.app", tarfile.DIRTYPE), ("nkb.app/../../escaped", tarfile.REGTYPE)],
                [("/nkb.app", tarfile.DIRTYPE)],
                [("nkb.app", tarfile.DIRTYPE), ("nkb.app/link", tarfile.SYMTYPE)],
                [("nkb", tarfile.DIRTYPE)]):
            with self.subTest(members=members):
                with self.assertRaises(sign_macos.Refused):
                    sign_macos.unpack(self.tar(*members), work)
                self.assertFalse(os.path.exists(work), "something was written before the refusal")
                self.assertFalse(os.path.exists(os.path.join(self.root, "escaped")))

    def test_a_bundle_is_unpacked_with_its_modes(self):
        path = self.tar(("nkb.app", tarfile.DIRTYPE), ("nkb.app/Contents", tarfile.DIRTYPE),
                        ("nkb.app/Contents/Info.plist", tarfile.REGTYPE))
        app = sign_macos.unpack(path, os.path.join(self.root, "work"))
        self.assertEqual(app, os.path.join(self.root, "work", "nkb.app"))
        self.assertTrue(os.path.isfile(os.path.join(app, "Contents", "Info.plist")))

    def test_the_bundle_goes_back_without_an_owner_and_with_its_modes(self):
        work = os.path.join(self.root, "work")
        os.makedirs(os.path.join(work, "nkb.app", "Contents", "MacOS"))
        program = os.path.join(work, "nkb.app", "Contents", "MacOS", "nkb")
        with open(program, "wb") as handle:
            handle.write(b"program")
        os.chmod(program, 0o755)
        packed = os.path.join(self.root, "out.tar")
        sign_macos.pack(work, packed)
        with tarfile.open(packed) as back:
            members = {m.name: m for m in back.getmembers()}
        self.assertEqual(sorted(members), ["nkb.app", "nkb.app/Contents", "nkb.app/Contents/MacOS",
                                           "nkb.app/Contents/MacOS/nkb"])
        for member in members.values():
            self.assertEqual((member.uid, member.gid, member.uname, member.gname), (0, 0, "", ""))
        with open(packed, "rb") as handle:
            entries = sign_release.entries_of_tar(handle.read())
        if os.name != "nt":
            self.assertEqual(next(e for e in entries if e.name.endswith("MacOS/nkb")).mode, 0o755)


class WhatTheScriptsNeverDo(unittest.TestCase):
    def test_nothing_publishes(self):
        for name in ("sign_release.py", "sign_macos.py"):
            code = code_of(name)
            for forbidden in ("release edit", "gh release publish", "--draft=false", "--generate-notes"):
                with self.subTest(script=name, forbidden=forbidden):
                    self.assertNotIn(forbidden, code)

    def test_the_mac_script_never_changes_directory_or_takes_a_password_as_an_argument(self):
        code = code_of("sign_macos.py")
        self.assertNotIn("chdir", code)
        self.assertNotIn("cwd=", code)
        # The password is typed at the prompt, never handed over as -p, where
        # every process on the Mac could read it.
        self.assertIn('["security", "unlock-keychain", KEYCHAIN]', code)
        self.assertNotRegex(code, r'"unlock-keychain"[^\]]*"-p"')

    def test_the_bundle_is_signed_with_the_hardened_runtime_and_a_timestamp(self):
        code = code_of("sign_macos.py")
        self.assertIn('["codesign", "--force", "--options", "runtime", "--timestamp", "--sign", sha1, app]', code)
        self.assertIn('"--keepParent"', code)
        self.assertIn('["xcrun", "stapler", "validate", app]', code)

    def test_a_rehearsal_stops_before_uploading(self):
        code = code_of("sign_release.py")
        rehearsal = code.index("        if args.rehearse:\n")
        self.assertLess(rehearsal, code.index("        upload(directory, tag, repository)\n"))
        self.assertIn("return 0", code[rehearsal:rehearsal + 200])


if __name__ == "__main__":
    unittest.main()
