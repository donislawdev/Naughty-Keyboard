//! What the portal for global shortcuts decides, without asking a bus, so that it is tested
//! on every system (`D145` points 39 and 53, `OBS-217`, `OBS-222`).
//!
//! # What was measured before this was written
//!
//! GNOME 50, `xdg-desktop-portal` 1.21.1, `xdg-desktop-portal-gnome` 50.0, the probe
//! `tools/sondy/portal-global-shortcuts.py` and a physical keyboard:
//!
//! - A program that is no application of a desktop has no identifier, and the portal
//!   refuses it (`An app id is required`). `Registry.Register(<id>)` gives it one, and
//!   only while a file `<id>.desktop` is in a directory of applications.
//! - The shortcuts of a session are the shortcuts that session asked for: a new session
//!   lists none, whatever an earlier one was given. So every start binds again, and a
//!   bind of an identifier the person has accepted before is answered at once (1.5 s,
//!   no window) with the shortcuts as the person left them.
//! - The first bind of an identifier shows a window of the system, and the person chooses
//!   the chords in it. The program offers a chord and is told which one it got, in words
//!   for a person: `Press <Control><Alt>n`, the chord in the syntax of GTK accelerators.
//! - A bind is accepted for a chord that belongs to the system and then never fires, so
//!   an answer of the portal is not proof that a press will arrive (`OBS-217` point 5).
//!
//! So this file knows how to name a chord to the portal, how to read one back out of its
//! words and how to say no to words it cannot read, and where the file that gives the
//! program its identifier is and what is in it.

use core::time::Duration;

use crate::hotkey::{Hotkey, HotkeyId};
use crate::linux_hotkeys::keysym_for_vk;
use crate::linux_ibus_rules::{AddressRefusal, socket_in_address};

/// The identifier of the program for the desktop. It names the file of the entry, the
/// permission the person gives and the page of the program in their settings, so it is
/// never changed once a release has used it.
pub(crate) const APP_ID: &str = "com.donislawdev.NaughtyKeyboard";

/// The file that gives the program its identifier, in a directory of applications.
pub(crate) const ENTRY_FILE: &str = "com.donislawdev.NaughtyKeyboard.desktop";

/// The line that says the program made the entry, so that a file of the same name that
/// somebody else made is never rewritten.
pub(crate) const ENTRY_MARK: &str = "X-NaughtyKeyboard-Entry=made by the palette on request";

/// The bus name, the object and the interfaces of the portal.
pub(crate) const PORTAL_NAME: &str = "org.freedesktop.portal.Desktop";
pub(crate) const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
pub(crate) const SHORTCUTS_INTERFACE: &str = "org.freedesktop.portal.GlobalShortcuts";
pub(crate) const REQUEST_INTERFACE: &str = "org.freedesktop.portal.Request";
pub(crate) const SESSION_INTERFACE: &str = "org.freedesktop.portal.Session";
pub(crate) const REGISTRY_INTERFACE: &str = "org.freedesktop.host.portal.Registry";

/// How long a call to the portal may take to return the handle of its request. The
/// answer of the person comes later and is waited for on its own terms.
pub(crate) const CALL_WAIT: Duration = Duration::from_secs(5);

/// How long the answer to a request that asks nothing of the person may take.
pub(crate) const ANSWER_WAIT: Duration = Duration::from_secs(15);

/// How long the answer to a bind may take. It is the one that shows a window the first
/// time, and a person reads it before they choose.
pub(crate) const BIND_WAIT: Duration = Duration::from_secs(300);

// ---------------------------------------------------------------------------
// Where the session bus is
// ---------------------------------------------------------------------------

/// The file socket of the session bus: `DBUS_SESSION_BUS_ADDRESS` when it is set, and
/// otherwise `bus` in the runtime directory, where the system manager puts it.
///
/// The address is read by the same rule as the address of the input method daemon, so
/// only `unix:path=` is followed and `tcp:`, `nonce-tcp:` and the abstract namespace are
/// refused by name before a connection is made. The runtime directory must be a full
/// path with nothing in it that is not part of a name.
pub(crate) fn session_bus_socket(
    address: Option<&str>,
    runtime_dir: Option<&str>,
) -> Result<String, AddressRefusal> {
    if let Some(address) = address.filter(|address| !address.trim().is_empty()) {
        return socket_in_address(address);
    }
    let dir = runtime_dir
        .map(str::trim)
        .filter(|dir| dir.starts_with('/') && !dir.contains('\0'))
        .ok_or(AddressRefusal::Missing)?;
    Ok(format!("{}/bus", dir.trim_end_matches('/')))
}

// ---------------------------------------------------------------------------
// Requests: the portal answers a call with a signal on a path both sides can name
// ---------------------------------------------------------------------------

/// The part of a unique bus name the portal puts in the path of a request: the name
/// without its colon and with dots as underscores (`:1.2107` is `1_2107`). `None` for
/// anything that is not a unique name.
pub(crate) fn sender_part(unique_name: &str) -> Option<String> {
    let rest = unique_name.strip_prefix(':')?;
    let fits = !rest.is_empty()
        && rest
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'));
    fits.then(|| rest.replace('.', "_"))
}

/// Whether `token` can be the last element of an object path, which is all the portal
/// asks of the token the caller chooses.
pub(crate) fn is_valid_token(token: &str) -> bool {
    !token.is_empty()
        && token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

/// A token for the `n`th request of this process. Two programs of one person have two
/// unique names, so the path is theirs even when the tokens are the same.
pub(crate) fn token(prefix: &str, process: u32, n: u32) -> String {
    format!("{prefix}{process}_{n}")
}

/// The path at which the answer to a request will be announced. It is known before the
/// call is made, which is what lets the caller listen first and so miss no answer.
pub(crate) fn request_path(unique_name: &str, token: &str) -> Option<String> {
    let sender = sender_part(unique_name)?;
    is_valid_token(token)
        .then(|| format!("/org/freedesktop/portal/desktop/request/{sender}/{token}"))
}

/// How a request ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Answer {
    /// The person agreed, or nobody had to.
    Granted,
    /// The person closed the window of the system without agreeing.
    Cancelled,
    /// Anything else, with the code the portal gave.
    Ended(u32),
}

/// The answer a response code stands for: 0 is granted, 1 is cancelled by the person.
pub(crate) const fn answer_of(code: u32) -> Answer {
    match code {
        0 => Answer::Granted,
        1 => Answer::Cancelled,
        other => Answer::Ended(other),
    }
}

// ---------------------------------------------------------------------------
// Naming a chord to the portal and reading one back
// ---------------------------------------------------------------------------

/// The name of a key of the shortcut vocabulary in the portal's syntax and in the
/// accelerators of GTK, which both use the name of the keysym: a letter or a digit as it
/// is typed, `F1` to `F24`, and `space`. The code is the one the vocabulary is written in
/// (`keysym_for_vk`).
fn key_name(vk: u16) -> Option<String> {
    keysym_for_vk(vk)?;
    match vk {
        0x41..=0x5A => Some(char::from(u8::try_from(vk).ok()? + 0x20).to_string()),
        0x30..=0x39 => Some(char::from(u8::try_from(vk).ok()?).to_string()),
        0x70..=0x87 => Some(format!("F{}", vk - 0x70 + 1)),
        0x20 => Some(String::from("space")),
        _ => None,
    }
}

/// The key a name stands for, as the code the vocabulary is written in. Only the names
/// [`key_name`] makes are read: an upper case letter is another keysym and `Shift` held,
/// so it is refused rather than guessed at.
fn key_of(name: &str) -> Option<u16> {
    let mut chars = name.chars();
    let first = chars.next()?;
    if chars.next().is_none() {
        return match first {
            'a'..='z' => Some(u16::from(u8::try_from(first).ok()? - 0x20)),
            '0'..='9' => Some(u16::from(u8::try_from(first).ok()?)),
            _ => None,
        };
    }
    if name == "space" {
        return Some(0x20);
    }
    let digits = name.strip_prefix('F')?;
    if !digits.bytes().all(|byte| byte.is_ascii_digit()) || digits.starts_with('0') {
        return None;
    }
    let number: u16 = digits.parse().ok()?;
    (1..=24).contains(&number).then_some(0x70 + number - 1)
}

/// The chord a program offers the portal, in the syntax of the specification of global
/// shortcuts: `CTRL`, `ALT`, `SHIFT` and `LOGO` joined to the key by `+`. `None` for a
/// key that has no name there.
pub(crate) fn preferred_trigger(hotkey: &Hotkey) -> Option<String> {
    let mut parts = Vec::new();
    if hotkey.ctrl {
        parts.push(String::from("CTRL"));
    }
    if hotkey.alt {
        parts.push(String::from("ALT"));
    }
    if hotkey.shift {
        parts.push(String::from("SHIFT"));
    }
    if hotkey.win {
        parts.push(String::from("LOGO"));
    }
    parts.push(key_name(hotkey.vk)?);
    Some(parts.join("+"))
}

/// The chord the portal says a shortcut has, from the words it gives for a person.
///
/// The words are `Press <Control><Alt>n` on a desktop in English and are in the person's
/// language on another, so the one thing read is the accelerator in them: the single word
/// that holds a `<`, which is the modifiers in angle brackets and then the key. A word
/// that is not an accelerator of this vocabulary, a second one, a modifier that is not
/// `Control`, `Alt`, `Shift` or `Super`, or a key that is not in the vocabulary gives
/// `None`, and the caller shows the words as they came. A guess dressed as a chord would
/// be worse than the words.
pub(crate) fn bound_hotkey(id: HotkeyId, description: &str) -> Option<Hotkey> {
    let mut words = description
        .split_whitespace()
        .filter(|word| word.contains('<'));
    let accelerator = words.next()?;
    if words.next().is_some() {
        return None;
    }
    let (mut ctrl, mut alt, mut shift, mut win) = (false, false, false, false);
    let mut rest = accelerator;
    while let Some(opened) = rest.strip_prefix('<') {
        let (name, after) = opened.split_once('>')?;
        match name.to_ascii_lowercase().as_str() {
            "control" | "ctrl" | "primary" => ctrl = true,
            "alt" | "mod1" => alt = true,
            "shift" => shift = true,
            "super" | "mod4" => win = true,
            _ => return None,
        }
        rest = after;
    }
    let vk = key_of(rest)?;
    Some(Hotkey {
        id,
        ctrl,
        alt,
        shift,
        win,
        vk,
    })
}

// ---------------------------------------------------------------------------
// The entry that gives the program its identifier
// ---------------------------------------------------------------------------

/// Where the desktop looks for the entries of applications, as the environment says.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct DataDirs<'a> {
    pub data_home: Option<&'a str>,
    pub home: Option<&'a str>,
    pub data_dirs: Option<&'a str>,
}

/// The directories of the base directory specification that are paths: a variable that
/// is empty or holds a path that is not a full one is ignored, as the specification says.
fn full_paths(list: &str) -> impl Iterator<Item = &str> {
    list.split(':').filter(|dir| dir.starts_with('/'))
}

/// The directory of the person's own applications, where an entry made on request goes:
/// `applications` in `XDG_DATA_HOME`, or `.local/share/applications` in the home.
pub(crate) fn user_applications(dirs: &DataDirs<'_>) -> Option<String> {
    let base = match dirs.data_home.filter(|dir| dir.starts_with('/')) {
        Some(dir) => dir.trim_end_matches('/').to_owned(),
        None => format!(
            "{}/.local/share",
            dirs.home
                .filter(|dir| dir.starts_with('/'))?
                .trim_end_matches('/')
        ),
    };
    Some(format!("{base}/applications"))
}

/// Every file an entry of this program may be in, in the order the desktop looks: the
/// person's own directory first and then those of the system.
pub(crate) fn entry_candidates(dirs: &DataDirs<'_>) -> Vec<String> {
    let mut out: Vec<String> = user_applications(dirs)
        .into_iter()
        .map(|dir| format!("{dir}/{ENTRY_FILE}"))
        .collect();
    let system = dirs
        .data_dirs
        .filter(|list| !list.trim().is_empty())
        .unwrap_or("/usr/local/share:/usr/share");
    out.extend(
        full_paths(system)
            .map(|dir| format!("{}/applications/{ENTRY_FILE}", dir.trim_end_matches('/'))),
    );
    out
}

/// Why no entry can be written for a program that lives where it does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EntryRefusal {
    /// The path of the program is not a full path, so the desktop could not start it.
    NotAbsolute,
    /// The path holds a character that would end the line of the entry.
    ControlCharacter,
}

/// The characters that make a word of an `Exec` line need quotes.
const SPECIAL_IN_EXEC: &[char] = &[
    ' ', '\t', '\n', '"', '\'', '\\', '>', '<', '~', '|', '&', ';', '$', '*', '?', '#', '(', ')',
    '`',
];

/// The path of the program as one word of an `Exec` line.
///
/// A word with a character of [`SPECIAL_IN_EXEC`] is put in double quotes, and in them
/// `"`, a backtick, `$` and `\` take a backslash each. That backslash is itself a
/// character of the file and takes another, which is what the specification asks for and
/// why a path with one backslash is written with four. A percent sign is a field code
/// for the desktop and is written twice wherever it is.
pub(crate) fn exec_word(path: &str) -> String {
    let quoted = path.contains(SPECIAL_IN_EXEC);
    let mut out = String::with_capacity(path.len() + 2);
    if quoted {
        out.push('"');
    }
    for c in path.chars() {
        match c {
            '%' => out.push_str("%%"),
            '\\' => out.push_str("\\\\\\\\"),
            '"' | '`' | '$' => {
                out.push_str("\\\\");
                out.push(c);
            }
            other => out.push(other),
        }
    }
    if quoted {
        out.push('"');
    }
    out
}

/// The text of the entry for the program at `exe`.
///
/// The entry is not shown in the menus of the desktop (`NoDisplay`): it exists to give the
/// program an identifier, not to put an icon on anybody's screen. It starts the program
/// the way the person started it, which is what a click in the settings of the desktop
/// would ask for.
pub(crate) fn entry_text(exe: &str) -> Result<String, EntryRefusal> {
    if !exe.starts_with('/') {
        return Err(EntryRefusal::NotAbsolute);
    }
    if exe.chars().any(char::is_control) {
        return Err(EntryRefusal::ControlCharacter);
    }
    Ok(format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=Naughty Keyboard\n\
         Comment=Hostile test data for the field under the cursor\n\
         Exec={}\n\
         Terminal=false\n\
         NoDisplay=true\n\
         {ENTRY_MARK}\n",
        exec_word(exe)
    ))
}

/// Whether an entry that is already there is the one this program made.
pub(crate) fn is_our_entry(content: &str) -> bool {
    content.lines().any(|line| line == ENTRY_MARK)
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;

    fn hotkey(ctrl: bool, alt: bool, shift: bool, win: bool, vk: u16) -> Hotkey {
        Hotkey {
            id: HotkeyId(3),
            ctrl,
            alt,
            shift,
            win,
            vk,
        }
    }

    /// Whether `id` is an identifier of an application as the desktop reads it: elements
    /// separated by dots, at least two, each of letters, digits, `_` and `-`, none empty
    /// and none starting with a digit, at most 255 characters in all.
    fn is_valid_app_id(id: &str) -> bool {
        let elements: Vec<&str> = id.split('.').collect();
        id.len() <= 255
            && elements.len() >= 2
            && elements.iter().all(|element| {
                !element.is_empty()
                    && !element.starts_with(|c: char| c.is_ascii_digit())
                    && element
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
            })
    }

    #[test]
    fn the_identifier_of_the_program_is_one_the_desktop_reads_and_names_its_entry() {
        assert!(is_valid_app_id(APP_ID), "{APP_ID}");
        assert_eq!(ENTRY_FILE, format!("{APP_ID}.desktop"));
    }

    #[test]
    fn an_identifier_is_dotted_elements_of_names() {
        for good in ["a.b", "com.example.App", "org.gnome.Foo-Bar_2", "x1.y_2"] {
            assert!(is_valid_app_id(good), "{good}");
        }
        for bad in [
            "",
            "single",
            "a..b",
            ".a.b",
            "a.b.",
            "a.1b",
            "a.b c",
            "a.b/c",
            "a.b\u{00e9}",
        ] {
            assert!(!is_valid_app_id(bad), "{bad:?}");
        }
        let long = format!("a.{}", "b".repeat(254));
        assert!(!is_valid_app_id(&long), "256 characters");
    }

    #[test]
    fn the_names_of_the_portal_are_the_ones_its_specification_gives() {
        assert_eq!(PORTAL_NAME, "org.freedesktop.portal.Desktop");
        assert_eq!(PORTAL_PATH, "/org/freedesktop/portal/desktop");
        assert_eq!(
            SHORTCUTS_INTERFACE,
            "org.freedesktop.portal.GlobalShortcuts"
        );
        assert_eq!(REQUEST_INTERFACE, "org.freedesktop.portal.Request");
        assert_eq!(SESSION_INTERFACE, "org.freedesktop.portal.Session");
        assert_eq!(REGISTRY_INTERFACE, "org.freedesktop.host.portal.Registry");
    }

    #[test]
    fn a_call_is_given_less_time_than_an_answer_and_an_answer_less_than_a_person() {
        assert!(CALL_WAIT < ANSWER_WAIT, "a handle comes before an answer");
        assert!(
            ANSWER_WAIT < BIND_WAIT,
            "a window of the system takes longer than a portal that answers"
        );
    }

    #[test]
    fn the_session_bus_is_the_file_socket_its_address_names() {
        assert_eq!(
            session_bus_socket(Some("unix:path=/run/user/1000/bus"), None).as_deref(),
            Ok("/run/user/1000/bus")
        );
        assert_eq!(
            session_bus_socket(Some("unix:path=/run/user/1000/bus,guid=abc"), Some("/x"))
                .as_deref(),
            Ok("/run/user/1000/bus"),
            "an address that is set wins over the directory"
        );
    }

    #[test]
    fn a_bus_that_is_not_a_file_socket_is_refused_by_name() {
        for address in [
            "tcp:host=example.org,port=1234",
            "nonce-tcp:host=localhost,port=1",
            "unix:abstract=/tmp/dbus-xyz",
            "unix:dir=/tmp",
            "launchd:env=X",
        ] {
            assert_eq!(
                session_bus_socket(Some(address), Some("/run/user/1000")),
                Err(AddressRefusal::NotAFileSocket),
                "{address}"
            );
        }
    }

    #[test]
    fn without_an_address_the_bus_is_in_the_runtime_directory() {
        for none in [None, Some(""), Some("   ")] {
            assert_eq!(
                session_bus_socket(none, Some("/run/user/1000")).as_deref(),
                Ok("/run/user/1000/bus")
            );
        }
        assert_eq!(
            session_bus_socket(None, Some("/run/user/1000/")).as_deref(),
            Ok("/run/user/1000/bus"),
            "a trailing slash is not doubled"
        );
        for no_dir in [None, Some(""), Some("relative/dir"), Some("run/user/1")] {
            assert_eq!(
                session_bus_socket(None, no_dir),
                Err(AddressRefusal::Missing),
                "{no_dir:?}"
            );
        }
        assert_eq!(
            session_bus_socket(None, Some("/run/user/1\u{0}000")),
            Err(AddressRefusal::Missing),
            "a name that is cut short by a NUL is not a name"
        );
    }

    #[test]
    fn a_unique_name_is_the_sender_of_a_path_without_its_colon_and_dots() {
        assert_eq!(sender_part(":1.2107").as_deref(), Some("1_2107"));
        assert_eq!(sender_part(":1.7").as_deref(), Some("1_7"));
        for bad in ["", ":", "1.2107", "org.freedesktop.portal", ":1/2", ":1 2"] {
            assert_eq!(sender_part(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn the_path_of_a_request_is_known_before_the_call_is_made() {
        assert_eq!(
            request_path(":1.2107", "nkb_77_1").as_deref(),
            Some("/org/freedesktop/portal/desktop/request/1_2107/nkb_77_1")
        );
        assert_eq!(request_path(":1.2107", ""), None);
        assert_eq!(request_path(":1.2107", "a/b"), None);
        assert_eq!(request_path(":1.2107", "a.b"), None);
        assert_eq!(request_path("not a name", "t"), None);
    }

    #[test]
    fn a_token_is_a_name_the_process_and_the_count_make() {
        let one = token("nkb", 4242, 1);
        let two = token("nkb", 4242, 2);
        assert_eq!(one, "nkb4242_1");
        assert_ne!(one, two);
        assert!(is_valid_token(&one));
        assert!(!is_valid_token("with space"));
        assert!(!is_valid_token("dash-ed"));
    }

    #[test]
    fn a_code_is_granted_cancelled_or_the_code() {
        assert_eq!(answer_of(0), Answer::Granted);
        assert_eq!(answer_of(1), Answer::Cancelled);
        assert_eq!(answer_of(2), Answer::Ended(2));
        assert_eq!(answer_of(77), Answer::Ended(77));
    }

    /// The accelerator of GTK for a hotkey, written the way the desktop writes it, which
    /// is what the words of the portal hold.
    fn gtk_accelerator(hotkey: &Hotkey) -> String {
        let mut out = String::new();
        if hotkey.ctrl {
            out.push_str("<Control>");
        }
        if hotkey.alt {
            out.push_str("<Alt>");
        }
        if hotkey.shift {
            out.push_str("<Shift>");
        }
        if hotkey.win {
            out.push_str("<Super>");
        }
        out.push_str(&key_name(hotkey.vk).expect("a key of the vocabulary"));
        out
    }

    /// Every key of the vocabulary, as the code it is written in.
    fn vocabulary() -> Vec<u16> {
        (0x41..=0x5A)
            .chain(0x30..=0x39)
            .chain(0x70..=0x87)
            .chain([0x20])
            .collect()
    }

    #[test]
    fn a_key_has_one_name_and_the_name_gives_the_key_back() {
        for vk in vocabulary() {
            let name = key_name(vk).expect("named");
            assert_eq!(key_of(&name), Some(vk), "{vk:#x} {name}");
        }
        assert_eq!(key_name(0x41).as_deref(), Some("a"));
        assert_eq!(key_name(0x5A).as_deref(), Some("z"));
        assert_eq!(key_name(0x35).as_deref(), Some("5"));
        assert_eq!(key_name(0x70).as_deref(), Some("F1"));
        assert_eq!(key_name(0x87).as_deref(), Some("F24"));
        assert_eq!(key_name(0x20).as_deref(), Some("space"));
        for outside in [0x00, 0x1F, 0x21, 0x40, 0x5B, 0x88, 0xFF] {
            assert_eq!(key_name(outside), None, "{outside:#x}");
        }
    }

    #[test]
    fn a_name_that_is_not_one_of_ours_is_no_key() {
        for name in [
            "", "A", "N", "F0", "F25", "F01", "F", "f1", "Space", "SPACE", "ab", "-", "+", "Tab",
            "Return", "F1 ", " a", "F1x", "F+1", "F-1", "\u{00e9}",
        ] {
            assert_eq!(key_of(name), None, "{name:?}");
        }
    }

    #[test]
    fn the_chord_offered_is_in_the_syntax_of_the_specification() {
        let cases = [
            (hotkey(true, true, false, false, 0x4E), "CTRL+ALT+n"),
            (hotkey(true, false, true, false, 0x70), "CTRL+SHIFT+F1"),
            (hotkey(false, true, true, false, 0x20), "ALT+SHIFT+space"),
            (
                hotkey(true, true, true, true, 0x31),
                "CTRL+ALT+SHIFT+LOGO+1",
            ),
            (hotkey(false, false, false, true, 0x4B), "LOGO+k"),
        ];
        for (chord, wanted) in cases {
            assert_eq!(preferred_trigger(&chord).as_deref(), Some(wanted));
        }
        assert_eq!(
            preferred_trigger(&hotkey(true, true, false, false, 0x21)),
            None
        );
    }

    #[test]
    fn the_words_of_the_portal_give_the_chord_back() {
        let id = HotkeyId(5);
        let chord = bound_hotkey(id, "Press <Control><Alt>n").expect("a chord");
        assert_eq!(chord, hotkey_with(id, true, true, false, false, 0x4E));
        let other = bound_hotkey(id, "Press <Control><Shift>F5").expect("a chord");
        assert_eq!(other, hotkey_with(id, true, false, true, false, 0x74));
    }

    fn hotkey_with(id: HotkeyId, ctrl: bool, alt: bool, shift: bool, win: bool, vk: u16) -> Hotkey {
        Hotkey {
            id,
            ctrl,
            alt,
            shift,
            win,
            vk,
        }
    }

    #[test]
    fn every_chord_with_a_modifier_survives_the_words() {
        let id = HotkeyId(9);
        for vk in vocabulary() {
            for bits in 1..16_u8 {
                let chord = hotkey_with(
                    id,
                    bits & 1 != 0,
                    bits & 2 != 0,
                    bits & 4 != 0,
                    bits & 8 != 0,
                    vk,
                );
                let words = format!("Press {}", gtk_accelerator(&chord));
                assert_eq!(bound_hotkey(id, &words), Some(chord), "{words}");
            }
        }
    }

    /// A key with no modifier is written as the key alone, and a key alone is a word like
    /// any other in a sentence, so it cannot be told from one. The palette takes no such
    /// shortcut (`Bindings::with`), and the person who chose one in the window of the system
    /// is shown the words as the system gave them.
    #[test]
    fn a_key_with_no_modifier_is_not_read_out_of_the_words() {
        let id = HotkeyId(9);
        for vk in vocabulary() {
            let chord = hotkey_with(id, false, false, false, false, vk);
            let words = format!("Press {}", gtk_accelerator(&chord));
            assert_eq!(bound_hotkey(id, &words), None, "{words}");
        }
    }

    #[test]
    fn the_language_of_the_words_does_not_matter_only_the_accelerator_in_them() {
        let id = HotkeyId(1);
        let wanted = hotkey_with(id, true, true, false, false, 0x4E);
        for words in [
            "Appuyez sur <Control><Alt>n",
            "Pressione <Control><Alt>n",
            "<Control><Alt>n",
            "  Press   <Control><Alt>n  ",
            "Press <ctrl><alt>n",
            "Press <Primary><Mod1>n",
        ] {
            assert_eq!(bound_hotkey(id, words), Some(wanted), "{words:?}");
        }
        assert_eq!(
            bound_hotkey(id, "Press <Super>k"),
            Some(hotkey_with(id, false, false, false, true, 0x4B))
        );
        assert_eq!(
            bound_hotkey(id, "Press <Mod4>k"),
            Some(hotkey_with(id, false, false, false, true, 0x4B))
        );
    }

    #[test]
    fn words_that_do_not_name_one_chord_of_the_vocabulary_name_none() {
        let id = HotkeyId(1);
        for words in [
            "",
            "Press",
            "Not assigned",
            "Press <Control><Alt>N",
            "Press <Control><Alt>Return",
            "Press <Control><Alt>",
            "Press <Control><Hyper>n",
            "Press <Control><Meta>n",
            "Press <Control><Alt>n or <Super>k",
            "Press <Control><Alt n",
            "Press Control>n",
            "Press <><Alt>n",
            "Press <Control>>n",
            "Press <Control><Alt>F25",
            "Press <Control><Alt>n<Shift>",
        ] {
            assert_eq!(bound_hotkey(id, words), None, "{words:?}");
        }
    }

    #[test]
    fn the_directory_of_the_person_is_the_data_home_or_the_home() {
        let both = DataDirs {
            data_home: Some("/data"),
            home: Some("/home/a"),
            data_dirs: None,
        };
        assert_eq!(
            user_applications(&both).as_deref(),
            Some("/data/applications")
        );
        let home = DataDirs {
            data_home: None,
            home: Some("/home/a/"),
            data_dirs: None,
        };
        assert_eq!(
            user_applications(&home).as_deref(),
            Some("/home/a/.local/share/applications")
        );
        for data_home in [Some(""), Some("relative"), Some("~/x")] {
            let ignored = DataDirs {
                data_home,
                home: Some("/home/a"),
                data_dirs: None,
            };
            assert_eq!(
                user_applications(&ignored).as_deref(),
                Some("/home/a/.local/share/applications"),
                "{data_home:?} is not a full path and is ignored"
            );
        }
        let nowhere = DataDirs {
            data_home: Some("rel"),
            home: Some("rel"),
            data_dirs: None,
        };
        assert_eq!(user_applications(&nowhere), None);
        assert_eq!(user_applications(&DataDirs::default()), None);
    }

    #[test]
    fn the_entry_is_looked_for_in_the_directory_of_the_person_and_then_the_system() {
        let dirs = DataDirs {
            data_home: Some("/data"),
            home: None,
            data_dirs: Some("/opt/share/:/usr/share:relative:"),
        };
        assert_eq!(
            entry_candidates(&dirs),
            vec![
                format!("/data/applications/{ENTRY_FILE}"),
                format!("/opt/share/applications/{ENTRY_FILE}"),
                format!("/usr/share/applications/{ENTRY_FILE}"),
            ]
        );
        let defaults = DataDirs {
            data_home: None,
            home: Some("/home/a"),
            data_dirs: Some("  "),
        };
        assert_eq!(
            entry_candidates(&defaults),
            vec![
                format!("/home/a/.local/share/applications/{ENTRY_FILE}"),
                format!("/usr/local/share/applications/{ENTRY_FILE}"),
                format!("/usr/share/applications/{ENTRY_FILE}"),
            ]
        );
    }

    #[test]
    fn a_plain_path_is_one_word_and_a_path_with_a_special_character_is_quoted() {
        assert_eq!(exec_word("/opt/nkb/nkb-gui"), "/opt/nkb/nkb-gui");
        assert_eq!(exec_word("/home/a b/nkb-gui"), "\"/home/a b/nkb-gui\"");
        assert_eq!(exec_word("/home/a;b/nkb"), "\"/home/a;b/nkb\"");
        assert_eq!(exec_word("/home/a'b/nkb"), "\"/home/a'b/nkb\"");
    }

    #[test]
    fn inside_quotes_four_characters_take_a_backslash_that_takes_another() {
        assert_eq!(exec_word("/a b/\"q\""), "\"/a b/\\\\\"q\\\\\"\"");
        assert_eq!(exec_word("/a b/$HOME"), "\"/a b/\\\\$HOME\"");
        assert_eq!(exec_word("/a b/`x`"), "\"/a b/\\\\`x\\\\`\"");
        assert_eq!(exec_word("/a b/c\\d"), "\"/a b/c\\\\\\\\d\"");
    }

    #[test]
    fn a_percent_sign_is_written_twice_quoted_or_not() {
        assert_eq!(exec_word("/opt/100%/nkb"), "/opt/100%%/nkb");
        assert_eq!(exec_word("/opt/1 00%/nkb"), "\"/opt/1 00%%/nkb\"");
    }

    #[test]
    fn a_backslash_outside_quotes_cannot_occur_because_it_forces_quotes() {
        assert!(exec_word("/a\\b").starts_with('"'));
    }

    #[test]
    fn the_entry_names_the_program_and_is_not_shown_in_menus() {
        let text = entry_text("/opt/nkb/nkb-gui").expect("an entry");
        assert!(text.starts_with("[Desktop Entry]\n"));
        assert!(text.contains("\nType=Application\n"));
        assert!(text.contains("\nExec=/opt/nkb/nkb-gui\n"));
        assert!(text.contains("\nNoDisplay=true\n"));
        assert!(text.contains("\nTerminal=false\n"));
        assert!(text.ends_with('\n'));
        assert!(is_our_entry(&text));
        assert!(
            text.lines()
                .all(|line| line.is_empty() || line.starts_with('[') || line.contains('=')),
            "every line is a key"
        );
    }

    #[test]
    fn a_program_whose_place_cannot_be_written_down_gets_no_entry() {
        assert_eq!(entry_text("nkb-gui"), Err(EntryRefusal::NotAbsolute));
        assert_eq!(entry_text("./nkb-gui"), Err(EntryRefusal::NotAbsolute));
        assert_eq!(entry_text(""), Err(EntryRefusal::NotAbsolute));
        for bad in ["/a\nb", "/a\rb", "/a\tb", "/a\0b"] {
            assert_eq!(
                entry_text(bad),
                Err(EntryRefusal::ControlCharacter),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn an_entry_that_somebody_else_made_is_not_ours() {
        assert!(!is_our_entry(
            "[Desktop Entry]\nType=Application\nName=Other\n"
        ));
        assert!(!is_our_entry(""));
        assert!(
            !is_our_entry(
                "[Desktop Entry]\n# X-NaughtyKeyboard-Entry=made by the palette on request\n"
            ),
            "a comment is not the mark"
        );
        assert!(is_our_entry(&format!("[Desktop Entry]\n{ENTRY_MARK}\n")));
    }
}
