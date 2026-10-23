//! The decisions of the Wayland route that need no input method (`D145` points 37 and 41,
//! `OBS-217` to `OBS-219`).
//!
//! A Wayland session lets a client type only the characters of the compositor's layout,
//! and the rest is lost or changed without a word, so the text goes to the desktop's input
//! method instead: this program registers an engine of its own with the IBus daemon,
//! makes it the global engine for the length of one send, and commits the text from it.
//! Measured on GNOME 50 (Ubuntu 26.04, 2026-10-23), 23 of 23 characters exact in GTK4,
//! Slint, Qt and xterm, among them line feed, tab, `U+200B`, `U+202E`, `U+FEFF` and
//! `U+00A0`. The engine exists only while this process runs, so a crash leaves nothing
//! behind.
//!
//! The system is `linux/ibus/`. Everything that is only a decision about names, numbers
//! and text lives here and is tested on every system. Two of those decisions are about
//! what must never happen: the daemon is reached through a FILE socket named by its own
//! address file and no other address (untouchable rule 17), and a global engine that
//! somebody else set is never lost.

use core::time::Duration;

use crate::linux_rules::KEYSYM_ESCAPE;

/// What every engine this program registers is called before its process number, so the
/// engine of a send that is still running can be told from the engine of the person.
pub(crate) const ENGINE_PREFIX: &str = "nkb-send";

/// How many BYTES of UTF-8 one commit carries at most. On Wayland the desktop hands a
/// commit to the application as one protocol message, and a message of more than 4096
/// bytes is refused by the library, which closes the connection of the application
/// ("Error reading events from display: Argument list too long", measured with GTK4 on
/// GNOME 50: a commit of 4000 characters went through and 4096 took the application down,
/// so a value of 65535 characters did). The rest of the limit is left for the message's
/// own framing, and a long value is cut into pieces that reach the application in order.
pub(crate) const COMMIT_BYTES: usize = 3500;

/// The pause between two pieces of one value. Commits sent one after the other without it
/// are LOST on the way to the application, without a word: 65535 characters sent at once
/// reached GTK4 as 8191, and with 10 ms between the pieces one value in two still lost a
/// piece. 100 000 characters in pieces of 3500 bytes came through five times in five at
/// 50 and 75 ms, and one time in five at 25 ms. 60 ms is that, with a little room. A value
/// of one piece pays nothing.
pub(crate) const COMMIT_GAP: Duration = Duration::from_millis(60);

/// How often `Escape` is looked for while the send waits.
pub(crate) const ESCAPE_LOOK: Duration = Duration::from_millis(10);

/// The bit of the key state that marks a key being let go, in the form IBus hands it to
/// an engine.
const IBUS_RELEASE_MASK: u32 = 1 << 30;

/// The modifiers of the key state that make a key something other than itself: Shift,
/// Control, Alt (Mod1), Mod4, Super, Hyper and Meta. The lock bits and Num Lock are not
/// among them.
const IBUS_MODIFIER_MASK: u32 =
    (1 << 0) | (1 << 2) | (1 << 3) | (1 << 6) | (1 << 26) | (1 << 27) | (1 << 28);

/// Whether the key an engine was handed is `Escape` pressed or let go, on its own. While
/// the engine of a send is the global one every key of the person goes through it, which
/// is the one way this route has to hear `Escape`: the engine takes it, press and release
/// alike, so the application does not get half of a key (`D96`).
pub(crate) fn is_plain_escape(keyval: u32, state: u32) -> bool {
    keyval == KEYSYM_ESCAPE && state & IBUS_MODIFIER_MASK == 0
}

/// Whether the key state says the key was let go and not pressed.
pub(crate) fn is_release(state: u32) -> bool {
    state & IBUS_RELEASE_MASK != 0
}

/// How long the daemon is given to tell the engine that an input context has the focus.
/// The daemon does it within milliseconds of the engine being made the global one
/// (0.01 s measured), so this is a ceiling for a daemon that is busy and not a pause.
pub(crate) const FOCUS_WAIT: Duration = Duration::from_secs(2);

/// The pause between the daemon's notice of the focus and the first commit. A commit sent
/// at once is dropped by an application that has not finished switching to the engine.
/// 300 ms is what the prototype sent with (`OBS-219`), and the loop of the route measures
/// shorter ones (`tools/petla-ofiary-wayland.py`).
pub(crate) const FOCUS_SETTLE: Duration = Duration::from_millis(300);

/// The pause after the last commit, once the daemon has answered a question put after it,
/// before the engine of the person is put back and the process ends. Both take the
/// engine out from under an application that has not yet read the commit. The pause is a
/// hedge for the slow ones, measured by the loop of the route.
pub(crate) const SETTLE_AFTER_COMMIT: Duration = Duration::from_millis(300);

/// How long the daemon is given to forget the engine of this send after the connection
/// of it is closed. It drops the global engine with it, WHATEVER engine was made the
/// global one in between (measured on GNOME 50: the person's engine put back from the same
/// connection, and even from another process, was gone once ours closed), so the person's
/// engine is asked for again from a second connection, once the daemon has let go.
pub(crate) const FORGET_WAIT: Duration = Duration::from_millis(1500);

/// How often the daemon is asked, while it is being waited for.
pub(crate) const FORGET_LOOK: Duration = Duration::from_millis(20);

/// How long one send may take from the first connection to the last, however slow the
/// daemon is. A daemon that stopped answering is waited for this long and no longer: the
/// connection is shut then, every call that waits on it returns, and nothing is sent.
pub(crate) const DEADLINE: Duration = Duration::from_secs(20);

/// How long a value of `pieces` pieces may take: the time of a send that has one, and for
/// every further piece its pause and a margin. A long value is paced on purpose
/// (`COMMIT_GAP`), so the watch over the connection must not close it on the way.
pub(crate) fn deadline_for(pieces: usize) -> Duration {
    DEADLINE + (COMMIT_GAP + CALL_WAIT / 50) * u32::try_from(pieces).unwrap_or(u32::MAX)
}

/// How long the daemon is given to answer one call.
pub(crate) const CALL_WAIT: Duration = Duration::from_secs(5);

/// The name of the engine of the send run by process `pid`.
pub(crate) fn engine_name(pid: u32) -> String {
    format!("{ENGINE_PREFIX}-{pid}")
}

/// Whether `name` is the name of an engine of ours, of this send or of another.
pub(crate) fn is_our_engine(name: &str) -> bool {
    name.strip_prefix(ENGINE_PREFIX)
        .is_some_and(|rest| rest.starts_with('-'))
}

/// The engine to put back once the send is over, if there is one.
///
/// The person's own engine, which `previous` names, is restored. Nothing is restored when
/// there was none (the engine of ours then goes with the process), and neither when the
/// one found is another send's: restoring that would hand the next person the engine of a
/// process that has gone.
pub(crate) fn engine_to_restore(previous: Option<&str>) -> Option<&str> {
    previous.filter(|name| !name.is_empty() && !is_our_engine(name))
}

/// Why the address of the daemon cannot be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AddressRefusal {
    /// There is no address to read.
    Missing,
    /// The address names something other than a file socket on this machine: TCP, a
    /// socket of the abstract namespace, a directory to look in. Refused before any
    /// connection is made, because a keystroke is data like any other.
    NotAFileSocket,
    /// The address does not read as an address.
    Malformed,
}

/// What the environment says about where the daemon's address is.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Where<'a> {
    pub config_home: Option<&'a str>,
    pub home: Option<&'a str>,
    pub machine_id: Option<&'a str>,
    pub wayland_display: Option<&'a str>,
}

/// The file the IBus daemon writes its address to, for this session: its directory is
/// the configuration directory of the person and its name is the machine, the word
/// `unix` and the Wayland socket (`b80044...-unix-wayland-0`, `OBS-219`). `None` when
/// any part is missing or cannot be part of a name, so that nothing the environment
/// says can lead out of the directory.
pub(crate) fn address_file(place: &Where<'_>) -> Option<String> {
    let config = match place.config_home.filter(|path| path.starts_with('/')) {
        Some(path) => path.trim_end_matches('/').to_owned(),
        None => format!(
            "{}/.config",
            place
                .home
                .filter(|path| path.starts_with('/'))?
                .trim_end_matches('/')
        ),
    };
    let machine = place.machine_id?.trim();
    let display = display_name(place.wayland_display?)?;
    if machine.is_empty() || !machine.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
        return None;
    }
    Some(format!("{config}/ibus/bus/{machine}-unix-{display}"))
}

/// The name of the Wayland socket: what `WAYLAND_DISPLAY` says, or its last part when it
/// is a path (the protocol allows both).
fn display_name(value: &str) -> Option<&str> {
    let name = value.trim().rsplit('/').next()?;
    let plain = !name.is_empty()
        && !name.starts_with('.')
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'));
    plain.then_some(name)
}

/// The socket in the address file the daemon writes: the line `IBUS_ADDRESS=...`, the
/// others being comments and the daemon's process number.
pub(crate) fn socket_in_file(content: &str) -> Result<String, AddressRefusal> {
    content
        .lines()
        .find_map(|line| line.trim().strip_prefix("IBUS_ADDRESS="))
        .ok_or(AddressRefusal::Missing)
        .and_then(socket_in_address)
}

/// The path of the file socket an address names.
///
/// Only `unix:path=...` is accepted, optionally followed by more keys such as `guid`.
/// Every other form is refused by name rather than followed: `tcp:` and `nonce-tcp:`
/// would send keystrokes over the network, `unix:abstract=` and `unix:dir=` name sockets
/// this program does not look for, and a library that parsed the address itself would
/// follow any of them (the stream is opened by this program, not by the library).
pub(crate) fn socket_in_address(address: &str) -> Result<String, AddressRefusal> {
    let address = address.trim();
    if address.is_empty() {
        return Err(AddressRefusal::Missing);
    }
    // More than one address means the daemon can be reached more than one way, and the
    // first that is not a file socket is a way this program does not take.
    let mut found = None;
    for one in address.split(';').filter(|part| !part.trim().is_empty()) {
        let Some(keys) = one.trim().strip_prefix("unix:") else {
            return Err(AddressRefusal::NotAFileSocket);
        };
        let mut path = None;
        for pair in keys.split(',') {
            let (key, value) = pair.split_once('=').ok_or(AddressRefusal::Malformed)?;
            match key {
                "path" if path.is_none() => path = Some(unescape(value)?),
                "path" => return Err(AddressRefusal::Malformed),
                "abstract" | "dir" | "tmpdir" | "runtime" => {
                    return Err(AddressRefusal::NotAFileSocket);
                }
                _ => {}
            }
        }
        if found.is_none() {
            found = Some(path.ok_or(AddressRefusal::NotAFileSocket)?);
        }
    }
    let path = found.ok_or(AddressRefusal::Missing)?;
    if path.starts_with('/') && !path.contains('\0') {
        Ok(path)
    } else {
        Err(AddressRefusal::Malformed)
    }
}

/// The value of an address key, with `%XX` read as the byte it stands for.
fn unescape(value: &str) -> Result<String, AddressRefusal> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while let Some(&byte) = bytes.get(at) {
        if byte == b'%' {
            let digits = bytes.get(at + 1..at + 3).ok_or(AddressRefusal::Malformed)?;
            let text = core::str::from_utf8(digits).map_err(|_| AddressRefusal::Malformed)?;
            decoded.push(u8::from_str_radix(text, 16).map_err(|_| AddressRefusal::Malformed)?);
            at += 3;
        } else {
            decoded.push(byte);
            at += 1;
        }
    }
    String::from_utf8(decoded).map_err(|_| AddressRefusal::Malformed)
}

/// `text` in pieces of at most `COMMIT_BYTES` bytes, in order. A character is never cut:
/// the pieces are cut between characters, so each is valid text on its own.
pub(crate) fn pieces(text: &str) -> Vec<&str> {
    let mut pieces = Vec::new();
    let mut start = 0;
    for (at, character) in text.char_indices() {
        if at + character.len_utf8() - start > COMMIT_BYTES {
            pieces.push(&text[start..at]);
            start = at;
        }
    }
    if start < text.len() {
        pieces.push(&text[start..]);
    }
    pieces
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;

    fn place() -> Where<'static> {
        Where {
            config_home: None,
            home: Some("/home/test"),
            machine_id: Some("b80044621b654098b8be31134f13b078\n"),
            wayland_display: Some("wayland-0"),
        }
    }

    #[test]
    fn the_address_file_is_named_by_the_machine_and_the_wayland_socket() {
        assert_eq!(
            address_file(&place()).as_deref(),
            Some("/home/test/.config/ibus/bus/b80044621b654098b8be31134f13b078-unix-wayland-0")
        );
    }

    #[test]
    fn the_configuration_directory_of_the_person_wins_over_the_home() {
        let place = Where {
            config_home: Some("/var/cfg/"),
            ..place()
        };
        assert_eq!(
            address_file(&place).as_deref(),
            Some("/var/cfg/ibus/bus/b80044621b654098b8be31134f13b078-unix-wayland-0")
        );
        let relative = Where {
            config_home: Some("cfg"),
            ..self::place()
        };
        assert!(
            address_file(&relative)
                .expect("the home stands in")
                .starts_with("/home/test/.config/"),
            "a relative XDG_CONFIG_HOME is not a directory"
        );
    }

    #[test]
    fn a_wayland_socket_given_as_a_path_is_read_by_its_last_part() {
        let place = Where {
            wayland_display: Some("/run/user/1000/wayland-1"),
            ..place()
        };
        assert!(
            address_file(&place)
                .expect("a path names a socket")
                .ends_with("-unix-wayland-1")
        );
    }

    #[test]
    fn nothing_the_environment_says_leads_out_of_the_directory() {
        for display in [
            "",
            "..",
            ".hidden",
            "wayland/../x/",
            "a b",
            "a;b",
            "wayland-0/",
        ] {
            let place = Where {
                wayland_display: Some(display),
                ..place()
            };
            assert_eq!(address_file(&place), None, "{display:?}");
        }
        for machine in ["", "  ", "../etc", "ab/cd", "ab cd"] {
            let place = Where {
                machine_id: Some(machine),
                ..place()
            };
            assert_eq!(address_file(&place), None, "{machine:?}");
        }
    }

    #[test]
    fn a_missing_part_means_no_file_and_not_a_guess() {
        assert_eq!(address_file(&Where::default()), None);
        assert_eq!(
            address_file(&Where {
                home: None,
                ..place()
            }),
            None
        );
        assert_eq!(
            address_file(&Where {
                wayland_display: None,
                ..place()
            }),
            None,
            "without the Wayland socket the file of an X session could be taken for ours"
        );
        assert_eq!(
            address_file(&Where {
                home: Some("home/test"),
                ..place()
            }),
            None,
            "a relative home is not a directory"
        );
    }

    #[test]
    fn the_daemon_file_gives_its_file_socket() {
        let file = "# This file is created by ibus-daemon, please do not modify it.\n\
                    IBUS_ADDRESS=unix:path=/home/test/.config/ibus/bus/x-unix-wayland-0,guid=ab12\n\
                    IBUS_DAEMON_PID=2290\n";
        assert_eq!(
            socket_in_file(file).as_deref(),
            Ok("/home/test/.config/ibus/bus/x-unix-wayland-0")
        );
    }

    #[test]
    fn a_file_without_the_address_line_has_no_address() {
        assert_eq!(
            socket_in_file("# comment\nIBUS_DAEMON_PID=1\n"),
            Err(AddressRefusal::Missing)
        );
        assert_eq!(socket_in_file(""), Err(AddressRefusal::Missing));
        assert_eq!(
            socket_in_file("IBUS_ADDRESS=\n"),
            Err(AddressRefusal::Missing)
        );
    }

    #[test]
    fn only_a_file_socket_is_taken_and_every_other_form_is_named() {
        // Untouchable rule 17: nothing leaves the machine. Each of these would be a TCP
        // connection, a socket this program does not look for, or a guess.
        for address in [
            "tcp:host=192.168.1.5,port=1234",
            "tcp:host=localhost,port=1234,family=ipv4",
            "nonce-tcp:host=localhost,port=1,noncefile=/tmp/n",
            "unix:abstract=/tmp/dbus-AbC",
            "unix:dir=/tmp",
            "unix:tmpdir=/tmp",
            "unix:runtime=yes",
            "unix:guid=ab12",
            "launchd:env=X",
            "systemd:",
            "autolaunch:",
        ] {
            assert_eq!(
                socket_in_address(address),
                Err(AddressRefusal::NotAFileSocket),
                "{address}"
            );
        }
    }

    #[test]
    fn a_list_of_addresses_is_refused_when_any_of_them_is_not_a_file_socket() {
        assert_eq!(
            socket_in_address("unix:path=/run/a;tcp:host=h,port=1"),
            Err(AddressRefusal::NotAFileSocket)
        );
        assert_eq!(
            socket_in_address("unix:path=/run/a;unix:path=/run/b").as_deref(),
            Ok("/run/a"),
            "the first of the file sockets"
        );
    }

    #[test]
    fn an_address_that_does_not_read_is_malformed() {
        for address in [
            "unix:path",
            "unix:path=relative/socket",
            "unix:path=",
            "unix:path=/a,path=/b",
            "unix:path=/a%2",
            "unix:path=/a%zz",
            "unix:path=/a%00b",
            "unix:path=/a%ff",
        ] {
            assert_eq!(
                socket_in_address(address),
                Err(AddressRefusal::Malformed),
                "{address}"
            );
        }
        assert_eq!(socket_in_address(""), Err(AddressRefusal::Missing));
        assert_eq!(socket_in_address("  ;  "), Err(AddressRefusal::Missing));
    }

    #[test]
    fn escaped_bytes_of_the_path_are_read() {
        assert_eq!(
            socket_in_address("unix:path=/home/a%20b/%C5%BC,guid=1").as_deref(),
            Ok("/home/a b/\u{17c}")
        );
    }

    #[test]
    fn engines_of_ours_are_told_from_the_engines_of_the_person() {
        assert_eq!(engine_name(42), "nkb-send-42");
        assert!(is_our_engine("nkb-send-42"));
        assert!(is_our_engine(&engine_name(1)));
        for name in [
            "xkb:us::eng",
            "mozc-jp",
            "nkb-sender",
            "nkb-send",
            "",
            "anthy",
        ] {
            assert!(!is_our_engine(name), "{name:?}");
        }
    }

    #[test]
    fn only_the_engine_of_the_person_is_put_back() {
        assert_eq!(engine_to_restore(Some("xkb:pl::pol")), Some("xkb:pl::pol"));
        assert_eq!(engine_to_restore(Some("mozc-jp")), Some("mozc-jp"));
        assert_eq!(engine_to_restore(None), None);
        assert_eq!(engine_to_restore(Some("")), None);
        assert_eq!(
            engine_to_restore(Some("nkb-send-9")),
            None,
            "the engine of a send that may be running is not the person's"
        );
    }

    #[test]
    fn a_text_is_cut_between_characters_and_in_order() {
        assert!(pieces("").is_empty(), "nothing to commit is no commit");
        assert_eq!(pieces("abc"), ["abc"]);
        let exact: String = "a".repeat(COMMIT_BYTES);
        assert_eq!(pieces(&exact), [exact.as_str()]);
        let longer = format!("{exact}b");
        assert_eq!(pieces(&longer), [exact.as_str(), "b"]);
    }

    #[test]
    fn a_piece_never_exceeds_the_bytes_a_message_may_carry() {
        // The limit is in bytes, so characters of two, three and four bytes cut sooner
        // than ASCII does, and a piece never splits one.
        let text: String = "\u{17c}\u{65e5}\u{1f600}"
            .chars()
            .cycle()
            .take(COMMIT_BYTES * 3)
            .collect();
        let cut = pieces(&text);
        assert!(
            cut.len() > 3,
            "multi-byte characters cut sooner: {}",
            cut.len()
        );
        for piece in &cut {
            assert!(piece.len() <= COMMIT_BYTES, "{} bytes", piece.len());
            assert!(!piece.is_empty());
        }
        assert_eq!(cut.concat(), text, "the pieces make the text, in order");
    }

    #[test]
    fn a_four_byte_character_on_the_cut_goes_whole_into_the_next_piece() {
        let mut text = "a".repeat(COMMIT_BYTES - 2);
        text.push('\u{1f600}');
        text.push('z');
        let cut = pieces(&text);
        assert_eq!(cut.len(), 2);
        assert_eq!(cut[0].len(), COMMIT_BYTES - 2);
        assert_eq!(cut[1], "\u{1f600}z");
    }

    #[test]
    fn a_long_value_is_given_the_time_its_pauses_take() {
        // The longest value the format allows is a million characters.
        let pieces = pieces(&"a".repeat(1_000_000)).len();
        let paused = COMMIT_GAP * u32::try_from(pieces).expect("a few hundred");
        assert!(deadline_for(pieces) > paused + DEADLINE, "{pieces} pieces");
        assert_eq!(deadline_for(0), DEADLINE);
        assert!(deadline_for(2) > deadline_for(1));
    }

    #[test]
    fn only_escape_on_its_own_is_escape() {
        // Plain, with the lock bits that do not change a key, pressed or let go.
        assert!(is_plain_escape(KEYSYM_ESCAPE, 0));
        assert!(is_plain_escape(KEYSYM_ESCAPE, IBUS_RELEASE_MASK));
        assert!(
            is_plain_escape(KEYSYM_ESCAPE, (1 << 1) | (1 << 4)),
            "Caps Lock and Num Lock"
        );
        // A modifier makes it a shortcut of the application, which is not ours to take.
        for bit in [0, 2, 3, 6, 26, 27, 28] {
            assert!(
                !is_plain_escape(KEYSYM_ESCAPE, 1 << bit),
                "modifier bit {bit}"
            );
        }
        // Another key is another key.
        assert!(!is_plain_escape(0x0061, 0));
        assert!(!is_plain_escape(0xff0d, 0));
        assert!(is_release(IBUS_RELEASE_MASK | 1));
        assert!(!is_release(1 << 2));
    }

    #[test]
    fn the_waits_are_ordered_so_that_a_slow_daemon_is_not_a_hung_one() {
        assert!(CALL_WAIT < DEADLINE);
        assert!(FOCUS_WAIT < DEADLINE);
        assert!(FOCUS_SETTLE < FOCUS_WAIT);
        assert!(SETTLE_AFTER_COMMIT < FOCUS_WAIT);
        assert!(FORGET_LOOK < FORGET_WAIT);
        assert!(ESCAPE_LOOK < COMMIT_GAP);
        assert!(FORGET_WAIT < DEADLINE);
    }
}
