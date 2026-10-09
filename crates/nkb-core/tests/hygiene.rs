//! What this repository is written in, and with what characters.
//!
//! # Why these are tests
//!
//! Three rules about every file of the repository, none of which the compiler
//! keeps and none of which anybody keeps by reading 60 000 lines:
//!
//! - **H7c.** No file carries a character nobody can see. Untouchable rule 22
//!   in the project memory, and the reason is measured: the tool that writes
//!   files here turns the six characters of an escape into the character
//!   itself. On 2026-10-08 this test's first run found one in the issue form
//!   for suggesting a value, where the help that tells a contributor how to
//!   write a zero-width space showed an empty pair of backticks, because the
//!   escape it meant to show had become a zero-width space.
//! - **H7.** Every comment is English (untouchable rules 6 and 7: the
//!   criterion is the PLACE, not the reader). Values may be in any language,
//!   because the locale of data is not the language of the interface, so
//!   string literals are not read.
//! - **H7b.** Every name in the code is English, for the same reason.
//!
//! The project keeps a check of the first rule among its own tools, outside
//! this repository - so on no pull request. It did not read `.github/` either.
//!
//! # One reader of the files
//!
//! The files, their kinds, their comments and their literals come from
//! `repo_text`, the module `prose_punctuation.rs` reads them with. A second
//! lexer would read a second version of the same file.
//!
//! # What is not read, and why
//!
//! - `tests/packs/`: their content is the subject of the tests, invisible
//!   characters and Polish included.
//! - the third-party files kept byte for byte beside the Unicode tables and
//!   the typeface. The Unicode data names the joiner it describes by using it.
//! - names inside a Slint string interpolation, which the lexer skips with the
//!   string. An interpolation holds an expression, not a declaration.
//! - the commit messages, which this test cannot see.

// A failed expectation in a test is a failed test.
#![allow(clippy::panic, clippy::expect_used)]

#[allow(dead_code)]
mod repo_text;

use nkb_core::text::needs_escaping;
use repo_text::{
    Syntax, blocks, code, lex, product_tree, prose, relative, site_language, workspace_root,
};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// The files
// ---------------------------------------------------------------------------

/// Every text file this repository carries, with its kind when it has one:
/// the tree `repo_text` knows, the shipped packs, and the two root files the
/// prose test passes over because they hold no prose.
fn every_text_file(root: &Path) -> Vec<(PathBuf, Option<Syntax>)> {
    let mut files: Vec<(PathBuf, Option<Syntax>)> = product_tree(root)
        .files
        .into_iter()
        .map(|(path, syntax)| (path, Some(syntax)))
        .collect();
    let packs = std::fs::read_dir(root.join("packs")).expect("the shipped packs are there");
    for entry in packs {
        let path = entry.expect("a directory entry").path();
        if path.extension().is_some_and(|e| e == "toml") {
            files.push((path, Some(Syntax::Toml)));
        }
    }
    for name in ["LICENSE", "Cargo.lock"] {
        files.push((root.join(name), None));
    }
    files.sort();
    files
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("{} must be readable as text: {e}", path.display()))
}

// ---------------------------------------------------------------------------
// H7c. Nothing in a file that nobody can see
// ---------------------------------------------------------------------------

/// Every character of `text` that nobody can see, with its line.
///
/// The rule is the product's own: `nkb_core::text::needs_escaping`, which the
/// pack format, the palette preview and the project's own check of its
/// documents all read (`D79`). A list written here would be a fourth version
/// of one idea, and the third drifted once already.
///
/// Two characters are let through: the tab, which indents, and the variation
/// selector right after the warning sign, which makes that sign the coloured
/// one and is how this repository writes it in prose. Any other variation
/// selector is a finding. A line break ends a line, and a carriage return is a
/// finding, because the repository normalises every file to `\n` and a lone
/// one is a character a reader cannot see.
fn invisible_characters(text: &str) -> Vec<(usize, char)> {
    let mut found = Vec::new();
    let mut line = 1;
    let mut previous = '\n';
    for c in text.chars() {
        let after_warning_sign = c == '\u{FE0F}' && previous == '\u{26A0}';
        if c == '\n' {
            line += 1;
        } else if c != '\t' && !after_warning_sign && needs_escaping(c, false) {
            found.push((line, c));
        }
        previous = c;
    }
    found
}

#[test]
fn no_file_carries_a_character_nobody_can_see() {
    let root = workspace_root();
    let files = every_text_file(&root);
    let mut offenders = Vec::new();
    let mut packs = 0;
    for (path, _) in &files {
        let at = relative(&root, path);
        if at.starts_with("packs/") {
            packs += 1;
        }
        for (line, c) in invisible_characters(&read(path)) {
            offenders.push(format!("{at}:{line} U+{:04X}", u32::from(c)));
        }
    }

    println!(
        "invisible-character scan: {} files, {packs} shipped packs",
        files.len()
    );
    // Literals, not counts derived from what they check: 202 files
    // and nine shipped packs when this was written. Fewer means the walk lost
    // a tree.
    assert!(files.len() >= 180, "read only {} files", files.len());
    assert!(packs >= 9, "read only {packs} shipped packs");
    assert!(
        offenders.is_empty(),
        "these lines carry a character that shows as nothing. Write it as an escape \
         (`\\u{{200B}}`), and check the bytes after saving - the editor that wrote it may \
         have turned the escape into the character again: {offenders:#?}"
    );
}

#[test]
fn the_invisible_character_scan_finds_what_it_should_and_nothing_else() {
    let zero_width = '\u{200B}';
    let text = format!(
        "plain\n\tindented\na{zero_width}b\n\u{26A0}\u{FE0F} warning\nheart \u{2764}\u{FE0F}\n\
         a\u{00A0}b\nend\r\n"
    );
    assert_eq!(
        invisible_characters(&text),
        [(3, zero_width), (5, '\u{FE0F}'), (6, '\u{00A0}'), (7, '\r')],
        "the scan missed a character, or caught the tab or the warning sign"
    );
    // A character the product's table names as ignorable, far from the two
    // the rule above was first written for.
    assert_eq!(invisible_characters("\u{3164}"), [(1, '\u{3164}')]);
}

// ---------------------------------------------------------------------------
// H7. Every comment is English
// ---------------------------------------------------------------------------

/// Polish diacritics.
const POLISH_LETTERS: &[char] = &[
    'ą', 'ć', 'ę', 'ł', 'ń', 'ó', 'ś', 'ź', 'ż', 'Ą', 'Ć', 'Ę', 'Ł', 'Ń', 'Ó', 'Ś', 'Ź', 'Ż',
];

/// Polish words that survive without their diacritics, which a letter scan
/// would miss. Every one is a word that cannot be English, because a list
/// that reddens on English gets suppressed rather than fixed - `to`, `me` and
/// `pole` are Polish too, and are not here.
///
/// The first twenty-one come from the project this guard was modelled on,
/// where each one was found in English text. The rest are the words of this
/// project's own work. The list only grows.
const POLISH_WORDS_WITHOUT_DIACRITICS: &[&str] = &[
    "plasterek",
    "jawne",
    "sonda",
    "straznik",
    "bramka",
    "wlasciciel",
    "zmierzone",
    "cisza",
    "wiec",
    "dlatego",
    "poniewaz",
    "kolejnosc",
    "sekcji",
    "urwany",
    "nie",
    "czy",
    "dla",
    "jak",
    "tak",
    "rdzeni",
    "interfejs",
    "wartosc",
    "wartosci",
    "paczka",
    "paczki",
    "okno",
    "skrot",
    "przyrzad",
    "nazwa",
    "obok",
    "inny",
    "wynik",
    "dowod",
    "kontrola",
    "zestaw",
    "tylko",
    "jest",
    "albo",
    "oraz",
    "czyli",
    "przez",
    "teraz",
    "gdzie",
];

/// A word of prose that names a file rather than saying anything: a path, or a
/// file of the project memory by its name. Comments here point at the tools
/// the project keeps outside this repository, and their names are Polish.
fn names_a_file(token: &str) -> bool {
    token.contains('/')
        || token.contains('\\')
        || [".md", ".ps1", ".py", ".rs", ".toml", ".slint", ".txt"]
            .iter()
            .any(|extension| {
                token
                    .trim_end_matches(|c: char| !c.is_alphanumeric())
                    .ends_with(extension)
            })
}

/// Why one paragraph of prose is not English, for every word that says so.
fn polish_in_prose(paragraph: &[(char, usize)]) -> Vec<(usize, String)> {
    let mut found = Vec::new();
    let mut token = String::new();
    let mut line = 0;
    for &(c, at) in paragraph {
        if c.is_whitespace() {
            polish_in_token(&token, line, &mut found);
            token.clear();
        } else {
            if token.is_empty() {
                line = at;
            }
            token.push(c);
        }
    }
    polish_in_token(&token, line, &mut found);
    found
}

/// Why one word of prose, as it stands between two spaces, is not English.
fn polish_in_token(token: &str, line: usize, found: &mut Vec<(usize, String)>) {
    if token.is_empty() || names_a_file(token) {
        return;
    }
    if token.chars().any(|c| POLISH_LETTERS.contains(&c)) {
        found.push((line, format!("a Polish letter in \"{token}\"")));
    }
    let lowered = token.to_lowercase();
    for word in lowered.split(|c: char| !c.is_alphanumeric()) {
        if POLISH_WORDS_WITHOUT_DIACRITICS.contains(&word) {
            found.push((line, format!("the Polish word \"{word}\"")));
        }
    }
}

/// Every non-English word in the comments of one file, with its line.
fn polish_in_comments(syntax: Syntax, source: &str) -> Vec<(usize, String)> {
    let lexed = lex(syntax, source);
    let mut found = Vec::new();
    for block in blocks(&lexed.comments) {
        for paragraph in prose(&block) {
            found.extend(polish_in_prose(&paragraph));
        }
    }
    found
}

/// Whether the Polish scan reads a file. Everything is read but a page of the
/// website in another language, which is that language on purpose - and only a
/// page: the comments of its language file stay English, because its words are
/// in the strings (`D118`).
fn read_for_polish(relative: &str, syntax: Syntax) -> bool {
    let elsewhere = site_language(relative).is_some_and(|l| l != "en");
    !(syntax == Syntax::Markdown && elsewhere)
}

#[test]
fn every_comment_in_the_repository_is_english() {
    let root = workspace_root();
    let mut offenders = Vec::new();
    let mut files = 0;
    for (path, syntax) in every_text_file(&root) {
        let Some(syntax) = syntax else { continue };
        if !read_for_polish(&relative(&root, &path), syntax) {
            continue;
        }
        files += 1;
        for (line, why) in polish_in_comments(syntax, &read(&path)) {
            offenders.push(format!("{}:{line}: {why}", relative(&root, &path)));
        }
    }
    println!("comment scan: {files} files");
    // 200 when this was written.
    assert!(files >= 180, "the language scan read only {files} files");
    assert!(
        offenders.is_empty(),
        "a comment in the repository is not English (untouchable rules 6 and 7 - the \
         criterion is the PLACE, not the reader). A Polish word that is data goes between \
         backticks: {offenders:#?}"
    );
}

#[test]
fn the_comment_scan_reads_prose_and_leaves_code_paths_and_values_alone() {
    let rust = "// a plain comment\n// nie po angielsku\n/// the value `Wartość:` is data\n\
                // measured by tools/sonda-okno and in 08-TEKSTY-DLA-UZYTKOWNIKA.md\n\
                fn x() -> &'static str { \"nie i wartość\" }\n/* zażółć */\n";
    let found: Vec<usize> = polish_in_comments(Syntax::Rust, rust)
        .into_iter()
        .map(|(line, _)| line)
        .collect();
    assert_eq!(
        found,
        [2, 6],
        "the scan missed a Polish comment, or read a code span, a path or a literal"
    );
    // Markdown is all prose, and a fence is code.
    let markdown = "# Title\n\n```\nnie\n```\n\nWord jest here.\n";
    let found: Vec<usize> = polish_in_comments(Syntax::Markdown, markdown)
        .into_iter()
        .map(|(line, _)| line)
        .collect();
    assert_eq!(found, [7]);
    // A whole word only: `Danie` is not `nie`, and `dlatego` is.
    assert!(polish_in_prose(&chars_of("Danie tablets")).is_empty());
    assert_eq!(polish_in_prose(&chars_of("so, dlatego.")).len(), 1);
    // A template of the website: both kinds of comment are prose, the markup
    // and the template code around them are not.
    let template = "<!-- nie -->\n<p>{{ T \"nie\" }}</p>\n{{- /* dlatego */ -}}\n{{/* fine */}}\n";
    let found: Vec<usize> = polish_in_comments(Syntax::Html, template)
        .into_iter()
        .map(|(line, _)| line)
        .collect();
    assert_eq!(
        found,
        [1, 3],
        "a template comment was missed, or template code was read"
    );
}

/// The website may speak another language in three places and nowhere else
/// (untouchable rule 7 as extended by `D118`). Every other path answers `None`,
/// and the Polish scan above reads it.
#[test]
fn the_website_keeps_each_other_language_in_files_marked_with_it() {
    for (path, language) in [
        ("web/content/pl/docs/getting-started.md", Some("pl")),
        ("web/content/en/_index.md", Some("en")),
        ("web/i18n/pl.toml", Some("pl")),
        ("web/data/translations/pl/packs.toml", Some("pl")),
        ("web/layouts/home.html", None),
        ("web/data/facts/catalogue.json", None),
        ("web/i18n/old/pl.toml", None),
        ("web/content/pl.md", None),
        ("crates/nkb-cli/src/main.rs", None),
    ] {
        assert_eq!(site_language(path), language, "{path}");
    }
    // The scan passes over a page in another language, and over nothing else.
    assert!(!read_for_polish(
        "web/content/pl/docs/palette.md",
        Syntax::Markdown
    ));
    assert!(read_for_polish(
        "web/content/en/docs/palette.md",
        Syntax::Markdown
    ));
    assert!(read_for_polish("web/i18n/pl.toml", Syntax::Toml));
    assert!(read_for_polish("README.md", Syntax::Markdown));
}

fn chars_of(text: &str) -> Vec<(char, usize)> {
    text.chars().map(|c| (c, 1)).collect()
}

// ---------------------------------------------------------------------------
// H7b. Every name in the code is English
// ---------------------------------------------------------------------------

/// Words the vocabulary below takes from Polish prose that are plain English as
/// well, so a name made of them is not Polish. Each one met a real English name
/// in the code when this guard was written (2026-10-08): `datą` folds to `data`,
/// `notę` to `note`. An entry is a word that is PLAINLY English, never a way to
/// let a Polish name through.
const ALSO_ENGLISH: &[&str] = &[
    "data", "date", "formula", "literal", "note", "probe", "role", "slow", "stale",
];

/// The shortest word the vocabulary keeps. Shorter Polish words are English
/// ones too often, and the few short ones that matter are in the hand list.
const VOCABULARY_WORD_MIN: usize = 4;

/// A Polish word in the ASCII form a name would carry it in.
fn fold_polish(word: &str) -> String {
    word.chars()
        .map(|c| match c {
            'ą' => 'a',
            'ć' => 'c',
            'ę' => 'e',
            'ł' => 'l',
            'ń' => 'n',
            'ó' => 'o',
            'ś' => 's',
            'ź' | 'ż' => 'z',
            other => other,
        })
        .collect()
}

/// Where the Polish words came from, said in the test's output.
enum Vocabulary {
    /// The project memory is on this machine: the hand list and every word of
    /// its Polish documents that carries a Polish letter.
    WithTheMemory(BTreeSet<String>),
    /// A clone of this repository alone, as in CI: the hand list only.
    HandListOnly(BTreeSet<String>),
}

impl Vocabulary {
    fn words(&self) -> &BTreeSet<String> {
        match self {
            Self::WithTheMemory(words) | Self::HandListOnly(words) => words,
        }
    }
}

/// The Polish words a name might carry, in the ASCII form it would carry them.
///
/// 🔴 Built from the project's own Polish documents rather than written out,
/// because a hand list only ever holds the words somebody already thought of.
/// Those documents are the project memory, which this repository does not
/// carry - so a clone of it alone, as in CI, reads the hand list only, and
/// says so. The owner chose that trade on 2026-10-08: a name that would only
/// redden here reddens on the machine where it is written, before it is pushed.
///
/// Only words that carry a Polish letter are taken. The documents quote code,
/// commands and English, and the letter is what says a word is Polish.
fn polish_vocabulary(root: &Path) -> Vocabulary {
    let mut words: BTreeSet<String> = POLISH_WORDS_WITHOUT_DIACRITICS
        .iter()
        .map(|w| (*w).to_string())
        .collect();
    let docs = root.join("docs");
    let memory_root_file = root.join("CLAUDE.md");
    assert_eq!(
        docs.is_dir(),
        memory_root_file.is_file(),
        "half of the project memory is here: docs/ and CLAUDE.md come and go together, \
         so one without the other means a path in this test is wrong"
    );
    if !docs.is_dir() {
        return Vocabulary::HandListOnly(words);
    }
    let mut sources = vec![memory_root_file, root.join("CHANGELOG-DEV.md")];
    for entry in std::fs::read_dir(&docs).expect("docs/ can be listed") {
        let path = entry.expect("a directory entry").path();
        if path.extension().is_some_and(|e| e == "md") {
            sources.push(path);
        }
    }
    for path in sources {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        for word in text.split(|c: char| !c.is_alphabetic()) {
            let lowered = word.to_lowercase();
            if !lowered.chars().any(|c| POLISH_LETTERS.contains(&c)) {
                continue;
            }
            let folded = fold_polish(&lowered);
            if folded.chars().count() >= VOCABULARY_WORD_MIN
                && !ALSO_ENGLISH.contains(&folded.as_str())
            {
                words.insert(folded);
            }
        }
    }
    Vocabulary::WithTheMemory(words)
}

/// Split a name into its words, lowercase: at underscores, hyphens and
/// digits, and where the case turns (`nazwaSekcji`, `HTTPServer` into `http`
/// and `server`).
fn name_words(name: &str) -> Vec<String> {
    let chars: Vec<char> = name.chars().collect();
    let mut words = Vec::new();
    let mut current = String::new();
    for (k, &c) in chars.iter().enumerate() {
        let previous = k.checked_sub(1).map(|p| chars[p]);
        let next = chars.get(k + 1).copied();
        let turns = c.is_uppercase()
            && previous.is_some_and(|p| {
                p.is_lowercase() || (p.is_uppercase() && next.is_some_and(char::is_lowercase))
            });
        if !c.is_alphabetic() || turns {
            if !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
            if !c.is_alphabetic() {
                continue;
            }
        }
        current.extend(c.to_lowercase());
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

/// Why a name is Polish, or `None` when it is not.
fn polish_in_name(name: &str, vocabulary: &BTreeSet<String>) -> Option<String> {
    if name.chars().any(|c| POLISH_LETTERS.contains(&c)) {
        return Some("a Polish letter".to_string());
    }
    name_words(name)
        .into_iter()
        .find(|w| vocabulary.contains(w))
        .map(|w| format!("the Polish word \"{w}\""))
}

/// The names in the code of one file, each with its line: what is left once
/// the comments and the literals are blanked.
fn names_in(syntax: Syntax, source: &str) -> Vec<(usize, String)> {
    let lexed = lex(syntax, source);
    let mut names = Vec::new();
    for (index, line) in code(source, &lexed).lines().enumerate() {
        let mut name = String::new();
        for c in line.chars().chain(std::iter::once(' ')) {
            if c.is_alphanumeric() || c == '_' {
                name.push(c);
            } else if !name.is_empty() {
                if !name.starts_with(|first: char| first.is_ascii_digit()) {
                    names.push((index + 1, name.clone()));
                }
                name.clear();
            }
        }
    }
    names
}

/// The kinds of file whose code holds names.
fn holds_names(syntax: Syntax) -> bool {
    matches!(
        syntax,
        Syntax::Rust
            | Syntax::Slint
            | Syntax::Resource
            | Syntax::Toml
            | Syntax::Python
            | Syntax::Shell
    )
}

#[test]
fn every_name_in_the_code_is_english() {
    let root = workspace_root();
    let vocabulary = polish_vocabulary(&root);
    let words = vocabulary.words();
    // Two canaries: a vocabulary that read nothing would pass every name, and
    // one that read the wrong documents would pass the ones that matter.
    for word in ["wartosc", "paczka", "nazwa", "sekcji"] {
        assert!(
            words.contains(word),
            "the vocabulary does not hold \"{word}\""
        );
    }
    if let Vocabulary::WithTheMemory(words) = &vocabulary {
        assert!(
            words.len() >= 5_000,
            "the vocabulary has only {} words",
            words.len()
        );
        // Words only the documents can supply: none is in the hand list.
        for word in ["wlasciwosc", "przypadkow", "dzialanie"] {
            assert!(
                words.contains(word),
                "the vocabulary does not hold \"{word}\""
            );
        }
    }

    let mut offenders = BTreeSet::new();
    let mut names = 0usize;
    for (path, syntax) in every_text_file(&root) {
        let Some(syntax) = syntax.filter(|s| holds_names(*s)) else {
            continue;
        };
        let found = names_in(syntax, &read(&path));
        names += found.len();
        for (line, name) in found {
            if let Some(why) = polish_in_name(&name, words) {
                offenders.insert(format!("{}:{line}: {name} ({why})", relative(&root, &path)));
            }
        }
    }
    let source = match vocabulary {
        Vocabulary::WithTheMemory(_) => "the hand list and the project memory",
        Vocabulary::HandListOnly(_) => "the hand list only - the project memory is not here",
    };
    println!(
        "name scan: {names} names against {} Polish words from {source}",
        words.len()
    );
    // 123 524 when this was written.
    assert!(
        names >= 100_000,
        "the name scan read only {names} names - it is reading the wrong files"
    );
    assert!(
        offenders.is_empty(),
        "a name in the code is Polish (untouchable rule 6). Rename it in English - or, if the \
         word is plainly English as well, add it to ALSO_ENGLISH: {offenders:#?}"
    );
}

#[test]
fn the_name_scan_reads_code_and_skips_comments_and_literals() {
    let rust = "fn nazwa() {\n    let s = \"obok\"; // inny\n    let r = r#\"multi\nline \"quoted\" obok\"#;\n    \
                /* outer /* nested */ inny */ let c = '\\'';\n    let q = b'x';\n    fn f<'a>(x: &'a str) {}\n}";
    let names: Vec<String> = names_in(Syntax::Rust, rust)
        .into_iter()
        .map(|(_, n)| n)
        .collect();
    // The prefix of a byte literal reads as a name of its own, `b`, which costs
    // nothing: a one-letter name is never Polish.
    assert_eq!(
        names,
        [
            "fn", "nazwa", "let", "s", "let", "r", "let", "c", "let", "q", "b", "fn", "f", "a",
            "x", "a", "str"
        ]
    );
    let lines: Vec<usize> = names_in(Syntax::Rust, rust)
        .into_iter()
        .map(|(l, _)| l)
        .collect();
    assert_eq!(
        lines[5], 3,
        "the name in front of the raw string, on its own line"
    );
    assert_eq!(
        lines[6], 5,
        "the name after a string that spans a line keeps its own line"
    );
    // Slint names carry hyphens, and a string is not a name.
    let slint =
        "export component Okno inherits Window {\n    title: \"nazwa\";\n    text-muted: 1px;\n}\n";
    let names: Vec<String> = names_in(Syntax::Slint, slint)
        .into_iter()
        .map(|(_, n)| n)
        .collect();
    assert_eq!(
        names,
        [
            "export",
            "component",
            "Okno",
            "inherits",
            "Window",
            "title",
            "text",
            "muted"
        ],
        "a number with its unit, `1px`, is not a name"
    );
}

#[test]
fn a_polish_word_is_found_wherever_it_sits_in_a_name() {
    let vocabulary = polish_vocabulary(&workspace_root());
    let words = vocabulary.words();
    for name in [
        "nazwa_sekcji",
        "NazwaSekcji",
        "nazwaSekcji",
        "SEKCJI_COUNT",
        "rowObok",
        "zażółć",
        "Okno",
    ] {
        assert!(polish_in_name(name, words).is_some(), "{name}");
    }
    for name in [
        "section_name",
        "HTTPServer",
        "obokeh",
        "unazwa",
        "stale_note",
        "data",
    ] {
        assert!(polish_in_name(name, words).is_none(), "{name}");
    }
    assert_eq!(
        name_words("HTTPServerName2x"),
        ["http", "server", "name", "x"]
    );
}

/// The two ways the vocabulary is built, on folders made for it: a clone with
/// no project memory reads the hand list and says so, a machine with the
/// memory reads its Polish words too, and half a memory is a wrong path rather
/// than a smaller vocabulary.
#[test]
fn the_vocabulary_says_where_it_came_from() {
    let base = std::env::temp_dir().join(format!("nkb-hygiene-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);

    let clone = base.join("clone");
    std::fs::create_dir_all(&clone).expect("a folder can be made");
    assert!(matches!(
        polish_vocabulary(&clone),
        Vocabulary::HandListOnly(_)
    ));

    let memory = base.join("memory");
    std::fs::create_dir_all(memory.join("docs")).expect("a folder can be made");
    std::fs::write(memory.join("CLAUDE.md"), "Stan.\n").expect("a file can be made");
    std::fs::write(memory.join("docs").join("a.md"), "Właściwość pola.\n")
        .expect("a file can be made");
    match polish_vocabulary(&memory) {
        Vocabulary::WithTheMemory(words) => {
            assert!(
                words.contains("wlasciwosc"),
                "a word with a Polish letter was not taken"
            );
            assert!(
                !words.contains("pola"),
                "a word with no Polish letter was taken"
            );
        }
        Vocabulary::HandListOnly(_) => panic!("the memory was there and was not read"),
    }

    let half = base.join("half");
    std::fs::create_dir_all(half.join("docs")).expect("a folder can be made");
    let refused = std::panic::catch_unwind(|| polish_vocabulary(&half));
    let _ = std::fs::remove_dir_all(&base);
    assert!(
        refused.is_err(),
        "docs/ without CLAUDE.md was read as a whole memory"
    );
}

// ---------------------------------------------------------------------------
// The shipped part of a source file
// ---------------------------------------------------------------------------

/// Everything before the `#[cfg(test)]` that opens the test module, whatever
/// attributes stand between the two. This workspace keeps a file's tests at
/// its end, so what comes before is what ships.
fn shipped_part(text: &str) -> &str {
    // Inclusive of the line break, so the offsets hold for `\r\n` as well.
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let mut offset = 0;
    for (index, line) in lines.iter().enumerate() {
        if line.trim_end() == "#[cfg(test)]" {
            let opens = lines[index + 1..]
                .iter()
                .find(|l| l.starts_with(|c: char| c.is_ascii_alphabetic()))
                .is_some_and(|l| l.starts_with("mod "));
            if opens {
                return &text[..offset];
            }
        }
        offset += line.len();
    }
    text
}

/// Every `.rs` file under `dir`.
fn rust_files(dir: &Path, into: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries {
        let path = entry.expect("a directory entry").path();
        if path.is_dir() {
            rust_files(&path, into);
        } else if path.extension().is_some_and(|e| e == "rs") {
            into.push(path);
        }
    }
}

// ---------------------------------------------------------------------------
// H5. The pure layer never throws a result away
// ---------------------------------------------------------------------------

/// The ways a line throws a result away without looking at it.
const DISCARDS: &[&str] = &["let _ = ", ".ok();", "if let Err(_)"];

/// Every place one line of shipped code throws a result away.
fn discards_in(line: &str) -> Vec<&'static str> {
    let code = line.find("//").map_or(line, |cut| &line[..cut]);
    let mut found: Vec<&'static str> = DISCARDS
        .iter()
        .copied()
        .filter(|bad| code.contains(bad))
        .collect();
    // An assignment to the underscore discards as surely as `let _`.
    if code.trim_start().starts_with("_ = ") {
        found.push("_ = ");
    }
    found
}

/// `nkb-core` does no input or output and calls no system (`architektura.md`
/// 2), so its results are returned values and never side effects. A result
/// thrown away there is a hidden defect every time, which lets the rule be
/// absolute here and nowhere else: the adapters and the CLI drop the results
/// of writes to standard error on purpose, and a blanket ban would need an
/// exception list longer than itself.
///
/// Measured when this was written: one such line, a `write!` into a `String`
/// in `screens.rs`. Writing into a string cannot fail, so nothing was lost,
/// and it was rewritten rather than excused.
#[test]
fn the_pure_layer_never_throws_a_result_away() {
    let mut files = Vec::new();
    rust_files(&workspace_root().join("crates/nkb-core/src"), &mut files);
    let mut offenders = Vec::new();
    for path in &files {
        let text = read(path);
        for (number, line) in shipped_part(&text).lines().enumerate() {
            for bad in discards_in(line) {
                offenders.push(format!("{}:{} has `{bad}`", path.display(), number + 1));
            }
        }
    }
    println!("discard scan: {} source files of nkb-core", files.len());
    // 24 source files when this was written.
    assert!(
        files.len() >= 20,
        "the scan read only {} files",
        files.len()
    );
    assert!(
        offenders.is_empty(),
        "nkb-core threw a result away - it does no input or output, so this hides a defect \
         rather than ignoring a write nobody can read: {offenders:#?}"
    );
}

#[test]
fn the_discard_scan_finds_each_shape_and_leaves_comments_alone() {
    assert_eq!(
        discards_in("    let _ = write!(text, \"x\");"),
        ["let _ = "]
    );
    assert_eq!(discards_in("    parse(text).ok();"), [".ok();"]);
    assert_eq!(
        discards_in("    if let Err(_) = check() {"),
        ["if let Err(_)"]
    );
    assert_eq!(discards_in("    _ = check();"), ["_ = "]);
    assert!(discards_in("    // let _ = in a comment").is_empty());
    assert!(discards_in("    let value = parse(text)?;").is_empty());
    let source = "fn a() {\n    let _ = b();\n}\n\n#[cfg(test)]\n#[allow(\n    clippy::panic,\n)]\nmod tests {\n    fn c() { let _ = d(); }\n}\n";
    let shipped: Vec<&str> = shipped_part(source).lines().collect();
    assert_eq!(shipped.len(), 4, "the test module was read as shipped code");
}

// ---------------------------------------------------------------------------
// H6. Every package depends only on what its layer allows
// ---------------------------------------------------------------------------

/// What each package may depend on, ours and others' alike, in
/// `[dependencies]`, `[build-dependencies]` and their per-target forms.
///
/// The direction is `architektura.md` 4: the pure layer depends on nothing,
/// the use cases on the pure layer, the adapters on both and on the system
/// layer, and the two programs on everything below them and never on each
/// other. `nkb-gui` reaches the system only through the adapters.
///
/// 🔴 Written as the ALLOWED set rather than a forbidden one, and naming the
/// dependencies of other people as well as ours: a dependency added anywhere
/// fails here until somebody decides which layer it belongs to, and a package
/// with no entry fails `the_dependency_table_covers_every_package`. That is the
/// decision the licence check in `09-CLI-I-CI.md` 10 is made at, and this is
/// where it becomes visible in a pull request.
///
/// Not read: `[dev-dependencies]`. A test may use what it needs, and nothing
/// a test uses ships.
const ALLOWED_DEPENDENCIES: &[(&str, &[&str])] = &[
    ("nkb-core", &[]),
    ("nkb-sys", &["windows-sys"]),
    ("nkb-app", &["nkb-core"]),
    (
        "nkb-adapters",
        &["nkb-core", "nkb-app", "nkb-sys", "toml_edit"],
    ),
    ("nkb-cli", &["nkb-core", "nkb-app", "nkb-adapters"]),
    (
        "nkb-gui",
        &[
            "nkb-core",
            "nkb-app",
            "nkb-adapters",
            "slint",
            "raw-window-handle",
            "arboard",
            // Build only: the compiler of the views and the icon resource.
            "slint-build",
            "embed-resource",
        ],
    ),
];

/// Whether a table header names a table of dependencies this check reads.
fn reads_table(header: &str) -> bool {
    let name = header.rsplit('.').next().unwrap_or(header);
    (header == "dependencies" || header == "build-dependencies" || header.starts_with("target."))
        && (name == "dependencies" || name == "build-dependencies")
}

/// Every dependency a manifest declares, with the table it stands in.
///
/// Read line by line rather than parsed: the pure layer's tests depend on
/// nothing, and the manifests here use two shapes only, `name = ...` and
/// `name.workspace = true`, plus `[dependencies.name]` for a third.
fn declared_dependencies(manifest: &str) -> Vec<(String, String)> {
    let mut found = Vec::new();
    let mut table: Option<String> = None;
    for raw in manifest.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if let Some(header) = line.strip_prefix('[').and_then(|h| h.strip_suffix(']')) {
            let header = header.trim().replace(['"', '\''], "");
            table = None;
            if reads_table(&header) {
                table = Some(header);
            } else if let Some((parent, name)) = header.rsplit_once('.')
                && reads_table(parent)
            {
                found.push((parent.to_string(), name.to_string()));
            }
            continue;
        }
        let Some(table) = &table else { continue };
        let key: String = line
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
            .collect();
        let rest = line[key.len()..].trim_start();
        if !key.is_empty() && (rest.starts_with('=') || rest.starts_with('.')) {
            found.push((table.clone(), key));
        }
    }
    found
}

#[test]
fn every_package_depends_only_on_what_its_layer_allows() {
    let root = workspace_root();
    let mut offenders = Vec::new();
    let mut declared = 0;
    for (package, allowed) in ALLOWED_DEPENDENCIES.iter().copied() {
        let manifest = read(&root.join("crates").join(package).join("Cargo.toml"));
        for (table, name) in declared_dependencies(&manifest) {
            declared += 1;
            if !allowed.contains(&name.as_str()) {
                offenders.push(format!("{package} depends on '{name}' in [{table}]"));
            }
        }
    }
    println!("dependency scan: {declared} declared dependencies");
    // 17 declarations when this was written.
    assert!(declared >= 15, "read only {declared} declared dependencies");
    assert!(
        offenders.is_empty(),
        "a dependency points the wrong way between the layers of architektura.md 4, or was \
         added without deciding which layer it belongs to: {offenders:#?}"
    );
}

/// The guard for the guard above: a package with no entry is a package
/// nothing guards.
#[test]
fn the_dependency_table_covers_every_package() {
    let root = workspace_root();
    let manifest = read(&root.join("Cargo.toml"));
    let members: Vec<String> = manifest
        .lines()
        .filter_map(|l| l.trim().strip_prefix("\"crates/"))
        .map(|l| l.trim_end_matches(['"', ',']).to_string())
        .collect();
    let listed: Vec<&str> = ALLOWED_DEPENDENCIES.iter().map(|(n, _)| *n).collect();
    let missing: Vec<&String> = members
        .iter()
        .filter(|m| !listed.contains(&m.as_str()))
        .collect();
    assert!(
        members.len() >= 6,
        "read only {} workspace members",
        members.len()
    );
    assert!(
        missing.is_empty(),
        "these packages have no entry in ALLOWED_DEPENDENCIES: {missing:?}"
    );
    let gone: Vec<&&str> = listed
        .iter()
        .filter(|n| !members.iter().any(|m| m == **n))
        .collect();
    assert!(gone.is_empty(), "these entries name no package: {gone:?}");
}

#[test]
fn the_manifest_reader_finds_every_shape_of_declaration() {
    let manifest = "[package]\nname = \"x\"\n\n[dependencies]\n# a comment = 1\nnkb-core.workspace = true\n\
                    toml_edit = \"1\"\nslint = { workspace = true, features = [\n    \"a\",\n] }\n\n\
                    [build-dependencies]\nembed-resource.workspace = true\n\n\
                    [target.'cfg(windows)'.dependencies]\nwindows-sys = { workspace = true }\n\n\
                    [dev-dependencies]\nimage = \"1\"\n\n[dependencies.arboard]\nversion = \"3\"\n\n\
                    [lints]\nworkspace = true\n";
    let names: Vec<String> = declared_dependencies(manifest)
        .into_iter()
        .map(|(_, name)| name)
        .collect();
    assert_eq!(
        names,
        [
            "nkb-core",
            "toml_edit",
            "slint",
            "embed-resource",
            "windows-sys",
            "arboard"
        ],
        "a declaration was missed, or a dev-dependency, a lint or a feature was read as one"
    );
}

// ---------------------------------------------------------------------------
// H2. Code nothing calls any more
// ---------------------------------------------------------------------------
//
// `cargo clippy -D warnings` catches an unused PRIVATE item and says nothing
// about a `pub` one in a library, because another package might call it. In a
// workspace whose libraries are called only by its own two programs, a `pub`
// helper written in one session and superseded in the next keeps compiling,
// keeps passing and keeps being read as something that matters.

/// One definition the scan knows about.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Definition {
    name: String,
    file: String,
    line: usize,
}

/// Where a mention came from. `Tests` is deliberately not a consumer: a
/// definition only its own tests name is dead code with a test suite attached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Site {
    /// Inside the definition at this index in the definition list.
    Inside(usize),
    Tests,
    External,
}

/// The definitions nothing alive reaches. Life SPREADS FROM ROOTS, to a fixed
/// point: a definition named from outside every tracked definition lives, and
/// one named from inside a living one lives. Spreading from the roots rather
/// than crossing out from the leaves is what reports a CYCLE that nothing
/// outside it reaches.
fn unreferenced(
    definitions: &[Definition],
    mentions: &std::collections::BTreeMap<String, Vec<Site>>,
) -> Vec<usize> {
    let mut alive: Vec<bool> = definitions
        .iter()
        .map(|d| {
            mentions
                .get(&d.name)
                .is_some_and(|sites| sites.contains(&Site::External))
        })
        .collect();
    loop {
        let mut grew = false;
        for (index, definition) in definitions.iter().enumerate() {
            if alive[index] {
                continue;
            }
            let reached = mentions.get(&definition.name).is_some_and(|sites| {
                sites.iter().any(|site| match site {
                    Site::External => true,
                    // A test is not a consumer, and a definition naming
                    // itself is recursion.
                    Site::Tests => false,
                    Site::Inside(other) => *other != index && alive[*other],
                })
            });
            if reached {
                alive[index] = true;
                grew = true;
            }
        }
        if !grew {
            return alive
                .iter()
                .enumerate()
                .filter_map(|(i, a)| (!a).then_some(i))
                .collect();
        }
    }
}

/// 🔴 Unreferenced ON PURPOSE, each with the reason it stays. A RATCHET: it may
/// shrink and may not grow without somebody deciding that it should. A list
/// that absorbs whatever the scan finds is not a guard but a place to put
/// things.
const KNOWN_UNUSED: &[(&str, &str)] = &[(
    "UNICODE_VERSION",
    "The version of the vendored Unicode data, written once. `tests/unicode_data.rs` checks \
     every vendored file against it, and the shipped code reads the tables built from those \
     files rather than the number. The pin exists for the tests, and deleting it would leave \
     the version written nowhere.",
)];

/// Top-level `pub` definitions of one source, with the line each starts on.
///
/// Methods are not tracked: a method name such as `new`, `text` or `len`
/// stands in many types at once, and a scan of names cannot tell them apart.
/// Fewer findings and no false ones, which is the right way round for a guard
/// people have to believe.
fn pub_definitions(file: &str, text: &str) -> Vec<Definition> {
    let mut out = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let Some(rest) = ["pub ", "pub(crate) ", "pub(super) "]
            .iter()
            .find_map(|p| line.strip_prefix(p))
        else {
            continue;
        };
        let mut words: Vec<&str> = rest.split_whitespace().collect();
        // `const` is a modifier only in `const fn`. Treated as one always, it
        // would hide every `pub const NAME`.
        while let Some(first) = words.first().copied() {
            let modifier = matches!(first, "unsafe" | "async" | "extern")
                || first.starts_with('"')
                || (first == "const" && words.get(1) == Some(&"fn"));
            if !modifier {
                break;
            }
            words.remove(0);
        }
        let mut words = words.into_iter();
        let Some(kind) = words.next() else { continue };
        if ![
            "fn", "struct", "enum", "trait", "union", "type", "const", "static",
        ]
        .contains(&kind)
        {
            continue;
        }
        let name: String = words
            .next()
            .unwrap_or("")
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if name.is_empty() || name.starts_with('_') || name == "main" {
            continue;
        }
        out.push(Definition {
            name,
            file: file.to_string(),
            line: index + 1,
        });
    }
    out
}

/// Every identifier on a line, as whole words.
///
/// 🔴 EVERY word counts, one inside a comment or a string included: prose that
/// still names a symbol is a sign somebody thinks it is alive, and a guard
/// that accuses living code is a guard people learn to ignore. The price is
/// the other direction - a definition whose name is an ordinary word survives
/// on prose alone.
fn identifiers(line: &str) -> Vec<&str> {
    line.split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|w| !w.is_empty())
        .collect()
}

/// One source file as the dead-code scan reads it.
struct Source {
    file: String,
    text: String,
    /// A file under `crates/*/src`, whose top-level items are tracked.
    shipped: bool,
    /// A file whose every line is test code.
    tests: bool,
}

/// The lines of a shipped source that only import: `use` and `pub use`, to
/// the semicolon that ends them. An import is not a use, and a `pub use` in a
/// `lib.rs` would otherwise keep alive everything it re-exports, used or not.
fn import_lines(text: &str) -> BTreeSet<usize> {
    let mut lines = BTreeSet::new();
    let mut open = false;
    for (index, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        let starts = ["use ", "pub use ", "pub(crate) use ", "pub(super) use "]
            .iter()
            .any(|p| trimmed.starts_with(p));
        if starts || open {
            lines.insert(index + 1);
            open = !line.contains(';');
        }
    }
    lines
}

/// Where one mention in a shipped source stands: inside a tracked definition,
/// or somewhere the scan does not model, which counts as life.
///
/// The boundaries are every top-level item, not only the tracked ones: with
/// tracked ones alone, a mention in a plain `impl` block below a `pub fn`
/// would be read as that function calling itself.
fn site_in_shipped(line: usize, spans: &[(usize, Option<usize>)]) -> Site {
    spans
        .iter()
        .rev()
        .find(|(start, _)| *start <= line)
        .and_then(|(_, index)| *index)
        .map_or(Site::External, Site::Inside)
}

/// Everything the dead-code scan reads: the definitions, and where every
/// tracked name is mentioned from.
fn collect_rust() -> (
    Vec<Definition>,
    std::collections::BTreeMap<String, Vec<Site>>,
    usize,
) {
    let root = workspace_root();
    let mut sources = Vec::new();
    for (path, syntax) in every_text_file(&root) {
        // Markdown is prose about the code, not code: a README naming a
        // function does not call it.
        let Some(syntax) = syntax.filter(|s| holds_names(*s) || *s == Syntax::Yaml) else {
            continue;
        };
        let file = relative(&root, &path);
        let shipped =
            syntax == Syntax::Rust && file.starts_with("crates/") && file.contains("/src/");
        let tests = file.contains("/tests/");
        sources.push(Source {
            file,
            text: read(&path),
            shipped,
            tests,
        });
    }

    let mut definitions = Vec::new();
    for source in sources.iter().filter(|s| s.shipped) {
        definitions.extend(pub_definitions(&source.file, shipped_part(&source.text)));
    }
    let names: BTreeSet<&str> = definitions.iter().map(|d| d.name.as_str()).collect();

    let mut mentions: std::collections::BTreeMap<String, Vec<Site>> =
        std::collections::BTreeMap::new();
    for source in &sources {
        let shipped_lines = if source.shipped {
            shipped_part(&source.text).lines().count()
        } else {
            0
        };
        let imports = if source.shipped {
            import_lines(&source.text)
        } else {
            BTreeSet::new()
        };
        let mut spans: Vec<(usize, Option<usize>)> = Vec::new();
        if source.shipped {
            for (index, line) in source.text.lines().enumerate() {
                if line.starts_with(|c: char| c.is_ascii_lowercase()) {
                    spans.push((index + 1, None));
                }
            }
            for (index, definition) in definitions.iter().enumerate() {
                if definition.file == source.file {
                    spans.push((definition.line, Some(index)));
                }
            }
            // A definition and a bare boundary on one line: the definition wins.
            spans.sort_unstable_by_key(|(line, index)| (*line, index.is_none()));
            spans.dedup_by_key(|(line, _)| *line);
        }
        for (index, line) in source.text.lines().enumerate() {
            let number = index + 1;
            if imports.contains(&number) {
                continue;
            }
            for word in identifiers(line) {
                if !names.contains(word) {
                    continue;
                }
                let site = if source.tests || (source.shipped && number > shipped_lines) {
                    Site::Tests
                } else if source.shipped {
                    site_in_shipped(number, &spans)
                } else {
                    Site::External
                };
                mentions.entry(word.to_string()).or_default().push(site);
            }
        }
    }
    (definitions, mentions, sources.len())
}

/// Nothing in the workspace is left over from a change that moved on without it.
#[test]
fn no_public_definition_in_the_workspace_is_unreferenced() {
    let (definitions, mentions, files) = collect_rust();
    let dead = unreferenced(&definitions, &mentions);
    println!(
        "dead-code scan: {files} files, {} public definitions, {} unreferenced",
        definitions.len(),
        dead.len()
    );
    // 422 definitions in 187 files when this was written.
    assert!(
        files >= 150 && definitions.len() >= 300,
        "the dead-code scan read {files} files and found {} definitions - it is reading the \
         wrong place",
        definitions.len()
    );
    let known: Vec<&str> = KNOWN_UNUSED.iter().map(|(n, _)| *n).collect();
    let unexpected: Vec<String> = dead
        .iter()
        .map(|i| &definitions[*i])
        .filter(|d| !known.contains(&d.name.as_str()))
        .map(|d| format!("{} ({}:{})", d.name, d.file, d.line))
        .collect();
    assert!(
        unexpected.is_empty(),
        "these public definitions are named from nowhere that is alive - delete one, or add it \
         to KNOWN_UNUSED with the reason it stays: {unexpected:#?}"
    );
}

/// A name that got a caller back must LEAVE the list, or the list rots.
#[test]
fn the_known_unused_list_only_ever_shrinks() {
    let (definitions, mentions, _) = collect_rust();
    let dead: Vec<&str> = unreferenced(&definitions, &mentions)
        .iter()
        .map(|i| definitions[*i].name.as_str())
        .collect();
    let revived: Vec<&str> = KNOWN_UNUSED
        .iter()
        .map(|(n, _)| *n)
        .filter(|n| !dead.contains(n))
        .collect();
    assert!(
        revived.is_empty(),
        "these names are used again, so they must come out of KNOWN_UNUSED: {revived:?}"
    );
}

fn fixture(name: &str, line: usize) -> Definition {
    Definition {
        name: name.to_string(),
        file: "fixture.rs".to_string(),
        line,
    }
}

#[test]
fn a_definition_only_its_tests_name_is_reported_unused() {
    let definitions = vec![fixture("only_tested", 1)];
    let mentions =
        std::collections::BTreeMap::from([("only_tested".to_string(), vec![Site::Tests])]);
    assert_eq!(unreferenced(&definitions, &mentions), [0]);
}

#[test]
fn a_definition_named_from_a_living_one_is_left_alone() {
    let definitions = vec![fixture("used", 1), fixture("caller", 10)];
    let mentions = std::collections::BTreeMap::from([
        ("used".to_string(), vec![Site::Inside(1)]),
        ("caller".to_string(), vec![Site::External]),
    ]);
    assert!(unreferenced(&definitions, &mentions).is_empty());
}

#[test]
fn two_definitions_that_only_call_each_other_are_both_reported() {
    let definitions = vec![fixture("a", 1), fixture("b", 10)];
    let mentions = std::collections::BTreeMap::from([
        ("a".to_string(), vec![Site::Inside(1)]),
        ("b".to_string(), vec![Site::Inside(0)]),
    ]);
    assert_eq!(unreferenced(&definitions, &mentions), [0, 1]);
}

#[test]
fn a_definition_that_only_names_itself_is_reported() {
    let definitions = vec![fixture("recursive", 1)];
    let mentions =
        std::collections::BTreeMap::from([("recursive".to_string(), vec![Site::Inside(0)])]);
    assert_eq!(unreferenced(&definitions, &mentions), [0]);
}

/// The modifiers are read with `async` and `extern`. The third one, which marks
/// a block the compiler cannot check, is never spelled in code outside
/// `nkb-sys`: `unsafe_lives_here_only.rs` reads every test file too, and does
/// not skip string literals. The first version of this fixture spelled it and
/// failed that test on all three systems.
#[test]
fn definitions_and_imports_are_read_the_way_the_scan_needs() {
    let source = "pub fn a() {}\npub const fn b() {}\npub const C: u8 = 1;\npub(crate) struct D;\n\
                  pub use e::F;\npub mod g;\nfn h() {}\nimpl I {\n    pub fn method() {}\n}\n\
                  pub async fn j() {}\npub extern \"system\" fn k() {}\n";
    let names: Vec<String> = pub_definitions("x.rs", source)
        .into_iter()
        .map(|d| d.name)
        .collect();
    assert_eq!(names, ["a", "b", "C", "D", "j", "k"]);
    let imports =
        import_lines("use a::b;\npub use c::{\n    D,\n    E,\n};\nfn f() {\n    use g::H;\n}\n");
    assert_eq!(imports.into_iter().collect::<Vec<_>>(), [1, 2, 3, 4, 5, 7]);
}
