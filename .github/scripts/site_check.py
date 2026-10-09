#!/usr/bin/env python3
"""Checks the project website the way Hugo built it, page by page.

A website can look right in a browser and still be invisible to a search
engine: a second h1, a canonical address on the wrong host, a language
version that does not link back, a sitemap that leaves a page out. None of
that shows on screen. This reads every page Hugo wrote and fails on each of
those, so it fails in CI rather than in a search console weeks later.

What it checks, on every page but the 404:

- one `h1`, a `lang`, a title and a description within what a search result
  shows, and no title or description used twice,
- a canonical address on the site's own host, which is the page itself,
- `hreflang` alternates that name the page itself and `x-default`, and that
  every alternate names back,
- every link inside the site leads to a page, and every `#fragment` to an
  element of that page,
- nothing loaded from another host: no script, stylesheet, picture or frame,
- every picture with `alt`, `width` and `height`,
- every block of structured data is JSON with the schema.org context.

And once for the site: the 404 page is `noindex`, the sitemap lists exactly
the pages there are, and robots.txt names the sitemap.

What it cannot see: whether a sentence is true, whether a page reads well, or
what a search engine will do with any of it.

Usage:
    python .github/scripts/site_check.py <public-dir> --config web/hugo.toml
    python .github/scripts/site_check.py <public-dir> --base https://example.test/
"""
from __future__ import annotations

import argparse
import json
import sys
import re
import tomllib
from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import unquote, urlsplit

TITLE_LIMIT = 60
DESCRIPTION_MIN = 70
DESCRIPTION_LIMIT = 160
#: An address in a sitemap. Hugo writes one per <loc> element, and a sitemap of
#: our own build needs no XML parser to be read, so none is brought in.
LOC = re.compile(r"<loc>\s*([^<\s]+)\s*</loc>")
#: The tags that load something into the page, and the attribute that says from
#: where. A link to another site is fine, a picture from one is not.
LOADS = {"script": "src", "img": "src", "iframe": "src", "source": "src", "video": "src", "audio": "src"}
#: The kinds of `<link>` that load something.
LOADING_LINKS = {"stylesheet", "icon", "preload", "modulepreload", "manifest", "apple-touch-icon"}


class Page(HTMLParser):
    """What a page says about itself, read in one pass."""

    def __init__(self) -> None:
        super().__init__(convert_charrefs=True)
        self.lang = ""
        self.title = ""
        self.description: str | None = None
        self.robots = ""
        self.canonical: str | None = None
        self.alternates: list[tuple[str, str]] = []
        self.h1 = 0
        self.ids: set[str] = set()
        self.links: list[str] = []
        self.loads: list[str] = []
        self.pictures: list[dict[str, str | None]] = []
        self.structured: list[str] = []
        self._in_title = False
        self._in_json = False
        self._json = ""

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        a = dict(attrs)
        if a.get("id"):
            self.ids.add(a["id"] or "")
        if tag == "html":
            self.lang = a.get("lang") or ""
        elif tag == "title":
            self._in_title = True
        elif tag == "h1":
            self.h1 += 1
        elif tag == "meta":
            if a.get("name") == "description":
                self.description = a.get("content") or ""
            elif a.get("name") == "robots":
                self.robots = a.get("content") or ""
        elif tag == "link":
            self._link(a)
        elif tag == "a" and a.get("href"):
            self.links.append(a["href"] or "")
        elif tag == "script" and a.get("type") == "application/ld+json":
            self._in_json = True
            self._json = ""
        if tag in LOADS and a.get(LOADS[tag]):
            self.loads.append(a[LOADS[tag]] or "")
        if tag == "img":
            self.pictures.append({k: a.get(k) for k in ("src", "alt", "width", "height")})

    def _link(self, a: dict[str, str | None]) -> None:
        rel = set((a.get("rel") or "").split())
        href = a.get("href") or ""
        if "canonical" in rel:
            self.canonical = href
        if "alternate" in rel and a.get("hreflang"):
            self.alternates.append((a.get("hreflang") or "", href))
        if rel & LOADING_LINKS:
            self.loads.append(href)

    def handle_endtag(self, tag: str) -> None:
        if tag == "title":
            self._in_title = False
        elif tag == "script" and self._in_json:
            self._in_json = False
            self.structured.append(self._json)

    def handle_data(self, data: str) -> None:
        if self._in_title:
            self.title += data
        if self._in_json:
            self._json += data


def read(path: Path) -> Page:
    page = Page()
    page.feed(path.read_text(encoding="utf-8"))
    return page


class Site:
    """The built site: where its files are, and the address it is served at."""

    def __init__(self, root: Path, base: str) -> None:
        self.root = root
        self.base = base if base.endswith("/") else base + "/"
        self.host = urlsplit(self.base).netloc

    def url_of(self, path: Path) -> str:
        relative = path.relative_to(self.root).as_posix()
        if relative == "index.html":
            return self.base
        if relative.endswith("/index.html"):
            return self.base + relative[: -len("index.html")]
        return self.base + relative

    def file_of(self, address: str, here: Path | None = None) -> Path | None:
        """The file an address inside the site is served from, or None for an
        address outside it."""
        parts = urlsplit(address)
        if parts.scheme and parts.scheme not in ("http", "https"):
            return None
        if parts.netloc and parts.netloc != self.host:
            return None
        path = unquote(parts.path)
        if not path:
            return here
        if not path.startswith("/"):
            base = (here.parent if here else self.root).relative_to(self.root).as_posix()
            path = "/" + (base + "/" if base not in ("", ".") else "") + path
        target = self.root / path.lstrip("/")
        if path.endswith("/"):
            target = target / "index.html"
        return target

    def pages(self) -> list[Path]:
        return sorted(p for p in self.root.rglob("*.html"))


def page_problems(site: Site, path: Path, page: Page) -> list[str]:
    """Everything wrong with one page on its own."""
    found = []
    own = site.url_of(path)
    if page.h1 != 1:
        found.append(f"{page.h1} h1 elements, and a page has one")
    if not page.lang:
        found.append("no lang on the html element")
    title = " ".join(page.title.split())
    if not title:
        found.append("no title")
    elif len(title) > TITLE_LIMIT:
        found.append(f"a title of {len(title)} characters, over {TITLE_LIMIT}: {title!r}")
    description = page.description or ""
    if not DESCRIPTION_MIN <= len(description) <= DESCRIPTION_LIMIT:
        found.append(f"a description of {len(description)} characters, outside {DESCRIPTION_MIN} to {DESCRIPTION_LIMIT}")
    if page.canonical != own:
        found.append(f"canonical {page.canonical!r}, and the page is {own!r}")
    languages = dict(page.alternates)
    if not page.alternates:
        found.append("no hreflang alternates")
    else:
        if own not in languages.values():
            found.append("the hreflang alternates do not name the page itself")
        if "x-default" not in languages:
            found.append("no x-default among the hreflang alternates")
        for language, href in page.alternates:
            if urlsplit(href).netloc != site.host:
                found.append(f"the {language} alternate {href!r} is on another host")
    for address in page.loads:
        if urlsplit(address).netloc not in ("", site.host):
            found.append(f"loads {address!r} from another host")
    for picture in page.pictures:
        missing = [k for k in ("alt", "width", "height") if picture.get(k) is None]
        if missing:
            found.append(f"the picture {picture.get('src')!r} has no {', '.join(missing)}")
    for block in page.structured:
        try:
            data = json.loads(block)
        except json.JSONDecodeError as error:
            found.append(f"structured data that is not JSON: {error}")
            continue
        if data.get("@context") != "https://schema.org":
            found.append(f"structured data without the schema.org context: {data.get('@type')}")
    return found


def link_problems(site: Site, path: Path, page: Page, read_page) -> list[str]:
    """Every link inside the site that leads nowhere, or to a fragment that is not there."""
    found = []
    for href in page.links:
        target = site.file_of(href, path)
        if target is None:
            continue
        if not target.is_file():
            found.append(f"links to {href!r}, which is not a page")
            continue
        fragment = urlsplit(href).fragment
        if fragment and fragment not in read_page(target).ids:
            found.append(f"links to {href!r}, and that page has no element {fragment!r}")
    return found


def alternate_problems(site: Site, pages: dict[Path, Page]) -> list[str]:
    """A language version that does not name back is ignored by a search engine."""
    found = []
    for path, page in pages.items():
        own = site.url_of(path)
        for language, href in page.alternates:
            if language == "x-default":
                continue
            target = site.file_of(href)
            other = pages.get(target) if target else None
            if other is None:
                found.append(f"{own}: the {language} alternate {href!r} is not a page")
            elif own not in dict(other.alternates).values():
                found.append(f"{own}: the {language} alternate {href!r} does not name it back")
    return found


def sitemap_addresses(site: Site, path: Path) -> list[str]:
    text = path.read_text(encoding="utf-8")
    listed = LOC.findall(text)
    if "<sitemapindex" not in text:
        return listed
    addresses = []
    for loc in listed:
        part = site.file_of(loc)
        addresses += sitemap_addresses(site, part) if part and part.is_file() else [f"missing:{loc}"]
    return addresses


def site_problems(site: Site, pages: dict[Path, Page]) -> list[str]:
    found = []
    missing = site.root / "404.html"
    if not missing.is_file():
        found.append("there is no 404.html")
    elif "noindex" not in read(missing).robots:
        found.append("404.html is not noindex")
    for what in ("title", "description"):
        seen: dict[str, str] = {}
        for path, page in pages.items():
            text = " ".join((page.title if what == "title" else page.description or "").split())
            if text in seen:
                found.append(f"{site.url_of(path)} has the same {what} as {seen[text]}")
            seen.setdefault(text, site.url_of(path))
    sitemap = site.root / "sitemap.xml"
    if not sitemap.is_file():
        found.append("there is no sitemap.xml")
    else:
        listed = sitemap_addresses(site, sitemap)
        there = {site.url_of(p) for p in pages}
        for address in sorted(set(listed) - there):
            found.append(f"the sitemap lists {address}, which is not a page")
        for address in sorted(there - set(listed)):
            found.append(f"the sitemap leaves out {address}")
    robots = site.root / "robots.txt"
    if not robots.is_file() or f"Sitemap: {site.base}sitemap.xml" not in robots.read_text(encoding="utf-8"):
        found.append(f"robots.txt does not name {site.base}sitemap.xml")
    return found


def check(root: Path, base: str) -> list[str]:
    site = Site(root, base)
    cache: dict[Path, Page] = {}

    def read_page(path: Path) -> Page:
        if path not in cache:
            cache[path] = read(path)
        return cache[path]

    pages = {p: read_page(p) for p in site.pages() if p.name != "404.html" or p.parent != root}
    if not pages:
        return [f"no pages under {root}"]
    found = []
    for path, page in pages.items():
        where = site.url_of(path)
        found += [f"{where}: {p}" for p in page_problems(site, path, page)]
        found += [f"{where}: {p}" for p in link_problems(site, path, page, read_page)]
    found += alternate_problems(site, pages)
    found += site_problems(site, pages)
    return found


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("public", type=Path, help="the folder Hugo built the site into")
    where = parser.add_mutually_exclusive_group(required=True)
    where.add_argument("--config", type=Path, help="the site's hugo.toml, whose baseURL is the address")
    where.add_argument("--base", help="the address the site is served at")
    args = parser.parse_args(argv)
    base = args.base
    if args.config:
        with args.config.open("rb") as config:
            base = tomllib.load(config)["baseURL"]
    found = check(args.public, base)
    pages = len(list(args.public.rglob("*.html")))
    if found:
        print(f"site: {len(found)} problems in {pages} pages")
        for problem in found:
            print(f"  {problem}")
        return 1
    print(f"site: {pages} pages, no problems")
    return 0


if __name__ == "__main__":
    sys.exit(main())
