//! What the Linux route of `folders.rs` decides without a bus (`D175`), tested on
//! every system: the address of a folder as the desktop takes it and gives it back,
//! the window the folder window belongs to, and what each answer means.
//!
//! # Addresses
//!
//! The portal answers with `file://` addresses, and the file manager is asked with
//! them. A path on Linux is bytes, not text, so both directions work on bytes: a
//! byte that is not a letter, a digit, `-`, `.`, `_`, `~` or `/` is written as `%`
//! and two hex digits (RFC 3986 section 2.3, RFC 8089), whatever it is. An address
//! back from the desktop is taken only when it names a file of THIS machine (no
//! host, or `localhost`) and decodes to a full path without a zero - anything else
//! is not a folder this program can read.

use crate::folders::{PickError, ShowError};
use crate::linux_portal_rules::Answer;
use crate::portal::Unavailable;

/// The window the folder window belongs to, as the portal names a window: an X11
/// window in hexadecimal - the palette's windows are X11 windows on either kind of
/// session - or nothing, which the portal takes for "no window".
pub(crate) fn parent_window(owner: Option<u64>) -> String {
    owner.map_or_else(String::new, |window| format!("x11:{window:x}"))
}

/// Whether a byte stands for itself in an address.
const fn plain(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~' | b'/')
}

/// The `file://` address of the full path `path`, or `None` for a path that is not
/// full or holds a zero.
pub(crate) fn file_uri(path: &[u8]) -> Option<String> {
    if path.first() != Some(&b'/') || path.contains(&0) {
        return None;
    }
    let mut uri = String::from("file://");
    for &byte in path {
        if plain(byte) {
            uri.push(char::from(byte));
        } else {
            uri.push_str(&format!("%{byte:02X}"));
        }
    }
    Some(uri)
}

/// The value of one hex digit.
fn hex(digit: u8) -> Option<u8> {
    char::from(digit)
        .to_digit(16)
        .and_then(|value| u8::try_from(value).ok())
}

/// The path a `file://` address of this machine names, as bytes, or `None` for
/// any other address: another scheme, another host, a query or a fragment, an
/// escape that is not two hex digits, a path that is not full, a zero.
pub(crate) fn path_of_uri(uri: &str) -> Option<Vec<u8>> {
    let rest = uri.strip_prefix("file://")?;
    let slash = rest.find('/')?;
    let (host, path) = rest.split_at(slash);
    if !(host.is_empty() || host.eq_ignore_ascii_case("localhost")) {
        return None;
    }
    if path.contains(['?', '#']) {
        return None;
    }
    let mut bytes = Vec::with_capacity(path.len());
    let mut raw = path.bytes();
    while let Some(byte) = raw.next() {
        if byte == b'%' {
            let high = hex(raw.next()?)?;
            let low = hex(raw.next()?)?;
            bytes.push(high * 16 + low);
        } else {
            bytes.push(byte);
        }
    }
    (bytes.first() == Some(&b'/') && !bytes.contains(&0)).then_some(bytes)
}

/// The folder the window opens in, as the portal takes it: the path's bytes with a
/// closing zero. `None` for a path that is not full or holds a zero.
pub(crate) fn current_folder(path: &[u8]) -> Option<Vec<u8>> {
    (path.first() == Some(&b'/') && !path.contains(&0))
        .then(|| path.iter().copied().chain(Some(0)).collect())
}

/// What a portal that could not be asked means for the folder window. A desktop
/// without the portal, or without a bus to ask, has no folder window - which the
/// window says as such, with the other ways to set the folder.
pub(crate) fn pick_refusal(why: Unavailable) -> PickError {
    match why {
        Unavailable::NotHere
        | Unavailable::NoBus
        | Unavailable::NoPortal
        | Unavailable::NeedsEntry => PickError::NoWindow,
        Unavailable::Unreachable => {
            PickError::Failed(String::from("the session bus did not answer"))
        }
        Unavailable::NoAnswer => {
            PickError::Failed(String::from("the folder window gave no answer in time"))
        }
        Unavailable::Ended(code) => {
            PickError::Failed(format!("the folder window ended with code {code}"))
        }
        Unavailable::Cancelled | Unavailable::Odd | Unavailable::BadWish => PickError::Failed(
            String::from("the folder window answered in a way its specification does not allow"),
        ),
    }
}

/// What a file manager that could not be asked means. No bus, or no file manager
/// on the bus, is a desktop without one. An error from the bus or the file manager -
/// one that would not start, among others - is said as an error, never as a
/// refusal of the folder, which nothing here could know.
pub(crate) fn show_refusal(why: Unavailable) -> ShowError {
    match why {
        Unavailable::NotHere
        | Unavailable::NoBus
        | Unavailable::NoPortal
        | Unavailable::NeedsEntry => ShowError::NoFileManager,
        Unavailable::Unreachable | Unavailable::NoAnswer => {
            ShowError::Failed(String::from("the file manager did not answer"))
        }
        Unavailable::Ended(_)
        | Unavailable::Cancelled
        | Unavailable::Odd
        | Unavailable::BadWish => {
            ShowError::Failed(String::from("the file manager answered with an error"))
        }
    }
}

/// What the answer of the portal comes to: the folder in its first address when
/// the person chose one, a cancel when they closed the window.
pub(crate) fn picked(answer: Answer, uris: &[String]) -> Result<Option<Vec<u8>>, PickError> {
    match answer {
        Answer::Cancelled => Ok(None),
        Answer::Ended(code) => Err(PickError::Failed(format!(
            "the folder window ended with code {code}"
        ))),
        Answer::Granted => uris
            .first()
            .and_then(|uri| path_of_uri(uri))
            .map(Some)
            .ok_or_else(|| {
                PickError::Failed(String::from(
                    "the folder window gave no folder of this machine",
                ))
            }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_goes_to_the_desktop_with_every_other_byte_escaped() {
        assert_eq!(
            file_uri(b"/home/t/Team packs/a,b%~"),
            Some(String::from("file:///home/t/Team%20packs/a%2Cb%25~"))
        );
        assert_eq!(
            file_uri("/home/t/za\u{17c}\u{f3}\u{142}\u{107}".as_bytes()),
            Some(String::from("file:///home/t/za%C5%BC%C3%B3%C5%82%C4%87"))
        );
        assert_eq!(
            file_uri(b"/home/t/\xff\n"),
            Some(String::from("file:///home/t/%FF%0A"))
        );
        assert_eq!(file_uri(b"relative/path"), None);
        assert_eq!(file_uri(b"/a\0b"), None);
    }

    #[test]
    fn an_address_comes_back_as_the_bytes_it_was_made_from() {
        for path in [
            &b"/home/t/Team packs/a,b%~"[..],
            "/home/t/za\u{17c}\u{f3}\u{142}\u{107} \u{6f22}\u{5b57}".as_bytes(),
            b"/home/t/\xff\x01",
            b"/",
        ] {
            let uri = file_uri(path).unwrap_or_default();
            assert_eq!(path_of_uri(&uri).as_deref(), Some(path), "{uri}");
        }
        assert_eq!(
            path_of_uri("file://localhost/srv/packs").as_deref(),
            Some(&b"/srv/packs"[..])
        );
    }

    #[test]
    fn an_address_that_is_not_a_folder_of_this_machine_is_refused() {
        for uri in [
            "file://server/srv/packs",
            "smb://server/packs",
            "file:///srv/packs?x=1",
            "file:///srv/packs#top",
            "file:///srv/%zz",
            "file:///srv/%4",
            "file:///srv/a%00b",
            "file://",
            "file:relative",
        ] {
            assert_eq!(path_of_uri(uri), None, "{uri}");
        }
    }

    #[test]
    fn the_window_belongs_to_the_palette_window_in_hexadecimal() {
        assert_eq!(parent_window(Some(0x3a0_0007)), "x11:3a00007");
        assert_eq!(parent_window(None), "");
    }

    #[test]
    fn the_folder_to_open_in_closes_with_a_zero() {
        assert_eq!(current_folder(b"/srv"), Some(b"/srv\0".to_vec()));
        assert_eq!(current_folder(b"srv"), None);
        assert_eq!(current_folder(b"/s\0rv"), None);
    }

    #[test]
    fn a_desktop_without_the_portal_has_no_folder_window_and_a_dead_bus_is_a_failure() {
        assert_eq!(pick_refusal(Unavailable::NoPortal), PickError::NoWindow);
        assert_eq!(pick_refusal(Unavailable::NoBus), PickError::NoWindow);
        assert!(matches!(
            pick_refusal(Unavailable::Unreachable),
            PickError::Failed(_)
        ));
        assert_eq!(
            show_refusal(Unavailable::NoPortal),
            ShowError::NoFileManager
        );
        assert!(matches!(
            show_refusal(Unavailable::Unreachable),
            ShowError::Failed(_)
        ));
    }

    #[test]
    fn the_answer_is_the_first_folder_or_a_cancel() {
        let uris = vec![String::from("file:///srv/a"), String::from("file:///srv/b")];
        assert_eq!(picked(Answer::Granted, &uris), Ok(Some(b"/srv/a".to_vec())));
        assert_eq!(picked(Answer::Cancelled, &uris), Ok(None));
        assert!(matches!(
            picked(Answer::Ended(2), &uris),
            Err(PickError::Failed(_))
        ));
        assert!(matches!(
            picked(Answer::Granted, &[]),
            Err(PickError::Failed(_))
        ));
        assert!(matches!(
            picked(Answer::Granted, &[String::from("file://far/srv")]),
            Err(PickError::Failed(_))
        ));
    }
}
