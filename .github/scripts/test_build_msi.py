"""Tests for build_msi.py, the Windows installer.

Run from the root of the repository:

    python3 -m unittest discover -s .github/scripts -p "test_build_msi.py"

What the installer DOES on a machine is asked by try_installer.py, on a runner
in release.yml and by hand on a machine of one's own. These hold what that run
cannot see coming: the lines each of its answers rests on, a value that must
never change, and every refusal of the script. Every refusal about the inputs
comes before WiX is looked for, so they run on every system. The one test that
builds a real installer runs where WiX and msi.dll are, and says so where they
are not.
"""
import contextlib
import io
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
import unittest.mock
import xml.etree.ElementTree as ElementTree
import zipfile

import build_msi
import contents
import release

# The product's identity in Windows Installer, for good. Every machine that has
# the program finds the version it has through it, so a new one would leave the
# old install beside the new one on every one of them. Written here a second
# time on purpose: this is the copy that does not move when the script does.
UPGRADE_CODE = "C64D1A47-17D7-43FE-BF82-76CC3DBA4981"
WIX = "{http://wixtoolset.org/schemas/v4/wxs}"
VERSION = contents.load_workspace().version
TAG = "v" + VERSION


def setUpModule():
    unittest.mock.patch.object(build_msi, "say", lambda text: None).start()


def tearDownModule():
    unittest.mock.patch.stopall()


def rendered(tag=TAG):
    return ElementTree.fromstring(build_msi.source(tag, build_msi.installer_version(tag)).encode("utf-8"))


def named(root, name):
    return [e for e in root.iter() if e.tag == WIX + name]


class TheSource(unittest.TestCase):
    """The lines the measurements rest on, read from the rendered source.

    Each one changed would still build an installer that installs - which is
    why it needs a test rather than a build. What breaks is an upgrade, an
    uninstall, or the PATH of every account, on somebody else's machine."""

    def setUp(self):
        self.root = rendered()

    def test_it_is_read_as_xml_and_every_placeholder_is_filled(self):
        self.assertGreater(len(list(self.root.iter())), 15)
        self.assertNotIn("{{", build_msi.source(TAG, VERSION))

    def test_no_extension_no_custom_action_and_nothing_that_ends_a_program(self):
        text = build_msi.source(TAG, VERSION)
        self.assertNotRegex(text, r"xmlns:\w+=")
        for element in self.root.iter():
            with self.subTest(element=element.tag):
                self.assertTrue(element.tag.startswith(WIX), element.tag)
                self.assertNotIn(element.tag[len(WIX):], ("CustomAction", "CloseApplication", "InstallExecuteSequence",
                                                          "UI", "UIRef", "Binary", "Upgrade"))

    def test_the_upgrade_code_is_the_products_for_good_and_it_installs_for_the_machine(self):
        package = named(self.root, "Package")
        self.assertEqual(len(package), 1)
        self.assertEqual(package[0].get("UpgradeCode"), UPGRADE_CODE)
        self.assertEqual(build_msi.UPGRADE_CODE, UPGRADE_CODE)
        self.assertEqual(package[0].get("Scope"), "perMachine")
        self.assertEqual(package[0].get("Version"), VERSION)

    def test_a_rebuild_replaces_the_files_of_its_version_and_an_older_version_is_refused(self):
        upgrade = named(self.root, "MajorUpgrade")
        self.assertEqual(len(upgrade), 1)
        self.assertEqual(upgrade[0].get("Schedule"), "afterInstallInitialize")
        self.assertEqual(upgrade[0].get("AllowSameVersionUpgrades"), "yes")
        self.assertIn("A newer version of [ProductName] is already installed", upgrade[0].get("DowngradeErrorMessage"))

    def test_nothing_running_is_closed_and_there_is_nothing_to_repair_or_modify(self):
        properties = {p.get("Id"): p.get("Value") for p in named(self.root, "Property")}
        self.assertEqual(properties.get("MSIRESTARTMANAGERCONTROL"), "Disable")
        self.assertEqual((properties.get("ARPNOREPAIR"), properties.get("ARPNOMODIFY")), ("1", "1"))
        for key in ("ARPURLINFOABOUT", "ARPHELPLINK", "ARPURLUPDATEINFO"):
            self.assertTrue(properties.get(key, "").startswith("https://"), key)

    def test_the_folder_goes_on_the_machine_path_once_last_and_comes_off_again(self):
        environment = named(self.root, "Environment")
        self.assertEqual(len(environment), 1)
        for attribute, wanted in (("Name", "PATH"), ("Value", "[INSTALLFOLDER]"), ("Part", "last"),
                                  ("Action", "set"), ("System", "yes"), ("Permanent", "no")):
            self.assertEqual(environment[0].get(attribute), wanted, attribute)

    def test_one_start_menu_entry_starts_the_palette_in_its_folder(self):
        shortcuts = named(self.root, "Shortcut")
        self.assertEqual(len(shortcuts), 1)
        installer = contents.load_installer()
        self.assertEqual(shortcuts[0].get("Target"), "[INSTALLFOLDER]%s.exe" % installer.shortcut)
        self.assertEqual(shortcuts[0].get("WorkingDirectory"), "INSTALLFOLDER")
        self.assertEqual(shortcuts[0].get("Name"), installer.product)

    def test_the_folder_is_the_product_directly_under_program_files(self):
        standard = [d for d in named(self.root, "StandardDirectory") if d.get("Id") == "ProgramFiles64Folder"]
        self.assertEqual(len(standard), 1)
        folders = [d for d in standard[0] if d.tag == WIX + "Directory"]
        self.assertEqual([(d.get("Id"), d.get("Name")) for d in folders],
                         [("INSTALLFOLDER", contents.load_installer().product)])

    def test_what_a_person_reads_follows_the_punctuation_rule(self):
        # A flat hyphen and no semicolon: the rule of every file of this
        # repository, and these are the sentences the installer shows.
        forbidden = (";", chr(0x2013), chr(0x2014))
        read = []
        for element in self.root.iter():
            for attribute in ("DowngradeErrorMessage", "Description", "Name"):
                if element.get(attribute) and element.tag in (WIX + "MajorUpgrade", WIX + "Shortcut", WIX + "Package"):
                    read.append(element.get(attribute))
        self.assertGreaterEqual(len(read), 4)
        for text in read:
            with self.subTest(text=text):
                self.assertFalse(any(mark in text for mark in forbidden))

    def test_the_product_name_printed_for_the_signature_is_the_name_in_the_package(self):
        out = io.StringIO()
        with contextlib.redirect_stdout(out), unittest.mock.patch.object(build_msi, "say", print):
            self.assertEqual(build_msi.main(["--tag", TAG, "--product-name"]), 0)
        self.assertEqual(out.getvalue().strip(), named(self.root, "Package")[0].get("Name"))


class Versions(unittest.TestCase):
    def test_a_candidate_gets_no_installer_unless_it_is_a_trial(self):
        with self.assertRaises(contents.Refused) as refusal:
            build_msi.installer_version(TAG + "-rc.1")
        self.assertIn("release candidate", refusal.exception.problems[0])
        self.assertEqual(build_msi.installer_version(TAG + "-dev.7", trial="0.0.2"), "0.0.2")

    def test_a_version_windows_installer_cannot_hold_is_refused_and_the_largest_one_is_not(self):
        for trial in ("256.0.0", "0.256.0", "0.0.65536", "1.2", "1.2.3.4", "01.2.3", "a.b.c", ""):
            with self.subTest(trial=trial):
                with self.assertRaises(contents.Refused):
                    build_msi.installer_version(TAG, trial=trial)
        self.assertEqual(build_msi.installer_version(TAG, trial="255.255.65535"), "255.255.65535")

    def test_a_tag_of_another_version_is_refused(self):
        with self.assertRaises(contents.Refused):
            build_msi.installer_version("v9.9.9")

    def test_a_trial_is_named_so_that_no_release_carries_its_name(self):
        release_name = release.installer_name(TAG)
        self.assertEqual(build_msi.output_name(TAG), release_name)
        trial = build_msi.output_name(TAG, trial="0.0.1")
        self.assertNotEqual(trial, release_name)
        self.assertIn("-trial-0.0.1", trial)
        for tag in (TAG, TAG + "-rc.1", TAG + "-dev.7"):
            self.assertNotIn(build_msi.output_name(tag, trial=VERSION), release.page_files(tag))


class Rendering(unittest.TestCase):
    TEMPLATE = '<a b="{{ONE}}" c="{{TWO}}"/>'

    def test_a_value_is_escaped_for_the_attribute_it_lands_in(self):
        text = build_msi.render(self.TEMPLATE, {"ONE": 'a "quoted" & <odd> one', "TWO": "x"})
        self.assertEqual(ElementTree.fromstring(text).get("b"), 'a "quoted" & <odd> one')

    def test_a_placeholder_nobody_gives_a_value_nobody_asks_for_and_a_broken_value_are_refused(self):
        for table, says in (({"ONE": "x"}, "asks for TWO"),
                            ({"ONE": "x", "TWO": "y", "THREE": "z"}, "uses no THREE"),
                            ({"ONE": "x", "TWO": "two\nlines"}, "spans lines"),
                            ({"ONE": "x", "TWO": ""}, "empty")):
            with self.subTest(says=says):
                with self.assertRaises(contents.Refused) as refusal:
                    build_msi.render(self.TEMPLATE, table)
                self.assertTrue(any(says in p for p in refusal.exception.problems), refusal.exception.problems)

    def test_the_publisher_is_the_one_author_of_the_workspace(self):
        publisher, homepage = build_msi.workspace_words()
        self.assertEqual(publisher, "DonislawDev")
        self.assertTrue(homepage.startswith("https://"))


def write_zip(path, files):
    with zipfile.ZipFile(path, "w") as out:
        for name, data in sorted(files.items()):
            out.writestr(name, data)


class Archives(unittest.TestCase):
    """A folder of the archives of the installer's platform, made up."""

    def setUp(self):
        self.root = tempfile.mkdtemp(prefix="msi-test-")
        self.addCleanup(shutil.rmtree, self.root, ignore_errors=True)
        self.installer = contents.load_installer()
        self.archives = {a.program: a for a in contents.load_archives()
                         if (a.os, a.arch) == (self.installer.os, self.installer.arch)}

    def good(self):
        return {program: {name: (b"the same %s" % name.encode()) if name in release.BESIDE else
                          (b"%s of %s" % (name.encode(), program.encode()))
                          for name in release.expected_names(archive)}
                for program, archive in self.archives.items()}

    def write(self, sets, version=VERSION):
        for program, files in sets.items():
            write_zip(os.path.join(self.root, self.archives[program].name(version)), files)


class Payload(Archives):
    """The two archives laid out as the installer puts them down."""

    def test_both_programs_once_each_shared_file_once_and_each_programs_notices_by_name(self):
        self.write(self.good())
        files = build_msi.payload_files(self.root, TAG)
        self.assertEqual(sorted(files), sorted(["LICENSE", "README.md", "nkb.exe", "nkb-gui.exe",
                                                "THIRD-PARTY-NOTICES-nkb.txt", "THIRD-PARTY-NOTICES-nkb-gui.txt"]))
        self.assertEqual(files["THIRD-PARTY-NOTICES-nkb.txt"], b"THIRD-PARTY-NOTICES.txt of nkb")
        self.assertEqual(files["THIRD-PARTY-NOTICES-nkb-gui.txt"], b"THIRD-PARTY-NOTICES.txt of nkb-gui")

    def test_an_archive_that_is_not_the_one_a_release_packs_is_refused(self):
        def missing_program(sets):
            sets["nkb"].pop("nkb.exe")

        def stray_file(sets):
            sets["nkb-gui"]["../escaped.txt"] = b"out"

        def other_licence(sets):
            sets["nkb-gui"]["LICENSE"] = b"another licence"

        for change, says in ((missing_program, "holds"),
                             (stray_file, "holds"),
                             (other_licence, "not the same bytes")):
            with self.subTest(change=change.__name__):
                sets = self.good()
                change(sets)
                self.write(sets)
                with self.assertRaises(contents.Refused) as refusal:
                    build_msi.payload_files(self.root, TAG)
                self.assertTrue(any(says in p for p in refusal.exception.problems), refusal.exception.problems)
        self.assertFalse(os.path.exists(os.path.join(os.path.dirname(self.root), "escaped.txt")))

    def test_a_missing_archive_and_one_that_is_not_a_zip_are_refused(self):
        sets = self.good()
        sets.pop("nkb")
        self.write(sets)
        with self.assertRaises(contents.Refused) as refusal:
            build_msi.payload_files(self.root, TAG)
        self.assertIn("holds no nkb_", refusal.exception.problems[0])
        with open(os.path.join(self.root, self.archives["nkb"].name(VERSION)), "wb") as handle:
            handle.write(b"not a zip")
        with self.assertRaises(contents.Refused) as refusal:
            build_msi.payload_files(self.root, TAG)
        self.assertIn("not a zip", refusal.exception.problems[0])

    def test_every_refusal_comes_before_wix_and_leaves_nothing_behind(self):
        self.write(self.good())
        out = os.path.join(self.root, "out")
        os.makedirs(out)
        temp = os.path.join(self.root, "temp")
        os.makedirs(temp)
        with unittest.mock.patch.object(tempfile, "tempdir", temp):
            for change, says in (
                    (lambda: shutil.rmtree(out), "is not a folder"),
                    (lambda: open(os.path.join(out, build_msi.output_name(TAG)), "w").close(), "already there"),
                    (lambda: os.remove(os.path.join(self.root, self.archives["nkb"].name(VERSION))), "holds no")):
                with self.subTest(says=says):
                    shutil.rmtree(out, ignore_errors=True)
                    os.makedirs(out)
                    change()
                    with unittest.mock.patch.object(build_msi, "find_wix",
                                                    side_effect=AssertionError("WiX was looked for")):
                        with self.assertRaises(contents.Refused) as refusal:
                            build_msi.build(TAG, self.root, out)
                    self.assertTrue(any(says in p for p in refusal.exception.problems), refusal.exception.problems)
                    if says != "already there":
                        self.assertEqual(os.listdir(out) if os.path.isdir(out) else [], [])
            self.assertEqual(os.listdir(temp), [])

    def test_wix_out_of_reach_or_of_another_version_is_refused_with_the_command_that_installs_it(self):
        install = "dotnet tool install --global wix --version %s" % build_msi.WIX_VERSION
        with self.assertRaises(contents.Refused) as refusal:
            build_msi.find_wix(which=lambda *a, **k: None, home=self.root)
        self.assertIn(install, refusal.exception.problems[0])
        self.assertNotIn("github.com", refusal.exception.problems[0])
        answer = lambda version: (lambda command, **kw: subprocess.CompletedProcess(command, 0, version + "+abc\n", ""))
        with self.assertRaises(contents.Refused) as refusal:
            build_msi.find_wix(which=lambda *a, **k: "wix", run=answer("6.0.0"))
        self.assertIn(install, refusal.exception.problems[0])
        self.assertEqual(build_msi.find_wix(which=lambda *a, **k: "wix", run=answer(build_msi.WIX_VERSION)), "wix")


class TheDatabase(unittest.TestCase):
    """What a built installer carries, asked of its database."""

    NAMES = ["LICENSE", "README.md", "nkb.exe"]

    def tables(self, **changes):
        tables = {
            "Property": [("ProductVersion", VERSION), ("UpgradeCode", "{%s}" % UPGRADE_CODE),
                         ("ProductName", "Naughty Keyboard"), ("MSIRESTARTMANAGERCONTROL", "Disable"),
                         ("ARPNOREPAIR", "1"), ("ARPNOMODIFY", "1"), ("ProductCode", "{X}")],
            "CustomAction": [], "Binary": [],
            "File": [("LICENSE",), ("README.md",), ("abcdefgh.exe|nkb.exe",)],
            "InstallExecuteSequence": [("InstallInitialize", "1500"), ("RemoveExistingProducts", "1501")],
        }
        tables.update(changes)
        return lambda query, fields, table: tables[table]

    def problems(self, **changes):
        return build_msi.database_problems(self.tables(**changes), VERSION, "Naughty Keyboard", self.NAMES)

    def test_an_installer_of_our_files_and_windows_installers_own_tables_passes(self):
        self.assertEqual(self.problems(), [])

    def test_each_thing_an_installer_must_not_carry_is_named(self):
        properties = dict(self.tables()("", 0, "Property"))
        for changes, says in (
                ({"Property": sorted(dict(properties, ProductVersion="0.0.1").items())}, "ProductVersion"),
                ({"Property": sorted(dict(properties, UpgradeCode="{0}").items())}, "UpgradeCode"),
                ({"Property": sorted(dict(properties, MSIRESTARTMANAGERCONTROL="0").items())}, "MSIRESTARTMANAGERCONTROL"),
                ({"CustomAction": [("CloseRunningApps",)]}, "CustomAction"),
                ({"Binary": [("WixCA",)]}, "Binary"),
                ({"File": [("LICENSE",), ("README.md",)]}, "lays down"),
                ({"InstallExecuteSequence": [("InstallInitialize", "1500"), ("RemoveExistingProducts", "6599")]},
                 "removed at 6599"),
                ({"Property": [("ProductVersion", VERSION)]}, "not reading it")):
            with self.subTest(says=says):
                found = self.problems(**changes)
                self.assertEqual(len(found), 1, found)
                self.assertIn(says, found[0])


def wix_and_msi():
    if not sys.platform.startswith("win"):
        return "msi.dll is on Windows"
    try:
        build_msi.find_wix()
    except contents.Refused:
        return "WiX %s is not installed here" % build_msi.WIX_VERSION
    return None


@unittest.skipIf(wix_and_msi(), wix_and_msi())
class ARealBuild(Archives):
    """The script end to end, with WiX, and the database read back by msi.dll:
    the only test here that can see the reader read a real file."""

    def test_a_trial_is_built_whole_and_its_database_says_what_the_source_says(self):
        self.write(self.good(), version=VERSION + "-dev.1")
        out = os.path.join(self.root, "out")
        os.makedirs(out)
        path = build_msi.build(TAG + "-dev.1", self.root, out, trial="0.0.3")
        self.assertEqual(os.listdir(out), [os.path.basename(path)])
        properties = dict(build_msi.database_rows(path, "SELECT `Property`, `Value` FROM `Property`", 2, "Property"))
        self.assertEqual(properties["ProductVersion"], "0.0.3")
        self.assertEqual(properties["UpgradeCode"], "{%s}" % UPGRADE_CODE)
        self.assertRegex(build_msi.product_code(path), r"^\{[0-9A-F-]{36}\}$")
        self.assertEqual(build_msi.database_rows(path, "SELECT * FROM `CustomAction`", 1, "CustomAction"), [])


if __name__ == "__main__":
    unittest.main()
