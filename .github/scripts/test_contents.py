"""Tests for contents.py, the bill of materials and the licence notices.

Run from the root of the repository:

    python3 -m unittest discover -s .github/scripts -p "test_contents.py"

None of these needs Cargo or the network. The parts that ask Cargo are fed a
made-up answer, and the checks that read the real tree read only files in it, so
CI runs them on every pull request. What Cargo says about the real dependency
tree is asked by `contents.py check` in the supply chain workflow.
"""
import json
import os
import shutil
import tempfile
import textwrap
import unittest

import contents

MIT = ("Copyright (c) 2020 Someone\n\nPermission is hereby granted, free of charge, to any person "
       "obtaining a copy of this software\n")
APACHE = "Apache License\nVersion 2.0, January 2004\nhttp://www.apache.org/licenses/\n"
GPL = "GNU GENERAL PUBLIC LICENSE\nVersion 3, 29 June 2007\n"
ALLOWED = {"MIT": 0, "Apache-2.0": 1, "Apache-2.0 WITH LLVM-exception": 2, "Unicode-3.0": 3,
           "GPL-3.0-only": 4}


def choices(*sets):
    return [frozenset(s) for s in sets]


class Expressions(unittest.TestCase):
    def test_a_single_licence_is_one_choice(self):
        self.assertEqual(contents.alternatives("MIT"), choices({"MIT"}))

    def test_or_offers_each_side(self):
        self.assertEqual(contents.alternatives("MIT OR Apache-2.0"), choices({"MIT"}, {"Apache-2.0"}))

    def test_the_old_slash_means_or_with_and_without_spaces(self):
        for written in ("MIT/Apache-2.0", "MIT / Apache-2.0", "Apache-2.0 / MIT"):
            self.assertEqual(set(contents.alternatives(written)), {frozenset({"MIT"}), frozenset({"Apache-2.0"})})
        self.assertEqual(contents.normalised("MIT/Apache-2.0"), "MIT OR Apache-2.0")

    def test_and_binds_tighter_than_or(self):
        self.assertEqual(contents.alternatives("MIT OR Apache-2.0 AND Unicode-3.0"),
                         choices({"MIT"}, {"Apache-2.0", "Unicode-3.0"}))

    def test_brackets_group_a_choice_inside_a_requirement(self):
        self.assertEqual(contents.alternatives("(MIT OR Apache-2.0) AND Unicode-3.0"),
                         choices({"MIT", "Unicode-3.0"}, {"Apache-2.0", "Unicode-3.0"}))

    def test_with_makes_one_term_of_a_licence_and_its_exception(self):
        self.assertEqual(contents.alternatives("Apache-2.0 WITH LLVM-exception OR MIT"),
                         choices({"Apache-2.0 WITH LLVM-exception"}, {"MIT"}))
        self.assertEqual(contents.parts_of("Apache-2.0 WITH LLVM-exception"), ["Apache-2.0", "LLVM-exception"])

    def test_operators_are_read_in_lower_case_too(self):
        self.assertEqual(contents.alternatives("MIT or Apache-2.0"), choices({"MIT"}, {"Apache-2.0"}))

    def test_a_licence_reference_is_a_licence(self):
        self.assertEqual(contents.alternatives("GPL-3.0-only OR LicenseRef-Slint-Software-3.0"),
                         choices({"GPL-3.0-only"}, {"LicenseRef-Slint-Software-3.0"}))

    def test_what_is_not_an_expression_is_refused(self):
        for written in ("", "   ", "MIT OR", "(MIT", "MIT)", "MIT AND AND Apache-2.0", "MIT WITH",
                        "OR MIT", "MIT $ Apache-2.0", "MIT Apache-2.0"):
            with self.subTest(written=written):
                with self.assertRaises(contents.Refused):
                    contents.alternatives(written)


class Choice(unittest.TestCase):
    def test_the_first_allowed_choice_whose_text_the_work_ships(self):
        picked = contents.choose(choices({"MIT"}, {"Apache-2.0"}), ALLOWED, lambda l: l == "Apache-2.0")
        self.assertEqual(picked, frozenset({"Apache-2.0"}))

    def test_with_no_text_shipped_the_first_allowed_choice(self):
        picked = contents.choose(choices({"Apache-2.0"}, {"MIT"}), ALLOWED, lambda l: False)
        self.assertEqual(picked, frozenset({"MIT"}))

    def test_a_choice_outside_the_allowed_list_is_never_taken(self):
        picked = contents.choose(choices({"0BSD"}, {"Apache-2.0"}), ALLOWED, lambda l: l == "0BSD")
        self.assertEqual(picked, frozenset({"Apache-2.0"}))
        self.assertIsNone(contents.choose(choices({"0BSD"}), ALLOWED, lambda l: True))

    def test_an_exception_needs_its_own_text_as_well(self):
        picked = contents.choose(choices({"Apache-2.0 WITH LLVM-exception"}, {"Apache-2.0"}),
                                 {"Apache-2.0": 0, "Apache-2.0 WITH LLVM-exception": 1},
                                 lambda l: l == "Apache-2.0")
        self.assertEqual(picked, frozenset({"Apache-2.0"}))

    def test_a_choice_is_spelled_in_the_allowed_order(self):
        self.assertEqual(contents.spelled(frozenset({"Unicode-3.0", "MIT"}), ALLOWED), "MIT AND Unicode-3.0")


class Recognition(unittest.TestCase):
    def test_each_licence_by_what_it_says(self):
        samples = {
            "MIT": MIT,
            "Apache-2.0": APACHE,
            "BSL-1.0": "Boost Software License - Version 1.0 - August 17th, 2003",
            "Unicode-3.0": "UNICODE LICENSE V3\n\nCOPYRIGHT AND PERMISSION NOTICE",
            "Zlib": "This software is provided 'as-is', without any express or implied warranty. "
                    "Altered source versions must be plainly marked as such.",
            "BSD-3-Clause": "Redistribution and use in source and binary forms, with or without "
                            "modification. Neither the name of the copyright holder",
            "BSD-2-Clause": "Redistribution and use in source and binary forms, with or without modification.",
            "ISC": "Permission to use, copy, modify, and/or distribute this software for any purpose "
                   "with or without fee is hereby granted, provided that the above copyright notice appear",
            "0BSD": "Permission to use, copy, modify, and/or distribute this software for any purpose "
                    "with or without fee is hereby granted.",
            "CC0-1.0": "Creative Commons Legal Code\n\nCC0 1.0 Universal",
            "Unlicense": "This is free and unencumbered software released into the public domain.",
            "GPL-3.0-only": GPL,
            "LLVM-exception": "--- LLVM Exceptions to the Apache 2.0 License ----",
        }
        for licence, text in samples.items():
            with self.subTest(licence=licence):
                self.assertEqual(contents.recognised(text), {licence})

    def test_typographic_quotes_and_line_breaks_do_not_hide_a_licence(self):
        text = ("This software is provided " + chr(0x2018) + "as-is" + chr(0x2019) + ", without any\n"
                "express or implied warranty.  Altered source versions must be\nplainly marked.")
        self.assertEqual(contents.recognised(text), {"Zlib"})

    def test_one_file_can_hold_two_licences(self):
        self.assertEqual(contents.recognised(MIT + "\n" + APACHE), {"MIT", "Apache-2.0"})

    def test_a_short_apache_notice_is_not_the_licence(self):
        notice = ('Copyright 2021 someone\n\nLicensed under the Apache License, Version 2.0 (the "License")\n'
                  "you may not use this file except in compliance with the License.")
        self.assertEqual(contents.recognised(notice), set())


class Tree(unittest.TestCase):
    """A directory for one test, removed afterwards."""

    def setUp(self):
        self.root = tempfile.mkdtemp(prefix="contents-test-")
        self.addCleanup(shutil.rmtree, self.root, ignore_errors=True)

    def put(self, where, text):
        path = os.path.join(self.root, *where.split("/"))
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w", encoding="utf-8", newline="\n") as handle:
            handle.write(text)
        return path


class ShippedTexts(Tree):
    def test_reuse_files_are_named_by_their_licence(self):
        self.put("LICENSES/LicenseRef-Thing-1.0.md", "the thing licence\n")
        known, unknown = contents.texts_shipped_in(self.root)
        self.assertEqual(known["LicenseRef-Thing-1.0"][0], "LICENSES/LicenseRef-Thing-1.0.md")
        self.assertEqual(unknown, [])

    def test_a_statement_is_kept_aside_as_unknown(self):
        self.put("LICENSE-MIT", MIT)
        self.put("COPYRIGHT", "Copyright 2016 The Authors. Licensed under MIT or Apache-2.0.\n")
        self.put("README.md", "not a licence\n")
        known, unknown = contents.texts_shipped_in(self.root)
        self.assertEqual(sorted(known), ["MIT"])
        self.assertEqual([name for name, _ in unknown], ["COPYRIGHT"])

    def test_text_is_read_with_plain_line_ends(self):
        path = os.path.join(self.root, "LICENSE")
        with open(path, "wb") as handle:
            handle.write(b"line one\r\nline two\r\n\r\n")
        self.assertEqual(contents.read_text(path), "line one\nline two\n")

    def test_a_text_that_is_not_utf8_is_refused(self):
        path = os.path.join(self.root, "LICENSE")
        with open(path, "wb") as handle:
            handle.write(b"Copyright \xe9 1999\n")
        with self.assertRaises(contents.Refused):
            contents.read_text(path)


class CrateParts(Tree):
    def crates(self, name, version, licence, files, source=contents.CRATES_IO,
               checksum="a" * 64, authors=("Ann Author <ann@example.org>",)):
        folder = os.path.join(self.root, "%s-%s" % (name, version))
        os.makedirs(folder, exist_ok=True)
        for file_name, text in files.items():
            with open(os.path.join(folder, file_name), "w", encoding="utf-8", newline="\n") as handle:
                handle.write(text)
        package = {"name": name, "version": version, "license": licence, "license_file": None,
                   "authors": list(authors), "manifest_path": os.path.join(folder, "Cargo.toml")}
        lock = {"name": name, "version": version, "source": source, "checksum": checksum}
        return contents.Crates({(name, version): package}, set(), {(name, version): lock})

    def part(self, *args, **kwargs):
        crates = self.crates(*args, **kwargs)
        name, version = next(iter(crates.packages))
        return contents.crate_part(name, version, crates, ALLOWED, "GPL-3.0-only")

    def test_a_crate_with_its_own_text_is_given_that_text(self):
        part, problems = self.part("thing", "1.0.0", "MIT OR Apache-2.0", {"LICENSE-MIT": MIT})
        self.assertEqual(problems, [])
        self.assertEqual(part.concluded, "MIT")
        self.assertEqual(part.texts[0][1], "as the crate ships it in LICENSE-MIT")
        self.assertIn("Copyright (c) 2020 Someone", part.texts[0][2])

    def test_a_crate_without_a_text_gets_the_standard_one_and_its_authors(self):
        part, problems = self.part("bare", "0.1.0", "MIT", {})
        self.assertEqual(problems, [])
        self.assertTrue(part.texts[0][1].startswith("the standard text"))
        self.assertIn("Permission is hereby granted", part.texts[0][2])
        self.assertEqual(part.authors, ("Ann Author",))

    def test_a_copyright_statement_travels_beside_the_licence(self):
        part, _ = self.part("stated", "1.0.0", "MIT/Apache-2.0", {"COPYING": "Copyright 2016 Frank.\n"})
        notices = [t for t in part.texts if t[0] == "Notice"]
        self.assertEqual(len(notices), 1)
        self.assertIn("Copyright 2016 Frank.", notices[0][2])
        self.assertEqual(part.declared, "MIT OR Apache-2.0")

    def test_a_sole_unknown_file_of_a_single_licence_is_its_text_and_not_a_notice(self):
        part, _ = self.part("odd", "1.0.0", "MIT", {"LICENSE": "Odd wording, Copyright 2001 Odd.\n"})
        self.assertEqual([t[0] for t in part.texts], ["MIT"])
        self.assertIn("Odd wording", part.texts[0][2])

    def test_the_program_licence_points_at_license_instead_of_repeating_it(self):
        part, _ = self.part("toolkit", "1.0.0", "GPL-3.0-only OR LicenseRef-Toolkit-1.0",
                            {})
        self.assertIsNone(part)
        os.makedirs(os.path.join(self.root, "toolkit-1.0.0", "LICENSES"), exist_ok=True)
        for name, text in (("GPL-3.0-only.txt", GPL), ("LicenseRef-Toolkit-1.0.md", "toolkit terms\n")):
            with open(os.path.join(self.root, "toolkit-1.0.0", "LICENSES", name), "w", encoding="utf-8") as handle:
                handle.write(text)
        crates = self.crates("toolkit", "1.0.0", "GPL-3.0-only OR LicenseRef-Toolkit-1.0", {})
        part, problems = contents.crate_part("toolkit", "1.0.0", crates, ALLOWED, "GPL-3.0-only")
        self.assertEqual(problems, [])
        self.assertEqual(part.texts, [("GPL-3.0-only", "the same licence as LICENSE beside this file", "")])
        self.assertEqual(part.references, {"LicenseRef-Toolkit-1.0": "toolkit terms\n"})

    def test_a_licence_reference_without_its_text_is_refused(self):
        part, problems = self.part("toolkit", "1.0.0", "GPL-3.0-only OR LicenseRef-Toolkit-1.0", {})
        self.assertIsNone(part)
        self.assertTrue(any("LicenseRef-Toolkit-1.0" in p for p in problems), problems)

    def test_what_a_release_must_not_be_built_from_is_refused(self):
        cases = {
            "git": dict(source="git+https://example.org/thing#abc"),
            "no checksum": dict(checksum=""),
        }
        for what, change in cases.items():
            with self.subTest(what=what):
                part, problems = self.part("thing", "1.0.0", "MIT", {"LICENSE-MIT": MIT}, **change)
                self.assertIsNone(part)
                self.assertTrue(problems)

    def test_a_crate_without_a_licence_expression_is_refused(self):
        part, problems = self.part("thing", "1.0.0", None, {"LICENSE-MIT": MIT})
        self.assertIsNone(part)
        self.assertTrue(any("clarify" in p for p in problems), problems)

    def test_a_crate_whose_licences_are_all_forbidden_is_refused(self):
        part, problems = self.part("thing", "1.0.0", "SSPL-1.0", {})
        self.assertIsNone(part)
        self.assertTrue(any("allows none" in p for p in problems), problems)


class Register(Tree):
    REGISTER = textwrap.dedent('''\
        schema = 1

        [[components]]
        name = "Font"
        version = "1"
        licence = "LicenseRef-Font"
        licence-text = "crates/gui/ui/fonts/LICENSE-Font.txt"
        source = "https://example.org/font"
        programs = ["gui"]
        files = [{ path = "crates/gui/ui/fonts/font.ttf", sha256 = "%s" }]
        derived = ["crates/gui/src/table.rs"]

        [[components]]
        name = "Data"
        licence = "CC-BY-4.0"
        source = "https://example.org/data"
        programs = ["gui"]
        files = [{ path = "data/" }]
        own = true

        [[not-compiled-in]]
        file = "crates/gui/src/lib.rs"
        reference = "lib.rs"
        reason = "a test reads it"
        ''')

    def build(self, digest=None):
        font = self.put("crates/gui/ui/fonts/font.ttf", "font bytes")
        self.put("crates/gui/ui/fonts/LICENSE-Font.txt", "font licence\n")
        self.put("data/one.toml", "x = 1\n")
        self.put("crates/gui/src/table.rs", "//! Generated from the font. Do not edit by hand.\n")
        self.put("crates/gui/src/lib.rs", 'static ONE: &str = include_str!("../../../data/one.toml");\n'
                                          "// include_str!(\"not/real.txt\") in a comment is not code\n"
                                          "mod tests { const SELF: &str = include_str!(\"lib.rs\"); }\n")
        self.put("crates/gui/ui/tokens.slint", 'import "fonts/font.ttf";\nimport { X } from "x.slint";\n')
        self.put("crates/gui/tests/fixture.rs", 'const T: &[u8] = include_bytes!("nowhere.bin");\n')
        register = self.put(".github/release/components.toml",
                            self.REGISTER % (digest or contents.sha256_of(font)))
        return contents.load_register(register, root=self.root, programs={"gui"})

    def test_a_register_that_accounts_for_everything_passes(self):
        components, allowances = self.build()
        self.assertEqual(contents.check_register_against_source(components, allowances, self.root), [])

    def test_a_file_compiled_in_and_not_named_is_refused(self):
        components, allowances = self.build()
        self.put("crates/gui/src/extra.rs", 'static X: &[u8] = include_bytes!("../blob.bin");\n')
        problems = contents.check_register_against_source(components, allowances, self.root)
        self.assertTrue(any("crates/gui/blob.bin" in p for p in problems), problems)

    def test_a_generated_table_nobody_names_is_refused(self):
        components, allowances = self.build()
        self.put("crates/gui/src/other_table.rs", "//! Generated from somewhere else.\n")
        problems = contents.check_register_against_source(components, allowances, self.root)
        self.assertTrue(any("other_table.rs" in p for p in problems), problems)

    def test_an_entry_nothing_compiles_in_is_refused(self):
        components, allowances = self.build()
        self.put("crates/gui/ui/tokens.slint", 'import { X } from "x.slint";\n')
        problems = contents.check_register_against_source(components, allowances, self.root)
        self.assertTrue(any("font.ttf" in p and "nothing" in p for p in problems), problems)

    def test_an_allowance_that_matches_nothing_is_refused(self):
        components, allowances = self.build()
        self.put("crates/gui/src/lib.rs", 'static ONE: &str = include_str!("../../../data/one.toml");\n')
        problems = contents.check_register_against_source(components, allowances, self.root)
        self.assertTrue(any("matches nothing" in p for p in problems), problems)

    def test_somebody_elses_bytes_that_changed_are_refused(self):
        with self.assertRaises(contents.Refused) as caught:
            self.build(digest="0" * 64)
        self.assertTrue(any("digest" in p for p in caught.exception.problems))

    def test_a_licence_reference_needs_its_text(self):
        self.build()
        register = self.put(".github/release/components.toml",
                            self.REGISTER.replace('licence-text = "crates/gui/ui/fonts/LICENSE-Font.txt"\n', "")
                            % contents.sha256_of(os.path.join(self.root, "crates/gui/ui/fonts/font.ttf")))
        with self.assertRaises(contents.Refused):
            contents.load_register(register, root=self.root, programs={"gui"})

    def test_a_component_in_a_program_that_does_not_exist_is_refused(self):
        self.build()
        register = os.path.join(self.root, ".github", "release", "components.toml")
        with self.assertRaises(contents.Refused):
            contents.load_register(register, root=self.root, programs={"cli"})


class StandardTexts(Tree):
    def test_an_edited_text_and_an_unlisted_text_are_refused(self):
        self.put("MIT.txt", "the text\n")
        self.put("Extra.txt", "nobody listed me\n")
        self.put("index.toml", 'list = "SPDX License List 3.29.0"\n[sha256]\n"MIT" = "%s"\n' % ("0" * 64))
        problems, listed = contents.check_standard_texts(self.root)
        self.assertEqual(listed, "SPDX License List 3.29.0")
        self.assertEqual(len(problems), 2, problems)


class Tags(unittest.TestCase):
    WORKSPACE = contents.Workspace("0.1.0", "https://github.com/x/y", "GPL-3.0-only")

    def test_a_release_and_a_candidate(self):
        self.assertEqual(contents.version_of_tag("v0.1.0", self.WORKSPACE), "0.1.0")
        self.assertEqual(contents.version_of_tag("v0.1.0-rc.1", self.WORKSPACE), "0.1.0-rc.1")

    def test_a_tag_that_is_not_a_release_or_another_version_is_refused(self):
        for tag in ("0.1.0", "v0.1", "v0.1.0-", "v0.2.0", "v0.1.0 ", "refs/tags/v0.1.0", None):
            with self.subTest(tag=tag):
                with self.assertRaises(contents.Refused):
                    contents.version_of_tag(tag, self.WORKSPACE)


class Document(unittest.TestCase):
    WORKSPACE = contents.Workspace("0.1.0", "https://github.com/x/y", "GPL-3.0-only")
    ARCHIVE = contents.Archive("nkb", "nkb-cli", "the tool", "linux", "amd64", "x86_64-unknown-linux-gnu", "tar.gz")

    def crate(self, name="thing", version="1.0.0+extra", declared="MIT OR LicenseRef-Mine",
              references=None):
        return contents.Part(
            key="crate:%s@%s" % (name, version), name=name, version=version, declared=declared,
            concluded="MIT", texts=[("MIT", "as the crate ships it in LICENSE", MIT)],
            references={"LicenseRef-Mine": "mine\n"} if references is None else references,
            crate=True, checksum="b" * 64, download="https://crates.io/x")

    def document(self, *parts):
        content = contents.Contents(self.ARCHIVE, list(parts))
        return contents.bill_of_materials([content], self.WORKSPACE, "v0.1.0", "0.1.0",
                                          "2026-10-09T00:00:00Z", "abc", "SPDX License List 3.29.0")

    def test_the_document_holds_what_spdx_23_json_names(self):
        document = self.document(self.crate())
        self.assertEqual(document["spdxVersion"], "SPDX-2.3")
        self.assertIn("hasExtractedLicensingInfos", document)
        self.assertNotIn("hasExtractedLicensingInfo", document)
        self.assertEqual(document["creationInfo"]["licenseListVersion"], "3.29")
        names = {p["name"] for p in document["packages"]}
        self.assertEqual(names, {"nkb_0.1.0_linux_amd64.tar.gz", "thing"})
        crate = next(p for p in document["packages"] if p["name"] == "thing")
        self.assertEqual(crate["externalRefs"][0]["referenceLocator"], "pkg:cargo/thing@1.0.0%2Bextra")
        self.assertEqual(crate["SPDXID"], "SPDXRef-Crate-thing-1.0.0-extra")
        json.dumps(document)

    def test_two_names_that_become_one_identifier_are_refused(self):
        with self.assertRaises(contents.Refused):
            self.document(self.crate(name="a_b", references={}, declared="MIT"),
                          self.crate(name="a-b", references={}, declared="MIT"))

    def test_a_licence_reference_without_a_definition_is_refused(self):
        with self.assertRaises(contents.Refused):
            self.document(self.crate(references={}))

    def test_the_singular_key_would_be_dropped_and_is_refused(self):
        # Asked of the key itself. Moving the definitions under the singular key also
        # leaves the LicenseRef undefined, and a check of that alone kept this test
        # green with the key check gone (M702).
        document = self.document(self.crate())
        document["hasExtractedLicensingInfo"] = document.pop("hasExtractedLicensingInfos")
        problems = contents.validate(document)
        self.assertTrue(any(p.startswith("the document carries hasExtractedLicensingInfo,") for p in problems),
                        problems)

    def test_two_runs_on_one_commit_write_the_same_document(self):
        self.assertEqual(json.dumps(self.document(self.crate())), json.dumps(self.document(self.crate())))


class Notices(unittest.TestCase):
    def test_the_notices_name_every_work_and_say_where_each_text_came_from(self):
        workspace = contents.Workspace("0.1.0", "https://github.com/x/y", "GPL-3.0-only")
        archive = contents.Archive("nkb", "nkb-cli", "the tool", "linux", "amd64", "t", "tar.gz")
        own = contents.Part("component:Packs", "Packs", "", "CC-BY-4.0", "CC-BY-4.0", [], {}, False, own=True)
        bare = contents.Part("crate:bare@1", "bare", "1", "MIT", "MIT",
                             [("MIT", "the standard text, because the crate ships none of its own", MIT)],
                             {}, True, authors=("Ann Author",))
        toolkit = contents.Part("crate:toolkit@2", "toolkit", "2", "GPL-3.0-only", "GPL-3.0-only",
                                [("GPL-3.0-only", "the same licence as LICENSE beside this file", "")], {}, True)
        text = contents.notices(contents.Contents(archive, [bare, toolkit, own]), workspace, "v0.1.0",
                                "0.1.0", "SPDX License List 3.29.0")
        self.assertTrue(text.startswith("Third-party notices for nkb 0.1.0 (linux, amd64)\n"))
        self.assertIn("https://github.com/x/y/tree/v0.1.0", text)
        self.assertIn("bare 1: Ann Author", text)
        self.assertIn("GPL-3.0-only, the same licence as LICENSE beside this file", text)
        self.assertIn("Packs: CC-BY-4.0", text)
        self.assertNotIn("GNU GENERAL PUBLIC LICENSE", text)
        self.assertTrue(text.endswith("\n") and not text.endswith("\n\n"))


class RealTree(unittest.TestCase):
    """The checks that need nothing but the files of this repository."""

    def test_the_archives_are_the_six_of_d25_on_three_systems(self):
        archives = contents.load_archives()
        self.assertEqual(len(archives), 6)
        names = sorted(a.name("0.1.0") for a in archives)
        self.assertIn("nkb_0.1.0_windows_amd64.zip", names)
        self.assertIn("nkb-gui_0.1.0_macos_arm64.tar.gz", names)

    def test_the_register_accounts_for_every_file_the_programs_compile_in(self):
        archives = contents.load_archives()
        components, allowances = contents.load_register(programs={a.program for a in archives})
        self.assertEqual(contents.check_register_against_source(components, allowances), [])

    def test_the_standard_texts_are_the_published_bytes(self):
        problems, listed = contents.check_standard_texts()
        self.assertEqual(problems, [])
        self.assertTrue(listed.startswith("SPDX License List"))

    def test_every_licence_deny_toml_allows_has_a_standard_text(self):
        allowed = contents.allowed_licences()
        for licence in allowed:
            for part in contents.parts_of(licence):
                with self.subTest(licence=part):
                    if part != contents.load_workspace().licence:
                        self.assertIsNotNone(contents.standard_text(part))


if __name__ == "__main__":
    unittest.main()
