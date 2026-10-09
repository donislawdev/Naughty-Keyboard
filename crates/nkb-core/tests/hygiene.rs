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
use repo_text::{Syntax, blocks, code, lex, product_tree, prose, relative, workspace_root};
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

    // Literals, not counts derived from what they check: 220 files and nine
    // shipped packs when this was written. Fewer means the walk lost a tree.
    assert!(files.len() >= 200, "read only {} files", files.len());
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

#[test]
fn every_comment_in_the_repository_is_english() {
    let root = workspace_root();
    let mut offenders = Vec::new();
    let mut files = 0;
    for (path, syntax) in every_text_file(&root) {
        let Some(syntax) = syntax else { continue };
        files += 1;
        for (line, why) in polish_in_comments(syntax, &read(&path)) {
            offenders.push(format!("{}:{line}: {why}", relative(&root, &path)));
        }
    }
    assert!(files >= 200, "the language scan read only {files} files");
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
        Syntax::Rust | Syntax::Slint | Syntax::Resource | Syntax::Toml | Syntax::Python
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
    // 119 000 when this was written.
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
