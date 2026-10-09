"""Tests for release.py, the steps of phase A of the release.

Run from the root of the repository:

    python3 -m unittest discover -s .github/scripts -p "test_release.py"

None of these needs Cargo, gh or the network. What gh would answer is made up,
and the archives are packed from made-up programs. CI runs them on every pull
request, so a step of the release that stopped refusing what it should refuses
fails there and not on the day of a release.
"""
import json
import os
import plistlib
import shutil
import stat
import struct
import subprocess
import tarfile
import tempfile
import unittest
import unittest.mock
import zipfile

import contents
import release

WORKSPACE = contents.Workspace("0.1.0", "https://github.com/example/project", "GPL-3.0-only")


def setUpModule():
    # What the steps say as they go is for the log of a run, not for this one.
    unittest.mock.patch.object(release, "say", lambda text: None).start()


def tearDownModule():
    unittest.mock.patch.stopall()


def archive_for(os_name, program="nkb"):
    return next(a for a in contents.load_archives() if a.os == os_name and a.program == program)


def answered(*answers):
    """A stand-in for subprocess.run that gives these answers in turn and keeps
    what it was asked."""
    asked = []

    def run(command, **_):
        asked.append(command)
        code, out, err = answers[len(asked) - 1] if len(asked) <= len(answers) else answers[-1]
        return subprocess.CompletedProcess(command, code, out, err)

    run.asked = asked
    return run


class Folder(unittest.TestCase):
    def setUp(self):
        self.root = tempfile.mkdtemp(prefix="release-test-")
        self.addCleanup(shutil.rmtree, self.root, ignore_errors=True)


class Plans(unittest.TestCase):
    def test_a_pushed_tag_is_a_release(self):
        plan = release.plan_of("push", "tag", "v0.1.0", "7", WORKSPACE)
        self.assertEqual((plan.release, plan.tag, plan.version, plan.base, plan.prerelease),
                         (True, "v0.1.0", "0.1.0", "0.1.0", False))
        self.assertEqual(plan.sbom(), "verify-naughty-keyboard_0.1.0.spdx.json")

    def test_a_candidate_is_a_prerelease_of_the_same_version(self):
        plan = release.plan_of("push", "tag", "v0.1.0-rc.1", "7", WORKSPACE)
        self.assertEqual((plan.release, plan.version, plan.base, plan.prerelease),
                         (True, "0.1.0-rc.1", "0.1.0", True))

    def test_anything_but_a_pushed_tag_builds_under_a_version_no_release_carries(self):
        for event, ref_type, ref_name in (("workflow_dispatch", "branch", "main"),
                                          ("workflow_dispatch", "tag", "v0.1.0"),
                                          ("pull_request", "branch", "12/merge"),
                                          ("push", "branch", "main")):
            with self.subTest(event=event, ref=ref_name):
                plan = release.plan_of(event, ref_type, ref_name, "42", WORKSPACE)
                self.assertFalse(plan.release)
                self.assertEqual(plan.tag, "v0.1.0-dev.42")

    def test_a_tag_of_another_version_is_refused(self):
        for tag in ("v0.2.0", "v0.1", "0.1.0", "v0.1.0+build"):
            with self.subTest(tag=tag):
                with self.assertRaises(contents.Refused):
                    release.plan_of("push", "tag", tag, "7", WORKSPACE)

    def test_a_run_number_that_is_not_a_number_is_refused(self):
        # 7a and x would make a well-formed suffix, so only the check of the run
        # number itself can refuse them.
        for number in ("7a", "x", "7 && rm"):
            with self.subTest(number=number):
                with self.assertRaises(contents.Refused) as caught:
                    release.plan_of("workflow_dispatch", "branch", "main", number, WORKSPACE)
                self.assertTrue(any("run number" in p for p in caught.exception.problems))

    def test_each_platform_is_built_once_on_its_runner(self):
        platforms = release.platforms_of(contents.load_archives())
        self.assertEqual([(p["os"], p["runner"]) for p in platforms],
                         [("windows", "windows-latest"), ("macos", "macos-latest"), ("linux", "ubuntu-22.04")])

    def test_an_output_with_a_line_break_is_refused_and_the_rest_are_written(self):
        with tempfile.TemporaryDirectory() as folder:
            path = os.path.join(folder, "output")
            release.write_outputs({"tag": "v0.1.0", "platforms": "[]"}, path)
            with open(path, encoding="utf-8") as handle:
                self.assertEqual(handle.read(), "tag=v0.1.0\nplatforms=[]\n")
            with self.assertRaises(contents.Refused):
                release.write_outputs({"tag": "v0.1.0\nrelease=true"}, path)


class Verdicts(unittest.TestCase):
    def run_on(self, branch, event, status, conclusion, created):
        return {"headBranch": branch, "event": event, "status": status, "conclusion": conclusion,
                "createdAt": created, "url": "u"}

    def test_the_newest_run_on_main_decides(self):
        older = self.run_on("main", "push", "completed", "failure", "2026-10-09T10:00:00Z")
        newer = self.run_on("main", "push", "completed", "success", "2026-10-09T11:00:00Z")
        self.assertEqual(release.verdict([older, newer]), (True, "success"))
        self.assertEqual(release.verdict([newer, dict(older, createdAt="2026-10-09T12:00:00Z")]),
                         (True, "failure"))

    def test_a_run_still_going_is_waited_for(self):
        done, _ = release.verdict([self.run_on("main", "push", "in_progress", None, "2026-10-09T10:00:00Z")])
        self.assertFalse(done)

    def test_runs_that_are_not_main_do_not_count(self):
        for run in (self.run_on("feature/x", "pull_request", "completed", "success", "t"),
                    self.run_on("main", "workflow_dispatch", "completed", "success", "t"),
                    self.run_on("feature/x", "push", "completed", "success", "t")):
            with self.subTest(run=run):
                done, answer = release.verdict([run])
                self.assertTrue(done)
                self.assertIn("never ran", answer)

    def test_a_scheduled_run_on_main_is_a_newer_answer(self):
        push = self.run_on("main", "push", "completed", "success", "2026-10-09T10:00:00Z")
        schedule = self.run_on("main", "schedule", "completed", "failure", "2026-10-12T05:00:00Z")
        self.assertEqual(release.verdict([push, schedule]), (True, "failure"))


class Waiting(unittest.TestCase):
    RUNNING = json.dumps([{"headBranch": "main", "event": "push", "status": "in_progress",
                           "conclusion": None, "createdAt": "2026-10-09T10:00:00Z", "url": "u"}])
    GREEN = json.dumps([{"headBranch": "main", "event": "push", "status": "completed",
                         "conclusion": "success", "createdAt": "2026-10-09T10:00:00Z", "url": "u"}])
    RED = json.dumps([{"headBranch": "main", "event": "push", "status": "completed",
                       "conclusion": "failure", "createdAt": "2026-10-09T10:00:00Z", "url": "u"}])

    def wait(self, run, limit=600, step=30):
        slept = []
        now = [0]

        def sleep(seconds):
            slept.append(seconds)
            now[0] += step

        answers = release.wait_for_green("abc", run=run, sleep=sleep, clock=lambda: now[0],
                                         workflows=("ci.yml", "semgrep.yml"), limit=limit)
        return answers, slept

    def test_it_waits_while_one_runs_and_passes_when_all_are_green(self):
        run = answered((0, self.GREEN, ""), (0, self.RUNNING, ""), (0, self.GREEN, ""), (0, self.GREEN, ""))
        answers, slept = self.wait(run)
        self.assertEqual(len(slept), 1)
        self.assertEqual({w: a for w, (_, a) in answers.items()}, {"ci.yml": "success", "semgrep.yml": "success"})
        self.assertEqual(run.asked[0][:4], ["gh", "run", "list", "--workflow"])
        self.assertIn("abc", run.asked[0])

    def test_a_red_workflow_refuses_and_says_which(self):
        run = answered((0, self.GREEN, ""), (0, self.RED, ""))
        with self.assertRaises(contents.Refused) as caught:
            self.wait(run)
        self.assertTrue(any("semgrep.yml: failure" in p for p in caught.exception.problems))

    def test_a_workflow_that_never_ran_refuses(self):
        run = answered((0, self.GREEN, ""), (0, "[]", ""))
        with self.assertRaises(contents.Refused) as caught:
            self.wait(run)
        self.assertTrue(any("never ran" in p for p in caught.exception.problems))

    def test_waiting_has_an_end(self):
        run = answered((0, self.RUNNING, ""))
        with self.assertRaises(contents.Refused) as caught:
            self.wait(run, limit=60)
        self.assertTrue(any("still running" in p for p in caught.exception.problems))

    def test_gh_that_cannot_answer_refuses(self):
        with self.assertRaises(contents.Refused):
            self.wait(answered((1, "", "HTTP 502")))


class Changelogs(unittest.TestCase):
    CLOSED = "# Changelog\n\n## [Unreleased]\n\n## [0.1.0] - 2026-10-10\n\n### Added\n\n- A thing.\n"

    def test_a_closed_changelog_gives_its_section_as_the_notes(self):
        problems, section = release.changelog_section(self.CLOSED, "0.1.0")
        self.assertEqual(problems, [])
        self.assertEqual(section, "### Added\n\n- A thing.")

    def test_a_candidate_has_its_own_section(self):
        text = self.CLOSED.replace("[0.1.0]", "[0.1.0-rc.1]")
        self.assertEqual(release.changelog_section(text, "0.1.0-rc.1")[0], [])
        self.assertTrue(release.changelog_section(text, "0.1.0")[0])

    def test_what_keeps_a_changelog_open_is_refused(self):
        for text, says in (
                (self.CLOSED.replace("## [Unreleased]\n", "## [Unreleased]\n\n- Not moved.\n"), "under [Unreleased]"),
                (self.CLOSED.replace("## [0.1.0] - 2026-10-10", "## [0.1.0]"), "no date"),
                (self.CLOSED.replace("## [0.1.0] - 2026-10-10", "## [0.1.0] - soon"), "no date"),
                (self.CLOSED.replace("[0.1.0]", "[0.0.9]"), "no section for 0.1.0"),
                (self.CLOSED.replace("## [Unreleased]\n", ""), "exactly one"),
                (self.CLOSED + "\n## [Unreleased]\n", "exactly one"),
                (self.CLOSED + "\n## [0.1.0] - 2026-10-11\n\n- Again.\n", "2 sections"),
                (self.CLOSED.replace("### Added\n\n- A thing.\n", ""), "is empty")):
            with self.subTest(says=says):
                problems, _ = release.changelog_section(text, "0.1.0")
                self.assertTrue(any(says in p for p in problems), problems)

    def test_link_definitions_are_not_entries(self):
        text = (self.CLOSED + "\n[Unreleased]: https://example.org/compare/v0.1.0...HEAD\n"
                "[0.1.0]: https://example.org/releases/tag/v0.1.0\n")
        problems, section = release.changelog_section(text, "0.1.0")
        self.assertEqual(problems, [])
        self.assertNotIn("https://", section)


class TheChangelog(unittest.TestCase):
    """CHANGELOG.md itself, which a release reads word for word."""

    def setUp(self):
        with open(release.CHANGELOG, encoding="utf-8") as handle:
            self.lines = handle.read().splitlines()

    def test_every_heading_is_unreleased_or_a_dated_version(self):
        headings = [line for line in self.lines if line.startswith("## ")]
        self.assertEqual(headings[0], "## [Unreleased]")
        for heading in headings[1:]:
            with self.subTest(heading=heading):
                match = release.VERSION_HEADING.match(heading)
                self.assertTrue(match and release.DATED.match(match.group(2)), heading)
                self.assertTrue(contents.TAG.match("v" + match.group(1)), heading)

    def test_no_entry_is_longer_than_a_hundred_words(self):
        # Nobody reads an entry longer than this, and one section of another
        # project's changelog grew to eleven thousand words without a limit.
        entries = []
        for line in self.lines:
            if line.startswith("- "):
                entries.append(line[2:])
            elif line.startswith("  ") and entries:
                entries[-1] += " " + line.strip()
        self.assertTrue(entries)
        for entry in entries:
            with self.subTest(entry=entry[:60]):
                self.assertLessEqual(len(entry.split()), 100)


class Builds(unittest.TestCase):
    def test_each_program_is_built_by_a_cargo_of_its_own(self):
        for os_name in ("windows", "macos", "linux"):
            archives = release.archives_for(os_name)
            commands = release.build_commands(archives)
            with self.subTest(os=os_name):
                self.assertEqual(len(commands), 2)
                for command, archive in zip(commands, archives):
                    self.assertEqual(command.count("-p"), 1)
                    self.assertEqual(command[command.index("-p") + 1], archive.package)
                    self.assertEqual(command[command.index("--target") + 1], archive.target)
                    self.assertIn("--locked", command)
                    self.assertIn("--release", command)

    def test_a_compiler_flag_variable_in_the_environment_is_refused_even_empty(self):
        archives = release.archives_for("windows")
        for name in ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "CARGO_BUILD_RUSTFLAGS",
                     "CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS"):
            with self.subTest(name=name):
                with self.assertRaises(contents.Refused):
                    release.build_environment(archives, {"PATH": "p", name: ""})
        self.assertEqual(release.build_environment(archives, {"RUSTDOCFLAGS": "-D warnings"}),
                         {"RUSTDOCFLAGS": "-D warnings"})

    def test_only_macos_is_told_its_oldest_version(self):
        self.assertEqual(release.build_environment(release.archives_for("macos"), {})["MACOSX_DEPLOYMENT_TARGET"],
                         "11.0")
        for os_name in ("windows", "linux"):
            self.assertNotIn("MACOSX_DEPLOYMENT_TARGET", release.build_environment(release.archives_for(os_name), {}))

    def test_the_program_is_taken_from_where_cargo_put_it(self):
        archive = archive_for("windows")
        self.assertEqual(release.built_program(archive, "/repo", {}).replace(os.sep, "/"),
                         "/repo/target/x86_64-pc-windows-msvc/release/nkb.exe")
        self.assertEqual(release.built_program(archive, "/repo", {"CARGO_TARGET_DIR": "/elsewhere"}
                                               ).replace(os.sep, "/").split(":")[-1],
                         "/elsewhere/x86_64-pc-windows-msvc/release/nkb.exe")

    def test_an_unknown_platform_is_refused(self):
        with self.assertRaises(contents.Refused):
            release.archives_for("freebsd")


class Packing(Folder):
    PROGRAM = b"\x7fELF a program"
    NOTICES = b"notices\n"
    EPOCH = 1791000000

    def pack(self, archive, name="out"):
        path = os.path.join(self.root, name + "." + archive.format)
        release.write_archive(path, archive.format, release.entries_of(
            archive, "0.1.0", self.PROGRAM, self.NOTICES), self.EPOCH)
        return path

    def test_a_macos_archive_holds_a_bundle_and_a_link_into_it(self):
        archive = archive_for("macos")
        with tarfile.open(self.pack(archive)) as packed:
            members = {m.name: m for m in packed.getmembers()}
            self.assertEqual(sorted(n for n, m in members.items() if not m.isdir()),
                             release.expected_names(archive))
            link = members["nkb"]
            self.assertTrue(link.issym())
            self.assertEqual(link.linkname, "nkb.app/Contents/MacOS/nkb")
            program = members["nkb.app/Contents/MacOS/nkb"]
            self.assertEqual(stat.S_IMODE(program.mode), 0o755)
            self.assertEqual(packed.extractfile(program).read(), self.PROGRAM)
            for member in members.values():
                self.assertEqual((member.uid, member.gid, member.uname, member.gname, member.mtime),
                                 (0, 0, "", "", self.EPOCH))
            plist = plistlib.loads(packed.extractfile(members["nkb.app/Contents/Info.plist"]).read())
        self.assertEqual(plist["CFBundleIdentifier"], "com.donislawdev.nkb")
        self.assertEqual(plist["CFBundleExecutable"], "nkb")
        self.assertEqual(plist["CFBundleIconFile"], "edamame")
        self.assertEqual(plist["LSMinimumSystemVersion"], "11.0")
        self.assertEqual((plist["CFBundleShortVersionString"], plist["CFBundleVersion"]), ("0.1.0", "0.1.0"))

    def test_the_bundle_says_the_version_of_the_workspace_and_never_a_suffix(self):
        archive = archive_for("macos", "nkb-gui")
        plist = plistlib.loads(release.info_plist(archive, "0.1.0", archive.packaged[0]))
        self.assertEqual(plist["CFBundleShortVersionString"], "0.1.0")
        self.assertEqual(plist["CFBundleIdentifier"], "com.donislawdev.nkb-gui")

    def test_a_linux_archive_holds_the_program_executable_and_no_bundle(self):
        archive = archive_for("linux")
        with tarfile.open(self.pack(archive)) as packed:
            names = sorted(m.name for m in packed.getmembers())
            self.assertEqual(names, release.expected_names(archive))
            self.assertEqual(stat.S_IMODE(packed.getmember("nkb").mode), 0o755)
            self.assertEqual(stat.S_IMODE(packed.getmember("LICENSE").mode), 0o644)
            self.assertEqual(packed.extractfile("THIRD-PARTY-NOTICES.txt").read(), self.NOTICES)

    def test_a_windows_archive_is_a_zip_of_files(self):
        archive = archive_for("windows", "nkb-gui")
        with zipfile.ZipFile(self.pack(archive)) as packed:
            self.assertEqual(sorted(packed.namelist()), release.expected_names(archive))
            self.assertEqual(packed.read("nkb-gui.exe"), self.PROGRAM)
            with open(os.path.join(contents.ROOT, "README.md"), "rb") as handle:
                self.assertEqual(packed.read("README.md"), handle.read())

    def test_the_same_build_packs_into_the_same_bytes(self):
        for os_name in ("windows", "macos", "linux"):
            archive = archive_for(os_name)
            with self.subTest(os=os_name):
                first, second = self.pack(archive, "first"), self.pack(archive, "second")
                with open(first, "rb") as one, open(second, "rb") as two:
                    self.assertEqual(one.read(), two.read())

    def test_the_environment_can_say_the_time_of_the_entries(self):
        self.assertEqual(release.commit_epoch(environ={"SOURCE_DATE_EPOCH": "1791000000"}), 1791000000)
        with self.assertRaises(contents.Refused):
            release.commit_epoch(environ={"SOURCE_DATE_EPOCH": "yesterday"})

    def test_otherwise_the_time_of_the_entries_is_the_commits(self):
        head = release.capture(["git", "-C", contents.ROOT, "log", "-1", "--format=%ct"])
        if head.returncode != 0:
            with self.assertRaises(contents.Refused):
                release.commit_epoch(environ={})
            self.skipTest("no git or not a git checkout, so there is no commit to compare with: %s"
                          % head.stderr.strip())
        self.assertEqual(release.commit_epoch(environ={}), int(head.stdout.strip()))

    def test_an_archive_read_and_written_again_is_the_same_bytes(self):
        # What lets a signed program go back into its archive with nothing else
        # moving: the reading is the exact reverse of the writing.
        for os_name in ("windows", "macos", "linux"):
            archive = archive_for(os_name, "nkb-gui")
            with self.subTest(os=os_name):
                first = self.pack(archive, "first")
                archive_format, entries, epoch = release.read_archive(first)
                self.assertEqual((archive_format, epoch), (archive.format, self.EPOCH))
                again = os.path.join(self.root, "again." + archive.format)
                release.write_archive(again, archive_format, entries, epoch)
                with open(first, "rb") as one, open(again, "rb") as two:
                    self.assertEqual(one.read(), two.read())

    def test_an_archive_this_script_would_not_write_is_refused(self):
        # A zip with DOS attributes and no Unix mode, which is what a Windows
        # tool such as Compress-Archive writes.
        path = os.path.join(self.root, "loose.zip")
        with zipfile.ZipFile(path, "w") as loose:
            info = zipfile.ZipInfo("a.txt", date_time=(2026, 1, 1, 0, 0, 0))
            info.create_system = 0
            info.external_attr = 0x20
            loose.writestr(info, b"no mode")
        with self.assertRaises(contents.Refused):
            release.read_archive(path)
        mixed = os.path.join(self.root, "mixed.zip")
        with zipfile.ZipFile(mixed, "w") as packed:
            for name, when in (("a", (2026, 1, 1, 0, 0, 0)), ("b", (2026, 1, 2, 0, 0, 0))):
                info = zipfile.ZipInfo(name, date_time=when)
                info.external_attr = 0o100644 << 16
                packed.writestr(info, b"x")
        with self.assertRaises(contents.Refused):
            release.read_archive(mixed)
        with self.assertRaises(contents.Refused):
            release.read_archive(os.path.join(self.root, "x.7z"))

    def test_a_link_in_a_zip_is_refused(self):
        entry = release.Entry("nkb", "link", 0o755, target="elsewhere")
        with self.assertRaises(contents.Refused):
            release.write_archive(os.path.join(self.root, "x.zip"), "zip", [entry], self.EPOCH)
        self.assertEqual(os.listdir(self.root), [], "a refused archive leaves nothing behind")

    def test_unpacked_with_tar_the_archive_is_what_the_layout_says(self):
        archive = archive_for("macos")
        path = self.pack(archive)
        unpacked = os.path.join(self.root, "unpacked")
        os.makedirs(unpacked)
        try:
            os.symlink("x", os.path.join(self.root, "probe"))
        except OSError:
            self.skipTest("this system cannot make a symbolic link, so a tar with one cannot be unpacked here")
        done = subprocess.run(["tar", "-xzf", path, "-C", unpacked], capture_output=True, text=True)
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertEqual(release.unpacked_names(unpacked), release.expected_names(archive))
        self.assertTrue(os.path.islink(os.path.join(unpacked, "nkb")))
        with open(os.path.join(unpacked, "nkb"), "rb") as handle:
            self.assertEqual(handle.read(), self.PROGRAM)


def portable_executable(imported, delayed, magic=0x20B):
    """The smallest Windows program header this reader needs: one section that
    holds the names, the import table and the delay-load table."""
    section_rva, section_raw = 0x1000, 0x400
    body = bytearray()

    def name(text):
        at = section_rva + len(body)
        body.extend(text.encode("ascii") + b"\0")
        return at

    imported_names = [name(n) for n in imported]
    delayed_names = [name(n) for n in delayed]
    while len(body) % 4:
        body.append(0)
    import_table = section_rva + len(body)
    for at in imported_names:
        body.extend(struct.pack("<5I", 0, 0, 0, at, 0))
    body.extend(bytes(20))
    delay_table = section_rva + len(body)
    for at in delayed_names:
        body.extend(struct.pack("<8I", 1, at, 0, 0, 0, 0, 0, 0))
    body.extend(bytes(32))

    header = bytearray(section_raw)
    header[0:2] = b"MZ"
    struct.pack_into("<I", header, 0x3C, 0x80)
    header[0x80:0x84] = b"PE\0\0"
    optional_size = 240 if magic == 0x20B else 224
    struct.pack_into("<HHIIIHH", header, 0x84, 0x8664, 1, 0, 0, 0, optional_size, 0x22)
    optional = 0x84 + 20
    struct.pack_into("<H", header, optional, magic)
    directories = optional + (112 if magic == 0x20B else 96)
    struct.pack_into("<I", header, directories - 4, 16)
    struct.pack_into("<II", header, directories + 8, import_table, 20 * (len(imported) + 1))
    struct.pack_into("<II", header, directories + 8 * 13, delay_table, 32 * (len(delayed) + 1))
    section = optional + optional_size
    header[section:section + 8] = b".idata\0\0"
    struct.pack_into("<IIII", header, section + 8, len(body), section_rva, len(body), section_raw)
    return bytes(header) + bytes(body)


class Imports(Folder):
    def put(self, data):
        path = os.path.join(self.root, "program.exe")
        with open(path, "wb") as handle:
            handle.write(data)
        return path

    def test_both_tables_are_read_in_both_header_kinds(self):
        for magic in (0x20B, 0x10B):
            with self.subTest(magic=hex(magic)):
                path = self.put(portable_executable(["KERNEL32.dll", "VCRUNTIME140.dll"],
                                                    ["api-ms-win-crt-heap-l1-1-0.dll"], magic))
                names = release.dlls_imported_by(path)
                self.assertEqual(names, ["KERNEL32.dll", "VCRUNTIME140.dll", "api-ms-win-crt-heap-l1-1-0.dll"])
                self.assertEqual(release.c_runtime_in(names), ["VCRUNTIME140.dll", "api-ms-win-crt-heap-l1-1-0.dll"])

    def test_a_program_without_the_runtime_has_none(self):
        path = self.put(portable_executable(["kernel32.dll", "api-ms-win-core-synch-l1-2-0.dll"], []))
        self.assertEqual(release.c_runtime_in(release.dlls_imported_by(path)), [])

    def test_every_library_of_the_runtime_is_named(self):
        for dll in ("vcruntime140.dll", "VCRUNTIME140_1.dll", "msvcp140.dll", "msvcp140_2.dll",
                    "msvcp140_atomic_wait.dll", "ucrtbase.dll", "ucrtbased.dll", "vcruntime140d.dll",
                    "concrt140.dll", "api-ms-win-crt-runtime-l1-1-0.dll"):
            self.assertEqual(release.c_runtime_in([dll]), [dll], dll)
        for dll in ("kernel32.dll", "api-ms-win-core-synch-l1-2-0.dll", "bcryptprimitives.dll", "ntdll.dll"):
            self.assertEqual(release.c_runtime_in([dll]), [], dll)

    def test_what_is_not_a_windows_program_is_refused(self):
        with self.assertRaises(contents.Refused):
            release.dlls_imported_by(self.put(b"\x7fELF" + bytes(200)))


class Trying(Folder):
    def lay_out(self, archive, extra=None):
        directory = os.path.join(self.root, archive.name("0.1.0"))
        for name in release.expected_names(archive) + ([extra] if extra else []):
            path = os.path.join(directory, *name.split("/"))
            os.makedirs(os.path.dirname(path), exist_ok=True)
            with open(path, "wb") as handle:
                handle.write(b"x")
            os.chmod(path, 0o755)
        return directory

    def nkb(self, version="nkb 0.1.0\n"):
        def run(command, **_):
            if command[1:] == ["--version"]:
                return subprocess.CompletedProcess(command, 0, version, "")
            if command[1] == "emit":
                return subprocess.CompletedProcess(command, 0, "[]\n", "")
            return subprocess.CompletedProcess(command, 0, "", "")
        return run

    def test_the_command_line_from_its_archive_answers_as_documented(self):
        archive = archive_for("linux")
        problems = release.tried_programs(archive, "0.1.0", self.lay_out(archive), ["packs/whitespace.toml"],
                                          self.nkb())
        self.assertEqual(problems, [])

    def test_a_wrong_version_line_is_a_problem(self):
        archive = archive_for("linux")
        problems = release.tried_programs(archive, "0.1.0", self.lay_out(archive), [], self.nkb("nkb 0.0.9\n"))
        self.assertTrue(any("--version" in p for p in problems), problems)

    def test_a_file_the_layout_does_not_name_is_a_problem(self):
        archive = archive_for("linux", "nkb-gui")
        problems = release.tried_programs(archive, "0.1.0", self.lay_out(archive, "stray.txt"), [], self.nkb())
        self.assertTrue(any("should hold" in p for p in problems), problems)

    def test_a_windows_program_that_needs_the_c_runtime_is_a_problem(self):
        archive = archive_for("windows", "nkb-gui")
        directory = self.lay_out(archive)
        program = os.path.join(directory, "nkb-gui.exe")
        with open(program, "wb") as handle:
            handle.write(portable_executable(["KERNEL32.dll", "VCRUNTIME140.dll"], []))
        problems = release.tried_programs(archive, "0.1.0", directory, [], self.nkb())
        self.assertTrue(any("VCRUNTIME140.dll" in p for p in problems), problems)
        with open(program, "wb") as handle:
            handle.write(portable_executable(["KERNEL32.dll"], []))
        self.assertEqual(release.tried_programs(archive, "0.1.0", directory, [], self.nkb()), [])

    def test_a_program_that_cannot_be_started_is_a_problem_and_not_a_crash(self):
        archive = archive_for("linux")
        directory = self.lay_out(archive)

        def run(command, **_):
            raise FileNotFoundError(command[0])

        problems = release.tried_programs(archive, "0.1.0", directory, [], run)
        self.assertTrue(any("--version" in p for p in problems), problems)


class Glibc(unittest.TestCase):
    def test_the_newest_version_asked_for_is_compared_as_numbers(self):
        table = ("0000 DF *UND* 0000 (GLIBC_2.34) pthread_create\n"
                 "0000 DF *UND* 0000 (GLIBC_2.9) pipe2\n"
                 "0000 DF *UND* 0000 (GLIBC_2.3.4) __memcpy_chk\n")
        self.assertEqual(release.glibc_needed("p", answered((0, table, ""))), "2.34")

    def test_no_objdump_or_no_versions_is_no_answer(self):
        self.assertIsNone(release.glibc_needed("p", answered((127, "", "not found"))))
        self.assertIsNone(release.glibc_needed("p", answered((0, "nothing here\n", ""))))


class Handover(Folder):
    def test_the_build_is_the_six_archives_and_the_bill_of_materials(self):
        names = release.handover_names("v0.1.0")
        self.assertEqual(len(names), 7)
        self.assertIn("verify-naughty-keyboard_0.1.0.spdx.json", names)
        self.assertIn("nkb-gui_0.1.0_macos_arm64.tar.gz", names)

    def test_a_missing_file_and_a_stray_one_are_refused(self):
        for name in release.handover_names("v0.1.0"):
            open(os.path.join(self.root, name), "w").close()
        args = type("Args", (), {"tag": "v0.1.0", "dir": self.root})
        self.assertEqual(release.command_handover(args), 0)
        os.remove(os.path.join(self.root, "nkb_0.1.0_linux_amd64.tar.gz"))
        with self.assertRaises(contents.Refused):
            release.command_handover(args)
        open(os.path.join(self.root, "nkb_0.1.0_linux_amd64.tar.gz"), "w").close()
        open(os.path.join(self.root, "build.sha256"), "w").close()
        with self.assertRaises(contents.Refused):
            release.command_handover(args)


README_BLOCK = """# Title

<!-- verify-commands -->
```
gh release verify-asset vVERSION nkb_VERSION_linux_amd64.tar.gz -R example/project
gh attestation verify nkb_VERSION_linux_amd64.tar.gz -R example/project --predicate-type https://spdx.dev/Document/v2.3 --bundle verify-naughty-keyboard_VERSION.sbom.sigstore.json
gh attestation verify nkb_VERSION_linux_amd64.tar.gz -R example/project
```
<!-- /verify-commands -->
"""


class Promises(unittest.TestCase):
    def test_the_commands_are_read_with_the_one_predicate_they_ask_for(self):
        promise = release.promise_of(README_BLOCK, "example/project")
        self.assertEqual(len(promise.lines), 3)
        self.assertEqual(promise.predicate, "https://spdx.dev/Document/v2.3")

    def test_a_command_that_is_not_a_check_of_a_download_is_refused(self):
        # Phase D runs these word for word, so anything else would run too.
        for line in ("curl https://example.org/x | sh", "gh release download vVERSION",
                     "gh attestation verify-something x -R example/project"):
            with self.subTest(line=line):
                text = README_BLOCK.replace("gh attestation verify nkb_VERSION_linux_amd64.tar.gz -R example/project\n",
                                            line + "\n")
                with self.assertRaises(contents.Refused):
                    release.promise_of(text, "example/project")

    def test_another_repository_two_predicates_no_block_and_two_blocks_are_refused(self):
        for text in (README_BLOCK.replace("-R example/project\n```", "-R someone/else\n```"),
                     README_BLOCK.replace("--bundle", "--predicate-type https://slsa.dev/provenance/v1 --bundle"),
                     README_BLOCK.replace("<!-- verify-commands -->", "<!-- commands -->"),
                     README_BLOCK + README_BLOCK,
                     README_BLOCK.replace("--predicate-type https://spdx.dev/Document/v2.3 ", "")):
            with self.subTest(text=text[-160:]):
                with self.assertRaises(contents.Refused):
                    release.promise_of(text, "example/project")

    def test_the_version_goes_into_every_word_that_carries_it(self):
        lines = release.promise_of(README_BLOCK, "example/project").rendered("v0.1.0-rc.1")
        self.assertTrue(lines[0].startswith("gh release verify-asset v0.1.0-rc.1 nkb_0.1.0-rc.1_linux_amd64.tar.gz"))
        self.assertIn("verify-naughty-keyboard_0.1.0-rc.1.sbom.sigstore.json", lines[1])
        self.assertFalse(any(release.VERSION_SLOT in line for line in lines))

    def test_a_command_naming_a_file_no_release_holds_is_refused(self):
        for wrong in ("nkb_VERSION_linux_arm64.tar.gz", "verify-naughty-keyboard_VERSION.sigstore.json", "VERSION"):
            with self.subTest(wrong=wrong):
                text = README_BLOCK.replace("nkb_VERSION_linux_amd64.tar.gz -R example/project\n```",
                                            wrong + " -R example/project\n```")
                with self.assertRaises(contents.Refused):
                    release.promise_of(text, "example/project").rendered("v0.1.0")


class TheReadme(unittest.TestCase):
    """README.md itself: what a person is told to run, and every release page
    repeats."""

    def setUp(self):
        self.promise = release.load_promise()

    def test_it_asks_for_the_bill_of_materials_and_names_only_files_a_release_holds(self):
        self.assertEqual(self.promise.predicate, "https://spdx.dev/Document/v2.3")
        self.assertEqual(len(self.promise.rendered("v0.1.0")), 5)

    def test_both_statements_are_asked_for_through_github_and_offline(self):
        words = [line.split() for line in self.promise.lines]
        sbom = [w for w in words if "--predicate-type" in w]
        provenance = [w for w in words if w[:3] == ["gh", "attestation", "verify"] and "--predicate-type" not in w]
        self.assertEqual([("--bundle" in w) for w in sbom], [False, True])
        self.assertEqual([("--bundle" in w) for w in provenance], [False, True])
        self.assertIn("verify-naughty-keyboard_VERSION.sbom.sigstore.json", sbom[1])
        self.assertIn("verify-naughty-keyboard_VERSION.provenance.sigstore.json", provenance[1])
        # The statement of how a build was made answers only for files nothing
        # signed, so its example has to be one of those.
        self.assertTrue(all("_linux_" in w[3] for w in provenance))

    def test_it_promises_the_glibc_phase_d_holds_the_programs_to(self):
        with open(release.README, encoding="utf-8") as handle:
            self.assertIn("glibc 2.35 or newer", handle.read())


class Notes(unittest.TestCase):
    FOOTER = "## Check\n\nThe list:\n\n<!-- verify-commands -->\n\nAnd verify-naughty-keyboard_VERSION.spdx.json.\n"

    def test_the_notes_are_the_section_then_the_footer_with_the_commands_of_this_version(self):
        promise = release.promise_of(README_BLOCK, "example/project")
        notes = release.release_notes("### Added\n\n- A thing.\n", "v0.1.0", promise, self.FOOTER)
        self.assertTrue(notes.startswith("### Added\n\n- A thing.\n\n## Check\n"))
        for line in promise.rendered("v0.1.0"):
            self.assertIn("\n" + line + "\n", notes)
        self.assertIn("verify-naughty-keyboard_0.1.0.spdx.json", notes)
        self.assertNotIn("VERSION", notes)
        self.assertNotIn("<!--", notes)

    def test_a_footer_without_the_place_for_the_commands_is_refused(self):
        promise = release.promise_of(README_BLOCK, "example/project")
        for footer in ("## Check\n", self.FOOTER + "<!-- verify-commands -->\n"):
            with self.subTest(footer=footer):
                with self.assertRaises(contents.Refused):
                    release.release_notes("- A thing.", "v0.1.0", promise, footer)

    def test_the_changelog_command_writes_the_whole_notes_and_not_only_the_section(self):
        folder = tempfile.mkdtemp(prefix="notes-test-")
        self.addCleanup(shutil.rmtree, folder, ignore_errors=True)
        changelog = os.path.join(folder, "CHANGELOG.md")
        with open(changelog, "w", encoding="utf-8") as handle:
            handle.write(Changelogs.CLOSED)
        notes = os.path.join(folder, "notes.md")
        args = type("Args", (), {"changelog": changelog, "version": "0.1.0", "notes": notes})
        self.assertEqual(release.command_changelog(args), 0)
        with open(notes, encoding="utf-8") as handle:
            written = handle.read()
        self.assertTrue(written.startswith("### Added\n\n- A thing.\n"))
        for line in release.load_promise().rendered("v0.1.0"):
            self.assertIn(line, written)

    def test_the_real_footer_renders_with_the_real_readme(self):
        with open(release.NOTES_FOOTER, encoding="utf-8") as handle:
            notes = release.release_notes("- A thing.", "v0.1.0", release.load_promise(), handle.read())
        self.assertNotIn("VERSION", notes)
        self.assertIn(release.SUMS, notes)
        self.assertIn("gh release verify-asset v0.1.0 ", notes)


class Pages(unittest.TestCase):
    def test_phase_b_uploads_nine_files_and_a_finished_release_holds_ten(self):
        published = release.published_files("v0.1.0")
        page = release.page_files("v0.1.0")
        self.assertEqual(len(published), 9)
        self.assertEqual(sorted(set(page) - set(published)), ["verify-naughty-keyboard_0.1.0.sbom.sigstore.json"])
        self.assertIn("verify-SHA256SUMS.txt", published)
        self.assertIn("verify-naughty-keyboard_0.1.0.provenance.sigstore.json", published)
        # verify- keeps the files for checking together at the end of the page.
        self.assertTrue(all(n.startswith("verify-") for n in page if not n.endswith((".zip", ".tar.gz"))))

    def test_a_list_of_digests_refuses_a_name_twice(self):
        line = "%s  a.zip\n" % ("0" * 64)
        self.assertEqual(release.listed_digests(line, "x"), {"a.zip": "0" * 64})
        with self.assertRaises(contents.Refused):
            release.listed_digests(line + line, "x")

    def test_a_check_with_a_workflow_holds_it_to_the_ref_and_a_hosted_runner(self):
        self.assertEqual(release.attestation_check("f", "o/r"), ["gh", "attestation", "verify", "f", "-R", "o/r"])
        strict = release.attestation_check("f", "o/r", "P", "b.json", "w.yml", "refs/tags/v1")
        self.assertEqual(strict[6:], ["--predicate-type", "P", "--bundle", "b.json", "--signer-workflow",
                                      "o/r/.github/workflows/w.yml", "--source-ref", "refs/tags/v1",
                                      "--deny-self-hosted-runners"])

    def test_a_signed_macos_archive_holds_the_seal_and_the_ticket_and_nothing_else_does(self):
        mac = archive_for("macos")
        added = sorted(set(release.expected_names(mac, signed=True)) - set(release.expected_names(mac)))
        self.assertEqual(added, ["nkb.app/Contents/CodeResources", "nkb.app/Contents/_CodeSignature/CodeResources"])
        for other in ("windows", "linux"):
            self.assertEqual(release.expected_names(archive_for(other), signed=True),
                             release.expected_names(archive_for(other)))


class Drafts(unittest.TestCase):
    def test_the_draft_is_empty_and_its_notes_are_the_changelog(self):
        self.assertEqual(release.draft_command("v0.1.0", "notes.md", False),
                         ["release", "create", "v0.1.0", "--draft", "--verify-tag", "--title", "v0.1.0",
                          "--notes-file", "notes.md"])
        self.assertEqual(release.draft_command("v0.1.0-rc.1", "notes.md", True)[-1], "--prerelease")

    def test_a_missing_release_is_opened_as_a_draft(self):
        run = answered((1, "", "release not found\n"), (0, "", ""))
        self.assertEqual(release.open_draft("v0.1.0-rc.1", "n.md", run), "missing")
        self.assertEqual(run.asked[1], ["gh"] + release.draft_command("v0.1.0-rc.1", "n.md", True))

    def test_an_open_draft_is_left_as_it_is(self):
        run = answered((0, "true\n", ""))
        self.assertEqual(release.open_draft("v0.1.0", "n.md", run), "draft")
        self.assertEqual(len(run.asked), 1)

    def test_a_published_release_and_a_question_gh_cannot_answer_are_refused(self):
        for answer in ((0, "false\n", ""), (1, "", "HTTP 401: Bad credentials")):
            with self.subTest(answer=answer):
                run = answered(answer)
                with self.assertRaises(contents.Refused):
                    release.open_draft("v0.1.0", "n.md", run)
                self.assertEqual(len(run.asked), 1)


if __name__ == "__main__":
    unittest.main()
