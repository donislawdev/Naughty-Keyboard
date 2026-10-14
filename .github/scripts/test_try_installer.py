"""Tests for try_installer.py, the run that asks a machine what the installer does.

Run from the root of the repository:

    python3 -m unittest discover -s .github/scripts -p "test_try_installer.py"

What the installer does is asked by the run itself, on a runner and on a
machine of one's own. These hold the run: that its kit can tell a rebuild that
replaced its files from one that did not, that it reads what it reads without
calling a difference nothing, that it fails rather than passes when it cannot
ask, and that it never writes the machine PATH itself.
"""
import json
import os
import re
import shutil
import sys
import tempfile
import types
import unittest
import unittest.mock

import build_msi
import release
import try_installer

HERE = os.path.dirname(os.path.abspath(__file__))


def code_of(name):
    """A script without its comments and docstrings."""
    with open(os.path.join(HERE, name), encoding="utf-8") as handle:
        text = re.sub(r'(?s)""".*?"""', '""', handle.read())
    return "\n".join(line.split("#", 1)[0] for line in text.splitlines())


class Quiet(unittest.TestCase):
    def setUp(self):
        self.report = try_installer.Report()
        patcher = unittest.mock.patch("builtins.print", lambda *a, **k: None)
        patcher.start()
        self.addCleanup(patcher.stop)


class TheKit(unittest.TestCase):
    def test_old_is_older_than_new_and_same_is_new_rebuilt(self):
        versions = dict(try_installer.TRIALS)
        self.assertEqual(sorted(versions), ["new", "old", "same"])
        as_numbers = {k: tuple(int(n) for n in v.split(".")) for k, v in versions.items()}
        self.assertLess(as_numbers["old"], as_numbers["new"])
        self.assertEqual(versions["new"], versions["same"])
        for trial in versions.values():
            self.assertEqual(build_msi.fitting(trial, "trial"), trial)

    def test_the_middle_installer_changes_the_programs_and_the_readme_and_nothing_else(self):
        files = {"nkb.exe": b"cli", "nkb-gui.exe": b"gui", "README.md": b"read", "LICENSE": b"licence",
                 "THIRD-PARTY-NOTICES-nkb.txt": b"notices"}
        altered = try_installer.changed(files, ["nkb.exe", "nkb-gui.exe"])
        self.assertEqual(sorted(n for n in files if altered[n] != files[n]), ["README.md", "nkb-gui.exe", "nkb.exe"])
        for name in ("nkb.exe", "nkb-gui.exe", "README.md"):
            self.assertTrue(altered[name].startswith(files[name]))
        self.assertEqual(files["nkb.exe"], b"cli")

    def test_no_installer_of_the_kit_carries_the_name_of_a_release(self):
        for tag in ("v0.1.0", "v0.1.0-rc.1", "v0.1.0-dev.7"):
            names = {build_msi.output_name(tag, trial=trial) for _, trial in try_installer.TRIALS}
            self.assertFalse(names & set(release.page_files(tag)))

    def test_the_held_run_writes_more_than_a_pipe_holds(self):
        # emit of this pack prints its longest value, and a pipe nobody reads
        # holds far less, so the run stays in progress until it is read.
        self.assertEqual(try_installer.HELD[:2], ("emit", "length-bombs"))
        with open(os.path.join(build_msi.ROOT, "packs", "length-bombs.toml"), encoding="utf-8") as handle:
            counts = [int(n) for n in re.findall(r"^count\s*=\s*(\d+)$", handle.read(), re.MULTILINE)]
        self.assertGreaterEqual(max(counts), try_installer.HELD_OUTPUT)


class ThePath(Quiet):
    BEFORE = r"C:\Windows\system32;C:\Windows;%SystemRoot%\System32\Wbem;C:\Tools;"

    def test_the_folder_is_found_on_path_once_whatever_its_case_or_trailing_separator(self):
        folder = r"C:\Program Files\Naughty Keyboard"
        value = r"C:\Windows;c:\program files\naughty keyboard\;C:\Tools"
        self.assertEqual(len(try_installer.ours_on_path(folder, value)), 1)
        self.assertEqual(try_installer.ours_on_path(folder, r"C:\Windows;C:\Program Files\Naughty Keyboard2"), [])

    def test_the_same_path_and_one_that_lost_an_empty_entry_come_back_and_the_second_is_said(self):
        try_installer.path_came_back(self.report, self.BEFORE, 2, self.BEFORE, 2)
        self.assertEqual((self.report.failed, len(self.report.lines)), (0, 1))
        try_installer.path_came_back(self.report, self.BEFORE, 2, self.BEFORE.rstrip(";"), 2)
        self.assertEqual(self.report.failed, 0)
        self.assertTrue(self.report.lines[-1].startswith("note"))

    def test_a_path_without_references_written_back_as_plain_text_reads_the_same_and_is_said(self):
        # Measured on a GitHub runner: expandable before, plain text after the
        # uninstall, the same text, and not one %reference% in it.
        plain = r"C:\Windows\system32;C:\Windows;C:\Tools"
        try_installer.path_came_back(self.report, plain, try_installer.REG_EXPAND_SZ, plain, try_installer.REG_SZ)
        self.assertEqual(self.report.failed, 0)
        self.assertTrue(self.report.lines[-1].startswith("note"))
        self.assertIn("plain text", self.report.lines[-1])
        # The other way round is not what Windows Installer does, and is not let through.
        report = try_installer.Report()
        try_installer.path_came_back(report, plain, try_installer.REG_SZ, plain, try_installer.REG_EXPAND_SZ)
        self.assertEqual(report.failed, 1)

    def test_a_path_that_lost_an_entry_changed_order_or_kind_fails(self):
        # An entry left behind is the case of an installer whose PATH entry is
        # permanent, which no run of the kit can be allowed to leave on a machine.
        for after, kind in ((self.BEFORE + r"C:\Program Files\Naughty Keyboard" + "\\", 2),
                            (self.BEFORE.replace(r"C:\Tools;", ""), 2),
                            (r"C:\Windows;C:\Windows\system32;%SystemRoot%\System32\Wbem;C:\Tools;", 2),
                            (self.BEFORE, 1)):
            with self.subTest(after=after, kind=kind):
                report = try_installer.Report()
                try_installer.path_came_back(report, self.BEFORE, 2, after, kind)
                self.assertEqual(report.failed, 1)


class TheReport(Quiet):
    def test_a_failure_wins_and_a_check_not_made_is_never_a_pass(self):
        self.assertEqual(self.report.code(), 0)
        self.report.unmeasured_because("a thing", "no desktop")
        self.assertEqual(self.report.code(), 3)
        self.report.check(False, "another thing")
        self.assertEqual(self.report.code(), 1)
        self.assertEqual([line.split()[0] for line in self.report.lines], ["NOT", "FAILED"])

    def test_the_log_is_read_in_utf16_with_its_mark_and_in_utf8_without(self):
        folder = tempfile.mkdtemp(prefix="try-test-")
        self.addCleanup(shutil.rmtree, folder, ignore_errors=True)
        sentence = "A newer version of Naughty Keyboard is already installed."
        for name, data in (("wide.log", b"\xff\xfe" + sentence.encode("utf-16-le")),
                           ("narrow.log", sentence.encode("utf-8"))):
            with open(os.path.join(folder, name), "wb") as handle:
                handle.write(data)
            self.assertIn(sentence, try_installer.log_text(os.path.join(folder, name)))
        self.assertEqual(try_installer.log_text(os.path.join(folder, "missing.log")), "")


class TheRun(Quiet):
    def test_it_stops_before_touching_anything_off_windows_or_without_the_rights(self):
        args = types.SimpleNamespace(cleanup=False, tag="v0.1.0-dev.7", archives="a", work="w", palette=False,
                                     kit_only=False, kit=None)
        with unittest.mock.patch.object(sys, "platform", "linux"):
            self.assertEqual(try_installer.run(args), 2)
        with unittest.mock.patch.object(sys, "platform", "win32"), \
                unittest.mock.patch.object(try_installer, "elevated", lambda: False):
            self.assertEqual(try_installer.run(args), 2)

    def test_a_run_needs_its_three_inputs_or_a_kit_or_cleanup(self):
        for argv in (["--tag", "v0.1.0-dev.7", "--archives", "a"],
                     ["--kit", "k"],
                     ["--kit", "k", "--work", "w", "--tag", "v0.1.0-dev.7"]):
            with self.subTest(argv=argv):
                with unittest.mock.patch("sys.stderr"):
                    with self.assertRaises(SystemExit):
                        try_installer.main(argv)

    def test_a_kit_built_here_has_whole_paths_and_reads_back_the_same_from_its_file(self):
        folder = tempfile.mkdtemp(prefix="try-kit-")
        self.addCleanup(shutil.rmtree, folder, ignore_errors=True)
        kit = os.path.join(folder, "kit")
        laid = {"nkb.exe": b"cli", "nkb-gui.exe": b"gui", "README.md": b"read"}

        def build(tag, archives, out, trial=None, files=None):
            path = os.path.join(out, "x-%s.msi" % trial)
            with open(path, "wb") as handle:
                handle.write(b"msi")
            return path

        with unittest.mock.patch.object(build_msi, "payload_files", lambda archives, tag: laid), \
                unittest.mock.patch.object(build_msi, "build", build), \
                unittest.mock.patch.object(build_msi, "product_code", lambda path: "{C}"):
            built = try_installer.build_kit("v0.1.0-dev.7", "archives", kit)
        for label, _ in try_installer.TRIALS:
            self.assertTrue(os.path.isabs(built["installers"][label]["path"]))
            self.assertTrue(os.path.isfile(built["installers"][label]["path"]))
        with open(os.path.join(kit, try_installer.KIT), encoding="utf-8") as handle:
            written = json.load(handle)
        self.assertFalse(any(os.path.isabs(e["path"]) for e in written["installers"].values()))
        self.assertEqual(try_installer.load_kit(kit), built)
        self.assertNotEqual(built["installers"]["new"]["files"]["nkb.exe"], built["installers"]["old"]["files"]["nkb.exe"])

    def test_a_kit_is_read_back_with_its_paths_whole_and_one_missing_an_installer_is_refused(self):
        folder = tempfile.mkdtemp(prefix="try-kit-")
        self.addCleanup(shutil.rmtree, folder, ignore_errors=True)
        with self.assertRaises(try_installer.Refused):
            try_installer.load_kit(folder)
        installers = {}
        for label, trial in try_installer.TRIALS:
            os.makedirs(os.path.join(folder, label))
            with open(os.path.join(folder, label, "x.msi"), "wb") as handle:
                handle.write(b"msi")
            installers[label] = {"path": os.path.join(label, "x.msi"), "version": trial, "code": "{C}", "files": {}}
        described = {"product": "P", "publisher": "Q", "programs_version": "0.1.0", "shortcut": "p.exe",
                     "installers": installers}
        with open(os.path.join(folder, try_installer.KIT), "w", encoding="utf-8") as handle:
            json.dump(described, handle)
        read = try_installer.load_kit(folder)
        self.assertEqual(read["installers"]["old"]["path"], os.path.join(folder, "old", "x.msi"))
        os.remove(os.path.join(folder, "same", "x.msi"))
        with self.assertRaises(try_installer.Refused):
            try_installer.load_kit(folder)

    def test_a_failed_first_install_leaves_every_later_step_not_measured(self):
        kit = {"old": {"path": "old.msi"}}
        with unittest.mock.patch.object(try_installer, "msiexec", lambda *a: (1603, 1.0)):
            try_installer.measure(types.SimpleNamespace(work="w", palette=False), self.report, kit,
                                  types.SimpleNamespace(folder="f", product="P"), [])
        self.assertEqual(self.report.failed, 1)
        self.assertEqual(self.report.unmeasured, len(try_installer.STEPS_AFTER))

    def test_it_never_writes_the_machine_path_and_ends_only_what_it_started(self):
        code = code_of("try_installer.py")
        for forbidden in ("SetValue", "setx", "SetEnvironmentVariable", "DeleteValue", "taskkill", "Stop-Process"):
            self.assertNotIn(forbidden, code)
        # Processes it ends are the ones it started and kept.
        self.assertIn("for process in processes:", code)
        self.assertIn("process.kill()", code)


if __name__ == "__main__":
    unittest.main()
