//! What a send on X11 keeps about the spare keys it rewrites, decided without a server so
//! that it is tested on every system (`D157`).
//!
//! # Why
//!
//! A send types the characters the layout has no key for through spare keys of the
//! server, rewritten for the send and given back when it ends (`linux/send.rs`). Two things
//! went wrong with that:
//!
//! - two sends on one display at once. The second read the map while the first had a key
//!   rewritten, took that key for a key of the layout and typed through it, and the first
//!   gave it back under the second. And a key was given back whatever another program had
//!   put on it in the meantime,
//! - a send that ended without giving its keys back, killed or with the server stopped
//!   (`D156` point 4), left them rewritten until the server restarted.
//!
//! So one send at a time types on a display: a lock in the runtime folder of the user, one
//! per display, held for the whole send. Before a key is rewritten, the file of the lock gets
//! a line naming the key and what is written on it: the journal. A key is given back only
//! while it still carries what this program wrote on it, and the next send, once it holds
//! the lock, gives back what the journal names before it reads the map.
//!
//! # What a key carries
//!
//! A send writes the keysym twice on one level (`OBS-202` point 7), and the server answers
//! with every column of the key filled from it: `[ks, ks, ks, ks, 0, 0, 0]` on Xvfb 21.1
//! (measured 2026-10-27). So a key is still ours while one symbol at least is on it and every
//! symbol on it is ours.
//!
//! # What it cannot tell
//!
//! Another user on the same server has another runtime folder, so another lock and another
//! journal. A program that wrote the same keysym on the same key looks like this one. A server
//! restarted between two sends does not hold the key any more, so the journal names a key
//! that carries nothing, and nothing happens to it.

use core::time::Duration;

use crate::linux_rules::Keymap;

/// How long a send waits for another one on the same display to end: as long as a held
/// modifier is waited for. A send that holds the display longer is typing a value of its
/// own, and the person is told that rather than kept waiting for it.
pub(crate) const LOCK_WAIT: Duration = Duration::from_secs(2);

/// The most of the file of the lock that is read. The journal holds a line per spare key,
/// and a server has fewer than 256 keys, so a longer file was not written by a send.
pub(crate) const JOURNAL_LIMIT: u64 = 64 * 1024;

/// The length of a line of the journal: the keycode in three decimal digits, a space, the
/// keysym in eight hexadecimal digits and a newline. Every line is as long as every other,
/// so a journal written over from its start holds whole lines wherever the writing stopped.
pub(crate) const JOURNAL_LINE: usize = 13;

/// A spare key, and the keysym written on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Rewrite {
    pub keycode: u8,
    pub keysym: u32,
}

/// The runtime folder of the user that `XDG_RUNTIME_DIR` names, when it names one. Only an
/// absolute path is taken: a relative one would put the lock wherever the program was
/// started, and two sends started in two folders would not see each other.
pub(crate) fn runtime_folder(value: Option<&str>) -> Option<&str> {
    value.filter(|folder| folder.starts_with('/'))
}

/// The name of the file of the lock of display `number`. One per display, because two
/// servers have two maps.
pub(crate) fn lock_file_name(number: u32) -> String {
    format!("naughty-keyboard-x11-{number}.lock")
}

/// The line of the journal for `rewrite`.
pub(crate) fn journal_line(rewrite: Rewrite) -> String {
    format!("{:03} {:08x}\n", rewrite.keycode, rewrite.keysym)
}

/// The journal of `rewrites`, a line each.
pub(crate) fn journal_of(rewrites: &[Rewrite]) -> String {
    rewrites.iter().copied().map(journal_line).collect()
}

/// The rewrites a journal names, each once. A line this program does not write is passed
/// over: the file is in a folder of the user's, and a line that cannot be read names no key.
pub(crate) fn journal_entries(bytes: &[u8]) -> Vec<Rewrite> {
    let mut found: Vec<Rewrite> = bytes
        .split(|byte| *byte == b'\n')
        .filter_map(parsed_line)
        .collect();
    found.sort_unstable();
    found.dedup();
    found
}

/// One line of the journal, without its newline.
fn parsed_line(line: &[u8]) -> Option<Rewrite> {
    if line.len() != JOURNAL_LINE - 1 || line[3] != b' ' {
        return None;
    }
    let (keycode, keysym) = (&line[..3], &line[4..]);
    if !keycode.iter().all(u8::is_ascii_digit) || !keysym.iter().all(u8::is_ascii_hexdigit) {
        return None;
    }
    let keycode = core::str::from_utf8(keycode).ok()?.parse().ok()?;
    let keysym = u32::from_str_radix(core::str::from_utf8(keysym).ok()?, 16).ok()?;
    (keysym != 0).then_some(Rewrite { keycode, keysym })
}

/// Whether a key that `keysym` was written on still carries it and nothing else.
pub(crate) fn still_ours(row: &[u32], keysym: u32) -> bool {
    keysym != 0
        && row.contains(&keysym)
        && row.iter().all(|symbol| *symbol == 0 || *symbol == keysym)
}

/// The keys the server is asked about to give `rewrites` back: the first keycode and how
/// many, among the keycodes the server has. `None` when no rewrite names one of them.
pub(crate) fn span(rewrites: &[Rewrite], min_keycode: u8, max_keycode: u8) -> Option<(u8, u8)> {
    let inside = || {
        rewrites
            .iter()
            .map(|rewrite| rewrite.keycode)
            .filter(|keycode| (min_keycode..=max_keycode).contains(keycode))
    };
    let first = inside().min()?;
    let last = inside().max()?;
    let count = u8::try_from(usize::from(last) - usize::from(first) + 1).ok()?;
    Some((first, count))
}

/// The keys among `rewrites` that still carry what was written on them in `keymap`, the
/// map of the keys `span` named: the ones to give back, each once.
pub(crate) fn ours(keymap: &Keymap, rewrites: &[Rewrite]) -> Vec<u8> {
    let mut keys: Vec<u8> = rewrites
        .iter()
        .filter(|rewrite| still_ours(keymap.keysyms_of(rewrite.keycode), rewrite.keysym))
        .map(|rewrite| rewrite.keycode)
        .collect();
    keys.sort_unstable();
    keys.dedup();
    keys
}

#[cfg(test)]
mod tests {
    use super::*;

    const A_OGONEK: u32 = 0x0100_0105;
    const CJK: u32 = 0x0100_4E2D;

    fn rewrite(keycode: u8, keysym: u32) -> Rewrite {
        Rewrite { keycode, keysym }
    }

    #[test]
    fn a_line_of_the_journal_has_its_fixed_length_and_reads_back() {
        for written in [rewrite(8, A_OGONEK), rewrite(255, u32::MAX), rewrite(0, 1)] {
            let line = journal_line(written);
            assert_eq!(line.len(), JOURNAL_LINE, "{line:?}");
            assert_eq!(journal_entries(line.as_bytes()), vec![written]);
        }
        assert_eq!(journal_line(rewrite(8, A_OGONEK)), "008 01000105\n");
    }

    #[test]
    fn a_journal_is_read_whole_lines_only_and_each_key_once() {
        let mut journal = journal_of(&[rewrite(9, CJK), rewrite(8, A_OGONEK), rewrite(9, CJK)]);
        // A line cut short by a stop in the middle of a write, after the whole ones.
        journal.push_str("010 0100");
        assert_eq!(
            journal_entries(journal.as_bytes()),
            vec![rewrite(8, A_OGONEK), rewrite(9, CJK)]
        );
    }

    #[test]
    fn a_line_this_program_does_not_write_names_no_key() {
        for line in [
            "",
            "8 01000105",
            "0008 01000105",
            "008 1000105",
            "008  1000105",
            "008-01000105",
            "008 0100010g",
            "256 01000105",
            "+08 01000105",
            "008 +1000105",
            "008 00000000",
            "008 01000105\r",
            // Twelve bytes, a digit of another script among them.
            "\u{0660}8 01000105",
        ] {
            assert_eq!(journal_entries(line.as_bytes()), Vec::new(), "{line:?}");
        }
        assert_eq!(journal_entries(&[0xFF; 12]), Vec::new());
        assert_eq!(
            journal_entries(b"008 0100010A\n"),
            vec![rewrite(8, 0x0100_010A)],
            "both cases of a hexadecimal digit"
        );
    }

    #[test]
    fn a_key_is_ours_while_it_carries_our_keysym_and_nothing_else() {
        // What the server answers after the keysym was written twice (measured 2026-10-27).
        assert!(still_ours(
            &[A_OGONEK, A_OGONEK, A_OGONEK, A_OGONEK, 0, 0, 0],
            A_OGONEK
        ));
        assert!(still_ours(&[A_OGONEK, A_OGONEK], A_OGONEK));
        assert!(still_ours(&[0, A_OGONEK], A_OGONEK));
        // Given back already, or never rewritten.
        assert!(!still_ours(&[0, 0, 0, 0, 0, 0, 0], A_OGONEK));
        assert!(!still_ours(&[], A_OGONEK));
        // Another program wrote on it, all of it or a level of it.
        assert!(!still_ours(&[CJK, CJK, CJK, CJK, 0, 0, 0], A_OGONEK));
        assert!(!still_ours(&[A_OGONEK, CJK, A_OGONEK, A_OGONEK], A_OGONEK));
        // Nothing is ours on a key for the keysym of nothing.
        assert!(!still_ours(&[0, 0], 0));
    }

    #[test]
    fn the_span_covers_the_keys_named_inside_the_server_s_keycodes() {
        let rewrites = [rewrite(12, CJK), rewrite(9, A_OGONEK), rewrite(200, CJK)];
        assert_eq!(span(&rewrites, 8, 255), Some((9, 192)));
        assert_eq!(span(&rewrites, 10, 100), Some((12, 1)));
        assert_eq!(span(&rewrites, 13, 100), None);
        assert_eq!(span(&[], 8, 255), None);
        assert_eq!(span(&[rewrite(8, CJK)], 8, 8), Some((8, 1)));
        // Every keycode would be 256 keys, which no request can ask for.
        assert_eq!(span(&[rewrite(0, CJK), rewrite(255, CJK)], 0, 255), None);
    }

    #[test]
    fn only_the_keys_that_still_carry_their_keysym_are_given_back() {
        // Keys 9 to 12, four columns each, as the server answers for the span.
        let keymap = Keymap::new(
            9,
            4,
            vec![
                A_OGONEK, A_OGONEK, A_OGONEK, A_OGONEK, // 9: ours
                CJK, CJK, CJK, CJK, // 10: written over by another program
                0, 0, 0, 0, // 11: given back already
                CJK, CJK, 0, 0, // 12: ours, under its second keysym
            ],
        );
        let rewrites = [
            rewrite(9, A_OGONEK),
            rewrite(10, A_OGONEK),
            rewrite(11, A_OGONEK),
            rewrite(12, A_OGONEK),
            rewrite(12, CJK),
            rewrite(9, A_OGONEK),
            rewrite(40, A_OGONEK),
        ];
        assert_eq!(ours(&keymap, &rewrites), vec![9, 12]);
    }

    #[test]
    fn the_wait_and_the_limit_keep_to_what_they_say() {
        // As long as a held modifier is waited for, no longer.
        assert_eq!(LOCK_WAIT, crate::MODIFIER_RELEASE_WAIT);
        // A journal naming every keycode a server can have is read whole.
        let every_key: Vec<Rewrite> = (0..=u8::MAX).map(|keycode| rewrite(keycode, CJK)).collect();
        let journal = journal_of(&every_key);
        assert!(journal.len() as u64 <= JOURNAL_LIMIT, "{}", journal.len());
        assert_eq!(journal_entries(journal.as_bytes()), every_key);
    }

    #[test]
    fn the_lock_is_one_per_display_in_an_absolute_runtime_folder() {
        assert_eq!(lock_file_name(97), "naughty-keyboard-x11-97.lock");
        assert_ne!(lock_file_name(0), lock_file_name(1));
        assert_eq!(
            runtime_folder(Some("/run/user/1000")),
            Some("/run/user/1000")
        );
        for refused in [None, Some(""), Some("run/user/1000"), Some("~/run")] {
            assert_eq!(runtime_folder(refused), None, "{refused:?}");
        }
    }
}
