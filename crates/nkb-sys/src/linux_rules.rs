//! The decisions of the Linux route that need no X server (`D145`, `OBS-202`).
//!
//! Typing on X11 is a recipe, and every number in it was measured (2026-10-20,
//! Xvfb 21.1 with openbox, a winit and a GTK4 victim, `tools/sonda-x11`):
//!
//! - a character the layout types goes through its own key, with Shift held where
//!   it takes one, but only while the state is plain and the layout group is one
//!   of the first two, because a key of the layout types what the state says
//!   (`Hello` came out as `hELLO` and as `Руддщ`, point 6). The page or program
//!   then sees what a person's keyboard gives it (`OBS-212`). Every other character
//!   goes through a SPARE key whose row was rewritten to the Unicode form of the
//!   keysym, written twice, because a keysym alone on a key is case-converted by
//!   the server (point 7),
//! - the first XTest key event of a connection wipes a remap made before it, so
//!   one tap of an unmapped key comes first (point 1),
//! - a remap before every character loses 30 to 80 percent, a batch with a pause
//!   loses 8 percent at 50 ms, and only the answer of the application to
//!   `_NET_WM_PING` after every batch lost nothing in seven of seven runs (point 3),
//! - there are 18 spare keys on the servers measured, so a batch holds at most 18
//!   different characters (point 5).
//!
//! The system is `linux/`. Everything that is only a decision about numbers,
//! names and text lives here and is tested on every system.

use core::time::Duration;
use std::collections::HashMap;

/// Where the X server of this machine keeps its sockets for local clients.
pub(crate) const SOCKET_DIR: &str = "/tmp/.X11-unix";

/// The most characters one batch holds before the application is asked whether it
/// has taken them. Measured with 240 different CJK characters in a dozen batches
/// (`OBS-202` point 3). It is a ceiling: how long a batch is depends on how fast the
/// application was seen to take the one before (`next_limit`).
pub(crate) const MAX_CHARS_PER_ACK: usize = 256;

/// The characters of the first batch, before any pace is known. A fast application
/// answers 64 characters in well under a second, and a slow one in far less than the
/// wait for the answer.
pub(crate) const FIRST_BATCH_CHARS: usize = 64;

/// The fewest characters a batch is cut to, however slow the application is.
pub(crate) const MIN_BATCH_CHARS: usize = 8;

/// How long an answer is hoped to take. A batch is cut to the characters that fit
/// this at the pace just seen, so an application that is slow but alive - 20 ms a
/// character in a long text field, measured on `nkb-target` - is never mistaken for
/// one that has stopped, which is waited for `PING_WAIT` and no longer.
pub(crate) const ANSWER_TARGET: Duration = Duration::from_secs(1);

/// How long the application is given to answer `_NET_WM_PING`. A stopped
/// application is waited for exactly this long (`OBS-202` point 9), and the batch it
/// did not answer is lost when it runs again, because the layout is given back by
/// then (`OBS-206`).
pub(crate) const PING_WAIT: Duration = Duration::from_secs(5);

/// The pause after a remap, before the keys that use it. The clients are told of
/// the new map before they are told of the keys, so this is a hedge for the ones
/// that read it late, not what the recipe stands on.
pub(crate) const REMAP_SETTLE: Duration = Duration::from_millis(50);

/// The pause after the tap that makes the server announce its map, before the
/// first remap (`OBS-202` point 1).
pub(crate) const WARMUP_SETTLE: Duration = Duration::from_millis(100);

/// The pause after a batch to an application that does not take part in
/// `_NET_WM_PING`. Longer than the 50 ms that still lost 8 percent: this send is
/// reported as unpaced and the person is told that characters may be missing.
pub(crate) const UNPACED_SETTLE: Duration = Duration::from_millis(150);

/// How long a wait for a held key to come up pauses between two looks.
pub(crate) const LOOK_EVERY: Duration = Duration::from_millis(10);

/// The event mask the answers to a ping are listened for with, on the root window.
/// EWMH asks for an answer sent with `SubstructureNotify` and `SubstructureRedirect`,
/// which GTK and winit do. Qt sends it with `StructureNotify` and
/// `SubstructureRedirect`, so a listener on `SubstructureNotify` alone never hears
/// it and a Qt application looks stopped (measured with Qt 6.10 on Xorg). The
/// redirect bit is the window manager's, so both notify bits are listened for.
pub(crate) const PING_ANSWER_MASK: u32 = STRUCTURE_NOTIFY | SUBSTRUCTURE_NOTIFY;

/// The two event mask bits of the X11 protocol that a ping answer can arrive on.
pub(crate) const STRUCTURE_NOTIFY: u32 = 0x0002_0000;
pub(crate) const SUBSTRUCTURE_NOTIFY: u32 = 0x0008_0000;

/// The keysyms that are not characters in the Unicode form.
pub(crate) const KEYSYM_RETURN: u32 = 0xff0d;
pub(crate) const KEYSYM_TAB: u32 = 0xff09;
pub(crate) const KEYSYM_ESCAPE: u32 = 0xff1b;

/// The keysym of a Unicode character is `0x0100_0000` plus its code point.
const UNICODE_KEYSYM_BASE: u32 = 0x0100_0000;

/// The Unicode form is defined from U+0100 up. Below it a character has its Latin-1
/// keysym, and an X server never hands out the Unicode form of a Latin-1 one.
const UNICODE_KEYSYM_FIRST: u32 = 0x0100_0100;
const UNICODE_KEYSYM_LAST: u32 = 0x0110_FFFF;

/// The `Lock` bit of the modifier state, which Caps Lock sets.
const LOCK_MASK: u16 = 0x02;

/// The display a program may type into: a local one, by number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DisplayAddress {
    pub number: u32,
    pub screen: usize,
}

/// Why `DISPLAY` does not name an X server this program may talk to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DisplayRefusal {
    /// Not set, or empty.
    Unset,
    /// Names a host, so the connection would leave the machine.
    NotLocal,
    /// Not of the form `:N`, `:N.S` or `unix:N`.
    Malformed,
}

/// Reads a `DISPLAY` value, accepting only the local forms `:N`, `:N.S` and
/// `unix:N` (`D145`).
///
/// Anything that names a host is refused BEFORE a connection is made, because
/// following it would send keystrokes over TCP to another machine - the program
/// that promises it sends nothing off the machine (untouchable rule 17).
pub(crate) fn parse_display(value: Option<&str>) -> Result<DisplayAddress, DisplayRefusal> {
    let value = value
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .ok_or(DisplayRefusal::Unset)?;
    let (host, rest) = value.split_once(':').ok_or(DisplayRefusal::Malformed)?;
    if !host.is_empty() && host != "unix" {
        return Err(DisplayRefusal::NotLocal);
    }
    let (number, screen) = match rest.split_once('.') {
        Some((number, screen)) => (number, screen),
        None => (rest, "0"),
    };
    Ok(DisplayAddress {
        number: digits(number, 5).ok_or(DisplayRefusal::Malformed)?,
        screen: digits(screen, 3).ok_or(DisplayRefusal::Malformed)? as usize,
    })
}

/// The number written in `text`: ASCII digits only, one to `longest` of them. No
/// sign, no space, nothing a parser might read more generously than a human.
fn digits(text: &str, longest: usize) -> Option<u32> {
    if text.is_empty() || text.len() > longest || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

/// The socket a local client connects to for this display.
pub(crate) fn socket_path(address: DisplayAddress) -> String {
    format!("{SOCKET_DIR}/X{}", address.number)
}

/// Whether the session says it is Wayland: the type the login set, or a
/// compositor socket the environment points at.
pub(crate) fn session_is_wayland(
    session_type: Option<&str>,
    wayland_display: Option<&str>,
) -> bool {
    session_type.is_some_and(|value| value.trim().eq_ignore_ascii_case("wayland"))
        || wayland_display.is_some_and(|value| !value.trim().is_empty())
}

/// What the environment says about where this program runs.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Environment<'a> {
    pub display: Option<&'a str>,
    pub session_type: Option<&'a str>,
    pub wayland_display: Option<&'a str>,
}

/// Why this program does not type on this machine, in the words the person reads
/// after `on` (`Unsupported { system }`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Refusal {
    /// A Wayland session: the X server there is XWayland, and through it only the
    /// characters of the compositor's layout arrive (`OBS-203`).
    Wayland,
    /// No local X server to type through.
    NoLocalServer,
    /// The server has no XTEST extension, the only way to inject key events.
    NoXtest,
    /// Every key of the server has a symbol, so there is none to type through.
    NoSpareKeys,
}

impl Refusal {
    /// The system as the refusal names it.
    pub(crate) const fn system(self) -> &'static str {
        match self {
            Self::Wayland => "Linux under Wayland",
            Self::NoLocalServer => "Linux without a local X server",
            Self::NoXtest => "Linux on an X server without the XTEST extension",
            Self::NoSpareKeys => "Linux on an X server with no spare keys to type through",
        }
    }
}

/// Whether the environment allows typing, and into which display. Wayland is
/// asked first: under it `DISPLAY` is set too, and it is XWayland.
pub(crate) fn route(environment: &Environment<'_>) -> Result<DisplayAddress, Refusal> {
    if session_is_wayland(environment.session_type, environment.wayland_display) {
        return Err(Refusal::Wayland);
    }
    parse_display(environment.display).map_err(|_| Refusal::NoLocalServer)
}

/// A character no key of an X server can type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CannotCarry(pub char);

/// The keysym that types `character`.
///
/// Line feed and tab are the keys of those names. Every other character is its
/// Unicode keysym, ASCII included (`OBS-202` point 6). `U+0000` is no keysym at
/// all, and refusing it by name is the alternative to sending nothing for it.
pub(crate) fn keysym_for(character: char) -> Result<u32, CannotCarry> {
    match character {
        '\0' => Err(CannotCarry(character)),
        '\n' => Ok(KEYSYM_RETURN),
        '\t' => Ok(KEYSYM_TAB),
        other => Ok(UNICODE_KEYSYM_BASE + u32::from(other)),
    }
}

/// UTF-16 units in `characters`, the unit every route reports in.
pub(crate) fn utf16_units(characters: &[char]) -> usize {
    characters
        .iter()
        .map(|character| character.len_utf16())
        .sum()
}

/// A key of the layout and whether Shift is held to reach the character.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LayoutKey {
    pub keycode: u8,
    pub shift: bool,
}

/// The keys of the layout that characters are typed through, when it has them.
///
/// A character the layout types is pressed as its own key, because that is what a
/// page or a program sees from a person: Chromium named the key of every character
/// typed through a spare key NUL, with an empty code and a key code of 0, against
/// 237 of 237 right for the keys of the layout, and Firefox gave the right letter but
/// the wrong code and one extra event (`OBS-212`). The keys of the layout are not
/// rewritten, so there is no race with an application that reads them late and no
/// wait for it (`OBS-210`), and an input method that hands every key on slowly
/// (xterm through IBus lost the end of 9 of 47 values) has nothing to lose.
///
/// An application that tells Return and Tab apart from every other key by the key
/// itself does nothing for them on a spare key: Chromium takes Enter from the
/// physical key through a table of its own, and a line feed typed through a spare
/// key reached its text area as nothing, 0 of 3 values with a new line or a tab
/// (`OBS-211`). These two go through the key of the layout in every state.
///
/// The printable characters do only when the state is plain (`layout_is_plain`) and
/// the layout group is one of the first two (`usable_group`), because the key of a
/// layout types what the state says and not what was meant: Caps Lock turned
/// `Hello` into `hELLO` and the second group into `Руддщ` (`OBS-202` point 6). A
/// spare key with its keysym on both levels does not depend on either, and is what
/// every other character goes through.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct LayoutKeys {
    return_key: Option<u8>,
    tab_key: Option<u8>,
    shift_key: Option<u8>,
    characters: HashMap<u32, LayoutKey>,
}

impl LayoutKeys {
    /// Return and Tab only.
    pub(crate) fn from_keymap(keymap: &Keymap) -> Self {
        Self {
            return_key: keymap.find_keycode(KEYSYM_RETURN),
            tab_key: keymap.find_keycode(KEYSYM_TAB),
            ..Self::default()
        }
    }

    /// Return and Tab, and the printable characters of the layout when `state` is
    /// plain and has a group to read. `shift_key` is the key the server counts as
    /// Shift, and without one only the characters that need no Shift are kept.
    pub(crate) fn typing(keymap: &Keymap, shift_key: Option<u8>, state: u16) -> Self {
        let mut layout = Self::from_keymap(keymap);
        let Some(group) = usable_group(state).filter(|_| layout_is_plain(state)) else {
            return layout;
        };
        layout.shift_key = shift_key;
        for (keysym, key) in keymap.character_keys(group) {
            if !key.shift || shift_key.is_some() {
                layout.characters.entry(keysym).or_insert(key);
            }
        }
        layout
    }

    /// The key of the layout that types `keysym`, if it has one to use.
    pub(crate) fn key_for(&self, keysym: u32) -> Option<LayoutKey> {
        let plain = |keycode: Option<u8>| {
            keycode.map(|keycode| LayoutKey {
                keycode,
                shift: false,
            })
        };
        match keysym {
            KEYSYM_RETURN => plain(self.return_key),
            KEYSYM_TAB => plain(self.tab_key),
            _ => self.characters.get(&keysym).copied(),
        }
    }

    /// The key that is held down for the characters that need Shift.
    pub(crate) fn shift_key(&self) -> Option<u8> {
        self.shift_key
    }
}

/// The modifiers of an X state that change what a key of the layout types: Shift,
/// Lock, Control, Mod1, Mod3, Mod4 and Mod5. Mod2 is Num Lock, which only the
/// keypad answers to, and the button bits are not modifiers.
const TYPING_MODIFIERS: u16 = 0x00EF;

/// The keyboard group lives in bits 13 and 14 of the state the server reports.
const GROUP_SHIFT: u16 = 13;

/// Whether no modifier is down, latched or locked that would change a character
/// typed through the layout's own key.
pub(crate) fn layout_is_plain(state: u16) -> bool {
    state & TYPING_MODIFIERS == 0
}

/// The layout group in effect, when it is one of the first two. The core map has
/// two columns for each of them (the first four columns) and the columns after
/// those belong to higher levels, so a third group cannot be told from a third
/// level.
pub(crate) fn usable_group(state: u16) -> Option<usize> {
    let group = usize::from((state >> GROUP_SHIFT) & 0x3);
    (group <= 1).then_some(group)
}

/// The keysym of a character in the form `keysym_for` gives, when `keysym` is one:
/// the Latin-1 keysyms are their own code points, and the Unicode form starts above
/// them. Function keys, dead keys, the keypad and the legacy keysyms of other
/// scripts are none, and are typed through a spare key.
fn character_keysym(keysym: u32) -> Option<u32> {
    let character = match keysym {
        0x20..=0x7E | 0xA0..=0xFF => keysym,
        UNICODE_KEYSYM_FIRST..=UNICODE_KEYSYM_LAST => keysym - UNICODE_KEYSYM_BASE,
        _ => return None,
    };
    char::from_u32(character)?;
    Some(UNICODE_KEYSYM_BASE + character)
}

/// A run of characters typed between two answers of the application, and the
/// different keysyms they need, in the order they first appear. The keysym at
/// index `n` goes on the `n`-th spare key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Batch {
    /// Index of the first character.
    pub first: usize,
    /// Index after the last character.
    pub end: usize,
    pub keysyms: Vec<u32>,
}

impl Batch {
    /// The spare key, by its index in the batch, that types `keysym`.
    pub(crate) fn slot(&self, keysym: u32) -> Option<usize> {
        self.keysyms.iter().position(|assigned| *assigned == keysym)
    }
}

/// The batch that starts at `first`: as many characters as fit `spare` keys and
/// `max_chars`, in order. A batch always holds at least one character, so a send
/// that is slow cannot stand still.
///
/// `first` must be inside `keysyms` and `spare` at least one - the caller refuses a
/// server with no spare key before it begins.
pub(crate) fn next_batch(
    keysyms: &[u32],
    first: usize,
    spare: usize,
    max_chars: usize,
    layout: &LayoutKeys,
) -> Batch {
    let max_chars = max_chars.max(1);
    let mut assigned: Vec<u32> = Vec::new();
    let mut end = first;
    while end < keysyms.len() && end - first < max_chars {
        let keysym = keysyms[end];
        if layout.key_for(keysym).is_none() && !assigned.contains(&keysym) {
            if assigned.len() == spare {
                break;
            }
            assigned.push(keysym);
        }
        end += 1;
    }
    Batch {
        first,
        end,
        keysyms: assigned,
    }
}

/// The different keysyms of a whole value, each once, in the order they first
/// appear: the keysym at index `n` goes on the `n`-th spare key for the whole send.
/// `None` when the value has more of them than there are spare keys, and then the
/// keys are rewritten between batches.
///
/// A key that is rewritten while an application has not yet read the keys sent
/// through it would type the new character where the old one was meant, so a value
/// that fits is assigned once and the keys are not touched again until the send is
/// over (`OBS-210`).
pub(crate) fn slots_for_value(
    keysyms: &[u32],
    spare: usize,
    layout: &LayoutKeys,
) -> Option<Vec<u32>> {
    let mut slots: Vec<u32> = Vec::new();
    for keysym in keysyms {
        if layout.key_for(*keysym).is_some() {
            continue;
        }
        if !slots.contains(keysym) {
            if slots.len() == spare {
                return None;
            }
            slots.push(*keysym);
        }
    }
    Some(slots)
}

/// The batch that starts at `first` of a value whose keys are all assigned already:
/// `max_chars` characters, or what is left. It carries every slot, because they all
/// stay assigned, and a batch holds at least one character.
pub(crate) fn batch_of_value(
    slots: &[u32],
    first: usize,
    length: usize,
    max_chars: usize,
) -> Batch {
    Batch {
        first,
        end: first.saturating_add(max_chars.max(1)).min(length),
        keysyms: slots.to_vec(),
    }
}

/// How long the layout is kept after keys were sent, so that an application that
/// has answered the ping but has not yet turned the keys into characters still
/// finds them where they were. An application reads a key with the map it has at
/// that moment, and some do it after the answer: Chromium answered a ping within
/// milliseconds and put the characters into the field up to 100 ms after, so a
/// layout given back at once left 0 of 9 characters in 6 runs of 6, and after
/// 50 ms or more 16 runs of 16 were whole (`OBS-210`).
///
/// The wait grows with the keys in the batch because an application with a
/// handler per key needs time per key. The 4 ms for each is a bound set between
/// what was measured and what a long batch could cost, not a measurement of its
/// own, and the ceiling keeps a long batch from holding the send for seconds.
pub(crate) const GIVE_BACK_BASE: Duration = Duration::from_millis(100);
pub(crate) const GIVE_BACK_PER_KEY: Duration = Duration::from_millis(4);
pub(crate) const GIVE_BACK_CEILING: Duration = Duration::from_millis(3000);

/// The wait for each key when the session names an input method in `XMODIFIERS`.
/// An application that goes through the X input method protocol hands every key to
/// the input method and has it back before it reads the next, and that took about
/// 22 ms a key with IBus on xterm: nine keys needed 200 ms and lost one to three
/// of them with none, and 12 ms a key still lost the end of values of 13 keys
/// (`OBS-211`). It is paid by every application of such a session, XIM or not,
/// and the ceiling cuts it, so a value of more than about 100 characters can
/// still lose its end in an XIM application.
pub(crate) const GIVE_BACK_PER_KEY_INPUT_METHOD: Duration = Duration::from_millis(25);

/// Whether `XMODIFIERS` names an input method: `@im=ibus`, `@im=fcitx`, and not
/// `@im=none`, which is how a session says it has none. Applications that use the
/// X input method protocol find their server by this variable, so a session
/// without it has none of the applications that the longer wait is for.
pub(crate) fn names_an_input_method(xmodifiers: Option<&str>) -> bool {
    xmodifiers
        .into_iter()
        .flat_map(|value| value.split('@'))
        .filter_map(|part| part.strip_prefix("im="))
        .any(|name| !name.is_empty() && name != "none")
}

/// The wait before the layout is given back, or a key is rewritten, after a batch
/// of `keys` characters that the application has answered.
pub(crate) fn give_back_wait(keys: usize, input_method: bool) -> Duration {
    let per_key = if input_method {
        GIVE_BACK_PER_KEY_INPUT_METHOD
    } else {
        GIVE_BACK_PER_KEY
    };
    let per_keys = per_key.saturating_mul(u32::try_from(keys).unwrap_or(u32::MAX));
    GIVE_BACK_BASE
        .saturating_add(per_keys)
        .min(GIVE_BACK_CEILING)
}

/// How many characters the next batch may hold, from how long the application took
/// to answer for the last one.
pub(crate) fn next_limit(characters: usize, answered_in: Duration) -> usize {
    let per_character = answered_in.as_secs_f64() / characters.max(1) as f64;
    if per_character <= 0.0 {
        return MAX_CHARS_PER_ACK;
    }
    let fit = (ANSWER_TARGET.as_secs_f64() / per_character).min(MAX_CHARS_PER_ACK as f64);
    (fit as usize).clamp(MIN_BATCH_CHARS, MAX_CHARS_PER_ACK)
}

/// The keyboard map of the server, as `GetKeyboardMapping` returns it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Keymap {
    min_keycode: u8,
    per_keycode: usize,
    keysyms: Vec<u32>,
}

impl Keymap {
    pub(crate) fn new(min_keycode: u8, per_keycode: usize, keysyms: Vec<u32>) -> Self {
        Self {
            min_keycode,
            per_keycode,
            keysyms,
        }
    }

    fn rows(&self) -> usize {
        self.keysyms.len() / self.per_keycode.max(1)
    }

    fn row(&self, index: usize) -> &[u32] {
        let start = index * self.per_keycode;
        &self.keysyms[start..start + self.per_keycode]
    }

    fn keycode(&self, index: usize) -> Option<u8> {
        u8::try_from(usize::from(self.min_keycode) + index).ok()
    }

    /// The keys with no symbol on any level: the ones to type through. Keys that
    /// have any symbol are never touched - changing one replaces its definition
    /// and breaks the layout (`OBS-202` point 5).
    pub(crate) fn spare_keycodes(&self) -> Vec<u8> {
        (0..self.rows())
            .filter(|index| self.row(*index).iter().all(|keysym| *keysym == 0))
            .filter_map(|index| self.keycode(index))
            .collect()
    }

    /// The first key that types `keysym` on its first level.
    pub(crate) fn find_keycode(&self, keysym: u32) -> Option<u8> {
        (0..self.rows())
            .find(|index| self.row(*index).first() == Some(&keysym))
            .and_then(|index| self.keycode(index))
    }

    /// The characters the keys type in `group` on their first two levels, each with
    /// the key that types it. The ones that need no Shift come first, so a character
    /// on two keys is pressed through the one that needs fewer events.
    pub(crate) fn character_keys(&self, group: usize) -> Vec<(u32, LayoutKey)> {
        let mut found = Vec::new();
        for shift in [false, true] {
            let column = 2 * group + usize::from(shift);
            for index in 0..self.rows() {
                let Some(keycode) = self.keycode(index) else {
                    continue;
                };
                let keysym = self
                    .row(index)
                    .get(column)
                    .copied()
                    .and_then(character_keysym);
                if let Some(keysym) = keysym {
                    found.push((keysym, LayoutKey { keycode, shift }));
                }
            }
        }
        found
    }
}

/// The window the keyboard focus is in, if it is in one. `None` and
/// `PointerRoot` (0 and 1) name no window, and the root window is where the
/// focus rests when nothing has it.
pub(crate) fn focus_window(raw: u32, root: u32) -> Option<u32> {
    (raw > 1 && raw != root).then_some(raw)
}

/// The name of a modifier whose being down changes what a typed character means,
/// by its index in the server's modifier map (`Shift`, `Lock`, `Control`, `Mod1`
/// to `Mod5`). The two lock-like ones are left out: `Lock` is Caps Lock, which has
/// its own refusal, and `Mod2` is Num Lock, and neither changes a character typed
/// through the Unicode keysym of a spare key. `Mod3` is left out because some
/// layouts keep Scroll Lock there.
pub(crate) const fn modifier_name(index: usize) -> Option<&'static str> {
    match index {
        0 => Some("Shift"),
        2 => Some("Ctrl"),
        3 => Some("Alt"),
        6 => Some("Super"),
        7 => Some("AltGr"),
        _ => None,
    }
}

/// Whether `keycode` is down in a 32-byte `QueryKeymap` reply.
pub(crate) fn key_is_down(keys: &[u8], keycode: u8) -> bool {
    keys.get(usize::from(keycode / 8))
        .is_some_and(|byte| byte & (1 << (keycode % 8)) != 0)
}

/// The first modifier of `modifier_name` that has a key down, given the
/// `QueryKeymap` bits and the server's modifier map (`keycodes_per_modifier` keys
/// for each of the eight modifiers, zero where there is none).
pub(crate) fn held_modifier(
    keys: &[u8],
    mapping: &[u8],
    keycodes_per_modifier: usize,
) -> Option<&'static str> {
    if keycodes_per_modifier == 0 {
        return None;
    }
    mapping
        .chunks(keycodes_per_modifier)
        .enumerate()
        .find_map(|(index, row)| {
            let name = modifier_name(index)?;
            row.iter()
                .any(|keycode| *keycode != 0 && key_is_down(keys, *keycode))
                .then_some(name)
        })
}

/// Whether Caps Lock is on, from the modifier state the server reports.
pub(crate) fn caps_lock_on(state: u16) -> bool {
    state & LOCK_MASK != 0
}

/// A token for one `_NET_WM_PING`, different for every ping of one send and made
/// different between programs by the process id, so two sends to one application
/// do not take each other's answers.
pub(crate) fn ping_token(process: u32, counter: u32) -> u32 {
    process.wrapping_mul(0x9E37_79B1) ^ counter
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;

    fn display(value: &str) -> Result<DisplayAddress, DisplayRefusal> {
        parse_display(Some(value))
    }

    #[test]
    fn the_local_forms_of_display_are_read() {
        let at = |number, screen| Ok(DisplayAddress { number, screen });
        assert_eq!(display(":0"), at(0, 0));
        assert_eq!(display(":99"), at(99, 0));
        assert_eq!(display(":1.2"), at(1, 2));
        assert_eq!(display("unix:3"), at(3, 0));
        assert_eq!(display("  :0 "), at(0, 0), "a stray space is not a host");
    }

    #[test]
    fn a_display_that_names_a_host_is_refused_before_any_connection() {
        // Untouchable rule 17: nothing leaves the machine. Each of these would be
        // a TCP connection for the library's own `connect`.
        for host in [
            "localhost:0",
            "192.168.1.5:0",
            "myhost:10.0",
            "[::1]:0",
            "tcp/localhost:0",
        ] {
            assert_eq!(display(host), Err(DisplayRefusal::NotLocal), "{host}");
        }
    }

    #[test]
    fn a_display_that_is_not_a_display_is_refused() {
        assert_eq!(parse_display(None), Err(DisplayRefusal::Unset));
        assert_eq!(display(""), Err(DisplayRefusal::Unset));
        assert_eq!(display("   "), Err(DisplayRefusal::Unset));
        for bad in [
            "0", ":", ":x", ":-1", ":+1", ":1.", ":.1", ":1.x", ":0:0", ":123456", ":1.1234", ": 1",
        ] {
            assert_eq!(display(bad), Err(DisplayRefusal::Malformed), "{bad:?}");
        }
    }

    #[test]
    fn the_socket_is_built_from_the_number_and_nothing_else() {
        let address = display(":12.1").expect("a display");
        assert_eq!(socket_path(address), "/tmp/.X11-unix/X12");
    }

    #[test]
    fn a_wayland_session_is_told_by_its_type_or_by_its_socket() {
        assert!(session_is_wayland(Some("wayland"), None));
        assert!(session_is_wayland(Some(" Wayland "), None));
        assert!(session_is_wayland(None, Some("wayland-0")));
        assert!(session_is_wayland(Some("x11"), Some("wayland-0")));
        assert!(!session_is_wayland(Some("x11"), None));
        assert!(!session_is_wayland(Some("tty"), Some("  ")));
        assert!(!session_is_wayland(None, None));
    }

    #[test]
    fn the_route_names_wayland_before_it_looks_at_display() {
        let wayland = Environment {
            display: Some(":0"),
            session_type: Some("wayland"),
            wayland_display: Some("wayland-0"),
        };
        assert_eq!(route(&wayland), Err(Refusal::Wayland));
        let x11 = Environment {
            display: Some(":0"),
            session_type: Some("x11"),
            wayland_display: None,
        };
        assert_eq!(
            route(&x11),
            Ok(DisplayAddress {
                number: 0,
                screen: 0
            })
        );
        let remote = Environment {
            display: Some("host:0"),
            ..Environment::default()
        };
        assert_eq!(route(&remote), Err(Refusal::NoLocalServer));
        assert_eq!(
            route(&Environment::default()),
            Err(Refusal::NoLocalServer),
            "no display, no server"
        );
    }

    #[test]
    fn every_refusal_names_linux_and_its_own_cause() {
        let all = [
            Refusal::Wayland,
            Refusal::NoLocalServer,
            Refusal::NoXtest,
            Refusal::NoSpareKeys,
        ];
        for (index, one) in all.iter().enumerate() {
            assert!(one.system().starts_with("Linux "), "{}", one.system());
            for other in &all[index + 1..] {
                assert_ne!(one.system(), other.system());
            }
        }
    }

    #[test]
    fn every_character_is_its_unicode_keysym_and_two_are_keys() {
        assert_eq!(keysym_for('a'), Ok(0x0100_0061));
        assert_eq!(keysym_for('ż'), Ok(0x0100_017C));
        assert_eq!(keysym_for('日'), Ok(0x0100_65E5));
        assert_eq!(keysym_for('😀'), Ok(0x0101_F600));
        assert_eq!(keysym_for(' '), Ok(0x0100_0020));
        assert_eq!(keysym_for('\n'), Ok(KEYSYM_RETURN));
        assert_eq!(keysym_for('\t'), Ok(KEYSYM_TAB));
        assert_eq!(keysym_for('\u{10FFFF}'), Ok(0x0110_FFFF));
    }

    #[test]
    fn nul_cannot_be_carried_and_is_refused_by_name() {
        assert_eq!(keysym_for('\0'), Err(CannotCarry('\0')));
    }

    #[test]
    fn units_are_utf16_units() {
        let text: Vec<char> = "a😀日".chars().collect();
        assert_eq!(utf16_units(&text), 4);
        assert_eq!(utf16_units(&[]), 0);
    }

    fn keysyms_of(text: &str) -> Vec<u32> {
        text.chars()
            .map(|character| keysym_for(character).expect("a carriable character"))
            .collect()
    }

    /// The batches a send would cut `keysyms` into with a fixed limit.
    fn plan(keysyms: &[u32], spare: usize, max_chars: usize) -> Vec<Batch> {
        let mut batches = Vec::new();
        let mut first = 0;
        while first < keysyms.len() {
            let batch = next_batch(keysyms, first, spare, max_chars, &LayoutKeys::default());
            first = batch.end;
            batches.push(batch);
        }
        batches
    }

    #[test]
    fn a_short_text_is_one_batch_with_one_key_per_different_character() {
        let keysyms = keysyms_of("hello");
        let batches = plan(&keysyms, 18, 256);
        assert_eq!(batches.len(), 1);
        let batch = &batches[0];
        assert_eq!((batch.first, batch.end), (0, 5));
        assert_eq!(batch.keysyms.len(), 4, "the two l share a key");
        assert_eq!(batch.slot(keysyms[2]), batch.slot(keysyms[3]));
        assert_eq!(batch.slot(keysyms[0]), Some(0), "first appearance first");
    }

    #[test]
    fn a_batch_ends_before_the_character_that_has_no_key_left() {
        let text: String = ('a'..='z').collect();
        let keysyms = keysyms_of(&text);
        let batches = plan(&keysyms, 18, 256);
        assert_eq!(batches.len(), 2);
        assert_eq!((batches[0].first, batches[0].end), (0, 18));
        assert_eq!((batches[1].first, batches[1].end), (18, 26));
        assert!(batches.iter().all(|batch| batch.keysyms.len() <= 18));
    }

    #[test]
    fn a_batch_keeps_repeating_a_character_that_already_has_its_key() {
        // Eighteen different characters and then the first one again: the
        // repeat needs no new key, so it stays in the same batch.
        let text: String = ('a'..='r').chain(['a', 'b']).collect();
        let batches = plan(&keysyms_of(&text), 18, 256);
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].end, 20);
    }

    #[test]
    fn a_long_text_is_cut_by_length_too() {
        let keysyms = keysyms_of(&"a".repeat(600));
        let batches = plan(&keysyms, 18, 256);
        let lengths: Vec<usize> = batches
            .iter()
            .map(|batch| batch.end - batch.first)
            .collect();
        assert_eq!(lengths, vec![256, 256, 88]);
    }

    #[test]
    fn batches_cover_the_text_once_and_in_order() {
        let text = "Zażółć gęślą jaźń 日本語 😀 AbC ".repeat(9);
        let keysyms = keysyms_of(&text);
        for spare in [1, 2, 5, 18] {
            let batches = plan(&keysyms, spare, 40);
            let mut next = 0;
            for batch in &batches {
                assert_eq!(batch.first, next, "no gap and no overlap, spare {spare}");
                assert!(batch.end > batch.first, "a batch always moves on");
                assert!(batch.keysyms.len() <= spare);
                for keysym in &keysyms[batch.first..batch.end] {
                    assert!(batch.slot(*keysym).is_some(), "every character has a key");
                }
                next = batch.end;
            }
            assert_eq!(next, keysyms.len());
        }
    }

    #[test]
    fn a_whole_value_gets_one_key_for_each_different_keysym_in_order_of_appearance() {
        let keysyms = keysyms_of("Kowalski ");
        let slots = slots_for_value(&keysyms, 18, &LayoutKeys::default())
            .expect("nine different characters fit");
        assert_eq!(
            slots.len(),
            9,
            "K and k are two keysyms and the space is a third kind"
        );
        assert_eq!(slots[0], keysym_for('K').expect("K has a keysym"));
        assert_eq!(slots[1], keysym_for('o').expect("o has a keysym"));
        // Every character of the value finds its key among the slots.
        for keysym in &keysyms {
            assert!(slots.contains(keysym));
        }
        // The same keysym twice is one slot, so a value of one repeated character
        // needs one key however long it is.
        let repeated = keysyms_of(&"a".repeat(5000));
        assert_eq!(
            slots_for_value(&repeated, 1, &LayoutKeys::default()).map(|slots| slots.len()),
            Some(1)
        );
    }

    #[test]
    fn a_value_with_more_different_keysyms_than_spare_keys_gets_no_plan() {
        let text: String = ('a'..='z').collect();
        let keysyms = keysyms_of(&text);
        assert!(slots_for_value(&keysyms, 25, &LayoutKeys::default()).is_none());
        assert_eq!(
            slots_for_value(&keysyms, 26, &LayoutKeys::default()).map(|slots| slots.len()),
            Some(26)
        );
        assert!(slots_for_value(&keysyms, 0, &LayoutKeys::default()).is_none());
        assert_eq!(
            slots_for_value(&[], 0, &LayoutKeys::default()),
            Some(Vec::new()),
            "nothing to type needs no key"
        );
    }

    #[test]
    fn a_batch_of_a_planned_value_carries_every_slot_and_is_cut_by_length_only() {
        let keysyms = keysyms_of(&"abc".repeat(100));
        let slots =
            slots_for_value(&keysyms, 18, &LayoutKeys::default()).expect("three keysyms fit");
        let first = batch_of_value(&slots, 0, keysyms.len(), 64);
        assert_eq!((first.first, first.end), (0, 64));
        assert_eq!(
            first.keysyms, slots,
            "the keys stay assigned for the whole send"
        );
        let last = batch_of_value(&slots, 256, keysyms.len(), 64);
        assert_eq!(
            (last.first, last.end),
            (256, 300),
            "the end is the end of the value"
        );
        assert_eq!(last.slot(keysyms[299]), first.slot(keysyms[299]));
        let stuck = batch_of_value(&slots, 10, keysyms.len(), 0);
        assert_eq!(
            (stuck.first, stuck.end),
            (10, 11),
            "a batch always moves on"
        );
    }

    /// A layout with letters on keycodes 10 and 11, Return on 36 and Tab on 23.
    fn layout_with_return_and_tab() -> LayoutKeys {
        let mut rows = vec![0u32; 2 * 40];
        rows[2 * (36 - 8)] = KEYSYM_RETURN;
        rows[2 * (23 - 8)] = KEYSYM_TAB;
        rows[2 * (10 - 8)] = 0x61;
        LayoutKeys::from_keymap(&Keymap::new(8, 2, rows))
    }

    fn bare_key(keycode: u8) -> Option<LayoutKey> {
        Some(LayoutKey {
            keycode,
            shift: false,
        })
    }

    fn shifted_key(keycode: u8) -> Option<LayoutKey> {
        Some(LayoutKey {
            keycode,
            shift: true,
        })
    }

    /// A keyboard of four columns (group 1 plain and Shift, group 2 plain and Shift)
    /// with the keys given by keycode, the rest of the map empty.
    fn keyboard(keys: &[(u8, [u32; 4])]) -> Keymap {
        let last = keys.iter().map(|(code, _)| *code).max().unwrap_or(8);
        let mut symbols = vec![0u32; 4 * (usize::from(last) - 7)];
        for (code, row) in keys {
            let start = 4 * (usize::from(*code) - 8);
            symbols[start..start + 4].copy_from_slice(row);
        }
        Keymap::new(8, 4, symbols)
    }

    const SHIFT_L: u8 = 50;

    /// A few keys of a layout with a second group: digits, letters, a space, a dead
    /// key, a keypad key, a euro sign in the Unicode form, a pound sign, Return, Tab
    /// and Shift.
    fn us_ru_keyboard() -> Keymap {
        keyboard(&[
            (10, [0x31, 0x21, 0x31, 0x21]),
            (11, [0x61, 0x41, 0x06C6, 0x06E6]),
            (12, [0x20, 0x20, 0x20, 0x20]),
            (13, [0xFE51, 0xFE57, 0, 0]),
            (14, [0xFFB1, 0xFFB1, 0, 0]),
            (15, [0x0100_20AC, 0xA3, 0, 0]),
            (
                16,
                [KEYSYM_RETURN, KEYSYM_RETURN, KEYSYM_RETURN, KEYSYM_RETURN],
            ),
            (17, [KEYSYM_TAB, 0xFE20, KEYSYM_TAB, 0xFE20]),
            (SHIFT_L, [0xFFE1, 0xFFE1, 0xFFE1, 0xFFE1]),
        ])
    }

    fn key_of(layout: &LayoutKeys, character: char) -> Option<LayoutKey> {
        layout.key_for(keysym_for(character).expect("a carriable character"))
    }

    #[test]
    fn a_keysym_is_a_character_only_in_the_forms_a_layout_hands_out() {
        for (keysym, character) in [
            (0x20, 0x20),
            (0x61, 0x61),
            (0x7E, 0x7E),
            (0xA0, 0xA0),
            (0xE9, 0xE9),
            (0xFF, 0xFF),
            (0x0100_0100, 0x100),
            (0x0100_20AC, 0x20AC),
            (0x0101_F600, 0x1_F600),
            (0x0110_FFFF, 0x10_FFFF),
        ] {
            assert_eq!(
                character_keysym(keysym),
                Some(UNICODE_KEYSYM_BASE + character),
                "{keysym:#x}"
            );
        }
        for keysym in [
            0x0,
            0x1F,
            0x7F,
            0x9F,
            0x0100,
            0x06C6,
            0xFE51,
            0xFFB1,
            0xFFE1,
            KEYSYM_RETURN,
            0x0100_0061,
            0x0100_00FF,
            0x0100_D800,
            0x0100_DFFF,
            0x0111_0000,
        ] {
            assert_eq!(character_keysym(keysym), None, "{keysym:#x}");
        }
    }

    #[test]
    fn the_characters_of_the_layout_are_typed_through_its_keys_with_shift_where_it_takes_one() {
        let layout = LayoutKeys::typing(&us_ru_keyboard(), Some(SHIFT_L), 0);
        assert_eq!(key_of(&layout, '1'), bare_key(10));
        assert_eq!(key_of(&layout, '!'), shifted_key(10));
        assert_eq!(key_of(&layout, 'a'), bare_key(11));
        assert_eq!(key_of(&layout, 'A'), shifted_key(11));
        assert_eq!(key_of(&layout, '\u{20AC}'), bare_key(15), "Unicode form");
        assert_eq!(key_of(&layout, '\u{A3}'), shifted_key(15), "Latin-1 form");
        assert_eq!(layout.shift_key(), Some(SHIFT_L));
        // Not on any key: a spare key types them.
        assert_eq!(key_of(&layout, 'b'), None);
        assert_eq!(key_of(&layout, '\u{17C}'), None);
        // The key of a legacy Cyrillic keysym is not a character this table knows.
        assert_eq!(key_of(&layout, '\u{444}'), None);
        // A dead key and the keypad type something other than their name.
        assert_eq!(key_of(&layout, '\u{B4}'), None);
        assert_eq!(key_of(&layout, '\u{A8}'), None);
        // Return and Tab are the keys of those names whatever is on them.
        assert_eq!(layout.key_for(KEYSYM_RETURN), bare_key(16));
        assert_eq!(layout.key_for(KEYSYM_TAB), bare_key(17));
    }

    #[test]
    fn a_space_on_both_levels_takes_no_shift() {
        let layout = LayoutKeys::typing(&us_ru_keyboard(), Some(SHIFT_L), 0);
        assert_eq!(key_of(&layout, ' '), bare_key(12));
    }

    #[test]
    fn a_character_on_two_keys_is_pressed_through_the_one_that_needs_no_shift() {
        // The digit 2 is on a Shift level of a lower key and on the plain level of a
        // higher one: the plain level wins, the order of the keys does not.
        let keymap = keyboard(&[
            (20, [0x61, 0x32, 0, 0]),
            (30, [0x32, 0x42, 0, 0]),
            (SHIFT_L, [0xFFE1, 0xFFE1, 0, 0]),
        ]);
        let layout = LayoutKeys::typing(&keymap, Some(SHIFT_L), 0);
        assert_eq!(key_of(&layout, '2'), bare_key(30));
        assert_eq!(key_of(&layout, 'B'), shifted_key(30));
    }

    #[test]
    fn without_a_shift_key_only_the_characters_that_need_none_are_typed_through_the_layout() {
        let layout = LayoutKeys::typing(&us_ru_keyboard(), None, 0);
        assert_eq!(key_of(&layout, 'a'), bare_key(11));
        assert_eq!(key_of(&layout, 'A'), None);
        assert_eq!(key_of(&layout, '!'), None);
        assert_eq!(layout.shift_key(), None);
    }

    #[test]
    fn the_second_group_reads_the_second_pair_of_columns() {
        let second = 1u16 << 13;
        let layout = LayoutKeys::typing(&us_ru_keyboard(), Some(SHIFT_L), second);
        // The letter a is a Cyrillic one in this group: no key of the layout types it.
        assert_eq!(key_of(&layout, 'a'), None);
        assert_eq!(key_of(&layout, '1'), bare_key(10), "the digit is on both");
        assert_eq!(layout.key_for(KEYSYM_RETURN), bare_key(16));
    }

    #[test]
    fn a_state_that_would_change_the_characters_leaves_them_to_the_spare_keys() {
        let keymap = us_ru_keyboard();
        // Shift, Lock, Control, Mod1, Mod3, Mod4 and Mod5, and the third and fourth group.
        for state in [
            0x01u16,
            0x02,
            0x04,
            0x08,
            0x20,
            0x40,
            0x80,
            0x2 << 13,
            0x3 << 13,
            0x01 | (0x1 << 13),
        ] {
            let layout = LayoutKeys::typing(&keymap, Some(SHIFT_L), state);
            assert_eq!(key_of(&layout, 'a'), None, "state {state:#x}");
            assert_eq!(layout.shift_key(), None, "state {state:#x}");
            assert_eq!(layout.key_for(KEYSYM_RETURN), bare_key(16), "{state:#x}");
            assert_eq!(layout.key_for(KEYSYM_TAB), bare_key(17), "{state:#x}");
        }
        // Num Lock and a pressed mouse button change no character of the main block.
        for state in [0x10u16, 0x0100, 0x1000, 0x10 | 0x0300] {
            let layout = LayoutKeys::typing(&keymap, Some(SHIFT_L), state);
            assert_eq!(key_of(&layout, 'a'), bare_key(11), "state {state:#x}");
        }
    }

    #[test]
    fn the_state_is_plain_without_modifiers_and_the_group_is_read_from_bits_13_and_14() {
        assert!(layout_is_plain(0));
        assert!(layout_is_plain(0x10), "Num Lock");
        assert!(layout_is_plain(0x1F00), "buttons");
        assert!(layout_is_plain(0x6000), "a group is not a modifier");
        for bit in [0x01u16, 0x02, 0x04, 0x08, 0x20, 0x40, 0x80] {
            assert!(!layout_is_plain(bit), "{bit:#x}");
        }
        assert_eq!(usable_group(0), Some(0));
        assert_eq!(usable_group(0x2000), Some(1));
        assert_eq!(usable_group(0x4000), None);
        assert_eq!(usable_group(0x6000), None);
        assert_eq!(usable_group(0x1FFF), Some(0), "the low bits are no group");
    }

    #[test]
    fn a_value_the_layout_types_needs_no_spare_key_and_no_batch_is_cut_by_them() {
        let keys: Vec<(u8, [u32; 4])> = ('a'..='z')
            .zip(20u8..)
            .map(|(letter, code)| {
                (
                    code,
                    [letter as u32, letter.to_ascii_uppercase() as u32, 0, 0],
                )
            })
            .chain([(SHIFT_L, [0xFFE1; 4])])
            .collect();
        let layout = LayoutKeys::typing(&keyboard(&keys), Some(SHIFT_L), 0);
        let keysyms = keysyms_of("QuickBrownFoxJumps");
        assert_eq!(
            slots_for_value(&keysyms, 0, &layout),
            Some(Vec::new()),
            "nothing to assign, and no spare key is needed to say so"
        );
        let batch = next_batch(&keysyms, 0, 0, 256, &layout);
        assert_eq!(batch.end, keysyms.len());
        assert!(batch.keysyms.is_empty());
        // One character it does not type needs the one spare key there is.
        let mixed = keysyms_of("QuickBrownFox\u{17C}Jumps");
        let slots = slots_for_value(&mixed, 1, &layout).expect("one spare key holds it");
        assert_eq!(slots, vec![keysym_for('\u{17C}').expect("a keysym")]);
    }

    #[test]
    fn return_and_tab_are_typed_through_the_keys_of_the_layout() {
        let layout = layout_with_return_and_tab();
        assert_eq!(layout.key_for(KEYSYM_RETURN), bare_key(36));
        assert_eq!(layout.key_for(KEYSYM_TAB), bare_key(23));
        assert_eq!(layout.key_for(0x61), None, "a letter is not one of the two");
        assert_eq!(layout.key_for(keysym_for('a').expect("a")), None);
        // A layout without them leaves them to the spare keys.
        let bare = LayoutKeys::from_keymap(&Keymap::new(8, 2, vec![0; 80]));
        assert_eq!(bare.key_for(KEYSYM_RETURN), None);
        assert_eq!(bare.key_for(KEYSYM_TAB), None);
    }

    #[test]
    fn a_line_feed_and_a_tab_take_no_spare_key_when_the_layout_has_them() {
        let layout = layout_with_return_and_tab();
        let keysyms = keysyms_of("a\tb\nc");
        let slots = slots_for_value(&keysyms, 3, &layout).expect("a, b and c fit three keys");
        assert_eq!(
            slots.len(),
            3,
            "the tab and the line feed are not among them"
        );
        assert!(!slots.contains(&KEYSYM_RETURN) && !slots.contains(&KEYSYM_TAB));
        // Without the keys of the layout they need two more, and three do not hold five.
        let bare = LayoutKeys::default();
        assert!(slots_for_value(&keysyms, 3, &bare).is_none());
        assert_eq!(
            slots_for_value(&keysyms, 5, &bare).map(|slots| slots.len()),
            Some(5)
        );
        // The batch is cut by the keys it needs, and these two need none.
        let batch = next_batch(&keysyms, 0, 3, 256, &layout);
        assert_eq!(batch.end, 5);
        assert_eq!(batch.keysyms.len(), 3);
    }

    #[test]
    fn the_wait_before_the_layout_is_given_back_grows_with_the_keys_and_stops_at_a_ceiling() {
        // Chromium needed 50 ms or more for nine keys, and a layout given back at
        // once lost them all: the wait for a short batch is never below that.
        assert!(give_back_wait(0, false) >= Duration::from_millis(100));
        assert!(give_back_wait(9, false) >= Duration::from_millis(50));
        for input_method in [false, true] {
            let waits: Vec<Duration> = [0, 1, 9, 64, 256]
                .iter()
                .map(|keys| give_back_wait(*keys, input_method))
                .collect();
            assert!(waits.windows(2).all(|pair| pair[0] < pair[1]), "{waits:?}");
            assert_eq!(give_back_wait(1_000_000, input_method), GIVE_BACK_CEILING);
            assert_eq!(
                give_back_wait(usize::MAX, input_method),
                GIVE_BACK_CEILING,
                "no overflow"
            );
        }
        assert_eq!(
            give_back_wait(64, false),
            GIVE_BACK_BASE + GIVE_BACK_PER_KEY * 64
        );
        assert!(
            GIVE_BACK_CEILING < PING_WAIT,
            "holding the layout is shorter than waiting for a stopped application"
        );
    }

    #[test]
    fn an_input_method_in_the_session_lengthens_the_wait_for_each_key() {
        // xterm through IBus needed 200 ms for nine keys: the wait for nine keys
        // of such a session reaches it, and the wait of a session without one is
        // shorter.
        assert!(give_back_wait(9, true) >= Duration::from_millis(200));
        assert!(give_back_wait(9, true) > give_back_wait(9, false));
        assert_eq!(
            give_back_wait(0, true),
            give_back_wait(0, false),
            "the base is the same"
        );
    }

    #[test]
    fn xmodifiers_names_an_input_method_unless_it_says_none_or_nothing() {
        assert!(names_an_input_method(Some("@im=ibus")));
        assert!(names_an_input_method(Some("@im=fcitx")));
        assert!(names_an_input_method(Some("@im=ibus@other=1")));
        assert!(!names_an_input_method(Some("@im=none")));
        assert!(!names_an_input_method(Some("@im=")));
        assert!(!names_an_input_method(Some("")));
        assert!(!names_an_input_method(Some("something else")));
        assert!(!names_an_input_method(None));
    }

    #[test]
    fn a_limit_of_zero_still_moves_the_send_on() {
        // A batch always holds one character, so a bad limit cannot loop for ever.
        let batches = plan(&keysyms_of("ab"), 18, 0);
        assert_eq!(batches.len(), 2);
        assert!(plan(&[], 18, 256).is_empty());
    }

    #[test]
    fn a_fast_application_gets_the_longest_batches() {
        // 18 characters answered in 20 ms is about a millisecond a character: a
        // second holds a thousand of them, and the ceiling is what is left.
        let fast = next_limit(18, Duration::from_millis(20));
        assert_eq!(fast, MAX_CHARS_PER_ACK);
        assert_eq!(next_limit(1, Duration::ZERO), MAX_CHARS_PER_ACK);
    }

    #[test]
    fn a_slow_application_gets_the_characters_that_fit_a_second() {
        // 256 characters that took 5 s are 20 ms a character, which is 50 in a
        // second: the shape of the slow field in `nkb-target` (`OBS-204`).
        let slow = next_limit(256, Duration::from_secs(5));
        assert_eq!(slow, 51);
        // Never below the floor, however slow.
        assert_eq!(next_limit(64, Duration::from_secs(60)), MIN_BATCH_CHARS);
    }

    #[test]
    fn the_first_batch_is_between_the_floor_and_the_ceiling() {
        let first = [FIRST_BATCH_CHARS];
        assert!(
            first
                .iter()
                .all(|chars| (MIN_BATCH_CHARS..=MAX_CHARS_PER_ACK).contains(chars))
        );
        assert!(ANSWER_TARGET < PING_WAIT);
    }

    fn keymap(rows: &[&[u32]]) -> Keymap {
        let per = rows.first().map_or(1, |row| row.len());
        Keymap::new(
            8,
            per,
            rows.iter().flat_map(|row| row.iter().copied()).collect(),
        )
    }

    #[test]
    fn a_key_with_no_symbol_on_any_level_is_spare() {
        let map = keymap(&[&[0x61, 0x41], &[0, 0], &[0, 0x42], &[0, 0]]);
        assert_eq!(map.spare_keycodes(), vec![9, 11]);
    }

    #[test]
    fn a_key_with_a_symbol_on_a_higher_level_only_is_not_spare() {
        // Typing through it would replace its definition (`OBS-202` point 5).
        let map = keymap(&[&[0, 0, 0x1000_0041]]);
        assert!(map.spare_keycodes().is_empty());
    }

    #[test]
    fn the_last_keycode_is_not_lost_to_overflow() {
        let rows: Vec<Vec<u32>> = (0..248).map(|_| vec![0, 0]).collect();
        let refs: Vec<&[u32]> = rows.iter().map(Vec::as_slice).collect();
        let map = keymap(&refs);
        let spare = map.spare_keycodes();
        assert_eq!(spare.first(), Some(&8));
        assert_eq!(spare.last(), Some(&255));
        // A map that runs past 255 keys is cut, not wrapped round.
        let too_many: Vec<Vec<u32>> = (0..250).map(|_| vec![0, 0]).collect();
        let refs: Vec<&[u32]> = too_many.iter().map(Vec::as_slice).collect();
        assert_eq!(keymap(&refs).spare_keycodes().len(), 248);
    }

    #[test]
    fn a_key_is_found_by_its_first_level() {
        let map = keymap(&[&[0x61, 0x41], &[KEYSYM_ESCAPE, 0], &[0x62, KEYSYM_ESCAPE]]);
        assert_eq!(map.find_keycode(KEYSYM_ESCAPE), Some(9));
        assert_eq!(
            map.find_keycode(0x41),
            None,
            "a second level is no key of its own"
        );
        assert_eq!(map.find_keycode(0x7A), None);
    }

    #[test]
    fn the_focus_names_a_window_only_when_it_is_one() {
        assert_eq!(focus_window(0, 0x100), None, "None");
        assert_eq!(focus_window(1, 0x100), None, "PointerRoot");
        assert_eq!(focus_window(0x100, 0x100), None, "the root rests the focus");
        assert_eq!(focus_window(0x4000_0002, 0x100), Some(0x4000_0002));
    }

    #[test]
    fn a_held_key_is_found_in_the_bits_of_the_keymap_reply() {
        let mut keys = [0u8; 32];
        assert!(!key_is_down(&keys, 50));
        keys[6] |= 1 << 2; // keycode 50
        assert!(key_is_down(&keys, 50));
        assert!(!key_is_down(&keys, 51));
        assert!(
            !key_is_down(&keys[..4], 50),
            "a short reply is not a held key"
        );
    }

    #[test]
    fn a_held_modifier_is_named_by_its_slot_in_the_modifier_map() {
        // Two keys per modifier. Shift: 50 and 62, Lock: 66, Control: 37 and 105.
        let mut mapping = [0u8; 16];
        mapping[..2].copy_from_slice(&[50, 62]);
        mapping[2..4].copy_from_slice(&[66, 0]);
        mapping[4..6].copy_from_slice(&[37, 105]);
        mapping[6..8].copy_from_slice(&[64, 108]);
        mapping[8..10].copy_from_slice(&[77, 0]);
        mapping[12..14].copy_from_slice(&[133, 134]);
        mapping[14..16].copy_from_slice(&[92, 0]);
        let mut keys = [0u8; 32];
        assert_eq!(held_modifier(&keys, &mapping, 2), None);
        for (keycode, name) in [
            (62u8, "Shift"),
            (105, "Ctrl"),
            (64, "Alt"),
            (134, "Super"),
            (92, "AltGr"),
        ] {
            keys = [0u8; 32];
            keys[usize::from(keycode / 8)] |= 1 << (keycode % 8);
            assert_eq!(held_modifier(&keys, &mapping, 2), Some(name), "{keycode}");
        }
        // Caps Lock (66) and Num Lock (77) are not held modifiers.
        for keycode in [66u8, 77] {
            keys = [0u8; 32];
            keys[usize::from(keycode / 8)] |= 1 << (keycode % 8);
            assert_eq!(held_modifier(&keys, &mapping, 2), None, "{keycode}");
        }
        assert_eq!(
            held_modifier(&keys, &mapping, 0),
            None,
            "no map, nothing held"
        );
    }

    #[test]
    fn caps_lock_is_the_lock_bit_alone() {
        assert!(caps_lock_on(0x02));
        assert!(caps_lock_on(0x12));
        assert!(!caps_lock_on(0x01), "Shift");
        assert!(!caps_lock_on(0x10), "Num Lock");
        assert!(!caps_lock_on(0));
    }

    #[test]
    fn the_pauses_and_limits_keep_the_order_the_measurements_gave_them() {
        // Each pause is shorter than the one that stands in for the answer it
        // replaces: a settle after a remap, then the settle that stands in for a
        // ping, then the wait for the ping itself (`OBS-202` points 3 and 9).
        let waits = [REMAP_SETTLE, UNPACED_SETTLE, PING_WAIT];
        assert!(waits.windows(2).all(|pair| pair[0] < pair[1]), "{waits:?}");
        // The tap that makes the server announce its map needs at least the 100 ms
        // that were measured to be enough.
        assert!(WARMUP_SETTLE >= Duration::from_millis(100));
        // A batch is never cut by length before it is cut by keys.
        let lengths = [MAX_CHARS_PER_ACK];
        assert!(lengths.iter().all(|length| *length >= 18));
        // A held modifier is looked at many times in the wait for it.
        assert!(LOOK_EVERY * 10 <= crate::MODIFIER_RELEASE_WAIT);
    }

    #[test]
    fn the_answer_to_a_ping_is_heard_on_the_masks_every_toolkit_sends_it_with() {
        // The bits are the X11 protocol's own: StructureNotify is bit 17 and
        // SubstructureNotify bit 19. GTK and winit answer on the second, Qt on the
        // first, and a listener on one of them alone takes the other for silence.
        assert_eq!(STRUCTURE_NOTIFY, 1 << 17);
        assert_eq!(SUBSTRUCTURE_NOTIFY, 1 << 19);
        assert_eq!(PING_ANSWER_MASK & STRUCTURE_NOTIFY, STRUCTURE_NOTIFY);
        assert_eq!(PING_ANSWER_MASK & SUBSTRUCTURE_NOTIFY, SUBSTRUCTURE_NOTIFY);
        assert_eq!(
            PING_ANSWER_MASK,
            STRUCTURE_NOTIFY | SUBSTRUCTURE_NOTIFY,
            "the redirect bit belongs to the window manager and cannot be listened for"
        );
    }

    #[test]
    fn the_pings_of_one_send_differ_and_so_do_those_of_two_programs() {
        let tokens: Vec<u32> = (0..64).map(|counter| ping_token(4242, counter)).collect();
        for (index, token) in tokens.iter().enumerate() {
            assert!(!tokens[index + 1..].contains(token));
        }
        assert_ne!(ping_token(4242, 1), ping_token(4243, 1));
    }
}
