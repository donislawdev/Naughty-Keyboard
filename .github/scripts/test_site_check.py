"""The check of the built website, proved on a small site written for it.

Every test breaks one thing in a site that passes, and expects that one
thing to be named. A check that is never seen failing cannot be told apart
from one that is broken.
"""
from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

import site_check

BASE = "https://example.test/"


def page(path: str, *, title: str = "", description: str = "",
         lang: str = "en", h1: int = 1, canonical: str | None = None, alternates: list[tuple[str, str]] | None = None,
         body: str = "", head: str = "") -> str:
    url = BASE + path
    title = title or f"Page {path or 'home'}"
    description = description or f"The page at /{path}, about one thing, long enough to say what it is about in a result."
    canonical = url if canonical is None else canonical
    if alternates is None:
        alternates = [("en", url), ("x-default", url)]
    links = "".join(f'<link rel="alternate" hreflang="{l}" href="{h}">' for l, h in alternates)
    heads = "".join("<h1>Title</h1>" for _ in range(h1))
    return (
        f'<!doctype html><html lang="{lang}"><head><title>{title}</title>'
        f'<meta name="description" content="{description}"><link rel="canonical" href="{canonical}">{links}{head}'
        f"</head><body>{heads}{body}</body></html>"
    )


class Fixture(unittest.TestCase):
    """A site of two pages, a 404, a sitemap and robots.txt, all correct."""

    def setUp(self) -> None:
        self.directory = tempfile.TemporaryDirectory()
        self.root = Path(self.directory.name)
        self.write("index.html", page("", body='<a href="/docs/">docs</a><a href="/docs/#part">part</a>'))
        self.write("docs/index.html", page("docs/", body='<h2 id="part">Part</h2><a href="https://elsewhere.test/">out</a>'))
        self.write("404.html", '<html lang="en"><head><meta name="robots" content="noindex"></head><body><h1>x</h1></body></html>')
        self.write(
            "sitemap.xml",
            '<?xml version="1.0"?><urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">'
            f"<url><loc>{BASE}</loc></url><url><loc>{BASE}docs/</loc></url></urlset>",
        )
        self.write("robots.txt", f"User-agent: *\n\nSitemap: {BASE}sitemap.xml\n")

    def tearDown(self) -> None:
        self.directory.cleanup()

    def write(self, name: str, text: str) -> None:
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")

    def problems(self) -> list[str]:
        return site_check.check(self.root, BASE)

    def assert_named(self, words: str) -> None:
        found = self.problems()
        self.assertTrue(any(words in p for p in found), f"expected {words!r} among {found}")


class TheCheck(Fixture):
    def test_a_correct_site_passes(self) -> None:
        self.assertEqual(self.problems(), [])

    def test_a_second_h1_is_named(self) -> None:
        self.write("docs/index.html", page("docs/", h1=2, body='<h2 id="part">x</h2>'))
        self.assert_named("2 h1 elements")

    def test_a_page_without_a_language_is_named(self) -> None:
        self.write("docs/index.html", page("docs/", lang="", body='<h2 id="part">x</h2>'))
        self.assert_named("no lang")

    def test_a_long_title_is_named(self) -> None:
        self.write("docs/index.html", page("docs/", title="x" * 61, body='<h2 id="part">x</h2>'))
        self.assert_named("a title of 61 characters")

    def test_a_short_description_is_named(self) -> None:
        self.write("docs/index.html", page("docs/", description="Too short.", body='<h2 id="part">x</h2>'))
        self.assert_named("a description of 10 characters")

    def test_a_canonical_address_elsewhere_is_named(self) -> None:
        self.write("docs/index.html", page("docs/", canonical=BASE, body='<h2 id="part">x</h2>'))
        self.assert_named("canonical")

    def test_alternates_that_leave_out_x_default_are_named(self) -> None:
        url = BASE + "docs/"
        self.write("docs/index.html", page("docs/", alternates=[("en", url)], body='<h2 id="part">x</h2>'))
        self.assert_named("no x-default")

    def test_an_alternate_that_does_not_name_back_is_named(self) -> None:
        url = BASE + "docs/"
        self.write("pl/index.html", page("pl/", lang="pl", alternates=[("pl", BASE + "pl/"), ("x-default", BASE)]))
        self.write("docs/index.html", page("docs/", alternates=[("en", url), ("pl", BASE + "pl/"), ("x-default", url)], body='<h2 id="part">x</h2>'))
        self.assert_named("does not name it back")

    def test_a_page_missing_from_one_language_is_named(self) -> None:
        both = [("en", BASE), ("pl", BASE + "pl/"), ("x-default", BASE)]
        self.write("index.html", page("", alternates=both, body='<a href="/docs/">d</a><a href="/docs/#part">p</a>'))
        self.write("pl/index.html", page("pl/", lang="pl", alternates=both))
        self.assert_named("docs/: there is no pl version of it")

    def test_a_sitemap_that_gives_a_priority_is_named(self) -> None:
        self.write(
            "sitemap.xml",
            '<?xml version="1.0"?><urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">'
            f"<url><loc>{BASE}</loc></url><url><loc>{BASE}docs/</loc><priority>0</priority></url></urlset>",
        )
        self.assert_named("gives a page a priority")

    def test_a_link_to_no_page_is_named(self) -> None:
        self.write("index.html", page("", body='<a href="/docs/">d</a><a href="/gone/">g</a><a href="/docs/#part">p</a>'))
        self.assert_named("links to '/gone/', which is not a page")

    def test_a_link_to_a_missing_fragment_is_named(self) -> None:
        self.write("index.html", page("", body='<a href="/docs/">d</a><a href="/docs/#nowhere">p</a>'))
        self.assert_named("has no element 'nowhere'")

    def test_a_fragment_written_with_percent_escapes_finds_its_element(self) -> None:
        self.write("index.html", page("", body='<a href="/docs/">d</a><a href="/docs/#co-pami%c4%99ta">p</a>'))
        self.write("docs/index.html", page("docs/", body='<h2 id="co-pamięta">x</h2>'))
        self.assertEqual(self.problems(), [])

    def test_a_script_from_another_host_is_named(self) -> None:
        self.write("docs/index.html", page("docs/", head='<script src="https://cdn.test/a.js"></script>', body='<h2 id="part">x</h2>'))
        self.assert_named("from another host")

    def test_a_stylesheet_from_another_host_is_named(self) -> None:
        self.write("docs/index.html", page("docs/", head='<link rel="stylesheet" href="https://fonts.test/a.css">', body='<h2 id="part">x</h2>'))
        self.assert_named("from another host")

    def test_a_picture_without_its_size_is_named(self) -> None:
        self.write("docs/index.html", page("docs/", body='<h2 id="part">x</h2><img src="/a.png" alt="a">'))
        self.assert_named("has no width, height")

    def test_structured_data_that_is_not_json_is_named(self) -> None:
        self.write("docs/index.html", page("docs/", head='<script type="application/ld+json">{nope</script>', body='<h2 id="part">x</h2>'))
        self.assert_named("not JSON")

    def test_two_pages_with_one_title_are_named(self) -> None:
        self.write("docs/index.html", page("docs/", title="Page home", body='<h2 id="part">x</h2>'))
        self.assert_named("the same title")

    def test_a_page_the_sitemap_leaves_out_is_named(self) -> None:
        self.write("more/index.html", page("more/"))
        self.assert_named(f"the sitemap leaves out {BASE}more/")

    def test_a_sitemap_entry_with_no_page_is_named(self) -> None:
        (self.root / "docs" / "index.html").unlink()
        self.assert_named("which is not a page")

    def test_a_404_that_can_be_indexed_is_named(self) -> None:
        self.write("404.html", '<html lang="en"><body><h1>x</h1></body></html>')
        self.assert_named("404.html is not noindex")

    def test_robots_without_the_sitemap_is_named(self) -> None:
        self.write("robots.txt", "User-agent: *\n")
        self.assert_named("robots.txt does not name")

    def test_a_sitemap_index_is_followed_to_its_parts(self) -> None:
        self.write(
            "sitemap.xml",
            '<?xml version="1.0"?><sitemapindex xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">'
            f"<sitemap><loc>{BASE}en/sitemap.xml</loc></sitemap></sitemapindex>",
        )
        self.write(
            "en/sitemap.xml",
            '<?xml version="1.0"?><urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">'
            f"<url><loc>{BASE}</loc></url><url><loc>{BASE}docs/</loc></url></urlset>",
        )
        self.assertEqual(self.problems(), [])

    def test_an_empty_folder_is_a_problem_and_not_a_pass(self) -> None:
        with tempfile.TemporaryDirectory() as empty:
            self.assertTrue(site_check.check(Path(empty), BASE))


if __name__ == "__main__":
    unittest.main()
