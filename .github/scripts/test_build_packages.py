"""Tests for build_packages.py, the WinGet and Chocolatey packages.

Run from the root of the repository:

    python3 -m unittest discover -s .github/scripts -p "test_build_packages.py"

What the packages DO on a machine is asked by installing them, the way
packaging/README.md describes. These hold what that run cannot see coming: the
identifiers that must never change, the lines each package rests on, and every
refusal of the renderer. A published installer's database is read through
msi.dll, which only Windows has, so these hand the renderer the properties such
an installer holds instead and run on every system.
"""
import contextlib
import hashlib
import io
import os
import re
import shutil
import tempfile
import unittest
import unittest.mock
import xml.etree.ElementTree as ElementTree

import build_msi
import build_packages
import contents

# A release that is dated in CHANGELOG.md for good, so the tests do not move
# when the workspace's version does.
TAG = "v0.3.0"
VERSION = "0.3.0"
PRODUCT_CODE = "{7A7FC4A0-783B-4D8E-A35E-985EC830232A}"

# The identifiers, written here a second time on purpose: every install of a
# package finds its upgrades by them, so this is the copy that does not move
# when the script does.
IDENTIFIERS = {
    "window": ("DonislawDev.NaughtyKeyboard", "naughty-keyboard"),
    "cli": ("DonislawDev.NaughtyKeyboard.CLI", "naughty-keyboard-cli"),
}
UPGRADE_CODE = "{C64D1A47-17D7-43FE-BF82-76CC3DBA4981}"

WINDOW = "winget/manifests/d/DonislawDev/NaughtyKeyboard/0.3.0/DonislawDev.NaughtyKeyboard"
CLI = "winget/manifests/d/DonislawDev/NaughtyKeyboard/CLI/0.3.0/DonislawDev.NaughtyKeyboard.CLI"
RENDERED = sorted([
    WINDOW + ".installer.yaml", WINDOW + ".locale.en-US.yaml", WINDOW + ".yaml",
    CLI + ".installer.yaml", CLI + ".locale.en-US.yaml", CLI + ".yaml",
    "chocolatey/naughty-keyboard/naughty-keyboard.nuspec",
    "chocolatey/naughty-keyboard/tools/chocolateyinstall.ps1",
    "chocolatey/naughty-keyboard/tools/chocolateyuninstall.ps1",
    "chocolatey/naughty-keyboard-cli/naughty-keyboard-cli.nuspec",
    "chocolatey/naughty-keyboard-cli/tools/chocolateyinstall.ps1",
])


def digest(text):
    return hashlib.sha256(text.encode()).hexdigest()


class Release:
    """A published release as the renderer sees it: its checksum file and its
    installer, in a folder of their own, with the installer's database answered
    by `properties` instead of msi.dll."""

    def __init__(self, folder, version=VERSION, properties=None, lines=None, msi_bytes=b"an installer"):
        self.folder = folder
        self.msi = os.path.join(folder, "nkb-setup_%s_windows_amd64.msi" % version)
        with open(self.msi, "wb") as handle:
            handle.write(msi_bytes)
        self.sums = {
            "nkb-gui_%s_windows_amd64.zip" % version: digest("palette"),
            "nkb_%s_windows_amd64.zip" % version: digest("command line"),
            "nkb-gui_%s_linux_amd64.tar.gz" % version: digest("linux"),
            os.path.basename(self.msi): hashlib.sha256(msi_bytes).hexdigest(),
        }
        self.sums_path = os.path.join(folder, "verify-SHA256SUMS.txt")
        with open(self.sums_path, "w", encoding="utf-8", newline="\n") as handle:
            handle.write("".join(lines if lines is not None else
                                 ("%s  %s\n" % (sha, name) for name, sha in self.sums.items())))
        self.properties = dict(properties or {
            "ProductCode": PRODUCT_CODE, "UpgradeCode": UPGRADE_CODE,
            "ProductVersion": version, "ProductName": "Naughty Keyboard", "Manufacturer": "DonislawDev",
        })

    def rows(self, path, query, fields, table):
        return list(self.properties.items())


@contextlib.contextmanager
def release(**kwargs):
    folder = tempfile.mkdtemp()
    try:
        made = Release(folder, **kwargs)
        with unittest.mock.patch.object(build_msi, "database_rows", made.rows):
            yield made
    finally:
        shutil.rmtree(folder, ignore_errors=True)


def refusal(test, call, *expected):
    """The problems a call refuses with, each expected piece found in one of them."""
    with test.assertRaises(contents.Refused) as caught:
        call()
    said = "\n".join(caught.exception.problems)
    for piece in expected:
        test.assertIn(piece, said)
    return said


def read(out, relative):
    with open(os.path.join(out, *relative.split("/")), encoding="utf-8", newline="") as handle:
        return handle.read()


class ThePackages(unittest.TestCase):
    """The lines every package rests on, read from one render of a release."""

    @classmethod
    def setUpClass(cls):
        cls.place = tempfile.mkdtemp()
        cls.out = os.path.join(cls.place, "rendered")
        with release() as made:
            cls.sums = made.sums
            build_packages.build(TAG, made.sums_path, cls.out, made.msi)

    @classmethod
    def tearDownClass(cls):
        shutil.rmtree(cls.place, ignore_errors=True)

    def files(self):
        return sorted(os.path.relpath(os.path.join(base, f), self.out).replace(os.sep, "/")
                      for base, _, names in os.walk(self.out) for f in names)

    def test_the_identifiers_never_change(self):
        self.assertEqual({p.kind: (p.winget_id, p.choco_id) for p in build_packages.PACKAGES}, IDENTIFIERS)

    def test_every_package_is_rendered_where_its_feed_takes_it(self):
        self.assertEqual(self.files(), RENDERED)

    def test_no_placeholder_is_left(self):
        for relative in self.files():
            self.assertNotIn("{{", read(self.out, relative), relative)

    def test_every_manifest_names_its_package_version_and_schema(self):
        for prefix, (winget_id, _) in ((WINDOW, IDENTIFIERS["window"]), (CLI, IDENTIFIERS["cli"])):
            for kind in (".installer.yaml", ".locale.en-US.yaml", ".yaml"):
                text = read(self.out, prefix + kind)
                self.assertIn("\nPackageIdentifier: %s\n" % winget_id, text)
                self.assertIn("\nPackageVersion: %s\n" % VERSION, text)
                self.assertIn("\nManifestVersion: %s\n" % build_packages.WINGET_SCHEMA, text)
                self.assertIn("schema.json", text.split("\n")[1])

    def test_the_palette_installs_the_installer_first_and_the_archive_second(self):
        # The order decides a fresh install, and the archive is what --scope user
        # and an install of the archive keep.
        text = "\n".join(line for line in read(self.out, WINDOW + ".installer.yaml").split("\n")
                         if not line.lstrip().startswith("#"))
        entries = re.split(r"\n- Architecture: ", text.split("\nInstallers:\n", 1)[1])
        self.assertEqual(len(entries), 2)
        installer, archive = entries
        self.assertIn("InstallerType: wix\n  Scope: machine\n  ElevationRequirement: elevatesSelf", installer)
        self.assertIn("/releases/download/%s/nkb-setup_%s_windows_amd64.msi" % (TAG, VERSION), installer)
        self.assertIn("ProductCode: '%s'" % PRODUCT_CODE, installer)
        self.assertIn("AppsAndFeaturesEntries:\n  - UpgradeCode: '%s'" % UPGRADE_CODE, installer)
        self.assertTrue(installer.endswith("Commands:\n  - nkb-gui\n  - nkb"), installer)
        self.assertIn("Commands:\n  - nkb-gui\nManifestType: installer", archive)
        self.assertIn("InstallerType: zip\n  NestedInstallerType: portable", archive)
        self.assertIn("RelativeFilePath: nkb-gui.exe", archive)
        self.assertNotIn("Scope:", archive)
        self.assertIn("/releases/download/%s/nkb-gui_%s_windows_amd64.zip" % (TAG, VERSION), archive)

    def test_both_archives_keep_the_program_where_it_is(self):
        for prefix in (WINDOW, CLI):
            self.assertIn("\n" + " " * (2 if prefix == WINDOW else 0) + "ArchiveBinariesDependOnPath: true\n",
                          read(self.out, prefix + ".installer.yaml"))
        cli = read(self.out, CLI + ".installer.yaml")
        self.assertIn("RelativeFilePath: nkb.exe", cli)
        self.assertIn("/releases/download/%s/nkb_%s_windows_amd64.zip" % (TAG, VERSION), cli)

    def test_each_installer_carries_the_checksum_of_its_file(self):
        window = read(self.out, WINDOW + ".installer.yaml")
        cli = read(self.out, CLI + ".installer.yaml")
        msi = "nkb-setup_%s_windows_amd64.msi" % VERSION
        self.assertIn("InstallerSha256: %s" % self.sums[msi].upper(), window)
        self.assertIn("InstallerSha256: %s" % self.sums["nkb-gui_%s_windows_amd64.zip" % VERSION].upper(), window)
        self.assertIn("InstallerSha256: %s" % self.sums["nkb_%s_windows_amd64.zip" % VERSION].upper(), cli)
        choco = read(self.out, "chocolatey/naughty-keyboard/tools/chocolateyinstall.ps1")
        self.assertIn("checksum64     = '%s'" % self.sums[msi].upper(), choco)

    def test_chocolatey_runs_the_installer_for_the_palette_and_unpacks_the_command_line(self):
        window = read(self.out, "chocolatey/naughty-keyboard/tools/chocolateyinstall.ps1")
        self.assertIn("fileType       = 'msi'", window)
        self.assertIn("Install-ChocolateyPackage @packageArgs", window)
        self.assertIn("softwareName   = 'Naughty Keyboard'", window)
        uninstall = read(self.out, "chocolatey/naughty-keyboard/tools/chocolateyuninstall.ps1")
        self.assertIn("$upgradeCode = '%s'" % UPGRADE_CODE, uninstall)
        cli = read(self.out, "chocolatey/naughty-keyboard-cli/tools/chocolateyinstall.ps1")
        self.assertIn("Install-ChocolateyZipPackage", cli)
        self.assertIn("/nkb_%s_windows_amd64.zip'" % VERSION, cli)

    def test_every_script_is_ascii_for_windows_powershell(self):
        for relative in self.files():
            if relative.endswith(".ps1"):
                build_packages.check_script(read(self.out, relative), relative)

    def test_the_nuspecs_are_xml_with_the_package_and_a_pinned_cdn_icon(self):
        space = "{http://schemas.microsoft.com/packaging/2015/06/nuspec.xsd}"
        for kind, (_, choco_id) in IDENTIFIERS.items():
            root = ElementTree.fromstring(read(self.out, "chocolatey/%s/%s.nuspec" % (choco_id, choco_id)))
            metadata = root.find(space + "metadata")
            self.assertEqual(metadata.find(space + "id").text, choco_id)
            self.assertEqual(metadata.find(space + "version").text, VERSION)
            icon = metadata.find(space + "iconUrl").text
            self.assertEqual(icon, "https://cdn.jsdelivr.net/gh/donislawdev/Naughty-Keyboard@%s/%s"
                             % (TAG, build_packages.ICON))
            self.assertTrue(os.path.isfile(os.path.join(contents.ROOT, build_packages.ICON)))
            self.assertIn("This package is the %s" % ("command line" if kind == "cli" else "palette window"),
                          metadata.find(space + "description").text)

    def test_the_words_follow_the_repository_rules(self):
        # Flat hyphens and no semicolons, the rule for every text the project
        # publishes, and the descriptions go out under its name.
        for relative in self.files():
            if relative.endswith((".nuspec", "locale.en-US.yaml")):
                text = read(self.out, relative)
                prose = [line for line in text.split("\n") if not line.lstrip().startswith(("<", "#"))]
                self.assertNotIn(";", "\n".join(prose), relative)
                self.assertNotIn(chr(0x2014), text, relative)
                self.assertNotIn(chr(0x2013), text, relative)


class TheWords(unittest.TestCase):
    """What each package tells a person, which differs by feed."""

    def test_the_palette_says_what_scope_user_gives_in_winget_only(self):
        window = build_packages.PACKAGES[0]
        winget = build_packages.how_to_start(window, "winget", "nkb-gui", "nkb")
        self.assertIn("--scope user", winget)
        self.assertIn("DonislawDev.NaughtyKeyboard.CLI", winget)
        chocolatey = build_packages.how_to_start(window, "chocolatey", "nkb-gui", "nkb")
        self.assertNotIn("--scope", chocolatey)
        self.assertIn("naughty-keyboard-cli", chocolatey)

    def test_the_command_line_points_at_the_palette_of_its_own_feed(self):
        cli = build_packages.PACKAGES[1]
        self.assertIn("DonislawDev.NaughtyKeyboard,", build_packages.how_to_start(cli, "winget", "nkb-gui", "nkb"))
        self.assertIn("naughty-keyboard,", build_packages.how_to_start(cli, "chocolatey", "nkb-gui", "nkb"))


class TheInputs(unittest.TestCase):
    """Every refusal about what a person passes, and that a refusal leaves nothing."""

    def setUp(self):
        self.place = tempfile.mkdtemp()
        self.out = os.path.join(self.place, "rendered")

    def tearDown(self):
        shutil.rmtree(self.place, ignore_errors=True)

    def test_a_candidate_and_a_name_that_is_no_tag_are_refused(self):
        refusal(self, lambda: build_packages.parse_tag("v0.3.0-rc.1"), "release candidate")
        refusal(self, lambda: build_packages.parse_tag("0.3.0"), "is not a release tag")
        refusal(self, lambda: build_packages.parse_tag("v0.3"), "is not a release tag")
        self.assertEqual(build_packages.parse_tag(TAG), VERSION)

    def test_a_version_the_changelog_never_dated_is_refused(self):
        refusal(self, lambda: build_packages.release_date("9.9.9"), "no dated section for 9.9.9")
        self.assertEqual(build_packages.release_date(VERSION), "2026-10-25")

    def test_a_checksum_file_of_another_release_is_named_as_such(self):
        with release(version="0.2.0") as made:
            refusal(self, lambda: build_packages.build(TAG, made.sums_path, self.out, made.msi),
                    "no line for nkb-setup_0.3.0_windows_amd64.msi", "It lists nkb-setup_0.2.0_windows_amd64.msi")

    def test_a_checksum_file_that_is_not_one_is_refused(self):
        with release(lines=["not a checksum line\n"]) as made:
            refusal(self, lambda: build_packages.read_sums(made.sums_path), "line 1 is not")
        big = os.path.join(self.place, "big.txt")
        with open(big, "wb") as handle:
            handle.write(b"0" * (build_packages.SUMS_LIMIT + 1))
        refusal(self, lambda: build_packages.read_sums(big), "is larger than")
        binary = os.path.join(self.place, "binary.txt")
        with open(binary, "wb") as handle:
            handle.write(b"\xff\xfe\x00not utf-8\xc3")
        refusal(self, lambda: build_packages.read_sums(binary), "is not UTF-8")
        refusal(self, lambda: build_packages.read_sums(self.place), "is not a file")
        refusal(self, lambda: build_packages.read_sums(os.path.join(self.place, "missing.txt")),
                "cannot read the checksum file")

    def test_a_checksum_file_in_any_spelling_is_read(self):
        sha = digest("x")
        path = os.path.join(self.place, "sums.txt")
        with open(path, "wb") as handle:
            handle.write((chr(0xFEFF) + "%s *a.zip\r\n%s  b.msi\r\n\r\n" % (sha.upper(), sha)).encode("utf-8"))
        self.assertEqual(build_packages.read_sums(path), {"a.zip": sha, "b.msi": sha})

    def test_a_destination_inside_the_repository_or_holding_something_is_refused(self):
        refusal(self, lambda: build_packages.check_out(os.path.join(contents.ROOT, "packaging", "x")),
                "is inside the repository")
        os.makedirs(self.out)
        open(os.path.join(self.out, "a file"), "w").close()
        refusal(self, lambda: build_packages.check_out(self.out), "already holds something")

    def test_winget_needs_the_installer_and_chocolatey_does_not(self):
        with release() as made:
            refusal(self, lambda: build_packages.build(TAG, made.sums_path, self.out), "pass it with --msi")
            build_packages.build(TAG, made.sums_path, self.out, feeds=("chocolatey",))
        names = [f for _, _, files in os.walk(self.out) for f in files]
        self.assertEqual(sorted(names), sorted(os.path.basename(r) for r in RENDERED if r.startswith("chocolatey/")))

    def test_an_installer_that_is_not_the_releases_is_refused(self):
        with release() as made:
            with open(made.msi, "ab") as handle:
                handle.write(b"changed")
            refusal(self, lambda: build_packages.build(TAG, made.sums_path, self.out, made.msi),
                    "is not the installer the checksum file names")
        self.assertFalse(os.path.exists(self.out))

    def test_an_installer_of_another_product_or_version_is_refused(self):
        for name, value in (("UpgradeCode", "{00000000-0000-0000-0000-000000000000}"),
                            ("ProductVersion", "0.2.0"), ("ProductName", "Something Else")):
            with self.subTest(name=name):
                properties = {"ProductCode": PRODUCT_CODE, "UpgradeCode": UPGRADE_CODE,
                              "ProductVersion": VERSION, "ProductName": "Naughty Keyboard", name: value}
                with release(properties=properties) as made:
                    refusal(self, lambda: build_packages.build(TAG, made.sums_path, self.out, made.msi),
                            "the installer says %s = %r" % (name, value))

    def test_a_product_code_that_is_not_a_code_is_refused(self):
        for code in ("", "7A7FC4A0-783B-4D8E-A35E-985EC830232A", "{7a7fc4a0-783b-4d8e-a35e-985ec830232a}"):
            with self.subTest(code=code):
                properties = {"ProductCode": code, "UpgradeCode": UPGRADE_CODE,
                              "ProductVersion": VERSION, "ProductName": "Naughty Keyboard"}
                with release(properties=properties) as made:
                    refusal(self, lambda: build_packages.build(TAG, made.sums_path, self.out, made.msi),
                            "is not a code in braces")

    def test_a_run_that_fails_while_writing_leaves_nothing_behind(self):
        nested = os.path.join(self.place, "made", "by", "the", "run")
        real = build_packages.render
        calls = []

        def render_then_fail(text, table, name):
            calls.append(name)
            if len(calls) == 3:
                raise contents.Refused(["stopped on purpose"])
            return real(text, table, name)

        with release() as made, unittest.mock.patch.object(build_packages, "render", render_then_fail):
            refusal(self, lambda: build_packages.build(TAG, made.sums_path, nested, made.msi), "stopped on purpose")
        self.assertEqual(len(calls), 3)
        self.assertEqual(os.listdir(self.place), [])

    def test_a_value_no_template_uses_is_refused(self):
        real = build_packages.values
        with release() as made, unittest.mock.patch.object(
                build_packages, "values", lambda *a: dict(real(*a), UNUSED_VALUE="x")):
            refusal(self, lambda: build_packages.build(TAG, made.sums_path, self.out, made.msi),
                    "no template uses UNUSED_VALUE")

    def test_main_says_the_refusal_and_answers_1(self):
        stderr = io.StringIO()
        with contextlib.redirect_stderr(stderr):
            code = build_packages.main(["--tag", "v0.3.0-rc.1", "--sums", "x", "--out", self.out])
        self.assertEqual(code, 1)
        self.assertIn("build_packages: refused:", stderr.getvalue())


class TheTemplates(unittest.TestCase):
    """A value that would change what the file it lands in means is refused."""

    def test_a_value_that_breaks_its_file_is_refused(self):
        for name, line, value, why in (
                ("tools/chocolateyinstall.ps1", "softwareName = '{{V}}'", "it's", "single quote ends the PowerShell"),
                ("package.nuspec", "<title>{{V}}</title>", "a <b>", "as markup"),
                ("package.nuspec", "<title>{{V}}</title>", "a & b", "as markup"),
                ("installer.yaml", "PackageName: {{V}}", "a: b", "as a key"),
                ("installer.yaml", "PackageName: {{V}}", "a #b", "as a comment"),
                ("installer.yaml", "ProductCode: {{V}}", "{X}", "first character as syntax"),
                ("installer.yaml", "ProductCode: '{{V}}'", "it's", "single quote ends the YAML")):
            with self.subTest(name=name, value=value):
                refusal(self, lambda: build_packages.render(line, {"V": value}, name), why)

    def test_a_value_between_single_quotes_may_start_with_a_brace(self):
        self.assertEqual(build_packages.render("ProductCode: '{{V}}'", {"V": "{X}"}, "installer.yaml"),
                         "ProductCode: '{X}'")

    def test_a_block_takes_the_indentation_of_its_placeholder(self):
        self.assertEqual(build_packages.render("Tags:\n  {{V}}\n", {"V": "- a\n- b"}, "locale.yaml"),
                         "Tags:\n  - a\n  - b\n")

    def test_several_lines_inside_a_line_and_an_unknown_placeholder_are_refused(self):
        refusal(self, lambda: build_packages.render("Name: {{V}}", {"V": "a\nb"}, "x.yaml"), "inside a line")
        refusal(self, lambda: build_packages.render("Name: {{W}}", {"V": "a"}, "x.yaml"), "does not know")

    def test_a_description_that_would_end_its_cdata_is_refused(self):
        refusal(self, lambda: build_packages.render("{{V}}", {"V": "a ]]> b"}, "package.nuspec"), "]]>")

    def test_a_script_that_is_not_ascii_is_refused(self):
        refusal(self, lambda: build_packages.check_script("Write-Host '%s'" % chr(0x105), "x.ps1"), "is not ASCII")


if __name__ == "__main__":
    unittest.main()
