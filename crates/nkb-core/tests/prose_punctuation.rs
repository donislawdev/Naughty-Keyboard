//! Untouchable rule 13 as a test: prose in this repository uses no semicolon,
//! and no dash other than the flat hyphen `-`.
//!
//! # Why a test, and why here
//!
//! The rule is about what this repository holds, and a fresh clone of it holds
//! none of the internal tooling that checks the project's documents. A check
//! kept outside would guard nothing on the machine where a change is written.
//! So it is a test, and it lives in the package every other package depends on,
//! because it needs nothing but the standard library and builds first.
//!
//! Measured 2026-09-24 at `fa867fa`, before this test existed: 173 comment
//! lines and 12 string literals carried a semicolon in prose, in 66 files and in
//! all six packages. A rule nothing checks is broken quietly, and that is worse
//! than a rule nobody wrote down, because it reads as protection.
//!
//! # What is prose
//!
//! - Every comment: `//`, `///`, `//!` and `/* */` in Rust, `//` and `/* */` in
//!   Slint and in the Windows resource script, `<!-- -->` in the SVG drawings,
//!   `#` in TOML, in the two git files at the root, in `.github/CODEOWNERS` and
//!   in the pinned requirements beside the CI scripts, and `#` in those scripts
//!   themselves, which are Python. A Python string is read the way a TOML string
//!   is, including the triple-quoted ones that hold a module's description.
//!   Code quoted in a comment is syntax, not prose, and is recognised the way rustdoc does it:
//!   between backticks, or between fence lines of three backticks or tildes. A
//!   block indented by four spaces is NOT recognised as code. Fence it.
//! - Every Markdown file, with the same two ways out for code.
//! - Every YAML file, which here means the issue forms on GitHub. Their words
//!   are values and not comments, so the whole file is read as prose, with the
//!   same two ways out for code.
//! - String literals, with a narrower question, because a literal also holds
//!   code under test. A semicolon counts there only when one space and then a
//!   letter or a backtick follow it: that is how a sentence goes on, and how a
//!   statement in a fixture almost never does. A literal quoting text the rule
//!   does not govern says so on its own line or the line above, in a comment
//!   that begins `prose-punctuation-exempt:` and gives a reason. An exemption
//!   that exempts nothing is itself a finding, so none outlives its reason.
//! - A dash other than the flat hyphen counts anywhere in prose and anywhere in
//!   a literal. A test that needs one spells it as an escape, `\u{2013}`, which
//!   is also the only spelling a reader can tell apart from the hyphen.
//!
//! # What is not read, and why
//!
//! - `packs/`: a pack's own prose may use a semicolon (`D31`), and a pack file
//!   answers to `nkb lint`, not to this rule.
//! - `tests/packs/`: their content is the subject of the tests.
//! - `LICENSE`, and the third-party files kept byte for byte beside the Unicode
//!   tables and the typeface. `Cargo.lock`, which a tool writes.
//! - The `.ico` of the application icon: nine pictures behind a directory of
//!   offsets, with no prose in it. The drawings it is made from are read.
//! - The `.png` of the social preview: a picture, with no prose in it. The
//!   drawing it is rendered from is read, and so is the README beside it.
//! - The project memory that the root `.gitignore` keeps out of this repository.
//!   Its names are listed here AND checked against that file, so the list cannot
//!   hide a file this repository does carry.
//! - Dot-files a system or an editor leaves behind, unless they have a syntax
//!   this test reads.
//! - Commit messages. The rule covers them and this test cannot: it sees the
//!   tree, not the history. Eighteen of the sixty commits written before it
//!   break it.
//!
//! Anything else at the root or under `crates/` that this test cannot read is a
//! finding, not a skip. A new kind of file has to be taught here, or excluded
//! here with a reason, and is never passed over in silence.

// A failed expectation in a test is a failed test.
#![allow(clippy::panic, clippy::expect_used)]

#[allow(dead_code)]
mod repo_text;

use repo_text::{
    Literal, MEMORY, Syntax, blocks, lex, product_tree, prose, relative, workspace_root,
};

/// Every dash that is not the flat hyphen: hyphen, non-breaking hyphen, figure
/// dash, en dash, em dash, horizontal bar, minus sign, two-em and three-em dash,
/// small em dash, small hyphen-minus and fullwidth hyphen-minus.
const OTHER_DASHES: [char; 12] = [
    '\u{2010}', '\u{2011}', '\u{2012}', '\u{2013}', '\u{2014}', '\u{2015}', '\u{2212}', '\u{2E3A}',
    '\u{2E3B}', '\u{FE58}', '\u{FE63}', '\u{FF0D}',
];

/// The comment that lets a literal starting on its line, or on the next one,
/// quote text this rule does not govern.
const EXEMPTION: &str = "prose-punctuation-exempt:";

/// How many words a reason needs, so that `exempt: x` is not one.
const REASON_WORDS: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Breach {
    Semicolon,
    Dash,
    LiteralSemicolon,
    LiteralDash,
    ExemptionWithoutReason,
    ExemptionUnused,
}

impl Breach {
    fn describe(self) -> &'static str {
        match self {
            Self::Semicolon => "semicolon in prose",
            Self::Dash => "dash other than the flat hyphen",
            Self::LiteralSemicolon => "semicolon inside a sentence in a literal",
            Self::LiteralDash => "dash other than the flat hyphen in a literal",
            Self::ExemptionWithoutReason => "exemption without a reason",
            Self::ExemptionUnused => "exemption with nothing to exempt",
        }
    }
}

/// What one file holds, and how much of it was looked at.
struct Scan {
    breaches: Vec<(usize, Breach)>,
    comment_lines: usize,
    literals: usize,
}

// ---- prose -----------------------------------------------------------------

/// Semicolons and dashes in the prose of one block.
fn prose_breaches(block: &[(usize, &str)], out: &mut Vec<(usize, Breach)>) {
    for paragraph in prose(block) {
        for (c, line) in paragraph {
            if c == ';' {
                out.push((line, Breach::Semicolon));
            } else if OTHER_DASHES.contains(&c) {
                out.push((line, Breach::Dash));
            }
        }
    }
}

fn literal_breaches(literal: &Literal) -> Vec<(usize, Breach)> {
    let chars = &literal.chars;
    let mut out = Vec::new();
    for (k, &(c, line)) in chars.iter().enumerate() {
        if OTHER_DASHES.contains(&c) {
            out.push((line, Breach::LiteralDash));
        }
        let sentence_goes_on = chars.get(k + 1).is_some_and(|n| n.0 == ' ')
            && chars
                .get(k + 2)
                .is_some_and(|n| n.0.is_ascii_alphabetic() || n.0 == '`');
        if c == ';' && sentence_goes_on {
            out.push((line, Breach::LiteralSemicolon));
        }
    }
    out
}

fn scan(syntax: Syntax, source: &str) -> Scan {
    let lexed = lex(syntax, source);
    let mut breaches = Vec::new();
    for block in blocks(&lexed.comments) {
        prose_breaches(&block, &mut breaches);
    }

    // Exemptions: the line each one sits on, and whether it has been used.
    let mut exemptions: Vec<(usize, bool)> = Vec::new();
    for comment in &lexed.comments {
        let [text] = comment.lines.as_slice() else {
            continue;
        };
        let Some(reason) = text.trim().strip_prefix(EXEMPTION) else {
            continue;
        };
        if reason.split_whitespace().count() < REASON_WORDS {
            breaches.push((comment.first_line, Breach::ExemptionWithoutReason));
        }
        exemptions.push((comment.first_line, false));
    }
    for literal in &lexed.literals {
        let found = literal_breaches(literal);
        if found.is_empty() {
            continue;
        }
        let exempted = exemptions
            .iter_mut()
            .find(|(line, _)| *line == literal.first_line || *line + 1 == literal.first_line);
        match exempted {
            Some(exemption) => exemption.1 = true,
            None => breaches.extend(found),
        }
    }
    breaches.extend(
        exemptions
            .iter()
            .filter(|(_, used)| !used)
            .map(|&(line, _)| (line, Breach::ExemptionUnused)),
    );

    breaches.sort();
    breaches.dedup();
    Scan {
        breaches,
        comment_lines: lexed.comments.iter().map(|c| c.lines.len()).sum(),
        literals: lexed.literals.len(),
    }
}

// ---- the tests on the repository -------------------------------------------

#[test]
fn product_prose_uses_no_semicolon_and_only_the_flat_hyphen() {
    let root = workspace_root();
    let tree = product_tree(&root);
    assert!(
        tree.unreadable.is_empty(),
        "\nfiles this test would have to pass over in silence:\n\n  {}\n\n\
         Teach `syntax_of` to read the new kind of file, or exclude it in `product_tree` or \
         `walk` with the reason written beside it. A skip nobody chose is the gap this test \
         exists to close.\n",
        tree.unreadable.join("\n  ")
    );

    let mut findings = Vec::new();
    let mut seen = [0usize; 9];
    let mut comment_lines = 0;
    let mut literals = 0;
    for (path, syntax) in &tree.files {
        let body = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("{} must be readable as text: {e}", path.display()));
        seen[*syntax as usize] += 1;
        let result = scan(*syntax, &body);
        comment_lines += result.comment_lines;
        literals += result.literals;
        let source_lines: Vec<&str> = body.split('\n').collect();
        for (line, breach) in result.breaches {
            let text = source_lines.get(line - 1).map_or("", |l| l.trim());
            findings.push(format!(
                "  {}:{line}  {}\n      {text}",
                relative(&root, path),
                breach.describe()
            ));
        }
    }

    // Without these, a clean result would also be what a wrong path or a blind
    // lexer produces.
    let [
        rust,
        slint,
        toml,
        git,
        markdown,
        svg,
        resource,
        yaml,
        python,
    ] = seen;
    assert!(
        rust >= 50,
        "read {rust} Rust files, expected at least 50 - the walk looked in the wrong place"
    );
    assert!(slint >= 3, "read {slint} Slint files, expected at least 3");
    assert!(
        toml >= 7,
        "read {toml} TOML files, expected the seven manifests at least"
    );
    assert!(
        git == 4,
        "read {git} git files, expected .gitignore, .gitattributes, .github/CODEOWNERS and \
         the pins in .github/scripts"
    );
    assert!(
        python >= 2,
        "read {python} Python files, expected the Semgrep gate and its tests in .github/scripts"
    );
    assert!(
        markdown >= 1,
        "read {markdown} Markdown files, expected at least 1"
    );
    assert!(svg >= 2, "read {svg} SVG drawings, expected the icon's two");
    assert!(
        resource >= 1,
        "read {resource} resource scripts, expected the icon's one"
    );
    assert!(
        yaml >= 1,
        "read {yaml} YAML files, expected the issue forms in .github/ISSUE_TEMPLATE"
    );
    assert!(
        comment_lines >= 5_000,
        "saw {comment_lines} comment lines in the whole tree - the lexer went blind"
    );
    assert!(
        literals >= 1_000,
        "saw {literals} string literals in the whole tree - the lexer went blind"
    );

    assert!(
        findings.is_empty(),
        "\nuntouchable rule 13 is broken in {} places:\n\n{}\n\n\
         Prose here ends a sentence with a full stop, or joins two with a comma or with the \
         flat hyphen between spaces. Code in a comment goes between backticks or inside a \
         fenced block. A dash other than `-` in a literal is written as an escape. A literal \
         quoting text the rule does not govern carries `{EXEMPTION}` and a reason on the line \
         above it.\n",
        findings.len(),
        findings.join("\n")
    );
}

#[test]
fn what_is_left_unread_as_memory_is_what_gitignore_keeps_out() {
    let root = workspace_root();
    let ignore =
        std::fs::read_to_string(root.join(".gitignore")).expect(".gitignore must be readable");
    let ignored: Vec<&str> = ignore
        .lines()
        .map(|l| l.trim().trim_start_matches('/').trim_end_matches('/'))
        .collect();
    let missing: Vec<&str> = MEMORY
        .iter()
        .copied()
        .filter(|m| !ignored.contains(m))
        .collect();
    assert!(
        missing.is_empty(),
        "these are passed over as project memory, but .gitignore does not keep them out of the \
         repository: {missing:?}. Either the file became part of the product and must be read, \
         or the ignore rule was lost and the memory is about to be committed."
    );
}

// ---- controls: the scanner can fail, and fails where it should ---------------
//
// A fixture writes SEMI and EMDASH for the two characters, so this file does not
// hold what it is looking for and stays readable to the test above.

fn fixture(text: &str) -> String {
    text.replace("SEMI", ";").replace("EMDASH", "\u{2014}")
}

fn found(syntax: Syntax, text: &str) -> Vec<(usize, Breach)> {
    scan(syntax, &fixture(text)).breaches
}

#[test]
fn control_every_kind_of_comment_is_read() {
    use Breach::Semicolon;
    let source =
        "// aSEMI b\n/// cSEMI d\n//! eSEMI f\n/* gSEMI h */\n/**\n * iSEMI j\n */\nfn x() {}\n";
    assert_eq!(
        found(Syntax::Rust, source),
        [
            (1, Semicolon),
            (2, Semicolon),
            (3, Semicolon),
            (4, Semicolon),
            (6, Semicolon)
        ]
    );
    assert_eq!(
        found(
            Syntax::Slint,
            "// aSEMI b\n/* cSEMI d */\nexport component X {}\n"
        ),
        [(1, Semicolon), (2, Semicolon)]
    );
    assert_eq!(
        found(Syntax::Toml, "# aSEMI b\nkey = 1\n"),
        [(1, Semicolon)]
    );
    assert_eq!(found(Syntax::Git, "# aSEMI b\n*.rs\n"), [(1, Semicolon)]);
    assert_eq!(
        found(Syntax::Markdown, "# Title\n\naSEMI b\n"),
        [(3, Semicolon)]
    );
    assert_eq!(
        found(Syntax::Rust, "fn x() {} // trailing, aSEMI b\n"),
        [(1, Semicolon)]
    );
}

#[test]
fn control_code_is_not_prose() {
    let rust = "fn x() {\n    let a = 1SEMI\n}\n/// Call `f(a)SEMI g()` here.\n/// ```\n/// let b = 2SEMI\n/// ```\n/// ``two `backticks` SEMI here``\nfn y() {}\n";
    assert_eq!(found(Syntax::Rust, rust), []);
    // A span may run over a line break inside one paragraph.
    assert_eq!(
        found(
            Syntax::Rust,
            "/// the span `a\n/// bSEMI c` closes here\nfn z() {}\n"
        ),
        []
    );
    assert_eq!(
        found(
            Syntax::Slint,
            "export component X {\n    width: 10pxSEMI\n}\n"
        ),
        []
    );
    assert_eq!(
        found(Syntax::Markdown, "Text.\n\n~~~\nlet a = 1SEMI\n~~~\n"),
        []
    );
    assert_eq!(found(Syntax::Git, "*.rs SEMI\n"), []);
}

#[test]
fn control_a_lone_backtick_hides_nothing() {
    use Breach::Semicolon;
    // An unmatched run is plain text, so what follows it is still prose.
    assert_eq!(
        found(Syntax::Rust, "/// a ` lone mark, aSEMI b\nfn x() {}\n"),
        [(1, Semicolon)]
    );
    // And a span cannot swallow the next paragraph.
    assert_eq!(
        found(
            Syntax::Rust,
            "/// open ` here\n///\n/// aSEMI b ` there\nfn x() {}\n"
        ),
        [(3, Semicolon)]
    );
    // Nor run from a doc comment into the plain comment below it.
    assert_eq!(
        found(
            Syntax::Rust,
            "/// open ` here\n// aSEMI b ` there\nfn x() {}\n"
        ),
        [(2, Semicolon)]
    );
}

#[test]
fn control_strings_and_characters_do_not_open_comments() {
    use Breach::Semicolon;
    let rust = "let u = \"a//b\"SEMI\nlet q = '\"'SEMI // aSEMI b\nlet e = '\\''SEMI // cSEMI d\nfn f<'a>(x: &'a str) -> &'a str { x } // eSEMI f\nlet r = r#\"/* no\"#SEMI\n// gSEMI h\n";
    assert_eq!(
        found(Syntax::Rust, rust),
        [
            (2, Semicolon),
            (3, Semicolon),
            (4, Semicolon),
            (6, Semicolon)
        ]
    );
    assert_eq!(found(Syntax::Toml, "key = \"a # bSEMI\"\n"), []);
    assert_eq!(
        found(Syntax::Slint, "x: \"a//b\"SEMI\n// cSEMI d\n"),
        [(2, Semicolon)]
    );
}

#[test]
fn control_a_literal_is_asked_a_narrower_question() {
    use Breach::{LiteralDash, LiteralSemicolon};
    assert_eq!(
        found(Syntax::Rust, "fn x() { f(\"oneSEMI two\") }\n"),
        [(1, LiteralSemicolon)]
    );
    assert_eq!(
        found(Syntax::Rust, "fn x() { f(\"oneSEMI `two`\") }\n"),
        [(1, LiteralSemicolon)]
    );
    // Code under test: a statement is followed by a brace, a newline or nothing.
    assert_eq!(
        found(
            Syntax::Rust,
            "fn x() { f(\"{ g(a)SEMI }\"), f(\"x = 1SEMI\\n\"), f(\"y = 2SEMI\") }\n"
        ),
        []
    );
    // A continuation joins the sentence it splits.
    assert_eq!(
        found(
            Syntax::Rust,
            "fn x() {\n    f(\"oneSEMI \\\n       two\")\n}\n"
        ),
        [(2, LiteralSemicolon)]
    );
    assert_eq!(
        found(Syntax::Rust, "fn x() { f(r#\"oneSEMI two\"#) }\n"),
        [(1, LiteralSemicolon)]
    );
    assert_eq!(
        found(Syntax::Toml, "description = \"oneSEMI two\"\n"),
        [(1, LiteralSemicolon)]
    );
    // A dash counts anywhere in a literal, and an escape is not a dash.
    assert_eq!(
        found(
            Syntax::Rust,
            "fn x() { f(\"aEMDASHb\"), f(\"a\\u{2014}b\") }\n"
        ),
        [(1, LiteralDash)]
    );
}

#[test]
fn control_dashes_in_prose_are_found_and_code_spans_keep_theirs() {
    use Breach::Dash;
    assert_eq!(
        found(
            Syntax::Rust,
            "// one EMDASH two\n/// the pair `-` and `EMDASH`\nfn x() {}\n"
        ),
        [(1, Dash)]
    );
    assert_eq!(found(Syntax::Markdown, "one EMDASH two\n"), [(1, Dash)]);
    assert_eq!(found(Syntax::Rust, "// flat - hyphen\nfn x() {}\n"), []);
}

#[test]
fn control_an_exemption_needs_a_reason_and_a_use() {
    use Breach::{ExemptionUnused, ExemptionWithoutReason, LiteralSemicolon};
    let above = "fn x() {\n    // prose-punctuation-exempt: quoted pack prose, D31\n    f(\"oneSEMI two\");\n}\n";
    assert_eq!(found(Syntax::Rust, above), []);
    let same_line = "fn x() {\n    f(\"oneSEMI two\"); // prose-punctuation-exempt: quoted pack prose, D31\n}\n";
    assert_eq!(found(Syntax::Rust, same_line), []);
    let bare = "fn x() {\n    // prose-punctuation-exempt: because\n    f(\"oneSEMI two\");\n}\n";
    assert_eq!(found(Syntax::Rust, bare), [(2, ExemptionWithoutReason)]);
    let unused = "fn x() {\n    // prose-punctuation-exempt: quoted pack prose, D31\n    f(\"one two\");\n}\n";
    assert_eq!(found(Syntax::Rust, unused), [(2, ExemptionUnused)]);
    // It reaches one line down, not two.
    let far = "fn x() {\n    // prose-punctuation-exempt: quoted pack prose, D31\n\n    f(\"oneSEMI two\");\n}\n";
    assert_eq!(
        found(Syntax::Rust, far),
        [(2, ExemptionUnused), (4, LiteralSemicolon)]
    );
}

#[test]
fn control_the_icon_files_are_read() {
    use Breach::{Dash, Semicolon};
    // A drawing's comments are prose, its markup is not.
    assert_eq!(
        found(
            Syntax::Svg,
            "<svg style=\"a:bSEMI c:d\">\n<!-- aSEMI b -->\n<!--\n  cSEMI d\n-->\n</svg>\n"
        ),
        [(2, Semicolon), (4, Semicolon)]
    );
    assert_eq!(found(Syntax::Svg, "<!-- one EMDASH two -->\n"), [(1, Dash)]);
    // A comment that never closes is read to the end and stops there.
    assert_eq!(found(Syntax::Svg, "<g/>\n<!-- aSEMI b"), [(2, Semicolon)]);
    // The resource script has the comments of C.
    assert_eq!(
        found(Syntax::Resource, "// aSEMI b\n1 ICON \"edamame.ico\"\n"),
        [(1, Semicolon)]
    );
}

#[test]
fn control_the_issue_forms_are_read() {
    use Breach::{Dash, Semicolon};
    // A form's words are values and not comments, so the whole file is prose.
    assert_eq!(
        found(
            Syntax::Yaml,
            "name: Bug\ndescription: aSEMI b\nbody:\n  - type: markdown\n"
        ),
        [(2, Semicolon)]
    );
    assert_eq!(found(Syntax::Yaml, "label: one EMDASH two\n"), [(1, Dash)]);
    // Code between backticks keeps its semicolon, as in every other kind of file.
    assert_eq!(found(Syntax::Yaml, "description: `aSEMI b`\n"), []);
}

#[test]
fn control_the_ci_scripts_are_read() {
    use Breach::{Dash, LiteralSemicolon, Semicolon};
    // A comment is prose, and a statement that ends in a semicolon is code.
    assert_eq!(
        found(Syntax::Python, "# aSEMI b\nx = 1SEMI y = 2\n"),
        [(1, Semicolon)]
    );
    // A docstring is a literal, so it is asked the narrower question.
    assert_eq!(
        found(
            Syntax::Python,
            "\"\"\"\nOne sentenceSEMI another.\n\"\"\"\n"
        ),
        [(2, LiteralSemicolon)]
    );
    assert_eq!(found(Syntax::Python, "x = 'a;b'\ny = \"c;d\"\n"), []);
    assert_eq!(found(Syntax::Python, "# one EMDASH two\n"), [(1, Dash)]);
    // A quote inside a comment opens no string, so what follows is still read.
    assert_eq!(
        found(Syntax::Python, "# it's here\n# aSEMI b\n"),
        [(2, Semicolon)]
    );
    // The pins beside the scripts are read like the git files.
    assert_eq!(
        found(Syntax::Git, "# aSEMI b\nsemgrep==1.0.0\n"),
        [(1, Semicolon)]
    );
}
