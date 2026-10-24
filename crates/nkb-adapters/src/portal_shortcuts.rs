//! Registering the palette's global shortcuts through the portal of the desktop.
//!
//! This is the `HotkeyRegistrar` adapter for a session in which the desktop, not
//! the program, holds the keys: a Wayland session has no X server that sees them
//! (`D145` points 39, 53 and 58). It is as thin as `GlobalShortcuts` next door,
//! because one port becomes one call into `nkb-sys` and the two vocabularies meet
//! in the one function that names keys. But the route is turned round, and four
//! things follow from that.
//!
//! - The program offers a chord for each shortcut, and the person and the desktop
//!   decide. A shortcut the desktop did not give is
//!   [`ShortcutRegistration::NotGiven`]. No outcome here is `Taken`, because the
//!   desktop never says that a chord is held by somebody.
//! - The chord given may not be the one offered. [`LiveShortcuts::given`] carries
//!   it, in the words of the desktop and in ours when they can be read.
//! - A shortcut given is not a shortcut that fires. A chord the portal accepted
//!   can belong to the system and never arrive (`OBS-217` point 5), so nothing
//!   here claims more than "given".
//! - Registering waits for the person: the first time the window of the system
//!   asks, and that is up to five minutes. It belongs on a thread that may wait.
//!
//! Nothing is kept here but what `nkb-sys` hands back: the shortcuts given, the
//! table from a fired id to its action, and the handle whose drop closes the
//! session. The ids are positions in the binding table, as they are on the grab.

use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use nkb_app::ports::{
    DesktopRefusal, GivenShortcut, HotkeyRegistrar, LiveShortcuts, ShortcutRegistration,
    ShortcutsUnavailable, Wait,
};
use nkb_core::hotkeys::{HotkeyAction, HotkeyChord, HotkeyKey};
use nkb_sys::hotkey::{Hotkey, HotkeyId};
use nkb_sys::portal::{self, Bound, PortalListener, Started, Unavailable, Wish};

use crate::i18n;
use crate::shortcuts::{hotkey_for, virtual_key, wait_for};

/// Asks the portal of the desktop for the shortcuts through `nkb_sys::portal`.
///
/// Stateless, like `GlobalShortcuts`: the state is the handle it returns.
#[derive(Debug, Default, Clone, Copy)]
pub struct PortalShortcuts;

/// What the portal answered to a start, in the shape this module reads.
struct Begun {
    bound: Vec<Bound>,
    fired: Receiver<HotkeyId>,
    /// What closes the session when it is let go. `None` only where no session
    /// was opened: an empty set, and the tests.
    guard: Option<PortalListener>,
}

impl From<Started> for Begun {
    fn from(started: Started) -> Self {
        Self {
            bound: started.bound,
            fired: started.fired,
            guard: Some(started.listener),
        }
    }
}

/// The shortcuts the desktop gave: what each came to, what each is called, the
/// table from a fired id back to its action, and the handle that keeps the
/// session open.
///
/// Field order is drop order: the session is closed first, and the receiver it
/// sends to goes after it.
struct PortalLive {
    _guard: Option<PortalListener>,
    fired: Receiver<HotkeyId>,
    /// Indexed by [`HotkeyId`]: the id handed to the portal for binding `n` is
    /// `n`, so a fired id is a position in this table and nothing else.
    actions: Vec<HotkeyAction>,
    outcomes: Vec<(HotkeyAction, ShortcutRegistration)>,
    given: Vec<(HotkeyAction, GivenShortcut)>,
}

impl PortalLive {
    /// The handle of a set that asked for nothing. No session stands behind it,
    /// and a wait on it ends with the channel, as on a set where nothing took.
    fn nothing_asked() -> Self {
        let (_, fired) = mpsc::channel::<HotkeyId>();
        Self {
            _guard: None,
            fired,
            actions: Vec::new(),
            outcomes: Vec::new(),
            given: Vec::new(),
        }
    }
}

impl LiveShortcuts for PortalLive {
    fn outcomes(&self) -> &[(HotkeyAction, ShortcutRegistration)] {
        &self.outcomes
    }

    fn given(&self) -> &[(HotkeyAction, GivenShortcut)] {
        &self.given
    }

    fn next(&self, wait: Duration) -> Wait {
        wait_for(&self.fired, &self.actions, wait)
    }
}

impl HotkeyRegistrar for PortalShortcuts {
    fn register(
        &self,
        bindings: &[(HotkeyAction, HotkeyChord)],
    ) -> Result<Box<dyn LiveShortcuts + Send>, ShortcutsUnavailable> {
        register_through(bindings, |wishes| portal::start(wishes).map(Begun::from))
    }
}

/// [`HotkeyRegistrar::register`] with the start given by the caller, so the
/// answers of a desktop can be dictated to it without one.
fn register_through(
    bindings: &[(HotkeyAction, HotkeyChord)],
    start: impl FnOnce(&[Wish]) -> Result<Begun, Unavailable>,
) -> Result<Box<dyn LiveShortcuts + Send>, ShortcutsUnavailable> {
    // Asking the desktop for nothing would open a session and a window of the
    // system for no shortcut, and the answer is known already.
    if bindings.is_empty() {
        return Ok(Box::new(PortalLive::nothing_asked()));
    }
    let wishes = wishes_for(bindings);
    let begun = start(&wishes).map_err(refusal)?;
    let answers = answers_for(&wishes, bindings, &begun.bound);
    if answers.given.is_empty() {
        // The session is closed as `begun` goes, and the palette is told in
        // one sentence rather than one for each shortcut left out.
        return Err(ShortcutsUnavailable::Desktop(DesktopRefusal::NothingGiven));
    }
    Ok(Box::new(PortalLive {
        _guard: begun.guard,
        fired: begun.fired,
        actions: bindings.iter().map(|(action, _)| *action).collect(),
        outcomes: answers.outcomes,
        given: answers.given,
    }))
}

/// What is asked of the desktop: a chord to offer for each binding, tagged with
/// its position, and the name of the action in the words the window of the
/// system shows beside it. The name is a key of the translations like any other
/// sentence of the palette.
fn wishes_for(bindings: &[(HotkeyAction, HotkeyChord)]) -> Vec<Wish> {
    bindings
        .iter()
        .enumerate()
        .map(|(index, (action, chord))| Wish {
            hotkey: hotkey_for(index, chord),
            name: action.id().to_owned(),
            description: i18n::action_name(*action).to_owned(),
        })
        .collect()
}

/// What the desktop answered, binding by binding.
struct Answers {
    outcomes: Vec<(HotkeyAction, ShortcutRegistration)>,
    given: Vec<(HotkeyAction, GivenShortcut)>,
}

/// Pairs each binding with what the desktop gave for it, or with the lack of it.
///
/// Matched by the id of the wish, so a shortcut the portal reports that nobody
/// asked for finds no binding and is left out, rather than being made into one.
fn answers_for(
    wishes: &[Wish],
    bindings: &[(HotkeyAction, HotkeyChord)],
    bound: &[Bound],
) -> Answers {
    let mut answers = Answers {
        outcomes: Vec::with_capacity(bindings.len()),
        given: Vec::new(),
    };
    for (wish, (action, _)) in wishes.iter().zip(bindings) {
        match bound.iter().find(|one| one.id == wish.hotkey.id) {
            Some(one) => {
                answers
                    .outcomes
                    .push((*action, ShortcutRegistration::Registered));
                answers.given.push((*action, given_by(one)));
            }
            None => answers
                .outcomes
                .push((*action, ShortcutRegistration::NotGiven)),
        }
    }
    answers
}

/// A shortcut the portal holds, as the port names it.
fn given_by(one: &Bound) -> GivenShortcut {
    GivenShortcut {
        words: one.words.clone(),
        chord: one.chord.as_ref().and_then(chord_of),
    }
}

/// The chord of the palette's vocabulary that a hotkey of `nkb-sys` stands for,
/// or `None` for a key that is none of its keys. The reverse of
/// [`hotkey_for`], and it meets the two vocabularies in the same place.
fn chord_of(hotkey: &Hotkey) -> Option<HotkeyChord> {
    let key = HotkeyKey::ALL
        .iter()
        .copied()
        .find(|key| virtual_key(*key) == hotkey.vk)?;
    Some(HotkeyChord {
        ctrl: hotkey.ctrl,
        alt: hotkey.alt,
        shift: hotkey.shift,
        win: hotkey.win,
        key,
    })
}

/// Why the desktop gave nothing, in the port's words. Every reason of the
/// route has an arm and none is folded away without saying so: a bus that is
/// missing, one that stops answering and a portal that is not there are one
/// thing to the person, who can do nothing about any of them.
fn refusal(why: Unavailable) -> ShortcutsUnavailable {
    match why {
        // Only a build for a system without the portal gets here, and it does not
        // choose this route. Named, never a shrug.
        Unavailable::NotHere => ShortcutsUnavailable::Unsupported {
            system: String::from("this system"),
        },
        Unavailable::NoBus
        | Unavailable::Unreachable
        | Unavailable::NoPortal
        | Unavailable::Odd => ShortcutsUnavailable::Desktop(DesktopRefusal::NoPortal),
        // The names are the ids of the actions, which a test holds to the rule of a name, so
        // this does not happen. It is named rather than let go by.
        Unavailable::BadWish => ShortcutsUnavailable::CouldNotStart,
        Unavailable::NeedsEntry => ShortcutsUnavailable::Desktop(DesktopRefusal::NeedsEntry),
        Unavailable::Cancelled => ShortcutsUnavailable::Desktop(DesktopRefusal::Closed),
        Unavailable::Ended(code) => ShortcutsUnavailable::Desktop(DesktopRefusal::Ended { code }),
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
    use nkb_core::hotkeys::DEFAULT_BINDINGS;

    fn chord(text: &str) -> HotkeyChord {
        HotkeyChord::parse(text).expect("a chord of the vocabulary")
    }

    /// Three bindings, enough to leave the middle one out.
    fn three() -> Vec<(HotkeyAction, HotkeyChord)> {
        vec![
            (HotkeyAction::NextValue, chord("Ctrl+Alt+N")),
            (HotkeyAction::PreviousValue, chord("Ctrl+Alt+P")),
            (HotkeyAction::RestartPack, chord("Ctrl+Alt+R")),
        ]
    }

    /// What a portal says of binding `index` when it holds `held` for it.
    fn bound(index: usize, words: &str, held: Option<HotkeyChord>) -> Bound {
        Bound {
            id: HotkeyId(u32::try_from(index).expect("a small index")),
            words: words.to_owned(),
            chord: held.map(|held| hotkey_for(index, &held)),
        }
    }

    /// A start that dictates the answer, with a channel the test can press on.
    fn answering(
        bound: Vec<Bound>,
    ) -> (
        std::sync::mpsc::Sender<HotkeyId>,
        impl FnOnce(&[Wish]) -> Result<Begun, Unavailable>,
    ) {
        let (press, fired) = mpsc::channel();
        (press, move |_: &[Wish]| {
            Ok(Begun {
                bound,
                fired,
                guard: None,
            })
        })
    }

    #[test]
    fn what_is_asked_is_each_chord_as_written_with_the_name_of_its_action() {
        let bindings = three();
        let wishes = wishes_for(&bindings);
        assert_eq!(wishes.len(), bindings.len());
        for (index, (wish, (action, asked))) in wishes.iter().zip(&bindings).enumerate() {
            assert_eq!(wish.hotkey, hotkey_for(index, asked));
            assert_eq!(wish.description, i18n::action_name(*action));
            assert!(!wish.description.is_empty());
        }
    }

    /// The desktop keeps the chord a person chose under the NAME of the shortcut, from one
    /// version of the program to the next (`OBS-223`). So the name is the id of the action,
    /// which the settings file already holds fixed, and not the place of the binding in a
    /// table that a later version may reorder or add to.
    #[test]
    fn a_shortcut_is_kept_under_its_action_not_under_its_place_in_the_table() {
        let mut bindings = three();
        let before: Vec<(HotkeyAction, String)> = wishes_for(&bindings)
            .into_iter()
            .zip(&bindings)
            .map(|(wish, (action, _))| (*action, wish.name))
            .collect();
        bindings.reverse();
        bindings.insert(1, (HotkeyAction::CopyReport, chord("Ctrl+Alt+B")));
        let after = wishes_for(&bindings);
        for (action, name) in &before {
            let still = after
                .iter()
                .zip(&bindings)
                .find(|(_, (asked, _))| asked == action)
                .map(|(wish, _)| wish.name.as_str());
            assert_eq!(still, Some(name.as_str()), "{action:?} changed its name");
            assert_eq!(name, action.id());
        }
    }

    /// Every action, and the nkb-sys rule of what the desktop can keep. A name that failed it
    /// would refuse the whole table, so the ids of the actions are held to it here.
    #[test]
    fn every_action_is_a_name_the_desktop_can_keep_and_none_is_asked_twice() {
        let wishes = wishes_for(&DEFAULT_BINDINGS);
        for wish in &wishes {
            assert!(nkb_sys::portal::usable_name(&wish.name), "{:?}", wish.name);
        }
        for action in HotkeyAction::ALL {
            assert!(nkb_sys::portal::usable_name(action.id()), "{action:?}");
            assert_eq!(
                HotkeyAction::ALL
                    .iter()
                    .filter(|other| other.id() == action.id())
                    .count(),
                1,
                "{action:?} shares its id"
            );
        }
    }

    #[test]
    fn every_shortcut_given_is_registered_with_the_words_of_the_desktop() {
        let bindings = three();
        let (_press, start) = answering(vec![
            bound(0, "Press <Control><Alt>n", Some(chord("Ctrl+Alt+N"))),
            bound(1, "Press <Control><Alt>p", Some(chord("Ctrl+Alt+P"))),
            bound(2, "Press <Control><Alt>r", Some(chord("Ctrl+Alt+R"))),
        ]);
        let live = register_through(&bindings, start).expect("the desktop gave them");
        assert_eq!(
            live.outcomes(),
            &[
                (HotkeyAction::NextValue, ShortcutRegistration::Registered),
                (
                    HotkeyAction::PreviousValue,
                    ShortcutRegistration::Registered
                ),
                (HotkeyAction::RestartPack, ShortcutRegistration::Registered),
            ]
        );
        assert!(live.holds_any());
        assert_eq!(live.given().len(), 3);
        assert_eq!(live.given()[0].0, HotkeyAction::NextValue);
        assert_eq!(live.given()[0].1.words, "Press <Control><Alt>n");
        assert_eq!(live.given()[0].1.chord, Some(chord("Ctrl+Alt+N")));
    }

    /// The person may leave one out. It is said as not given, in its place, and
    /// the others stay what they are: never `Taken`, which nobody has said.
    #[test]
    fn a_shortcut_left_out_is_not_given_in_its_place_and_the_rest_stand() {
        let bindings = three();
        let (_press, start) = answering(vec![
            bound(0, "Press <Control><Alt>n", Some(chord("Ctrl+Alt+N"))),
            bound(2, "Press <Control><Alt>r", Some(chord("Ctrl+Alt+R"))),
        ]);
        let live = register_through(&bindings, start).expect("two of three were given");
        assert_eq!(
            live.outcomes(),
            &[
                (HotkeyAction::NextValue, ShortcutRegistration::Registered),
                (HotkeyAction::PreviousValue, ShortcutRegistration::NotGiven),
                (HotkeyAction::RestartPack, ShortcutRegistration::Registered),
            ]
        );
        assert!(live.holds_any());
        let given: Vec<HotkeyAction> = live.given().iter().map(|(action, _)| *action).collect();
        assert_eq!(given, [HotkeyAction::NextValue, HotkeyAction::RestartPack]);
    }

    /// The desktop answered and gave none: no set, and one reason, rather than
    /// a set in which nothing can be pressed and a sentence for each.
    #[test]
    fn a_desktop_that_gave_none_gives_no_set() {
        let (_press, start) = answering(Vec::new());
        let refused = register_through(&three(), start).err();
        assert_eq!(
            refused,
            Some(ShortcutsUnavailable::Desktop(DesktopRefusal::NothingGiven))
        );
    }

    /// Only the ids of the wishes are heard. A shortcut the portal reports that
    /// nobody asked for finds no binding, and it is not made into one.
    #[test]
    fn a_shortcut_nobody_asked_for_is_left_out() {
        let bindings = three();
        let (_press, start) = answering(vec![
            bound(1, "Press <Control><Alt>p", Some(chord("Ctrl+Alt+P"))),
            bound(40, "Press <Control><Alt>q", Some(chord("Ctrl+Alt+Q"))),
        ]);
        let live = register_through(&bindings, start).expect("one of three was given");
        assert_eq!(live.outcomes().len(), bindings.len());
        assert_eq!(live.given().len(), 1);
        assert_eq!(live.given()[0].0, HotkeyAction::PreviousValue);
    }

    /// The person chose another chord than the one offered. What is given is
    /// what the desktop holds, not what was asked for.
    #[test]
    fn the_chord_given_is_the_one_the_desktop_holds_not_the_one_offered() {
        let (_press, start) = answering(vec![bound(
            0,
            "Press <Control><Shift>k",
            Some(chord("Ctrl+Shift+K")),
        )]);
        let live = register_through(&three(), start).expect("one was given");
        assert_eq!(live.given()[0].1.chord, Some(chord("Ctrl+Shift+K")));
        assert_ne!(live.given()[0].1.chord, Some(chord("Ctrl+Alt+N")));
    }

    /// Words with no chord of ours in them are all there is to show, and they
    /// are kept as they came.
    #[test]
    fn words_that_name_no_chord_of_ours_are_kept_as_they_came() {
        let (_press, start) = answering(vec![bound(0, "Presione <Super>x", None)]);
        let live = register_through(&three(), start).expect("one was given");
        assert_eq!(live.given()[0].1.words, "Presione <Super>x");
        assert_eq!(live.given()[0].1.chord, None);
    }

    /// The key of a hotkey of `nkb-sys` is read back for every key of the
    /// vocabulary and every combination of modifiers, as the chord that was
    /// offered. A hotkey with a key of no chord of ours is no chord.
    #[test]
    fn every_chord_offered_is_read_back_as_itself() {
        for key in HotkeyKey::ALL {
            for bits in 0u8..16 {
                let asked = HotkeyChord {
                    ctrl: bits & 1 != 0,
                    alt: bits & 2 != 0,
                    shift: bits & 4 != 0,
                    win: bits & 8 != 0,
                    key: *key,
                };
                assert_eq!(chord_of(&hotkey_for(0, &asked)), Some(asked), "{asked:?}");
            }
        }
        let odd = Hotkey {
            id: HotkeyId(0),
            ctrl: true,
            alt: true,
            shift: false,
            win: false,
            vk: 0x09,
        };
        assert_eq!(chord_of(&odd), None, "Tab is no key of a shortcut");
    }

    /// A press names its action by the position of the binding, even for a set
    /// in which the binding before it was left out.
    #[test]
    fn a_press_finds_its_action_by_position_when_an_earlier_one_was_left_out() {
        let bindings = three();
        let (press, start) = answering(vec![
            bound(0, "Press <Control><Alt>n", Some(chord("Ctrl+Alt+N"))),
            bound(2, "Press <Control><Alt>r", Some(chord("Ctrl+Alt+R"))),
        ]);
        let live = register_through(&bindings, start).expect("two of three were given");
        assert_eq!(live.next(Duration::ZERO), Wait::Nothing);
        press.send(HotkeyId(2)).expect("the set is listening");
        assert_eq!(
            live.next(Duration::ZERO),
            Wait::Pressed(HotkeyAction::RestartPack)
        );
        drop(press);
        assert_eq!(
            live.next(Duration::ZERO),
            Wait::Gone,
            "no press will arrive again"
        );
    }

    /// Asking for nothing asks the desktop nothing: no session, no window of
    /// the system, and a set that holds nothing and is not waited on.
    #[test]
    fn asking_for_nothing_asks_the_desktop_nothing() {
        let live = register_through(&[], |_| {
            panic!("the desktop was asked for no shortcut at all")
        })
        .expect("an empty set is still a set");
        assert!(live.outcomes().is_empty());
        assert!(live.given().is_empty());
        assert!(!live.holds_any());
        assert_eq!(live.next(Duration::ZERO), Wait::Gone);
    }

    /// Every reason of the route is said as one of the port's, and the ones
    /// the person can tell apart stay apart.
    #[test]
    fn every_reason_of_the_route_is_said_in_the_ports_words() {
        let desktop = |refusal| ShortcutsUnavailable::Desktop(refusal);
        assert_eq!(
            refusal(Unavailable::NeedsEntry),
            desktop(DesktopRefusal::NeedsEntry)
        );
        assert_eq!(
            refusal(Unavailable::Cancelled),
            desktop(DesktopRefusal::Closed)
        );
        assert_eq!(
            refusal(Unavailable::Ended(2)),
            desktop(DesktopRefusal::Ended { code: 2 })
        );
        for nothing_to_ask in [
            Unavailable::NoBus,
            Unavailable::Unreachable,
            Unavailable::NoPortal,
            Unavailable::Odd,
        ] {
            assert_eq!(
                refusal(nothing_to_ask),
                desktop(DesktopRefusal::NoPortal),
                "{nothing_to_ask:?}"
            );
        }
        assert!(matches!(
            refusal(Unavailable::NotHere),
            ShortcutsUnavailable::Unsupported { .. }
        ));
        assert_eq!(
            refusal(Unavailable::BadWish),
            ShortcutsUnavailable::CouldNotStart
        );
    }

    /// A start that fails is the registration that fails, and nothing is held.
    #[test]
    fn a_start_that_fails_is_a_registration_that_fails() {
        let refused = register_through(&three(), |_| Err(Unavailable::NeedsEntry)).err();
        assert_eq!(
            refused,
            Some(ShortcutsUnavailable::Desktop(DesktopRefusal::NeedsEntry))
        );
    }

    /// The table the palette starts from is asked as it is: the id of every
    /// wish is its position and the chord is the one written.
    #[test]
    fn the_default_table_is_asked_whole_in_its_own_order() {
        let wishes = wishes_for(&DEFAULT_BINDINGS);
        assert_eq!(wishes.len(), DEFAULT_BINDINGS.len());
        for (index, wish) in wishes.iter().enumerate() {
            assert_eq!(
                wish.hotkey.id,
                HotkeyId(u32::try_from(index).expect("a table fits"))
            );
        }
    }

    /// A system without the portal answers by name through the port, and a
    /// listener is never started for it.
    #[test]
    #[cfg(not(target_os = "linux"))]
    fn a_system_without_the_portal_names_itself_through_the_port() {
        let refused = PortalShortcuts.register(&three()).err();
        assert!(
            matches!(refused, Some(ShortcutsUnavailable::Unsupported { .. })),
            "{refused:?}"
        );
    }
}
