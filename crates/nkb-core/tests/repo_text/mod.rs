//! What this repository's files hold, read the way each kind of file is
//! written: the comments, the string literals and the tree of files itself.
//!
//! Shared by `prose_punctuation.rs` and `hygiene.rs`. It lives in a
//! subdirectory so that Cargo treats it as a module rather than as a test
//! binary of its own, and it is one module rather than a copy in each test
//! because two lexers of one file drift, and the second guard would then read
//! a different file than the first.
//!
//! Moved out of `prose_punctuation.rs` unchanged, apart from `prose` and `lex`,
//! which were the inside of two of its functions.

use std::path::{Path, PathBuf};

/// Entries at the root that are the project memory. Checked against `.gitignore`.
pub const MEMORY: [&str; 4] = ["docs", "tools", "CLAUDE.md", "CHANGELOG-DEV.md"];

/// Directories whose `.txt` and `.ttf` files are third-party bytes, kept as their
/// authors wrote them. A Markdown or TOML file there is ours and is read. The
/// standard licence texts the release notices quote are the SPDX License List's
/// bytes, and `.github/release/licence-texts/index.toml` pins their digests.
pub const VERBATIM_DIRS: [&str; 3] = [
    "crates/nkb-core/unicode",
    "crates/nkb-gui/ui/fonts",
    ".github/release/licence-texts",
];

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

/// Where the scripts the workflows run live, and the pins they install. The
/// scripts are Python and the pins are a `.txt`, and both are read.
const CI_SCRIPTS_DIR: &str = ".github/scripts";

fn is_leftover(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| LEFTOVER_EXTENSIONS.contains(&e))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Syntax {
    Rust,
    Slint,
    Toml,
    Git,
    Markdown,
    Svg,
    Resource,
    Yaml,
    Python,
}

/// Which comments may run together into one block. Only line comments of the
/// same kind on consecutive lines do, so a code span or a fence can span them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommentKind {
    Plain,
    OuterDoc,
    InnerDoc,
    Block,
    Document,
}

/// A comment cut into lines, with its markers removed.
pub struct Comment {
    pub first_line: usize,
    pub kind: CommentKind,
    pub lines: Vec<String>,
}

/// A literal's text with every character's line. Escapes stay as written, so
/// `\u{2014}` is not a dash. A line continuation is resolved, so a sentence
/// split across source lines reads as the one sentence it is.
pub struct Literal {
    pub first_line: usize,
    pub chars: Vec<(char, usize)>,
}

#[derive(Default)]
pub struct Lexed {
    pub comments: Vec<Comment>,
    pub literals: Vec<Literal>,
    /// Where the comments, the literals and the character literals stand, as
    /// ranges of character indices. Filled by the lexers of the kinds of file
    /// that hold code, so that [`code`] can leave only the code.
    pub skipped: Vec<(usize, usize)>,
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
            out.skipped.push((i, end));
            i = end;
        } else if at(&s, i, "/*") {
            let end = block_comment_end(&s, i, rust);
            let text: String = s[i..end].iter().collect();
            out.comments.push(block_comment(line, &text));
            out.skipped.push((i, end));
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
            let end = (close + 1 + hashes).min(s.len());
            out.skipped.push((i, end));
            i = end;
        } else if c == '"'
            || (rust && !after_ident && matches!(c, 'b' | 'c') && s.get(i + 1) == Some(&'"'))
        {
            let open = if c == '"' { i + 1 } else { i + 2 };
            let (literal, end, lines) = quoted_string(&s, open, line, rust);
            out.literals.push(literal);
            out.skipped.push((i, end));
            line += lines;
            i = end;
        } else if rust && c == '\'' {
            let end = after_quote_mark(&s, i);
            out.skipped.push((i, end));
            i = end;
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
            out.skipped.push((i, end));
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
            let end = (close + 3).min(s.len());
            out.skipped.push((i, end));
            i = end;
        } else if c == '"' || c == '\'' {
            let open = i + 1;
            let mut j = open;
            while j < s.len() && s[j] != c && s[j] != '\n' {
                j += if c == '"' && s[j] == '\\' { 2 } else { 1 };
            }
            let close = j.min(s.len());
            out.literals
                .push(toml_literal(&s[open..close], line, c == '"'));
            out.skipped.push((i, (close + 1).min(s.len())));
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
        ..Lexed::default()
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
        ..Lexed::default()
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
pub fn blocks(comments: &[Comment]) -> Vec<Vec<(usize, &str)>> {
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

/// The prose of one block, paragraph by paragraph. Fenced lines are code, a
/// blank line ends a paragraph, and the code spans of a paragraph are left
/// out. A code span never crosses a paragraph.
pub fn prose(block: &[(usize, &str)]) -> Vec<Vec<(char, usize)>> {
    let mut out = Vec::new();
    let mut paragraph: Vec<(char, usize)> = Vec::new();
    let mut fenced = false;
    for &(line, text) in block {
        let trimmed = text.trim();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fenced = !fenced;
            out.push(outside_code_spans(&paragraph));
            paragraph.clear();
        } else if trimmed.is_empty() && !fenced {
            out.push(outside_code_spans(&paragraph));
            paragraph.clear();
        } else if !fenced {
            paragraph.extend(text.chars().map(|c| (c, line)));
            paragraph.push(('\n', line));
        }
    }
    out.push(outside_code_spans(&paragraph));
    out
}

fn backtick_run(chars: &[(char, usize)], at: usize) -> usize {
    chars[at..].iter().take_while(|(c, _)| *c == '`').count()
}

/// A code span opens with a run of backticks and closes at the next run of
/// the same length. A run with no partner is plain text, as in CommonMark.
fn outside_code_spans(chars: &[(char, usize)]) -> Vec<(char, usize)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].0 == '`' {
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
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// One file, read the way its kind of file is written.
pub fn lex(syntax: Syntax, source: &str) -> Lexed {
    match syntax {
        Syntax::Rust => lex_c_like(source, true),
        Syntax::Slint | Syntax::Resource => lex_c_like(source, false),
        // Python has the comments and the three string forms that TOML has, and
        // the CI scripts use no other form that would read differently.
        Syntax::Toml | Syntax::Python => lex_toml(source),
        Syntax::Git => lex_git(source),
        Syntax::Markdown => lex_markdown(source),
        Syntax::Svg => lex_svg(source),
        Syntax::Yaml => lex_markdown(source),
    }
}

/// The source with every comment and literal blanked and its line breaks
/// kept, so that what is left is code and every name stays on its own line.
pub fn code(source: &str, lexed: &Lexed) -> String {
    let mut out: Vec<char> = source.chars().collect();
    let len = out.len();
    for &(start, end) in &lexed.skipped {
        for c in &mut out[start.min(len)..end.min(len)] {
            if *c != '\n' {
                *c = ' ';
            }
        }
    }
    out.into_iter().collect()
}

// ---- the tree --------------------------------------------------------------

pub fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the workspace root must be two levels above this package")
        .to_path_buf()
}

pub fn relative(root: &Path, path: &Path) -> String {
    let rel = path.strip_prefix(root).unwrap_or(path);
    rel.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

pub fn syntax_of(path: &Path) -> Option<Syntax> {
    match path.extension()?.to_str()? {
        "rs" => Some(Syntax::Rust),
        "slint" => Some(Syntax::Slint),
        "toml" => Some(Syntax::Toml),
        "md" => Some(Syntax::Markdown),
        "svg" => Some(Syntax::Svg),
        "yml" | "yaml" => Some(Syntax::Yaml),
        "py" => Some(Syntax::Python),
        "rc" => Some(Syntax::Resource),
        _ => None,
    }
}

#[derive(Default)]
pub struct Tree {
    pub files: Vec<(PathBuf, Syntax)>,
    pub unreadable: Vec<String>,
}

/// Every file this repository carries that holds prose, and every file this
/// test would have to guess about.
pub fn product_tree(root: &Path) -> Tree {
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
        // The file GitHub reads to ask the owner for a review. It has no extension
        // and its comments are `#` lines, so it is read the way the git files are.
        if here == ".github" && name == "CODEOWNERS" {
            tree.files.push((path, Syntax::Git));
            continue;
        }
        let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        // The pinned requirements of the CI scripts: `#` lines and the package
        // pins that nothing here treats as prose.
        if here == CI_SCRIPTS_DIR && extension == "txt" {
            tree.files.push((path, Syntax::Git));
            continue;
        }
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
