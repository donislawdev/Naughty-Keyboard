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
//!   `#` in TOML and in the two git files at the root. Code quoted in a
//!   comment is syntax, not prose, and is recognised the way rustdoc does it:
//!   between backticks, or between fence lines of three backticks or tildes. A
//!   block indented by four spaces is NOT recognised as code. Fence it.
//! - Every Markdown file, with the same two ways out for code.
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

use std::path::{Path, PathBuf};

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

/// Entries at the root that are the project memory. Checked against `.gitignore`.
const MEMORY: [&str; 4] = ["docs", "tools", "CLAUDE.md", "CHANGELOG-DEV.md"];

/// Directories whose `.txt` and `.ttf` files are third-party bytes, kept as their
/// authors wrote them. A Markdown file there is ours and is read.
const VERBATIM_DIRS: [&str; 2] = ["crates/nkb-core/unicode", "crates/nkb-gui/ui/fonts"];

/// Output a tool writes next to the source: the build leftovers the root
/// `.gitignore` names, and logs. Never tracked, sometimes on disk. Measured on
/// the Linux build copy, where a saved test log at the root was reported as a
/// file this test cannot read.
const LEFTOVER_EXTENSIONS: [&str; 4] = ["bk", "log", "pdb", "pyc"];

/// Where the application icon lives. Its `.ico` is binary and is not read.
const ICON_DIR: &str = "crates/nkb-gui/assets";

/// Where the repository's social preview lives. Its `.png` is a picture and is
/// not read.
const SOCIAL_PREVIEW_DIR: &str = ".github/social-preview";

fn is_leftover(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| LEFTOVER_EXTENSIONS.contains(&e))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Syntax {
    Rust,
    Slint,
    Toml,
    Git,
    Markdown,
    Svg,
    Resource,
}

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

/// Which comments may run together into one block. Only line comments of the
/// same kind on consecutive lines do, so a code span or a fence can span them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CommentKind {
    Plain,
    OuterDoc,
    InnerDoc,
    Block,
    Document,
}

/// A comment cut into lines, with its markers removed.
struct Comment {
    first_line: usize,
    kind: CommentKind,
    lines: Vec<String>,
}

/// A literal's text with every character's line. Escapes stay as written, so
/// `\u{2014}` is not a dash. A line continuation is resolved, so a sentence
/// split across source lines reads as the one sentence it is.
struct Literal {
    first_line: usize,
    chars: Vec<(char, usize)>,
}

#[derive(Default)]
struct Lexed {
    comments: Vec<Comment>,
    literals: Vec<Literal>,
}

/// What one file holds, and how much of it was looked at.
struct Scan {
    breaches: Vec<(usize, Breach)>,
    comment_lines: usize,
    literals: usize,
}

fn at(s: &[char], i: usize, what: &str) -> bool {
    what.chars()
        .enumerate()
        .all(|(k, c)| s.get(i + k) == Some(&c))
}

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn line_end(s: &[char], i: usize) -> usize {
    s[i..]
        .iter()
        .position(|&c| c == '\n')
        .map_or(s.len(), |p| i + p)
}

fn count_newlines(s: &[char]) -> usize {
    s.iter().filter(|&&c| c == '\n').count()
}

// ---- lexers ----------------------------------------------------------------

/// Rust when `rust` is true, Slint otherwise. They share comments and the
/// double-quoted string. Rust adds raw strings, byte and C string prefixes,
/// character literals and nested block comments. Slint adds `\{...}`
/// interpolation inside a string, which is code.
fn lex_c_like(source: &str, rust: bool) -> Lexed {
    let s: Vec<char> = source.chars().collect();
    let mut out = Lexed::default();
    let mut line = 1;
    let mut i = 0;
    while i < s.len() {
        let c = s[i];
        let after_ident = i > 0 && is_ident(s[i - 1]);
        if c == '\n' {
            line += 1;
            i += 1;
        } else if at(&s, i, "//") {
            let end = line_end(&s, i);
            let text: String = s[i..end].iter().collect();
            out.comments.push(line_comment(line, &text));
            i = end;
        } else if at(&s, i, "/*") {
            let end = block_comment_end(&s, i, rust);
            let text: String = s[i..end].iter().collect();
            out.comments.push(block_comment(line, &text));
            line += count_newlines(&s[i..end]);
            i = end;
        } else if let Some((open, hashes)) = raw_string_open(&s, i).filter(|_| rust && !after_ident)
        {
            let close = raw_string_close(&s, open, hashes);
            let first_line = line + count_newlines(&s[i..open]);
            let mut chars = Vec::new();
            let mut at_line = first_line;
            for &ch in &s[open..close] {
                chars.push((ch, at_line));
                if ch == '\n' {
                    at_line += 1;
                }
            }
            out.literals.push(Literal { first_line, chars });
            line = at_line;
            i = (close + 1 + hashes).min(s.len());
        } else if c == '"'
            || (rust && !after_ident && matches!(c, 'b' | 'c') && s.get(i + 1) == Some(&'"'))
        {
            let open = if c == '"' { i + 1 } else { i + 2 };
            let (literal, end, lines) = quoted_string(&s, open, line, rust);
            out.literals.push(literal);
            line += lines;
            i = end;
        } else if rust && c == '\'' {
            i = after_quote_mark(&s, i);
        } else {
            i += 1;
        }
    }
    out
}

fn line_comment(line: usize, text: &str) -> Comment {
    let (kind, body) = if let Some(rest) = text.strip_prefix("///").filter(|r| !r.starts_with('/'))
    {
        (CommentKind::OuterDoc, rest)
    } else if let Some(rest) = text.strip_prefix("//!") {
        (CommentKind::InnerDoc, rest)
    } else {
        (CommentKind::Plain, text.strip_prefix("//").unwrap_or(text))
    };
    Comment {
        first_line: line,
        kind,
        lines: vec![body.to_string()],
    }
}

fn block_comment(line: usize, text: &str) -> Comment {
    let inner = text.strip_prefix("/*").unwrap_or(text);
    let inner = inner.strip_suffix("*/").unwrap_or(inner);
    let lines = inner
        .split('\n')
        .enumerate()
        .map(|(k, part)| {
            let trimmed = part.trim_start();
            match trimmed.strip_prefix('*') {
                Some(rest) if k > 0 => rest.to_string(),
                _ => part.to_string(),
            }
        })
        .collect();
    Comment {
        first_line: line,
        kind: CommentKind::Block,
        lines,
    }
}

/// Where a block comment starting at `i` ends, just past its last `*/`.
fn block_comment_end(s: &[char], i: usize, nested: bool) -> usize {
    let mut depth = 0usize;
    let mut j = i;
    while j < s.len() {
        if at(s, j, "/*") && (nested || depth == 0) {
            depth += 1;
            j += 2;
        } else if at(s, j, "*/") {
            depth = depth.saturating_sub(1);
            j += 2;
            if depth == 0 {
                return j;
            }
        } else {
            j += 1;
        }
    }
    s.len()
}

/// Where a raw string's text starts and how many `#` close it, if one opens at `i`.
fn raw_string_open(s: &[char], i: usize) -> Option<(usize, usize)> {
    let mut j = i;
    if matches!(s.get(j), Some('b' | 'c')) {
        j += 1;
    }
    if s.get(j) != Some(&'r') {
        return None;
    }
    j += 1;
    let hashes = s[j..].iter().take_while(|&&c| c == '#').count();
    j += hashes;
    (s.get(j) == Some(&'"')).then_some((j + 1, hashes))
}

fn raw_string_close(s: &[char], from: usize, hashes: usize) -> usize {
    (from..s.len())
        .find(|&j| {
            s[j] == '"'
                && s.len() - (j + 1) >= hashes
                && s[j + 1..j + 1 + hashes].iter().all(|&c| c == '#')
        })
        .unwrap_or(s.len())
}

/// Reads a double-quoted string whose text starts at `open`. Returns the
/// literal, the index just past the closing quote and the newlines crossed.
fn quoted_string(
    s: &[char],
    open: usize,
    first_line: usize,
    rust: bool,
) -> (Literal, usize, usize) {
    let mut chars = Vec::new();
    let mut line = first_line;
    let mut j = open;
    while j < s.len() && s[j] != '"' {
        let c = s[j];
        if c == '\\' && rust && s.get(j + 1) == Some(&'\n') {
            // A continuation: the newline and the indentation after it are not
            // part of the string.
            j += 1;
            while j < s.len() && s[j].is_whitespace() {
                if s[j] == '\n' {
                    line += 1;
                }
                j += 1;
            }
        } else if c == '\\' && !rust && s.get(j + 1) == Some(&'{') {
            // Slint interpolation is an expression, not text.
            let mut depth = 0usize;
            j += 1;
            while j < s.len() {
                match s[j] {
                    '{' => depth += 1,
                    '}' => depth = depth.saturating_sub(1),
                    '\n' => line += 1,
                    _ => {}
                }
                j += 1;
                if depth == 0 {
                    break;
                }
            }
        } else if c == '\\' {
            chars.push((c, line));
            if let Some(&next) = s.get(j + 1) {
                chars.push((next, line));
                if next == '\n' {
                    line += 1;
                }
            }
            j += 2;
        } else {
            chars.push((c, line));
            if c == '\n' {
                line += 1;
            }
            j += 1;
        }
    }
    (
        Literal { first_line, chars },
        (j + 1).min(s.len()),
        line - first_line,
    )
}

/// Past a character literal, or past the mark of a lifetime or a label.
fn after_quote_mark(s: &[char], i: usize) -> usize {
    if s.get(i + 1) == Some(&'\\') {
        // `'\n'`, `'\''`, `'\u{1F600}'`: the escaped character is skipped
        // before looking for the closing mark, so `'\''` closes where it should.
        let close = (i + 3..s.len()).find(|&j| s[j] == '\'' || s[j] == '\n');
        return close.map_or(s.len(), |j| j + 1);
    }
    if s.get(i + 2) == Some(&'\'') {
        return i + 3;
    }
    i + 1
}

fn lex_toml(source: &str) -> Lexed {
    let s: Vec<char> = source.chars().collect();
    let mut out = Lexed::default();
    let mut line = 1;
    let mut i = 0;
    while i < s.len() {
        let c = s[i];
        if c == '\n' {
            line += 1;
            i += 1;
        } else if c == '#' {
            let end = line_end(&s, i);
            let body: String = s[i + 1..end].iter().collect();
            out.comments.push(Comment {
                first_line: line,
                kind: CommentKind::Plain,
                lines: vec![body],
            });
            i = end;
        } else if at(&s, i, "\"\"\"") || at(&s, i, "'''") {
            let fence: String = s[i..i + 3].iter().collect();
            let open = i + 3;
            let close = (open..s.len())
                .find(|&j| at(&s, j, &fence))
                .unwrap_or(s.len());
            out.literals
                .push(toml_literal(&s[open..close], line, c == '"'));
            line += count_newlines(&s[i..close]);
            i = (close + 3).min(s.len());
        } else if c == '"' || c == '\'' {
            let open = i + 1;
            let mut j = open;
            while j < s.len() && s[j] != c && s[j] != '\n' {
                j += if c == '"' && s[j] == '\\' { 2 } else { 1 };
            }
            let close = j.min(s.len());
            out.literals
                .push(toml_literal(&s[open..close], line, c == '"'));
            i = close + 1;
        } else {
            i += 1;
        }
    }
    out
}

fn toml_literal(text: &[char], first_line: usize, escapes: bool) -> Literal {
    let mut chars = Vec::new();
    let mut line = first_line;
    let mut j = 0;
    while j < text.len() {
        let c = text[j];
        if escapes && c == '\\' && text.get(j + 1).is_some_and(|n| n.is_whitespace()) {
            // A line-ending backslash in a multi-line basic string.
            j += 1;
            while j < text.len() && text[j].is_whitespace() {
                if text[j] == '\n' {
                    line += 1;
                }
                j += 1;
            }
            continue;
        }
        chars.push((c, line));
        if c == '\n' {
            line += 1;
        }
        j += 1;
    }
    Literal { first_line, chars }
}

/// `.gitignore` and `.gitattributes`: a comment is a line that starts with `#`,
/// and nothing else in them is prose.
fn lex_git(source: &str) -> Lexed {
    let comments = source
        .split('\n')
        .enumerate()
        .filter_map(|(k, raw)| {
            let body = raw.trim_start().strip_prefix('#')?;
            Some(Comment {
                first_line: k + 1,
                kind: CommentKind::Plain,
                lines: vec![body.to_string()],
            })
        })
        .collect();
    Lexed {
        comments,
        literals: Vec::new(),
    }
}

fn lex_markdown(source: &str) -> Lexed {
    let lines = source.split('\n').map(str::to_string).collect();
    let comments = vec![Comment {
        first_line: 1,
        kind: CommentKind::Document,
        lines,
    }];
    Lexed {
        comments,
        literals: Vec::new(),
    }
}

/// An SVG drawing: a comment is what stands between `<!--` and `-->`, and
/// nothing else in it is prose. An attribute value is markup, so a semicolon in
/// a style is not a finding.
fn lex_svg(source: &str) -> Lexed {
    let s: Vec<char> = source.chars().collect();
    let mut out = Lexed::default();
    let mut line = 1;
    let mut i = 0;
    while i < s.len() {
        if at(&s, i, "<!--") {
            let open = i + 4;
            let close = (open..s.len())
                .find(|&j| at(&s, j, "-->"))
                .unwrap_or(s.len());
            let body: String = s[open..close].iter().collect();
            out.comments.push(Comment {
                first_line: line,
                kind: CommentKind::Block,
                lines: body.split('\n').map(str::to_string).collect(),
            });
            line += count_newlines(&s[i..close]);
            i = (close + 3).min(s.len());
        } else {
            if s[i] == '\n' {
                line += 1;
            }
            i += 1;
        }
    }
    out
}

// ---- prose -----------------------------------------------------------------

/// Line comments of one kind on consecutive lines, run together.
fn blocks(comments: &[Comment]) -> Vec<Vec<(usize, &str)>> {
    let mut out: Vec<Vec<(usize, &str)>> = Vec::new();
    let mut last: Option<(CommentKind, usize)> = None;
    for comment in comments {
        let single = comment.lines.len() == 1
            && matches!(
                comment.kind,
                CommentKind::Plain | CommentKind::OuterDoc | CommentKind::InnerDoc
            );
        let joins = single
            && last
                .is_some_and(|(kind, line)| kind == comment.kind && line + 1 == comment.first_line);
        if !joins {
            out.push(Vec::new());
        }
        if let Some(block) = out.last_mut() {
            for (k, text) in comment.lines.iter().enumerate() {
                block.push((comment.first_line + k, text.as_str()));
            }
        }
        last = single.then_some((comment.kind, comment.first_line));
    }
    out
}

/// Semicolons and dashes in the prose of one block. Fenced lines are code. A
/// blank line ends a paragraph, and a code span never crosses one.
fn prose_breaches(block: &[(usize, &str)], out: &mut Vec<(usize, Breach)>) {
    let mut paragraph: Vec<(char, usize)> = Vec::new();
    let mut fenced = false;
    for &(line, text) in block {
        let trimmed = text.trim();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fenced = !fenced;
            paragraph_breaches(&paragraph, out);
            paragraph.clear();
        } else if trimmed.is_empty() && !fenced {
            paragraph_breaches(&paragraph, out);
            paragraph.clear();
        } else if !fenced {
            paragraph.extend(text.chars().map(|c| (c, line)));
            paragraph.push(('\n', line));
        }
    }
    paragraph_breaches(&paragraph, out);
}

fn backtick_run(chars: &[(char, usize)], at: usize) -> usize {
    chars[at..].iter().take_while(|(c, _)| *c == '`').count()
}

/// A code span opens with a run of backticks and closes at the next run of
/// the same length. A run with no partner is plain text, as in CommonMark.
fn paragraph_breaches(chars: &[(char, usize)], out: &mut Vec<(usize, Breach)>) {
    let mut i = 0;
    while i < chars.len() {
        let (c, line) = chars[i];
        if c == '`' {
            let run = backtick_run(chars, i);
            let mut j = i + run;
            let mut close = None;
            while j < chars.len() {
                if chars[j].0 == '`' {
                    let other = backtick_run(chars, j);
                    if other == run {
                        close = Some(j + other);
                        break;
                    }
                    j += other;
                } else {
                    j += 1;
                }
            }
            i = close.unwrap_or(i + run);
            continue;
        }
        if c == ';' {
            out.push((line, Breach::Semicolon));
        } else if OTHER_DASHES.contains(&c) {
            out.push((line, Breach::Dash));
        }
        i += 1;
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
    let lexed = match syntax {
        Syntax::Rust => lex_c_like(source, true),
        Syntax::Slint | Syntax::Resource => lex_c_like(source, false),
        Syntax::Toml => lex_toml(source),
        Syntax::Git => lex_git(source),
        Syntax::Markdown => lex_markdown(source),
        Syntax::Svg => lex_svg(source),
    };
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

// ---- the tree --------------------------------------------------------------

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the workspace root must be two levels above this package")
        .to_path_buf()
}

fn relative(root: &Path, path: &Path) -> String {
    let rel = path.strip_prefix(root).unwrap_or(path);
    rel.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn syntax_of(path: &Path) -> Option<Syntax> {
    match path.extension()?.to_str()? {
        "rs" => Some(Syntax::Rust),
        "slint" => Some(Syntax::Slint),
        "toml" => Some(Syntax::Toml),
        "md" => Some(Syntax::Markdown),
        "svg" => Some(Syntax::Svg),
        "rc" => Some(Syntax::Resource),
        _ => None,
    }
}

#[derive(Default)]
struct Tree {
    files: Vec<(PathBuf, Syntax)>,
    unreadable: Vec<String>,
}

/// Every file this repository carries that holds prose, and every file this
/// test would have to guess about.
fn product_tree(root: &Path) -> Tree {
    let mut tree = Tree::default();
    let Ok(entries) = std::fs::read_dir(root) else {
        return tree;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if path.is_dir() {
            match name.as_str() {
                "crates" | "tests" | ".github" => walk(root, &path, &mut tree),
                "packs" | "target" => {}
                n if MEMORY.contains(&n) || n.starts_with('.') => {}
                _ => tree.unreadable.push(format!(
                    "{name}/ is a directory at the root this test does not know"
                )),
            }
            continue;
        }
        match name.as_str() {
            ".gitignore" | ".gitattributes" => tree.files.push((path, Syntax::Git)),
            "LICENSE" | "Cargo.lock" => {}
            n if MEMORY.contains(&n) => {}
            _ => match syntax_of(&path) {
                Some(syntax) => tree.files.push((path, syntax)),
                None if name.starts_with('.') || is_leftover(&path) => {}
                None => tree.unreadable.push(format!(
                    "{name} is a file at the root this test cannot read"
                )),
            },
        }
    }
    tree
}

fn walk(root: &Path, dir: &Path, tree: &mut Tree) {
    let here = relative(root, dir);
    if here == "tests/packs" {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        tree.unreadable.push(format!("{here}/ could not be listed"));
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if path.is_dir() {
            if name != "target" && name != "__pycache__" {
                walk(root, &path, tree);
            }
            continue;
        }
        let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        match syntax_of(&path) {
            Some(syntax) => tree.files.push((path, syntax)),
            None if is_leftover(&path) || name.starts_with('.') => {}
            None if VERBATIM_DIRS.contains(&here.as_str())
                && matches!(extension, "txt" | "ttf") => {}
            // Pictures behind a directory of offsets: nothing in it is prose.
            // The drawings beside it are read, and `tests/app_icon.rs` holds the
            // file to them.
            None if here == ICON_DIR && extension == "ico" => {}
            // The picture GitHub shows beside a link: nothing in it is prose.
            // The drawing it is rendered from is read.
            None if here == SOCIAL_PREVIEW_DIR && extension == "png" => {}
            None => tree.unreadable.push(format!(
                "{} is a file this test cannot read",
                relative(root, &path)
            )),
        }
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
    let mut seen = [0usize; 7];
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
    let [rust, slint, toml, git, markdown, svg, resource] = seen;
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
        git == 2,
        "read {git} git files, expected .gitignore and .gitattributes"
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
