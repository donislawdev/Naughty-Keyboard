//! The settings file on disk: where it lives, how it is read, and how one key
//! is changed without touching the rest.
//!
//! The contract - location, keys, what happens to a file this version cannot
//! read - is `docs/settings-format.md`. This module is the one implementation
//! of it, and the notes below are about how, not what.
//!
//! # Where
//!
//! From the environment, never from a system call: `%APPDATA%` on Windows,
//! `$HOME/Library/Application Support` on macOS, `$XDG_CONFIG_HOME` or
//! `$HOME/.config` elsewhere. A variable that is missing, empty or relative
//! names no place, and the answer is `Nowhere` with the variable's name - so
//! the tester learns what to set rather than that "something" failed.
//!
//! ⚠️ The tools that start the palette under test rely on this: they point
//! `APPDATA` at a folder of their own, so a measurement never reads the
//! owner's settings and never writes into them. A change to how the place is
//! found is a change to those tools too.
//!
//! # One key, and every other byte left alone
//!
//! A save reads the file AGAIN, changes one key with `toml_edit` and writes the
//! document back. Reading again rather than writing from memory is what keeps
//! an edit the tester made while the palette ran. `toml_edit` keeps comments,
//! blank lines, order and keys this version does not know. Three things it
//! does not keep were measured with a probe on 0.25.13 and are put back here:
//!
//! - a comment after a replaced value goes with the value, so the value's
//!   decoration is carried over.
//! - line endings come out as LF, so a file written with CRLF throughout gets
//!   CRLF back.
//! - an index into a missing table makes an inline table, so a missing
//!   `[palette]` is inserted as a table of its own.
//!
//! # Atomic, and in the right place
//!
//! The text goes under a neighbouring temporary name, is flushed, and takes
//! the real name by a rename - the pattern `PackSink::replace` uses, for the
//! same reason: an interrupted plain write leaves half a file that still looks
//! like one. The temporary name carries the process number, so two palettes
//! saving at once never write into the same temporary file (`W4`).
//!
//! A settings file that is a symbolic link is written THROUGH the link: a
//! rename over the link would replace it with a plain file and quietly cut a
//! tester's dotfiles folder out of the loop.

use std::ffi::OsString;
use std::io::Read;
use std::path::{Path, PathBuf};

use nkb_app::{
    Clearing, SaveError, SettingChange, Settings, SettingsLoad, SettingsNote, SettingsStore,
    SettingsUnusable, ShortcutUnreadable,
};
use nkb_core::hotkeys::{HotkeyAction, HotkeyChord};
use nkb_core::identity::is_pack_id;
use toml_edit::{DocumentMut, Item, Table, TableLike, Value};

/// The one schema this version reads and writes.
pub const SCHEMA: i64 = 1;

/// The largest settings file this version reads, in bytes.
///
/// The files this tool writes are well under one kilobyte. A limit sixty-four
/// times that leaves room for years of new keys, and still refuses a path
/// that points at the wrong file before reading all of it into memory.
pub const LIMIT_BYTES: u64 = 64 * 1024;

const FILE_NAME: &str = "settings.toml";
const FOLDER: &str = "Naughty Keyboard";
/// The folder under `$XDG_CONFIG_HOME` - lower case and without a space, the
/// convention of that folder.
const FOLDER_XDG: &str = "naughty-keyboard";

/// The three conventions for where a user's settings live.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum System {
    Windows,
    MacOs,
    OtherUnix,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Place {
    File(PathBuf),
    Nowhere { missing: &'static str },
}

/// The settings file of one user, or the absence of a place for it.
#[derive(Debug, Clone)]
pub struct SettingsFile {
    place: Place,
}

impl SettingsFile {
    /// The settings file of the user running this process.
    #[must_use]
    pub fn for_this_user() -> Self {
        let system = if cfg!(windows) {
            System::Windows
        } else if cfg!(target_os = "macos") {
            System::MacOs
        } else {
            System::OtherUnix
        };
        Self {
            place: locate(system, &|name| std::env::var_os(name)),
        }
    }

    /// A settings file at exactly this path.
    #[must_use]
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self {
            place: Place::File(path.into()),
        }
    }

    /// Where the file is, for a sentence that tells the tester where to look.
    /// `None` when there is no place.
    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        match &self.place {
            Place::File(path) => Some(path),
            Place::Nowhere { .. } => None,
        }
    }
}

/// Where the settings live on `system`, read from the environment by `var`.
///
/// Only an absolute path counts: XDG says a relative `XDG_CONFIG_HOME` is to be
/// ignored, and a relative `APPDATA` or `HOME` would put the file wherever the
/// palette happened to be started from.
fn locate(system: System, var: &dyn Fn(&str) -> Option<OsString>) -> Place {
    let absolute = |name: &str| {
        var(name)
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
    };
    let (folder, missing) = match system {
        System::Windows => (absolute("APPDATA").map(|base| base.join(FOLDER)), "APPDATA"),
        System::MacOs => (
            absolute("HOME").map(|home| {
                home.join("Library")
                    .join("Application Support")
                    .join(FOLDER)
            }),
            "HOME",
        ),
        System::OtherUnix => (
            absolute("XDG_CONFIG_HOME")
                .or_else(|| absolute("HOME").map(|home| home.join(".config")))
                .map(|base| base.join(FOLDER_XDG)),
            "HOME",
        ),
    };
    folder.map_or(Place::Nowhere { missing }, |folder| {
        Place::File(folder.join(FILE_NAME))
    })
}

/// Why reading the file did not produce text.
enum ReadFailure {
    /// Nothing there.
    Missing,
    /// Something there that cannot be used.
    Unusable(SettingsUnusable),
}

/// The file's text, read with the size limit.
fn read_text(path: &Path) -> Result<String, ReadFailure> {
    let file = std::fs::File::open(path).map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => ReadFailure::Missing,
        _ => ReadFailure::Unusable(SettingsUnusable::Unreadable),
    })?;
    // One byte past the limit, so "exactly at the limit" and "over it" are told
    // apart without trusting a length the file system reports - a special file
    // reports none.
    let mut bytes = Vec::new();
    file.take(LIMIT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| ReadFailure::Unusable(SettingsUnusable::Unreadable))?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > LIMIT_BYTES {
        return Err(ReadFailure::Unusable(SettingsUnusable::TooLarge {
            limit_bytes: LIMIT_BYTES,
        }));
    }
    String::from_utf8(bytes).map_err(|_| ReadFailure::Unusable(SettingsUnusable::NotUtf8))
}

/// The document, when this version can use it at all.
fn understand(text: &str) -> Result<DocumentMut, SettingsUnusable> {
    let document: DocumentMut = text.parse().map_err(|error: toml_edit::TomlError| {
        // Counted over bytes rather than by slicing the text, so an offset
        // that does not fall on a character boundary cannot panic.
        let line = error.span().map_or(1, |span| {
            let end = span.start.min(text.len());
            text.as_bytes()[..end]
                .iter()
                .filter(|byte| **byte == b'\n')
                .count()
                + 1
        });
        SettingsUnusable::NotToml {
            line,
            detail: error.message().to_owned(),
        }
    })?;
    match document.get("schema").and_then(Item::as_integer) {
        Some(SCHEMA) => Ok(document),
        Some(found) if found > SCHEMA => Err(SettingsUnusable::SchemaTooNew { found }),
        _ => Err(SettingsUnusable::SchemaNotDeclared),
    }
}

/// The settings in a document this version can use, and what was not used.
fn settings_of(document: &DocumentMut) -> (Settings, Vec<SettingsNote>) {
    let mut settings = Settings::default();
    let mut notes = Vec::new();
    let mut unknown = Vec::new();

    for (key, _) in document.iter() {
        if key != "schema" && key != "palette" && key != "shortcuts" {
            unknown.push(key.to_owned());
        }
    }
    match document.get("palette").map(Item::as_table_like) {
        None => {}
        Some(None) => notes.push(SettingsNote::NotATable {
            key: String::from("palette"),
        }),
        Some(Some(palette)) => {
            for (key, item) in palette.iter() {
                match key {
                    "pack" => match item.as_str() {
                        Some(pack) if is_pack_id(pack) => settings.pack = Some(pack.to_owned()),
                        _ => notes.push(SettingsNote::NotAPackName {
                            key: String::from("palette.pack"),
                        }),
                    },
                    "compact" => match item.as_bool() {
                        Some(compact) => settings.compact = Some(compact),
                        None => notes.push(SettingsNote::NotTrueOrFalse {
                            key: String::from("palette.compact"),
                        }),
                    },
                    "clearing" => match item.as_str().and_then(clearing_named) {
                        Some(clearing) => settings.clearing = Some(clearing),
                        None => notes.push(SettingsNote::NotAClearing {
                            key: String::from("palette.clearing"),
                        }),
                    },
                    other => unknown.push(format!("palette.{other}")),
                }
            }
        }
    }
    match document.get("shortcuts").map(Item::as_table_like) {
        None => {}
        Some(None) => notes.push(SettingsNote::NotATable {
            key: String::from("shortcuts"),
        }),
        Some(Some(shortcuts)) => {
            for (key, item) in shortcuts.iter() {
                let Some(action) = HotkeyAction::from_id(key) else {
                    unknown.push(format!("shortcuts.{key}"));
                    continue;
                };
                let read = item
                    .as_str()
                    .ok_or(ShortcutUnreadable::NotText)
                    .and_then(|text| HotkeyChord::parse(text).map_err(ShortcutUnreadable::Grammar));
                match read {
                    Ok(chord) => settings.shortcuts.push((action, chord)),
                    Err(why) => notes.push(SettingsNote::NotAShortcut {
                        key: format!("shortcuts.{key}"),
                        why,
                    }),
                }
            }
        }
    }
    if !unknown.is_empty() {
        notes.push(SettingsNote::UnknownKeys { keys: unknown });
    }
    (settings, notes)
}

/// Puts one change into the document. `false` when it was already there, so
/// the file does not need writing at all.
fn set(document: &mut DocumentMut, change: &SettingChange) -> bool {
    let (table_key, key) = place_of(change);
    let current = document
        .get(table_key)
        .and_then(Item::as_table_like)
        .and_then(|table| table.get(key));
    let (same, new): (bool, Option<Value>) = match change {
        SettingChange::Pack(pack) => (
            current.and_then(Item::as_str) == Some(pack.as_str()),
            Some(Value::from(pack.as_str())),
        ),
        SettingChange::Compact(compact) => (
            current.and_then(Item::as_bool) == Some(*compact),
            Some(Value::from(*compact)),
        ),
        SettingChange::Clearing(clearing) => (
            current.and_then(Item::as_str).and_then(clearing_named) == Some(*clearing),
            Some(Value::from(clearing_word(*clearing))),
        ),
        // Read back through the grammar, so `alt + shift + n` the tester
        // wrote is the same shortcut as `Alt+Shift+N` and stays as written.
        SettingChange::Shortcut {
            chord: Some(chord), ..
        } => (
            current
                .and_then(Item::as_str)
                .is_some_and(|text| HotkeyChord::parse(text) == Ok(*chord)),
            Some(Value::from(chord.text())),
        ),
        // A restore removes the key, so it is already so when there is no key
        // - including when the table itself is a single value (named when the
        // file was read) and so holds no key at all.
        SettingChange::Shortcut { chord: None, .. } => (current.is_none(), None),
    };
    if same {
        return false;
    }
    let Some(new) = new else {
        let inline = document
            .get(table_key)
            .and_then(Item::as_value)
            .is_some_and(Value::is_inline_table);
        if let Some(table) = document
            .get_mut(table_key)
            .and_then(Item::as_table_like_mut)
        {
            remove_keeping_comments(table, key, inline);
        }
        return true;
    };

    let is_a_table = document
        .get(table_key)
        .is_some_and(|item| item.as_table_like().is_some());
    if !is_a_table {
        // Missing, or a single value where a table belongs. The second was
        // named when the file was read (`SettingsNote::NotATable`), and the
        // tester has now changed a setting inside it.
        let mut table = Table::new();
        table.insert(key, Item::Value(new));
        document.insert(table_key, Item::Table(table));
        return true;
    }
    if let Some(table) = document
        .get_mut(table_key)
        .and_then(Item::as_table_like_mut)
    {
        replace_in(table, key, new);
    }
    true
}

/// The table and the key a change is written to.
fn place_of(change: &SettingChange) -> (&'static str, &'static str) {
    match change {
        SettingChange::Pack(_) => ("palette", "pack"),
        SettingChange::Compact(_) => ("palette", "compact"),
        SettingChange::Clearing(_) => ("palette", "clearing"),
        SettingChange::Shortcut { action, .. } => ("shortcuts", action.id()),
    }
}

/// The words `palette.clearing` takes, each beside the way it names - one
/// table for reading and for writing, so the two cannot drift apart.
///
/// Public names (untouchable rule 3, `settings-format.md` 2): a word here is
/// read by every later version, so one is never renamed, only added. Text
/// rather than `true`/`false`, so that a third way - a multi-line field
/// cleared after the tester agreed (`ux-spec.md` 4) - is a new word, not a new
/// schema (`D101`).
const CLEARING_WORDS: [(Clearing, &str); 2] = [(Clearing::Line, "line"), (Clearing::Keep, "none")];

/// The way of clearing a word names, or `None` for a word that names none.
/// Exact: `Line` is not `line`, and the note says which words there are.
fn clearing_named(word: &str) -> Option<Clearing> {
    CLEARING_WORDS
        .iter()
        .find(|(_, named)| *named == word)
        .map(|(clearing, _)| *clearing)
}

/// The word for a way of clearing.
fn clearing_word(clearing: Clearing) -> &'static str {
    CLEARING_WORDS
        .iter()
        .find(|(way, _)| *way == clearing)
        .map_or("line", |(_, word)| word)
}

/// Removes one key, and hands the lines written above it - comments, blank
/// lines - to the key that follows, so they stay where the tester put them.
///
/// `toml_edit` keeps a comment above a key in that KEY's decoration, so a
/// plain removal would take `# marking results` away with the first of the
/// three keys it heads. Only the whole lines move: the indentation of the
/// removed key's own line is its own and would double the next one's.
///
/// ⚠️ With no key after it in the table, the lines go with the key: they
/// stood directly above the one setting the tester asked to remove, and there
/// is no other key they could belong to (`settings-format.md` 4).
///
/// In a table written in one line the blank before `}` belongs to the LAST
/// value, so removing the last entry would leave `"Alt+Shift+K"}` - valid,
/// and not what the tester wrote. The entry before it takes that blank over.
fn remove_keeping_comments(table: &mut dyn TableLike, key: &str, inline: bool) {
    let lines_above = table
        .key(key)
        .and_then(|found| found.leaf_decor().prefix())
        .and_then(|prefix| prefix.as_str())
        .map(|prefix| {
            prefix
                .rfind('\n')
                .map_or("", |end| &prefix[..=end])
                .to_owned()
        })
        .unwrap_or_default();
    let blank_before_brace = table
        .get(key)
        .and_then(Item::as_value)
        .and_then(|value| value.decor().suffix())
        .and_then(|suffix| suffix.as_str())
        .map(str::to_owned);
    let (previous, next) = {
        let keys: Vec<&str> = table.iter().map(|(name, _)| name).collect();
        let at = keys.iter().position(|name| *name == key);
        (
            at.and_then(|at| at.checked_sub(1))
                .and_then(|before| keys.get(before))
                .map(|name| (*name).to_owned()),
            at.and_then(|at| keys.get(at + 1))
                .map(|name| (*name).to_owned()),
        )
    };
    table.remove(key);
    if inline
        && next.is_none()
        && let Some(blank) = blank_before_brace
        && let Some(value) = previous
            .as_deref()
            .and_then(|name| table.get_mut(name))
            .and_then(Item::as_value_mut)
    {
        value.decor_mut().set_suffix(blank);
    }
    if lines_above.is_empty() {
        return;
    }
    if let Some(mut following) = next.as_deref().and_then(|name| table.key_mut(name)) {
        let decor = following.leaf_decor_mut();
        let theirs = decor
            .prefix()
            .and_then(|prefix| prefix.as_str())
            .unwrap_or_default()
            .to_owned();
        decor.set_prefix(format!("{lines_above}{theirs}"));
    }
}

/// Replaces one key's value, keeping what surrounds it - the spaces and the
/// comment after it are the tester's.
fn replace_in(table: &mut dyn TableLike, key: &str, mut new: Value) {
    match table.get_mut(key) {
        Some(item) => match item.as_value_mut() {
            Some(old) => {
                *new.decor_mut() = old.decor().clone();
                *old = new;
            }
            None => *item = Item::Value(new),
        },
        None => {
            table.insert(key, Item::Value(new));
        }
    }
}

/// Whether every line of `text` ends with CRLF.
fn uses_crlf(text: &str) -> bool {
    text.contains("\r\n") && !text.replace("\r\n", "").contains('\n')
}

/// The file a save has to write: the path itself, or what a link at the path
/// points to.
fn write_target(path: &Path) -> Result<PathBuf, SaveError> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => {
            std::fs::canonicalize(path).map_err(|_| SaveError::Unwritable)
        }
        _ => Ok(path.to_path_buf()),
    }
}

/// Writes `text` to `target` so that either the old content or the new one is
/// there afterwards, and nothing in between.
fn write_atomically(target: &Path, text: &str) -> std::io::Result<()> {
    let folder = target.parent().ok_or(std::io::ErrorKind::InvalidInput)?;
    let name = target.file_name().ok_or(std::io::ErrorKind::InvalidInput)?;
    std::fs::create_dir_all(folder)?;
    let mut temporary_name = name.to_os_string();
    temporary_name.push(format!(".nkb-new-{}", std::process::id()));
    let temporary = folder.join(temporary_name);

    let written = (|| -> std::io::Result<()> {
        let mut file = std::fs::File::create(&temporary)?;
        std::io::Write::write_all(&mut file, text.as_bytes())?;
        // Before the rename: a rename that publishes content the system has
        // not committed is a rename that publishes an empty file on the next
        // power cut.
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temporary, target)
    })();
    if written.is_err() {
        // Nothing half written is left under a name a person might open.
        let _ = std::fs::remove_file(&temporary);
    }
    written
}

impl SettingsStore for SettingsFile {
    fn load(&self) -> SettingsLoad {
        let path = match &self.place {
            Place::File(path) => path,
            Place::Nowhere { missing } => {
                return SettingsLoad::Nowhere {
                    missing: (*missing).to_owned(),
                };
            }
        };
        let text = match read_text(path) {
            Ok(text) => text,
            Err(ReadFailure::Missing) => return SettingsLoad::Absent,
            Err(ReadFailure::Unusable(why)) => return SettingsLoad::Unusable(why),
        };
        match understand(&text) {
            Ok(document) => {
                let (settings, notes) = settings_of(&document);
                SettingsLoad::Read { settings, notes }
            }
            Err(why) => SettingsLoad::Unusable(why),
        }
    }

    fn save(&self, change: &SettingChange) -> Result<(), SaveError> {
        let path = match &self.place {
            Place::File(path) => path,
            Place::Nowhere { .. } => return Err(SaveError::Nowhere),
        };
        let target = write_target(path)?;
        let (mut document, crlf) = match read_text(&target) {
            Ok(text) => (
                understand(&text).map_err(SaveError::Unusable)?,
                uses_crlf(&text),
            ),
            Err(ReadFailure::Missing) => {
                let mut document = DocumentMut::new();
                document.insert("schema", Item::Value(Value::from(SCHEMA)));
                (document, false)
            }
            Err(ReadFailure::Unusable(why)) => return Err(SaveError::Unusable(why)),
        };
        if !set(&mut document, change) {
            return Ok(());
        }
        // 🔴 Asked here rather than left to the rename, and measured on three
        // systems: on Windows a rename over a read-only file fails, on Linux and
        // macOS it SUCCEEDS - the folder's permission decides there, not the
        // file's - so the tester's lock held on one system of three. Asked after
        // `set`, so a change already in the file still answers without a write.
        if std::fs::metadata(&target).is_ok_and(|meta| meta.permissions().readonly()) {
            return Err(SaveError::Unwritable);
        }
        let mut text = document.to_string();
        if crlf {
            text = text.replace("\r\n", "\n").replace('\n', "\r\n");
        }
        write_atomically(&target, &text).map_err(|_| SaveError::Unwritable)
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;

    /// A folder of its own for one test, removed when the test ends.
    ///
    /// Real files rather than a double, because what is being trusted is the
    /// operating system: rename, read-only files, a folder that is not there.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("nkb-settings-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            Self(path)
        }
        fn file(&self) -> PathBuf {
            self.0.join("nested").join(FILE_NAME)
        }
        fn store(&self) -> SettingsFile {
            SettingsFile::at(self.file())
        }
        fn write(&self, text: &str) {
            self.write_bytes(text.as_bytes());
        }
        fn write_bytes(&self, bytes: &[u8]) {
            std::fs::create_dir_all(self.0.join("nested")).expect("a scratch folder");
            std::fs::write(self.file(), bytes).expect("a scratch file");
        }
        fn text(&self) -> String {
            std::fs::read_to_string(self.file()).expect("the file is there")
        }
        fn bytes(&self) -> Vec<u8> {
            std::fs::read(self.file()).expect("the file is there")
        }
        fn leftovers(&self) -> Vec<String> {
            std::fs::read_dir(self.0.join("nested"))
                .map(|entries| {
                    entries
                        .filter_map(Result::ok)
                        .map(|entry| entry.file_name().to_string_lossy().into_owned())
                        .filter(|name| name != FILE_NAME)
                        .collect()
                })
                .unwrap_or_default()
        }
    }

    impl Drop for Scratch {
        #[allow(
            clippy::permissions_set_readonly_false,
            reason = "a scratch file made read-only by a test has to go"
        )]
        fn drop(&mut self) {
            if let Ok(meta) = std::fs::metadata(self.file()) {
                let mut permissions = meta.permissions();
                permissions.set_readonly(false);
                let _ = std::fs::set_permissions(self.file(), permissions);
            }
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn read(store: &SettingsFile) -> (Settings, Vec<SettingsNote>) {
        match store.load() {
            SettingsLoad::Read { settings, notes } => (settings, notes),
            other => panic!("expected a readable file, got {other:?}"),
        }
    }

    fn unusable(store: &SettingsFile) -> SettingsUnusable {
        match store.load() {
            SettingsLoad::Unusable(why) => why,
            other => panic!("expected an unusable file, got {other:?}"),
        }
    }

    // ---- where -----------------------------------------------------------

    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<OsString> + 'a {
        move |name| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| OsString::from(value))
        }
    }

    #[test]
    fn each_system_puts_the_file_where_its_own_convention_says() {
        // An absolute path on whatever machine runs the test, because what
        // counts as absolute is the host's question.
        let base = std::env::temp_dir();
        let base_text = base.to_string_lossy().into_owned();

        assert_eq!(
            locate(System::Windows, &env(&[("APPDATA", &base_text)])),
            Place::File(base.join("Naughty Keyboard").join("settings.toml"))
        );
        assert_eq!(
            locate(System::MacOs, &env(&[("HOME", &base_text)])),
            Place::File(
                base.join("Library")
                    .join("Application Support")
                    .join("Naughty Keyboard")
                    .join("settings.toml")
            )
        );
        assert_eq!(
            locate(System::OtherUnix, &env(&[("HOME", &base_text)])),
            Place::File(
                base.join(".config")
                    .join("naughty-keyboard")
                    .join("settings.toml")
            )
        );
        let xdg = base.join("xdg");
        let xdg_text = xdg.to_string_lossy().into_owned();
        assert_eq!(
            locate(
                System::OtherUnix,
                &env(&[("HOME", &base_text), ("XDG_CONFIG_HOME", &xdg_text)])
            ),
            Place::File(xdg.join("naughty-keyboard").join("settings.toml")),
            "XDG_CONFIG_HOME wins over HOME"
        );
    }

    #[test]
    fn a_missing_empty_or_relative_variable_names_no_place_and_says_which() {
        assert_eq!(
            locate(System::Windows, &env(&[])),
            Place::Nowhere { missing: "APPDATA" }
        );
        assert_eq!(
            locate(System::Windows, &env(&[("APPDATA", "")])),
            Place::Nowhere { missing: "APPDATA" }
        );
        assert_eq!(
            locate(System::Windows, &env(&[("APPDATA", "relative")])),
            Place::Nowhere { missing: "APPDATA" }
        );
        assert_eq!(
            locate(System::MacOs, &env(&[])),
            Place::Nowhere { missing: "HOME" }
        );
        // XDG says a relative XDG_CONFIG_HOME is to be ignored - not obeyed,
        // and not a reason to give up while HOME is there.
        let base = std::env::temp_dir();
        let base_text = base.to_string_lossy().into_owned();
        assert_eq!(
            locate(
                System::OtherUnix,
                &env(&[("XDG_CONFIG_HOME", "relative"), ("HOME", &base_text)])
            ),
            Place::File(
                base.join(".config")
                    .join("naughty-keyboard")
                    .join("settings.toml")
            )
        );
        assert_eq!(
            locate(System::OtherUnix, &env(&[("XDG_CONFIG_HOME", "relative")])),
            Place::Nowhere { missing: "HOME" }
        );
    }

    #[test]
    fn no_place_answers_both_questions_without_touching_anything() {
        let store = SettingsFile {
            place: Place::Nowhere { missing: "APPDATA" },
        };
        assert_eq!(
            store.load(),
            SettingsLoad::Nowhere {
                missing: String::from("APPDATA")
            }
        );
        assert_eq!(
            store.save(&SettingChange::Compact(true)),
            Err(SaveError::Nowhere)
        );
        assert_eq!(store.path(), None);
    }

    // ---- reading ---------------------------------------------------------

    #[test]
    fn a_first_run_finds_nothing_and_creates_nothing() {
        let scratch = Scratch::new("first-run");
        assert_eq!(scratch.store().load(), SettingsLoad::Absent);
        assert!(
            !scratch.0.exists(),
            "reading created the folder - a first run must leave nothing behind"
        );
    }

    #[test]
    fn a_first_save_writes_the_schema_and_a_table_of_its_own() {
        let scratch = Scratch::new("first-save");
        let store = scratch.store();
        assert_eq!(store.save(&SettingChange::Compact(true)), Ok(()));
        assert_eq!(scratch.text(), "schema = 1\n\n[palette]\ncompact = true\n");
        assert_eq!(
            store.save(&SettingChange::Pack(String::from("unicode-text"))),
            Ok(())
        );
        assert_eq!(
            read(&store),
            (
                Settings {
                    pack: Some(String::from("unicode-text")),
                    compact: Some(true),
                    clearing: None,
                    shortcuts: Vec::new(),
                },
                Vec::new()
            )
        );
        assert!(scratch.leftovers().is_empty(), "{:?}", scratch.leftovers());
    }

    #[test]
    fn a_save_changes_one_key_and_keeps_every_other_byte_of_a_hand_written_file() {
        // 🔴 Comments, a blank line, a key from a newer version, a trailing
        // comment on the very value being replaced, and CRLF throughout - all
        // of it the tester's, all of it back after the save.
        let scratch = Scratch::new("hand-written");
        scratch.write(concat!(
            "# my settings\r\n",
            "schema = 1\r\n",
            "\r\n",
            "[palette]\r\n",
            "pack = \"whitespace\"   # the one I use\r\n",
            "later = 7\r\n",
            "\r\n",
            "[future]\r\n",
            "x = 1\r\n",
        ));
        assert_eq!(
            scratch
                .store()
                .save(&SettingChange::Pack(String::from("unicode-text"))),
            Ok(())
        );
        assert_eq!(
            scratch.text(),
            concat!(
                "# my settings\r\n",
                "schema = 1\r\n",
                "\r\n",
                "[palette]\r\n",
                "pack = \"unicode-text\"   # the one I use\r\n",
                "later = 7\r\n",
                "\r\n",
                "[future]\r\n",
                "x = 1\r\n",
            )
        );
    }

    #[test]
    fn a_setting_in_an_inline_table_stays_in_it() {
        let scratch = Scratch::new("inline");
        scratch.write("schema = 1\npalette = { pack = \"whitespace\" }\n");
        assert_eq!(scratch.store().save(&SettingChange::Compact(true)), Ok(()));
        assert_eq!(
            read(&scratch.store()).0,
            Settings {
                pack: Some(String::from("whitespace")),
                compact: Some(true),
                clearing: None,
                shortcuts: Vec::new(),
            }
        );
        assert!(
            scratch.text().contains("palette = {"),
            "the tester's inline table became something else: {}",
            scratch.text()
        );
    }

    /// `D101`: the way of clearing is a word, read and written through one
    /// table - `line` and `none` both ways, a word in another case or another
    /// type named and replaced by the default, and the word already there not
    /// written again.
    #[test]
    fn the_way_of_clearing_is_a_word_both_ways_and_a_wrong_one_is_named() {
        let scratch = Scratch::new("clearing");
        let store = scratch.store();
        assert_eq!(store.save(&SettingChange::Clearing(Clearing::Keep)), Ok(()));
        assert_eq!(
            scratch.text(),
            "schema = 1\n\n[palette]\nclearing = \"none\"\n"
        );
        assert_eq!(read(&store).0.clearing, Some(Clearing::Keep));
        assert_eq!(store.save(&SettingChange::Clearing(Clearing::Line)), Ok(()));
        assert_eq!(
            scratch.text(),
            "schema = 1\n\n[palette]\nclearing = \"line\"\n"
        );
        assert_eq!(read(&store).0.clearing, Some(Clearing::Line));

        for wrong in ["\"Line\"", "\"cursor\"", "true", "1"] {
            scratch.write(&format!("schema = 1\n[palette]\nclearing = {wrong}\n"));
            let (settings, notes) = read(&store);
            assert_eq!(settings.clearing, None, "{wrong}");
            assert_eq!(
                notes,
                vec![SettingsNote::NotAClearing {
                    key: String::from("palette.clearing")
                }],
                "{wrong}"
            );
        }

        // Already so: a read-only file takes the "save" of the word it holds.
        scratch.write("schema = 1\n[palette]\nclearing = \"none\"\n");
        let mut permissions = std::fs::metadata(scratch.file())
            .expect("the file is there")
            .permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(scratch.file(), permissions).expect("read-only");
        assert_eq!(store.save(&SettingChange::Clearing(Clearing::Keep)), Ok(()));
        assert_eq!(
            store.save(&SettingChange::Clearing(Clearing::Line)),
            Err(SaveError::Unwritable)
        );
    }

    #[test]
    fn a_change_that_is_already_in_the_file_does_not_write() {
        // Proven with a read-only file: a write would fail, so success means
        // nothing was attempted.
        let scratch = Scratch::new("unchanged");
        scratch.write("schema = 1\n[palette]\ncompact = true\n");
        let mut permissions = std::fs::metadata(scratch.file())
            .expect("the file is there")
            .permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(scratch.file(), permissions).expect("read-only");

        assert_eq!(
            scratch.store().save(&SettingChange::Compact(true)),
            Ok(()),
            "a change already in the file was written anyway"
        );
        assert_eq!(
            scratch.store().save(&SettingChange::Compact(false)),
            Err(SaveError::Unwritable),
            "a read-only file is a place that cannot be written - the tester's lock holds"
        );
        assert_eq!(scratch.text(), "schema = 1\n[palette]\ncompact = true\n");
        assert!(scratch.leftovers().is_empty(), "{:?}", scratch.leftovers());
    }

    #[test]
    fn what_this_version_does_not_use_is_named_and_left_in_place() {
        let scratch = Scratch::new("notes");
        let written = concat!(
            "schema = 1\n",
            "future = true\n",
            "[palette]\n",
            "pack = \"../outside\"\n",
            "compact = \"yes\"\n",
            "compct = true\n",
        );
        scratch.write(written);
        let (settings, notes) = read(&scratch.store());
        assert_eq!(settings, Settings::default());
        assert_eq!(
            notes,
            vec![
                SettingsNote::NotAPackName {
                    key: String::from("palette.pack")
                },
                SettingsNote::NotTrueOrFalse {
                    key: String::from("palette.compact")
                },
                SettingsNote::UnknownKeys {
                    keys: vec![String::from("future"), String::from("palette.compct")]
                },
            ]
        );
        assert_eq!(scratch.text(), written, "reading changed the file");
    }

    #[test]
    fn the_shortcuts_table_gives_what_reads_and_names_what_does_not() {
        use nkb_app::ShortcutUnreadable;
        use nkb_core::hotkeys::ChordError;
        let scratch = Scratch::new("shortcuts");
        let written = concat!(
            "schema = 1\n",
            "[shortcuts]\n",
            "next-value = \"alt+shift+m\"\n",
            "previous-value = \"Alt+Shfit+P\"\n",
            "repeat-last = 5\n",
            "nxt-value = \"Alt+Shift+Q\"\n",
            "open-packs = \"Shift+Space\"\n",
        );
        scratch.write(written);
        let (settings, notes) = read(&scratch.store());
        let chord = |text| HotkeyChord::parse(text).expect("reads");
        // A chord that reads is given as it is, even one with a problem - that
        // question needs the other nine and belongs to `Bindings::with`.
        assert_eq!(
            settings.shortcuts,
            vec![
                (HotkeyAction::NextValue, chord("Alt+Shift+M")),
                (HotkeyAction::OpenPacks, chord("Shift+Space")),
            ]
        );
        assert_eq!(
            notes,
            vec![
                SettingsNote::NotAShortcut {
                    key: String::from("shortcuts.previous-value"),
                    why: ShortcutUnreadable::Grammar(ChordError::Unknown(String::from("Shfit"))),
                },
                SettingsNote::NotAShortcut {
                    key: String::from("shortcuts.repeat-last"),
                    why: ShortcutUnreadable::NotText,
                },
                SettingsNote::UnknownKeys {
                    keys: vec![String::from("shortcuts.nxt-value")]
                },
            ]
        );
        assert_eq!(scratch.text(), written, "reading changed the file");

        scratch.write("schema = 1\nshortcuts = \"Alt+Shift+N\"\n");
        let (settings, notes) = read(&scratch.store());
        assert!(settings.shortcuts.is_empty());
        assert_eq!(
            notes,
            vec![SettingsNote::NotATable {
                key: String::from("shortcuts")
            }]
        );
    }

    #[test]
    fn a_saved_change_leaves_the_shortcuts_the_tester_wrote() {
        let scratch = Scratch::new("shortcuts-kept");
        let written = "schema = 1\n\n[shortcuts]\n# mine\nnext-value = \"Ctrl+Alt+Win+N\"\n";
        scratch.write(written);
        assert_eq!(scratch.store().save(&SettingChange::Compact(true)), Ok(()));
        let text = scratch.text();
        assert!(text.starts_with(written), "{text}");
        assert!(text.contains("compact = true"), "{text}");
    }

    fn recorded(action: HotkeyAction, text: &str) -> SettingChange {
        SettingChange::Shortcut {
            action,
            chord: Some(HotkeyChord::parse(text).expect("the test's chord reads")),
        }
    }

    fn restored(action: HotkeyAction) -> SettingChange {
        SettingChange::Shortcut {
            action,
            chord: None,
        }
    }

    fn make_read_only(scratch: &Scratch) {
        let mut permissions = std::fs::metadata(scratch.file())
            .expect("the file is there")
            .permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(scratch.file(), permissions).expect("read-only");
    }

    #[test]
    fn a_recorded_shortcut_is_written_in_its_own_table_and_reads_back() {
        let scratch = Scratch::new("shortcut-first");
        assert_eq!(
            scratch
                .store()
                .save(&recorded(HotkeyAction::NextValue, "win+ctrl+alt+n")),
            Ok(())
        );
        // In the order the core writes a chord, whatever order it was given in.
        assert_eq!(
            scratch.text(),
            "schema = 1\n\n[shortcuts]\nnext-value = \"Ctrl+Alt+Win+N\"\n"
        );
        assert_eq!(
            read(&scratch.store()).0.shortcuts,
            vec![(
                HotkeyAction::NextValue,
                HotkeyChord::parse("Ctrl+Alt+Win+N").expect("reads")
            )]
        );
    }

    #[test]
    fn a_recorded_shortcut_replaces_one_value_and_keeps_every_other_byte() {
        let scratch = Scratch::new("shortcut-replace");
        scratch.write(concat!(
            "schema = 1\r\n",
            "\r\n",
            "[shortcuts]\r\n",
            "# moving through the pack\r\n",
            "next-value = \"Ctrl+Alt+Win+N\"   # left hand\r\n",
            "previous-value = \"Ctrl+Alt+Win+P\"\r\n",
        ));
        assert_eq!(
            scratch
                .store()
                .save(&recorded(HotkeyAction::NextValue, "Alt+Shift+M")),
            Ok(())
        );
        assert_eq!(
            scratch.text(),
            concat!(
                "schema = 1\r\n",
                "\r\n",
                "[shortcuts]\r\n",
                "# moving through the pack\r\n",
                "next-value = \"Alt+Shift+M\"   # left hand\r\n",
                "previous-value = \"Ctrl+Alt+Win+P\"\r\n",
            )
        );
    }

    #[test]
    fn a_shortcut_the_file_already_holds_in_any_spelling_is_not_written() {
        // Read-only, so success proves no write was attempted - and the
        // tester's own spelling stays.
        let scratch = Scratch::new("shortcut-same");
        let written = "schema = 1\n[shortcuts]\nnext-value = \"shift + ALT + m\"\n";
        scratch.write(written);
        make_read_only(&scratch);
        assert_eq!(
            scratch
                .store()
                .save(&recorded(HotkeyAction::NextValue, "Alt+Shift+M")),
            Ok(()),
            "the same shortcut in another spelling was written anyway"
        );
        assert_eq!(
            scratch.store().save(&restored(HotkeyAction::PreviousValue)),
            Ok(()),
            "restoring a shortcut the file does not name was written anyway"
        );
        assert_eq!(
            scratch
                .store()
                .save(&recorded(HotkeyAction::NextValue, "Alt+Shift+Q")),
            Err(SaveError::Unwritable),
            "a real change to a read-only file must be refused"
        );
        assert_eq!(scratch.text(), written);
    }

    #[test]
    fn a_restore_removes_the_key_and_the_lines_above_it_move_to_the_next() {
        // 🔴 The comment above the first key heads the whole group. Without the
        // move it would leave with the key it happens to stand above.
        let scratch = Scratch::new("shortcut-restore");
        scratch.write(concat!(
            "schema = 1\n",
            "\n",
            "[shortcuts]\n",
            "\n",
            "# marking results\n",
            "  mark-ok = \"Ctrl+Alt+Win+1\"   # works\n",
            "  mark-problem = \"Ctrl+Alt+Win+2\"\n",
            "\n",
            "[palette]\n",
            "compact = true\n",
        ));
        assert_eq!(
            scratch.store().save(&restored(HotkeyAction::MarkOk)),
            Ok(())
        );
        assert_eq!(
            scratch.text(),
            concat!(
                "schema = 1\n",
                "\n",
                "[shortcuts]\n",
                "\n",
                "# marking results\n",
                "  mark-problem = \"Ctrl+Alt+Win+2\"\n",
                "\n",
                "[palette]\n",
                "compact = true\n",
            )
        );
        // The last key takes its own lines with it, and nothing else moves.
        assert_eq!(
            scratch.store().save(&restored(HotkeyAction::MarkProblem)),
            Ok(())
        );
        assert_eq!(
            scratch.text(),
            "schema = 1\n\n[shortcuts]\n\n[palette]\ncompact = true\n"
        );
        assert!(read(&scratch.store()).0.shortcuts.is_empty());
        assert!(scratch.leftovers().is_empty(), "{:?}", scratch.leftovers());
    }

    #[test]
    fn a_restore_of_a_value_that_did_not_read_clears_it() {
        // The key a restore exists for: the palette named it at start and fell
        // back to the default, and the tester asks for that default.
        let scratch = Scratch::new("shortcut-garbage");
        scratch.write("schema = 1\n[shortcuts]\nrepeat-last = 5\nnext-value = \"Alt+Shift+M\"\n");
        assert_eq!(
            scratch.store().save(&restored(HotkeyAction::RepeatLast)),
            Ok(())
        );
        assert_eq!(
            scratch.text(),
            "schema = 1\n[shortcuts]\nnext-value = \"Alt+Shift+M\"\n"
        );
        assert!(read(&scratch.store()).1.is_empty());
    }

    #[test]
    fn shortcuts_in_an_inline_table_or_dotted_keys_keep_that_form() {
        let scratch = Scratch::new("shortcut-forms");
        scratch.write("schema = 1\nshortcuts = { next-value = \"Alt+Shift+M\", repeat-last = \"Alt+Shift+Q\" }\n");
        assert_eq!(
            scratch
                .store()
                .save(&recorded(HotkeyAction::NextValue, "Alt+Shift+K")),
            Ok(())
        );
        assert_eq!(
            scratch.store().save(&restored(HotkeyAction::RepeatLast)),
            Ok(())
        );
        assert_eq!(
            scratch.text(),
            "schema = 1\nshortcuts = { next-value = \"Alt+Shift+K\" }\n"
        );

        scratch.write("schema = 1\nshortcuts.next-value = \"Alt+Shift+M\"\nshortcuts.repeat-last = \"Alt+Shift+Q\"\n");
        assert_eq!(
            scratch.store().save(&restored(HotkeyAction::NextValue)),
            Ok(())
        );
        assert_eq!(
            scratch.text(),
            "schema = 1\nshortcuts.repeat-last = \"Alt+Shift+Q\"\n"
        );
    }

    #[test]
    fn a_single_value_where_the_shortcuts_belong_is_left_by_a_restore_and_replaced_by_a_record() {
        let scratch = Scratch::new("shortcut-not-a-table");
        scratch.write("schema = 1\nshortcuts = \"Alt+Shift+N\"\n");
        make_read_only(&scratch);
        assert_eq!(
            scratch.store().save(&restored(HotkeyAction::NextValue)),
            Ok(()),
            "a restore found a key inside a single value"
        );
        let mut permissions = std::fs::metadata(scratch.file())
            .expect("the file is there")
            .permissions();
        #[allow(
            clippy::permissions_set_readonly_false,
            reason = "the test undoes its own lock"
        )]
        permissions.set_readonly(false);
        std::fs::set_permissions(scratch.file(), permissions).expect("writable again");
        assert_eq!(
            scratch
                .store()
                .save(&recorded(HotkeyAction::NextValue, "Alt+Shift+M")),
            Ok(())
        );
        assert_eq!(
            read(&scratch.store()).0.shortcuts,
            vec![(
                HotkeyAction::NextValue,
                HotkeyChord::parse("Alt+Shift+M").expect("reads")
            )]
        );
    }

    #[test]
    fn a_single_value_where_the_table_belongs_is_named_and_replaced_only_by_a_change() {
        let scratch = Scratch::new("not-a-table");
        scratch.write("schema = 1\npalette = 5\n");
        assert_eq!(
            read(&scratch.store()).1,
            vec![SettingsNote::NotATable {
                key: String::from("palette")
            }]
        );
        assert_eq!(scratch.store().save(&SettingChange::Compact(true)), Ok(()));
        assert_eq!(read(&scratch.store()).0.compact, Some(true));
    }

    #[test]
    fn a_byte_order_mark_is_read_past() {
        let scratch = Scratch::new("bom");
        scratch.write("\u{FEFF}schema = 1\n[palette]\ncompact = true\n");
        assert_eq!(read(&scratch.store()).0.compact, Some(true));
    }

    // ---- files that cannot be used, and are never written over ----------

    /// Every way a file can be unusable, each checked for the answer AND for
    /// the file being byte for byte what it was after a save was tried.
    #[test]
    fn a_file_that_cannot_be_used_is_named_and_never_written_over() {
        let over_the_limit = {
            let mut text = String::from("schema = 1\n# ");
            text.push_str(&"x".repeat(usize::try_from(LIMIT_BYTES).unwrap_or(0)));
            text.into_bytes()
        };
        let cases: Vec<(&str, Vec<u8>, SettingsUnusable)> = vec![
            (
                "syntax",
                b"schema = 1\n[palette\ncompact = true\n".to_vec(),
                SettingsUnusable::NotToml {
                    line: 2,
                    detail: String::from("unclosed table, expected `]`"),
                },
            ),
            (
                "not-utf8",
                b"schema = 1\n# \xFF\n".to_vec(),
                SettingsUnusable::NotUtf8,
            ),
            (
                "no-schema",
                b"[palette]\ncompact = true\n".to_vec(),
                SettingsUnusable::SchemaNotDeclared,
            ),
            (
                "schema-text",
                b"schema = \"1\"\n".to_vec(),
                SettingsUnusable::SchemaNotDeclared,
            ),
            (
                "schema-zero",
                b"schema = 0\n".to_vec(),
                SettingsUnusable::SchemaNotDeclared,
            ),
            (
                "schema-newer",
                b"schema = 2\n[palette]\ncompact = true\n".to_vec(),
                SettingsUnusable::SchemaTooNew { found: 2 },
            ),
            (
                "too-large",
                over_the_limit,
                SettingsUnusable::TooLarge {
                    limit_bytes: LIMIT_BYTES,
                },
            ),
        ];
        for (name, bytes, expected) in cases {
            let scratch = Scratch::new(&format!("unusable-{name}"));
            scratch.write_bytes(&bytes);
            assert_eq!(unusable(&scratch.store()), expected, "{name}");
            assert_eq!(
                scratch.store().save(&SettingChange::Compact(false)),
                Err(SaveError::Unusable(expected)),
                "{name}"
            );
            assert_eq!(scratch.bytes(), bytes, "{name}: the file was written over");
            assert!(
                scratch.leftovers().is_empty(),
                "{name}: {:?}",
                scratch.leftovers()
            );
        }
    }

    #[test]
    fn a_file_exactly_at_the_limit_is_still_read() {
        let scratch = Scratch::new("at-limit");
        let mut text = String::from("schema = 1\n# ");
        let room = usize::try_from(LIMIT_BYTES).unwrap_or(0) - text.len();
        text.push_str(&"x".repeat(room));
        assert_eq!(u64::try_from(text.len()).ok(), Some(LIMIT_BYTES));
        scratch.write(&text);
        assert_eq!(read(&scratch.store()).0, Settings::default());
    }

    #[test]
    fn a_folder_where_the_file_should_be_is_unreadable_and_stays_a_folder() {
        let scratch = Scratch::new("folder");
        std::fs::create_dir_all(scratch.file()).expect("a folder in the file's place");
        assert_eq!(unusable(&scratch.store()), SettingsUnusable::Unreadable);
        assert_eq!(
            scratch.store().save(&SettingChange::Compact(true)),
            Err(SaveError::Unusable(SettingsUnusable::Unreadable))
        );
        assert!(scratch.file().is_dir());
    }

    #[cfg(unix)]
    #[test]
    fn a_settings_file_that_is_a_link_is_written_through_the_link() {
        let scratch = Scratch::new("link");
        let real = scratch.0.join("dotfiles").join("settings.toml");
        std::fs::create_dir_all(real.parent().expect("a parent")).expect("dotfiles");
        std::fs::write(&real, "schema = 1\n").expect("the real file");
        std::fs::create_dir_all(scratch.0.join("nested")).expect("nested");
        std::os::unix::fs::symlink(&real, scratch.file()).expect("a link");

        assert_eq!(scratch.store().save(&SettingChange::Compact(true)), Ok(()));
        assert!(
            std::fs::symlink_metadata(scratch.file())
                .expect("still there")
                .file_type()
                .is_symlink(),
            "the link was replaced by a plain file"
        );
        assert!(
            std::fs::read_to_string(&real)
                .expect("the real file")
                .contains("compact = true")
        );
    }
}
